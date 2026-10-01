//! 点歌队列：后台接收、优先插队、手动调整和已唱历史共用一份持久化状态。
use super::Extension;
use crate::live_events::{ReceivedGift, ReceivedText, TextSource};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashSet, VecDeque},
    time::Instant,
};

const MAX_PENDING: usize = 1000;
const MAX_SUNG: usize = 1000;
const MAX_SEEN_EVENTS: usize = 4096;

fn current_time_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudienceRule {
    pub enabled: bool,
    pub limit: u32,
    pub cooldown_secs: u32,
}
impl AudienceRule {
    fn new(limit: u32) -> Self {
        Self {
            enabled: true,
            limit,
            cooldown_secs: 60,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudienceRules {
    pub normal: AudienceRule,
    pub fans: AudienceRule,
    pub captain: AudienceRule,
    pub admiral: AudienceRule,
    pub governor: AudienceRule,
}
impl Default for AudienceRules {
    fn default() -> Self {
        Self {
            normal: AudienceRule::new(10),
            fans: AudienceRule::new(10),
            captain: AudienceRule::new(20),
            admiral: AudienceRule::new(30),
            governor: AudienceRule::new(40),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GiftBonusRule {
    pub id: String,
    pub gift_id: Option<u64>,
    pub gift_name: String,
    pub gifts_required: u32,
    pub extra_requests: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UserAllowance {
    used: u32,
    bonus_remaining: u32,
    last_request_ms: Option<i64>,
    gift_progress: BTreeMap<String, u64>,
}
#[derive(Debug, Default, Serialize, Deserialize)]
struct SongSession {
    room_id: u64,
    streamer_uid: u64,
    live_start: Option<i64>,
    started_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Priorities {
    pub superchat: u16,
    pub governor: u16,
    pub admiral: u16,
    pub captain: u16,
}
impl Default for Priorities {
    fn default() -> Self {
        Self {
            superchat: 100,
            governor: 80,
            admiral: 60,
            captain: 40,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SongConfig {
    pub enabled: bool,
    pub accept_danmaku: bool,
    pub accept_superchat: bool,
    pub command: String,
    pub deduplicate: bool,
    pub priorities: Priorities,
    pub audience: AudienceRules,
    pub gift_bonus_enabled: bool,
    pub gift_rules: Vec<GiftBonusRule>,
    /// 设置界面以元为单位；SC 事件仍以电池为单位。
    pub sc_min_price: f64,
    pub sc_content_as_song: bool,
    pub sc_sort_by_price: bool,
    pub overlay_max_rows: u8,
    pub overlay_show_username: bool,
}
impl Default for SongConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            accept_danmaku: true,
            accept_superchat: true,
            command: "点歌".into(),
            deduplicate: true,
            priorities: Priorities::default(),
            audience: AudienceRules::default(),
            gift_bonus_enabled: false,
            gift_rules: Vec::new(),
            sc_min_price: 0.0,
            sc_content_as_song: true,
            sc_sort_by_price: true,
            overlay_max_rows: 8,
            overlay_show_username: true,
        }
    }
}
impl SongConfig {
    fn validate(&mut self) -> Result<(), String> {
        self.command = self.command.trim().to_owned();
        if self.command.is_empty()
            || self.command.chars().count() > 20
            || self.command.chars().any(char::is_whitespace)
        {
            return Err("点歌口令须为 1–20 个字符，且不能包含空白".into());
        }
        for rule in [
            &self.audience.normal,
            &self.audience.fans,
            &self.audience.captain,
            &self.audience.admiral,
            &self.audience.governor,
        ] {
            if rule.limit > 10000 || rule.cooldown_secs > 86400 {
                return Err("每场次数须为 0–10000，冷却时间须为 0–86400 秒".into());
            }
        }
        if !self.sc_min_price.is_finite() || !(0.0..=1_000_000.0).contains(&self.sc_min_price) {
            return Err("SC 最低金额须为 0–1000000 元".into());
        }
        if self.gift_rules.len() > 10 {
            return Err("最多配置 10 种礼物".into());
        }
        let mut ids = HashSet::new();
        let mut gifts = HashSet::new();
        for rule in &mut self.gift_rules {
            rule.gift_name = rule.gift_name.trim().to_owned();
            if rule.id.is_empty()
                || !ids.insert(rule.id.clone())
                || rule.gift_id == Some(0)
                || rule.gift_name.is_empty()
                || rule.gift_name.chars().count() > 80
                || !(1..=10000).contains(&rule.gifts_required)
                || !(1..=10000).contains(&rule.extra_requests)
            {
                return Err("礼物规则需填写唯一标识、礼物名称，个数和增加次数须为 1–10000".into());
            }
            let gift_key = rule.gift_id.map_or_else(
                || format!("name:{}", rule.gift_name),
                |id| format!("id:{id}"),
            );
            if !gifts.insert(gift_key) {
                return Err("同一种礼物只能配置一条规则".into());
            }
        }
        if !(1..=30).contains(&self.overlay_max_rows) {
            return Err("OBS 显示条数须为 1–30".into());
        }
        if [
            self.priorities.superchat,
            self.priorities.governor,
            self.priorities.admiral,
            self.priorities.captain,
        ]
        .iter()
        .any(|&p| p > 1000)
        {
            return Err("优先级须为 0–1000 的整数".into());
        }
        Ok(())
    }
    fn priority(&self, item: &SongRequestItem) -> (u16, u64) {
        let guard = match item.guard_level {
            1 => self.priorities.governor,
            2 => self.priorities.admiral,
            3 => self.priorities.captain,
            _ => 0,
        };
        let priority = guard.max(if item.source == TextSource::Superchat {
            self.priorities.superchat
        } else {
            0
        });
        (
            priority,
            if self.priorities.superchat > 0
                && self.sc_sort_by_price
                && item.source == TextSource::Superchat
            {
                item.sc_price.unwrap_or(0)
            } else {
                0
            },
        )
    }
    fn song_name(&self, content: &str) -> Option<String> {
        let rest = content.trim().strip_prefix(&self.command)?;
        // 只识别开头的独立口令，支持半角/全角空格与冒号，避免把“点歌机”当歌名。
        if !rest.starts_with(|c: char| c.is_whitespace() || c == ':' || c == '：') {
            return None;
        }
        let name = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '：');
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        (!name.is_empty() && name.chars().count() <= 120).then_some(name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongRequestItem {
    pub id: String,
    pub song_name: String,
    pub username: String,
    pub uid: u64,
    pub source: TextSource,
    pub guard_level: u8,
    /// 电池，与直播数据保持一致。
    pub sc_price: Option<u64>,
    pub timestamp: i64,
    pub sung: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct SongRequestManager {
    config: SongConfig,
    requests: Vec<SongRequestItem>,
    #[serde(default)]
    seen_events: VecDeque<String>,
    #[serde(default)]
    allowances: BTreeMap<u64, UserAllowance>,
    #[serde(default)]
    session: SongSession,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    Configure {
        config: SongConfig,
    },
    MarkSung {
        request_id: String,
        sung: bool,
    },
    Move {
        request_id: String,
        target_id: String,
        placement: Placement,
    },
    Remove {
        request_id: String,
    },
    ClearSung,
    ClearAll,
    SortPriority,
    NewSession,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Placement {
    Before,
    After,
}

impl SongRequestManager {
    fn pending_count(&self) -> usize {
        self.requests.iter().filter(|r| !r.sung).count()
    }
    fn insert_pending(&mut self, item: SongRequestItem) {
        let priority = self.config.priority(&item);
        // 仅插入新请求，不重排已手动调整的条目；同优先级保持先到先唱。
        let index = self
            .requests
            .iter()
            .position(|r| r.sung || self.config.priority(r) < priority)
            .unwrap_or(self.requests.len());
        self.requests.insert(index, item);
    }
    fn sort_priority(&mut self) {
        self.requests.sort_by_key(|r| {
            (
                r.sung,
                if r.sung {
                    Reverse((0, 0))
                } else {
                    Reverse(self.config.priority(r))
                },
            )
        });
    }
    fn index(&self, id: &str) -> Result<usize, String> {
        self.requests
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| "点歌已被删除，请刷新队列".into())
    }
    fn audience_rule(&self, text: &ReceivedText) -> &AudienceRule {
        match text.guard_level {
            1 => &self.config.audience.governor,
            2 => &self.config.audience.admiral,
            3 => &self.config.audience.captain,
            _ if (text.medal_anchor_uid > 0
                && text.medal_anchor_uid == self.session.streamer_uid)
                || (text.medal_anchor_uid == 0
                    && text.medal_room_id > 0
                    && text.medal_room_id == self.session.room_id) =>
            {
                &self.config.audience.fans
            }
            _ => &self.config.audience.normal,
        }
    }
    fn remember(&mut self, key: String) {
        self.seen_events.push_back(key);
        if self.seen_events.len() > MAX_SEEN_EVENTS {
            self.seen_events.pop_front();
        }
    }
    fn new_session(&mut self) {
        self.allowances.clear();
        self.session.started_at = current_time_ms();
    }
    fn accept_text(&mut self, text: &ReceivedText, now_ms: i64) -> bool {
        if !self.config.enabled
            || (text.source == TextSource::Danmaku && !self.config.accept_danmaku)
            || (text.source == TextSource::Superchat && !self.config.accept_superchat)
        {
            return false;
        }
        let is_sc = text.source == TextSource::Superchat;
        if is_sc && (text.sc_price.unwrap_or(0) as f64) < self.config.sc_min_price * 10.0 {
            return false;
        }
        let song_name = self.config.song_name(&text.content).or_else(|| {
            // 开启直接点歌时，带口令的 SC 仍去除口令；不把空口令当成歌名。
            let empty_command = text
                .content
                .trim()
                .strip_prefix(&self.config.command)
                .is_some_and(|rest| {
                    rest.trim_matches(|c: char| c.is_whitespace() || c == ':' || c == '：')
                        .is_empty()
                });
            if !is_sc || !self.config.sc_content_as_song || empty_command {
                return None;
            }
            let name = text
                .content
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            (!name.is_empty() && name.chars().count() <= 120).then_some(name)
        });
        let Some(song_name) = song_name else {
            return false;
        };
        if self.pending_count() >= MAX_PENDING {
            return false;
        }
        let key = format!(
            "text:{}:{:?}:{}:{}",
            self.session.room_id, text.source, text.event_id, text.content
        );
        if self.seen_events.contains(&key) {
            return false;
        }
        if self.config.deduplicate
            && self.requests.iter().any(|r| {
                !r.sung
                    && r.uid == text.uid
                    && r.song_name.to_lowercase() == song_name.to_lowercase()
            })
        {
            return false;
        }
        if !is_sc {
            let rule = self.audience_rule(text).clone();
            if !rule.enabled || rule.limit == 0 {
                return false;
            }
            let allowance = self.allowances.entry(text.uid).or_default();
            if (rule.cooldown_secs > 0
                && allowance.last_request_ms.is_some_and(|last| {
                    now_ms.saturating_sub(last) < i64::from(rule.cooldown_secs) * 1000
                }))
                || (allowance.used >= rule.limit && allowance.bonus_remaining == 0)
            {
                return false;
            }
            if allowance.used < rule.limit {
                allowance.used += 1;
            } else {
                allowance.bonus_remaining -= 1;
            }
            allowance.last_request_ms = Some(now_ms);
        }
        self.remember(key);
        self.insert_pending(SongRequestItem {
            id: super::new_id("song"),
            song_name,
            username: text.username.clone(),
            uid: text.uid,
            source: text.source,
            guard_level: if text.guard_level <= 3 {
                text.guard_level
            } else {
                0
            },
            sc_price: text.sc_price,
            timestamp: if text.timestamp < 1_000_000_000_000 {
                text.timestamp.saturating_mul(1000)
            } else {
                text.timestamp
            },
            sung: false,
        });
        true
    }
}

impl Extension for SongRequestManager {
    fn id(&self) -> &'static str {
        "song-request"
    }
    fn snapshot(&self, _now: Instant) -> Value {
        json!({ "config": self.config, "requests": self.requests, "session": self.session })
    }
    fn browser_visible(&self) -> bool {
        true
    }
    fn browser_snapshot(&self, _now: Instant) -> Value {
        let rows: Vec<_> = self.requests.iter().filter(|r| !r.sung).take(self.config.overlay_max_rows as usize).map(|r| {
            json!({ "id": r.id, "song_name": r.song_name, "username": if self.config.overlay_show_username { &r.username } else { "" } })
        }).collect();
        json!({ "show_username": self.config.overlay_show_username, "total": self.pending_count(), "requests": rows })
    }
    fn checkpoint(&self, _now: Instant) -> Value {
        json!(self)
    }
    fn restore(&mut self, value: Value, _now: Instant) -> Result<(), String> {
        let mut restored: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        restored.config.validate()?;
        let mut ids = HashSet::new();
        if restored.requests.iter().any(|r| {
            r.id.is_empty()
                || !ids.insert(&r.id)
                || r.song_name.trim().is_empty()
                || r.song_name.chars().count() > 120
                || r.guard_level > 3
        }) || restored.pending_count() > MAX_PENDING
            || restored.requests.len() - restored.pending_count() > MAX_SUNG
            || restored.seen_events.len() > MAX_SEEN_EVENTS
        {
            return Err("点歌记录无效或超出容量".into());
        }
        // 只分离历史，保留人工排序。
        restored.requests.sort_by_key(|r| r.sung);
        *self = restored;
        Ok(())
    }
    fn request(&mut self, request: Value, _now: Instant) -> Result<Value, String> {
        match serde_json::from_value::<Request>(request).map_err(|e| e.to_string())? {
            Request::Configure { mut config } => {
                config.validate()?;
                let priorities_changed = self.config.priorities != config.priorities
                    || self.config.sc_sort_by_price != config.sc_sort_by_price;
                // 修改规则时仅清除该规则未兑换的余数，已获得次数保留。
                for allowance in self.allowances.values_mut() {
                    allowance.gift_progress.retain(|id, _| {
                        self.config
                            .gift_rules
                            .iter()
                            .find(|r| &r.id == id)
                            .is_some_and(|old| config.gift_rules.iter().any(|new| old == new))
                    });
                }
                self.config = config;
                if priorities_changed {
                    self.sort_priority();
                }
            }
            Request::MarkSung { request_id, sung } => {
                let index = self.index(&request_id)?;
                if self.requests[index].sung != sung {
                    if !sung && self.pending_count() >= MAX_PENDING {
                        return Err("待唱队列已满（1000 首）".into());
                    }
                    let mut item = self.requests.remove(index);
                    item.sung = sung;
                    if sung {
                        self.requests.push(item);
                        if self.requests.len() - self.pending_count() > MAX_SUNG {
                            let oldest = self.requests.iter().position(|r| r.sung).unwrap();
                            self.requests.remove(oldest);
                        }
                    } else {
                        self.insert_pending(item);
                    }
                }
            }
            Request::Move {
                request_id,
                target_id,
                placement,
            } => {
                let from = self.index(&request_id)?;
                let target = self.index(&target_id)?;
                if self.requests[from].sung || self.requests[target].sung {
                    return Err("只能调整待唱歌曲的顺序".into());
                }
                if from != target {
                    // 验证通过再变更，使用相邻条目 ID，避免并发新增时覆盖整份队列。
                    let item = self.requests.remove(from);
                    let target = if from < target { target - 1 } else { target };
                    self.requests.insert(
                        target + usize::from(matches!(placement, Placement::After)),
                        item,
                    );
                }
            }
            Request::Remove { request_id } => {
                let index = self.index(&request_id)?;
                self.requests.remove(index);
            }
            Request::ClearSung => self.requests.retain(|r| !r.sung),
            Request::ClearAll => self.requests.clear(),
            Request::SortPriority => self.sort_priority(),
            Request::NewSession => self.new_session(),
        }
        Ok(Value::Null)
    }
    fn on_text(&mut self, text: &ReceivedText, _now: Instant) -> bool {
        self.accept_text(text, current_time_ms())
    }
    fn on_gift(&mut self, gift: &ReceivedGift, _now: Instant) -> bool {
        if !self.config.enabled
            || !self.config.gift_bonus_enabled
            || gift.num == 0
            || gift.sender_uid == 0
        {
            return false;
        }
        let key = gift
            .event_id
            .as_ref()
            .map(|id| format!("bonus:{}:{id}", self.session.room_id));
        if key
            .as_ref()
            .is_some_and(|key| self.seen_events.contains(key))
        {
            return false;
        }
        // 直接处理盲盒本身，使用本条增量数量，不再匹配爆出礼物。
        let (gift_id, gift_name) = match &gift.blind_gift {
            Some(blind) => (blind.gift_id, &blind.gift_name),
            None => (gift.gift_id, &gift.gift_name),
        };
        // ID 匹配优先于自定义名称，避免同一礼物被重复兑换。
        let rule = self
            .config
            .gift_rules
            .iter()
            .find(|r| r.gift_id == Some(gift_id))
            .or_else(|| {
                self.config
                    .gift_rules
                    .iter()
                    .find(|r| r.gift_id.is_none() && &r.gift_name == gift_name)
            });
        let Some(rule) = rule else {
            return false;
        };
        let allowance = self.allowances.entry(gift.sender_uid).or_default();
        let progress = allowance.gift_progress.entry(rule.id.clone()).or_default();
        let total = progress.saturating_add(u64::from(gift.num));
        let bonus =
            (total / u64::from(rule.gifts_required)).saturating_mul(u64::from(rule.extra_requests));
        *progress = total % u64::from(rule.gifts_required);
        allowance.bonus_remaining = allowance
            .bonus_remaining
            .saturating_add(bonus.min(u64::from(u32::MAX)) as u32);
        if let Some(key) = key {
            self.remember(key);
        }
        true
    }
    fn on_room(&mut self, room_id: u64, streamer_uid: u64, live_start: Option<i64>) -> bool {
        if room_id == 0 {
            return false;
        }
        let new_room = self.session.room_id != room_id;
        let new_live = live_start
            .filter(|&time| time > 0)
            .is_some_and(|time| self.session.live_start != Some(time));
        let changed = new_room || new_live || self.session.streamer_uid != streamer_uid;
        if new_room {
            self.session.live_start = None;
        }
        if new_room || new_live {
            self.new_session();
        }
        self.session.room_id = room_id;
        self.session.streamer_uid = streamer_uid;
        if let Some(time) = live_start.filter(|&time| time > 0) {
            self.session.live_start = Some(time);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(name: &str, uid: u64, source: TextSource, guard_level: u8) -> ReceivedText {
        ReceivedText {
            event_id: super::super::new_id("event"),
            content: name.into(),
            username: format!("用户{uid}"),
            uid,
            guard_level,
            medal_anchor_uid: 0,
            medal_room_id: 0,
            timestamp: 1700000000,
            source,
            sc_price: (source == TextSource::Superchat).then_some(300),
        }
    }
    fn add(manager: &mut SongRequestManager, name: &str, source: TextSource, guard: u8) -> String {
        let event = text(
            &format!("点歌 {name}"),
            42 + manager.requests.len() as u64,
            source,
            guard,
        );
        assert!(manager.on_text(&event, Instant::now()));
        manager
            .requests
            .iter()
            .find(|r| r.song_name == name)
            .unwrap()
            .id
            .clone()
    }
    fn names(manager: &SongRequestManager) -> Vec<&str> {
        manager
            .requests
            .iter()
            .map(|r| r.song_name.as_str())
            .collect()
    }
    fn request(manager: &mut SongRequestManager, value: Value) {
        manager.request(value, Instant::now()).unwrap();
    }

    #[test]
    fn command_boundaries_sources_and_deduplication() {
        let mut manager = SongRequestManager::default();
        for content in [
            "点歌",
            "点歌   ",
            "来点歌 晴天",
            "点歌机",
            "点歌晴天",
            "点歌：",
        ] {
            assert!(!manager.on_text(&text(content, 1, TextSource::Danmaku, 0), Instant::now()));
        }
        let event = text("  点歌：  晴天  live  ", 1, TextSource::Superchat, 3);
        assert!(manager.on_text(&event, Instant::now()));
        assert_eq!(names(&manager), ["晴天 live"]);
        assert_eq!(manager.requests[0].timestamp, 1700000000000);
        assert_eq!(manager.requests[0].sc_price, Some(300));
        assert!(!manager.on_text(&event, Instant::now()));
        assert!(!manager.on_text(
            &text("点歌 晴天 live", 1, TextSource::Danmaku, 0),
            Instant::now()
        ));
        assert!(manager.on_text(
            &text("点歌 晴天 live", 2, TextSource::Danmaku, 0),
            Instant::now()
        ));
        manager.config.deduplicate = false;
        assert!(manager.on_text(
            &text("点歌 晴天 live", 1, TextSource::Danmaku, 0),
            Instant::now()
        ));
        manager.config.accept_danmaku = false;
        assert!(!manager.on_text(
            &text("点歌 其他", 3, TextSource::Danmaku, 0),
            Instant::now()
        ));
        manager.config.accept_superchat = false;
        assert!(!manager.on_text(
            &text("点歌 其他", 3, TextSource::Superchat, 0),
            Instant::now()
        ));
        manager.config.enabled = false;
        manager.config.accept_superchat = true;
        assert!(!manager.on_text(
            &text("点歌 其他", 3, TextSource::Superchat, 0),
            Instant::now()
        ));
        manager.config.enabled = true;
        manager.config.command = "来首".into();
        assert!(manager.on_text(
            &text("来首　稻香", 3, TextSource::Superchat, 0),
            Instant::now()
        ));
    }

    #[test]
    fn priority_fifo_manual_order_and_history_restore() {
        let mut manager = SongRequestManager::default();
        let normal = add(&mut manager, "普通", TextSource::Danmaku, 0);
        let captain = add(&mut manager, "舰长", TextSource::Danmaku, 3);
        add(&mut manager, "提督", TextSource::Danmaku, 2);
        add(&mut manager, "总督", TextSource::Danmaku, 1);
        let sc = add(&mut manager, "SC", TextSource::Superchat, 0);
        add(&mut manager, "舰长SC", TextSource::Superchat, 3);
        assert_eq!(
            names(&manager),
            ["SC", "舰长SC", "总督", "提督", "舰长", "普通"]
        );
        request(
            &mut manager,
            json!({"type":"move","request_id":normal,"target_id":sc,"placement":"before"}),
        );
        add(&mut manager, "新普通", TextSource::Danmaku, 0);
        assert_eq!(names(&manager)[0], "普通");
        request(
            &mut manager,
            json!({"type":"mark_sung","request_id":normal,"sung":true}),
        );
        assert!(manager.requests.last().unwrap().sung);
        let mut restored = SongRequestManager::default();
        restored
            .restore(manager.checkpoint(Instant::now()), Instant::now())
            .unwrap();
        assert_eq!(names(&manager), names(&restored));
        request(
            &mut restored,
            json!({"type":"mark_sung","request_id":normal,"sung":false}),
        );
        assert_eq!(
            names(&restored),
            ["SC", "舰长SC", "总督", "提督", "舰长", "新普通", "普通"]
        );
        request(
            &mut restored,
            json!({"type":"move","request_id":sc,"target_id":captain,"placement":"after"}),
        );
        assert_eq!(names(&restored)[4], "SC");
        let before = restored.checkpoint(Instant::now());
        assert!(restored
            .request(
                json!({"type":"move","request_id":sc,"target_id":"missing","placement":"before"}),
                Instant::now()
            )
            .is_err());
        assert_eq!(before, restored.checkpoint(Instant::now()));
        request(&mut restored, json!({"type":"sort_priority"}));
        assert_eq!(names(&restored)[..2], ["舰长SC", "SC"]);
    }

    #[test]
    fn config_validation_sorting_and_private_browser_snapshot() {
        let mut manager = SongRequestManager::default();
        let normal = add(&mut manager, "普通", TextSource::Danmaku, 0);
        add(&mut manager, "舰长", TextSource::Danmaku, 3);
        let sc = add(&mut manager, "SC", TextSource::Superchat, 0);
        request(
            &mut manager,
            json!({"type":"move","request_id":normal,"target_id":sc,"placement":"before"}),
        );
        let mut config = manager.config.clone();
        config.overlay_max_rows = 1;
        config.overlay_show_username = false;
        request(&mut manager, json!({"type":"configure","config":config}));
        assert_eq!(names(&manager)[0], "普通");
        config.priorities.captain = 200;
        request(&mut manager, json!({"type":"configure","config":config}));
        assert_eq!(names(&manager), ["舰长", "SC", "普通"]);
        let public = manager.browser_snapshot(Instant::now());
        assert_eq!(public["total"], 3);
        assert_eq!(public["requests"].as_array().unwrap().len(), 1);
        assert_eq!(public["requests"][0]["username"], "");
        assert!(public["requests"][0].get("uid").is_none());
        assert!(public.get("config").is_none());
        let before = manager.checkpoint(Instant::now());
        for bad in [
            json!({"command":""}),
            json!({"command":"点 歌"}),
            json!({"overlay_max_rows":0}),
            json!({"priorities":{"captain":1001}}),
        ] {
            assert!(manager
                .request(json!({"type":"configure","config":bad}), Instant::now())
                .is_err());
            assert_eq!(before, manager.checkpoint(Instant::now()));
        }
        request(
            &mut manager,
            json!({"type":"mark_sung","request_id":sc,"sung":true}),
        );
        assert_eq!(manager.browser_snapshot(Instant::now())["total"], 2);
        assert!(manager
            .request(
                json!({"type":"move","request_id":sc,"target_id":normal,"placement":"before"}),
                Instant::now()
            )
            .is_err());
        request(&mut manager, json!({"type":"clear_sung"}));
        assert_eq!(names(&manager), ["舰长", "普通"]);
        request(&mut manager, json!({"type":"remove","request_id":normal}));
        request(&mut manager, json!({"type":"clear_all"}));
        assert!(manager.requests.is_empty());
    }

    #[test]
    fn audience_identity_quotas_follow_current_room_and_do_not_refund_on_delete() {
        let mut manager = SongRequestManager::default();
        manager.on_room(100, 900, None);
        manager.config.audience.normal = AudienceRule {
            enabled: true,
            limit: 1,
            cooldown_secs: 0,
        };
        manager.config.audience.fans = AudienceRule {
            enabled: true,
            limit: 2,
            cooldown_secs: 0,
        };
        manager.config.audience.captain = AudienceRule {
            enabled: true,
            limit: 3,
            cooldown_secs: 0,
        };
        let normal = text("点歌 普通", 1, TextSource::Danmaku, 0);
        assert!(manager.accept_text(&normal, 0));
        request(&mut manager, json!({"type":"clear_all"}));
        assert!(!manager.accept_text(&text("点歌 超限", 1, TextSource::Danmaku, 0), 1));
        let mut fan = text("点歌 粉丝一", 2, TextSource::Danmaku, 0);
        fan.medal_anchor_uid = 900;
        assert!(manager.accept_text(&fan, 0));
        fan.content = "点歌 粉丝二".into();
        assert!(manager.accept_text(&fan, 1));
        fan.content = "点歌 粉丝三".into();
        assert!(!manager.accept_text(&fan, 2));
        // 带其他直播间的勋章不能获得本房间粉丝次数。
        fan.uid = 3;
        fan.medal_anchor_uid = 901;
        fan.medal_room_id = 100;
        assert!(manager.accept_text(&fan, 0));
        fan.content = "点歌 别房勋章".into();
        assert!(!manager.accept_text(&fan, 1));
        // 舰长使用舰长规则；关闭舰长后不能回退到粉丝规则。
        fan.uid = 4;
        fan.guard_level = 3;
        fan.medal_anchor_uid = 900;
        assert_eq!(manager.audience_rule(&fan).limit, 3);
        manager.config.audience.captain.enabled = false;
        assert!(!manager.accept_text(&fan, 0));
        manager.config.audience.captain.enabled = true;
        manager.config.audience.captain.limit = 0;
        assert!(!manager.accept_text(&fan, 0));
        fan.source = TextSource::Superchat;
        fan.sc_price = Some(300);
        assert!(manager.accept_text(&fan, 0));
        assert!(!manager.allowances.contains_key(&4));
    }

    #[test]
    fn cooldown_sc_threshold_direct_content_and_amount_ordering() {
        let mut manager = SongRequestManager::default();
        assert!(manager.accept_text(&text("点歌 第一首", 1, TextSource::Danmaku, 0), 1000));
        assert!(!manager.accept_text(&text("点歌 第二首", 1, TextSource::Danmaku, 0), 60999));
        assert!(manager.accept_text(&text("点歌 第二首", 1, TextSource::Danmaku, 0), 61000));
        manager.config.sc_min_price = 30.5;
        let mut sc = text("直接写歌名", 1, TextSource::Superchat, 0);
        assert!(!manager.accept_text(&sc, 61001));
        sc.sc_price = Some(305);
        assert!(manager.accept_text(&sc, 61001));
        assert_eq!(manager.allowances[&1].used, 2);
        assert_eq!(manager.allowances[&1].last_request_ms, Some(61000));
        sc = text("更高金额", 2, TextSource::Superchat, 0);
        sc.sc_price = Some(500);
        assert!(manager.accept_text(&sc, 1));
        sc = text("同金额后到", 3, TextSource::Superchat, 0);
        sc.sc_price = Some(500);
        assert!(manager.accept_text(&sc, 1));
        assert_eq!(
            names(&manager)[..3],
            ["更高金额", "同金额后到", "直接写歌名"]
        );
        manager.config.sc_content_as_song = false;
        sc = text("不带口令", 4, TextSource::Superchat, 0);
        sc.sc_price = Some(500);
        assert!(!manager.accept_text(&sc, 1));
        sc.content = "点歌 带口令".into();
        assert!(manager.accept_text(&sc, 1));
        manager.config.sc_content_as_song = true;
        sc = text("点歌的人", 5, TextSource::Superchat, 0);
        sc.sc_price = Some(500);
        assert!(manager.accept_text(&sc, 1));
        assert!(names(&manager).contains(&"点歌的人"));
        let mut no_priority = SongRequestManager::default();
        no_priority.config.priorities.superchat = 0;
        add(&mut no_priority, "先到弹幕", TextSource::Danmaku, 0);
        add(&mut no_priority, "后到SC", TextSource::Superchat, 0);
        assert_eq!(names(&no_priority), ["先到弹幕", "后到SC"]);
        no_priority.config.audience.normal.cooldown_secs = 0;
        assert!(no_priority.accept_text(&text("点歌 关闭冷却", 42, TextSource::Danmaku, 0), 0));
    }

    #[test]
    fn gifts_accumulate_per_user_persist_and_reset_only_on_new_session() {
        let mut manager = SongRequestManager::default();
        manager.on_room(100, 900, Some(1000));
        manager.config.audience.normal = AudienceRule {
            enabled: true,
            limit: 1,
            cooldown_secs: 0,
        };
        manager.config.gift_bonus_enabled = true;
        manager.config.gift_rules.push(GiftBonusRule {
            id: "rose".into(),
            gift_id: Some(7),
            gift_name: "玫瑰".into(),
            gifts_required: 10,
            extra_requests: 2,
        });
        assert!(manager.accept_text(&text("点歌 基础", 1, TextSource::Danmaku, 0), 0));
        let mut gift = ReceivedGift {
            event_id: Some("gift1".into()),
            sender_uid: 1,
            gift_id: 7,
            gift_name: "玫瑰".into(),
            sender_name: "观众".into(),
            num: 6,
            blind_gift: None,
        };
        assert!(manager.on_gift(&gift, Instant::now()));
        let mut restored = SongRequestManager::default();
        restored
            .restore(manager.checkpoint(Instant::now()), Instant::now())
            .unwrap();
        assert!(!restored.on_gift(&gift, Instant::now()));
        assert!(!restored.on_room(100, 900, None));
        assert!(!restored.on_room(100, 900, Some(1000)));
        gift.event_id = Some("gift2".into());
        gift.num = 4;
        assert!(restored.on_gift(&gift, Instant::now()));
        assert_eq!(restored.allowances[&1].bonus_remaining, 2);
        for name in ["赠送一", "赠送二"] {
            assert!(
                restored.accept_text(&text(&format!("点歌 {name}"), 1, TextSource::Danmaku, 0), 1)
            );
        }
        assert!(!restored.accept_text(&text("点歌 赠送三", 1, TextSource::Danmaku, 0), 1));
        assert!(restored.accept_text(&text("点歌 他人基础", 2, TextSource::Danmaku, 0), 1));
        assert!(!restored.accept_text(&text("点歌 不共享赠送", 2, TextSource::Danmaku, 0), 1));
        assert!(restored.on_room(100, 900, Some(2000)));
        assert!(restored.allowances.is_empty());
        assert!(!restored.requests.is_empty());
        assert!(restored.accept_text(&text("点歌 新场", 1, TextSource::Danmaku, 0), 1));
        request(&mut restored, json!({"type":"new_session"}));
        assert!(restored.allowances.is_empty());
        assert!(restored.on_room(200, 999, None));
        assert_eq!(restored.session.streamer_uid, 999);
    }

    #[test]
    fn blind_boxes_award_original_gift_only_by_id_or_name() {
        use crate::live_events::ReceivedBlindGift;

        for match_by_id in [true, false] {
            let now = Instant::now();
            let mut manager = SongRequestManager::default();
            manager.config.gift_bonus_enabled = true;
            manager.config.gift_rules = vec![
                GiftBonusRule {
                    id: "blind".into(),
                    gift_id: match_by_id.then_some(10),
                    gift_name: "心动盲盒".into(),
                    gifts_required: 3,
                    extra_requests: 2,
                },
                GiftBonusRule {
                    id: "revealed".into(),
                    gift_id: Some(20),
                    gift_name: "棉花糖".into(),
                    gifts_required: 1,
                    extra_requests: 99,
                },
            ];
            if match_by_id {
                // 同名自定义规则不能与 ID 规则重复兑换。
                let mut name_rule = manager.config.gift_rules[0].clone();
                name_rule.id = "blind-name".into();
                name_rule.gift_id = None;
                name_rule.extra_requests = 50;
                manager.config.gift_rules.insert(0, name_rule);
            }
            let mut gift = ReceivedGift {
                event_id: Some("blind1".into()),
                sender_uid: 1,
                gift_id: 20,
                gift_name: "棉花糖".into(),
                sender_name: "观众".into(),
                num: 2,
                blind_gift: Some(ReceivedBlindGift {
                    gift_id: 10,
                    gift_name: "心动盲盒".into(),
                }),
            };
            assert!(manager.on_gift(&gift, now));
            assert_eq!(manager.allowances[&1].bonus_remaining, 0);
            assert_eq!(manager.allowances[&1].gift_progress.len(), 1);
            assert_eq!(manager.allowances[&1].gift_progress["blind"], 2);

            let mut restored = SongRequestManager::default();
            restored.restore(manager.checkpoint(now), now).unwrap();
            assert!(!restored.on_gift(&gift, now));
            gift.event_id = Some("blind2".into());
            gift.num = 1;
            assert!(restored.on_gift(&gift, now));
            assert_eq!(restored.allowances[&1].bonus_remaining, 2);
            assert_eq!(restored.allowances[&1].gift_progress["blind"], 0);

            // 未配置盲盒时不回退到爆出礼物，直接赠送普通礼物仍正常兑换。
            restored.config.gift_rules.retain(|rule| rule.id == "revealed");
            gift.event_id = Some("blind3".into());
            assert!(!restored.on_gift(&gift, now));
            assert_eq!(restored.allowances[&1].bonus_remaining, 2);
            gift.blind_gift = None;
            assert!(restored.on_gift(&gift, now));
            assert_eq!(restored.allowances[&1].bonus_remaining, 101);
        }
    }

    #[test]
    fn old_checkpoints_gain_defaults_and_invalid_gift_rules_are_atomic() {
        let mut manager = SongRequestManager::default();
        let mut old = manager.checkpoint(Instant::now());
        old.as_object_mut().unwrap().remove("allowances");
        old.as_object_mut().unwrap().remove("session");
        for key in [
            "audience",
            "gift_rules",
            "gift_bonus_enabled",
            "sc_min_price",
            "sc_content_as_song",
            "sc_sort_by_price",
        ] {
            old["config"].as_object_mut().unwrap().remove(key);
        }
        manager.restore(old, Instant::now()).unwrap();
        assert_eq!(manager.config.audience.captain.limit, 20);
        assert_eq!(manager.config.audience.normal.cooldown_secs, 60);
        let before = manager.checkpoint(Instant::now());
        for config in [
            json!({"sc_min_price":-1}),
            json!({"audience":{"normal":{"enabled":true,"limit":10001,"cooldown_secs":0}}}),
            json!({"gift_rules":[{"id":"gift","gift_id":1,"gift_name":"辣条","gifts_required":0,"extra_requests":1}]}),
        ] {
            assert!(manager
                .request(json!({"type":"configure","config":config}), Instant::now())
                .is_err());
            assert_eq!(before, manager.checkpoint(Instant::now()));
        }
    }
}
