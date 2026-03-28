//! 存档管理器
//!
//! 使用 SQLite 持久化弹幕、礼物、SC 等直播数据。
//! - 每次连接直播间创建一个 session
//! - 通过 mpsc channel 异步批量写入
//! - 提供分页查询、搜索、删除等功能

use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

use crate::blive_service::{LiveStats, ProcessedDanmaku, ProcessedGift, ProcessedSuperChat};

// ==================== 数据库初始化 ====================

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    room_id         INTEGER NOT NULL,
    room_title      TEXT NOT NULL DEFAULT '',
    streamer_uid    INTEGER NOT NULL DEFAULT 0,
    start_time      INTEGER NOT NULL,
    end_time        INTEGER,
    total_revenue   INTEGER NOT NULL DEFAULT 0,
    gift_revenue    INTEGER NOT NULL DEFAULT 0,
    sc_revenue      INTEGER NOT NULL DEFAULT 0,
    guard_revenue   INTEGER NOT NULL DEFAULT 0,
    danmaku_count   INTEGER NOT NULL DEFAULT 0,
    gift_count      INTEGER NOT NULL DEFAULT 0,
    sc_count        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS danmaku (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL REFERENCES sessions(id),
    original_id     TEXT NOT NULL,
    content         TEXT NOT NULL,
    user_uid        INTEGER NOT NULL,
    user_name       TEXT NOT NULL,
    timestamp       INTEGER NOT NULL,
    is_emoticon     INTEGER NOT NULL DEFAULT 0,
    emoticon_url    TEXT
);

CREATE INDEX IF NOT EXISTS idx_danmaku_session ON danmaku(session_id);
CREATE INDEX IF NOT EXISTS idx_danmaku_timestamp ON danmaku(session_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_danmaku_user ON danmaku(session_id, user_uid);

CREATE TABLE IF NOT EXISTS gifts (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL REFERENCES sessions(id),
    original_id     TEXT NOT NULL,
    gift_id         INTEGER NOT NULL,
    gift_name       TEXT NOT NULL,
    gift_icon       TEXT,
    num             INTEGER NOT NULL,
    total_value     INTEGER NOT NULL,
    is_paid         INTEGER NOT NULL DEFAULT 0,
    user_uid        INTEGER NOT NULL,
    user_name       TEXT NOT NULL,
    timestamp       INTEGER NOT NULL,
    guard_level     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_gifts_session ON gifts(session_id);
CREATE INDEX IF NOT EXISTS idx_gifts_value ON gifts(session_id, total_value);

CREATE TABLE IF NOT EXISTS super_chats (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL REFERENCES sessions(id),
    original_id     TEXT NOT NULL,
    content         TEXT NOT NULL,
    price           INTEGER NOT NULL,
    user_uid        INTEGER NOT NULL,
    user_name       TEXT NOT NULL,
    background_color TEXT,
    duration        INTEGER NOT NULL,
    start_time      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sc_session ON super_chats(session_id);
CREATE INDEX IF NOT EXISTS idx_sc_price ON super_chats(session_id, price);
"#;

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
    pub is_paid: bool,
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

// ==================== ArchiveManager ====================

pub struct ArchiveManager {
    db: Mutex<Connection>,
    active_session_id: Mutex<Option<i64>>,
}

impl ArchiveManager {
    /// 创建并初始化 ArchiveManager
    pub fn new(db_path: std::path::PathBuf) -> Result<Self, String> {
        let conn = Connection::open(&db_path).map_err(|e| format!("打开存档数据库失败: {}", e))?;

        // 性能优化
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("设置 PRAGMA 失败: {}", e))?;

        // 初始化表
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| format!("初始化存档表失败: {}", e))?;

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
                    "SELECT COALESCE(SUM(total_value), 0) FROM gifts WHERE session_id = ?1 AND is_paid = 1",
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

        tx.commit()
            .map_err(|e| format!("提交事务失败: {}", e))?;
        Ok(())
    }

    pub async fn save_gift(&self, session_id: i64, gift: &ProcessedGift) -> Result<(), String> {
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO gifts (session_id, original_id, gift_id, gift_name, gift_icon, num, total_value, is_paid, user_uid, user_name, timestamp, guard_level) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                session_id,
                &gift.id,
                gift.gift_id as i64,
                &gift.gift_name,
                &gift.gift_icon,
                gift.num as i64,
                gift.total_value as i64,
                gift.is_paid as i32,
                gift.user.uid as i64,
                &gift.user.name,
                gift.timestamp,
                gift.guard_level.map(|g| g as i64),
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

    pub async fn search_danmaku(
        &self,
        session_id: i64,
        query: &str,
        page: u32,
        page_size: u32,
    ) -> Result<PagedResult<ArchivedDanmaku>, String> {
        let db = self.db.lock().await;
        let offset = (page.saturating_sub(1)) * page_size;

        let (where_clause, query_param) = if query.is_empty() {
            ("session_id = ?1".to_string(), None)
        } else {
            (
                "session_id = ?1 AND (content LIKE ?2 OR user_name LIKE ?2)".to_string(),
                Some(format!("%{}%", query)),
            )
        };

        let total: u64 = if let Some(ref q) = query_param {
            db.query_row(
                &format!("SELECT COUNT(*) FROM danmaku WHERE {}", where_clause),
                params![session_id, q],
                |row| row.get::<_, i64>(0),
            )
        } else {
            db.query_row(
                &format!("SELECT COUNT(*) FROM danmaku WHERE {}", where_clause),
                params![session_id],
                |row| row.get::<_, i64>(0),
            )
        }
        .map_err(|e| e.to_string())? as u64;

        let sql = format!(
            "SELECT id, content, user_uid, user_name, timestamp, is_emoticon, emoticon_url FROM danmaku WHERE {} ORDER BY timestamp ASC LIMIT ?{} OFFSET ?{}",
            where_clause,
            if query_param.is_some() { "3" } else { "2" },
            if query_param.is_some() { "4" } else { "3" },
        );

        let items = if let Some(ref q) = query_param {
            let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(
                    params![session_id, q, page_size as i64, offset as i64],
                    map_danmaku_row,
                )
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        } else {
            let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(
                    params![session_id, page_size as i64, offset as i64],
                    map_danmaku_row,
                )
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };

        Ok(PagedResult {
            items,
            total,
            page,
            page_size,
        })
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
        let offset = (page.saturating_sub(1)) * page_size;

        let mut conditions = vec!["session_id = ?1".to_string()];
        let mut param_idx = 2;

        if !query.is_empty() {
            conditions.push(format!(
                "(gift_name LIKE ?{0} OR user_name LIKE ?{0})",
                param_idx
            ));
            param_idx += 1;
        }
        if min_price.is_some() {
            conditions.push(format!("total_value >= ?{}", param_idx));
            param_idx += 1;
        }
        if max_price.is_some() {
            conditions.push(format!("total_value <= ?{}", param_idx));
            param_idx += 1;
        }

        let where_clause = conditions.join(" AND ");
        let limit_param = param_idx;
        let offset_param = param_idx + 1;

        // Build dynamic params
        let count_sql = format!("SELECT COUNT(*) FROM gifts WHERE {}", where_clause);
        let data_sql = format!(
            "SELECT id, gift_name, gift_icon, num, total_value, is_paid, user_uid, user_name, timestamp, guard_level FROM gifts WHERE {} ORDER BY timestamp ASC LIMIT ?{} OFFSET ?{}",
            where_clause, limit_param, offset_param
        );

        // We need to use dynamic params; rusqlite supports this via a Vec<Box<dyn ToSql>>
        use rusqlite::types::ToSql;
        let mut bind_values: Vec<Box<dyn ToSql>> = Vec::new();
        bind_values.push(Box::new(session_id));
        if !query.is_empty() {
            bind_values.push(Box::new(format!("%{}%", query)));
        }
        if let Some(min) = min_price {
            bind_values.push(Box::new(min as i64));
        }
        if let Some(max) = max_price {
            bind_values.push(Box::new(max as i64));
        }

        let bind_refs: Vec<&dyn ToSql> = bind_values.iter().map(|b| b.as_ref()).collect();

        let total: u64 = db
            .query_row(&count_sql, bind_refs.as_slice(), |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|e| e.to_string())? as u64;

        let mut data_bind_values = bind_values;
        data_bind_values.push(Box::new(page_size as i64));
        data_bind_values.push(Box::new(offset as i64));
        let data_bind_refs: Vec<&dyn ToSql> =
            data_bind_values.iter().map(|b| b.as_ref()).collect();

        let mut stmt = db.prepare(&data_sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(data_bind_refs.as_slice(), |row| {
                Ok(ArchivedGift {
                    id: row.get(0)?,
                    gift_name: row.get(1)?,
                    gift_icon: row.get(2)?,
                    num: row.get::<_, i64>(3)? as u32,
                    total_value: row.get::<_, i64>(4)? as u64,
                    is_paid: row.get::<_, i32>(5)? != 0,
                    user_uid: row.get::<_, i64>(6)? as u64,
                    user_name: row.get(7)?,
                    timestamp: row.get(8)?,
                    guard_level: row
                        .get::<_, Option<i64>>(9)?
                        .map(|g| g as u8),
                })
            })
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
        let offset = (page.saturating_sub(1)) * page_size;

        let mut conditions = vec!["session_id = ?1".to_string()];
        let mut param_idx = 2;

        if !query.is_empty() {
            conditions.push(format!(
                "(content LIKE ?{0} OR user_name LIKE ?{0})",
                param_idx
            ));
            param_idx += 1;
        }
        if min_price.is_some() {
            conditions.push(format!("price >= ?{}", param_idx));
            param_idx += 1;
        }
        if max_price.is_some() {
            conditions.push(format!("price <= ?{}", param_idx));
            param_idx += 1;
        }

        let where_clause = conditions.join(" AND ");
        let limit_param = param_idx;
        let offset_param = param_idx + 1;

        let count_sql = format!("SELECT COUNT(*) FROM super_chats WHERE {}", where_clause);
        let data_sql = format!(
            "SELECT id, content, price, user_uid, user_name, background_color, duration, start_time FROM super_chats WHERE {} ORDER BY start_time ASC LIMIT ?{} OFFSET ?{}",
            where_clause, limit_param, offset_param
        );

        use rusqlite::types::ToSql;
        let mut bind_values: Vec<Box<dyn ToSql>> = Vec::new();
        bind_values.push(Box::new(session_id));
        if !query.is_empty() {
            bind_values.push(Box::new(format!("%{}%", query)));
        }
        if let Some(min) = min_price {
            bind_values.push(Box::new(min as i64));
        }
        if let Some(max) = max_price {
            bind_values.push(Box::new(max as i64));
        }

        let bind_refs: Vec<&dyn ToSql> = bind_values.iter().map(|b| b.as_ref()).collect();

        let total: u64 = db
            .query_row(&count_sql, bind_refs.as_slice(), |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|e| e.to_string())? as u64;

        let mut data_bind_values = bind_values;
        data_bind_values.push(Box::new(page_size as i64));
        data_bind_values.push(Box::new(offset as i64));
        let data_bind_refs: Vec<&dyn ToSql> =
            data_bind_values.iter().map(|b| b.as_ref()).collect();

        let mut stmt = db.prepare(&data_sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(data_bind_refs.as_slice(), |row| {
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
            })
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

    pub async fn delete_session(&self, session_id: i64) -> Result<(), String> {
        let db = self.db.lock().await;
        db.execute_batch(&format!(
            "DELETE FROM danmaku WHERE session_id = {};
             DELETE FROM gifts WHERE session_id = {};
             DELETE FROM super_chats WHERE session_id = {};
             DELETE FROM sessions WHERE id = {};",
            session_id, session_id, session_id, session_id,
        ))
        .map_err(|e| format!("删除存档失败: {}", e))?;
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
                    flush_buffers(&archive, session_id, &mut danmaku_buf, &mut gift_buf, &mut sc_buf).await;
                    break;
                }
                Err(_) => {
                    // Timeout, will flush below
                }
            }

            // Flush when buffer is large enough or on timeout
            if danmaku_buf.len() >= 100 || gift_buf.len() >= 50 || sc_buf.len() >= 20 {
                flush_buffers(&archive, session_id, &mut danmaku_buf, &mut gift_buf, &mut sc_buf).await;
            } else if is_timeout && (!danmaku_buf.is_empty() || !gift_buf.is_empty() || !sc_buf.is_empty()) {
                flush_buffers(&archive, session_id, &mut danmaku_buf, &mut gift_buf, &mut sc_buf).await;
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
        let items: Vec<_> = danmaku_buf.drain(..).collect();
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
