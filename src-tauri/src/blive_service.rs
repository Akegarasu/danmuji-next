//! Bilibili 弹幕服务管理器
//!
//! 负责：
//! - 弹幕客户端生命周期管理（连接/断开/重连）
//! - 事件处理和数据聚合（礼物合并、统计等）
//! - 窗口订阅机制：按需向不同窗口分发事件
//! - 数据快照：新窗口可获取当前完整数据

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::archive::{ArchiveEvent, ArchiveManager};
use crate::video_info::{self, VideoInfo};
use crate::blivedm::api::{
    get_contribution_rank, get_danmu_info, get_room_init, ContributionRankUser, RoomInfo,
};
use crate::blivedm::{
    BliveDmClient, CoinType, Danmaku, DanmakuType, Error as BliveError, Event, Gift, GuardBuy,
    GuardLevel, Medal, OnlineRankCount, OnlineRankV2, SuperChat,
};

/// 数据推送间隔
const DATA_PUSH_INTERVAL: Duration = Duration::from_millis(100);

/// 最大列表长度（扩大缓存，10-20MB 可接受）
const MAX_DANMAKU_LIST: usize = 10000;
const MAX_GIFT_LIST: usize = 5000;
const MAX_SUPERCHAT_LIST: usize = 2000;

/// 礼物合并时间窗口（秒）
const GIFT_MERGE_WINDOW_SECS: i64 = 5;

// ==================== 事件类型（用于订阅）====================

/// 事件类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// 弹幕事件
    Danmaku,
    /// 礼物事件
    Gift,
    /// SuperChat 事件
    SuperChat,
    /// 贡献排行（实时和完整）
    ContributionRank,
    /// 统计数据
    Stats,
    /// 直播状态（开播/下播）
    LiveStatus,
    /// 点播请求
    VideoRequest,
}

// ==================== 连接状态 ====================

/// 连接状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error { message: String },
}

/// 房间信息（发送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfoResponse {
    pub room_id: u64,
    pub short_id: u64,
    pub uid: u64,
    pub title: String,
    pub live_status: i32,
}

impl From<RoomInfo> for RoomInfoResponse {
    fn from(info: RoomInfo) -> Self {
        Self {
            room_id: info.room_id,
            short_id: info.short_id,
            uid: info.uid,
            title: info.title,
            live_status: info.live_status,
        }
    }
}

/// 连接结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectResult {
    pub success: bool,
    pub message: String,
    pub room_info: Option<RoomInfoResponse>,
}

// ==================== 处理后的数据类型 ====================

/// 处理后的弹幕
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedDanmaku {
    pub id: String,
    pub content: String,
    pub user: ProcessedUser,
    pub timestamp: i64,
    pub is_emoticon: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoticon_url: Option<String>,
}

/// 处理后的礼物（可合并）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedGift {
    pub id: String,
    pub merge_key: String,
    pub gift_id: u64,
    pub gift_name: String,
    pub gift_icon: String,
    pub num: u32,
    pub total_value: u64,
    pub is_paid: bool,
    pub user: ProcessedUser,
    pub timestamp: i64,
    /// 大航海等级（仅大航海购买时有值：1=总督, 2=提督, 3=舰长）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard_level: Option<u8>,
}

/// 处理后的 SC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedSuperChat {
    pub id: String,
    pub content: String,
    pub price: u64, // 电池（1元=10电池）
    pub user: ProcessedUser,
    pub background_color: String,
    pub duration: u32,
    pub start_time: i64,
}

/// 处理后的用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedUser {
    pub uid: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal: Option<ProcessedMedal>,
    pub guard_level: u8,
    pub is_admin: bool,
}

/// 处理后的勋章
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedMedal {
    pub name: String,
    pub level: u32,
    pub color: String,
}

/// 高能用户排行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedOnlineRankUser {
    pub uid: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
    pub rank: u32,
    pub score: String,
    pub guard_level: u8,
}

/// 用户贡献统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContribution {
    pub uid: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
    pub total_value: u64, // 电池
    pub guard_level: u8,
}

/// 直播统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiveStats {
    pub total_revenue: u64, // 总收入（电池）
    pub gift_revenue: u64,  // 礼物收入
    pub sc_revenue: u64,    // SC 收入
    pub guard_revenue: u64, // 大航海收入
    pub online_count: u32,  // 在线人数（观看人数）
}

// ==================== 数据更新类型 ====================

/// 点播请求（发送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoRequestItem {
    /// 唯一 ID
    pub id: String,
    /// 视频 ID（BV/AV号）
    pub video_id: String,
    /// 请求者用户名
    pub username: String,
    /// 请求者 UID
    pub uid: u64,
    /// 来源类型
    pub source: VideoRequestSource,
    /// SC 金额（电池，仅 SC 来源时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sc_price: Option<u64>,
    /// 请求时间戳（毫秒）
    pub timestamp: i64,
    /// 是否已看
    pub watched: bool,
    /// 视频信息（异步加载后填充）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_info: Option<VideoInfo>,
    /// 是否正在加载
    pub loading: bool,
    /// 加载错误
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 点播来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoRequestSource {
    Danmaku,
    Superchat,
}

/// 数据更新（发送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DataUpdate {
    /// 弹幕追加
    DanmakuAppend(Vec<ProcessedDanmaku>),
    /// 礼物更新（新增或合并）
    GiftUpsert(Vec<GiftUpsert>),
    /// SC 追加
    SuperChatAppend(ProcessedSuperChat),
    /// 贡献排行实时更新（ONLINE_RANK_V2，前几名）
    ContributionRankLive(Vec<ProcessedOnlineRankUser>),
    /// 贡献排行完整列表（API 获取，最多 100 人）
    ContributionRankFull(Vec<ContributionRankUser>),
    /// 统计更新
    StatsUpdate(LiveStats),
    /// 用户贡献排行更新（本场礼物/SC 贡献前 N 名）
    ContributionsUpdate(Vec<UserContribution>),
    /// 开播
    LiveStart,
    /// 下播
    LiveStop,
    /// 点播请求追加
    VideoRequestAppend(VideoRequestItem),
    /// 点播请求更新（视频信息加载完成）
    VideoRequestUpdate(VideoRequestItem),
    /// 点播列表全量同步（用于 watched/remove/clear 操作后）
    VideoRequestSync(Vec<VideoRequestItem>),
}

impl DataUpdate {
    /// 获取事件类型
    fn event_type(&self) -> EventType {
        match self {
            DataUpdate::DanmakuAppend(_) => EventType::Danmaku,
            DataUpdate::GiftUpsert(_) => EventType::Gift,
            DataUpdate::SuperChatAppend(_) => EventType::SuperChat,
            DataUpdate::ContributionRankLive(_) => EventType::ContributionRank,
            DataUpdate::ContributionRankFull(_) => EventType::ContributionRank,
            DataUpdate::StatsUpdate(_) => EventType::Stats,
            DataUpdate::ContributionsUpdate(_) => EventType::ContributionRank,
            DataUpdate::LiveStart => EventType::LiveStatus,
            DataUpdate::LiveStop => EventType::LiveStatus,
            DataUpdate::VideoRequestAppend(_) => EventType::VideoRequest,
            DataUpdate::VideoRequestUpdate(_) => EventType::VideoRequest,
            DataUpdate::VideoRequestSync(_) => EventType::VideoRequest,
        }
    }
}

/// 礼物更新操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftUpsert {
    pub merge_key: String,
    pub gift: ProcessedGift,
    pub action: UpsertAction,
}

/// 更新动作类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpsertAction {
    Insert,
    Update,
}

// ==================== 数据快照（用于新窗口同步）====================

/// 数据快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSnapshot {
    /// 弹幕列表
    #[serde(skip_serializing_if = "Option::is_none")]
    pub danmaku_list: Option<Vec<ProcessedDanmaku>>,
    /// 礼物列表
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gift_list: Option<Vec<ProcessedGift>>,
    /// SC 列表
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superchat_list: Option<Vec<ProcessedSuperChat>>,
    /// 贡献排行实时（前几名）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution_rank_live: Option<Vec<ProcessedOnlineRankUser>>,
    /// 贡献排行完整（API）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution_rank_full: Option<Vec<ContributionRankUser>>,
    /// 用户贡献排行（本场）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributions: Option<Vec<UserContribution>>,
    /// 统计数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<LiveStats>,
    /// 点播请求列表
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_requests: Option<Vec<VideoRequestItem>>,
}

// ==================== 数据状态 ====================

/// 直播数据状态
struct LiveData {
    /// 弹幕列表
    danmaku_list: VecDeque<ProcessedDanmaku>,
    /// 礼物列表
    gift_list: VecDeque<ProcessedGift>,
    /// 礼物合并索引: merge_key -> list index
    gift_merge_index: HashMap<String, usize>,
    /// SC 列表
    superchat_list: Vec<ProcessedSuperChat>,
    /// 高能用户排行
    online_rank: Vec<ProcessedOnlineRankUser>,
    /// 贡献排行完整列表（API）
    contribution_rank_full: Vec<ContributionRankUser>,
    /// 用户贡献 map: uid -> contribution
    user_contributions: HashMap<u64, UserContribution>,
    /// 统计数据
    stats: LiveStats,

    /// 点播请求列表
    video_requests: Vec<VideoRequestItem>,
    /// 已见过的视频 ID（用于去重，小写）
    video_request_ids: HashSet<String>,

    /// 待发送的更新
    pending_updates: Vec<DataUpdate>,
    /// 待发送的弹幕（批量）
    pending_danmaku: Vec<ProcessedDanmaku>,
    /// 待发送的礼物更新（批量）
    pending_gift_upserts: Vec<GiftUpsert>,
    /// 统计是否有变化
    stats_dirty: bool,
    /// 贡献排行是否有变化
    contributions_dirty: bool,
    /// 存档 channel sender（连接时设置）
    archive_tx: Option<mpsc::UnboundedSender<ArchiveEvent>>,
}

impl Default for LiveData {
    fn default() -> Self {
        Self {
            danmaku_list: VecDeque::with_capacity(MAX_DANMAKU_LIST),
            gift_list: VecDeque::with_capacity(MAX_GIFT_LIST),
            gift_merge_index: HashMap::new(),
            superchat_list: Vec::with_capacity(MAX_SUPERCHAT_LIST),
            online_rank: Vec::new(),
            contribution_rank_full: Vec::new(),
            user_contributions: HashMap::new(),
            stats: LiveStats::default(),
            video_requests: Vec::new(),
            video_request_ids: HashSet::new(),
            pending_updates: Vec::new(),
            pending_danmaku: Vec::new(),
            pending_gift_upserts: Vec::new(),
            stats_dirty: false,
            contributions_dirty: false,
            archive_tx: None,
        }
    }
}

impl LiveData {
    /// 清空所有数据
    fn clear(&mut self) {
        *self = Self::default();
    }

    /// 生成数据快照
    fn snapshot(&self, event_types: &HashSet<EventType>) -> DataSnapshot {
        DataSnapshot {
            danmaku_list: if event_types.contains(&EventType::Danmaku) {
                Some(self.danmaku_list.iter().cloned().collect())
            } else {
                None
            },
            gift_list: if event_types.contains(&EventType::Gift) {
                Some(self.gift_list.iter().cloned().collect())
            } else {
                None
            },
            superchat_list: if event_types.contains(&EventType::SuperChat) {
                Some(self.superchat_list.clone())
            } else {
                None
            },
            contribution_rank_live: if event_types.contains(&EventType::ContributionRank) {
                Some(self.online_rank.clone())
            } else {
                None
            },
            contribution_rank_full: if event_types.contains(&EventType::ContributionRank) {
                Some(self.contribution_rank_full.clone())
            } else {
                None
            },
            contributions: if event_types.contains(&EventType::ContributionRank) {
                let mut contributions: Vec<_> = self.user_contributions.values().cloned().collect();
                contributions.sort_by(|a, b| b.total_value.cmp(&a.total_value));
                contributions.truncate(50);
                Some(contributions)
            } else {
                None
            },
            stats: if event_types.contains(&EventType::Stats) {
                Some(self.stats.clone())
            } else {
                None
            },
            video_requests: if event_types.contains(&EventType::VideoRequest) {
                Some(self.video_requests.clone())
            } else {
                None
            },
        }
    }

    /// 处理弹幕
    fn process_danmaku(&mut self, danmaku: Danmaku) -> Vec<(String, String, u64, Option<u64>)> {
        let processed = ProcessedDanmaku {
            id: format!("dm_{}_{}", danmaku.timestamp, danmaku.sender.uid),
            content: danmaku.content,
            user: convert_user(&danmaku.sender),
            timestamp: danmaku.timestamp,
            is_emoticon: danmaku.r#type == DanmakuType::Emoticon,
            emoticon_url: danmaku.emoticon.map(|e| e.url),
        };

        self.danmaku_list.push_back(processed.clone());
        if self.danmaku_list.len() > MAX_DANMAKU_LIST {
            self.danmaku_list.pop_front();
        }

        // 发送到存档
        if let Some(tx) = &self.archive_tx {
            let _ = tx.send(ArchiveEvent::Danmaku(processed.clone()));
        }

        // 检测 BV/AV号
        let detected = self.detect_and_add_video_requests(
            &processed.content,
            &processed.user.name,
            processed.user.uid,
            VideoRequestSource::Danmaku,
            None,
            processed.timestamp,
        );

        self.pending_danmaku.push(processed);
        detected
    }

    /// 处理礼物
    fn process_gift(&mut self, gift: Gift) {
        // 计算合并 key
        let time_bucket = gift.timestamp / GIFT_MERGE_WINDOW_SECS;
        let merge_key = format!("{}_{}_{}", gift.sender_uid, gift.gift_id, time_bucket);

        // 计算价值（电池）
        let value = if gift.coin_type == CoinType::Gold {
            gift.price as u64 / 100 
        } else {
            0
        };
        let is_paid = gift.coin_type == CoinType::Gold;

        // 先保存用于贡献统计的数据
        let sender_uid = gift.sender_uid;
        let sender_name = gift.sender_name.clone();
        let sender_face = gift.sender_face.clone();
        let guard_level = gift.guard_level.clone();

        // 检查是否需要合并
        if let Some(&index) = self.gift_merge_index.get(&merge_key) {
            // 更新已有礼物
            if let Some(existing) = self.gift_list.get_mut(index) {
                existing.num += gift.num;
                existing.total_value += value;
                existing.timestamp = gift.timestamp;

                self.pending_gift_upserts.push(GiftUpsert {
                    merge_key: merge_key.clone(),
                    gift: existing.clone(),
                    action: UpsertAction::Update,
                });
            }
        } else {
            // 新增礼物
            let processed = ProcessedGift {
                id: format!("gift_{}_{}", gift.timestamp, gift.sender_uid),
                merge_key: merge_key.clone(),
                gift_id: gift.gift_id,
                gift_name: gift.gift_name,
                gift_icon: gift.gift_icon,
                num: gift.num,
                total_value: value,
                is_paid,
                user: ProcessedUser {
                    uid: gift.sender_uid,
                    name: gift.sender_name,
                    face: gift.sender_face,
                    medal: gift.medal.map(|m| convert_medal(&m)),
                    guard_level: guard_level_to_u8(&gift.guard_level),
                    is_admin: false,
                },
                timestamp: gift.timestamp,
                guard_level: None,
            };

            // 发送到存档（每条原始礼物记录都存档）
            if let Some(tx) = &self.archive_tx {
                let _ = tx.send(ArchiveEvent::Gift(processed.clone()));
            }

            let index = self.gift_list.len();
            self.gift_list.push_back(processed.clone());
            self.gift_merge_index.insert(merge_key.clone(), index);

            // 限制列表长度
            if self.gift_list.len() > MAX_GIFT_LIST {
                self.gift_list.pop_front();
                self.rebuild_gift_index();
            }

            self.pending_gift_upserts.push(GiftUpsert {
                merge_key,
                gift: processed,
                action: UpsertAction::Insert,
            });
        }

        // 更新统计
        if is_paid && value > 0 {
            self.stats.gift_revenue += value;
            self.stats.total_revenue += value;
            self.stats_dirty = true;

            // 更新用户贡献
            self.update_user_contribution(
                sender_uid,
                &sender_name,
                sender_face.as_deref(),
                value,
                &guard_level,
            );
        }
    }

    /// 处理 SC
    fn process_superchat(&mut self, sc: SuperChat) -> Vec<(String, String, u64, Option<u64>)> {
        // SC 价格：原始 price 是人民币，转换为电池（1元=10电池）
        let price = (sc.price as u64) * 10;

        // 先保存用于贡献统计的数据
        let sender_uid = sc.sender_uid;
        let sender_name = sc.sender_name.clone();
        let sender_face = sc.sender_face.clone();
        let guard_level = sc.guard_level.clone();

        let processed = ProcessedSuperChat {
            id: format!("sc_{}", sc.id),
            content: sc.message,
            price,
            user: ProcessedUser {
                uid: sc.sender_uid,
                name: sc.sender_name,
                face: sc.sender_face,
                medal: sc.medal.map(|m| convert_medal(&m)),
                guard_level: guard_level_to_u8(&sc.guard_level),
                is_admin: false,
            },
            background_color: sc.background_color,
            duration: sc.duration,
            start_time: sc.start_time,
        };

        self.superchat_list.insert(0, processed.clone()); // 新的在前面
        if self.superchat_list.len() > MAX_SUPERCHAT_LIST {
            self.superchat_list.pop();
        }

        // 发送到存档
        if let Some(tx) = &self.archive_tx {
            let _ = tx.send(ArchiveEvent::SuperChat(processed.clone()));
        }

        // 更新统计
        self.stats.sc_revenue += price;
        self.stats.total_revenue += price;
        self.stats_dirty = true;

        // 更新用户贡献
        self.update_user_contribution(
            sender_uid,
            &sender_name,
            sender_face.as_deref(),
            price,
            &guard_level,
        );

        // 检测 BV/AV号
        let detected = self.detect_and_add_video_requests(
            &processed.content,
            &processed.user.name,
            processed.user.uid,
            VideoRequestSource::Superchat,
            Some(price),
            processed.start_time,
        );

        self.pending_updates
            .push(DataUpdate::SuperChatAppend(processed));

        detected
    }

    /// 处理大航海
    fn process_guard_buy(&mut self, guard: GuardBuy) {
        // 大航海价值（电池）：price 是金瓜子，除以 100 得到电池
        let mut value = guard.price / 100;
        let guard_level_u8 = guard_level_to_u8(&guard.guard_level);
        let timestamp = guard.start_time;

        if guard.num > 1 {
            value *= guard.num as u64;
        }

        // 生成唯一 ID（大航海不合并）
        let id = format!("guard_{}_{}", timestamp, guard.uid);
        let merge_key = id.clone();

        // 作为礼物添加到列表
        let processed = ProcessedGift {
            id,
            merge_key: merge_key.clone(),
            gift_id: guard.gift_id,
            gift_name: guard.guard_name().to_string(),
            gift_icon: "".to_string(),
            num: guard.num,
            total_value: value,
            is_paid: true,
            user: ProcessedUser {
                uid: guard.uid,
                name: guard.username.clone(),
                face: None,
                medal: None,
                guard_level: guard_level_u8,
                is_admin: false,
            },
            timestamp,
            guard_level: Some(guard_level_u8),
        };

        // 发送到存档
        if let Some(tx) = &self.archive_tx {
            let _ = tx.send(ArchiveEvent::Gift(processed.clone()));
        }

        let index = self.gift_list.len();
        self.gift_list.push_back(processed.clone());
        self.gift_merge_index.insert(merge_key, index);

        // 限制列表长度
        if self.gift_list.len() > MAX_GIFT_LIST {
            self.gift_list.pop_front();
            self.rebuild_gift_index();
        }

        self.pending_gift_upserts.push(GiftUpsert {
            merge_key: processed.merge_key.clone(),
            gift: processed,
            action: UpsertAction::Insert,
        });

        // 更新统计
        self.stats.guard_revenue += value;
        self.stats.total_revenue += value;
        self.stats_dirty = true;

        self.update_user_contribution(guard.uid, &guard.username, None, value, &guard.guard_level);
    }

    /// 处理贡献排行实时更新（ONLINE_RANK_V2）
    fn process_online_rank(&mut self, rank: OnlineRankV2) {
        self.online_rank = rank
            .online_list
            .into_iter()
            .map(|u| ProcessedOnlineRankUser {
                uid: u.uid,
                name: u.name,
                face: u.face,
                rank: u.rank,
                score: u.score,
                guard_level: guard_level_to_u8(&u.guard_level),
            })
            .collect();

        self.pending_updates
            .push(DataUpdate::ContributionRankLive(self.online_rank.clone()));
    }

    /// 处理在线人数
    fn process_online_count(&mut self, count: OnlineRankCount) {
        self.stats.online_count = count.online_count;
        self.stats_dirty = true;
    }

    /// 设置贡献排行完整列表
    fn set_contribution_rank_full(&mut self, rank: Vec<ContributionRankUser>) {
        self.contribution_rank_full = rank.clone();
        self.pending_updates
            .push(DataUpdate::ContributionRankFull(rank));
    }

    /// 更新用户贡献
    fn update_user_contribution(
        &mut self,
        uid: u64,
        name: &str,
        face: Option<&str>,
        value: u64,
        guard_level: &GuardLevel,
    ) {
        let entry = self
            .user_contributions
            .entry(uid)
            .or_insert_with(|| UserContribution {
                uid,
                name: name.to_string(),
                face: face.map(String::from),
                total_value: 0,
                guard_level: guard_level_to_u8(guard_level),
            });
        entry.total_value += value;
        entry.name = name.to_string();
        if let Some(f) = face {
            entry.face = Some(f.to_string());
        }
        self.contributions_dirty = true;
    }

    /// 重建礼物合并索引
    fn rebuild_gift_index(&mut self) {
        self.gift_merge_index.clear();
        for (i, gift) in self.gift_list.iter().enumerate() {
            self.gift_merge_index.insert(gift.merge_key.clone(), i);
        }
    }

    /// 从文本中检测 BV/AV号，创建点播请求
    /// 返回需要异步获取视频信息的列表: (request_id, video_id, uid, sc_price)
    fn detect_and_add_video_requests(
        &mut self,
        content: &str,
        username: &str,
        uid: u64,
        source: VideoRequestSource,
        sc_price: Option<u64>,
        timestamp: i64,
    ) -> Vec<(String, String, u64, Option<u64>)> {
        let re = Regex::new(r"(?i)(BV[a-zA-Z0-9]{10}|av\d+)").unwrap();
        let mut to_fetch = Vec::new();

        for cap in re.captures_iter(content) {
            let video_id = cap[1].to_string();
            let key = video_id.to_lowercase();

            // 去重
            if self.video_request_ids.contains(&key) {
                continue;
            }
            self.video_request_ids.insert(key);

            let id = format!("vr_{}_{}", timestamp, uid);
            let item = VideoRequestItem {
                id: id.clone(),
                video_id: video_id.clone(),
                username: username.to_string(),
                uid,
                source: source.clone(),
                sc_price,
                timestamp: if timestamp < 1_000_000_000_000 {
                    timestamp * 1000
                } else {
                    timestamp
                },
                watched: false,
                video_info: None,
                loading: true,
                error: None,
            };

            self.video_requests.insert(0, item.clone());
            self.pending_updates
                .push(DataUpdate::VideoRequestAppend(item));
            to_fetch.push((id, video_id, uid, sc_price));
        }

        to_fetch
    }

    /// 更新点播请求的视频信息
    fn update_video_request_info(
        &mut self,
        request_id: &str,
        info: Result<VideoInfo, String>,
    ) {
        if let Some(item) = self.video_requests.iter_mut().find(|r| r.id == request_id) {
            match info {
                Ok(vi) => {
                    item.video_info = Some(vi);
                    item.loading = false;
                    item.error = None;
                }
                Err(e) => {
                    item.loading = false;
                    item.error = Some(e);
                }
            }
            self.pending_updates
                .push(DataUpdate::VideoRequestUpdate(item.clone()));
        }
    }

    /// 标记点播为已看/未看
    fn set_video_watched(&mut self, request_id: &str, watched: bool) {
        if let Some(item) = self.video_requests.iter_mut().find(|r| r.id == request_id) {
            item.watched = watched;
        }
        self.pending_updates
            .push(DataUpdate::VideoRequestSync(self.video_requests.clone()));
    }

    /// 删除点播请求
    fn remove_video_request(&mut self, request_id: &str) {
        self.video_requests.retain(|r| r.id != request_id);
        self.pending_updates
            .push(DataUpdate::VideoRequestSync(self.video_requests.clone()));
    }

    /// 清空已看的点播
    fn clear_watched_videos(&mut self) {
        self.video_requests.retain(|r| !r.watched);
        self.pending_updates
            .push(DataUpdate::VideoRequestSync(self.video_requests.clone()));
    }

    /// 清空所有点播
    fn clear_all_videos(&mut self) {
        self.video_requests.clear();
        self.video_request_ids.clear();
        self.pending_updates
            .push(DataUpdate::VideoRequestSync(self.video_requests.clone()));
    }

    /// 获取待发送的更新，并清空缓冲区
    fn take_pending_updates(&mut self) -> Vec<DataUpdate> {
        let mut updates = std::mem::take(&mut self.pending_updates);

        // 弹幕批量发送
        if !self.pending_danmaku.is_empty() {
            updates.push(DataUpdate::DanmakuAppend(std::mem::take(
                &mut self.pending_danmaku,
            )));
        }

        // 礼物批量发送
        if !self.pending_gift_upserts.is_empty() {
            updates.push(DataUpdate::GiftUpsert(std::mem::take(
                &mut self.pending_gift_upserts,
            )));
        }

        // 统计更新
        if self.stats_dirty {
            updates.push(DataUpdate::StatsUpdate(self.stats.clone()));
            self.stats_dirty = false;
        }

        // 贡献排行更新（取前 50 名）
        if self.contributions_dirty {
            let mut contributions: Vec<_> = self.user_contributions.values().cloned().collect();
            contributions.sort_by(|a, b| b.total_value.cmp(&a.total_value));
            contributions.truncate(50);
            updates.push(DataUpdate::ContributionsUpdate(contributions));
            self.contributions_dirty = false;
        }

        updates
    }
}

// ==================== 辅助函数 ====================

fn convert_user(user: &crate::blivedm::User) -> ProcessedUser {
    ProcessedUser {
        uid: user.uid,
        name: user.name.clone(),
        face: user.face.clone(),
        medal: user.medal.as_ref().map(convert_medal),
        guard_level: guard_level_to_u8(&user.guard_level),
        is_admin: user.is_admin,
    }
}

fn convert_medal(medal: &Medal) -> ProcessedMedal {
    ProcessedMedal {
        name: medal.name.clone(),
        level: medal.level,
        color: format!("#{:06x}", medal.color),
    }
}

fn guard_level_to_u8(level: &GuardLevel) -> u8 {
    match level {
        GuardLevel::Governor => 1,
        GuardLevel::Admiral => 2,
        GuardLevel::Captain => 3,
        GuardLevel::None => 0,
    }
}

// ==================== 窗口订阅 ====================

/// 窗口订阅信息
#[derive(Debug, Clone, Default)]
struct WindowSubscription {
    /// 订阅的事件类型
    event_types: HashSet<EventType>,
}

// ==================== 服务状态 ====================

struct ServiceState {
    status: ConnectionStatus,
    room_id: u64,
    room_info: Option<RoomInfo>,
    stop_tx: Option<mpsc::Sender<()>>,
    task_handle: Option<JoinHandle<()>>,
}

impl Default for ServiceState {
    fn default() -> Self {
        Self {
            status: ConnectionStatus::Disconnected,
            room_id: 0,
            room_info: None,
            stop_tx: None,
            task_handle: None,
        }
    }
}

// ==================== 弹幕服务 ====================

pub struct BliveService {
    state: RwLock<ServiceState>,
    live_data: Arc<Mutex<LiveData>>,
    /// 窗口订阅: window_label -> subscription
    subscriptions: RwLock<HashMap<String, WindowSubscription>>,
}

impl BliveService {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(ServiceState::default()),
            live_data: Arc::new(Mutex::new(LiveData::default())),
            subscriptions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_status(&self) -> ConnectionStatus {
        self.state.read().await.status.clone()
    }

    pub async fn get_room_info(&self) -> Option<RoomInfoResponse> {
        self.state.read().await.room_info.clone().map(Into::into)
    }

    /// 刷新贡献排行榜（手动调用 API 获取最新数据）
    pub async fn refresh_contribution_rank(
        &self,
        cookie: &str,
    ) -> Result<Vec<ContributionRankUser>, String> {
        let room_info = {
            let state = self.state.read().await;
            state.room_info.clone()
        };

        let room_info = match room_info {
            Some(info) => info,
            None => return Err("未连接房间".to_string()),
        };

        let http_client = reqwest::Client::new();
        match get_contribution_rank(
            &http_client,
            room_info.room_id,
            room_info.uid,
            Some(cookie),
            1,
            100,
        )
        .await
        {
            Ok(rank) => {
                let list = rank.list.clone();
                self.live_data
                    .lock()
                    .await
                    .set_contribution_rank_full(list.clone());
                Ok(list)
            }
            Err(e) => Err(format!("获取贡献排行榜失败: {}", e)),
        }
    }

    /// 订阅事件
    pub async fn subscribe(&self, window_label: String, event_types: HashSet<EventType>) {
        let mut subs = self.subscriptions.write().await;
        let sub = subs.entry(window_label.clone()).or_default();
        sub.event_types = event_types;
        log::info!(
            "Window {} subscribed to events: {:?}",
            window_label,
            sub.event_types
        );
    }

    /// 取消订阅
    pub async fn unsubscribe(&self, window_label: &str) {
        let mut subs = self.subscriptions.write().await;
        subs.remove(window_label);
        log::info!("Window {} unsubscribed", window_label);
    }

    /// 获取数据快照
    pub async fn get_snapshot(&self, event_types: HashSet<EventType>) -> DataSnapshot {
        let data = self.live_data.lock().await;
        data.snapshot(&event_types)
    }

    // ==================== 点播管理 ====================

    /// 异步获取视频信息（批量）
    async fn spawn_video_fetches(&self, to_fetch: Vec<(String, String, u64, Option<u64>)>) {
        for (request_id, video_id, _uid, _sc_price) in to_fetch {
            let live_data = self.live_data.clone();
            tokio::spawn(async move {
                let result = video_info::fetch_video_info(&video_id).await;
                let mut data = live_data.lock().await;
                data.update_video_request_info(&request_id, result);
            });
        }
    }

    /// 标记点播为已看
    pub async fn mark_video_watched(&self, request_id: &str, watched: bool) {
        let mut data = self.live_data.lock().await;
        data.set_video_watched(request_id, watched);
    }

    /// 删除点播请求
    pub async fn remove_video_request(&self, request_id: &str) {
        let mut data = self.live_data.lock().await;
        data.remove_video_request(request_id);
    }

    /// 清空已看
    pub async fn clear_watched_videos(&self) {
        let mut data = self.live_data.lock().await;
        data.clear_watched_videos();
    }

    /// 清空所有
    pub async fn clear_all_videos(&self) {
        let mut data = self.live_data.lock().await;
        data.clear_all_videos();
    }

    pub async fn connect(
        &self,
        app: AppHandle,
        room_id: u64,
        cookie: Option<String>,
    ) -> ConnectResult {
        // 检查 Cookie
        if cookie.is_none() || cookie.as_ref().map(|c| c.is_empty()).unwrap_or(true) {
            return ConnectResult {
                success: false,
                message: "请先设置 Cookie".to_string(),
                room_info: None,
            };
        }

        let cookie = cookie.unwrap();

        // 先断开现有连接
        self.disconnect().await;

        // 清空数据
        self.live_data.lock().await.clear();

        // 更新状态为连接中
        {
            let mut state = self.state.write().await;
            state.status = ConnectionStatus::Connecting;
            state.room_id = room_id;
        }

        let _ = app.emit("blive-status", ConnectionStatus::Connecting);

        // 预检查
        let http_client = reqwest::Client::new();

        let room_info = match get_room_init(&http_client, room_id).await {
            Ok(info) => info,
            Err(e) => {
                let msg = format!("获取房间信息失败: {}", e);
                self.set_error(&app, &msg).await;
                return ConnectResult {
                    success: false,
                    message: msg,
                    room_info: None,
                };
            }
        };

        let danmu_info = match get_danmu_info(&http_client, room_info.room_id, Some(&cookie)).await
        {
            Ok(info) => info,
            Err(e) => {
                let msg = format!("获取弹幕服务器失败: {}", e);
                self.set_error(&app, &msg).await;
                return ConnectResult {
                    success: false,
                    message: msg,
                    room_info: None,
                };
            }
        };

        if danmu_info.host_list.is_empty() || danmu_info.token.is_empty() {
            let msg = "Cookie 无效或已过期，无法获取弹幕服务器".to_string();
            self.set_error(&app, &msg).await;
            return ConnectResult {
                success: false,
                message: msg,
                room_info: None,
            };
        }

        {
            let mut state = self.state.write().await;
            state.room_info = Some(room_info.clone());
        }

        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);

        let app_clone = app.clone();
        let service = app.state::<Arc<BliveService>>().inner().clone();
        let cookie_clone = cookie.clone();
        let room_info_for_rank = room_info.clone();

        let task = tokio::spawn(async move {
            let client = match BliveDmClient::builder()
                .room_id(room_id)
                .cookie(cookie_clone.clone())
                .auto_reconnect(true)
                .build()
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    let msg = format!("创建客户端失败: {}", e);
                    service.set_error(&app_clone, &msg).await;
                    return;
                }
            };

            let mut stream = match client.connect().await {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!("连接失败: {}", e);
                    service.set_error(&app_clone, &msg).await;
                    return;
                }
            };

            {
                let mut state = service.state.write().await;
                state.status = ConnectionStatus::Connected;
            }
            let _ = app_clone.emit("blive-status", ConnectionStatus::Connected);

            // 启动存档会话
            let archive = app_clone.state::<Arc<ArchiveManager>>().inner().clone();
            let room_title = room_info_for_rank.title.clone();
            let streamer_uid = room_info_for_rank.uid;
            let archive_session_id = match archive
                .start_session(room_id, &room_title, streamer_uid)
                .await
            {
                Ok(id) => {
                    let (tx, rx) = mpsc::unbounded_channel::<ArchiveEvent>();
                    // 设置 archive_tx 到 LiveData
                    service.live_data.lock().await.archive_tx = Some(tx);
                    // 启动写入任务
                    let _writer_handle =
                        crate::archive::spawn_archive_writer(archive.clone(), rx, id);
                    Some(id)
                }
                Err(e) => {
                    log::error!("Failed to start archive session: {}", e);
                    None
                }
            };

            // 获取贡献排行榜（连接成功后立即获取一次）
            let http_client = reqwest::Client::new();
            match get_contribution_rank(
                &http_client,
                room_info_for_rank.room_id,
                room_info_for_rank.uid,
                Some(&cookie_clone),
                1,
                100,
            )
            .await
            {
                Ok(rank) => {
                    log::info!("获取贡献排行榜成功: {} 人", rank.list.len());
                    service
                        .live_data
                        .lock()
                        .await
                        .set_contribution_rank_full(rank.list);
                }
                Err(e) => {
                    log::warn!("获取贡献排行榜失败: {}", e);
                }
            }

            // 启动数据推送任务
            let app_for_push = app_clone.clone();
            let service_for_push = service.clone();
            let push_task = tokio::spawn(async move {
                let mut ticker = interval(DATA_PUSH_INTERVAL);
                loop {
                    ticker.tick().await;
                    service_for_push.push_updates(&app_for_push).await;
                }
            });

            // 事件处理循环
            loop {
                tokio::select! {
                    _ = stop_rx.recv() => {
                        log::info!("Received stop signal");
                        break;
                    }
                    event = stream.next() => {
                        match event {
                            Some(Ok(e)) => {
                                service.process_event(e).await;
                            }
                            Some(Err(e)) => {
                                log::error!("Event error: {}", e);
                                if matches!(e, BliveError::ConnectionClosed) {
                                    let mut state = service.state.write().await;
                                    state.status = ConnectionStatus::Reconnecting;
                                    let _ = app_clone.emit("blive-status", ConnectionStatus::Reconnecting);
                                }
                            }
                            None => {
                                log::info!("Stream ended");
                                break;
                            }
                        }
                    }
                }
            }

            push_task.abort();
            service.push_updates(&app_clone).await;

            // 结束存档会话：先 drop archive_tx 以让 writer flush，再 end_session
            {
                let stats = service.live_data.lock().await.stats.clone();
                service.live_data.lock().await.archive_tx = None; // drop sender, writer will flush & exit
                if archive_session_id.is_some() {
                    // 给 writer 一点时间 flush
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    if let Err(e) = archive.end_session(&stats).await {
                        log::error!("Failed to end archive session: {}", e);
                    }
                }
            }

            {
                let mut state = service.state.write().await;
                state.status = ConnectionStatus::Disconnected;
            }
            let _ = app_clone.emit("blive-status", ConnectionStatus::Disconnected);
        });

        {
            let mut state = self.state.write().await;
            state.stop_tx = Some(stop_tx);
            state.task_handle = Some(task);
            state.status = ConnectionStatus::Connected;
        }

        ConnectResult {
            success: true,
            message: "连接成功".to_string(),
            room_info: Some(room_info.into()),
        }
    }

    pub async fn disconnect(&self) {
        let (stop_tx, task_handle) = {
            let mut state = self.state.write().await;
            state.status = ConnectionStatus::Disconnected;
            (state.stop_tx.take(), state.task_handle.take())
        };

        if let Some(tx) = stop_tx {
            let _ = tx.send(()).await;
        }

        if let Some(handle) = task_handle {
            let _ = handle.await;
        }

        self.live_data.lock().await.clear();
    }

    /// 处理事件
    async fn process_event(&self, event: Event) {
        let mut data = self.live_data.lock().await;

        match event {
            Event::Danmaku(danmaku) => {
                let to_fetch = data.process_danmaku(danmaku);
                drop(data); // 释放锁后异步获取视频信息
                self.spawn_video_fetches(to_fetch).await;
            }
            Event::Gift(gift) => data.process_gift(gift),
            Event::SuperChat(sc) => {
                let to_fetch = data.process_superchat(sc);
                drop(data);
                self.spawn_video_fetches(to_fetch).await;
            }
            Event::GuardBuy(guard) => data.process_guard_buy(guard),
            Event::OnlineRankV2(rank) => data.process_online_rank(rank),
            Event::OnlineRankCount(count) => data.process_online_count(count),
            Event::LiveStart(live_data) => {
                log::info!(
                    "Live started: room_id={}, live_time={}",
                    live_data.room_id,
                    live_data.live_time
                );
                // 更新 room_info 的 live_status
                drop(data); // 先释放 live_data 锁，避免死锁
                {
                    let mut state = self.state.write().await;
                    if let Some(ref mut room_info) = state.room_info {
                        room_info.live_status = 1; // 1 = 直播中
                    }
                }
                self.live_data.lock().await.pending_updates.push(DataUpdate::LiveStart);
            }
            Event::LiveStop(preparing) => {
                log::info!(
                    "Live stopped: room_id={}, round={}",
                    preparing.room_id,
                    preparing.round
                );
                // 更新 room_info 的 live_status
                drop(data); // 先释放 live_data 锁，避免死锁
                {
                    let mut state = self.state.write().await;
                    if let Some(ref mut room_info) = state.room_info {
                        room_info.live_status = if preparing.round == 1 { 2 } else { 0 };
                        // 0 = 未开播, 2 = 轮播中
                    }
                }
                self.live_data.lock().await.pending_updates.push(DataUpdate::LiveStop);
            }
            Event::Raw { .. } => {} // 忽略未处理的命令
        }
    }

    /// 推送更新到前端（按窗口订阅过滤）
    async fn push_updates(&self, app: &AppHandle) {
        let updates = {
            let mut data = self.live_data.lock().await;
            data.take_pending_updates()
        };

        if updates.is_empty() {
            return;
        }

        let subs = self.subscriptions.read().await;

        // 如果没有订阅，不发送任何事件（前端必须先订阅）
        if subs.is_empty() {
            return;
        }

        // 按窗口订阅分发，使用带窗口标签的事件名，确保每个窗口只收到自己的事件
        for (window_label, sub) in subs.iter() {
            let filtered: Vec<_> = updates
                .iter()
                .filter(|u| sub.event_types.contains(&u.event_type()))
                .cloned()
                .collect();

            if !filtered.is_empty() {
                // 使用带窗口标签的事件名，前端监听对应的事件名
                let event_name = format!("blive-data:{}", window_label);
                let _ = app.emit(&event_name, &filtered);
            }
        }
    }

    async fn set_error(&self, app: &AppHandle, message: &str) {
        let status = ConnectionStatus::Error {
            message: message.to_string(),
        };
        {
            let mut state = self.state.write().await;
            state.status = status.clone();
        }
        let _ = app.emit("blive-status", status);
    }
}

impl Default for BliveService {
    fn default() -> Self {
        Self::new()
    }
}
