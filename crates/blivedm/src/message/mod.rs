//! 消息类型定义

mod danmaku;
mod gift;
mod guard;
mod interact_word;
mod live_status;
mod online_rank_count;
mod online_rank_v2;
mod superchat;
mod user;

pub use danmaku::*;
pub use gift::*;
pub use guard::*;
pub use interact_word::*;
pub use live_status::*;
pub use online_rank_count::*;
pub use online_rank_v2::*;
pub use superchat::*;
pub use user::*;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::RawEventHandler;
use crate::packet::{Operation, Packet};

/// 所有事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
#[non_exhaustive]
pub enum Event {
    /// 弹幕消息
    Danmaku(Danmaku),
    /// 礼物
    Gift(Box<Gift>),
    /// 醒目留言
    SuperChat(SuperChat),
    /// 大航海（舰长/提督/总督）
    GuardBuy(GuardBuy),
    /// 开播
    LiveStart(LiveStartData),
    /// 下播
    LiveStop(PreparingData),
    /// 在线人数统计
    OnlineRankCount(OnlineRankCount),
    /// 高能用户排行榜
    OnlineRankV2(OnlineRankV2),
    /// 进入直播间
    InteractWord(InteractWord),
    /// 原始事件（未解析的 CMD）
    Raw { cmd: String, payload: Value },
}

/// 从数据包解析事件
pub(crate) fn parse_event(
    packet: &Packet,
    raw_event_handler: Option<&RawEventHandler>,
) -> Option<Event> {
    match packet.operation {
        Operation::HeartbeatReply => {
            // 心跳响应，忽略（人气值已废弃）
            None
        }
        Operation::Notification => {
            // 通知消息，body 是 JSON
            let json_str = std::str::from_utf8(&packet.body).ok()?;
            parse_notification(json_str, raw_event_handler)
        }
        Operation::EnterRoomReply => {
            // 进入房间响应，通常忽略
            None
        }
        _ => None,
    }
}

/// 解析通知消息
fn parse_notification(
    json_str: &str,
    raw_event_handler: Option<&RawEventHandler>,
) -> Option<Event> {
    let value: Value = serde_json::from_str(json_str).ok()?;

    if let Some(handler) = raw_event_handler {
        handler(&value);
    }

    let cmd = value.get("cmd")?.as_str()?;

    // 处理带参数的 CMD（例如 "DANMU_MSG:4:0:2:2:2:0"）
    let cmd_base = cmd.split(':').next().unwrap_or(cmd);

    match cmd_base {
        "DANMU_MSG" => {
            let danmaku = Danmaku::parse(&value)?;
            Some(Event::Danmaku(danmaku))
        }
        "SEND_GIFT" => {
            let gift = Gift::parse(&value)?;
            Some(Event::Gift(Box::new(gift)))
        }
        "SUPER_CHAT_MESSAGE" => {
            let superchat = SuperChat::parse(&value)?;
            Some(Event::SuperChat(superchat))
        }
        "GUARD_BUY" => {
            let guard = GuardBuy::parse(&value)?;
            Some(Event::GuardBuy(guard))
        }
        "LIVE" => {
            let data = LiveStartData::parse(&value)?;
            Some(Event::LiveStart(data))
        }
        "PREPARING" => {
            let data = PreparingData::parse(&value)?;
            Some(Event::LiveStop(data))
        }
        "ONLINE_RANK_COUNT" => {
            let online_rank_count = OnlineRankCount::parse(&value)?;
            Some(Event::OnlineRankCount(online_rank_count))
        }
        "ONLINE_RANK_V2" => {
            let online_rank_v2 = OnlineRankV2::parse(&value)?;
            Some(Event::OnlineRankV2(online_rank_v2))
        }
        "INTERACT_WORD" => {
            let iw = InteractWord::parse(&value)?;
            Some(Event::InteractWord(iw))
        }
        "INTERACT_WORD_V2" => {
            let iw = InteractWord::parse_v2(&value)?;
            Some(Event::InteractWord(iw))
        }
        // 其他已知但不处理的 CMD
        "ENTRY_EFFECT"
        | "COMBO_SEND"
        | "WATCHED_CHANGE"
        | "STOP_LIVE_ROOM_LIST"
        | "WIDGET_BANNER"
        | "HOT_RANK_CHANGED"
        | "HOT_RANK_CHANGED_V2"
        | "LIKE_INFO_V3_CLICK"
        | "LIKE_INFO_V3_UPDATE"
        | "COMMON_NOTICE_DANMAKU"
        | "ROOM_REAL_TIME_MESSAGE_UPDATE"
        | "POPULARITY_RED_POCKET_START"
        | "POPULARITY_RED_POCKET_WINNER_LIST" => None,
        // 未知 CMD，返回原始数据
        _ => Some(Event::Raw {
            cmd: cmd.to_string(),
            payload: value,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::packet::ProtocolVersion;

    #[test]
    fn invokes_raw_event_handler_before_parsing() {
        let calls = Arc::new(AtomicUsize::new(0));
        let handler_calls = Arc::clone(&calls);
        let handler = move |value: &Value| {
            assert_eq!(value["cmd"], "UNKNOWN_EVENT");
            handler_calls.fetch_add(1, Ordering::Relaxed);
        };
        let packet = Packet::new(
            ProtocolVersion::Plain,
            Operation::Notification,
            br#"{"cmd":"UNKNOWN_EVENT"}"#.to_vec(),
        );

        assert!(matches!(
            parse_event(&packet, Some(&handler)),
            Some(Event::Raw { .. })
        ));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
