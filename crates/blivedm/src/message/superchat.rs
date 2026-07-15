//! 醒目留言（SuperChat）

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GuardLevel, Medal};

/// 醒目留言
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperChat {
    /// SC ID
    pub id: u64,
    /// 消息内容
    pub message: String,
    /// 价格（人民币元）
    pub price: u32,
    /// 发送者 UID
    pub sender_uid: u64,
    /// 发送者名称
    pub sender_name: String,
    /// 发送者头像
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_face: Option<String>,
    /// 开始时间戳
    pub start_time: i64,
    /// 结束时间戳
    pub end_time: i64,
    /// 持续时间（秒）
    pub duration: u32,
    /// 背景颜色
    pub background_color: String,
    /// 消息字体颜色
    pub message_font_color: String,
    /// 发送者勋章
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal: Option<Medal>,
    /// 舰队等级
    pub guard_level: GuardLevel,
    /// 用户等级
    pub user_level: u32,
}

impl SuperChat {
    /// 从 JSON 解析醒目留言
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.get("data")?;

        let id = data.get("id")?.as_u64()?;
        let message = data.get("message")?.as_str()?.to_string();
        let price = data.get("price")?.as_u64()? as u32;

        let sender_uid = data.get("uid")?.as_u64()?;

        let user_info = data.get("user_info")?;
        let sender_name = user_info.get("uname")?.as_str()?.to_string();
        let sender_face = user_info
            .get("face")
            .and_then(|v| v.as_str())
            .map(String::from);
        let user_level = user_info.get("user_level")?.as_u64()? as u32;
        let guard_level = user_info
            .get("guard_level")
            .and_then(|v| v.as_i64())
            .map(GuardLevel::from)
            .unwrap_or(GuardLevel::None);

        let start_time = data.get("start_time")?.as_i64()?;
        let end_time = data.get("end_time")?.as_i64()?;
        let duration = data.get("time")?.as_u64()? as u32;

        let background_color = data
            .get("background_color")
            .and_then(|v| v.as_str())
            .unwrap_or("#EDF5FF")
            .to_string();
        let message_font_color = data
            .get("message_font_color")
            .and_then(|v| v.as_str())
            .unwrap_or("#323232")
            .to_string();

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
                color: m
                    .get("medal_color")
                    .and_then(|v| {
                        // medal_color 可能是字符串或数字
                        v.as_u64().or_else(|| {
                            v.as_str().and_then(|s| {
                                u64::from_str_radix(s.trim_start_matches('#'), 16).ok()
                            })
                        })
                    })
                    .unwrap_or(0) as u32,
                anchor_uid: m.get("target_id")?.as_u64().unwrap_or(0),
            })
        });

        Some(SuperChat {
            id,
            message,
            price,
            sender_uid,
            sender_name,
            sender_face,
            start_time,
            end_time,
            duration,
            background_color,
            message_font_color,
            medal,
            guard_level,
            user_level,
        })
    }

    /// 价值（人民币分）
    pub fn value_cny_fen(&self) -> u32 {
        self.price / 10
    }
}
