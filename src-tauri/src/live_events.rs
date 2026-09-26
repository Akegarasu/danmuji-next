//! 直播聚合向扩展发布的最小事件，不包含窗口、连接或原始通知。
#[derive(Debug, Clone)]
pub struct ReceivedGift {
    pub gift_id: u64,
    pub gift_name: String,
    pub sender_name: String,
    pub num: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSource {
    Danmaku,
    Superchat,
}

#[derive(Debug, Clone)]
pub struct ReceivedText {
    pub content: String,
    pub username: String,
    pub uid: u64,
    pub timestamp: i64,
    pub source: TextSource,
    /// SC 金额，单位与直播统计一致（电池）。
    pub sc_price: Option<u64>,
}
