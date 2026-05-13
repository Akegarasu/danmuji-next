//! 进入直播间消息

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GuardLevel, Medal, User};

/// 进入直播间消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractWord {
    /// 用户信息
    pub user: User,
    /// 时间戳（秒）
    pub timestamp: i64,
    /// 消息类型 (1=进入直播间)
    pub msg_type: u32,
}

impl InteractWord {
    /// 从 JSON 解析
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.get("data")?;

        let uid = data.get("uid")?.as_u64()?;
        let uname = data.get("uname")?.as_str()?.to_string();
        let timestamp = data.get("timestamp")?.as_i64()?;
        let msg_type = data.get("msg_type")?.as_u64().unwrap_or(1) as u32;

        // 头像
        let face = data
            .get("uinfo")
            .and_then(|u| u.get("base"))
            .and_then(|b| b.get("face"))
            .and_then(|f| f.as_str())
            .map(String::from);

        // 舰队等级
        let guard_level = data
            .get("uinfo")
            .and_then(|u| u.get("guard"))
            .and_then(|g| g.get("level"))
            .and_then(|l| l.as_i64())
            .map(GuardLevel::from)
            .unwrap_or(GuardLevel::None);

        // 粉丝勋章
        let medal = data.get("fans_medal").and_then(|fm| {
            let level = fm.get("medal_level")?.as_u64()? as u32;
            if level == 0 {
                return None;
            }
            Some(Medal {
                level,
                name: fm.get("medal_name")?.as_str()?.to_string(),
                color: fm.get("medal_color")?.as_u64()? as u32,
                room_id: fm.get("anchor_roomid")?.as_u64()?,
                anchor_uid: fm.get("target_id")?.as_u64().unwrap_or(0),
                anchor_name: String::new(),
            })
        });

        Some(InteractWord {
            user: User {
                uid,
                name: uname,
                face,
                medal,
                guard_level,
                user_level: 0,
                is_admin: false,
            },
            timestamp,
            msg_type,
        })
    }
}
