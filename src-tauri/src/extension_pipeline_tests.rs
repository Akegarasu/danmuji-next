//! 扩展独立协议回归：状态版本、合并发布、持久化和异步回包。
use crate::{
    extensions::{EffectResult, ExtensionHost},
    live_data::LiveData,
    live_events::{ReceivedText, TextSource},
};
use serde_json::{json, Value};
use std::path::PathBuf;

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "danmuji-extension-test-{now}-{}",
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("extensions")).unwrap();
        Self(path)
    }
    fn host(&self) -> ExtensionHost {
        ExtensionHost::new(self.0.clone())
    }
    fn write(&self, name: &str, value: Value) {
        std::fs::write(
            self.0.join("extensions").join(name),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
    fn read(&self, name: &str) -> Value {
        serde_json::from_slice(&std::fs::read(self.0.join("extensions").join(name)).unwrap())
            .unwrap()
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn text(content: &str, uid: u64) -> ReceivedText {
    ReceivedText {
        content: content.into(),
        username: format!("用户{uid}"),
        uid,
        timestamp: 1_700_000_000,
        source: TextSource::Danmaku,
        sc_price: None,
    }
}
fn poll(host: &ExtensionHost, duration: Option<u64>) -> Value {
    host.request("voting", json!({"type":"create", "title":"选一个", "options":[["A","甲"],["B","乙"]], "key_type":"letter", "duration_secs":duration})).unwrap()
}
fn changes(host: &ExtensionHost, id: &str) -> Vec<Value> {
    host.take_changes()
        .into_iter()
        .filter(|state| state.extension_id == id)
        .map(|state| serde_json::to_value(state).unwrap())
        .collect()
}

#[test]
fn changes_coalesce_with_revisions_and_stale_completions_are_ignored() {
    let dir = TestDirectory::new();
    let host = dir.host();
    assert_eq!(host.state("video-request").unwrap().revision, 0);
    host.dispatch_text(&text("av1 AV1 av2", 42));
    assert_eq!(host.take_effects().len(), 2);
    let entries = host.state("video-request").unwrap().state;
    assert_ne!(entries[0]["id"], entries[1]["id"]);
    let old_id = entries[1]["id"].as_str().unwrap();
    host.request(
        "video-request",
        json!({"type":"remove", "request_id":old_id}),
    )
    .unwrap();
    host.dispatch_text(&text("av1", 42));
    let new_id = host.state("video-request").unwrap().state[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(old_id, new_id);
    let updates = changes(&host, "video-request");
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["extension_id"], "video-request");
    assert_eq!(updates[0]["revision"], 3);
    assert_eq!(updates[0]["state"].as_array().unwrap().len(), 2);
    assert!(host.take_changes().is_empty());
    host.complete_effect(
        "video-request",
        EffectResult::VideoInfo {
            request_id: old_id.into(),
            result: Err("过期回包".into()),
        },
    );
    assert!(host.take_changes().is_empty());
    assert_eq!(
        host.state("video-request").unwrap().state[0]["loading"],
        true
    );
    host.complete_effect(
        "video-request",
        EffectResult::VideoInfo {
            request_id: new_id,
            result: Err("获取失败".into()),
        },
    );
    let updates = changes(&host, "video-request");
    assert_eq!(updates[0]["revision"], 4);
    assert_eq!(updates[0]["state"][0]["loading"], false);
    assert_eq!(updates[0]["state"][0]["error"], "获取失败");
}

#[test]
fn checkpoints_restore_pending_jobs_and_dedup_without_kv_wrappers() {
    let dir = TestDirectory::new();
    let host = dir.host();
    host.dispatch_text(&text("av1 av2", 42));
    let entries = host.state("video-request").unwrap().state;
    host.request(
        "video-request",
        json!({"type":"mark_watched", "request_id":entries[0]["id"], "watched":true}),
    )
    .unwrap();
    let saved = dir.read("video-request.json");
    assert!(saved.is_array());
    assert_eq!(saved[0]["watched"], true);
    let restored = dir.host();
    assert_eq!(restored.state("video-request").unwrap().state, saved);
    assert_eq!(restored.take_effects().len(), 2);
    assert!(restored.take_effects().is_empty());
    restored.dispatch_text(&text("AV1", 999));
    assert!(restored.take_changes().is_empty());
    assert_eq!(
        restored
            .state("video-request")
            .unwrap()
            .state
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // 重复 ID 是无效检查点，不在读取时猜测或迁移数据。
    dir.write("video-request.json", json!([saved[0], saved[0]]));
    let invalid = dir.host();
    assert!(invalid.last_error().is_some());
    assert_eq!(invalid.state("video-request").unwrap().state, json!([]));
}

#[test]
fn voting_restores_indexes_and_uid_dedup_across_multiple_polls() {
    let dir = TestDirectory::new();
    let host = dir.host();
    let first = poll(&host, None);
    let second = poll(&host, None);
    assert_ne!(first["id"], second["id"]);
    host.dispatch_text(&text(" a ", 42));
    let mut sc = text("B", 43);
    sc.source = TextSource::Superchat;
    sc.sc_price = Some(300);
    host.dispatch_text(&sc);
    let restored = dir.host();
    restored.dispatch_text(&text("B", 42));
    assert!(restored.take_changes().is_empty());
    restored.dispatch_text(&text("B", 43));
    let updates = changes(&restored, "voting");
    assert_eq!(updates.len(), 1);
    for poll in updates[0]["state"].as_array().unwrap() {
        assert_eq!(poll["total_votes"], 2);
        assert!(poll["options"][0].get("voters").is_none());
        assert!(poll.get("voted_uids").is_none());
    }
    let voters = restored
        .query(
            "voting",
            json!({"type":"voters", "poll_id":first["id"], "option_key":"A"}),
        )
        .unwrap();
    assert_eq!(voters[0]["uid"], 42);
    assert!(restored.take_changes().is_empty());
    restored
        .request("voting", json!({"type":"delete", "poll_id":first["id"]}))
        .unwrap();
    restored.dispatch_text(&text("A", 44));
    assert_eq!(
        changes(&restored, "voting")[0]["state"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(dir.read("voting.json")[0]["total_votes"], 3);
}

#[test]
fn polls_expire_without_a_connection_and_reject_late_ballots() {
    let dir = TestDirectory::new();
    let host = dir.host();
    poll(&host, Some(0));
    host.take_changes();
    host.tick(false);
    let updates = changes(&host, "voting");
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["state"][0]["status"], "ended");
    host.tick(false);
    assert!(changes(&host, "voting").is_empty());
    poll(&host, Some(0));
    host.take_changes();
    host.dispatch_text(&text("A", 42));
    let updates = changes(&host, "voting");
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["state"][0]["total_votes"], 0);
    assert_eq!(updates[0]["state"][0]["status"], "ended");
    assert_eq!(
        dir.host().state("voting").unwrap().state[0]["status"],
        "ended"
    );
}

#[test]
fn live_protocol_and_clear_are_independent_of_extensions() {
    let dir = TestDirectory::new();
    let host = dir.host();
    host.dispatch_text(&text("av1", 42));
    poll(&host, None);
    let mut live = LiveData::default();
    live.clear();
    assert_eq!(
        host.state("video-request")
            .unwrap()
            .state
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        host.state("voting")
            .unwrap()
            .state
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        serde_json::to_value(live.snapshot(&Default::default())).unwrap(),
        json!({})
    );
    assert!(serde_json::from_value::<crate::live_types::EventType>(json!("voting")).is_err());
    assert!(
        serde_json::from_value::<crate::live_types::EventType>(json!("video_request")).is_err()
    );
}

#[test]
fn invalid_checkpoints_survive_background_events_and_bad_requests() {
    let dir = TestDirectory::new();
    let path = dir.0.join("extensions/video-request.json");
    std::fs::write(&path, "broken").unwrap();
    let host = dir.host();
    assert!(host.last_error().is_some());
    host.dispatch_text(&text("av1", 42));
    host.tick(true);
    assert!(host
        .request("video-request", json!({"type":"unknown"}))
        .is_err());
    host.tick(true);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken");
    host.request("video-request", json!({"type":"clear_all"}))
        .unwrap();
    assert_eq!(dir.read("video-request.json"), json!([]));
}

#[test]
fn invalid_poll_requests_do_not_publish_changes() {
    let dir = TestDirectory::new();
    let host = dir.host();
    for request in [
        json!({"type":"create","title":"错误","options":[["A","甲"]],"key_type":"letter","duration_secs":u64::MAX}),
        json!({"type":"create","title":"错误","options":[["A","甲"],["a","乙"]],"key_type":"letter"}),
        json!({"type":"create","title":"错误","options":[],"key_type":"letter"}),
    ] {
        assert!(host.request("voting", request).is_err());
    }
    assert_eq!(host.state("voting").unwrap().revision, 0);
    assert_eq!(host.state("voting").unwrap().state, json!([]));
    assert!(host.take_changes().is_empty());
}

#[test]
fn normalized_danmaku_and_sc_keep_live_updates_and_battery_units() {
    use blivedm::{Danmaku, DanmakuType, GuardLevel, SuperChat, User};
    let dir = TestDirectory::new();
    let host = dir.host();
    let mut live = LiveData::default();
    let danmaku = Danmaku {
        content: "av1".into(),
        timestamp: 1700000000123,
        r#type: DanmakuType::Text,
        emoticon: None,
        color: 0xffffff,
        mode: 1,
        sender: User {
            uid: 42,
            name: "用户42".into(),
            face: None,
            medal: None,
            guard_level: GuardLevel::None,
            user_level: 0,
            wealth_level: 0,
            is_admin: false,
        },
    };
    host.dispatch_text(&live.process_danmaku(danmaku));
    let sc = SuperChat::parse(&json!({"data":{"id":1,"message":"av2","price":30,"uid":43,"user_info":{"uname":"用户43"},"start_time":1700000000,"time":60}})).unwrap();
    host.dispatch_text(&live.process_superchat(sc));
    let requests = host.state("video-request").unwrap().state;
    assert_eq!(requests[0]["source"], "superchat");
    assert_eq!(requests[0]["sc_price"], 300);
    assert_eq!(requests[0]["timestamp"], 1700000000000i64);
    assert_eq!(requests[1]["source"], "danmaku");
    assert_eq!(requests[1]["timestamp"], 1700000000123i64);
    assert!(requests[1].get("sc_price").is_none());
    let updates = serde_json::to_value(live.take_pending_updates()).unwrap();
    let updates = updates.as_array().unwrap();
    assert!(updates.iter().any(|u| u["type"] == "DanmakuAppend"));
    assert!(updates
        .iter()
        .any(|u| u["type"] == "SuperChatAppend" && u["data"]["price"] == 300));
    assert!(updates.iter().all(|u| u["type"] != "VideoRequestAppend"));
    assert_eq!(changes(&host, "video-request").len(), 1);
    host.request(
        "video-request",
        json!({"type":"mark_watched","request_id":requests[0]["id"],"watched":true}),
    )
    .unwrap();
    host.request("video-request", json!({"type":"clear_watched"}))
        .unwrap();
    assert_eq!(host.take_effects().len(), 1);
    assert_eq!(
        host.state("video-request")
            .unwrap()
            .state
            .as_array()
            .unwrap()
            .len(),
        1
    );
    host.request("video-request", json!({"type":"clear_all"}))
        .unwrap();
    assert_eq!(host.state("video-request").unwrap().state, json!([]));
    host.dispatch_text(&text("av1 av2", 42));
    assert_eq!(host.take_effects().len(), 2);
}
