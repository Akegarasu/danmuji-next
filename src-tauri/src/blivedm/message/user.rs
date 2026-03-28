//! 用户相关类型

use serde::{Deserialize, Serialize};

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// 用户 UID
    pub uid: u64,
    /// 用户名
    pub name: String,
    /// 头像 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
    /// 粉丝勋章
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal: Option<Medal>,
    /// 舰队等级
    pub guard_level: GuardLevel,
    /// 用户等级
    pub user_level: u32,
    /// 是否为房管
    pub is_admin: bool,
}

/// 粉丝勋章
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Medal {
    /// 勋章名称
    pub name: String,
    /// 勋章等级
    pub level: u32,
    /// 勋章颜色
    pub color: u32,
    /// 主播房间号
    pub room_id: u64,
    /// 主播 UID
    pub anchor_uid: u64,
    /// 主播名称
    pub anchor_name: String,
}

/// 舰队等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardLevel {
    /// 无
    None,
    /// 总督
    Governor,
    /// 提督
    Admiral,
    /// 舰长
    Captain,
}

impl From<i64> for GuardLevel {
    fn from(level: i64) -> Self {
        match level {
            1 => Self::Governor,
            2 => Self::Admiral,
            3 => Self::Captain,
            _ => Self::None,
        }
    }
}

impl From<GuardLevel> for i64 {
    fn from(level: GuardLevel) -> Self {
        match level {
            GuardLevel::None => 0,
            GuardLevel::Governor => 1,
            GuardLevel::Admiral => 2,
            GuardLevel::Captain => 3,
        }
    }
}
