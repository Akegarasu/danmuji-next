//! Bilibili 弹幕服务管理器
//!
//! 负责：
//! - 弹幕客户端生命周期管理（连接/断开/重连）
//! - 窗口订阅机制：按需向不同窗口分发事件
//! - 数据快照：新窗口可获取当前完整数据

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::archive::{ArchiveManager, Recording};
use crate::live_events::LiveEvent;
use crate::live_session::LiveSession;
use crate::live_types::*;
use crate::speech::SpeechService;
use blivedm::api::{
    get_all_guard_top_list, get_contribution_rank, get_contribution_rank_by_type, get_danmu_info,
    get_room_init, ContributionRankResponse, ContributionRankType, GuardTopListResponse, RoomInfo,
};
use blivedm::{parse_notification, BliveDmClient, CancellationToken, Error as BliveError, Event};

/// 控制句柄与对外可观察的状态分离，替换时必须取消并等待旧任务。
struct ConnectionTask {
    cancellation: CancellationToken,
    handle: JoinHandle<()>,
}

// ==================== 弹幕服务 ====================

pub struct BliveService {
    session: Mutex<LiveSession>,
    /// 只串行化资源交接，不在此锁内等待网络预检查。
    connection: Mutex<Option<ConnectionTask>>,
    speech: Arc<SpeechService>,
    extensions: Arc<crate::extensions::ExtensionHost>,
    archive: Arc<ArchiveManager>,
    /// 串行化批次发布；语音提交期间不阻塞直播聚合。
    publish_lock: Mutex<()>,
    /// 窗口订阅: window_label -> subscription
    subscriptions: RwLock<HashMap<String, HashSet<EventType>>>,
}

impl BliveService {
    pub fn new(
        speech: Arc<SpeechService>,
        extensions: Arc<crate::extensions::ExtensionHost>,
        archive: Arc<ArchiveManager>,
    ) -> Self {
        Self {
            session: Mutex::new(LiveSession::default()),
            connection: Mutex::new(None),
            speech,
            extensions,
            archive,
            subscriptions: RwLock::new(HashMap::new()),
            publish_lock: Mutex::new(()),
        }
    }

    pub async fn get_status(&self) -> ConnectionStatus {
        self.session.lock().await.status.clone()
    }

    pub async fn get_room_info(&self) -> Option<RoomInfoResponse> {
        self.session.lock().await.room_info.clone().map(Into::into)
    }

    /// 解析并处理手动输入的原始 B 站通知事件。
    pub async fn process_test_event(
        &self,
        app: &AppHandle,
        event_json: &str,
    ) -> Result<(), String> {
        let event = parse_notification(event_json.as_bytes(), None)
            .map_err(|error| format!("解析事件失败: {error}"))?;

        self.process_event(event).await;
        self.push_updates(app).await;
        Ok(())
    }

    /// 刷新贡献排行榜（手动调用 API 获取最新数据）
    pub async fn refresh_contribution_rank(
        &self,
        cookie: &str,
        rank_type: ContributionRankType,
    ) -> Result<ContributionRankResponse, String> {
        let (room_info, generation) = {
            let state = self.session.lock().await;
            let generation = matches!(
                state.status,
                ConnectionStatus::Connected | ConnectionStatus::Reconnecting
            )
            .then_some(state.generation);
            (state.room_info.clone(), generation)
        };

        let room_info = match room_info {
            Some(info) => info,
            None => {
                log::warn!("[ContributionRank] refresh skipped: room is not connected");
                return Err("未连接房间".to_string());
            }
        };

        log::info!(
            "[ContributionRank] refresh requested: room_id={}, ruid={}, type={:?}, has_cookie={}",
            room_info.room_id,
            room_info.uid,
            rank_type,
            !cookie.is_empty()
        );

        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| format!("创建榜单请求客户端失败: {error}"))?;
        match get_contribution_rank_by_type(
            &http_client,
            room_info.room_id,
            room_info.uid,
            Some(cookie),
            rank_type,
            1,
            100,
        )
        .await
        {
            Ok(rank) => {
                let list = rank.list.clone();
                log::info!(
                    "[ContributionRank] refresh success: count={}, top_uid={:?}",
                    list.len(),
                    list.first().map(|user| user.uid)
                );
                if rank_type == ContributionRankType::Online {
                    // 请求期间可能已切换房间；旧榜单不能写入新房间的快照。
                    let mut state = self.session.lock().await;
                    if matches!(
                        state.status,
                        ConnectionStatus::Connected | ConnectionStatus::Reconnecting
                    ) && Some(state.generation) == generation
                        && state.room_info.as_ref().map(|info| info.room_id)
                            == Some(room_info.room_id)
                    {
                        state.data.set_contribution_rank_full(list);
                    }
                }
                Ok(rank)
            }
            Err(e) => {
                log::warn!("[ContributionRank] refresh failed: {}", e);
                Err(format!("获取贡献排行榜失败: {}", e))
            }
        }
    }

    /// 刷新大航海榜。
    pub async fn refresh_guard_top_list(
        &self,
        cookie: &str,
    ) -> Result<GuardTopListResponse, String> {
        let room_info = self
            .session
            .lock()
            .await
            .room_info
            .clone()
            .ok_or_else(|| "未连接房间".to_string())?;
        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| format!("创建榜单请求客户端失败: {error}"))?;

        get_all_guard_top_list(&http_client, room_info.room_id, room_info.uid, Some(cookie))
            .await
            .map_err(|error| {
                log::warn!("[GuardTopList] refresh failed: {}", error);
                format!("获取大航海榜失败: {error}")
            })
    }

    /// 订阅事件
    pub async fn subscribe(&self, window_label: String, event_types: HashSet<EventType>) {
        log::info!(
            "Window {} subscribed to events: {:?}",
            window_label,
            event_types
        );
        self.subscriptions
            .write()
            .await
            .insert(window_label, event_types);
    }

    /// 取消订阅
    pub async fn unsubscribe(&self, window_label: &str) {
        let mut subs = self.subscriptions.write().await;
        subs.remove(window_label);
        log::info!("Window {} unsubscribed", window_label);
    }

    /// 获取数据快照
    pub async fn get_snapshot(&self, event_types: HashSet<EventType>) -> DataSnapshot {
        self.session.lock().await.data.snapshot(&event_types)
    }

    pub async fn connect(
        self: &Arc<Self>,
        app: AppHandle,
        room_id: u64,
        cookie: Option<String>,
    ) -> ConnectResult {
        let Some(cookie) = cookie.filter(|value| !value.is_empty()) else {
            return connection_failure("请先设置 Cookie");
        };

        let response = {
            let mut connection = self.connection.lock().await;
            self.stop_connection(&mut connection).await;
            self.session.lock().await.status = ConnectionStatus::Connecting;
            let _ = app.emit("blive-status", ConnectionStatus::Connecting);

            let (sender, receiver) = oneshot::channel();
            let cancellation = CancellationToken::new();
            let task_cancellation = cancellation.clone();
            let service = self.clone();
            let handle = tokio::spawn(async move {
                service
                    .run_connection(app, room_id, cookie, task_cancellation, sender)
                    .await;
            });
            *connection = Some(ConnectionTask {
                cancellation,
                handle,
            });
            receiver
        };
        // 保留原有成功返回时机：预检查通过即可返回，实际连接状态继续通过事件发布。
        response
            .await
            .unwrap_or_else(|_| connection_failure("连接任务异常结束"))
    }

    async fn run_connection(
        self: Arc<Self>,
        app: AppHandle,
        room_id: u64,
        cookie: String,
        cancellation: CancellationToken,
        response: oneshot::Sender<ConnectResult>,
    ) {
        let prepared = tokio::select! {
            biased;
            _ = cancellation.cancelled() => None,
            result = prepare_room(room_id, &cookie) => Some(result),
        };
        let room_info = match prepared {
            Some(Ok(info)) => info,
            Some(Err(message)) => {
                self.set_error(&app, &message).await;
                let _ = response.send(connection_failure(&message));
                return;
            }
            None => {
                let _ = response.send(connection_failure("连接已取消"));
                self.finish_connection(&app).await;
                return;
            }
        };
        {
            let mut state = self.session.lock().await;
            state.streamer_uid = room_info.uid;
            state.room_info = Some(room_info.clone());
            state.status = ConnectionStatus::Connected;
        }
        let _ = response.send(ConnectResult {
            success: true,
            message: "连接成功".to_owned(),
            room_info: Some(room_info.clone().into()),
        });

        let opening = async {
            let client = BliveDmClient::builder()
                .room_id(room_id)
                .cookie(cookie.clone())
                .auto_reconnect(true)
                .raw_event_handler(crate::raw_event_dump::dump)
                .build()
                .await
                .map_err(|error| format!("创建客户端失败: {error}"))?;
            client
                .connect()
                .await
                .map_err(|error| format!("连接失败: {error}"))
        };
        let opened = tokio::select! {
            biased;
            _ = cancellation.cancelled() => None,
            result = opening => Some(result),
        };
        let mut stream = match opened {
            Some(Ok(stream)) => stream,
            Some(Err(message)) => {
                self.set_error(&app, &message).await;
                return;
            }
            None => {
                self.finish_connection(&app).await;
                return;
            }
        };
        self.session.lock().await.status = ConnectionStatus::Connected;
        let _ = app.emit("blive-status", ConnectionStatus::Connected);
        self.extensions
            .dispatch_room(room_info.room_id, room_info.uid, None);

        // 保留存档使用用户输入房号的既有契约。创建和挂接期间不取消，确保资源被接管。
        match Recording::start(
            self.archive.clone(),
            room_id,
            &room_info.title,
            room_info.uid,
        )
        .await
        {
            Ok(recording) => self.session.lock().await.start_recording(recording),
            Err(error) => log::error!("Failed to start archive session: {error}"),
        }

        let http_client = reqwest::Client::new();
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {},
            result = get_contribution_rank(&http_client, room_info.room_id, room_info.uid, Some(&cookie), 1, 100) => {
                match result {
                    Ok(rank) => self.session.lock().await.data.set_contribution_rank_full(rank.list),
                    Err(error) => log::warn!("获取贡献排行榜失败: {error}"),
                }
            }
        }

        loop {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => break,
                event = stream.next() => {
                    match event {
                        Some(Ok(event)) => self.process_event(event).await,
                        Some(Err(error)) => {
                            log::error!("Event error: {error}");
                            if matches!(error, BliveError::ConnectionClosed) {
                                self.session.lock().await.status = ConnectionStatus::Reconnecting;
                                let _ = app.emit("blive-status", ConnectionStatus::Reconnecting);
                            }
                        }
                        None => break,
                    }
                }
            }
        }
        // 先释放协议流及其 socket/重连任务，再排空本会话的输出。
        drop(stream);
        self.finish_connection(&app).await;
    }

    async fn finish_connection(&self, app: &AppHandle) {
        self.push_updates(app).await;
        self.speech.reset_session();
        let finished = self.session.lock().await.finish_recording();
        if let Err(error) = finished.await {
            log::error!("Failed to end archive session: {error}");
        }
        self.session.lock().await.status = ConnectionStatus::Disconnected;
        let _ = app.emit("blive-status", ConnectionStatus::Disconnected);
    }

    pub async fn disconnect(&self) {
        let mut connection = self.connection.lock().await;
        self.stop_connection(&mut connection).await;
    }

    /// 调用方持有资源交接锁；所有旧任务完成后才允许清空聚合状态或开始新会话。
    async fn stop_connection(&self, connection: &mut Option<ConnectionTask>) {
        self.speech.reset_session();
        {
            let mut state = self.session.lock().await;
            state.status = ConnectionStatus::Disconnected;
        }
        if let Some(task) = connection.as_mut() {
            task.cancellation.cancel();
            // 等待被取消时仍将句柄留在槽内，后续命令必须继续等待同一任务。
            if let Err(error) = (&mut task.handle).await {
                log::error!("连接任务异常结束: {error}");
            }
        }
        *connection = None;
        // 任务异常退出也由拥有者完成最后一次录制交接。
        let finished = self.session.lock().await.finish_recording();
        if let Err(error) = finished.await {
            log::error!("Failed to end archive session: {error}");
        }
        let _publication = self.publish_lock.lock().await;
        {
            let mut session = self.session.lock().await;
            session.streamer_uid = 0;
            session.data.clear();
            session.generation = session.generation.wrapping_add(1);
            session.status = ConnectionStatus::Disconnected;
        }
        self.speech.reset_session();
    }

    /// 一次输入只聚合一次，持久化记录与领域通知在会话内同时产生。
    async fn process_event(&self, event: Event) {
        let (events, room) = {
            let mut session = self.session.lock().await;
            let events = session.process(event);
            let room = session
                .room_info
                .as_ref()
                .map(|room| (room.room_id, room.uid));
            (events, room)
        };
        // 扩展自己的状态和持久化不占用直播状态锁。
        for event in events {
            match event {
                LiveEvent::Text(text) => self.extensions.dispatch_text(&text),
                LiveEvent::Gift(gift) => self.extensions.dispatch_gift(&gift),
                LiveEvent::Started { room_id, live_time } => {
                    log::info!("Live started: room_id={}, live_time={}", room_id, live_time);
                    if let Some((room_id, uid)) = room {
                        self.extensions.dispatch_room(room_id, uid, Some(live_time));
                    }
                }
                LiveEvent::Stopped { room_id, round } => {
                    log::info!("Live stopped: room_id={}, round={}", room_id, round);
                }
            }
        }
    }

    /// 推送更新到前端（按窗口订阅过滤）
    pub(crate) async fn push_updates(&self, app: &AppHandle) {
        let _publication = self.publish_lock.lock().await;
        let (updates, streamer_uid) = {
            let mut session = self.session.lock().await;
            (session.data.take_pending_updates(), session.streamer_uid)
        };

        if updates.is_empty() {
            return;
        }

        // 在按窗口订阅分发前只投递一次，避免多窗口重复播报；
        // 语音未启用时跳过事件克隆和通道提交。
        if self.speech.accepts_events() {
            self.speech.enqueue_updates(&updates, streamer_uid);
        }

        let subs = self.subscriptions.read().await;

        // 如果没有订阅，不发送任何事件（前端必须先订阅）
        if subs.is_empty() {
            return;
        }

        // 按窗口订阅分发，使用带窗口标签的事件名，确保每个窗口只收到自己的事件
        for (window_label, sub) in subs.iter() {
            let filtered: Vec<_> = updates
                .iter()
                .filter(|u| sub.contains(&u.event_type()))
                .cloned()
                .collect();

            if !filtered.is_empty() {
                // 使用带窗口标签的事件名，前端监听对应的事件名
                let event_name = format!("blive-data:{}", window_label);
                let _ = app.emit(&event_name, &filtered);
            }
        }
    }

    async fn set_error(&self, app: &AppHandle, message: &str) {
        let status = ConnectionStatus::Error {
            message: message.to_string(),
        };
        {
            let mut state = self.session.lock().await;
            state.status = status.clone();
        }
        let _ = app.emit("blive-status", status);
    }
}

fn connection_failure(message: &str) -> ConnectResult {
    ConnectResult {
        success: false,
        message: message.to_owned(),
        room_info: None,
    }
}

/// 预检查的错误文本和返回契约与前端保持兼容。
async fn prepare_room(room_id: u64, cookie: &str) -> Result<RoomInfo, String> {
    let client = reqwest::Client::new();
    let room = get_room_init(&client, room_id)
        .await
        .map_err(|error| format!("获取房间信息失败: {error}"))?;
    let danmu = get_danmu_info(&client, room.room_id, Some(cookie))
        .await
        .map_err(|error| format!("获取弹幕服务器失败: {error}"))?;
    if danmu.host_list.is_empty() || danmu.token.is_empty() {
        return Err("Cookie 无效或已过期，无法获取弹幕服务器".to_owned());
    }
    Ok(room)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{extensions::ExtensionHost, speech::SpeechRuntimeConfig};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture {
        service: Arc<BliveService>,
        directory: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "danmuji-live-service-{}-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&directory).unwrap();
            let speech = Arc::new(SpeechService::new(SpeechRuntimeConfig::default()));
            let service = Arc::new(BliveService::new(
                speech,
                Arc::new(ExtensionHost::new(directory.clone())),
                Arc::new(ArchiveManager::new(":memory:".into()).unwrap()),
            ));
            Self { service, directory }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.service.speech.shutdown();
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn gift_batch() -> Event {
        parse_notification(
            include_bytes!("../../crates/blivedm/tests/fixtures/ten_blind_gift_v2.json"),
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn service_fans_out_deduplicated_events_to_recording_extensions_and_filtered_snapshot() {
        let fixture = Fixture::new();
        let service = &fixture.service;
        let archive = service.archive.clone();
        service.session.lock().await.start_recording(
            Recording::start(archive.clone(), 1, "测试", 900)
                .await
                .unwrap(),
        );
        service.extensions.request("overtime", json!({"type":"configure","config":{
            "enabled":true,"initial_seconds":0,"rules":[
                {"id":"pillow","enabled":true,"gift_id":32128,"gift_name":"爱心抱枕","action":"add","value":10,"per_gift":true}
            ]
        }})).unwrap();
        service
            .extensions
            .request("overtime", json!({"type":"reset"}))
            .unwrap();
        service.process_event(gift_batch()).await;
        service.process_event(gift_batch()).await;
        let gifts = service.get_snapshot(HashSet::from([EventType::Gift])).await;
        assert_eq!(
            gifts
                .gift_list
                .unwrap()
                .iter()
                .map(|gift| gift.num)
                .sum::<u32>(),
            10
        );
        assert!(gifts.stats.is_none());
        assert!(gifts.danmaku_list.is_none());
        assert_eq!(
            service.extensions.state("overtime").unwrap().state["remaining_ms"].as_f64(),
            Some(40_000.0)
        );
        let stats = service
            .get_snapshot(HashSet::from([EventType::Stats]))
            .await
            .stats
            .unwrap();
        assert_eq!(stats.total_revenue, 1110);
        let finished = service.session.lock().await.finish_recording();
        finished.await.unwrap();
        let saved = archive.get_sessions().await.unwrap();
        assert_eq!((saved[0].gift_count, saved[0].gift_revenue), (3, 1110));
    }

    #[tokio::test]
    async fn cancelled_disconnect_keeps_task_owned_and_next_disconnect_drains_before_clearing() {
        let fixture = Fixture::new();
        let service = fixture.service.clone();
        let archive = service.archive.clone();
        service.session.lock().await.start_recording(
            Recording::start(archive.clone(), 1, "等待断开", 900)
                .await
                .unwrap(),
        );
        service.process_event(gift_batch()).await;
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let (release, released) = oneshot::channel();
        let handle = tokio::spawn(async move {
            task_cancellation.cancelled().await;
            released.await.unwrap();
        });
        *service.connection.lock().await = Some(ConnectionTask {
            cancellation: cancellation.clone(),
            handle,
        });
        {
            let disconnect = service.disconnect();
            tokio::pin!(disconnect);
            tokio::select! {
                biased;
                _ = &mut disconnect => panic!("旧任务仍在退出"),
                _ = std::future::ready(()) => {},
            }
        }
        assert!(cancellation.is_cancelled());
        assert!(service.connection.lock().await.is_some());
        let next_service = service.clone();
        let next = tokio::spawn(async move { next_service.disconnect().await });
        tokio::task::yield_now().await;
        assert!(!next.is_finished());
        assert_eq!(
            service
                .get_snapshot(HashSet::from([EventType::Gift]))
                .await
                .gift_list
                .unwrap()
                .len(),
            3
        );
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), next)
            .await
            .unwrap()
            .unwrap();
        assert!(service.connection.lock().await.is_none());
        assert!(matches!(
            service.get_status().await,
            ConnectionStatus::Disconnected
        ));
        assert!(service
            .get_snapshot(HashSet::from([EventType::Gift]))
            .await
            .gift_list
            .unwrap()
            .is_empty());
        let saved = archive.get_sessions().await.unwrap();
        assert!(saved[0].end_time.is_some());
        assert_eq!((saved[0].gift_count, saved[0].total_revenue), (3, 1110));
    }

    #[tokio::test]
    async fn live_status_updates_room_state_and_keeps_existing_window_protocol() {
        let fixture = Fixture::new();
        let service = &fixture.service;
        service.session.lock().await.room_info = Some(RoomInfo {
            room_id: 1,
            short_id: 0,
            uid: 900,
            title: "测试".to_owned(),
            live_status: 0,
        });
        for raw in [
            json!({"cmd":"LIVE","roomid":1,"live_key":"key","live_time":1700000000}),
            json!({"cmd":"PREPARING","roomid":"1","round":1}),
        ] {
            service
                .process_event(
                    parse_notification(&serde_json::to_vec(&raw).unwrap(), None).unwrap(),
                )
                .await;
        }
        assert_eq!(service.get_room_info().await.unwrap().live_status, 2);
        let updates = service.session.lock().await.data.take_pending_updates();
        assert_eq!(
            serde_json::to_value(updates).unwrap(),
            json!([{"type":"LiveStart"},{"type":"LiveStop"}])
        );
    }
}
