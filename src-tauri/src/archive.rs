//! 存档管理器
//!
//! 使用 SQLite 持久化弹幕、礼物、SC 等直播数据。
//! - 每次连接直播间创建一个 session
//! - 通过 mpsc channel 异步批量写入
//! - 提供分页查询、搜索、删除等功能

use std::sync::Arc;
use std::time::Duration;

use rusqlite::{named_params, params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

use crate::archive_migrations;
use crate::live_types::{
    LiveStats, ProcessedBlindGift, ProcessedDanmaku, ProcessedGift, ProcessedGiftCombo,
    ProcessedSuperChat,
};

// ==================== 存档事件（用于 channel 传输）====================

pub enum ArchiveEvent {
    Danmaku(ProcessedDanmaku),
    Gift(ProcessedGift),
    SuperChat(ProcessedSuperChat),
}

// ==================== 查询结果类型 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveSession {
    pub id: i64,
    pub room_id: u64,
    pub room_title: String,
    pub streamer_uid: u64,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub total_revenue: u64,
    pub gift_revenue: u64,
    pub sc_revenue: u64,
    pub guard_revenue: u64,
    pub danmaku_count: u64,
    pub gift_count: u64,
    pub sc_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedResult<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedDanmaku {
    pub id: i64,
    pub content: String,
    pub user_uid: u64,
    pub user_name: String,
    pub timestamp: i64,
    pub is_emoticon: bool,
    pub emoticon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedGift {
    pub id: i64,
    pub gift_name: String,
    pub gift_icon: Option<String>,
    pub num: u32,
    pub total_value: u64,
    pub revenue_value: u64,
    pub is_paid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combo: Option<ProcessedGiftCombo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blind_gift: Option<ProcessedBlindGift>,
    pub user_uid: u64,
    pub user_name: String,
    pub timestamp: i64,
    pub guard_level: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedSuperChat {
    pub id: i64,
    pub content: String,
    pub price: u64,
    pub user_uid: u64,
    pub user_name: String,
    pub background_color: String,
    pub duration: u32,
    pub start_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedUserName {
    pub uid: u64,
    pub name: String,
}

/// 首页与房间页共用的聚合统计。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchiveSummary {
    pub room_count: u64,
    pub session_count: u64,
    pub live_duration: u64,
    pub total_revenue: u64,
    pub gift_revenue: u64,
    pub sc_revenue: u64,
    pub guard_revenue: u64,
    pub danmaku_count: u64,
    pub gift_count: u64,
    pub sc_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveRoomSummary {
    pub room_id: u64,
    pub room_title: String,
    pub streamer_uid: u64,
    pub session_count: u64,
    pub live_duration: u64,
    pub total_revenue: u64,
    pub danmaku_count: u64,
    pub gift_count: u64,
    pub sc_count: u64,
    pub first_live_time: i64,
    pub last_live_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveOverview {
    pub summary: ArchiveSummary,
    pub rooms: Vec<ArchiveRoomSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveDailyStat {
    pub date: String,
    pub session_count: u64,
    pub live_duration: u64,
    pub total_revenue: u64,
    pub gift_revenue: u64,
    pub sc_revenue: u64,
    pub guard_revenue: u64,
    pub danmaku_count: u64,
    pub gift_count: u64,
    pub sc_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveStatistics {
    pub summary: ArchiveSummary,
    pub daily: Vec<ArchiveDailyStat>,
}

/// 跨事件类型的统一搜索结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveSearchItem {
    pub event_type: String,
    pub id: i64,
    pub session_id: i64,
    pub room_id: u64,
    pub room_title: String,
    pub content: String,
    pub detail: Option<String>,
    pub user_uid: u64,
    pub user_name: String,
    pub timestamp: i64,
    pub amount: Option<u64>,
    pub quantity: Option<u32>,
    pub image_url: Option<String>,
    pub is_emoticon: bool,
    pub is_paid: bool,
    pub guard_level: Option<u8>,
}

// ==================== ArchiveManager ====================

pub struct ArchiveManager {
    db: Mutex<Connection>,
    active_session_id: Mutex<Option<i64>>,
}

impl ArchiveManager {
    /// 创建并初始化 ArchiveManager
    pub fn new(db_path: std::path::PathBuf) -> Result<Self, String> {
        let mut conn =
            Connection::open(&db_path).map_err(|e| format!("打开存档数据库失败: {}", e))?;

        // 性能优化
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("设置 PRAGMA 失败: {}", e))?;

        archive_migrations::initialize(&mut conn)?;

        Ok(Self {
            db: Mutex::new(conn),
            active_session_id: Mutex::new(None),
        })
    }

    // ==================== 会话生命周期 ====================

    pub async fn start_session(
        &self,
        room_id: u64,
        room_title: &str,
        streamer_uid: u64,
    ) -> Result<i64, String> {
        let db = self.db.lock().await;
        let now = chrono::Utc::now().timestamp();

        db.execute(
            "INSERT INTO sessions (room_id, room_title, streamer_uid, start_time) VALUES (?1, ?2, ?3, ?4)",
            params![room_id as i64, room_title, streamer_uid as i64, now],
        )
        .map_err(|e| format!("创建存档会话失败: {}", e))?;

        let session_id = db.last_insert_rowid();
        drop(db);

        *self.active_session_id.lock().await = Some(session_id);
        log::info!(
            "Archive session started: id={}, room={}",
            session_id,
            room_id
        );
        Ok(session_id)
    }

    pub async fn end_session(&self, stats: &LiveStats) -> Result<(), String> {
        let session_id = self.active_session_id.lock().await.take();
        let Some(session_id) = session_id else {
            return Ok(());
        };

        let db = self.db.lock().await;
        let now = chrono::Utc::now().timestamp();

        // 统计实际条目数
        let danmaku_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM danmaku WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let gift_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM gifts WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let sc_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM super_chats WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        db.execute(
            "UPDATE sessions SET end_time = ?1, total_revenue = ?2, gift_revenue = ?3, sc_revenue = ?4, guard_revenue = ?5, danmaku_count = ?6, gift_count = ?7, sc_count = ?8 WHERE id = ?9",
            params![
                now,
                stats.total_revenue as i64,
                stats.gift_revenue as i64,
                stats.sc_revenue as i64,
                stats.guard_revenue as i64,
                danmaku_count,
                gift_count,
                sc_count,
                session_id,
            ],
        )
        .map_err(|e| format!("结束存档会话失败: {}", e))?;

        log::info!(
            "Archive session ended: id={}, danmaku={}, gifts={}, sc={}",
            session_id,
            danmaku_count,
            gift_count,
            sc_count,
        );
        Ok(())
    }

    pub async fn get_active_session_id(&self) -> Option<i64> {
        *self.active_session_id.lock().await
    }

    /// 恢复孤立的会话（end_time 为 NULL 的会话）
    /// 在应用启动时调用，处理上次异常退出未正常关闭的会话
    pub async fn recover_orphaned_sessions(&self) -> Result<u32, String> {
        let db = self.db.lock().await;
        let now = chrono::Utc::now().timestamp();

        // 查找所有 end_time 为 NULL 的会话
        let mut stmt = db
            .prepare("SELECT id FROM sessions WHERE end_time IS NULL")
            .map_err(|e| e.to_string())?;

        let orphan_ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        if orphan_ids.is_empty() {
            return Ok(0);
        }

        // 用已写入的数据补全每个孤立会话的统计信息
        for &session_id in &orphan_ids {
            let danmaku_count: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM danmaku WHERE session_id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let gift_count: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM gifts WHERE session_id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let sc_count: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM super_chats WHERE session_id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            // 从 gifts 表计算收入
            let gift_revenue: i64 = db
                .query_row(
                    "SELECT COALESCE(SUM(COALESCE(revenue_value, blind_gift_total_value, total_value)), 0) FROM gifts WHERE session_id = ?1 AND is_paid = 1",
                    params![session_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let guard_revenue: i64 = db
                .query_row(
                    "SELECT COALESCE(SUM(total_value), 0) FROM gifts WHERE session_id = ?1 AND guard_level IS NOT NULL",
                    params![session_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let sc_revenue: i64 = db
                .query_row(
                    "SELECT COALESCE(SUM(price), 0) FROM super_chats WHERE session_id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            // gift_revenue 已包含 guard_revenue，total = gift + sc
            let total_revenue = gift_revenue + sc_revenue;

            db.execute(
                "UPDATE sessions SET end_time = ?1, total_revenue = ?2, gift_revenue = ?3, sc_revenue = ?4, guard_revenue = ?5, danmaku_count = ?6, gift_count = ?7, sc_count = ?8 WHERE id = ?9",
                params![
                    now,
                    total_revenue,
                    gift_revenue,
                    sc_revenue,
                    guard_revenue,
                    danmaku_count,
                    gift_count,
                    sc_count,
                    session_id,
                ],
            )
            .map_err(|e| format!("恢复孤立会话失败: {}", e))?;

            log::info!(
                "Recovered orphaned archive session: id={}, danmaku={}, gifts={}, sc={}",
                session_id,
                danmaku_count,
                gift_count,
                sc_count,
            );
        }

        Ok(orphan_ids.len() as u32)
    }

    // ==================== 数据写入 ====================

    pub async fn save_danmaku_batch(
        &self,
        session_id: i64,
        items: &[ProcessedDanmaku],
    ) -> Result<(), String> {
        if items.is_empty() {
            return Ok(());
        }
        let db = self.db.lock().await;
        let tx = db
            .unchecked_transaction()
            .map_err(|e| format!("开启事务失败: {}", e))?;

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT INTO danmaku (session_id, original_id, content, user_uid, user_name, timestamp, is_emoticon, emoticon_url) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .map_err(|e| format!("准备语句失败: {}", e))?;

            for item in items {
                stmt.execute(params![
                    session_id,
                    &item.id,
                    &item.content,
                    item.user.uid as i64,
                    &item.user.name,
                    item.timestamp,
                    item.is_emoticon as i32,
                    &item.emoticon_url,
                ])
                .map_err(|e| format!("写入弹幕失败: {}", e))?;
            }
        }

        tx.commit().map_err(|e| format!("提交事务失败: {}", e))?;
        Ok(())
    }

    pub async fn save_gift(&self, session_id: i64, gift: &ProcessedGift) -> Result<(), String> {
        let db = self.db.lock().await;
        let blind_gift = gift.blind_gift.as_ref();
        let combo = gift.combo.as_ref();
        let updated = db
            .execute(
                "UPDATE gifts SET gift_id = ?1, gift_name = ?2, gift_icon = ?3, num = ?4, total_value = ?5, revenue_value = ?6, blind_gift_id = ?7, blind_gift_name = ?8, blind_gift_total_value = ?9, is_paid = ?10, user_uid = ?11, user_name = ?12, timestamp = ?13, guard_level = ?14, batch_combo_id = ?15, combo_total_coin = ?16, super_batch_gift_num = ?17, combo_resources_id = ?18, combo_stay_time = ?19, show_batch_combo_send = ?20 WHERE id = (SELECT id FROM gifts WHERE session_id = ?21 AND original_id = ?22 ORDER BY id ASC LIMIT 1)",
                params![
                    gift.gift_id as i64,
                    &gift.gift_name,
                    &gift.gift_icon,
                    gift.num as i64,
                    gift.total_value as i64,
                    gift.revenue_value as i64,
                    blind_gift.map(|blind_gift| blind_gift.gift_id as i64),
                    blind_gift.map(|blind_gift| blind_gift.gift_name.as_str()),
                    blind_gift.map(|blind_gift| blind_gift.total_value as i64),
                    gift.is_paid as i32,
                    gift.user.uid as i64,
                    &gift.user.name,
                    gift.timestamp,
                    gift.guard_level.map(i64::from),
                    combo.map(|combo| combo.batch_combo_id.as_str()),
                    combo
                        .and_then(|combo| combo.combo_total_coin)
                        .map(|value| value as i64),
                    combo
                        .and_then(|combo| combo.super_batch_gift_num)
                        .map(|value| value as i64),
                    combo
                        .and_then(|combo| combo.combo_resources_id)
                        .map(|value| value as i64),
                    combo
                        .and_then(|combo| combo.combo_stay_time)
                        .map(|value| value as i64),
                    combo
                        .and_then(|combo| combo.show_batch_combo_send)
                        .map(|value| if value { 1_i32 } else { 0_i32 }),
                    session_id,
                    &gift.id,
                ],
            )
            .map_err(|e| format!("更新礼物失败: {}", e))?;

        if updated > 0 {
            return Ok(());
        }

        db.execute(
            "INSERT INTO gifts (session_id, original_id, gift_id, gift_name, gift_icon, num, total_value, revenue_value, blind_gift_id, blind_gift_name, blind_gift_total_value, is_paid, user_uid, user_name, timestamp, guard_level, batch_combo_id, combo_total_coin, super_batch_gift_num, combo_resources_id, combo_stay_time, show_batch_combo_send) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            params![
                session_id,
                &gift.id,
                gift.gift_id as i64,
                &gift.gift_name,
                &gift.gift_icon,
                gift.num as i64,
                gift.total_value as i64,
                gift.revenue_value as i64,
                blind_gift.map(|blind_gift| blind_gift.gift_id as i64),
                blind_gift.map(|blind_gift| blind_gift.gift_name.as_str()),
                blind_gift.map(|blind_gift| blind_gift.total_value as i64),
                gift.is_paid as i32,
                gift.user.uid as i64,
                &gift.user.name,
                gift.timestamp,
                gift.guard_level.map(i64::from),
                combo.map(|combo| combo.batch_combo_id.as_str()),
                combo
                    .and_then(|combo| combo.combo_total_coin)
                    .map(|value| value as i64),
                combo
                    .and_then(|combo| combo.super_batch_gift_num)
                    .map(|value| value as i64),
                combo
                    .and_then(|combo| combo.combo_resources_id)
                    .map(|value| value as i64),
                combo
                    .and_then(|combo| combo.combo_stay_time)
                    .map(|value| value as i64),
                combo
                    .and_then(|combo| combo.show_batch_combo_send)
                    .map(|value| if value { 1_i32 } else { 0_i32 }),
            ],
        )
        .map_err(|e| format!("写入礼物失败: {}", e))?;
        Ok(())
    }

    pub async fn save_superchat(
        &self,
        session_id: i64,
        sc: &ProcessedSuperChat,
    ) -> Result<(), String> {
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO super_chats (session_id, original_id, content, price, user_uid, user_name, background_color, duration, start_time) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session_id,
                &sc.id,
                &sc.content,
                sc.price as i64,
                sc.user.uid as i64,
                &sc.user.name,
                &sc.background_color,
                sc.duration as i64,
                sc.start_time,
            ],
        )
        .map_err(|e| format!("写入 SC 失败: {}", e))?;
        Ok(())
    }

    // ==================== 查询方法 ====================

    pub async fn get_sessions(&self) -> Result<Vec<ArchiveSession>, String> {
        let db = self.db.lock().await;
        let mut stmt = db
            .prepare("SELECT id, room_id, room_title, streamer_uid, start_time, end_time, total_revenue, gift_revenue, sc_revenue, guard_revenue, danmaku_count, gift_count, sc_count FROM sessions ORDER BY start_time DESC")
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok(ArchiveSession {
                    id: row.get(0)?,
                    room_id: row.get::<_, i64>(1)? as u64,
                    room_title: row.get(2)?,
                    streamer_uid: row.get::<_, i64>(3)? as u64,
                    start_time: row.get(4)?,
                    end_time: row.get(5)?,
                    total_revenue: row.get::<_, i64>(6)? as u64,
                    gift_revenue: row.get::<_, i64>(7)? as u64,
                    sc_revenue: row.get::<_, i64>(8)? as u64,
                    guard_revenue: row.get::<_, i64>(9)? as u64,
                    danmaku_count: row.get::<_, i64>(10)? as u64,
                    gift_count: row.get::<_, i64>(11)? as u64,
                    sc_count: row.get::<_, i64>(12)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row.map_err(|e| e.to_string())?);
        }
        Ok(sessions)
    }

    pub async fn get_session_detail(&self, session_id: i64) -> Result<ArchiveSession, String> {
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, room_id, room_title, streamer_uid, start_time, end_time, total_revenue, gift_revenue, sc_revenue, guard_revenue, danmaku_count, gift_count, sc_count FROM sessions WHERE id = ?1",
            params![session_id],
            |row| {
                Ok(ArchiveSession {
                    id: row.get(0)?,
                    room_id: row.get::<_, i64>(1)? as u64,
                    room_title: row.get(2)?,
                    streamer_uid: row.get::<_, i64>(3)? as u64,
                    start_time: row.get(4)?,
                    end_time: row.get(5)?,
                    total_revenue: row.get::<_, i64>(6)? as u64,
                    gift_revenue: row.get::<_, i64>(7)? as u64,
                    sc_revenue: row.get::<_, i64>(8)? as u64,
                    guard_revenue: row.get::<_, i64>(9)? as u64,
                    danmaku_count: row.get::<_, i64>(10)? as u64,
                    gift_count: row.get::<_, i64>(11)? as u64,
                    sc_count: row.get::<_, i64>(12)? as u64,
                })
            },
        )
        .map_err(|e| format!("获取存档详情失败: {}", e))
    }

    /// 获取归档首页：总统计以及按直播间聚合的卡片。
    pub async fn get_overview(
        &self,
        from_time: Option<i64>,
        to_time: Option<i64>,
        query: &str,
    ) -> Result<ArchiveOverview, String> {
        validate_time_range(from_time, to_time)?;
        let db = self.db.lock().await;
        let summary = query_archive_summary(&db, None, from_time, to_time)?;
        let query = query.trim();
        let pattern = format!("%{query}%");

        let mut stmt = db
            .prepare(
                r#"
SELECT
    s.room_id,
    COALESCE(
        NULLIF((
            SELECT latest.room_title
            FROM sessions latest
            WHERE latest.room_id = s.room_id AND latest.room_title <> ''
            ORDER BY latest.start_time DESC
            LIMIT 1
        ), ''),
        '房间 ' || s.room_id
    ) AS room_title,
    COALESCE((
        SELECT latest.streamer_uid
        FROM sessions latest
        WHERE latest.room_id = s.room_id
        ORDER BY latest.start_time DESC
        LIMIT 1
    ), 0) AS streamer_uid,
    COUNT(*) AS session_count,
    COALESCE(SUM(MAX(COALESCE(s.end_time, CAST(strftime('%s', 'now') AS INTEGER)) - s.start_time, 0)), 0),
    COALESCE(SUM(s.total_revenue), 0),
    COALESCE(SUM(s.danmaku_count), 0),
    COALESCE(SUM(s.gift_count), 0),
    COALESCE(SUM(s.sc_count), 0),
    MIN(s.start_time),
    MAX(s.start_time)
FROM sessions s
WHERE (:from_time IS NULL OR s.start_time >= :from_time)
  AND (:to_time IS NULL OR s.start_time < :to_time)
  AND (
      :query = ''
      OR CAST(s.room_id AS TEXT) LIKE :pattern
      OR EXISTS (
          SELECT 1
          FROM sessions matched
          WHERE matched.room_id = s.room_id
            AND (
                matched.room_title LIKE :pattern COLLATE NOCASE
                OR CAST(matched.streamer_uid AS TEXT) LIKE :pattern
            )
      )
  )
GROUP BY s.room_id
ORDER BY MAX(s.start_time) DESC
"#,
            )
            .map_err(|e| format!("准备直播间聚合查询失败: {e}"))?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":from_time": from_time,
                    ":to_time": to_time,
                    ":query": query,
                    ":pattern": pattern,
                },
                |row| {
                    Ok(ArchiveRoomSummary {
                        room_id: row.get::<_, i64>(0)? as u64,
                        room_title: row.get(1)?,
                        streamer_uid: row.get::<_, i64>(2)? as u64,
                        session_count: row.get::<_, i64>(3)? as u64,
                        live_duration: row.get::<_, i64>(4)? as u64,
                        total_revenue: row.get::<_, i64>(5)? as u64,
                        danmaku_count: row.get::<_, i64>(6)? as u64,
                        gift_count: row.get::<_, i64>(7)? as u64,
                        sc_count: row.get::<_, i64>(8)? as u64,
                        first_live_time: row.get(9)?,
                        last_live_time: row.get(10)?,
                    })
                },
            )
            .map_err(|e| format!("查询直播间聚合失败: {e}"))?;
        let rooms = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("读取直播间聚合失败: {e}"))?;

        Ok(ArchiveOverview { summary, rooms })
    }

    /// 按直播间分页获取场次。
    pub async fn get_room_sessions(
        &self,
        room_id: u64,
        from_time: Option<i64>,
        to_time: Option<i64>,
        page: u32,
        page_size: u32,
    ) -> Result<PagedResult<ArchiveSession>, String> {
        validate_time_range(from_time, to_time)?;
        let (page, page_size, offset) = normalize_pagination(page, page_size);
        let db = self.db.lock().await;
        let room_id = room_id as i64;
        let total = db
            .query_row(
                r#"
SELECT COUNT(*)
FROM sessions
WHERE room_id = :room_id
  AND (:from_time IS NULL OR start_time >= :from_time)
  AND (:to_time IS NULL OR start_time < :to_time)
"#,
                named_params! {
                    ":room_id": room_id,
                    ":from_time": from_time,
                    ":to_time": to_time,
                },
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| format!("统计直播场次失败: {e}"))? as u64;

        let mut stmt = db
            .prepare(
                r#"
SELECT id, room_id, room_title, streamer_uid, start_time, end_time,
       total_revenue, gift_revenue, sc_revenue, guard_revenue,
       danmaku_count, gift_count, sc_count
FROM sessions
WHERE room_id = :room_id
  AND (:from_time IS NULL OR start_time >= :from_time)
  AND (:to_time IS NULL OR start_time < :to_time)
ORDER BY start_time DESC
LIMIT :limit OFFSET :offset
"#,
            )
            .map_err(|e| format!("准备直播场次查询失败: {e}"))?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":room_id": room_id,
                    ":from_time": from_time,
                    ":to_time": to_time,
                    ":limit": page_size as i64,
                    ":offset": offset,
                },
                map_session_row,
            )
            .map_err(|e| format!("查询直播场次失败: {e}"))?;
        let items = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("读取直播场次失败: {e}"))?;

        Ok(PagedResult {
            items,
            total,
            page,
            page_size,
        })
    }

    /// 获取全局或单个直播间的按日统计。
    pub async fn get_statistics(
        &self,
        room_id: Option<u64>,
        from_time: Option<i64>,
        to_time: Option<i64>,
    ) -> Result<ArchiveStatistics, String> {
        validate_time_range(from_time, to_time)?;
        let db = self.db.lock().await;
        let room_id = room_id.map(|value| value as i64);
        let summary = query_archive_summary(&db, room_id, from_time, to_time)?;
        let mut stmt = db
            .prepare(
                r#"
SELECT
    strftime('%Y-%m-%d', start_time, 'unixepoch', 'localtime') AS day,
    COUNT(*),
    COALESCE(SUM(MAX(COALESCE(end_time, CAST(strftime('%s', 'now') AS INTEGER)) - start_time, 0)), 0),
    COALESCE(SUM(total_revenue), 0),
    COALESCE(SUM(gift_revenue), 0),
    COALESCE(SUM(sc_revenue), 0),
    COALESCE(SUM(guard_revenue), 0),
    COALESCE(SUM(danmaku_count), 0),
    COALESCE(SUM(gift_count), 0),
    COALESCE(SUM(sc_count), 0)
FROM sessions
WHERE (:room_id IS NULL OR room_id = :room_id)
  AND (:from_time IS NULL OR start_time >= :from_time)
  AND (:to_time IS NULL OR start_time < :to_time)
GROUP BY day
ORDER BY day ASC
"#,
            )
            .map_err(|e| format!("准备归档趋势查询失败: {e}"))?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":room_id": room_id,
                    ":from_time": from_time,
                    ":to_time": to_time,
                },
                |row| {
                    Ok(ArchiveDailyStat {
                        date: row.get(0)?,
                        session_count: row.get::<_, i64>(1)? as u64,
                        live_duration: row.get::<_, i64>(2)? as u64,
                        total_revenue: row.get::<_, i64>(3)? as u64,
                        gift_revenue: row.get::<_, i64>(4)? as u64,
                        sc_revenue: row.get::<_, i64>(5)? as u64,
                        guard_revenue: row.get::<_, i64>(6)? as u64,
                        danmaku_count: row.get::<_, i64>(7)? as u64,
                        gift_count: row.get::<_, i64>(8)? as u64,
                        sc_count: row.get::<_, i64>(9)? as u64,
                    })
                },
            )
            .map_err(|e| format!("查询归档趋势失败: {e}"))?;
        let daily = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("读取归档趋势失败: {e}"))?;

        Ok(ArchiveStatistics { summary, daily })
    }

    /// 跨弹幕、礼物和醒目留言统一搜索，可限制到直播间或单场直播。
    #[allow(clippy::too_many_arguments)]
    pub async fn search(
        &self,
        room_id: Option<u64>,
        session_id: Option<i64>,
        query: &str,
        event_type: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        page: u32,
        page_size: u32,
    ) -> Result<PagedResult<ArchiveSearchItem>, String> {
        validate_time_range(from_time, to_time)?;
        if !matches!(event_type, "all" | "danmaku" | "gift" | "superchat") {
            return Err("不支持的归档事件类型".to_string());
        }
        let (page, page_size, offset) = normalize_pagination(page, page_size);
        let query = query.trim();
        let pattern = format!("%{query}%");
        let room_id = room_id.map(|value| value as i64);
        let db = self.db.lock().await;

        let total = db
            .query_row(
                r#"
SELECT COUNT(*)
FROM archive_events
WHERE (:room_id IS NULL OR room_id = :room_id)
  AND (:session_id IS NULL OR session_id = :session_id)
  AND (:from_time IS NULL OR timestamp >= :from_time)
  AND (:to_time IS NULL OR timestamp < :to_time)
  AND (:event_type = 'all' OR event_type = :event_type)
  AND (
      :query = ''
      OR content LIKE :pattern COLLATE NOCASE
      OR detail LIKE :pattern COLLATE NOCASE
      OR user_name LIKE :pattern COLLATE NOCASE
      OR CAST(user_uid AS TEXT) LIKE :pattern
  )
"#,
                named_params! {
                    ":room_id": room_id,
                    ":session_id": session_id,
                    ":from_time": from_time,
                    ":to_time": to_time,
                    ":event_type": event_type,
                    ":query": query,
                    ":pattern": pattern,
                },
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| format!("统计归档搜索结果失败: {e}"))? as u64;

        let mut stmt = db
            .prepare(
                r#"
SELECT event_type, id, session_id, room_id, room_title, content, detail,
       user_uid, user_name, timestamp, amount, quantity, image_url,
       is_emoticon, is_paid, guard_level
FROM archive_events
WHERE (:room_id IS NULL OR room_id = :room_id)
  AND (:session_id IS NULL OR session_id = :session_id)
  AND (:from_time IS NULL OR timestamp >= :from_time)
  AND (:to_time IS NULL OR timestamp < :to_time)
  AND (:event_type = 'all' OR event_type = :event_type)
  AND (
      :query = ''
      OR content LIKE :pattern COLLATE NOCASE
      OR detail LIKE :pattern COLLATE NOCASE
      OR user_name LIKE :pattern COLLATE NOCASE
      OR CAST(user_uid AS TEXT) LIKE :pattern
  )
ORDER BY timestamp DESC, id DESC
LIMIT :limit OFFSET :offset
"#,
            )
            .map_err(|e| format!("准备归档搜索失败: {e}"))?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":room_id": room_id,
                    ":session_id": session_id,
                    ":from_time": from_time,
                    ":to_time": to_time,
                    ":event_type": event_type,
                    ":query": query,
                    ":pattern": pattern,
                    ":limit": page_size as i64,
                    ":offset": offset,
                },
                map_search_row,
            )
            .map_err(|e| format!("查询归档失败: {e}"))?;
        let items = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("读取归档搜索结果失败: {e}"))?;

        Ok(PagedResult {
            items,
            total,
            page,
            page_size,
        })
    }

    pub async fn search_danmaku(
        &self,
        session_id: i64,
        query: &str,
        page: u32,
        page_size: u32,
    ) -> Result<PagedResult<ArchivedDanmaku>, String> {
        let db = self.db.lock().await;
        let (page, page_size, offset) = normalize_pagination(page, page_size);
        let query = query.trim();
        let pattern = format!("%{query}%");
        let total = db
            .query_row(
                r#"
SELECT COUNT(*) FROM danmaku
WHERE session_id = :session_id
  AND (
      :query = ''
      OR content LIKE :pattern COLLATE NOCASE
      OR user_name LIKE :pattern COLLATE NOCASE
      OR CAST(user_uid AS TEXT) LIKE :pattern
  )
"#,
                named_params! {
                    ":session_id": session_id,
                    ":query": query,
                    ":pattern": pattern,
                },
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())? as u64;

        let mut stmt = db
            .prepare(
                r#"
SELECT id, content, user_uid, user_name, timestamp, is_emoticon, emoticon_url
FROM danmaku
WHERE session_id = :session_id
  AND (
      :query = ''
      OR content LIKE :pattern COLLATE NOCASE
      OR user_name LIKE :pattern COLLATE NOCASE
      OR CAST(user_uid AS TEXT) LIKE :pattern
  )
ORDER BY timestamp ASC
LIMIT :limit OFFSET :offset
"#,
            )
            .map_err(|e| e.to_string())?;
        let items = stmt
            .query_map(
                named_params! {
                    ":session_id": session_id,
                    ":query": query,
                    ":pattern": pattern,
                    ":limit": page_size as i64,
                    ":offset": offset,
                },
                map_danmaku_row,
            )
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(PagedResult {
            items,
            total,
            page,
            page_size,
        })
    }

    pub async fn lookup_user_names(&self, uids: Vec<u64>) -> Result<Vec<ArchivedUserName>, String> {
        let mut uids = uids.into_iter().filter(|uid| *uid > 0).collect::<Vec<_>>();
        uids.sort_unstable();
        uids.dedup();

        if uids.is_empty() {
            return Ok(Vec::new());
        }

        let db = self.db.lock().await;
        let mut result = Vec::new();

        for uid in uids {
            let mut latest: Option<(String, i64)> = None;
            for sql in [
                "SELECT user_name, timestamp FROM danmaku WHERE user_uid = ?1 AND user_name <> '' ORDER BY timestamp DESC LIMIT 1",
                "SELECT user_name, timestamp FROM gifts WHERE user_uid = ?1 AND user_name <> '' ORDER BY timestamp DESC LIMIT 1",
                "SELECT user_name, start_time FROM super_chats WHERE user_uid = ?1 AND user_name <> '' ORDER BY start_time DESC LIMIT 1",
            ] {
                let row = db
                    .query_row(sql, params![uid as i64], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })
                    .optional()
                    .map_err(|e| e.to_string())?;

                if let Some((name, timestamp)) = row {
                    if latest
                        .as_ref()
                        .is_none_or(|(_, latest_timestamp)| timestamp > *latest_timestamp)
                    {
                        latest = Some((name, timestamp));
                    }
                }
            }

            if let Some((name, _)) = latest {
                result.push(ArchivedUserName { uid, name });
            }
        }

        Ok(result)
    }

    pub async fn search_gifts(
        &self,
        session_id: i64,
        query: &str,
        min_price: Option<u64>,
        max_price: Option<u64>,
        page: u32,
        page_size: u32,
    ) -> Result<PagedResult<ArchivedGift>, String> {
        let db = self.db.lock().await;
        let (page, page_size, offset) = normalize_pagination(page, page_size);
        let query = query.trim();
        let pattern = format!("%{query}%");
        let min_price = min_price.map(|value| value as i64);
        let max_price = max_price.map(|value| value as i64);
        let total = db
            .query_row(
                r#"
SELECT COUNT(*) FROM gifts
WHERE session_id = :session_id
  AND (:min_price IS NULL OR total_value >= :min_price)
  AND (:max_price IS NULL OR total_value <= :max_price)
  AND (
      :query = ''
      OR gift_name LIKE :pattern COLLATE NOCASE
      OR blind_gift_name LIKE :pattern COLLATE NOCASE
      OR user_name LIKE :pattern COLLATE NOCASE
      OR CAST(user_uid AS TEXT) LIKE :pattern
  )
"#,
                named_params! {
                    ":session_id": session_id,
                    ":min_price": min_price,
                    ":max_price": max_price,
                    ":query": query,
                    ":pattern": pattern,
                },
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())? as u64;

        let mut stmt = db
            .prepare(
                r#"
SELECT id, gift_name, gift_icon, num, total_value, is_paid, user_uid,
       user_name, timestamp, guard_level, blind_gift_id, blind_gift_name,
       blind_gift_total_value, revenue_value, batch_combo_id, combo_total_coin,
       super_batch_gift_num, combo_resources_id, combo_stay_time,
       show_batch_combo_send
FROM gifts
WHERE session_id = :session_id
  AND (:min_price IS NULL OR total_value >= :min_price)
  AND (:max_price IS NULL OR total_value <= :max_price)
  AND (
      :query = ''
      OR gift_name LIKE :pattern COLLATE NOCASE
      OR blind_gift_name LIKE :pattern COLLATE NOCASE
      OR user_name LIKE :pattern COLLATE NOCASE
      OR CAST(user_uid AS TEXT) LIKE :pattern
  )
ORDER BY timestamp ASC
LIMIT :limit OFFSET :offset
"#,
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":session_id": session_id,
                    ":min_price": min_price,
                    ":max_price": max_price,
                    ":query": query,
                    ":pattern": pattern,
                    ":limit": page_size as i64,
                    ":offset": offset,
                },
                |row| {
                    let blind_gift_id = row.get::<_, Option<i64>>(10)?;
                    let blind_gift_name = row.get::<_, Option<String>>(11)?;
                    let blind_gift_total_value = row.get::<_, Option<i64>>(12)?;
                    let revenue_value = row
                        .get::<_, Option<i64>>(13)?
                        .or(blind_gift_total_value)
                        .unwrap_or(row.get::<_, i64>(4)?);
                    let blind_gift = match (blind_gift_id, blind_gift_name, blind_gift_total_value)
                    {
                        (Some(gift_id), Some(gift_name), Some(total_value)) => {
                            Some(ProcessedBlindGift {
                                gift_id: gift_id as u64,
                                gift_name,
                                total_value: total_value as u64,
                            })
                        }
                        _ => None,
                    };
                    let combo = if let Some(batch_combo_id) = row
                        .get::<_, Option<String>>(14)?
                        .filter(|batch_combo_id| !batch_combo_id.is_empty())
                    {
                        Some(ProcessedGiftCombo {
                            batch_combo_id,
                            combo_total_coin: row
                                .get::<_, Option<i64>>(15)?
                                .map(|value| value as u64),
                            super_batch_gift_num: row
                                .get::<_, Option<i64>>(16)?
                                .map(|value| value as u64),
                            combo_resources_id: row
                                .get::<_, Option<i64>>(17)?
                                .map(|value| value as u64),
                            combo_stay_time: row
                                .get::<_, Option<i64>>(18)?
                                .map(|value| value as u64),
                            show_batch_combo_send: row
                                .get::<_, Option<i32>>(19)?
                                .map(|value| value != 0),
                        })
                    } else {
                        None
                    };

                    Ok(ArchivedGift {
                        id: row.get(0)?,
                        gift_name: row.get(1)?,
                        gift_icon: row.get(2)?,
                        num: row.get::<_, i64>(3)? as u32,
                        total_value: row.get::<_, i64>(4)? as u64,
                        revenue_value: revenue_value as u64,
                        is_paid: row.get::<_, i32>(5)? != 0,
                        combo,
                        blind_gift,
                        user_uid: row.get::<_, i64>(6)? as u64,
                        user_name: row.get(7)?,
                        timestamp: row.get(8)?,
                        guard_level: row.get::<_, Option<i64>>(9)?.map(|g| g as u8),
                    })
                },
            )
            .map_err(|e| e.to_string())?;

        let items = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(PagedResult {
            items,
            total,
            page,
            page_size,
        })
    }

    pub async fn search_superchat(
        &self,
        session_id: i64,
        query: &str,
        min_price: Option<u64>,
        max_price: Option<u64>,
        page: u32,
        page_size: u32,
    ) -> Result<PagedResult<ArchivedSuperChat>, String> {
        let db = self.db.lock().await;
        let (page, page_size, offset) = normalize_pagination(page, page_size);
        let query = query.trim();
        let pattern = format!("%{query}%");
        let min_price = min_price.map(|value| value as i64);
        let max_price = max_price.map(|value| value as i64);
        let total = db
            .query_row(
                r#"
SELECT COUNT(*) FROM super_chats
WHERE session_id = :session_id
  AND (:min_price IS NULL OR price >= :min_price)
  AND (:max_price IS NULL OR price <= :max_price)
  AND (
      :query = ''
      OR content LIKE :pattern COLLATE NOCASE
      OR user_name LIKE :pattern COLLATE NOCASE
      OR CAST(user_uid AS TEXT) LIKE :pattern
  )
"#,
                named_params! {
                    ":session_id": session_id,
                    ":min_price": min_price,
                    ":max_price": max_price,
                    ":query": query,
                    ":pattern": pattern,
                },
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())? as u64;

        let mut stmt = db
            .prepare(
                r#"
SELECT id, content, price, user_uid, user_name, background_color, duration, start_time
FROM super_chats
WHERE session_id = :session_id
  AND (:min_price IS NULL OR price >= :min_price)
  AND (:max_price IS NULL OR price <= :max_price)
  AND (
      :query = ''
      OR content LIKE :pattern COLLATE NOCASE
      OR user_name LIKE :pattern COLLATE NOCASE
      OR CAST(user_uid AS TEXT) LIKE :pattern
  )
ORDER BY start_time ASC
LIMIT :limit OFFSET :offset
"#,
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":session_id": session_id,
                    ":min_price": min_price,
                    ":max_price": max_price,
                    ":query": query,
                    ":pattern": pattern,
                    ":limit": page_size as i64,
                    ":offset": offset,
                },
                |row| {
                    Ok(ArchivedSuperChat {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        price: row.get::<_, i64>(2)? as u64,
                        user_uid: row.get::<_, i64>(3)? as u64,
                        user_name: row.get(4)?,
                        background_color: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        duration: row.get::<_, i64>(6)? as u32,
                        start_time: row.get(7)?,
                    })
                },
            )
            .map_err(|e| e.to_string())?;

        let items = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(PagedResult {
            items,
            total,
            page,
            page_size,
        })
    }

    /// 删除没有任何事件的历史场次，当前正在录制的场次始终保留。
    pub async fn prune_empty_sessions(&self) -> Result<u64, String> {
        let active_session_id = self.get_active_session_id().await;
        let db = self.db.lock().await;
        let deleted = db
            .execute(
                r#"
DELETE FROM sessions
WHERE (:active_session_id IS NULL OR id <> :active_session_id)
  AND NOT EXISTS (SELECT 1 FROM danmaku WHERE danmaku.session_id = sessions.id)
  AND NOT EXISTS (SELECT 1 FROM gifts WHERE gifts.session_id = sessions.id)
  AND NOT EXISTS (SELECT 1 FROM super_chats WHERE super_chats.session_id = sessions.id)
"#,
                named_params! { ":active_session_id": active_session_id },
            )
            .map_err(|e| format!("清理空归档场次失败: {e}"))?;
        if deleted > 0 {
            log::info!("Pruned {} empty archive session(s)", deleted);
        }
        Ok(deleted as u64)
    }

    pub async fn delete_session(&self, session_id: i64) -> Result<(), String> {
        if self.get_active_session_id().await == Some(session_id) {
            return Err("直播进行中，不能删除当前场次".to_string());
        }
        let db = self.db.lock().await;
        let tx = db
            .unchecked_transaction()
            .map_err(|e| format!("开启删除事务失败: {e}"))?;
        tx.execute(
            "DELETE FROM danmaku WHERE session_id = ?1",
            rusqlite::params![session_id],
        )
        .map_err(|e| format!("删除存档失败: {}", e))?;
        tx.execute(
            "DELETE FROM gifts WHERE session_id = ?1",
            rusqlite::params![session_id],
        )
        .map_err(|e| format!("删除存档失败: {}", e))?;
        tx.execute(
            "DELETE FROM super_chats WHERE session_id = ?1",
            rusqlite::params![session_id],
        )
        .map_err(|e| format!("删除存档失败: {}", e))?;
        tx.execute(
            "DELETE FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
        )
        .map_err(|e| format!("删除存档失败: {}", e))?;
        tx.commit()
            .map_err(|e| format!("提交删除存档事务失败: {e}"))?;
        log::info!("Archive session deleted: id={}", session_id);
        Ok(())
    }
}

// ==================== Archive Writer Task ====================

/// 启动存档写入任务，从 channel 接收事件并批量写入 SQLite
pub fn spawn_archive_writer(
    archive: Arc<ArchiveManager>,
    mut rx: mpsc::UnboundedReceiver<ArchiveEvent>,
    session_id: i64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut danmaku_buf: Vec<ProcessedDanmaku> = Vec::new();
        let mut gift_buf: Vec<ProcessedGift> = Vec::new();
        let mut sc_buf: Vec<ProcessedSuperChat> = Vec::new();

        let flush_interval = Duration::from_millis(500);

        loop {
            // 等待事件或超时（用于定期 flush）
            let event = tokio::time::timeout(flush_interval, rx.recv()).await;
            let is_timeout = event.is_err();

            match event {
                Ok(Some(ArchiveEvent::Danmaku(d))) => {
                    danmaku_buf.push(d);
                }
                Ok(Some(ArchiveEvent::Gift(g))) => {
                    gift_buf.push(g);
                }
                Ok(Some(ArchiveEvent::SuperChat(sc))) => {
                    sc_buf.push(sc);
                }
                Ok(None) => {
                    // Channel closed, flush and exit
                    flush_buffers(
                        &archive,
                        session_id,
                        &mut danmaku_buf,
                        &mut gift_buf,
                        &mut sc_buf,
                    )
                    .await;
                    break;
                }
                Err(_) => {
                    // Timeout, will flush below
                }
            }

            // Flush when buffer is large enough or on timeout
            let reached_batch_size =
                danmaku_buf.len() >= 100 || gift_buf.len() >= 50 || sc_buf.len() >= 20;
            let has_buffered_events =
                !danmaku_buf.is_empty() || !gift_buf.is_empty() || !sc_buf.is_empty();
            if reached_batch_size || (is_timeout && has_buffered_events) {
                flush_buffers(
                    &archive,
                    session_id,
                    &mut danmaku_buf,
                    &mut gift_buf,
                    &mut sc_buf,
                )
                .await;
            }
        }

        log::info!("Archive writer task exited for session {}", session_id);
    })
}

async fn flush_buffers(
    archive: &ArchiveManager,
    session_id: i64,
    danmaku_buf: &mut Vec<ProcessedDanmaku>,
    gift_buf: &mut Vec<ProcessedGift>,
    sc_buf: &mut Vec<ProcessedSuperChat>,
) {
    if !danmaku_buf.is_empty() {
        let items = std::mem::take(danmaku_buf);
        if let Err(e) = archive.save_danmaku_batch(session_id, &items).await {
            log::error!("Archive flush danmaku error: {}", e);
        }
    }
    for gift in gift_buf.drain(..) {
        if let Err(e) = archive.save_gift(session_id, &gift).await {
            log::error!("Archive flush gift error: {}", e);
        }
    }
    for sc in sc_buf.drain(..) {
        if let Err(e) = archive.save_superchat(session_id, &sc).await {
            log::error!("Archive flush SC error: {}", e);
        }
    }
}

// ==================== Helper ====================

fn normalize_pagination(page: u32, page_size: u32) -> (u32, u32, i64) {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 100);
    let offset = (u64::from(page - 1) * u64::from(page_size)).min(i64::MAX as u64) as i64;
    (page, page_size, offset)
}

fn validate_time_range(from_time: Option<i64>, to_time: Option<i64>) -> Result<(), String> {
    if matches!((from_time, to_time), (Some(from), Some(to)) if from >= to) {
        return Err("开始时间必须早于结束时间".to_string());
    }
    Ok(())
}

fn query_archive_summary(
    db: &Connection,
    room_id: Option<i64>,
    from_time: Option<i64>,
    to_time: Option<i64>,
) -> Result<ArchiveSummary, String> {
    db.query_row(
        r#"
SELECT
    COUNT(DISTINCT room_id),
    COUNT(*),
    COALESCE(SUM(MAX(COALESCE(end_time, CAST(strftime('%s', 'now') AS INTEGER)) - start_time, 0)), 0),
    COALESCE(SUM(total_revenue), 0),
    COALESCE(SUM(gift_revenue), 0),
    COALESCE(SUM(sc_revenue), 0),
    COALESCE(SUM(guard_revenue), 0),
    COALESCE(SUM(danmaku_count), 0),
    COALESCE(SUM(gift_count), 0),
    COALESCE(SUM(sc_count), 0)
FROM sessions
WHERE (:room_id IS NULL OR room_id = :room_id)
  AND (:from_time IS NULL OR start_time >= :from_time)
  AND (:to_time IS NULL OR start_time < :to_time)
"#,
        named_params! {
            ":room_id": room_id,
            ":from_time": from_time,
            ":to_time": to_time,
        },
        |row| {
            Ok(ArchiveSummary {
                room_count: row.get::<_, i64>(0)? as u64,
                session_count: row.get::<_, i64>(1)? as u64,
                live_duration: row.get::<_, i64>(2)? as u64,
                total_revenue: row.get::<_, i64>(3)? as u64,
                gift_revenue: row.get::<_, i64>(4)? as u64,
                sc_revenue: row.get::<_, i64>(5)? as u64,
                guard_revenue: row.get::<_, i64>(6)? as u64,
                danmaku_count: row.get::<_, i64>(7)? as u64,
                gift_count: row.get::<_, i64>(8)? as u64,
                sc_count: row.get::<_, i64>(9)? as u64,
            })
        },
    )
    .map_err(|e| format!("查询归档统计失败: {e}"))
}

fn map_session_row(row: &rusqlite::Row) -> rusqlite::Result<ArchiveSession> {
    Ok(ArchiveSession {
        id: row.get(0)?,
        room_id: row.get::<_, i64>(1)? as u64,
        room_title: row.get(2)?,
        streamer_uid: row.get::<_, i64>(3)? as u64,
        start_time: row.get(4)?,
        end_time: row.get(5)?,
        total_revenue: row.get::<_, i64>(6)? as u64,
        gift_revenue: row.get::<_, i64>(7)? as u64,
        sc_revenue: row.get::<_, i64>(8)? as u64,
        guard_revenue: row.get::<_, i64>(9)? as u64,
        danmaku_count: row.get::<_, i64>(10)? as u64,
        gift_count: row.get::<_, i64>(11)? as u64,
        sc_count: row.get::<_, i64>(12)? as u64,
    })
}

fn map_search_row(row: &rusqlite::Row) -> rusqlite::Result<ArchiveSearchItem> {
    Ok(ArchiveSearchItem {
        event_type: row.get(0)?,
        id: row.get(1)?,
        session_id: row.get(2)?,
        room_id: row.get::<_, i64>(3)? as u64,
        room_title: row.get(4)?,
        content: row.get(5)?,
        detail: row.get(6)?,
        user_uid: row.get::<_, i64>(7)? as u64,
        user_name: row.get(8)?,
        timestamp: row.get(9)?,
        amount: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
        quantity: row.get::<_, Option<i64>>(11)?.map(|value| value as u32),
        image_url: row.get(12)?,
        is_emoticon: row.get::<_, i32>(13)? != 0,
        is_paid: row.get::<_, i32>(14)? != 0,
        guard_level: row.get::<_, Option<i64>>(15)?.map(|value| value as u8),
    })
}

fn map_danmaku_row(row: &rusqlite::Row) -> rusqlite::Result<ArchivedDanmaku> {
    Ok(ArchivedDanmaku {
        id: row.get(0)?,
        content: row.get(1)?,
        user_uid: row.get::<_, i64>(2)? as u64,
        user_name: row.get(3)?,
        timestamp: row.get(4)?,
        is_emoticon: row.get::<_, i32>(5)? != 0,
        emoticon_url: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::ArchiveManager;
    use crate::archive_migrations;
    use crate::live_types::{ProcessedGift, ProcessedGiftCombo, ProcessedUser};
    use rusqlite::{params, Connection};
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn updates_combo_snapshot_instead_of_inserting_duplicate_rows() {
        let mut conn = Connection::open_in_memory().expect("in-memory database");
        archive_migrations::initialize(&mut conn).expect("archive schema");
        conn.execute(
            "INSERT INTO sessions (id, room_id, start_time) VALUES (1, 1, 1700000000)",
            [],
        )
        .expect("archive session");
        let archive = ArchiveManager {
            db: Mutex::new(conn),
            active_session_id: Mutex::new(None),
        };

        let mut gift = ProcessedGift {
            id: "gift:combo:42:1:combo-1".to_owned(),
            merge_key: "combo:42:1:combo-1".to_owned(),
            gift_id: 1,
            gift_name: "辣条".to_owned(),
            gift_icon: String::new(),
            num: 1,
            total_value: 1,
            revenue_value: 1,
            is_paid: true,
            combo: Some(ProcessedGiftCombo {
                batch_combo_id: "combo-1".to_owned(),
                combo_total_coin: Some(100),
                super_batch_gift_num: Some(1),
                combo_resources_id: None,
                combo_stay_time: Some(5),
                show_batch_combo_send: Some(true),
            }),
            blind_gift: None,
            user: ProcessedUser {
                uid: 42,
                name: "tester".to_owned(),
                face: None,
                medal: None,
                guard_level: 0,
                is_admin: false,
            },
            timestamp: 1_700_000_000,
            guard_level: None,
        };

        archive.save_gift(1, &gift).await.expect("insert gift");
        gift.num = 2;
        gift.total_value = 2;
        gift.revenue_value = 2;
        gift.combo.as_mut().unwrap().combo_total_coin = Some(200);
        gift.combo.as_mut().unwrap().super_batch_gift_num = Some(2);
        archive.save_gift(1, &gift).await.expect("update gift");

        let db = archive.db.lock().await;
        let (count, num, revenue): (i64, i64, i64) = db
            .query_row(
                "SELECT COUNT(*), MAX(num), MAX(revenue_value) FROM gifts WHERE original_id = ?1",
                params![&gift.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("saved combo snapshot");
        assert_eq!((count, num, revenue), (1, 2, 2));
    }

    #[tokio::test]
    async fn queries_room_overview_statistics_and_unified_search() {
        let mut conn = Connection::open_in_memory().expect("in-memory database");
        archive_migrations::initialize(&mut conn).expect("archive schema");
        conn.execute_batch(
            r#"
INSERT INTO sessions (
    id, room_id, room_title, streamer_uid, start_time, end_time,
    total_revenue, gift_revenue, sc_revenue, guard_revenue,
    danmaku_count, gift_count, sc_count
) VALUES
    (1, 100, '测试房间', 9001, 1700000000, 1700003600, 1500, 1000, 500, 0, 1, 1, 1),
    (2, 100, '测试房间', 9001, 1700086400, 1700090000, 200, 200, 0, 0, 0, 0, 0),
    (3, 200, '另一个房间', 9002, 1700172800, 1700176400, 0, 0, 0, 0, 0, 0, 0);

INSERT INTO danmaku (
    session_id, original_id, content, user_uid, user_name, timestamp
) VALUES (1, 'd1', '你好，世界', 42, 'Alice', 1700000100);

INSERT INTO gifts (
    session_id, original_id, gift_id, gift_name, num, total_value,
    revenue_value, is_paid, user_uid, user_name, timestamp
) VALUES (1, 'g1', 1, '小花花', 2, 1000, 1000, 1, 42, 'Alice', 1700000200);

INSERT INTO super_chats (
    session_id, original_id, content, price, user_uid, user_name, duration, start_time
) VALUES (1, 'sc1', '支持主播', 500, 84, 'Bob', 60, 1700000300);
"#,
        )
        .expect("seed archive data");
        let archive = ArchiveManager {
            db: Mutex::new(conn),
            active_session_id: Mutex::new(None),
        };

        let overview = archive
            .get_overview(None, None, "")
            .await
            .expect("archive overview");
        assert_eq!(overview.summary.room_count, 2);
        assert_eq!(overview.summary.session_count, 3);
        assert_eq!(overview.summary.total_revenue, 1700);
        assert_eq!(overview.rooms.len(), 2);

        let sessions = archive
            .get_room_sessions(100, None, None, 1, 20)
            .await
            .expect("room sessions");
        assert_eq!(sessions.total, 2);
        assert_eq!(sessions.items[0].id, 2);

        let statistics = archive
            .get_statistics(Some(100), None, None)
            .await
            .expect("room statistics");
        assert_eq!(statistics.summary.total_revenue, 1700);
        assert_eq!(statistics.daily.len(), 2);

        let by_content = archive
            .search(None, None, "世界", "all", None, None, 1, 20)
            .await
            .expect("global content search");
        assert_eq!(by_content.total, 1);
        assert_eq!(by_content.items[0].event_type, "danmaku");

        let by_uid = archive
            .search(Some(100), None, "42", "all", None, None, 1, 20)
            .await
            .expect("room uid search");
        assert_eq!(by_uid.total, 2);

        let session_events = archive
            .search(None, Some(1), "", "superchat", None, None, 1, 20)
            .await
            .expect("session event search");
        assert_eq!(session_events.total, 1);
        assert_eq!(session_events.items[0].content, "支持主播");
    }

    #[tokio::test]
    async fn prunes_only_inactive_sessions_without_events() {
        let mut conn = Connection::open_in_memory().expect("in-memory database");
        archive_migrations::initialize(&mut conn).expect("archive schema");
        conn.execute_batch(
            r#"
INSERT INTO sessions (id, room_id, start_time) VALUES
    (1, 100, 1700000000),
    (2, 100, 1700000100),
    (3, 100, 1700000200);
INSERT INTO danmaku (
    session_id, original_id, content, user_uid, user_name, timestamp
) VALUES (2, 'd1', '保留', 42, 'Alice', 1700000110);
"#,
        )
        .expect("seed archive data");
        let archive = ArchiveManager {
            db: Mutex::new(conn),
            active_session_id: Mutex::new(Some(3)),
        };

        assert_eq!(archive.prune_empty_sessions().await.expect("prune"), 1);
        let db = archive.db.lock().await;
        let ids = db
            .prepare("SELECT id FROM sessions ORDER BY id")
            .expect("session query")
            .query_map([], |row| row.get::<_, i64>(0))
            .expect("session rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("session ids");
        assert_eq!(ids, vec![2, 3]);
    }
}
