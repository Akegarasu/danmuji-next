//! 弹幕消息

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GuardLevel, Medal, User};

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

        // 解析用户信息
        let uid = user_info.get(0)?.as_u64()?;
        let name = user_info.get(1)?.as_str()?.to_string();
        let is_admin = user_info.get(2)?.as_i64().unwrap_or(0) == 1;
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
                anchor_uid: m.get(12)?.as_u64().unwrap_or(0),
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
                face: None,
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
