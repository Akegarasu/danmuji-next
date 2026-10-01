//! 直播聚合向扩展发布的最小事件，不包含窗口、连接或原始通知。
#[derive(Debug, Clone)]
pub struct ReceivedGift {
    pub event_id: Option<String>,
    pub sender_uid: u64,
    pub gift_id: u64,
    pub gift_name: String,
    pub sender_name: String,
    pub num: u32,
    /// 本条增量礼物的盲盒来源，数量沿用 num，不使用连击累计数量。
    pub blind_gift: Option<ReceivedBlindGift>,
}

#[derive(Debug, Clone)]
pub struct ReceivedBlindGift {
    pub gift_id: u64,
    pub gift_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSource {
    Danmaku,
    Superchat,
}

#[derive(Debug, Clone)]
pub struct ReceivedText {
    pub event_id: String,
    pub content: String,
    pub username: String,
    pub uid: u64,
    /// 本直播间的大航海等级：0 无，1 总督，2 提督，3 舰长。
    pub guard_level: u8,
    pub medal_anchor_uid: u64,
    pub medal_room_id: u64,
    pub timestamp: i64,
    pub source: TextSource,
    /// SC 金额，单位与直播统计一致（电池）。
    pub sc_price: Option<u64>,
}
