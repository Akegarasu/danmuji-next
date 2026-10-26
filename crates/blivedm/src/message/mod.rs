//! 消息类型定义

mod danmaku;
mod gift;
mod guard;
mod interact_word;
mod live_status;
mod online_rank_count;
mod online_rank_v2;
mod online_rank_v3;
mod send_gift_v2;
mod superchat;
mod user;

pub use danmaku::*;
pub use gift::*;
pub use guard::*;
pub use interact_word::*;
pub use live_status::*;
pub use online_rank_count::*;
pub use online_rank_v2::*;
pub use online_rank_v3::*;
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
    /// 同一条 V2 通知中的多个礼物结果（例如十连盲盒）
    GiftBatch(Vec<Gift>),
    /// 醒目留言
    SuperChat(SuperChat),
    /// 大航海原始购买通知；其中价格是标准标价
    GuardBuy(GuardBuy),
    /// 大航海成交 Toast；其中价格是本次订单总金额
    GuardToast(GuardToast),
    /// 开播
    LiveStart(LiveStartData),
    /// 下播
    LiveStop(PreparingData),
    /// 在线人数统计
    OnlineRankCount(OnlineRankCount),
    /// 高能用户排行榜
    OnlineRankV2(OnlineRankV2),
    /// 高能用户排行榜 V3
    OnlineRankV3(OnlineRankV3),
    /// 进入直播间
    InteractWord(InteractWord),
    /// 原始事件（未解析的 CMD）
    Raw { cmd: String, payload: Value },
}

/// 解析通知消息
pub fn parse_notification(
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
        "DANMU_MSG" | "DANMU_MSG_MIRROR" => {
            let mut danmaku = parse_required(cmd_base, Danmaku::parse(&value))?;
            danmaku.is_mirror = cmd_base == "DANMU_MSG_MIRROR";
            Ok(Event::Danmaku(danmaku))
        }
        "SEND_GIFT" => {
            let gift = parse_required(cmd_base, Gift::parse(&value))?;
            Ok(Event::Gift(Box::new(gift)))
        }
        "SEND_GIFT_V2" => {
            let mut gifts = parse_required(cmd_base, Gift::parse_v2(&value))?;
            if gifts.len() == 1 {
                Ok(Event::Gift(Box::new(gifts.pop().expect("one gift"))))
            } else {
                Ok(Event::GiftBatch(gifts))
            }
        }
        "SUPER_CHAT_MESSAGE" => {
            let superchat = parse_required(cmd_base, SuperChat::parse(&value))?;
            Ok(Event::SuperChat(superchat))
        }
        "GUARD_BUY" => {
            let guard = parse_required(cmd_base, GuardBuy::parse(&value))?;
            Ok(Event::GuardBuy(guard))
        }
        "USER_TOAST_MSG" | "USER_TOAST_MSG_V2" => {
            let toast = parse_required(cmd_base, GuardToast::parse(&value))?;
            Ok(Event::GuardToast(toast))
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
        "ONLINE_RANK_V3" => {
            let online_rank_v3 = parse_required(cmd_base, OnlineRankV3::parse(&value))?;
            Ok(Event::OnlineRankV3(online_rank_v3))
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

    // 基于 2026-10-26 的跨房 dump，替换用户身份并裁剪无关字段。
    fn mirror_messages() -> Vec<Value> {
        serde_json::from_str(include_str!("../../tests/fixtures/mirror_danmaku.json")).unwrap()
    }

    fn parse_danmaku(value: &Value) -> Danmaku {
        let Event::Danmaku(danmaku) =
            parse_notification(&serde_json::to_vec(value).unwrap(), None).unwrap()
        else {
            panic!("expected typed danmaku event");
        };
        danmaku
    }

    #[test]
    fn parses_mirror_notifications_with_and_without_medals() {
        let messages = mirror_messages();
        for message in &messages {
            let danmaku = parse_danmaku(message);
            assert!(danmaku.is_mirror);
            assert_eq!(danmaku.content, message["info"][1].as_str().unwrap());
            assert_eq!(danmaku.sender.uid, message["info"][2][0].as_u64().unwrap());
            assert_eq!(danmaku.timestamp, message["info"][0][4].as_i64().unwrap());
        }
        let with_medal = parse_danmaku(&messages[0]);
        assert_eq!(with_medal.sender.guard_level, GuardLevel::Captain);
        assert_eq!(with_medal.sender.wealth_level, 32);
        assert_eq!(with_medal.sender.medal.unwrap().room_id, 100);
        assert!(parse_danmaku(&messages[1]).sender.medal.is_none());
    }

    #[test]
    fn mirror_command_remains_authoritative_without_valid_extra() {
        let mut message = mirror_messages().remove(0);
        for cmd in ["DANMU_MSG_MIRROR", "DANMU_MSG_MIRROR:4:0:2:2:2:0"] {
            message["cmd"] = cmd.into();
            for extra in [
                Value::Null,
                "invalid json".into(),
                r#"{"is_mirror":false}"#.into(),
            ] {
                message["info"][0][15]["extra"] = extra;
                assert!(parse_danmaku(&message).is_mirror);
            }
        }
    }

    #[test]
    fn ordinary_command_ignores_mirror_flag_in_extra() {
        let mut message = mirror_messages().remove(0);
        for cmd in ["DANMU_MSG", "DANMU_MSG:4:0:2:2:2:0"] {
            message["cmd"] = cmd.into();
            for extra in [
                Value::Null,
                "invalid json".into(),
                r#"{"is_mirror":true}"#.into(),
                r#"{"is_mirror":false}"#.into(),
            ] {
                message["info"][0][15]["extra"] = extra;
                assert!(!parse_danmaku(&message).is_mirror);
            }
        }
    }

    #[test]
    fn mirror_emoticons_keep_the_existing_emoticon_payload() {
        let mut message = mirror_messages().remove(0);
        message["info"][0][12] = 1.into();
        message["info"][0][13] = serde_json::json!({
            "emoticon_unique": "test", "url": "https://example.com/emoticon.png", "width": 40, "height": 40
        });
        let danmaku = parse_danmaku(&message);
        assert!(danmaku.is_mirror);
        assert_eq!(danmaku.r#type, DanmakuType::Emoticon);
        assert_eq!(
            danmaku.emoticon.unwrap().url,
            "https://example.com/emoticon.png"
        );
    }

    #[test]
    fn parses_every_result_in_a_ten_draw_notification_and_observes_raw_once() {
        // 合成十连：三个结果共享支付流水，分别为 4、1、5 件。
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let handler = move |_: &Value| {
            observed.fetch_add(1, Ordering::Relaxed);
        };
        let event = parse_notification(
            include_bytes!("../../tests/fixtures/ten_blind_gift_v2.json"),
            Some(&handler),
        )
        .unwrap();
        let Event::GiftBatch(gifts) = event else {
            panic!("expected all V2 gift results");
        };
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            gifts
                .iter()
                .map(|gift| (gift.gift_id, gift.num))
                .collect::<Vec<_>>(),
            vec![(32128, 4), (32125, 1), (32126, 5)]
        );
        assert_eq!(
            gifts.iter().map(Gift::revealed_total_coin).sum::<u64>(),
            111_000
        );
        assert_eq!(
            gifts.iter().map(|gift| gift.total_coin).sum::<u64>(),
            150_000
        );
        assert!(gifts
            .iter()
            .all(|gift| gift.sender_uid == 42 && gift.blind_gift.is_some()));
    }
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

    #[test]
    fn parses_guard_toast_as_typed_event() {
        let raw = r#"{
            "cmd":"USER_TOAST_MSG_V2",
            "data":{
                "sender_uinfo":{"uid":42,"base":{"name":"tester","face":""}},
                "guard_info":{"guard_level":3,"role_name":"舰长","start_time":1700000000,"end_time":1700000000},
                "pay_info":{"num":1,"price":138000,"unit":"月","payflow_id":"guard-order"},
                "gift_info":{"gift_id":10003},
                "option":{"source":0}
            }
        }"#
        .as_bytes();

        assert!(matches!(
            parse_notification(raw, None).unwrap(),
            Event::GuardToast(toast)
                if toast.price == 138_000 && toast.payflow_id.as_deref() == Some("guard-order")
        ));
    }
}
