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
        let message = data
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let price = u32::try_from(data.get("price")?.as_u64()?).ok()?;

        let sender_uid = data.get("uid")?.as_u64()?;

        let user_info = data.get("user_info");
        let sender_name = user_info
            .and_then(|info| info.get("uname"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let sender_face = user_info
            .and_then(|info| info.get("face"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let user_level = user_info
            .and_then(|info| info.get("user_level"))
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default();
        let guard_level = user_info
            .and_then(|info| info.get("guard_level"))
            .and_then(|v| v.as_i64())
            .map(GuardLevel::from)
            .unwrap_or(GuardLevel::None);

        let start_time = data.get("start_time")?.as_i64()?;
        let parsed_end_time = data.get("end_time").and_then(Value::as_i64);
        let parsed_duration = data
            .get("time")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok());
        let duration = parsed_duration
            .or_else(|| {
                parsed_end_time
                    .and_then(|end_time| end_time.checked_sub(start_time))
                    .and_then(|duration| u32::try_from(duration).ok())
            })
            .unwrap_or_default();
        let end_time =
            parsed_end_time.unwrap_or_else(|| start_time.saturating_add(i64::from(duration)));

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
            let level = u32::try_from(m.get("medal_level")?.as_u64()?).ok()?;
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
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or_default(),
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
        // `SUPER_CHAT_MESSAGE.data.price` 的单位是人民币元，而不是金瓜子。
        self.price.saturating_mul(100)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::SuperChat;

    fn minimal_super_chat() -> serde_json::Value {
        json!({
            "data": {
                "id": 123,
                "price": 30,
                "uid": 456,
                "start_time": 1_700_000_000
            }
        })
    }

    #[test]
    fn converts_upstream_yuan_price_to_cny_fen() {
        let raw = json!({
            "data": {
                "id": 123,
                "message": "hello",
                "price": 30,
                "uid": 456,
                "user_info": {
                    "uname": "tester",
                    "user_level": 1,
                    "guard_level": 0
                },
                "start_time": 1_700_000_000,
                "end_time": 1_700_000_060,
                "time": 60
            }
        });
        let super_chat = SuperChat::parse(&raw).expect("valid fixture");
        assert_eq!(super_chat.price, 30);
        assert_eq!(super_chat.value_cny_fen(), 3_000);
    }

    #[test]
    fn accepts_missing_presentation_and_expiry_fields() {
        let super_chat = SuperChat::parse(&minimal_super_chat())
            .expect("presentation and expiry fields are optional");

        assert_eq!(super_chat.message, "");
        assert_eq!(super_chat.sender_name, "");
        assert_eq!(super_chat.sender_face, None);
        assert_eq!(super_chat.user_level, 0);
        assert_eq!(super_chat.duration, 0);
        assert_eq!(super_chat.end_time, super_chat.start_time);
    }

    #[test]
    fn derives_missing_end_time_from_duration() {
        let mut raw = minimal_super_chat();
        raw["data"]["time"] = json!(60);

        let super_chat = SuperChat::parse(&raw).expect("duration is enough to derive expiry");
        assert_eq!(super_chat.duration, 60);
        assert_eq!(super_chat.end_time, super_chat.start_time + 60);
    }

    #[test]
    fn rejects_out_of_range_price_instead_of_truncating() {
        let mut raw = minimal_super_chat();
        raw["data"]["price"] = json!(u64::from(u32::MAX) + 1);

        assert!(SuperChat::parse(&raw).is_none());
    }

    #[test]
    fn defaults_out_of_range_noncritical_integers_instead_of_truncating() {
        let mut raw = minimal_super_chat();
        raw["data"]["time"] = json!(u64::from(u32::MAX) + 1);
        raw["data"]["user_info"] = json!({
            "user_level": u64::from(u32::MAX) + 1
        });

        let super_chat = SuperChat::parse(&raw).expect("noncritical metadata must not reject SC");
        assert_eq!(super_chat.duration, 0);
        assert_eq!(super_chat.end_time, super_chat.start_time);
        assert_eq!(super_chat.user_level, 0);
    }
}
