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
use crate::error::{Error, Result};

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

/// 解析通知消息
pub(crate) fn parse_notification(
    body: &[u8],
    raw_event_handler: Option<&RawEventHandler>,
) -> Result<Event> {
    let value: Value = serde_json::from_slice(body)?;

    if let Some(handler) = raw_event_handler {
        // 第三方观察回调不应让协议读取任务因 panic 退出。
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(&value))).is_err() {
            log::error!("raw_event_handler panicked; ignoring callback failure");
        }
    }

    let cmd = value
        .get("cmd")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::PacketParse("notification has no string cmd".to_string()))?;

    // 处理带参数的 CMD（例如 "DANMU_MSG:4:0:2:2:2:0"）
    let cmd_base = cmd.split(':').next().unwrap_or(cmd);

    match cmd_base {
        "DANMU_MSG" => {
            let danmaku = parse_required(cmd_base, Danmaku::parse(&value))?;
            Ok(Event::Danmaku(danmaku))
        }
        "SEND_GIFT" => {
            let gift = parse_required(cmd_base, Gift::parse(&value))?;
            Ok(Event::Gift(Box::new(gift)))
        }
        "SUPER_CHAT_MESSAGE" => {
            let superchat = parse_required(cmd_base, SuperChat::parse(&value))?;
            Ok(Event::SuperChat(superchat))
        }
        "GUARD_BUY" => {
            let guard = parse_required(cmd_base, GuardBuy::parse(&value))?;
            Ok(Event::GuardBuy(guard))
        }
        "LIVE" => {
            let data = parse_required(cmd_base, LiveStartData::parse(&value))?;
            Ok(Event::LiveStart(data))
        }
        "PREPARING" => {
            let data = parse_required(cmd_base, PreparingData::parse(&value))?;
            Ok(Event::LiveStop(data))
        }
        "ONLINE_RANK_COUNT" => {
            let online_rank_count = parse_required(cmd_base, OnlineRankCount::parse(&value))?;
            Ok(Event::OnlineRankCount(online_rank_count))
        }
        "ONLINE_RANK_V2" => {
            let online_rank_v2 = parse_required(cmd_base, OnlineRankV2::parse(&value))?;
            Ok(Event::OnlineRankV2(online_rank_v2))
        }
        "INTERACT_WORD" => {
            let iw = parse_required(cmd_base, InteractWord::parse(&value))?;
            Ok(Event::InteractWord(iw))
        }
        "INTERACT_WORD_V2" => {
            let iw = parse_required(cmd_base, InteractWord::parse_v2(&value))?;
            Ok(Event::InteractWord(iw))
        }
        // 已知但尚未建模的 CMD 也必须保留为 Raw 事件。调用方可能依赖其中
        // 的 UID/点赞/入场信息；静默 Ok(None) 会让全局监控产生永久盲区。
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
        | "POPULARITY_RED_POCKET_WINNER_LIST" => Ok(Event::Raw {
            cmd: cmd.to_string(),
            payload: value,
        }),
        // 未知 CMD，返回原始数据
        _ => Ok(Event::Raw {
            cmd: cmd.to_string(),
            payload: value,
        }),
    }
}

fn parse_required<T>(cmd: &str, value: Option<T>) -> Result<T> {
    value.ok_or_else(|| Error::PacketParse(format!("failed to parse notification {cmd}")))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;
    #[test]
    fn invokes_raw_event_handler_before_parsing() {
        let calls = Arc::new(AtomicUsize::new(0));
        let handler_calls = Arc::clone(&calls);
        let handler = move |value: &Value| {
            assert_eq!(value["cmd"], "UNKNOWN_EVENT");
            handler_calls.fetch_add(1, Ordering::Relaxed);
        };
        assert!(matches!(
            parse_notification(br#"{"cmd":"UNKNOWN_EVENT"}"#, Some(&handler)).unwrap(),
            Event::Raw { .. }
        ));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn preserves_known_but_unmodelled_notifications_as_raw_events() {
        assert!(matches!(
            parse_notification(
                br#"{"cmd":"ENTRY_EFFECT","data":{"uid":42}}"#,
                None
            )
            .unwrap(),
            Event::Raw { cmd, payload }
                if cmd == "ENTRY_EFFECT" && payload["data"]["uid"] == 42
        ));
    }
}
