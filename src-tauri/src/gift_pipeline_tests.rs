//! 礼物管线回归：原始通知 → 解析 → 连击合并 → 推送与 SQLite。
//! 除显式读取本地 dump 的调查用例外，输入均为构造数据。

use blivedm::{parse_notification, Event, Gift};
use serde_json::{json, Value};

use crate::archive::{ArchiveEvent, ArchiveManager};
use crate::live_data::LiveData;
use crate::live_types::{DataUpdate, ProcessedGift, UpsertAction, MAX_GIFT_LIST};

const ITEMS: [(u64, &str, u64); 3] = [
    (32128, "爱心抱枕", 16000),
    (32125, "电影票", 2000),
    (32126, "棉花糖", 9000),
];
const ROUNDS: [[u32; 3]; 3] = [[4, 1, 5], [6, 2, 2], [1, 5, 4]];

fn v1_packet(round: usize, item: usize, same_combo: bool) -> Value {
    let (gift_id, gift_name, unit_coin) = ITEMS[item];
    let num = ROUNDS[round][item];
    let batch = if same_combo { 0 } else { round };
    let cumulative: u32 = if same_combo {
        ROUNDS[..=round].iter().map(|counts| counts[item]).sum()
    } else {
        num
    };
    json!({"cmd": "SEND_GIFT", "data": {
        "giftId": gift_id, "giftName": gift_name, "num": num,
        "price": unit_coin, "total_coin": 15000 * u64::from(num),
        "coin_type": "gold", "uid": 42, "uname": "测试用户",
        "timestamp": 1700000000 + round * 30 + item,
        "tid": format!("payment-{round}"), "batch_combo_id": format!("combo-{batch}"),
        "combo_total_coin": unit_coin * u64::from(cumulative),
        "super_batch_gift_num": (round + 1) * 10, "combo_stay_time": 5,
        "batch_combo_send": {
            "action": "投喂", "batch_combo_id": format!("combo-{batch}"),
            "batch_combo_num": (round + 1) * 10,
            "gift_id": gift_id, "gift_name": gift_name, "gift_num": num,
            "uid": 42, "uname": "测试用户"
        },
        "blind_gift": {
            "blind_gift_config_id": 139, "from": 0, "gift_action": "爆出",
            "gift_tip_price": unit_coin, "original_gift_id": 32251,
            "original_gift_name": "心动盲盒", "original_gift_price": 15000
        }
    }})
}

fn parse(raw: &Value) -> Vec<Gift> {
    match parse_notification(&serde_json::to_vec(raw).unwrap(), None).unwrap() {
        Event::Gift(gift) => vec![*gift],
        Event::GiftBatch(gifts) => gifts,
        event => panic!("expected gifts, got {event:?}"),
    }
}

fn feed(data: &mut LiveData, raw: &Value) {
    for gift in parse(raw) {
        data.process_gift(gift);
    }
}

fn v1_from_gift(gift: &Gift) -> Value {
    json!({"cmd": "SEND_GIFT", "data": {
        "giftId": gift.gift_id, "giftName": gift.gift_name, "num": gift.num,
        "price": gift.price, "total_coin": gift.total_coin, "coin_type": gift.coin_type,
        "uid": gift.sender_uid, "uname": gift.sender_name, "timestamp": gift.timestamp,
        "tid": gift.transaction_id, "batch_combo_id": gift.batch_combo_id,
        "combo_total_coin": gift.combo_total_coin,
        "super_batch_gift_num": gift.super_batch_gift_num,
        "blind_gift": gift.blind_gift
    }})
}

fn assert_totals(data: &LiveData, rows: usize, quantity: u32, value: u64, cost: u64) {
    assert_eq!(data.gift_list.len(), rows);
    assert_eq!(
        data.gift_list.iter().map(|gift| gift.num).sum::<u32>(),
        quantity
    );
    assert_eq!(
        data.gift_list
            .iter()
            .map(|gift| gift.total_value)
            .sum::<u64>(),
        value
    );
    assert_eq!(data.stats.gift_revenue, value);
    assert_eq!(data.stats.total_revenue, value);
    assert_eq!(
        data.gift_list
            .iter()
            .map(|gift| gift
                .blind_gift
                .as_ref()
                .map_or(0, |blind| blind.total_value))
            .sum::<u64>(),
        cost
    );
}

fn summary(gift: &ProcessedGift) -> (u64, u32, u64, u64) {
    (
        gift.gift_id,
        gift.num,
        gift.total_value,
        gift.blind_gift
            .as_ref()
            .map_or(0, |blind| blind.total_value),
    )
}

#[tokio::test]
async fn v1_three_ten_draws_keep_counts_in_updates_and_sqlite() {
    let mut data = LiveData::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    data.archive_tx = Some(tx);
    let mut update_batches = Vec::new();
    for round in 0..3 {
        for item in 0..3 {
            let raw = v1_packet(round, item, true);
            feed(&mut data, &raw);
            feed(&mut data, &raw); // 原包重放不得重复入账。
        }
        update_batches.push(data.take_pending_updates());
    }
    assert_totals(&data, 3, 30, 2910, 4500);
    assert_eq!(
        data.gift_list.iter().map(summary).collect::<Vec<_>>(),
        [
            (32128, 11, 1760, 1650),
            (32125, 8, 160, 1200),
            (32126, 11, 990, 1650)
        ]
    );
    let upserts: Vec<_> = update_batches
        .iter()
        .flatten()
        .filter_map(|update| {
            if let DataUpdate::GiftUpsert(gifts) = update {
                Some(gifts)
            } else {
                None
            }
        })
        .flatten()
        .collect();
    assert_eq!(upserts.len(), 9);
    assert_eq!(
        upserts
            .iter()
            .filter(|upsert| matches!(upsert.action, UpsertAction::Insert))
            .count(),
        3
    );
    assert_eq!(
        upserts
            .iter()
            .filter(|upsert| matches!(upsert.action, UpsertAction::Update))
            .count(),
        6
    );
    let contribution = update_batches.last().unwrap().iter().find_map(|update| {
        if let DataUpdate::ContributionsUpdate(users) = update {
            Some(users[0].total_value)
        } else {
            None
        }
    });
    assert_eq!(contribution, Some(2910));

    let archive = ArchiveManager::new(":memory:".into()).unwrap();
    let session_id = archive.start_session(1, "礼物回归", 7).await.unwrap();
    let mut writes = 0;
    while let Ok(ArchiveEvent::Gift(gift)) = rx.try_recv() {
        archive.save_gift(session_id, &gift).await.unwrap();
        writes += 1;
    }
    assert_eq!(writes, 9);
    let saved = archive
        .search_gifts(session_id, "", None, None, 1, 100)
        .await
        .unwrap();
    assert_eq!(saved.total, 3);
    assert_eq!(saved.items.iter().map(|gift| gift.num).sum::<u32>(), 30);
    assert_eq!(
        saved.items.iter().map(|gift| gift.total_value).sum::<u64>(),
        2910
    );
    assert_eq!(
        saved
            .items
            .iter()
            .map(|gift| gift.revenue_value)
            .sum::<u64>(),
        2910
    );
    assert_eq!(
        saved
            .items
            .iter()
            .map(|gift| gift.blind_gift.as_ref().unwrap().total_value)
            .sum::<u64>(),
        4500
    );
    archive.end_session(&data.stats).await.unwrap();
    let session = archive.get_session_detail(session_id).await.unwrap();
    assert_eq!(session.gift_count, 3); // 会话统计为合并后的行数，数量另见每行 num。
    assert_eq!(session.gift_revenue, 2910);
    // 供离线前端探针消费实际后端更新；正常 cargo test 不显示此输出。
    println!(
        "GIFT_COMBO_UPDATES={}",
        serde_json::to_string(&update_batches).unwrap()
    );
}

#[test]
fn v1_new_combo_ids_keep_rounds_separate() {
    let mut data = LiveData::default();
    for round in 0..3 {
        for item in 0..3 {
            feed(&mut data, &v1_packet(round, item, false));
        }
    }
    assert_totals(&data, 9, 30, 2910, 4500);
}

#[test]
fn v1_two_rounds_of_interleaved_single_draws_merge_by_result() {
    let mut data = LiveData::default();
    let mut counts = [0u64; 3];
    let order = [0, 2, 0, 1, 2, 0, 2, 0, 2, 2];
    for round in 0..2 {
        for (sequence, item) in order.into_iter().enumerate() {
            counts[item] += 1;
            let mut raw = v1_packet(0, item, true);
            raw["data"]["num"] = json!(1);
            raw["data"]["total_coin"] = json!(15000);
            raw["data"]["tid"] = json!(format!("single-{round}-{sequence}"));
            raw["data"]["timestamp"] = json!(1700000000 + round * 30 + sequence);
            raw["data"]["combo_total_coin"] = json!(ITEMS[item].2 * counts[item]);
            raw["data"]["super_batch_gift_num"] = json!(round * 10 + sequence + 1);
            feed(&mut data, &raw);
            feed(&mut data, &raw);
        }
    }
    assert_totals(&data, 3, 20, 2220, 3000);
    for (item, expected) in [(0, 8), (1, 2), (2, 10)] {
        let gift = data
            .gift_list
            .iter()
            .find(|gift| gift.gift_id == ITEMS[item].0)
            .unwrap();
        assert_eq!(gift.num, expected);
    }
}

#[test]
fn v1_regular_discounted_and_free_combos_use_each_payment_total() {
    let mut data = LiveData::default();
    for (sequence, num) in [2, 3, 4].into_iter().enumerate() {
        for currency in ["gold", "silver"] {
            let raw = json!({"cmd": "SEND_GIFT", "data": {
                "giftId": 1, "giftName": "普通礼物", "num": num, "price": 1000,
                "total_coin": 800 * num, "coin_type": currency, "uid": 42,
                "uname": "测试用户", "timestamp": 1700000000 + sequence,
                "tid": format!("{currency}-{sequence}"),
                "batch_combo_id": format!("{currency}-combo"), "blind_gift": null
            }});
            feed(&mut data, &raw);
            feed(&mut data, &raw);
        }
    }
    assert_totals(&data, 2, 18, 72, 0);
    let free = data.gift_list.iter().find(|gift| !gift.is_paid).unwrap();
    assert_eq!((free.num, free.total_value, free.revenue_value), (9, 0, 0));
    let paid = data.gift_list.iter().find(|gift| gift.is_paid).unwrap();
    assert_eq!(
        (paid.num, paid.total_value, paid.revenue_value),
        (9, 72, 72)
    );
}

#[test]
fn v1_interleaved_users_do_not_share_combo_or_transaction_state() {
    let mut data = LiveData::default();
    for round in 0..3 {
        for item in 0..3 {
            for uid in [42, 43] {
                let mut raw = v1_packet(round, item, true);
                raw["data"]["uid"] = json!(uid);
                feed(&mut data, &raw);
            }
        }
    }
    assert_totals(&data, 6, 60, 5820, 9000);
    for uid in [42, 43] {
        assert_eq!(
            data.gift_list
                .iter()
                .filter(|gift| gift.user.uid == uid)
                .map(|gift| gift.num)
                .sum::<u32>(),
            30
        );
    }
}

#[test]
fn v1_without_batch_ids_remains_separate_and_deduplicates_replay() {
    let mut data = LiveData::default();
    for round in 0..3 {
        for item in 0..3 {
            let mut raw = v1_packet(round, item, true);
            raw["data"]["batch_combo_id"] = Value::Null;
            raw["data"]["batch_combo_send"] = Value::Null;
            feed(&mut data, &raw);
            feed(&mut data, &raw);
        }
    }
    assert_totals(&data, 9, 30, 2910, 4500);
}

#[test]
fn v1_v2_equivalent_transactions_deduplicate_in_both_orders() {
    let v2: Value = serde_json::from_slice(include_bytes!(
        "../../crates/blivedm/tests/fixtures/ten_blind_gift_v2.json"
    ))
    .unwrap();
    let v1: Vec<_> = parse(&v2).iter().map(v1_from_gift).collect();
    for v2_first in [true, false] {
        let mut data = LiveData::default();
        for _ in 0..2 {
            if v2_first {
                feed(&mut data, &v2);
            }
            for raw in &v1 {
                feed(&mut data, raw);
            }
            if !v2_first {
                feed(&mut data, &v2);
            }
        }
        assert_totals(&data, 3, 10, 1110, 1500);
        let updates = data.take_pending_updates();
        let count: usize = updates
            .iter()
            .filter_map(|update| {
                if let DataUpdate::GiftUpsert(gifts) = update {
                    Some(gifts.len())
                } else {
                    None
                }
            })
            .sum();
        assert_eq!(count, 3);
    }
}

#[test]
fn v1_out_of_order_distinct_transactions_preserve_quantity_and_money() {
    let mut data = LiveData::default();
    for round in [2, 0, 1] {
        for item in [2, 0, 1] {
            feed(&mut data, &v1_packet(round, item, true));
        }
    }
    assert_totals(&data, 3, 30, 2910, 4500);
}

#[test]
#[ignore = "本地调查：需要 test/dump_blind_gift_combo.txt，不随仓库分发"]
fn audit_existing_real_v1_dump() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../test/dump_blind_gift_combo.txt");
    let dump = std::fs::read_to_string(path).unwrap();
    let mut data = LiveData::default();
    for line in dump.lines() {
        let (_, body) = line.split_once("] ").unwrap();
        let raw: Value = serde_json::from_str(body).unwrap();
        if raw["cmd"] == "SEND_GIFT" {
            feed(&mut data, &raw);
        }
    }
    assert_totals(&data, 6, 6, 890, 900);
}

#[test]
#[ignore = "条件性失败探针：需要真实包确认同种礼物是否会共用 tid 分包"]
fn audit_same_tid_split_result_should_keep_both_parts() {
    let mut data = LiveData::default();
    let first = v1_packet(0, 0, true);
    let mut next = first.clone();
    next["data"]["num"] = json!(2);
    next["data"]["total_coin"] = json!(30000);
    next["data"]["combo_total_coin"] = json!(96000);
    feed(&mut data, &first);
    feed(&mut data, &next);
    assert_totals(&data, 1, 6, 960, 900);
}

#[tokio::test]
#[ignore = "失败探针：旧 combo 移出 5000 行缓存后继续收到礼物，归档累计值会回退"]
async fn audit_evicted_combo_should_not_overwrite_archive_with_partial_count() {
    let mut data = LiveData::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    data.archive_tx = Some(tx);
    feed(&mut data, &v1_packet(0, 0, true));
    for index in 0..MAX_GIFT_LIST {
        let raw = json!({"cmd": "SEND_GIFT", "data": {
            "giftId": 1, "giftName": "填充礼物", "num": 1, "price": 1000,
            "total_coin": 1000, "coin_type": "gold", "uid": 7,
            "timestamp": 1700000010, "tid": format!("filler-{index}")
        }});
        feed(&mut data, &raw);
        data.take_pending_updates();
    }
    assert!(data.gift_list.iter().all(|gift| gift.gift_id != ITEMS[0].0));
    let mut next = v1_packet(1, 0, true);
    next["data"]["num"] = json!(2);
    next["data"]["total_coin"] = json!(30000);
    feed(&mut data, &next);
    assert_eq!(data.stats.gift_revenue, MAX_GIFT_LIST as u64 * 10 + 960);

    let archive = ArchiveManager::new(":memory:".into()).unwrap();
    let session_id = archive.start_session(1, "缓存淘汰边界", 7).await.unwrap();
    while let Ok(ArchiveEvent::Gift(gift)) = rx.try_recv() {
        archive.save_gift(session_id, &gift).await.unwrap();
    }
    let saved = archive
        .search_gifts(session_id, "爱心抱枕", None, None, 1, 100)
        .await
        .unwrap();
    assert_eq!(saved.items.iter().map(|gift| gift.num).sum::<u32>(), 6);
    assert_eq!(
        saved.items.iter().map(|gift| gift.total_value).sum::<u64>(),
        960
    );
}
