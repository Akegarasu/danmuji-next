//! 大航海（舰长/提督/总督）

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::GuardLevel;

/// 大航海购买消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardBuy {
    /// 用户 UID
    pub uid: u64,
    /// 用户名
    pub username: String,
    /// 舰队等级
    pub guard_level: GuardLevel,
    /// 购买数量（月数）
    pub num: u32,
    /// 原始标价（金瓜子），不一定等于本次实际成交金额
    pub price: u64,
    /// 礼物 ID
    pub gift_id: u64,
    /// 礼物名称
    pub gift_name: String,
    /// 开始时间戳
    pub start_time: i64,
    /// 结束时间戳
    pub end_time: i64,
}

impl GuardBuy {
    /// 从 JSON 解析大航海购买消息
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.get("data")?;

        let uid = data.get("uid")?.as_u64()?;
        let username = data
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let guard_level = GuardLevel::from(data.get("guard_level")?.as_i64()?);
        if guard_level == GuardLevel::None {
            return None;
        }
        let num = u32::try_from(data.get("num")?.as_u64()?).ok()?;
        let price = data.get("price")?.as_u64()?;
        let gift_id = data.get("gift_id")?.as_u64()?;
        let gift_name = data
            .get("gift_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let start_time = data.get("start_time")?.as_i64()?;
        let end_time = data
            .get("end_time")
            .and_then(Value::as_i64)
            .unwrap_or(start_time);

        Some(GuardBuy {
            uid,
            username,
            guard_level,
            num,
            price,
            gift_id,
            gift_name,
            start_time,
            end_time,
        })
    }

    /// 获取舰队名称
    pub fn guard_name(&self) -> &'static str {
        match self.guard_level {
            GuardLevel::Governor => "总督",
            GuardLevel::Admiral => "提督",
            GuardLevel::Captain => "舰长",
            GuardLevel::None => "无",
        }
    }

    pub fn value_cny_fen(&self) -> u64 {
        self.price / 10
    }
}

/// 大航海成交 Toast。
///
/// 与 `GUARD_BUY` 不同，Toast 中的价格是本次订单总金额，已经包含
/// 连续包月、续费等折扣；`num > 1` 时不得再次乘以数量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardToast {
    /// 用户 UID
    pub uid: u64,
    /// 用户名
    pub username: String,
    /// 用户头像；旧版 Toast 通常不提供
    pub face: Option<String>,
    /// 舰队等级
    pub guard_level: GuardLevel,
    /// 购买数量
    pub num: u32,
    /// 本次订单总金额（金瓜子，人民币金额为 price / 1000）
    pub price: u64,
    /// 礼物 ID
    pub gift_id: u64,
    /// 身份名称
    pub role_name: String,
    /// 数量单位，例如“月”或“年”
    pub unit: String,
    /// 支付流水号，可用于合并同时下发的 V1/V2 Toast
    pub payflow_id: Option<String>,
    /// 来源；通常 0 为付费，特殊活动赠送时可能为 2
    pub source: u32,
    /// 开始时间戳
    pub start_time: i64,
    /// 结束时间戳
    pub end_time: i64,
}

impl GuardToast {
    /// 解析 `USER_TOAST_MSG` 或 `USER_TOAST_MSG_V2`。
    pub fn parse(value: &Value) -> Option<Self> {
        let cmd = value.get("cmd")?.as_str()?.split(':').next()?;
        match cmd {
            "USER_TOAST_MSG" => Self::parse_v1(value),
            "USER_TOAST_MSG_V2" => Self::parse_v2(value),
            _ => None,
        }
    }

    fn parse_v1(value: &Value) -> Option<Self> {
        let data = value.get("data")?;
        let guard_level = valid_guard_level(data.get("guard_level"))?;
        let start_time = data.get("start_time")?.as_i64()?;

        Some(Self {
            uid: data.get("uid")?.as_u64()?,
            username: non_empty_string(data.get("username")).unwrap_or_default(),
            face: None,
            guard_level,
            num: u32::try_from(data.get("num")?.as_u64()?).ok()?,
            price: data.get("price")?.as_u64()?,
            gift_id: data.get("gift_id")?.as_u64()?,
            role_name: non_empty_string(data.get("role_name")).unwrap_or_default(),
            unit: non_empty_string(data.get("unit")).unwrap_or_default(),
            payflow_id: non_empty_string(data.get("payflow_id")),
            source: data
                .get("source")
                .and_then(Value::as_u64)
                .and_then(|source| u32::try_from(source).ok())
                .unwrap_or_default(),
            start_time,
            end_time: data
                .get("end_time")
                .and_then(Value::as_i64)
                .unwrap_or(start_time),
        })
    }

    fn parse_v2(value: &Value) -> Option<Self> {
        let data = value.get("data")?;
        let sender = data.get("sender_uinfo")?;
        let sender_base = sender.get("base");
        let guard_info = data.get("guard_info")?;
        let pay_info = data.get("pay_info")?;
        let gift_info = data.get("gift_info")?;

        let guard_level = valid_guard_level(guard_info.get("guard_level"))?;
        let start_time = guard_info.get("start_time")?.as_i64()?;

        Some(Self {
            uid: sender.get("uid")?.as_u64()?,
            username: non_empty_string(sender_base.and_then(|base| base.get("name")))
                .unwrap_or_default(),
            face: non_empty_string(sender_base.and_then(|base| base.get("face"))),
            guard_level,
            num: u32::try_from(pay_info.get("num")?.as_u64()?).ok()?,
            price: pay_info.get("price")?.as_u64()?,
            gift_id: gift_info.get("gift_id")?.as_u64()?,
            role_name: non_empty_string(guard_info.get("role_name")).unwrap_or_default(),
            unit: non_empty_string(pay_info.get("unit")).unwrap_or_default(),
            payflow_id: non_empty_string(pay_info.get("payflow_id")),
            source: data
                .get("option")
                .and_then(|option| option.get("source"))
                .and_then(Value::as_u64)
                .and_then(|source| u32::try_from(source).ok())
                .unwrap_or_default(),
            start_time,
            end_time: guard_info
                .get("end_time")
                .and_then(Value::as_i64)
                .unwrap_or(start_time),
        })
    }

    /// 获取舰队名称。上游缺少展示名称时使用等级对应的固定名称。
    pub fn guard_name(&self) -> &str {
        if !self.role_name.is_empty() {
            return &self.role_name;
        }
        match self.guard_level {
            GuardLevel::Governor => "总督",
            GuardLevel::Admiral => "提督",
            GuardLevel::Captain => "舰长",
            GuardLevel::None => "无",
        }
    }

    /// 本次订单总金额（人民币分）。
    pub fn value_cny_fen(&self) -> u64 {
        self.price / 10
    }
}

fn valid_guard_level(value: Option<&Value>) -> Option<GuardLevel> {
    let guard_level = GuardLevel::from(value?.as_i64()?);
    (guard_level != GuardLevel::None).then_some(guard_level)
}

fn non_empty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{GuardBuy, GuardLevel, GuardToast};

    fn minimal_guard_buy() -> serde_json::Value {
        json!({
            "data": {
                "uid": 42,
                "guard_level": 3,
                "num": 1,
                "price": 198_000,
                "gift_id": 10_003,
                "start_time": 1_700_000_000
            }
        })
    }

    #[test]
    fn accepts_missing_presentation_and_end_time_fields() {
        let guard = GuardBuy::parse(&minimal_guard_buy())
            .expect("presentation and end time fields are optional");

        assert_eq!(guard.username, "");
        assert_eq!(guard.gift_name, "");
        assert_eq!(guard.guard_level, GuardLevel::Captain);
        assert_eq!(guard.end_time, guard.start_time);
    }

    #[test]
    fn rejects_out_of_range_quantity_instead_of_truncating() {
        let mut raw = minimal_guard_buy();
        raw["data"]["num"] = json!(u64::from(u32::MAX) + 1);

        assert!(GuardBuy::parse(&raw).is_none());
    }

    #[test]
    fn rejects_missing_or_invalid_critical_fields() {
        let mut raw = minimal_guard_buy();
        raw["data"]["guard_level"] = json!(0);
        assert!(GuardBuy::parse(&raw).is_none());

        let mut raw = minimal_guard_buy();
        raw["data"].as_object_mut().unwrap().remove("price");
        assert!(GuardBuy::parse(&raw).is_none());

        let mut raw = minimal_guard_buy();
        raw["data"].as_object_mut().unwrap().remove("gift_id");
        assert!(GuardBuy::parse(&raw).is_none());
    }

    fn toast_v1() -> serde_json::Value {
        json!({
            "cmd": "USER_TOAST_MSG",
            "data": {
                "uid": 3375817,
                "username": "J酱desu",
                "guard_level": 3,
                "num": 1,
                "price": 138000,
                "gift_id": 10003,
                "role_name": "舰长",
                "unit": "月",
                "payflow_id": "2605132016258832158173516",
                "source": 0,
                "start_time": 1778674585,
                "end_time": 1778674585
            }
        })
    }

    fn toast_v2() -> serde_json::Value {
        json!({
            "cmd": "USER_TOAST_MSG_V2",
            "data": {
                "sender_uinfo": {
                    "uid": 3375817,
                    "base": { "name": "J酱desu", "face": "" }
                },
                "guard_info": {
                    "guard_level": 3,
                    "role_name": "舰长",
                    "start_time": 1778674585,
                    "end_time": 1778674585
                },
                "pay_info": {
                    "num": 1,
                    "price": 138000,
                    "unit": "月",
                    "payflow_id": "2605132016258832158173516"
                },
                "gift_info": { "gift_id": 10003 },
                "option": { "source": 0 }
            }
        })
    }

    #[test]
    fn parses_actual_total_price_from_v1_toast() {
        let toast = GuardToast::parse(&toast_v1()).expect("valid V1 guard toast");

        assert_eq!(toast.uid, 3_375_817);
        assert_eq!(toast.guard_level, GuardLevel::Captain);
        assert_eq!(toast.price, 138_000);
        assert_eq!(toast.value_cny_fen(), 13_800);
        assert_eq!(
            toast.payflow_id.as_deref(),
            Some("2605132016258832158173516")
        );
    }

    #[test]
    fn v1_and_v2_toasts_produce_the_same_transaction_data() {
        let v1 = GuardToast::parse(&toast_v1()).expect("valid V1 guard toast");
        let v2 = GuardToast::parse(&toast_v2()).expect("valid V2 guard toast");

        assert_eq!(v1.uid, v2.uid);
        assert_eq!(v1.guard_level, v2.guard_level);
        assert_eq!(v1.num, v2.num);
        assert_eq!(v1.price, v2.price);
        assert_eq!(v1.gift_id, v2.gift_id);
        assert_eq!(v1.start_time, v2.start_time);
        assert_eq!(v1.payflow_id, v2.payflow_id);
        assert_eq!(v2.face, None);
    }
}
