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
    /// 价格（金瓜子）
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{GuardBuy, GuardLevel};

    fn minimal_guard_buy() -> serde_json::Value {
        json!({
            "data": {
                "uid": 42,
                "guard_level": 3,
                "num": 1,
                "price": 19_800_000,
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
}
