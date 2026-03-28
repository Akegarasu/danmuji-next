//! 直播间在线人数消息

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 直播间在线人数统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineRankCount {
    /// 高能用户数量
    pub count: u32,
    /// 高能用户数量文本（例如 "100+"）
    pub count_text: String,
    /// 在线人数
    pub online_count: u32,
    /// 在线人数文本（例如 "1万+"）
    pub online_count_text: String,
}

impl OnlineRankCount {
    /// 从 JSON 解析在线人数消息
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.get("data")?;

        let count = data.get("count")?.as_u64()? as u32;
        let count_text = data.get("count_text")?.as_str()?.to_string();
        let online_count = data.get("online_count")?.as_u64()? as u32;
        let online_count_text = data.get("online_count_text")?.as_str()?.to_string();

        Some(OnlineRankCount {
            count,
            count_text,
            online_count,
            online_count_text,
        })
    }
}
