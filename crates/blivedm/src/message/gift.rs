//! 礼物消息

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GuardLevel, Medal};

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
    pub price: u32,
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
    pub combo_stay_time: Option<u32>,
    /// 连击总价值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combo_total_coin: Option<u64>,
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
            .and_then(|info| info.get("img_basic"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let num = u32::try_from(data.get("num")?.as_u64()?).ok()?;
        let price = u32::try_from(data.get("price")?.as_u64()?).ok()?;
        let total_coin = data.get("total_coin")?.as_u64()?;
        let coin_type_str = data.get("coin_type")?.as_str()?;
        let coin_type = match coin_type_str {
            "gold" => CoinType::Gold,
            "silver" => CoinType::Silver,
            _ => return None,
        };

        let sender_uid = data.get("uid")?.as_u64()?;
        let sender_name = data
            .get("uname")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let sender_face = data.get("face").and_then(|v| v.as_str()).map(String::from);
        let action = data
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("投喂")
            .to_string();
        let timestamp = data.get("timestamp")?.as_i64()?;
        // 旧协议和部分免费礼物可能完全不提供 `tid`；但如果提供了，
        // 它就是幂等所需的关键身份，不能把错误类型或空值降级为“未提供”。
        let transaction_id = match data.get("tid") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let value = value.as_str()?;
                if value.trim().is_empty() {
                    return None;
                }
                Some(value.to_string())
            }
        };
        let batch_combo_id = data
            .get("batch_combo_id")
            .and_then(|v| v.as_str())
            .map(String::from);
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
            })
        });
        let combo_stay_time = data
            .get("combo_stay_time")
            .and_then(|v| v.as_u64())
            .and_then(|value| u32::try_from(value).ok());
        let combo_total_coin = data.get("combo_total_coin").and_then(|v| v.as_u64());

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
                anchor_uid: m.get("target_id")?.as_u64().unwrap_or(0),
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
            self.total_coin / 10
        } else {
            0
        }
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
    fn accepts_missing_presentation_fields() {
        let gift = Gift::parse(&minimal_gift()).expect("presentation fields are optional");

        assert_eq!(gift.gift_name, "");
        assert_eq!(gift.gift_icon, "");
        assert_eq!(gift.sender_name, "");
        assert_eq!(gift.action, "投喂");
        assert_eq!(gift.transaction_id, None);
    }

    #[test]
    fn rejects_out_of_range_core_amounts_instead_of_truncating() {
        let mut raw = minimal_gift();
        raw["data"]["num"] = json!(u64::from(u32::MAX) + 1);
        assert!(Gift::parse(&raw).is_none());

        let mut raw = minimal_gift();
        raw["data"]["price"] = json!(u64::from(u32::MAX) + 1);
        assert!(Gift::parse(&raw).is_none());
    }

    #[test]
    fn drops_out_of_range_optional_metadata_instead_of_truncating() {
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
        assert_eq!(gift.combo_stay_time, None);
        assert!(gift.batch_combo_send.is_none());
    }

    #[test]
    fn rejects_malformed_critical_fields() {
        let mut raw = minimal_gift();
        raw["data"]["coin_type"] = json!("points");
        assert!(Gift::parse(&raw).is_none());

        let mut raw = minimal_gift();
        raw["data"]["tid"] = json!(123);
        assert!(Gift::parse(&raw).is_none());

        let mut raw = minimal_gift();
        raw["data"]["tid"] = json!("");
        assert!(Gift::parse(&raw).is_none());

        let mut raw = minimal_gift();
        raw["data"]["tid"] = json!("   ");
        assert!(Gift::parse(&raw).is_none());
    }
}
