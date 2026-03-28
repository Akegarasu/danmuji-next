//! 礼物消息

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GuardLevel, Medal};

/// 礼物消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gift {
    /// 礼物 ID
    pub gift_id: u64,
    /// 礼物名称
    pub gift_name: String,
    /// 礼物图片
    pub gift_icon: String,
    /// 数量
    pub num: u32,
    /// 单价（金瓜子/银瓜子）
    pub price: u32,
    /// 总价值
    pub total_coin: u64,
    /// 货币类型
    pub coin_type: CoinType,
    /// 发送者 UID
    pub sender_uid: u64,
    /// 发送者名称
    pub sender_name: String,
    /// 发送者头像
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_face: Option<String>,
    /// 动作（投喂等）
    pub action: String,
    /// 时间戳
    pub timestamp: i64,
    /// 发送者勋章
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal: Option<Medal>,
    /// 舰队等级
    pub guard_level: GuardLevel,
}

/// 货币类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoinType {
    /// 金瓜子（付费）
    Gold,
    /// 银瓜子（免费）
    Silver,
}

impl Gift {
    /// 从 JSON 解析礼物消息
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.get("data")?;

        let gift_id = data.get("giftId")?.as_u64()?;
        let gift_name = data.get("giftName")?.as_str()?.to_string();
        let gift_icon = data.get("gift_info")?.get("img_basic")?.as_str()?.to_string();
        let num = data.get("num")?.as_u64()? as u32;
        let price = data.get("price")?.as_u64().unwrap_or(0) as u32;
        let total_coin = data.get("total_coin")?.as_u64()?;
        let coin_type_str = data.get("coin_type")?.as_str()?;
        let coin_type = if coin_type_str == "gold" {
            CoinType::Gold
        } else {
            CoinType::Silver
        };

        let sender_uid = data.get("uid")?.as_u64()?;
        let sender_name = data.get("uname")?.as_str()?.to_string();
        let sender_face = data.get("face").and_then(|v| v.as_str()).map(String::from);
        let action = data
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("投喂")
            .to_string();
        let timestamp = data.get("timestamp")?.as_i64()?;

        let guard_level = data
            .get("guard_level")
            .and_then(|v| v.as_i64())
            .map(GuardLevel::from)
            .unwrap_or(GuardLevel::None);

        // 解析勋章信息
        let medal = data.get("medal_info").and_then(|m| {
            let level = m.get("medal_level")?.as_u64()? as u32;
            if level == 0 {
                return None;
            }
            Some(Medal {
                level,
                name: m.get("medal_name")?.as_str()?.to_string(),
                anchor_name: m.get("anchor_uname")?.as_str()?.to_string(),
                room_id: m.get("anchor_roomid")?.as_u64()?,
                color: m.get("medal_color")?.as_u64()? as u32,
                anchor_uid: m.get("target_id")?.as_u64().unwrap_or(0),
            })
        });

        Some(Gift {
            gift_id,
            gift_name,
            gift_icon,
            num,
            price,
            total_coin,
            coin_type,
            sender_uid,
            sender_name,
            sender_face,
            action,
            timestamp,
            medal,
            guard_level,
        })
    }

    /// 是否为付费礼物
    pub fn is_paid(&self) -> bool {
        self.coin_type == CoinType::Gold
    }

    /// 礼物价值（人民币，分）
    pub fn value_cny_fen(&self) -> u64 {
        if self.is_paid() {
            self.total_coin / 10
        } else {
            0
        }
    }
}
