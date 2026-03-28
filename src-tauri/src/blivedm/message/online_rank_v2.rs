//! 直播间高能用户排行榜消息

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::GuardLevel;

/// 直播间高能用户排行榜
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineRankV2 {
    /// 排行类型（例如 "online_rank"）
    pub rank_type: String,
    /// 在线用户列表
    pub online_list: Vec<OnlineRankUser>,
}

/// 排行榜中的用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineRankUser {
    /// 用户 UID
    pub uid: u64,
    /// 用户名
    pub name: String,
    /// 头像 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
    /// 排名（1 开始）
    pub rank: u32,
    /// 贡献分数（文本形式，例如 "1000"）
    pub score: String,
    /// 舰队等级
    pub guard_level: GuardLevel,
}

impl OnlineRankV2 {
    /// 从 JSON 解析高能用户排行榜
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.get("data")?;

        let rank_type = data
            .get("rank_type")
            .and_then(|v| v.as_str())
            .unwrap_or("online_rank")
            .to_string();

        let online_list = data
            .get("online_list")?
            .as_array()?
            .iter()
            .filter_map(|item| OnlineRankUser::parse(item))
            .collect();

        Some(OnlineRankV2 {
            rank_type,
            online_list,
        })
    }
}

impl OnlineRankUser {
    /// 从 JSON 解析用户信息
    fn parse(value: &Value) -> Option<Self> {
        let uid = value.get("uid")?.as_u64()?;

        // 优先从 uinfo.base 获取用户名和头像（更完整）
        // 否则使用顶层字段
        let (name, face) = if let Some(uinfo) = value.get("uinfo") {
            if let Some(base) = uinfo.get("base") {
                let name = base
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let face = base.get("face").and_then(|v| v.as_str()).map(String::from);
                (name, face)
            } else {
                parse_top_level_user(value)
            }
        } else {
            parse_top_level_user(value)
        };

        // 如果用户名为空，使用顶层的 uname
        let name = if name.is_empty() {
            value
                .get("uname")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            name
        };

        let rank = value.get("rank")?.as_u64()? as u32;
        let score = value
            .get("score")
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .to_string();
        let guard_level = value
            .get("guard_level")
            .and_then(|v| v.as_i64())
            .map(GuardLevel::from)
            .unwrap_or(GuardLevel::None);

        Some(OnlineRankUser {
            uid,
            name,
            face,
            rank,
            score,
            guard_level,
        })
    }
}

/// 从顶层字段解析用户名和头像
fn parse_top_level_user(value: &Value) -> (String, Option<String>) {
    let name = value
        .get("uname")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let face = value.get("face").and_then(|v| v.as_str()).map(String::from);
    (name, face)
}
