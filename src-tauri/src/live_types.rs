//! 直播数据公共类型
//!
//! 被 blive_service、live_data、archive 等模块共享的类型定义。

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::blivedm::api::ContributionRankUser;
use crate::blivedm::{GuardLevel, Medal, User};
use crate::video_info::VideoInfo;

// ==================== 常量 ====================

/// 数据推送间隔
pub const DATA_PUSH_INTERVAL: Duration = Duration::from_millis(100);

/// 最大列表长度（扩大缓存，10-20MB 可接受）
pub const MAX_DANMAKU_LIST: usize = 10000;
pub const MAX_GIFT_LIST: usize = 5000;
pub const MAX_SUPERCHAT_LIST: usize = 2000;

/// 礼物合并时间窗口（秒）
pub const GIFT_MERGE_WINDOW_SECS: i64 = 5;

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

// ==================== 房间信息 & 连接结果 ====================

/// 房间信息（发送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfoResponse {
    pub room_id: u64,
    pub short_id: u64,
    pub uid: u64,
    pub title: String,
    pub live_status: i32,
}

impl From<crate::blivedm::api::RoomInfo> for RoomInfoResponse {
    fn from(info: crate::blivedm::api::RoomInfo) -> Self {
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

// ==================== 点播相关 ====================

/// 点播请求（发送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoRequestItem {
    pub id: String,
    pub video_id: String,
    pub username: String,
    pub uid: u64,
    pub source: VideoRequestSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sc_price: Option<u64>,
    pub timestamp: i64,
    pub watched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_info: Option<VideoInfo>,
    pub loading: bool,
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

// ==================== 数据更新 ====================

/// 数据更新（发送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DataUpdate {
    DanmakuAppend(Vec<ProcessedDanmaku>),
    GiftUpsert(Vec<GiftUpsert>),
    SuperChatAppend(ProcessedSuperChat),
    ContributionRankLive(Vec<ProcessedOnlineRankUser>),
    ContributionRankFull(Vec<ContributionRankUser>),
    StatsUpdate(LiveStats),
    ContributionsUpdate(Vec<UserContribution>),
    LiveStart,
    LiveStop,
    VideoRequestAppend(VideoRequestItem),
    VideoRequestUpdate(VideoRequestItem),
    VideoRequestSync(Vec<VideoRequestItem>),
}

impl DataUpdate {
    /// 获取事件类型
    pub fn event_type(&self) -> EventType {
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

// ==================== 数据快照 ====================

/// 数据快照（用于新窗口同步）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub danmaku_list: Option<Vec<ProcessedDanmaku>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gift_list: Option<Vec<ProcessedGift>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superchat_list: Option<Vec<ProcessedSuperChat>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution_rank_live: Option<Vec<ProcessedOnlineRankUser>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution_rank_full: Option<Vec<ContributionRankUser>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributions: Option<Vec<UserContribution>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<LiveStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_requests: Option<Vec<VideoRequestItem>>,
}

// ==================== 辅助函数 ====================

pub fn convert_user(user: &User) -> ProcessedUser {
    ProcessedUser {
        uid: user.uid,
        name: user.name.clone(),
        face: user.face.clone(),
        medal: user.medal.as_ref().map(convert_medal),
        guard_level: guard_level_to_u8(&user.guard_level),
        is_admin: user.is_admin,
    }
}

pub fn convert_medal(medal: &Medal) -> ProcessedMedal {
    ProcessedMedal {
        name: medal.name.clone(),
        level: medal.level,
        color: format!("#{:06x}", medal.color),
    }
}

pub fn guard_level_to_u8(level: &GuardLevel) -> u8 {
    match level {
        GuardLevel::Governor => 1,
        GuardLevel::Admiral => 2,
        GuardLevel::Captain => 3,
        GuardLevel::None => 0,
    }
}
