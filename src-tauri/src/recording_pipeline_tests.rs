//! 录制链路的行为契约：协议输入、窗口更新、落盘、汇总与查询一致。
use std::sync::Arc;

use blivedm::parse_notification;
use serde_json::json;

use crate::archive::{ArchiveManager, Recording};
use crate::live_events::{LiveEvent, TextSource};
use crate::live_session::LiveSession;
use crate::live_types::DataUpdate;

fn parse(raw: serde_json::Value) -> blivedm::Event {
    parse_notification(&serde_json::to_vec(&raw).unwrap(), None).unwrap()
}

#[tokio::test]
async fn records_mixed_notifications_and_finalizes_queryable_totals() {
    let archive = Arc::new(ArchiveManager::new(":memory:".into()).unwrap());
    let recording = Recording::start(archive.clone(), 123, "录制契约", 900)
        .await
        .unwrap();
    let session_id = archive.get_sessions().await.unwrap()[0].id;
    let mut session = LiveSession::default();
    session.start_recording(recording);

    let gift_bytes = include_bytes!("../../crates/blivedm/tests/fixtures/ten_blind_gift_v2.json");
    for _ in 0..2 {
        session.process(parse_notification(gift_bytes, None).unwrap());
    }
    let sc = json!({"cmd":"SUPER_CHAT_MESSAGE","data":{
        "id":7,"message":"支持主播","price":30,"uid":43,
        "user_info":{"uname":"用户43"},"start_time":1700000000,"time":60
    }});
    session.process(parse_notification(&serde_json::to_vec(&sc).unwrap(), None).unwrap());
    let updates = session.data.take_pending_updates();
    assert!(updates
        .iter()
        .any(|update| matches!(update, DataUpdate::GiftUpsert(items) if items.len() == 3)));
    assert!(updates
        .iter()
        .any(|update| matches!(update, DataUpdate::SuperChatAppend(sc) if sc.price == 300)));
    assert!(session.data.take_pending_updates().is_empty());

    session.finish_recording().await.unwrap();
    let saved = archive.get_session_detail(session_id).await.unwrap();
    assert!(saved.end_time.is_some());
    assert_eq!((saved.gift_count, saved.sc_count), (3, 1));
    assert_eq!(
        (saved.gift_revenue, saved.sc_revenue, saved.total_revenue),
        (1110, 300, 1410)
    );
    let results = archive
        .search(Some(123), Some(session_id), "", "all", None, None, 1, 100)
        .await
        .unwrap();
    assert_eq!(results.total, 4);
    assert_eq!(
        results
            .items
            .iter()
            .filter(|item| item.event_type == "gift")
            .map(|item| item.quantity.unwrap())
            .sum::<u32>(),
        10
    );
    assert_eq!(
        archive
            .get_overview(None, None, "")
            .await
            .unwrap()
            .summary
            .total_revenue,
        1410
    );
    assert!(archive.delete_session(session_id).await.is_ok());
}

#[tokio::test]
async fn recording_protects_active_sessions_and_preserves_empty_session_cleanup() {
    let archive = Arc::new(ArchiveManager::new(":memory:".into()).unwrap());
    let recording = Recording::start(archive.clone(), 123, "空会话", 900)
        .await
        .unwrap();
    let session_id = archive.get_sessions().await.unwrap()[0].id;
    assert_eq!(
        archive.delete_session(session_id).await.unwrap_err(),
        "直播进行中，不能删除当前场次"
    );
    assert_eq!(archive.prune_empty_sessions().await.unwrap(), 0);
    recording.finish(Default::default()).await.unwrap();
    assert_eq!(archive.prune_empty_sessions().await.unwrap(), 1);
}

#[tokio::test]
async fn session_cutoff_keeps_notifications_and_windows_independent_of_recording() {
    let archive = Arc::new(ArchiveManager::new(":memory:".into()).unwrap());
    let mut session = LiveSession::default();
    session.start_recording(
        Recording::start(archive.clone(), 1, "第一场", 900)
            .await
            .unwrap(),
    );
    let id = archive.get_sessions().await.unwrap()[0].id;
    let danmaku = || {
        parse(json!({"cmd":"DANMU_MSG","info":[
            [0,1,25,16777215,1700000000123i64], "点歌 测试歌曲", [42,"观众",0], []
        ]}))
    };
    let events = session.process(danmaku());
    assert!(
        matches!(&events[..], [LiveEvent::Text(text)] if text.source == TextSource::Danmaku && text.timestamp == 1700000000123i64)
    );
    let toast = || {
        parse(json!({"cmd":"USER_TOAST_MSG","data":{
            "uid":42,"username":"观众","guard_level":3,"num":3,"price":414000,
            "gift_id":10003,"role_name":"舰长","payflow_id":"order-1","start_time":1700000000
        }}))
    };
    assert!(matches!(&session.process(toast())[..], [LiveEvent::Gift(gift)] if gift.num == 3));
    assert!(session.process(toast()).is_empty());
    assert!(session
        .process(parse(json!({"cmd":"GUARD_BUY","data":{
            "uid":42,"guard_level":3,"num":3,"price":198000,"gift_id":10003,"start_time":1700000000
        }})))
        .is_empty());
    let finishing = session.finish_recording();
    assert!(matches!(
        &session.process(danmaku())[..],
        [LiveEvent::Text(_)]
    ));
    finishing.await.unwrap();
    let saved = archive.get_session_detail(id).await.unwrap();
    assert_eq!(
        (saved.danmaku_count, saved.gift_count, saved.guard_revenue),
        (1, 1, 4140)
    );
    let updates = session.data.take_pending_updates();
    assert!(updates
        .iter()
        .any(|event| matches!(event, DataUpdate::DanmakuAppend(items) if items.len() == 2)));
    assert!(updates
        .iter()
        .any(|event| matches!(event, DataUpdate::GiftUpsert(items) if items.len() == 1)));

    session.data.clear();
    session.start_recording(
        Recording::start(archive.clone(), 2, "第二场", 901)
            .await
            .unwrap(),
    );
    // 切换会话会重置去重范围，旧订单可以属于新的录制。
    assert_eq!(session.process(toast()).len(), 1);
    session.finish_recording().await.unwrap();
    let second = archive
        .get_sessions()
        .await
        .unwrap()
        .into_iter()
        .find(|session| session.room_id == 2)
        .unwrap();
    assert_eq!(
        (
            second.danmaku_count,
            second.gift_count,
            second.guard_revenue
        ),
        (0, 1, 4140)
    );
}
