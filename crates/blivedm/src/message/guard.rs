//! 大航海（舰长/提督/总督）

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::GuardLevel;

/// 大航海购买消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardBuy {
    /// 用户 UID
    pub uid: u64,
    /// 用户名
    pub username: String,
    /// 舰队等级
    pub guard_level: GuardLevel,
    /// 购买数量（月数）
    pub num: u32,
    /// 价格（金瓜子）
    pub price: u64,
    /// 礼物 ID
    pub gift_id: u64,
    /// 礼物名称
    pub gift_name: String,
    /// 开始时间戳
    pub start_time: i64,
    /// 结束时间戳
    pub end_time: i64,
}

impl GuardBuy {
    /// 从 JSON 解析大航海购买消息
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.get("data")?;

        let uid = data.get("uid")?.as_u64()?;
        let username = data.get("username")?.as_str()?.to_string();
        let guard_level = data
            .get("guard_level")
            .and_then(|v| v.as_i64())
            .map(GuardLevel::from)
            .unwrap_or(GuardLevel::None);
        let num = data.get("num")?.as_u64()? as u32;
        let price = data.get("price")?.as_u64()?;
        let gift_id = data.get("gift_id")?.as_u64()?;
        let gift_name = data.get("gift_name")?.as_str()?.to_string();
        let start_time = data.get("start_time")?.as_i64()?;
        let end_time = data.get("end_time")?.as_i64()?;

        Some(GuardBuy {
            uid,
            username,
            guard_level,
            num,
            price,
            gift_id,
            gift_name,
            start_time,
            end_time,
        })
    }

    /// 获取舰队名称
    pub fn guard_name(&self) -> &'static str {
        match self.guard_level {
            GuardLevel::Governor => "总督",
            GuardLevel::Admiral => "提督",
            GuardLevel::Captain => "舰长",
            GuardLevel::None => "无",
        }
    }

    pub fn value_cny_fen(&self) -> u64 {
        self.price / 10
    }
}
