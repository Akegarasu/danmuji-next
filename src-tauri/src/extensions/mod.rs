//! 内置扩展领域接口。持久化、网络任务和桌面协议由各自的适配层负责。
mod host;
pub mod overtime;
pub mod server;
mod storage;
pub mod tasks;
pub mod video_info;
pub mod video_request;
pub mod voting;
pub use host::ExtensionHost;

use crate::live_events::{ReceivedGift, ReceivedText};
use serde_json::Value;
use std::time::Instant;
/// 桌面端统一状态消息；版本用于避免初始查询覆盖较新的推送。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtensionState {
    pub extension_id: String,
    pub revision: u64,
    pub state: Value,
}

#[derive(Debug)]
pub enum ExtensionEffect {
    FetchVideoInfo {
        request_id: String,
        video_id: String,
    },
}

pub enum EffectResult {
    VideoInfo {
        request_id: String,
        result: Result<video_info::VideoInfo, String>,
    },
}

pub trait Extension: Send {
    fn id(&self) -> &'static str;
    fn snapshot(&self, now: Instant) -> Value;
    fn checkpoint(&self, now: Instant) -> Value;
    fn restore(&mut self, value: Value, now: Instant) -> Result<(), String>;
    fn request(&mut self, request: Value, now: Instant) -> Result<Value, String>;
    fn query(&self, _query: Value) -> Result<Value, String> {
        Err("该扩展不支持此查询".into())
    }
    /// 默认不允许通过本机 HTTP 暴露桌面扩展数据。
    fn browser_visible(&self) -> bool {
        false
    }
    fn on_gift(&mut self, _gift: &ReceivedGift, _now: Instant) -> bool {
        false
    }
    fn on_text(&mut self, _text: &ReceivedText, _now: Instant) -> bool {
        false
    }
    fn tick(&mut self, _now: Instant) -> bool {
        false
    }
    fn take_effects(&mut self) -> Vec<ExtensionEffect> {
        Vec::new()
    }
    fn complete_effect(&mut self, _result: EffectResult) -> bool {
        false
    }
}

/// 避免同一条弹幕中的多个点播以及删除后重建的异步回包发生 ID 冲突。
fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{prefix}_{now}_{}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}
