//! 开播/下播事件解析

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 开播事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveStartData {
    pub live_key: String,
    pub live_platform: String,
    pub live_model: i32,
    pub live_time: i64,
    pub room_id: u64,
    pub sub_session_key: String,
}

impl LiveStartData {
    pub fn parse(value: &Value) -> Option<Self> {
        Some(Self {
            live_key: value.get("live_key")?.as_str().unwrap_or("").to_string(),
            live_platform: value
                .get("live_platform")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            live_model: value
                .get("live_model")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32,
            live_time: value
                .get("live_time")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            room_id: value
                .get("roomid")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            sub_session_key: value
                .get("sub_session_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }
}

/// 下播事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparingData {
    pub room_id: u64,
    pub round: i32,
}

impl PreparingData {
    pub fn parse(value: &Value) -> Option<Self> {
        // roomid 在 PREPARING 中可能是 string 或 number
        let room_id = value
            .get("roomid")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(0);

        Some(Self {
            room_id,
            round: value
                .get("round")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32,
        })
    }
}
