//! 礼物消息

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{parse_bool_flag, GuardLevel, Medal};

/// 礼物消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gift {
    /// 礼物 ID
    pub gift_id: u64,
    /// 礼物名称
    pub gift_name: String,
    /// 礼物图片
    pub gift_icon: String,
    /// 数量
    pub num: u32,
    /// 单价（金瓜子/银瓜子）
    pub price: u64,
    /// 总价值
    pub total_coin: u64,
    /// 货币类型
    pub coin_type: CoinType,
    /// 发送者 UID
    pub sender_uid: u64,
    /// 发送者名称
    pub sender_name: String,
    /// 发送者头像
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_face: Option<String>,
    /// 动作（投喂等）
    pub action: String,
    /// 时间戳
    pub timestamp: i64,
    /// Bilibili 上游礼物交易 ID（`data.tid`）。
    ///
    /// 旧协议或部分免费礼物可能不提供。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    /// 批次连击 ID（同一轮 batch combo 共用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_combo_id: Option<String>,
    /// 批次连击详情
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_combo_send: Option<BatchComboSend>,
    /// 连击详情
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combo_send: Option<ComboSend>,
    /// 连击停留时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combo_stay_time: Option<u64>,
    /// 连击总价值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combo_total_coin: Option<u64>,
    /// 批量连击累计数量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub super_batch_gift_num: Option<u64>,
    /// 连击动效资源 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combo_resources_id: Option<u64>,
    /// 是否显示批量连击发送
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_batch_combo_send: Option<bool>,
    /// 盲盒信息；普通礼物的上游字段为 `null`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blind_gift: Option<BlindGift>,
    /// 发送者勋章
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal: Option<Medal>,
    /// 舰队等级
    pub guard_level: GuardLevel,
}

/// 批次连击信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchComboSend {
    pub action: String,
    pub batch_combo_id: String,
    pub batch_combo_num: u32,
    pub gift_id: u64,
    pub gift_name: String,
    pub gift_num: u32,
    pub uid: u64,
    pub uname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blind_gift: Option<BlindGift>,
}

/// 连击信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComboSend {
    pub action: String,
    pub combo_id: String,
    pub combo_num: u32,
    pub gift_id: u64,
    pub gift_name: String,
    pub gift_num: u32,
    pub uid: u64,
    pub uname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blind_gift: Option<BlindGift>,
}

/// 盲盒礼物信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindGift {
    /// 盲盒配置 ID
    pub blind_gift_config_id: u64,
    /// 盲盒来源
    pub from: u64,
    /// 礼物动作（例如“爆出”）
    pub gift_action: String,
    /// 爆出礼物单价（金瓜子）
    pub gift_tip_price: u64,
    /// 原盲盒礼物 ID
    pub original_gift_id: u64,
    /// 原盲盒礼物名称
    pub original_gift_name: String,
    /// 原盲盒单价（金瓜子）
    pub original_gift_price: u64,
}

impl BlindGift {
    fn parse(value: &Value) -> Option<Self> {
        Some(Self {
            blind_gift_config_id: value.get("blind_gift_config_id")?.as_u64()?,
            from: value.get("from")?.as_u64()?,
            gift_action: value.get("gift_action")?.as_str()?.to_string(),
            gift_tip_price: value.get("gift_tip_price")?.as_u64()?,
            original_gift_id: value.get("original_gift_id")?.as_u64()?,
            original_gift_name: value.get("original_gift_name")?.as_str()?.to_string(),
            original_gift_price: value.get("original_gift_price")?.as_u64()?,
        })
    }
}

/// 货币类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoinType {
    /// 金瓜子（付费）
    Gold,
    /// 银瓜子（免费）
    Silver,
}

impl Gift {
    /// 从 JSON 解析礼物消息
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.get("data")?;

        let gift_id = data.get("giftId")?.as_u64()?;
        let gift_name = data
            .get("giftName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let gift_icon = data
            .get("gift_info")
            .and_then(|info| {
                // `img_basic` 是静态礼物图；`webp`/`gif` 可能包含动画，
                // 仅在静态图缺失时用于兼容旧消息。
                ["img_basic", "webp", "gif"]
                    .into_iter()
                    .find_map(|key| non_empty_json_string(info.get(key)))
            })
            .unwrap_or_default();
        let num = u32::try_from(data.get("num")?.as_u64()?).ok()?;
        let price = data.get("price")?.as_u64()?;
        let total_coin = data.get("total_coin")?.as_u64()?;
        let coin_type_str = data.get("coin_type")?.as_str()?;
        let coin_type = match coin_type_str {
            "gold" => CoinType::Gold,
            "silver" => CoinType::Silver,
            _ => return None,
        };

        let sender_uinfo = data.get("sender_uinfo").filter(|value| !value.is_null());
        let sender_base = sender_uinfo.and_then(|uinfo| uinfo.get("base"));
        let sender_uid = sender_uinfo
            .and_then(|uinfo| uinfo.get("uid"))
            .and_then(Value::as_u64)
            .filter(|uid| *uid != 0)
            .or_else(|| data.get("uid").and_then(Value::as_u64))?;
        let sender_name = sender_base
            .and_then(|base| non_empty_json_string(base.get("name")))
            .or_else(|| non_empty_json_string(data.get("uname")))
            .unwrap_or_default();
        let sender_face = sender_base
            .and_then(|base| non_empty_json_string(base.get("face")))
            .or_else(|| non_empty_json_string(data.get("face")));
        let action = data
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("投喂")
            .to_string();
        let timestamp = data.get("timestamp")?.as_i64()?;
        // 旧协议和部分免费礼物可能不提供或返回空 `tid`。
        let transaction_id = match data.get("tid") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let value = value.as_str()?;
                if value.trim().is_empty() {
                    None
                } else {
                    Some(value.to_string())
                }
            }
        };
        let batch_combo_id = parse_optional_combo_id(data.get("batch_combo_id"))?;
        let batch_combo_send = data.get("batch_combo_send").and_then(|v| {
            Some(BatchComboSend {
                action: v.get("action")?.as_str()?.to_string(),
                batch_combo_id: v.get("batch_combo_id")?.as_str()?.to_string(),
                batch_combo_num: u32::try_from(v.get("batch_combo_num")?.as_u64()?).ok()?,
                gift_id: v.get("gift_id")?.as_u64()?,
                gift_name: v.get("gift_name")?.as_str()?.to_string(),
                gift_num: u32::try_from(v.get("gift_num")?.as_u64()?).ok()?,
                uid: v.get("uid")?.as_u64()?,
                uname: v.get("uname")?.as_str()?.to_string(),
                blind_gift: v.get("blind_gift").and_then(BlindGift::parse),
            })
        });
        let combo_send = data.get("combo_send").and_then(|v| {
            Some(ComboSend {
                action: v.get("action")?.as_str()?.to_string(),
                combo_id: v.get("combo_id")?.as_str()?.to_string(),
                combo_num: u32::try_from(v.get("combo_num")?.as_u64()?).ok()?,
                gift_id: v.get("gift_id")?.as_u64()?,
                gift_name: v.get("gift_name")?.as_str()?.to_string(),
                gift_num: u32::try_from(v.get("gift_num")?.as_u64()?).ok()?,
                uid: v.get("uid")?.as_u64()?,
                uname: v.get("uname")?.as_str()?.to_string(),
                blind_gift: v.get("blind_gift").and_then(BlindGift::parse),
            })
        });
        let combo_stay_time = data.get("combo_stay_time").and_then(Value::as_u64);
        let combo_total_coin = data.get("combo_total_coin").and_then(|v| v.as_u64());
        let super_batch_gift_num = data.get("super_batch_gift_num").and_then(Value::as_u64);
        let combo_resources_id = data.get("combo_resources_id").and_then(Value::as_u64);
        let show_batch_combo_send = data.get("show_batch_combo_send").and_then(Value::as_bool);
        let blind_gift = data.get("blind_gift").and_then(BlindGift::parse);

        let guard_level = data
            .get("guard_level")
            .and_then(|v| v.as_i64())
            .map(GuardLevel::from)
            .unwrap_or(GuardLevel::None);

        // 解析勋章信息
        let medal = data.get("medal_info").and_then(|m| {
            let level = u32::try_from(m.get("medal_level")?.as_u64()?).ok()?;
            if level == 0 {
                return None;
            }
            Some(Medal {
                level,
                name: m.get("medal_name")?.as_str()?.to_string(),
                anchor_name: m.get("anchor_uname")?.as_str()?.to_string(),
                room_id: m.get("anchor_roomid")?.as_u64()?,
                color: u32::try_from(m.get("medal_color")?.as_u64()?).ok()?,
                anchor_uid: m.get("target_id").and_then(Value::as_u64).unwrap_or(0),
                is_light: parse_bool_flag(m.get("is_lighted")).unwrap_or(true),
            })
        });

        Some(Gift {
            gift_id,
            gift_name,
            gift_icon,
            num,
            price,
            total_coin,
            coin_type,
            sender_uid,
            sender_name,
            sender_face,
            action,
            timestamp,
            transaction_id,
            batch_combo_id,
            batch_combo_send,
            combo_send,
            combo_stay_time,
            combo_total_coin,
            super_batch_gift_num,
            combo_resources_id,
            show_batch_combo_send,
            blind_gift,
            medal,
            guard_level,
        })
    }

    /// 是否为付费礼物
    pub fn is_paid(&self) -> bool {
        self.coin_type == CoinType::Gold
    }

    /// 礼物价值（人民币，分）
    pub fn value_cny_fen(&self) -> u64 {
        if self.is_paid() {
            self.revealed_total_coin() / 10
        } else {
            0
        }
    }

    /// 礼物用于展示的总价值（金瓜子）。
    ///
    /// 盲盒消息的 `total_coin` 是盲盒消费金额，爆出礼物价值由
    /// `blind_gift.gift_tip_price * num` 给出。
    pub fn revealed_total_coin(&self) -> u64 {
        self.blind_gift
            .as_ref()
            .map_or(self.total_coin, |blind_gift| {
                blind_gift
                    .gift_tip_price
                    .saturating_mul(u64::from(self.num))
            })
    }

    /// 盲盒实际消费金额（人民币，分）
    pub fn blind_gift_cost_cny_fen(&self) -> Option<u64> {
        self.blind_gift.as_ref().map(|_| self.total_coin / 10)
    }

    /// 是否属于 Bilibili 明确标识的一轮批量连击。
    pub fn is_combo(&self) -> bool {
        self.batch_combo_id.is_some()
    }

}

fn non_empty_json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn parse_optional_combo_id(value: Option<&Value>) -> Option<Option<String>> {
    match value {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(value)) if value.trim().is_empty() => Some(None),
        Some(Value::String(value)) => Some(Some(value.clone())),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Gift;

    fn minimal_gift() -> serde_json::Value {
        json!({
            "data": {
                "giftId": 1,
                "num": 1,
                "price": 100,
                "total_coin": 100,
                "coin_type": "gold",
                "uid": 42,
                "timestamp": 1_700_000_000
            }
        })
    }

    #[test]
    fn preserves_upstream_transaction_id() {
        let raw = json!({
            "data": {
                "giftId": 1,
                "giftName": "辣条",
                "gift_info": { "img_basic": "https://example.invalid/gift.png" },
                "num": 1,
                "price": 100,
                "total_coin": 100,
                "coin_type": "gold",
                "uid": 42,
                "uname": "tester",
                "timestamp": 1_700_000_000,
                "tid": "txn-123"
            }
        });
        let gift = Gift::parse(&raw).expect("valid fixture");
        assert_eq!(gift.transaction_id.as_deref(), Some("txn-123"));
    }

    #[test]
    fn prefers_static_gift_icon_over_animated_resources() {
        let mut raw = minimal_gift();
        raw["data"]["gift_info"] = json!({
            "img_basic": "https://example.invalid/static.png",
            "webp": "https://example.invalid/animated.webp",
            "gif": "https://example.invalid/animated.gif"
        });

        let gift = Gift::parse(&raw).expect("gift with presentation resources");

        assert_eq!(gift.gift_icon, "https://example.invalid/static.png");
    }

    #[test]
    fn prefers_sender_uinfo_and_falls_back_to_top_level_sender() {
        let mut raw = minimal_gift();
        raw["data"]["uid"] = json!(7);
        raw["data"]["uname"] = json!("顶层用户");
        raw["data"]["face"] = json!("https://example.invalid/top.webp");
        raw["data"]["sender_uinfo"] = json!({
            "uid": 42,
            "base": {
                "name": "嵌套用户",
                "face": "https://example.invalid/nested.webp"
            }
        });

        let gift = Gift::parse(&raw).expect("gift with sender_uinfo");
        assert_eq!(gift.sender_uid, 42);
        assert_eq!(gift.sender_name, "嵌套用户");
        assert_eq!(
            gift.sender_face.as_deref(),
            Some("https://example.invalid/nested.webp")
        );

        raw["data"]["sender_uinfo"] = json!({ "uid": 0, "base": {} });
        let gift = Gift::parse(&raw).expect("top-level sender fallback");
        assert_eq!(gift.sender_uid, 7);
        assert_eq!(gift.sender_name, "顶层用户");
        assert_eq!(
            gift.sender_face.as_deref(),
            Some("https://example.invalid/top.webp")
        );
    }

    #[test]
    fn accepts_missing_presentation_fields() {
        let gift = Gift::parse(&minimal_gift()).expect("presentation fields are optional");

        assert_eq!(gift.gift_name, "");
        assert_eq!(gift.gift_icon, "");
        assert_eq!(gift.sender_name, "");
        assert_eq!(gift.action, "投喂");
        assert_eq!(gift.transaction_id, None);
        assert!(gift.blind_gift.is_none());
    }

    #[test]
    fn parses_blind_gift_and_uses_revealed_value() {
        let blind_gift = json!({
            "blind_gift_config_id": 139,
            "from": 0,
            "gift_action": "爆出",
            "gift_tip_price": 16_000,
            "original_gift_id": 32_251,
            "original_gift_name": "心动盲盒",
            "original_gift_price": 15_000
        });
        let raw = json!({
            "data": {
                "giftId": 32_128,
                "giftName": "爱心抱枕",
                "gift_info": {
                    "img_basic": "https://example.invalid/static.png",
                    "webp": "https://example.invalid/animated.webp",
                    "gif": "https://example.invalid/animated.gif"
                },
                "num": 7,
                "price": 16_000,
                "total_coin": 15_000,
                "coin_type": "gold",
                "uid": 12_566_101,
                "uname": "秋葉aaaki",
                "timestamp": 1_785_243_039_i64,
                "batch_combo_id": "blind-combo",
                "batch_combo_send": {
                    "action": "投喂",
                    "batch_combo_id": "blind-combo",
                    "batch_combo_num": 1,
                    "gift_id": 32_128,
                    "gift_name": "爱心抱枕",
                    "gift_num": 1,
                    "uid": 12_566_101,
                    "uname": "秋葉aaaki",
                    "blind_gift": blind_gift.clone()
                },
                "blind_gift": blind_gift
            }
        });

        let gift = Gift::parse(&raw).expect("blind gift should parse");
        let blind_gift = gift.blind_gift.as_ref().expect("blind gift metadata");

        assert_eq!(gift.gift_id, 32128);
        assert_eq!(gift.gift_name, "爱心抱枕");
        assert_eq!(gift.num, 7);
        assert_eq!(
            gift.batch_combo_send
                .as_ref()
                .map(|combo| combo.batch_combo_num),
            Some(1)
        );
        assert_eq!(gift.total_coin, 15_000);
        assert_eq!(blind_gift.blind_gift_config_id, 139);
        assert_eq!(blind_gift.gift_action, "爆出");
        assert_eq!(blind_gift.gift_tip_price, 16_000);
        assert_eq!(blind_gift.original_gift_id, 32251);
        assert_eq!(blind_gift.original_gift_name, "心动盲盒");
        assert_eq!(blind_gift.original_gift_price, 15_000);
        assert_eq!(gift.revealed_total_coin(), 112_000);
        assert_eq!(gift.value_cny_fen(), 11_200);
        assert_eq!(gift.blind_gift_cost_cny_fen(), Some(1_500));
        assert!(gift
            .batch_combo_send
            .as_ref()
            .and_then(|combo| combo.blind_gift.as_ref())
            .is_some());
    }

    #[test]
    fn treats_null_blind_gift_as_regular_gift() {
        let mut raw = minimal_gift();
        raw["data"]["blind_gift"] = serde_json::Value::Null;

        let gift = Gift::parse(&raw).expect("null blind_gift is valid for regular gifts");

        assert!(gift.blind_gift.is_none());
        assert_eq!(gift.revealed_total_coin(), 100);
        assert_eq!(gift.value_cny_fen(), 10);
        assert_eq!(gift.blind_gift_cost_cny_fen(), None);
    }

    #[test]
    fn rejects_out_of_range_quantity_and_preserves_u64_price() {
        let mut raw = minimal_gift();
        raw["data"]["num"] = json!(u64::from(u32::MAX) + 1);
        assert!(Gift::parse(&raw).is_none());

        let mut raw = minimal_gift();
        raw["data"]["price"] = json!(u64::from(u32::MAX) + 1);
        let gift = Gift::parse(&raw).expect("price is uint64 in SEND_GIFT_V2");
        assert_eq!(gift.price, u64::from(u32::MAX) + 1);
    }

    #[test]
    fn preserves_u64_combo_stay_time_and_drops_invalid_nested_quantity() {
        let mut raw = minimal_gift();
        raw["data"]["combo_stay_time"] = json!(u64::from(u32::MAX) + 1);
        raw["data"]["batch_combo_send"] = json!({
            "action": "投喂",
            "batch_combo_id": "combo-1",
            "batch_combo_num": u64::from(u32::MAX) + 1,
            "gift_id": 1,
            "gift_name": "辣条",
            "gift_num": 1,
            "uid": 42,
            "uname": "tester"
        });

        let gift = Gift::parse(&raw).expect("optional metadata must not reject the gift");
        assert_eq!(gift.combo_stay_time, Some(u64::from(u32::MAX) + 1));
        assert!(gift.batch_combo_send.is_none());
    }

    #[test]
    fn handles_malformed_and_empty_identity_fields() {
        let mut raw = minimal_gift();
        raw["data"]["coin_type"] = json!("points");
        assert!(Gift::parse(&raw).is_none());

        let mut raw = minimal_gift();
        raw["data"]["tid"] = json!(123);
        assert!(Gift::parse(&raw).is_none());

        let mut raw = minimal_gift();
        raw["data"]["tid"] = json!("");
        assert_eq!(Gift::parse(&raw).unwrap().transaction_id, None);

        let mut raw = minimal_gift();
        raw["data"]["tid"] = json!("   ");
        assert_eq!(Gift::parse(&raw).unwrap().transaction_id, None);

        let mut raw = minimal_gift();
        raw["data"]["batch_combo_id"] = json!("");
        assert!(!Gift::parse(&raw).unwrap().is_combo());
    }
}
