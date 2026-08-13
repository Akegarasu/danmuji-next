//! 弹幕消息

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{parse_bool_flag, GuardLevel, Medal, User};

/// 弹幕消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Danmaku {
    /// 弹幕内容
    pub content: String,
    /// 发送者
    pub sender: User,
    /// 时间戳（毫秒）
    pub timestamp: i64,
    /// 弹幕类型
    pub r#type: DanmakuType,
    /// 表情信息（如果是表情弹幕）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoticon: Option<Emoticon>,
    /// 弹幕颜色
    pub color: u32,
    /// 弹幕模式（1:滚动, 4:底部, 5:顶部）
    pub mode: u32,
}

/// 弹幕类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DanmakuType {
    /// 普通文字弹幕
    Text,
    /// 表情弹幕
    Emoticon,
}

impl From<i64> for DanmakuType {
    fn from(v: i64) -> Self {
        match v {
            1 => Self::Emoticon,
            _ => Self::Text,
        }
    }
}

/// 表情信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Emoticon {
    /// 表情唯一标识
    pub unique: String,
    /// 表情 URL
    pub url: String,
    /// 宽度
    pub width: u32,
    /// 高度
    pub height: u32,
}

impl Danmaku {
    /// 从 JSON 解析弹幕
    pub fn parse(value: &Value) -> Option<Self> {
        let info = value.get("info")?;

        // info 是一个数组
        // info[0]: 弹幕元数据数组
        // info[1]: 弹幕内容
        // info[2]: 用户信息数组
        // info[3]: 勋章信息数组
        // info[7]: 舰队等级

        let meta = info.get(0)?;
        let content = info.get(1)?.as_str()?.to_string();
        let user_info = info.get(2)?;
        let medal_info = info.get(3);
        let nested_user = meta
            .get(15)
            .and_then(|extra| extra.get("user"))
            .filter(|user| !user.is_null());
        let nested_medal = nested_user
            .and_then(|user| user.get("medal"))
            .filter(|medal| !medal.is_null());

        // 解析用户信息
        let uid = user_info.get(0)?.as_u64()?;
        let name = user_info.get(1)?.as_str()?.to_string();
        let is_admin = user_info.get(2)?.as_i64().unwrap_or(0) == 1;
        let face = nested_user
            .and_then(|user| user.get("base"))
            .and_then(|base| base.get("face"))
            .and_then(Value::as_str)
            .filter(|face| !face.is_empty())
            .map(String::from);
        let user_level = user_info
            .get(16)
            .and_then(|v| v.get(0))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        // 解析舰队等级
        let guard_level = info
            .get(7)
            .and_then(|v| v.as_i64())
            .map(GuardLevel::from)
            .unwrap_or(GuardLevel::None);

        // 解析勋章信息
        let medal = medal_info.and_then(|m| {
            let level = m.get(0)?.as_u64()? as u32;
            if level == 0 {
                return None;
            }
            Some(Medal {
                level,
                name: m.get(1)?.as_str()?.to_string(),
                anchor_name: m.get(2)?.as_str()?.to_string(),
                room_id: m.get(3)?.as_u64()?,
                color: m.get(4)?.as_u64()? as u32,
                anchor_uid: nested_medal
                    .and_then(|medal| medal.get("ruid"))
                    .and_then(Value::as_u64)
                    .filter(|uid| *uid != 0)
                    .or_else(|| m.get(12).and_then(Value::as_u64))
                    .unwrap_or(0),
                is_light: parse_bool_flag(
                    nested_medal.and_then(|medal| medal.get("is_light")),
                )
                .or_else(|| parse_bool_flag(m.get(11)))
                .unwrap_or(true),
            })
        });

        // 解析弹幕元数据
        let timestamp = meta.get(4)?.as_i64()?;
        let dm_type = meta
            .get(12)
            .and_then(|v| v.as_i64())
            .map(DanmakuType::from)
            .unwrap_or(DanmakuType::Text);
        let mode = meta.get(1).and_then(|v| v.as_u64()).unwrap_or(1) as u32;
        let color = meta.get(3).and_then(|v| v.as_u64()).unwrap_or(0xFFFFFF) as u32;

        // 解析表情信息
        // info[0][13] 直接是一个 JSON 对象，不需要二次解析
        let emoticon = if dm_type == DanmakuType::Emoticon {
            meta.get(13).and_then(|emo| {
                // emo 直接就是对象，不是字符串
                if emo.is_object() {
                    Some(Emoticon {
                        unique: emo.get("emoticon_unique")?.as_str()?.to_string(),
                        url: emo.get("url")?.as_str()?.to_string(),
                        width: emo.get("width")?.as_u64().unwrap_or(0) as u32,
                        height: emo.get("height")?.as_u64().unwrap_or(0) as u32,
                    })
                } else {
                    None
                }
            })
        } else {
            None
        };

        Some(Danmaku {
            content,
            sender: User {
                uid,
                name,
                face,
                medal,
                guard_level,
                user_level,
                is_admin,
            },
            timestamp,
            r#type: dm_type,
            emoticon,
            color,
            mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::Danmaku;

    fn medal_message(is_light: u8, anchor_uid: u64) -> Value {
        json!({
            "info": [
                [
                    0, 1, 25, 16_777_215, 1_785_657_743_032_i64, 0, 0, "", 0, 0, 0,
                    "", 0, {}, {},
                    {
                        "user": {
                            "base": {
                                "face": "https://i0.hdslb.com/bfs/face/test.jpg"
                            },
                            "medal": {
                                "is_light": is_light,
                                "ruid": anchor_uid
                            }
                        }
                    }
                ],
                "测试弹幕",
                [42, "测试用户", 0],
                [12, "测试牌", "测试主播", 23_151_928, 9_272_486, "", 0, 0, 0, 0, 0, 1, 999],
                [], [], 0, 0
            ]
        })
    }

    #[test]
    fn parses_named_medal_light_and_anchor_uid_fields() {
        let lit = Danmaku::parse(&medal_message(1, 398_629_298)).expect("lit medal danmaku");
        let lit_medal = lit.sender.medal.expect("lit medal");
        assert_eq!(
            lit.sender.face.as_deref(),
            Some("https://i0.hdslb.com/bfs/face/test.jpg")
        );
        assert!(lit_medal.is_light);
        assert_eq!(lit_medal.anchor_uid, 398_629_298);

        let unlit =
            Danmaku::parse(&medal_message(0, 13_548_043)).expect("unlit medal danmaku");
        let unlit_medal = unlit.sender.medal.expect("unlit medal");
        assert!(!unlit_medal.is_light);
        assert_eq!(unlit_medal.anchor_uid, 13_548_043);
    }
}
