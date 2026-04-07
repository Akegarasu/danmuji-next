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
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::archive::{ArchiveEvent, ArchiveManager};
use crate::blivedm::api::{
    get_contribution_rank, get_danmu_info, get_room_init, ContributionRankUser, RoomInfo,
};
use crate::blivedm::{BliveDmClient, Error as BliveError, Event};
use crate::kv_store::VideoRequestStore;
use crate::live_data::{LiveData, WindowSubscription};
use crate::live_types::*;
use crate::video_info;

// ==================== 服务状态 ====================

struct ServiceState {
    status: ConnectionStatus,
    room_id: u64,
    room_info: Option<RoomInfo>,
    stop_tx: Option<mpsc::Sender<()>>,
    task_handle: Option<JoinHandle<()>>,
}

impl Default for ServiceState {
    fn default() -> Self {
        Self {
            status: ConnectionStatus::Disconnected,
            room_id: 0,
            room_info: None,
            stop_tx: None,
            task_handle: None,
        }
    }
}

// ==================== 弹幕服务 ====================

pub struct BliveService {
    state: RwLock<ServiceState>,
    live_data: Arc<Mutex<LiveData>>,
    /// 窗口订阅: window_label -> subscription
    subscriptions: RwLock<HashMap<String, WindowSubscription>>,
}

impl BliveService {
    pub fn new(vr_store: VideoRequestStore) -> Self {
        Self {
            state: RwLock::new(ServiceState::default()),
            live_data: Arc::new(Mutex::new(LiveData::new(vr_store))),
            subscriptions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_status(&self) -> ConnectionStatus {
        self.state.read().await.status.clone()
    }

    pub async fn get_room_info(&self) -> Option<RoomInfoResponse> {
        self.state.read().await.room_info.clone().map(Into::into)
    }

    /// 刷新贡献排行榜（手动调用 API 获取最新数据）
    pub async fn refresh_contribution_rank(
        &self,
        cookie: &str,
    ) -> Result<Vec<ContributionRankUser>, String> {
        let room_info = {
            let state = self.state.read().await;
            state.room_info.clone()
        };

        let room_info = match room_info {
            Some(info) => info,
            None => return Err("未连接房间".to_string()),
        };

        let http_client = reqwest::Client::new();
        match get_contribution_rank(
            &http_client,
            room_info.room_id,
            room_info.uid,
            Some(cookie),
            1,
            100,
        )
        .await
        {
            Ok(rank) => {
                let list = rank.list.clone();
                self.live_data
                    .lock()
                    .await
                    .set_contribution_rank_full(list.clone());
                Ok(list)
            }
            Err(e) => Err(format!("获取贡献排行榜失败: {}", e)),
        }
    }

    /// 订阅事件
    pub async fn subscribe(&self, window_label: String, event_types: HashSet<EventType>) {
        let mut subs = self.subscriptions.write().await;
        let sub = subs.entry(window_label.clone()).or_default();
        sub.event_types = event_types;
        log::info!(
            "Window {} subscribed to events: {:?}",
            window_label,
            sub.event_types
        );
    }

    /// 取消订阅
    pub async fn unsubscribe(&self, window_label: &str) {
        let mut subs = self.subscriptions.write().await;
        subs.remove(window_label);
        log::info!("Window {} unsubscribed", window_label);
    }

    /// 获取数据快照
    pub async fn get_snapshot(&self, event_types: HashSet<EventType>) -> DataSnapshot {
        let data = self.live_data.lock().await;
        data.snapshot(&event_types)
    }

    // ==================== 点播管理 ====================

    /// 从 KV Store 加载持久化的点播数据
    pub async fn load_video_requests(&self) {
        let mut data = self.live_data.lock().await;
        data.load_video_requests();
    }

    /// 异步获取视频信息（批量）
    async fn spawn_video_fetches(&self, to_fetch: Vec<(String, String, u64, Option<u64>)>) {
        for (request_id, video_id, _uid, _sc_price) in to_fetch {
            let live_data = self.live_data.clone();
            tokio::spawn(async move {
                let result = video_info::fetch_video_info(&video_id).await;
                let mut data = live_data.lock().await;
                data.update_video_request_info(&request_id, result);
            });
        }
    }

    /// 标记点播为已看
    pub async fn mark_video_watched(&self, request_id: &str, watched: bool) {
        let mut data = self.live_data.lock().await;
        data.set_video_watched(request_id, watched);
    }

    /// 删除点播请求
    pub async fn remove_video_request(&self, request_id: &str) {
        let mut data = self.live_data.lock().await;
        data.remove_video_request(request_id);
    }

    /// 清空已看
    pub async fn clear_watched_videos(&self) {
        let mut data = self.live_data.lock().await;
        data.clear_watched_videos();
    }

    /// 清空所有
    pub async fn clear_all_videos(&self) {
        let mut data = self.live_data.lock().await;
        data.clear_all_videos();
    }

    pub async fn connect(
        &self,
        app: AppHandle,
        room_id: u64,
        cookie: Option<String>,
    ) -> ConnectResult {
        // 检查 Cookie
        if cookie.is_none() || cookie.as_ref().map(|c| c.is_empty()).unwrap_or(true) {
            return ConnectResult {
                success: false,
                message: "请先设置 Cookie".to_string(),
                room_info: None,
            };
        }

        let cookie = cookie.unwrap();

        // 先断开现有连接
        self.disconnect().await;

        // 清空数据
        self.live_data.lock().await.clear();

        // 更新状态为连接中
        {
            let mut state = self.state.write().await;
            state.status = ConnectionStatus::Connecting;
            state.room_id = room_id;
        }

        let _ = app.emit("blive-status", ConnectionStatus::Connecting);

        // 预检查
        let http_client = reqwest::Client::new();

        let room_info = match get_room_init(&http_client, room_id).await {
            Ok(info) => info,
            Err(e) => {
                let msg = format!("获取房间信息失败: {}", e);
                self.set_error(&app, &msg).await;
                return ConnectResult {
                    success: false,
                    message: msg,
                    room_info: None,
                };
            }
        };

        let danmu_info = match get_danmu_info(&http_client, room_info.room_id, Some(&cookie)).await
        {
            Ok(info) => info,
            Err(e) => {
                let msg = format!("获取弹幕服务器失败: {}", e);
                self.set_error(&app, &msg).await;
                return ConnectResult {
                    success: false,
                    message: msg,
                    room_info: None,
                };
            }
        };

        if danmu_info.host_list.is_empty() || danmu_info.token.is_empty() {
            let msg = "Cookie 无效或已过期，无法获取弹幕服务器".to_string();
            self.set_error(&app, &msg).await;
            return ConnectResult {
                success: false,
                message: msg,
                room_info: None,
            };
        }

        {
            let mut state = self.state.write().await;
            state.room_info = Some(room_info.clone());
        }

        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);

        let app_clone = app.clone();
        let service = app.state::<Arc<BliveService>>().inner().clone();
        let cookie_clone = cookie.clone();
        let room_info_for_rank = room_info.clone();

        let task = tokio::spawn(async move {
            let client = match BliveDmClient::builder()
                .room_id(room_id)
                .cookie(cookie_clone.clone())
                .auto_reconnect(true)
                .build()
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    let msg = format!("创建客户端失败: {}", e);
                    service.set_error(&app_clone, &msg).await;
                    return;
                }
            };

            let mut stream = match client.connect().await {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!("连接失败: {}", e);
                    service.set_error(&app_clone, &msg).await;
                    return;
                }
            };

            {
                let mut state = service.state.write().await;
                state.status = ConnectionStatus::Connected;
            }
            let _ = app_clone.emit("blive-status", ConnectionStatus::Connected);

            // 启动存档会话
            let archive = app_clone.state::<Arc<ArchiveManager>>().inner().clone();
            let room_title = room_info_for_rank.title.clone();
            let streamer_uid = room_info_for_rank.uid;
            let archive_session_id = match archive
                .start_session(room_id, &room_title, streamer_uid)
                .await
            {
                Ok(id) => {
                    let (tx, rx) = mpsc::unbounded_channel::<ArchiveEvent>();
                    // 设置 archive_tx 到 LiveData
                    service.live_data.lock().await.archive_tx = Some(tx);
                    // 启动写入任务
                    let _writer_handle =
                        crate::archive::spawn_archive_writer(archive.clone(), rx, id);
                    Some(id)
                }
                Err(e) => {
                    log::error!("Failed to start archive session: {}", e);
                    None
                }
            };

            // 获取贡献排行榜（连接成功后立即获取一次）
            let http_client = reqwest::Client::new();
            match get_contribution_rank(
                &http_client,
                room_info_for_rank.room_id,
                room_info_for_rank.uid,
                Some(&cookie_clone),
                1,
                100,
            )
            .await
            {
                Ok(rank) => {
                    log::info!("获取贡献排行榜成功: {} 人", rank.list.len());
                    service
                        .live_data
                        .lock()
                        .await
                        .set_contribution_rank_full(rank.list);
                }
                Err(e) => {
                    log::warn!("获取贡献排行榜失败: {}", e);
                }
            }

            // 启动数据推送任务
            let app_for_push = app_clone.clone();
            let service_for_push = service.clone();
            let push_task = tokio::spawn(async move {
                let mut ticker = interval(DATA_PUSH_INTERVAL);
                loop {
                    ticker.tick().await;
                    service_for_push.push_updates(&app_for_push).await;
                }
            });

            // 事件处理循环
            loop {
                tokio::select! {
                    _ = stop_rx.recv() => {
                        log::info!("Received stop signal");
                        break;
                    }
                    event = stream.next() => {
                        match event {
                            Some(Ok(e)) => {
                                service.process_event(e).await;
                            }
                            Some(Err(e)) => {
                                log::error!("Event error: {}", e);
                                if matches!(e, BliveError::ConnectionClosed) {
                                    let mut state = service.state.write().await;
                                    state.status = ConnectionStatus::Reconnecting;
                                    let _ = app_clone.emit("blive-status", ConnectionStatus::Reconnecting);
                                }
                            }
                            None => {
                                log::info!("Stream ended");
                                break;
                            }
                        }
                    }
                }
            }

            push_task.abort();
            service.push_updates(&app_clone).await;

            // 结束存档会话：先 drop archive_tx 以让 writer flush，再 end_session
            {
                let stats = service.live_data.lock().await.stats.clone();
                service.live_data.lock().await.archive_tx = None; // drop sender, writer will flush & exit
                if archive_session_id.is_some() {
                    // 给 writer 一点时间 flush
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    if let Err(e) = archive.end_session(&stats).await {
                        log::error!("Failed to end archive session: {}", e);
                    }
                }
            }

            {
                let mut state = service.state.write().await;
                state.status = ConnectionStatus::Disconnected;
            }
            let _ = app_clone.emit("blive-status", ConnectionStatus::Disconnected);
        });

        {
            let mut state = self.state.write().await;
            state.stop_tx = Some(stop_tx);
            state.task_handle = Some(task);
            state.status = ConnectionStatus::Connected;
        }

        ConnectResult {
            success: true,
            message: "连接成功".to_string(),
            room_info: Some(room_info.into()),
        }
    }

    pub async fn disconnect(&self) {
        let (stop_tx, task_handle) = {
            let mut state = self.state.write().await;
            state.status = ConnectionStatus::Disconnected;
            (state.stop_tx.take(), state.task_handle.take())
        };

        if let Some(tx) = stop_tx {
            let _ = tx.send(()).await;
        }

        if let Some(handle) = task_handle {
            let _ = handle.await;
        }

        self.live_data.lock().await.clear();
    }

    /// 处理事件
    async fn process_event(&self, event: Event) {
        let mut data = self.live_data.lock().await;

        match event {
            Event::Danmaku(danmaku) => {
                let to_fetch = data.process_danmaku(danmaku);
                drop(data);
                self.spawn_video_fetches(to_fetch).await;
            }
            Event::Gift(gift) => data.process_gift(gift),
            Event::SuperChat(sc) => {
                let to_fetch = data.process_superchat(sc);
                drop(data);
                self.spawn_video_fetches(to_fetch).await;
            }
            Event::GuardBuy(guard) => data.process_guard_buy(guard),
            Event::OnlineRankV2(rank) => data.process_online_rank(rank),
            Event::OnlineRankCount(count) => data.process_online_count(count),
            Event::LiveStart(live_data) => {
                log::info!(
                    "Live started: room_id={}, live_time={}",
                    live_data.room_id,
                    live_data.live_time
                );
                // 更新 room_info 的 live_status
                drop(data); // 先释放 live_data 锁，避免死锁
                {
                    let mut state = self.state.write().await;
                    if let Some(ref mut room_info) = state.room_info {
                        room_info.live_status = 1; // 1 = 直播中
                    }
                }
                self.live_data.lock().await.pending_updates.push(DataUpdate::LiveStart);
            }
            Event::LiveStop(preparing) => {
                log::info!(
                    "Live stopped: room_id={}, round={}",
                    preparing.room_id,
                    preparing.round
                );
                // 更新 room_info 的 live_status
                drop(data); // 先释放 live_data 锁，避免死锁
                {
                    let mut state = self.state.write().await;
                    if let Some(ref mut room_info) = state.room_info {
                        room_info.live_status = if preparing.round == 1 { 2 } else { 0 };
                        // 0 = 未开播, 2 = 轮播中
                    }
                }
                self.live_data.lock().await.pending_updates.push(DataUpdate::LiveStop);
            }
            Event::Raw { .. } => {} // 忽略未处理的命令
        }
    }

    /// 推送更新到前端（按窗口订阅过滤）
    async fn push_updates(&self, app: &AppHandle) {
        let updates = {
            let mut data = self.live_data.lock().await;
            data.take_pending_updates()
        };

        if updates.is_empty() {
            return;
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
                .filter(|u| sub.event_types.contains(&u.event_type()))
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
            let mut state = self.state.write().await;
            state.status = status.clone();
        }
        let _ = app.emit("blive-status", status);
    }
}
