//! 加班机领域逻辑：不依赖 Tauri、HTTP 或页面生命周期。
use std::time::Instant;

use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::Extension;
use crate::live_types::ReceivedGift;

// 限制在十年以内，避免恶意数量、倍率溢出和浏览器数值精度问题。
const MAX_SECONDS: f64 = 315_360_000.0;
const MAX_RATE: f64 = 100.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Add,
    Subtract,
    Multiply,
    Divide,
    SetTime,
    SetRate,
    Clear,
    Random,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomRange {
    pub action: Action,
    pub min: f64,
    pub max: f64,
}

impl RandomRange {
    fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.action,
            Action::Add | Action::Subtract | Action::Multiply | Action::Divide
        ) {
            return Err("随机操作仅支持增加、减少、乘以和除以".into());
        }
        validate_value(self.action, self.min)?;
        validate_value(self.action, self.max)?;
        if self.min > self.max {
            return Err("随机范围的最小值不能大于最大值".into());
        }
        let scale = self.scale();
        for value in [self.min, self.max] {
            if (value * scale - (value * scale).round()).abs() > 0.00001 {
                return Err("随机加减使用整数秒，随机乘除最多支持两位小数".into());
            }
        }
        Ok(())
    }

    fn scale(&self) -> f64 {
        if matches!(self.action, Action::Add | Action::Subtract) {
            1.0
        } else {
            100.0
        }
    }

    fn draw(&self, rng: &mut impl Rng) -> f64 {
        let scale = self.scale();
        let min = (self.min * scale).round() as u64;
        let max = (self.max * scale).round() as u64;
        rng.gen_range(min..=max) as f64 / scale
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftRule {
    pub id: String,
    pub enabled: bool,
    pub gift_id: Option<u64>,
    pub gift_name: String,
    pub action: Action,
    pub value: f64,
    pub per_gift: bool,
    /// 旧配置缺省为固定操作；随机规则从勾选的动作中等概率抽取。
    #[serde(default)]
    pub random_ranges: Vec<RandomRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TimerConfig {
    pub enabled: bool,
    pub initial_seconds: f64,
    pub show_rules: bool,
    pub show_notice: bool,
    pub rules: Vec<GiftRule>,
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            initial_seconds: 3600.0,
            show_rules: true,
            show_notice: true,
            rules: Vec::new(),
        }
    }
}

fn validate_value(action: Action, value: f64) -> Result<(), String> {
    if !value.is_finite() || value < 0.0 || value > MAX_SECONDS {
        return Err("时间或数值必须在 0 到 315360000 之间".into());
    }
    if matches!(action, Action::Multiply | Action::Divide | Action::SetRate)
        && !(0.01..=MAX_RATE).contains(&value)
    {
        return Err("倍率必须在 0.01 到 100 之间".into());
    }
    Ok(())
}

impl TimerConfig {
    fn validate(&self) -> Result<(), String> {
        validate_value(Action::SetTime, self.initial_seconds)?;
        if self.rules.len() > 100 {
            return Err("最多支持 100 条礼物规则".into());
        }
        let mut ids = std::collections::HashSet::new();
        for rule in &self.rules {
            if rule.id.is_empty() || rule.id.len() > 100 || !ids.insert(&rule.id) {
                return Err("规则 ID 不能为空或重复".into());
            }
            if rule.gift_name.trim().is_empty() || rule.gift_name.chars().count() > 80 {
                return Err("请填写 1 到 80 个字的礼物名称".into());
            }
            if rule
                .gift_id
                .is_some_and(|id| id == 0 || id > 9_007_199_254_740_991)
            {
                return Err("礼物 ID 必须是有效的正整数".into());
            }
            if rule.action == Action::Random {
                if rule.random_ranges.is_empty() || rule.random_ranges.len() > 4 {
                    return Err("随机规则需要勾选 1 到 4 种操作".into());
                }
                let mut actions = Vec::new();
                for range in &rule.random_ranges {
                    range.validate()?;
                    if actions.contains(&range.action) {
                        return Err("随机规则中的操作不能重复".into());
                    }
                    actions.push(range.action);
                }
            } else {
                validate_value(rule.action, rule.value)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedTimer {
    config: TimerConfig,
    remaining_ms: f64,
    rate: f64,
}

#[derive(Clone, Serialize)]
struct AppliedAction {
    action: Action,
    value: f64,
    count: u32,
    random: bool,
    delta_ms: f64,
}

#[derive(Clone, Serialize)]
struct GiftNotice {
    id: u64,
    gift_name: String,
    sender_name: String,
    num: u32,
    delta_ms: f64,
    actions: Vec<Action>,
    results: Vec<AppliedAction>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TimerRequest {
    Configure { config: TimerConfig },
    Start,
    Pause,
    Reset,
    Apply { action: Action, value: f64 },
}

pub struct Overtime {
    config: TimerConfig,
    remaining_ms: f64,
    rate: f64,
    running: bool,
    last_tick: Instant,
    sequence: u64,
    notices: Vec<GiftNotice>,
}

impl Overtime {
    pub fn new(now: Instant) -> Self {
        let config = TimerConfig::default();
        Self {
            remaining_ms: config.initial_seconds * 1000.0,
            config,
            rate: 1.0,
            running: false,
            last_tick: now,
            sequence: 0,
            notices: Vec::new(),
        }
    }

    fn remaining_at(&self, now: Instant) -> f64 {
        if self.running && self.config.enabled {
            (self.remaining_ms
                - now.saturating_duration_since(self.last_tick).as_secs_f64() * 1000.0 * self.rate)
                .max(0.0)
        } else {
            self.remaining_ms
        }
    }

    fn settle(&mut self, now: Instant) {
        self.remaining_ms = self.remaining_at(now);
        self.last_tick = now;
    }

    fn apply(&mut self, action: Action, value: f64, count: u32) {
        let count = f64::from(count);
        self.remaining_ms = match action {
            Action::Add => self.remaining_ms + value * 1000.0 * count,
            Action::Subtract => self.remaining_ms - value * 1000.0 * count,
            // 按个执行：3 个 ×2 等于 ×8；绝不使用连击累计数量。
            Action::Multiply => {
                if self.remaining_ms == 0.0 {
                    0.0
                } else {
                    self.remaining_ms * value.powf(count)
                }
            }
            Action::Divide => {
                if self.remaining_ms == 0.0 {
                    0.0
                } else {
                    self.remaining_ms / value.powf(count)
                }
            }
            Action::SetTime => value * 1000.0,
            Action::Clear => 0.0,
            Action::Random => unreachable!("随机规则必须先在后端抽取为具体操作"),
            Action::SetRate => {
                self.rate = value;
                self.remaining_ms
            }
        }
        .clamp(0.0, MAX_SECONDS * 1000.0);
    }
}

impl Extension for Overtime {
    fn id(&self) -> &'static str {
        "overtime"
    }

    fn snapshot(&self, now: Instant) -> Value {
        json!({"config": self.config, "remaining_ms": self.remaining_at(now),
            "running": self.running, "rate": self.rate, "notices": self.notices})
    }

    fn checkpoint(&self, now: Instant) -> Value {
        // 重启后暂停，停机期间不扣时间；直播中的刷新/重连不会影响计时。
        json!({"config": self.config, "remaining_ms": self.remaining_at(now), "rate": self.rate})
    }

    fn restore(&mut self, value: Value, now: Instant) -> Result<(), String> {
        let saved: SavedTimer = serde_json::from_value(value).map_err(|e| e.to_string())?;
        saved.config.validate()?;
        validate_value(Action::SetTime, saved.remaining_ms / 1000.0)?;
        validate_value(Action::SetRate, saved.rate)?;
        self.config = saved.config;
        self.remaining_ms = saved.remaining_ms;
        self.rate = saved.rate;
        self.running = false;
        self.last_tick = now;
        Ok(())
    }

    fn request(&mut self, request: Value, now: Instant) -> Result<(), String> {
        let request: TimerRequest =
            serde_json::from_value(request).map_err(|e| format!("无效的加班机操作：{e}"))?;
        // 先校验，失败不能改变正在运行的状态。
        match &request {
            TimerRequest::Configure { config } => config.validate()?,
            TimerRequest::Apply {
                action: Action::Random,
                ..
            } => {
                return Err("请在礼物规则中配置随机操作与范围".into());
            }
            TimerRequest::Apply { action, value } => validate_value(*action, *value)?,
            TimerRequest::Start if !self.config.enabled => {
                return Err("请先启用加班机并保存设置".into())
            }
            _ => {}
        }
        self.settle(now);
        match request {
            TimerRequest::Configure { config } => {
                self.config = config;
                if !self.config.enabled {
                    self.running = false;
                }
            }
            TimerRequest::Start => self.running = true,
            TimerRequest::Pause => self.running = false,
            TimerRequest::Reset => {
                self.remaining_ms = self.config.initial_seconds * 1000.0;
                self.rate = 1.0;
                self.running = false;
                self.notices.clear();
            }
            TimerRequest::Apply { action, value } => self.apply(action, value, 1),
        }
        Ok(())
    }

    fn on_gift(&mut self, gift: &ReceivedGift, now: Instant) -> bool {
        if !self.config.enabled || gift.num == 0 {
            return false;
        }
        let rules: Vec<_> = self
            .config
            .rules
            .iter()
            .filter(|rule| {
                rule.enabled
                    && match rule.gift_id {
                        Some(id) => id == gift.gift_id,
                        None => rule.gift_name.trim() == gift.gift_name,
                    }
            })
            .cloned()
            .collect();
        if rules.is_empty() {
            return false;
        }
        self.settle(now);
        let before = self.remaining_ms;
        let mut actions = Vec::new();
        let mut results = Vec::new();
        let mut rng = rand::thread_rng();
        for rule in rules {
            let (action, value) = if rule.action == Action::Random {
                let range = &rule.random_ranges[rng.gen_range(0..rule.random_ranges.len())];
                (range.action, range.draw(&mut rng))
            } else {
                (rule.action, rule.value)
            };
            // 每条礼物通知只抽一次，按个执行时复用抽出的值；快照/重连不抽签。
            let count = if rule.per_gift { gift.num } else { 1 };
            let before_action = self.remaining_ms;
            self.apply(action, value, count);
            actions.push(action);
            results.push(AppliedAction {
                action,
                value,
                count,
                random: rule.action == Action::Random,
                delta_ms: self.remaining_ms - before_action,
            });
        }
        self.sequence += 1;
        self.notices.push(GiftNotice {
            id: self.sequence,
            gift_name: gift.gift_name.clone(),
            sender_name: gift.sender_name.clone(),
            num: gift.num,
            delta_ms: self.remaining_ms - before,
            actions,
            results,
        });
        if self.notices.len() > 20 {
            self.notices.remove(0);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn timer(now: Instant) -> Overtime {
        let mut timer = Overtime::new(now);
        timer.config.enabled = true;
        timer
    }
    fn gift(num: u32) -> ReceivedGift {
        ReceivedGift {
            gift_id: 1,
            gift_name: "小心心".into(),
            sender_name: "测试用户".into(),
            num,
        }
    }
    fn rule(action: Action, value: f64) -> GiftRule {
        GiftRule {
            id: "one".into(),
            enabled: true,
            gift_id: Some(1),
            gift_name: "小心心".into(),
            action,
            value,
            per_gift: true,
            random_ranges: Vec::new(),
        }
    }
    #[test]
    fn monotonic_countdown_pause_rate_and_refresh() {
        let now = Instant::now();
        let mut t = timer(now);
        t.request(json!({"type":"start"}), now).unwrap();
        let later = now + Duration::from_secs(3);
        assert_eq!(t.snapshot(later)["remaining_ms"], 3_597_000.0);
        assert_eq!(t.snapshot(later), t.snapshot(later));
        t.request(json!({"type":"apply","action":"set_rate","value":2}), later)
            .unwrap();
        t.request(json!({"type":"pause"}), later + Duration::from_secs(2))
            .unwrap();
        assert_eq!(t.remaining_at(later + Duration::from_secs(50)), 3_593_000.0);
    }
    #[test]
    fn filters_gifts_and_counts_incremental_quantity() {
        let now = Instant::now();
        let mut t = timer(now);
        t.config.rules.push(rule(Action::Add, 10.0));
        let mut unrelated = gift(2);
        unrelated.gift_id = 2;
        assert!(!t.on_gift(&unrelated, now));
        assert!(t.on_gift(&gift(2), now));
        assert!(t.on_gift(&gift(3), now));
        assert_eq!(t.remaining_ms, 3_650_000.0);
        t.config.enabled = false;
        assert!(!t.on_gift(&gift(2), now));
    }
    #[test]
    fn actions_clamp_and_zero_does_not_produce_nan() {
        let now = Instant::now();
        let mut t = timer(now);
        t.apply(Action::Multiply, 2.0, 3);
        assert_eq!(t.remaining_ms, 28_800_000.0);
        t.apply(Action::Divide, 2.0, 3);
        assert_eq!(t.remaining_ms, 3_600_000.0);
        t.apply(Action::Subtract, 4000.0, 1);
        assert_eq!(t.remaining_ms, 0.0);
        t.apply(Action::Multiply, 100.0, u32::MAX);
        assert_eq!(t.remaining_ms, 0.0);
        t.apply(Action::SetTime, 100.0, 3);
        t.apply(Action::Multiply, 100.0, u32::MAX);
        assert_eq!(t.remaining_ms, MAX_SECONDS * 1000.0);
        t.apply(Action::Clear, 0.0, 1);
        assert_eq!(t.remaining_ms, 0.0);
        assert!(validate_value(Action::Divide, 0.0).is_err());
        assert!(validate_value(Action::Add, f64::NAN).is_err());
    }
    #[test]
    fn restore_pauses_and_configuration_does_not_reset_clock() {
        let now = Instant::now();
        let mut t = timer(now);
        t.running = true;
        let later = now + Duration::from_secs(10);
        let mut config = t.config.clone();
        config.initial_seconds = 7200.0;
        t.request(json!({"type":"configure","config":config}), later)
            .unwrap();
        assert_eq!(t.remaining_ms, 3_590_000.0);
        let mut restored = timer(later);
        restored.restore(t.checkpoint(later), later).unwrap();
        assert!(!restored.running);
        assert_eq!(
            restored.remaining_at(later + Duration::from_secs(30)),
            3_590_000.0
        );
        restored.request(json!({"type":"reset"}), later).unwrap();
        assert_eq!(restored.remaining_ms, 7_200_000.0);
    }
    #[test]
    fn one_action_per_packet_and_name_matching() {
        let now = Instant::now();
        let mut t = timer(now);
        let mut r = rule(Action::Add, 10.0);
        r.per_gift = false;
        r.gift_id = None;
        t.config.rules.push(r);
        t.on_gift(&gift(100), now);
        assert_eq!(t.remaining_ms, 3_610_000.0);
        t.config.rules[0].action = Action::SetRate;
        t.config.rules[0].value = 3.0;
        t.on_gift(&gift(100), now);
        assert_eq!(t.rate, 3.0);
    }
    #[test]
    fn random_ranges_validate_boundaries_and_legacy_rules_remain_fixed() {
        for range in [
            RandomRange {
                action: Action::Add,
                min: 20.0,
                max: 10.0,
            },
            RandomRange {
                action: Action::Divide,
                min: 0.0,
                max: 2.0,
            },
            RandomRange {
                action: Action::SetTime,
                min: 1.0,
                max: 2.0,
            },
            RandomRange {
                action: Action::Add,
                min: 0.5,
                max: 1.0,
            },
            RandomRange {
                action: Action::Multiply,
                min: 1.001,
                max: 2.0,
            },
            RandomRange {
                action: Action::Add,
                min: f64::NAN,
                max: 2.0,
            },
        ] {
            assert!(range.validate().is_err());
        }
        let old: GiftRule = serde_json::from_value(json!({"id":"old","enabled":true,"gift_id":1,
            "gift_name":"小心心","action":"add","value":60,"per_gift":true}))
        .unwrap();
        assert!(old.random_ranges.is_empty());
        let mut config = TimerConfig::default();
        let mut random = rule(Action::Random, 0.0);
        config.rules.push(random.clone());
        assert!(config.validate().is_err());
        random.random_ranges = vec![RandomRange {
            action: Action::Add,
            min: 0.0,
            max: 0.0,
        }];
        config.rules[0] = random.clone();
        assert!(config.validate().is_ok());
        random.random_ranges.push(random.random_ranges[0].clone());
        config.rules[0] = random;
        assert!(config.validate().is_err());
    }

    #[test]
    fn random_sampling_covers_inclusive_endpoints_and_decimal_factors() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for range in [
            RandomRange {
                action: Action::Add,
                min: 2.0,
                max: 3.0,
            },
            RandomRange {
                action: Action::Divide,
                min: 0.99,
                max: 1.01,
            },
        ] {
            let mut saw_min = false;
            let mut saw_max = false;
            for _ in 0..200 {
                let value = range.draw(&mut rng);
                assert!((range.min..=range.max).contains(&value));
                saw_min |= value == range.min;
                saw_max |= value == range.max;
            }
            assert!(saw_min && saw_max);
        }
    }

    #[test]
    fn random_operations_use_server_result_and_respect_quantity() {
        let now = Instant::now();
        for (action, value, expected) in [
            (Action::Add, 10.0, 3_620_000.0),
            (Action::Subtract, 10.0, 3_580_000.0),
            (Action::Multiply, 2.0, 14_400_000.0),
            (Action::Divide, 2.0, 900_000.0),
        ] {
            let mut timer = timer(now);
            let mut random = rule(Action::Random, 0.0);
            random.random_ranges.push(RandomRange {
                action,
                min: value,
                max: value,
            });
            timer.config.rules.push(random);
            assert!(timer.on_gift(&gift(2), now));
            let state = timer.snapshot(now);
            assert_eq!(state["remaining_ms"], expected);
            assert_eq!(state["notices"][0]["results"][0]["value"], value);
            assert_eq!(state["notices"][0]["results"][0]["random"], true);
            assert_eq!(timer.snapshot(now), state);
            timer.config.rules[0].per_gift = false;
            timer.request(json!({"type":"reset"}), now).unwrap();
            timer.on_gift(&gift(100), now);
            assert_eq!(timer.snapshot(now)["notices"][0]["results"][0]["count"], 1);
        }
        let mut timer = timer(now);
        assert!(timer
            .request(json!({"type":"apply","action":"random","value":1}), now)
            .is_err());
    }
}
