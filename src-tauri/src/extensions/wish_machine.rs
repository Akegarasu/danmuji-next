//! 心愿机：只消费去重后的礼物增量，配置和手动校准分别修改。
use super::Extension;
use crate::live_events::ReceivedGift;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

const MAX_COUNT: u64 = 999_999_999;
const MAX_GOALS: usize = 20;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlindGiftMode {
    #[default]
    Original,
    Revealed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Default,
    TestA,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WishConfig {
    pub enabled: bool,
    pub blind_gift_mode: BlindGiftMode,
    pub theme: Theme,
    pub theme_a_font_family: String,
    pub custom_css: String,
}

impl Default for WishConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            blind_gift_mode: BlindGiftMode::Original,
            theme: Theme::Default,
            theme_a_font_family: String::new(),
            custom_css: String::new(),
        }
    }
}

impl WishConfig {
    fn validate(&self) -> Result<(), String> {
        if self.theme_a_font_family.chars().count() > 100
            || self.theme_a_font_family.chars().any(char::is_control)
        {
            return Err("字体名称最多 100 个字符，不能包含控制字符".into());
        }
        if self.custom_css.len() > 32_768 {
            return Err("自定义 CSS 不能超过 32 KB".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WishGoal {
    pub id: String,
    pub gift_id: Option<u64>,
    pub gift_name: String,
    pub current: u64,
    pub target: u64,
}

impl WishGoal {
    fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 100
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err("无效的心愿 ID".into());
        }
        if self.gift_name.trim().is_empty() || self.gift_name.chars().count() > 80 {
            return Err("礼物名称需要 1～80 个字符".into());
        }
        if self
            .gift_id
            .is_some_and(|id| id == 0 || id > 9_007_199_254_740_991)
        {
            return Err("礼物 ID 必须是有效的正整数".into());
        }
        if self.target == 0 || self.target > MAX_COUNT || self.current > MAX_COUNT {
            return Err(format!(
                "目标数量需要在 1～{MAX_COUNT} 之间，已获得数量需要在 0～{MAX_COUNT} 之间"
            ));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct WishMachine {
    config: WishConfig,
    goals: Vec<WishGoal>,
}

impl Default for WishMachine {
    fn default() -> Self {
        Self {
            config: WishConfig::default(),
            goals: [
                ("captain", "舰长", 5),
                ("blind-box", "心动盲盒", 200),
                ("fans", "粉丝团灯牌", 100),
            ]
            .into_iter()
            .map(|(id, name, target)| WishGoal {
                id: id.into(),
                gift_id: None,
                gift_name: name.into(),
                current: 0,
                target,
            })
            .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WishRequest {
    Configure {
        config: WishConfig,
    },
    AddGoal {
        gift_id: Option<u64>,
        gift_name: String,
        current: u64,
        target: u64,
    },
    UpdateGoal {
        id: String,
        gift_id: Option<u64>,
        gift_name: String,
        target: u64,
        current: Option<u64>,
    },
    RemoveGoal {
        id: String,
    },
    MoveGoal {
        id: String,
        direction: i8,
    },
}

impl Extension for WishMachine {
    fn id(&self) -> &'static str {
        "wish-machine"
    }
    fn snapshot(&self, _now: Instant) -> Value {
        json!(self)
    }
    fn checkpoint(&self, now: Instant) -> Value {
        self.snapshot(now)
    }
    fn restore(&mut self, value: Value, _now: Instant) -> Result<(), String> {
        let saved: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        saved.config.validate()?;
        if saved.goals.len() > MAX_GOALS {
            return Err("最多设置 20 个心愿".into());
        }
        let mut ids = std::collections::HashSet::new();
        for goal in &saved.goals {
            goal.validate()?;
            if !ids.insert(&goal.id) {
                return Err("心愿 ID 重复".into());
            }
        }
        *self = saved;
        Ok(())
    }
    fn request(&mut self, request: Value, now: Instant) -> Result<Value, String> {
        let request: WishRequest =
            serde_json::from_value(request).map_err(|e| format!("无效的心愿机操作：{e}"))?;
        match request {
            WishRequest::Configure { config } => {
                config.validate()?;
                self.config = config;
            }
            WishRequest::AddGoal {
                gift_id,
                gift_name,
                current,
                target,
            } => {
                if self.goals.len() >= MAX_GOALS {
                    return Err("最多设置 20 个心愿".into());
                }
                let goal = WishGoal {
                    id: super::new_id("wish"),
                    gift_id,
                    gift_name: gift_name.trim().into(),
                    current,
                    target,
                };
                goal.validate()?;
                self.goals.push(goal);
            }
            WishRequest::UpdateGoal {
                id,
                gift_id,
                gift_name,
                target,
                current,
            } => {
                let goal = self
                    .goals
                    .iter_mut()
                    .find(|goal| goal.id == id)
                    .ok_or("心愿不存在")?;
                // 未手动修改数量时，保留编辑期间新收到的礼物。
                let updated = WishGoal {
                    id,
                    gift_id,
                    gift_name: gift_name.trim().into(),
                    target,
                    current: current.unwrap_or(goal.current),
                };
                updated.validate()?;
                *goal = updated;
            }
            WishRequest::RemoveGoal { id } => {
                let index = self
                    .goals
                    .iter()
                    .position(|goal| goal.id == id)
                    .ok_or("心愿不存在")?;
                self.goals.remove(index);
            }
            WishRequest::MoveGoal { id, direction } => {
                if ![-1, 1].contains(&direction) {
                    return Err("无效的移动方向".into());
                }
                let index = self
                    .goals
                    .iter()
                    .position(|goal| goal.id == id)
                    .ok_or("心愿不存在")?;
                let next = index as isize + direction as isize;
                if next < 0 || next as usize >= self.goals.len() {
                    return Err("已经到达列表边缘".into());
                }
                self.goals.swap(index, next as usize);
            }
        }
        Ok(self.snapshot(now))
    }
    fn browser_visible(&self) -> bool {
        true
    }
    fn browser_snapshot(&self, _now: Instant) -> Value {
        // CSS 是供用户复制到 OBS 的编辑草稿，不通过直播覆盖层自动应用。
        json!({"goals": self.goals.iter().map(|g| json!({"id":g.id, "gift_name":g.gift_name, "current":g.current, "target":g.target})).collect::<Vec<_>>()})
    }
    fn on_gift(&mut self, gift: &ReceivedGift, _now: Instant) -> bool {
        if !self.config.enabled || gift.num == 0 {
            return false;
        }
        let (id, name) = match (self.config.blind_gift_mode, &gift.blind_gift) {
            (BlindGiftMode::Original, Some(blind)) => (blind.gift_id, &blind.gift_name),
            _ => (gift.gift_id, &gift.gift_name),
        };
        let mut changed = false;
        for goal in &mut self.goals {
            if goal
                .gift_id
                .map_or(goal.gift_name == *name, |expected| expected == id)
            {
                let count = (goal.current + u64::from(gift.num)).min(MAX_COUNT);
                changed |= count != goal.current;
                goal.current = count;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_events::ReceivedBlindGift;

    fn gift(name: &str, id: u64, num: u32) -> ReceivedGift {
        ReceivedGift {
            event_id: None,
            sender_uid: 1,
            gift_id: id,
            gift_name: name.into(),
            sender_name: "测试用户".into(),
            num,
            blind_gift: None,
        }
    }
    #[test]
    fn counts_incremental_gifts_guards_and_selected_blind_identity() {
        let now = Instant::now();
        let mut m = WishMachine::default();
        assert!(m.on_gift(&gift("舰长", 10003, 2), now));
        assert!(m.on_gift(&gift("舰长", 10003, 4), now));
        assert_eq!(m.goals[0].current, 6); // 达标后继续累计。
        let mut blind = gift("粉丝团灯牌", 1, 3);
        blind.blind_gift = Some(ReceivedBlindGift {
            gift_id: 32251,
            gift_name: "心动盲盒".into(),
        });
        m.on_gift(&blind, now);
        assert_eq!((m.goals[1].current, m.goals[2].current), (3, 0));
        m.config.blind_gift_mode = BlindGiftMode::Revealed;
        m.on_gift(&blind, now);
        assert_eq!((m.goals[1].current, m.goals[2].current), (3, 3));
        m.goals[2].gift_id = Some(2);
        assert!(!m.on_gift(&gift("粉丝团灯牌", 1, 1), now));
        assert!(m.on_gift(&gift("改名后礼物", 2, 1), now));
        m.config.enabled = false;
        assert!(!m.on_gift(&gift("舰长", 10003, 1), now));
    }
    #[test]
    fn editing_targets_preserves_live_counts_and_manual_count_is_explicit() {
        let now = Instant::now();
        let mut m = WishMachine::default();
        m.on_gift(&gift("舰长", 10003, 2), now);
        let update = json!({"type":"update_goal","id":"captain","gift_id":null,"gift_name":"舰长","target":10});
        m.request(update.clone(), now).unwrap();
        assert_eq!(m.goals[0].current, 2);
        let mut manual = update.clone();
        manual["current"] = json!(7);
        m.request(manual, now).unwrap();
        m.on_gift(&gift("舰长", 10003, 1), now);
        assert_eq!(m.goals[0].current, 8);
        let before = m.snapshot(now);
        for invalid in [json!(0), json!(-1), json!(1.5), json!(MAX_COUNT + 1)] {
            let mut request = update.clone();
            request["target"] = invalid;
            assert!(m.request(request, now).is_err());
            assert_eq!(m.snapshot(now), before);
        }
        m.request(json!({"type":"configure","config":{"enabled":false,"theme":"test-a","theme_a_font_family":"楷体","custom_css":"#wish-list { gap: 8px; }"}}), now).unwrap();
        assert_eq!(m.goals[0].current, 8);
        let mut restored = WishMachine::default();
        restored.restore(m.checkpoint(now), now).unwrap();
        assert_eq!(restored.snapshot(now), m.snapshot(now));
        assert_eq!(restored.config.theme_a_font_family, "楷体");
        let mut legacy = m.checkpoint(now);
        legacy["config"]
            .as_object_mut()
            .unwrap()
            .remove("theme_a_font_family");
        restored.restore(legacy, now).unwrap();
        assert!(restored.config.theme_a_font_family.is_empty());
        for font in ["a".repeat(101), "字体\n名称".into()] {
            let mut invalid = m.snapshot(now)["config"].clone();
            invalid["theme_a_font_family"] = json!(font);
            assert!(m
                .request(json!({"type":"configure","config":invalid}), now)
                .is_err());
            assert_eq!(m.config.theme_a_font_family, "楷体");
        }
        assert!(m.browser_snapshot(now).get("config").is_none());
    }
    #[test]
    fn add_move_remove_limits_and_restore_validation() {
        let now = Instant::now();
        let mut m = WishMachine::default();
        let add = json!({"type":"add_goal","gift_id":null,"gift_name":" 小心心 ","target":10,"current":12});
        m.request(add.clone(), now).unwrap();
        let id = m.goals[3].id.clone();
        assert_eq!(m.goals[3].gift_name, "小心心");
        m.request(json!({"type":"move_goal","id":id,"direction":-1}), now)
            .unwrap();
        assert_eq!(m.goals[2].id, id);
        m.request(json!({"type":"remove_goal","id":id}), now)
            .unwrap();
        for _ in 3..MAX_GOALS {
            m.request(add.clone(), now).unwrap();
        }
        assert!(m.request(add, now).is_err());
        let before = m.snapshot(now);
        let mut invalid = before.clone();
        invalid["goals"][1]["id"] = invalid["goals"][0]["id"].clone();
        assert!(m.restore(invalid, now).is_err());
        assert_eq!(m.snapshot(now), before);
        m.goals[0].current = MAX_COUNT - 1;
        m.on_gift(&gift("舰长", 10003, u32::MAX), now);
        assert_eq!(m.goals[0].current, MAX_COUNT);
    }
}
