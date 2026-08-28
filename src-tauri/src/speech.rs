//! 语音播报服务。
//!
//! 实时事件在后端统一进入此服务，避免多窗口重复播报。Windows 下使用
//! SAPI，所有 COM 调用都固定在独立线程中执行。

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::get_config_path;
use crate::live_types::{
    DataUpdate, GiftUpsert, ProcessedDanmaku, ProcessedGift, ProcessedSuperChat,
};

const COMMAND_CHANNEL_CAPACITY: usize = 128;
const WORKER_TICK: Duration = Duration::from_millis(50);
const WORKER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);
const DANMAKU_RATE_WINDOW: Duration = Duration::from_secs(10);
const DANMAKU_SUSPEND_COUNT: usize = 20;
const DANMAKU_RESUME_COUNT: usize = 5;
const DANMAKU_RESUME_STABLE: Duration = Duration::from_secs(15);
const MAX_DANMAKU_QUEUE: usize = 6;
const MAX_IMPORTANT_QUEUE: usize = 100;
const DANMAKU_TTL: Duration = Duration::from_secs(10);
const IMPORTANT_TTL: Duration = Duration::from_secs(60);
const GIFT_COMBO_DEBOUNCE: Duration = Duration::from_secs(5);
const COMBO_DEDUP_TTL: Duration = Duration::from_secs(120);
const EVENT_DEDUP_TTL: Duration = Duration::from_secs(600);
const MAX_PENDING_COMBOS: usize = 256;
const MAX_SEEN_EVENTS: usize = 4096;

/// 用户可持久化的语音设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeechSettings {
    pub enabled: bool,
    pub voice_id: Option<String>,
    pub rate: i32,
    pub speak_danmaku: bool,
    pub speak_gift: bool,
    pub speak_super_chat: bool,
}

impl Default for SpeechSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            voice_id: None,
            rate: 0,
            speak_danmaku: true,
            speak_gift: true,
            speak_super_chat: true,
        }
    }
}

impl SpeechSettings {
    fn normalize(mut self) -> Self {
        self.rate = self.rate.clamp(-10, 10);
        self.voice_id = self
            .voice_id
            .take()
            .map(|id| id.trim().to_owned())
            .filter(|id| !id.is_empty());
        self
    }
}

/// 语音运行时还需要复用现有的弹幕及礼物过滤设置。
#[derive(Debug, Clone)]
pub struct SpeechRuntimeConfig {
    pub settings: SpeechSettings,
    pub ignored_uids: HashSet<u64>,
    pub gift_show_free: bool,
    pub gift_min_price: u64,
}

impl Default for SpeechRuntimeConfig {
    fn default() -> Self {
        Self {
            settings: SpeechSettings::default(),
            ignored_uids: HashSet::new(),
            gift_show_free: true,
            gift_min_price: 0,
        }
    }
}

impl SpeechRuntimeConfig {
    pub fn new(
        settings: SpeechSettings,
        ignored_uids: Vec<u64>,
        gift_show_free: bool,
        gift_min_price: u64,
    ) -> Self {
        Self {
            settings: settings.normalize(),
            ignored_uids: ignored_uids.into_iter().filter(|uid| *uid > 0).collect(),
            gift_show_free,
            gift_min_price,
        }
    }

    /// 后端启动时直接读取配置，避免依赖某个 WebView 先完成初始化。
    pub fn load_from_config() -> Self {
        let Ok(content) = fs::read_to_string(get_config_path()) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            return Self::default();
        };

        let settings = value
            .get("speech")
            .cloned()
            .and_then(|speech| serde_json::from_value::<SpeechSettings>(speech).ok())
            .unwrap_or_default();
        let ignored_uids = value
            .get("danmakuFilterUids")
            .and_then(|uids| serde_json::from_value::<Vec<u64>>(uids.clone()).ok())
            .unwrap_or_default();
        let display = value.get("display");
        let gift_show_free = display
            .and_then(|item| item.get("giftShowFree"))
            .and_then(|item| item.as_bool())
            .unwrap_or(true);
        let gift_min_price = display
            .and_then(|item| item.get("giftMinPrice"))
            .and_then(|item| item.as_u64())
            .unwrap_or(0);

        Self::new(settings, ignored_uids, gift_show_free, gift_min_price)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeechVoice {
    pub id: String,
    pub name: String,
    pub language: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SpeechStatus {
    pub available: bool,
    pub speaking: bool,
    pub danmaku_suspended: bool,
    pub queue_depth: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
enum SpeechInput {
    Danmaku(ProcessedDanmaku),
    Gift(Box<GiftUpsert>),
    SuperChat(ProcessedSuperChat),
}

enum WorkerCommand {
    Events(Vec<SpeechInput>),
    UpdateConfig(SpeechRuntimeConfig),
    ListVoices(SyncSender<Result<Vec<SpeechVoice>, String>>),
    Preview {
        voice_id: Option<String>,
        rate: i32,
        response: SyncSender<Result<(), String>>,
    },
    Reset,
    Shutdown,
}

/// 全局语音服务。公开方法不会执行 COM 或音频操作。
pub struct SpeechService {
    command_tx: SyncSender<WorkerCommand>,
    status: Arc<RwLock<SpeechStatus>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl SpeechService {
    pub fn new(config: SpeechRuntimeConfig) -> Self {
        let (command_tx, command_rx) = mpsc::sync_channel(COMMAND_CHANNEL_CAPACITY);
        let status = Arc::new(RwLock::new(SpeechStatus::default()));
        let worker_status = status.clone();
        let worker = thread::Builder::new()
            .name("speech-worker".to_owned())
            .spawn(move || run_worker(command_rx, worker_status, config))
            .expect("failed to start speech worker");

        Self {
            command_tx,
            status,
            worker: Mutex::new(Some(worker)),
        }
    }

    /// 从本次实时更新中提取可播报事件。快照不会经过这里。
    pub fn enqueue_updates(&self, updates: &[DataUpdate]) {
        if updates
            .iter()
            .any(|update| matches!(update, DataUpdate::LiveStop))
        {
            self.reset_session();
            return;
        }

        let mut inputs = Vec::new();
        for update in updates {
            match update {
                DataUpdate::DanmakuAppend(items) => {
                    inputs.extend(items.iter().cloned().map(SpeechInput::Danmaku));
                }
                DataUpdate::GiftUpsert(items) => {
                    inputs.extend(
                        items
                            .iter()
                            .cloned()
                            .map(|item| SpeechInput::Gift(Box::new(item))),
                    );
                }
                DataUpdate::SuperChatAppend(item) => {
                    inputs.push(SpeechInput::SuperChat(item.clone()));
                }
                _ => {}
            }
        }

        if inputs.is_empty() {
            return;
        }

        let has_priority_event = inputs
            .iter()
            .any(|input| !matches!(input, SpeechInput::Danmaku(_)));
        match self.command_tx.try_send(WorkerCommand::Events(inputs)) {
            Ok(()) => {}
            Err(TrySendError::Full(command)) if has_priority_event => {
                // 推送任务独立于 WebSocket 处理循环。通道拥塞时允许它短暂等待，
                // 避免礼物和 SC 被弹幕批次挤掉。
                if self.command_tx.send(command).is_err() {
                    log::warn!("[Speech] worker is unavailable");
                }
            }
            Err(TrySendError::Full(_)) => {
                log::warn!("[Speech] input channel full, dropping danmaku update batch");
            }
            Err(TrySendError::Disconnected(_)) => {
                log::warn!("[Speech] worker is unavailable");
            }
        }
    }

    pub fn update_config(&self, config: SpeechRuntimeConfig) -> Result<(), String> {
        self.command_tx
            .send(WorkerCommand::UpdateConfig(config))
            .map_err(|_| "语音服务不可用".to_owned())
    }

    pub fn list_voices(&self) -> Result<Vec<SpeechVoice>, String> {
        self.request_worker(WorkerCommand::ListVoices, "获取系统语音超时")
    }

    pub fn preview(&self, voice_id: Option<String>, rate: i32) -> Result<(), String> {
        self.request_worker(
            |response| WorkerCommand::Preview {
                voice_id,
                rate: rate.clamp(-10, 10),
                response,
            },
            "语音试听请求超时",
        )
    }

    fn request_worker<T>(
        &self,
        create_command: impl FnOnce(SyncSender<Result<T, String>>) -> WorkerCommand,
        timeout_message: &str,
    ) -> Result<T, String> {
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(create_command(response_tx))
            .map_err(|_| "语音服务不可用".to_owned())?;
        response_rx
            .recv_timeout(WORKER_RESPONSE_TIMEOUT)
            .map_err(|_| timeout_message.to_owned())?
    }

    pub fn status(&self) -> SpeechStatus {
        self.status
            .read()
            .map(|status| status.clone())
            .unwrap_or_default()
    }

    pub fn reset_session(&self) {
        let _ = self.command_tx.send(WorkerCommand::Reset);
    }

    pub fn shutdown(&self) {
        let _ = self.command_tx.send(WorkerCommand::Shutdown);
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(handle) = worker.take() {
                let _ = handle.join();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpeechKind {
    Danmaku,
    Gift,
    SuperChat,
}

struct QueuedSpeech {
    kind: SpeechKind,
    text: String,
    queued_at: Instant,
}

struct PendingGift {
    gift: ProcessedGift,
    updated_at: Instant,
}

struct SpeechWorker {
    engine: Option<PlatformSpeechEngine>,
    config: SpeechRuntimeConfig,
    status: Arc<RwLock<SpeechStatus>>,
    engine_error: Option<String>,
    important_queue: VecDeque<QueuedSpeech>,
    danmaku_queue: VecDeque<QueuedSpeech>,
    pending_combos: HashMap<String, PendingGift>,
    spoken_combos: HashMap<String, Instant>,
    seen_super_chats: HashMap<String, Instant>,
    danmaku_arrivals: VecDeque<Instant>,
    danmaku_suspended: bool,
    low_rate_since: Option<Instant>,
    speaking: bool,
}

impl SpeechWorker {
    fn new(
        mut engine: Option<PlatformSpeechEngine>,
        engine_error: Option<String>,
        status: Arc<RwLock<SpeechStatus>>,
        config: SpeechRuntimeConfig,
    ) -> Self {
        let mut final_error = engine_error;
        if let Some(engine) = engine.as_mut() {
            if let Err(error) = engine.apply_settings(&config.settings) {
                final_error = Some(error);
            }
        }

        Self {
            engine,
            config,
            status,
            engine_error: final_error,
            important_queue: VecDeque::new(),
            danmaku_queue: VecDeque::new(),
            pending_combos: HashMap::new(),
            spoken_combos: HashMap::new(),
            seen_super_chats: HashMap::new(),
            danmaku_arrivals: VecDeque::new(),
            danmaku_suspended: false,
            low_rate_since: None,
            speaking: false,
        }
    }

    fn handle_command(&mut self, command: WorkerCommand) -> bool {
        match command {
            WorkerCommand::Events(events) => self.handle_events(events),
            WorkerCommand::UpdateConfig(config) => self.apply_config(config),
            WorkerCommand::ListVoices(response) => {
                let result = self
                    .engine
                    .as_ref()
                    .ok_or_else(|| self.unavailable_message())
                    .and_then(|engine| engine.list_voices());
                let _ = response.send(result);
            }
            WorkerCommand::Preview {
                voice_id,
                rate,
                response,
            } => {
                let unavailable = self.unavailable_message();
                let result = self.engine.as_mut().ok_or(unavailable).and_then(|engine| {
                    engine.preview(voice_id.as_deref(), rate, "这是一条语音播报试听消息。")
                });
                let _ = response.send(result);
            }
            WorkerCommand::Reset => self.reset_session(),
            WorkerCommand::Shutdown => {
                self.reset_session();
                return false;
            }
        }
        true
    }

    fn unavailable_message(&self) -> String {
        self.engine_error
            .clone()
            .unwrap_or_else(|| "当前系统不支持语音播报".to_owned())
    }

    fn apply_config(&mut self, config: SpeechRuntimeConfig) {
        let was_enabled = self.config.settings.enabled;
        self.config = config;
        if let Some(engine) = self.engine.as_mut() {
            match engine.apply_settings(&self.config.settings) {
                Ok(()) => self.engine_error = None,
                Err(error) => self.engine_error = Some(error),
            }
        }

        if !self.config.settings.enabled {
            self.reset_queues();
            if was_enabled {
                if let Some(engine) = self.engine.as_mut() {
                    let _ = engine.stop();
                }
                self.speaking = false;
            }
            return;
        }

        if !self.config.settings.speak_danmaku {
            self.danmaku_queue.clear();
            self.danmaku_arrivals.clear();
            self.danmaku_suspended = false;
            self.low_rate_since = None;
        }
        if !self.config.settings.speak_gift {
            self.pending_combos.clear();
            self.important_queue
                .retain(|item| item.kind != SpeechKind::Gift);
        }
        if !self.config.settings.speak_super_chat {
            self.important_queue
                .retain(|item| item.kind != SpeechKind::SuperChat);
        }
    }

    fn handle_events(&mut self, events: Vec<SpeechInput>) {
        if !self.config.settings.enabled || self.engine.is_none() {
            return;
        }

        for event in events {
            match event {
                SpeechInput::Danmaku(item) => self.handle_danmaku(item),
                SpeechInput::Gift(item) => self.handle_gift(*item),
                SpeechInput::SuperChat(item) => self.handle_super_chat(item),
            }
        }
    }

    fn handle_danmaku(&mut self, danmaku: ProcessedDanmaku) {
        if !self.config.settings.speak_danmaku
            || self.config.ignored_uids.contains(&danmaku.user.uid)
        {
            return;
        }

        let now = Instant::now();
        self.danmaku_arrivals.push_back(now);
        prune_instants(&mut self.danmaku_arrivals, now, DANMAKU_RATE_WINDOW);
        while self.danmaku_arrivals.len() > DANMAKU_SUSPEND_COUNT {
            self.danmaku_arrivals.pop_front();
        }

        if self.danmaku_arrivals.len() >= DANMAKU_SUSPEND_COUNT {
            self.suspend_danmaku();
        }
        if self.danmaku_suspended {
            return;
        }
        if self.danmaku_queue.len() >= MAX_DANMAKU_QUEUE {
            self.suspend_danmaku();
            return;
        }

        if let Some(text) = format_danmaku(&danmaku) {
            self.danmaku_queue.push_back(QueuedSpeech {
                kind: SpeechKind::Danmaku,
                text,
                queued_at: now,
            });
        }
    }

    fn handle_gift(&mut self, upsert: GiftUpsert) {
        if !self.config.settings.speak_gift {
            return;
        }
        let gift = upsert.gift;
        if !self.should_speak_gift(&gift) {
            return;
        }

        let now = Instant::now();
        if gift.combo.is_none() {
            self.enqueue_gift(gift, now);
            return;
        }
        if self.spoken_combos.contains_key(&gift.merge_key) {
            return;
        }

        self.make_room_for_combo(&gift.merge_key);
        self.pending_combos.insert(
            gift.merge_key.clone(),
            PendingGift {
                gift,
                updated_at: now,
            },
        );
    }

    fn make_room_for_combo(&mut self, merge_key: &str) {
        if self.pending_combos.len() < MAX_PENDING_COMBOS
            || self.pending_combos.contains_key(merge_key)
        {
            return;
        }

        if let Some(oldest_key) = self
            .pending_combos
            .iter()
            .min_by_key(|(_, pending)| pending.updated_at)
            .map(|(key, _)| key.clone())
        {
            self.pending_combos.remove(&oldest_key);
            log::warn!("[Speech] too many active gift combos, dropped {oldest_key}");
        }
    }

    fn should_speak_gift(&self, gift: &ProcessedGift) -> bool {
        (self.config.gift_show_free || gift.is_paid)
            && gift.total_value >= self.config.gift_min_price
    }

    fn handle_super_chat(&mut self, super_chat: ProcessedSuperChat) {
        if !self.config.settings.speak_super_chat {
            return;
        }
        let now = Instant::now();
        if self.seen_super_chats.contains_key(&super_chat.id) {
            return;
        }
        if self.seen_super_chats.len() >= MAX_SEEN_EVENTS {
            self.seen_super_chats.clear();
        }
        self.seen_super_chats.insert(super_chat.id.clone(), now);

        if let Some(text) = format_super_chat(&super_chat) {
            self.enqueue_important(SpeechKind::SuperChat, text, now);
        }
    }

    fn enqueue_gift(&mut self, gift: ProcessedGift, now: Instant) {
        if let Some(text) = format_gift(&gift) {
            self.enqueue_important(SpeechKind::Gift, text, now);
        }
    }

    fn enqueue_important(&mut self, kind: SpeechKind, text: String, now: Instant) {
        if self.important_queue.len() >= MAX_IMPORTANT_QUEUE {
            self.important_queue.pop_front();
            log::warn!("[Speech] important queue full, dropped the oldest item");
        }
        self.important_queue.push_back(QueuedSpeech {
            kind,
            text,
            queued_at: now,
        });
    }

    fn suspend_danmaku(&mut self) {
        if !self.danmaku_suspended {
            log::info!("[Speech] danmaku speech suspended due to high traffic");
        }
        self.danmaku_suspended = true;
        self.low_rate_since = None;
        self.danmaku_queue.clear();
    }

    fn tick(&mut self) {
        let now = Instant::now();
        self.finalize_gift_combos(now);
        self.update_danmaku_suspension(now);
        self.prune_expired(now);
        self.update_speaking_state();
        self.start_next(now);
        self.publish_status();
    }

    fn finalize_gift_combos(&mut self, now: Instant) {
        let ready: Vec<_> = self
            .pending_combos
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.updated_at) >= GIFT_COMBO_DEBOUNCE)
            .map(|(key, _)| key.clone())
            .collect();

        for key in ready {
            if let Some(pending) = self.pending_combos.remove(&key) {
                if self.should_speak_gift(&pending.gift) {
                    self.enqueue_gift(pending.gift, now);
                }
                self.spoken_combos.insert(key, now);
            }
        }
        self.spoken_combos
            .retain(|_, spoken_at| now.duration_since(*spoken_at) < COMBO_DEDUP_TTL);
        if self.spoken_combos.len() > MAX_SEEN_EVENTS {
            self.spoken_combos.clear();
        }
        self.seen_super_chats
            .retain(|_, seen_at| now.duration_since(*seen_at) < EVENT_DEDUP_TTL);
    }

    fn update_danmaku_suspension(&mut self, now: Instant) {
        prune_instants(&mut self.danmaku_arrivals, now, DANMAKU_RATE_WINDOW);
        if !self.danmaku_suspended {
            return;
        }

        if self.danmaku_arrivals.len() >= DANMAKU_RESUME_COUNT {
            self.low_rate_since = None;
            return;
        }

        let low_rate_since = self.low_rate_since.get_or_insert(now);
        if now.duration_since(*low_rate_since) >= DANMAKU_RESUME_STABLE {
            self.danmaku_suspended = false;
            self.low_rate_since = None;
            log::info!("[Speech] danmaku speech resumed");
        }
    }

    fn prune_expired(&mut self, now: Instant) {
        self.important_queue
            .retain(|item| now.duration_since(item.queued_at) < IMPORTANT_TTL);
        self.danmaku_queue
            .retain(|item| now.duration_since(item.queued_at) < DANMAKU_TTL);
    }

    fn update_speaking_state(&mut self) {
        if !self.speaking {
            return;
        }
        let Some(engine) = self.engine.as_mut() else {
            self.speaking = false;
            return;
        };
        match engine.is_done() {
            Ok(true) => self.speaking = false,
            Ok(false) => {}
            Err(error) => {
                self.engine_error = Some(error);
                self.speaking = false;
            }
        }
    }

    fn start_next(&mut self, now: Instant) {
        if self.speaking || !self.config.settings.enabled {
            return;
        }
        let next = self
            .important_queue
            .pop_front()
            .or_else(|| self.danmaku_queue.pop_front());
        let Some(next) = next else {
            return;
        };
        let ttl = if next.kind == SpeechKind::Danmaku {
            DANMAKU_TTL
        } else {
            IMPORTANT_TTL
        };
        if now.duration_since(next.queued_at) >= ttl {
            return;
        }

        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        match engine.speak(&next.text) {
            Ok(()) => {
                self.speaking = true;
                self.engine_error = None;
            }
            Err(error) => {
                log::warn!("[Speech] synthesis failed: {error}");
                self.engine_error = Some(error);
            }
        }
    }

    fn publish_status(&self) {
        if let Ok(mut status) = self.status.write() {
            *status = SpeechStatus {
                available: self.engine.is_some(),
                speaking: self.speaking,
                danmaku_suspended: self.danmaku_suspended,
                queue_depth: self.important_queue.len()
                    + self.danmaku_queue.len()
                    + self.pending_combos.len(),
                error: self.engine_error.clone(),
            };
        }
    }

    fn reset_queues(&mut self) {
        self.important_queue.clear();
        self.danmaku_queue.clear();
        self.pending_combos.clear();
        self.spoken_combos.clear();
        self.seen_super_chats.clear();
        self.danmaku_arrivals.clear();
        self.danmaku_suspended = false;
        self.low_rate_since = None;
    }

    fn reset_session(&mut self) {
        self.reset_queues();
        if let Some(engine) = self.engine.as_mut() {
            let _ = engine.stop();
        }
        self.speaking = false;
    }
}

fn run_worker(
    command_rx: Receiver<WorkerCommand>,
    status: Arc<RwLock<SpeechStatus>>,
    config: SpeechRuntimeConfig,
) {
    let (engine, error) = match PlatformSpeechEngine::new() {
        Ok(engine) => (Some(engine), None),
        Err(error) => {
            log::warn!("[Speech] engine unavailable: {error}");
            (None, Some(error))
        }
    };
    let mut worker = SpeechWorker::new(engine, error, status, config);
    worker.publish_status();

    loop {
        worker.tick();
        match command_rx.recv_timeout(WORKER_TICK) {
            Ok(command) => {
                if !worker.handle_command(command) {
                    break;
                }
                while let Ok(command) = command_rx.try_recv() {
                    if !worker.handle_command(command) {
                        return;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn prune_instants(items: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while items
        .front()
        .is_some_and(|item| now.duration_since(*item) >= window)
    {
        items.pop_front();
    }
}

fn format_danmaku(danmaku: &ProcessedDanmaku) -> Option<String> {
    let username = sanitize_text(&danmaku.user.name, 24);
    if username.is_empty() {
        return None;
    }
    let content = sanitize_text(&danmaku.content, 80);
    if danmaku.is_emoticon {
        if content.is_empty() {
            return Some(format!("{username}发送了一个表情。"));
        }
        return Some(format!("{username}发送了表情，{content}。"));
    }
    if content.is_empty() {
        None
    } else {
        Some(format!("{username}说，{content}。"))
    }
}

fn format_gift(gift: &ProcessedGift) -> Option<String> {
    let username = sanitize_text(&gift.user.name, 24);
    let gift_name = sanitize_text(&gift.gift_name, 32);
    if username.is_empty() || gift_name.is_empty() {
        return None;
    }

    let mut text = if gift.guard_level.is_some() {
        format!("{username}开通了{gift_name}，{}个月", gift.num)
    } else if let Some(blind) = &gift.blind_gift {
        let blind_name = sanitize_text(&blind.gift_name, 32);
        format!(
            "{username}赠送了{blind_name}，开出了{gift_name}，数量{}",
            gift.num
        )
    } else {
        format!("{username}赠送了{gift_name}，数量{}", gift.num)
    };

    if gift.is_paid && gift.revenue_value > 0 {
        text.push_str(&format!("，价值{}元", format_price(gift.revenue_value)));
    }
    text.push('。');
    Some(text)
}

fn format_super_chat(super_chat: &ProcessedSuperChat) -> Option<String> {
    let username = sanitize_text(&super_chat.user.name, 24);
    if username.is_empty() {
        return None;
    }
    let content = sanitize_text(&super_chat.content, 200);
    let mut text = format!(
        "{username}发送了{}元醒目留言",
        format_price(super_chat.price)
    );
    if !content.is_empty() {
        text.push_str(&format!("，{content}"));
    }
    text.push('。');
    Some(text)
}

fn format_price(battery: u64) -> String {
    if battery.is_multiple_of(10) {
        (battery / 10).to_string()
    } else {
        format!("{}.{:01}", battery / 10, battery % 10)
    }
}

fn sanitize_text(input: &str, max_chars: usize) -> String {
    static URL_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static SPACE_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let url_re = URL_RE.get_or_init(|| Regex::new(r"(?i)(?:https?://|www\.)\S+").unwrap());
    let space_re = SPACE_RE.get_or_init(|| Regex::new(r"\s+").unwrap());
    let replaced = url_re.replace_all(input, "链接");
    let normalized = space_re.replace_all(replaced.trim(), " ");
    let mut chars = normalized.chars();
    let mut result: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        result.push_str("……");
    }
    result
}

#[cfg(windows)]
struct PlatformSpeechEngine {
    voice: windows::Win32::Media::Speech::ISpeechVoice,
    preview_voice: windows::Win32::Media::Speech::ISpeechVoice,
    _com: ComApartment,
}

#[cfg(windows)]
struct ComApartment;

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::Com::CoUninitialize() };
    }
}

#[cfg(windows)]
fn sapi_speech_flags(
    purge_before_speak: bool,
) -> windows::Win32::Media::Speech::SpeechVoiceSpeakFlags {
    use windows::Win32::Media::Speech::{
        SVSFIsNotXML, SVSFPurgeBeforeSpeak, SVSFlagsAsync, SpeechVoiceSpeakFlags,
    };

    let purge_flag = if purge_before_speak {
        SVSFPurgeBeforeSpeak.0
    } else {
        0
    };
    SpeechVoiceSpeakFlags(SVSFlagsAsync.0 | SVSFIsNotXML.0 | purge_flag)
}

#[cfg(windows)]
impl PlatformSpeechEngine {
    fn new() -> Result<Self, String> {
        use windows::Win32::Media::Speech::{ISpeechVoice, SpVoice};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        };

        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|error| format!("初始化 SAPI COM 失败：{error}"))?;
            let com = ComApartment;
            let voice: ISpeechVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL)
                .map_err(|error| format!("创建 SAPI 语音失败：{error}"))?;
            let preview_voice: ISpeechVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL)
                .map_err(|error| format!("创建 SAPI 试听语音失败：{error}"))?;
            let engine = Self {
                voice,
                preview_voice,
                _com: com,
            };
            if engine.list_voices()?.is_empty() {
                return Err("系统中没有可用的 SAPI 语音".to_owned());
            }
            Ok(engine)
        }
    }

    fn list_voices(&self) -> Result<Vec<SpeechVoice>, String> {
        use windows::core::BSTR;

        unsafe {
            let tokens = self
                .voice
                .GetVoices(&BSTR::new(), &BSTR::new())
                .map_err(|error| format!("枚举系统语音失败：{error}"))?;
            let count = tokens
                .Count()
                .map_err(|error| format!("读取语音数量失败：{error}"))?;
            let mut voices = Vec::with_capacity(count.max(0) as usize);
            for index in 0..count {
                let token = tokens
                    .Item(index)
                    .map_err(|error| format!("读取语音失败：{error}"))?;
                let id = token
                    .Id()
                    .map(|value| value.to_string())
                    .map_err(|error| format!("读取语音 ID 失败：{error}"))?;
                let name = token
                    .GetDescription(0)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|_| id.clone());
                let language = token
                    .GetAttribute(&BSTR::from("Language"))
                    .ok()
                    .and_then(|value| language_from_sapi_attribute(&value.to_string()))
                    .unwrap_or_default();
                voices.push(SpeechVoice { id, name, language });
            }
            voices.sort_by(|a, b| {
                let a_zh = a.language.starts_with("zh");
                let b_zh = b.language.starts_with("zh");
                b_zh.cmp(&a_zh).then_with(|| a.name.cmp(&b.name))
            });
            Ok(voices)
        }
    }

    fn apply_settings(&mut self, settings: &SpeechSettings) -> Result<(), String> {
        unsafe {
            self.voice
                .SetRate(settings.rate.clamp(-10, 10))
                .map_err(|error| format!("设置语速失败：{error}"))?;
        }
        Self::select_voice(&self.voice, settings.voice_id.as_deref())
    }

    fn select_voice(
        voice: &windows::Win32::Media::Speech::ISpeechVoice,
        voice_id: Option<&str>,
    ) -> Result<(), String> {
        use windows::core::BSTR;

        unsafe {
            let tokens = voice
                .GetVoices(&BSTR::new(), &BSTR::new())
                .map_err(|error| format!("枚举系统语音失败：{error}"))?;
            let count = tokens.Count().map_err(|error| error.to_string())?;
            let mut chinese_fallback = None;
            let mut selected = None;
            for index in 0..count {
                let token = tokens.Item(index).map_err(|error| error.to_string())?;
                let id = token
                    .Id()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                if voice_id.is_some_and(|expected| expected == id) {
                    selected = Some(token);
                    break;
                }
                if chinese_fallback.is_none() {
                    let is_chinese = token
                        .GetAttribute(&BSTR::from("Language"))
                        .ok()
                        .and_then(|value| language_from_sapi_attribute(&value.to_string()))
                        .is_some_and(|language| language.starts_with("zh"));
                    if is_chinese {
                        chinese_fallback = Some(token);
                    }
                }
            }

            let selected_exists = selected.is_some();
            let token = selected
                .or(chinese_fallback)
                .or_else(|| if count > 0 { tokens.Item(0).ok() } else { None })
                .ok_or_else(|| "系统中没有可用的语音".to_owned())?;
            voice
                .putref_Voice(&token)
                .map_err(|error| format!("切换语音失败：{error}"))?;
            if voice_id.is_some() && !selected_exists {
                return Err("已选择的语音不存在，已回退到可用语音".to_owned());
            }
            Ok(())
        }
    }

    fn speak(&mut self, text: &str) -> Result<(), String> {
        use windows::core::BSTR;

        unsafe {
            self.voice
                .Speak(&BSTR::from(text), sapi_speech_flags(false))
                .map(|_| ())
                .map_err(|error| format!("SAPI 播报失败：{error}"))
        }
    }

    fn is_done(&mut self) -> Result<bool, String> {
        unsafe {
            self.voice
                .WaitUntilDone(0)
                .map(|done| done.as_bool())
                .map_err(|error| format!("读取 SAPI 状态失败：{error}"))
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        use windows::core::BSTR;

        unsafe {
            self.voice
                .Speak(&BSTR::new(), sapi_speech_flags(true))
                .map(|_| ())
                .map_err(|error| format!("停止 SAPI 播报失败：{error}"))
        }
    }

    fn preview(&mut self, voice_id: Option<&str>, rate: i32, text: &str) -> Result<(), String> {
        use windows::core::BSTR;

        unsafe {
            self.preview_voice
                .SetRate(rate.clamp(-10, 10))
                .map_err(|error| format!("设置试听语速失败：{error}"))?;
        }
        Self::select_voice(&self.preview_voice, voice_id)?;
        unsafe {
            self.preview_voice
                .Speak(&BSTR::from(text), sapi_speech_flags(true))
                .map(|_| ())
                .map_err(|error| format!("语音试听失败：{error}"))
        }
    }
}

#[cfg(windows)]
fn language_from_sapi_attribute(attribute: &str) -> Option<String> {
    use windows::Win32::Globalization::LCIDToLocaleName;

    let first = attribute.split(';').next()?.trim();
    let lcid = u32::from_str_radix(first, 16).ok()?;
    let mut buffer = [0u16; 85];
    let length = unsafe { LCIDToLocaleName(lcid, Some(&mut buffer), 0) };
    if length <= 1 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..length as usize - 1]))
}

#[cfg(not(windows))]
const UNSUPPORTED_PLATFORM_ERROR: &str = "语音播报当前仅支持 Windows";

#[cfg(not(windows))]
struct PlatformSpeechEngine;

#[cfg(not(windows))]
impl PlatformSpeechEngine {
    fn new() -> Result<Self, String> {
        Err(UNSUPPORTED_PLATFORM_ERROR.to_owned())
    }

    fn list_voices(&self) -> Result<Vec<SpeechVoice>, String> {
        Err(UNSUPPORTED_PLATFORM_ERROR.to_owned())
    }

    fn apply_settings(&mut self, _settings: &SpeechSettings) -> Result<(), String> {
        Err(UNSUPPORTED_PLATFORM_ERROR.to_owned())
    }

    fn speak(&mut self, _text: &str) -> Result<(), String> {
        Err(UNSUPPORTED_PLATFORM_ERROR.to_owned())
    }

    fn is_done(&mut self) -> Result<bool, String> {
        Ok(true)
    }

    fn stop(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn preview(&mut self, _voice_id: Option<&str>, _rate: i32, _text: &str) -> Result<(), String> {
        Err(UNSUPPORTED_PLATFORM_ERROR.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_types::{ProcessedBlindGift, ProcessedGiftCombo, ProcessedUser, UpsertAction};

    fn user(name: &str) -> ProcessedUser {
        ProcessedUser {
            uid: 1,
            name: name.to_owned(),
            face: None,
            medal: None,
            guard_level: 0,
            wealth_level: 0,
            is_admin: false,
        }
    }

    #[test]
    fn sanitizes_urls_whitespace_and_length() {
        assert_eq!(
            sanitize_text("  查看 https://example.com/a\n  谢谢  ", 20),
            "查看 链接 谢谢"
        );
        assert_eq!(sanitize_text("一二三四五", 3), "一二三……");
    }

    #[test]
    fn formats_super_chat_price_in_battery() {
        let sc = ProcessedSuperChat {
            id: "1".to_owned(),
            content: "你好".to_owned(),
            price: 305,
            user: user("测试用户"),
            background_color: String::new(),
            duration: 60,
            start_time: 0,
        };
        assert_eq!(
            format_super_chat(&sc).as_deref(),
            Some("测试用户发送了30.5元醒目留言，你好。")
        );
    }

    fn gift_with_combo(num: u32) -> ProcessedGift {
        ProcessedGift {
            id: "gift:combo".to_owned(),
            merge_key: "combo:1".to_owned(),
            gift_id: 1,
            gift_name: "小花花".to_owned(),
            gift_icon: String::new(),
            num,
            total_value: 0,
            revenue_value: 0,
            is_paid: false,
            combo: Some(ProcessedGiftCombo {
                batch_combo_id: "combo:1".to_owned(),
                combo_total_coin: None,
                super_batch_gift_num: None,
                combo_resources_id: None,
                combo_stay_time: None,
                show_batch_combo_send: None,
            }),
            blind_gift: None,
            user: user("测试用户"),
            timestamp: 0,
            guard_level: None,
        }
    }

    fn test_worker() -> SpeechWorker {
        let mut config = SpeechRuntimeConfig::default();
        config.settings.enabled = true;
        SpeechWorker::new(
            None,
            None,
            Arc::new(RwLock::new(SpeechStatus::default())),
            config,
        )
    }

    #[test]
    fn formats_blind_gift_with_actual_revenue() {
        let gift = ProcessedGift {
            id: "1".to_owned(),
            merge_key: "1".to_owned(),
            gift_id: 1,
            gift_name: "小电视".to_owned(),
            gift_icon: String::new(),
            num: 2,
            total_value: 100,
            revenue_value: 50,
            is_paid: true,
            combo: None,
            blind_gift: Some(ProcessedBlindGift {
                gift_id: 2,
                gift_name: "欢乐盲盒".to_owned(),
                total_value: 50,
            }),
            user: user("测试用户"),
            timestamp: 0,
            guard_level: None,
        };
        assert_eq!(
            format_gift(&gift).as_deref(),
            Some("测试用户赠送了欢乐盲盒，开出了小电视，数量2，价值5元。")
        );
    }

    #[test]
    fn combo_updates_become_one_final_utterance() {
        let mut worker = test_worker();
        worker.handle_gift(GiftUpsert {
            merge_key: "combo:1".to_owned(),
            gift: gift_with_combo(1),
            action: UpsertAction::Insert,
        });
        worker.handle_gift(GiftUpsert {
            merge_key: "combo:1".to_owned(),
            gift: gift_with_combo(12),
            action: UpsertAction::Update,
        });
        assert_eq!(worker.pending_combos.len(), 1);
        assert!(worker.important_queue.is_empty());

        worker.pending_combos.get_mut("combo:1").unwrap().updated_at =
            Instant::now() - GIFT_COMBO_DEBOUNCE;
        worker.finalize_gift_combos(Instant::now());

        assert!(worker.pending_combos.is_empty());
        assert_eq!(worker.important_queue.len(), 1);
        assert!(worker.important_queue[0].text.contains("数量12"));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires an interactive Windows installation with SAPI voices"]
    fn windows_sapi_enumerates_installed_voices() {
        let engine = PlatformSpeechEngine::new().expect("SAPI should initialize");
        assert!(!engine.list_voices().unwrap().is_empty());
    }

    #[test]
    fn high_danmaku_rate_suspends_and_clears_queue() {
        let mut worker = test_worker();
        for index in 0..DANMAKU_SUSPEND_COUNT {
            worker.handle_danmaku(ProcessedDanmaku {
                id: index.to_string(),
                content: "测试弹幕".to_owned(),
                user: user("测试用户"),
                timestamp: 0,
                is_emoticon: false,
                emoticon_url: None,
            });
        }

        assert!(worker.danmaku_suspended);
        assert!(worker.danmaku_queue.is_empty());
    }
}
