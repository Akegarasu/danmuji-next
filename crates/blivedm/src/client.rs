//! 弹幕客户端核心。
//!
//! 公开入口保留在本模块；连接状态机、配置、生命周期和原始捕获分别由子模块负责。

mod capture;
mod connection;
mod lifecycle;
mod options;

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::Stream;
use reqwest::Client as HttpClient;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;

use crate::api::{
    extract_buvid_from_cookie, extract_uid_from_cookie, get_danmu_info, get_room_init,
    DanmuServerInfo, RoomInfo,
};
use crate::error::{Error, Result};
use crate::message::Event;

use capture::RawCaptureTarget;
use connection::{ConnectionChannels, ConnectionTask};

pub use capture::{DecodeStage, RawCapture, RawCaptureMode, RawEventHandler};
pub use lifecycle::{
    CancellationToken, ConnectionState, DisconnectReason, LifecycleEvent, OnlineTrigger,
};
pub use options::ConnectionOptions;

/// 弹幕客户端。
pub struct BliveDmClient {
    room_info: RoomInfo,
    danmu_info: DanmuServerInfo,
    uid: u64,
    buvid: String,
    auto_reconnect: bool,
    reconnect_interval: Duration,
    raw_event_handler: Option<Arc<RawEventHandler>>,
    raw_capture: Option<RawCaptureTarget>,
}

/// 客户端配置 Builder。
#[derive(Default)]
pub struct BliveDmClientBuilder {
    room_id: Option<u64>,
    cookie: Option<String>,
    auto_reconnect: bool,
    reconnect_interval: Duration,
    raw_event_handler: Option<Arc<RawEventHandler>>,
    raw_capture: Option<RawCaptureTarget>,
}

impl BliveDmClientBuilder {
    /// 设置房间号（必需）。
    pub fn room_id(mut self, room_id: u64) -> Self {
        self.room_id = Some(room_id);
        self
    }

    /// 设置 Cookie（可选，用于获取更完整的弹幕服务器信息）。
    pub fn cookie(mut self, cookie: impl Into<String>) -> Self {
        self.cookie = Some(cookie.into());
        self
    }

    /// 是否自动重连（默认 false）。
    pub fn auto_reconnect(mut self, enabled: bool) -> Self {
        self.auto_reconnect = enabled;
        self
    }

    /// 重连间隔（默认 3 秒）。
    pub fn reconnect_interval(mut self, duration: Duration) -> Self {
        self.reconnect_interval = duration;
        self
    }

    /// 设置原始 JSON 事件回调。
    ///
    /// 此兼容 API 同步执行，回调不得执行阻塞 I/O。生产采集应优先使用
    /// [`Self::raw_capture`]，在调用方任务中消费有界通道。
    pub fn raw_event_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(&Value) + Send + Sync + 'static,
    {
        self.raw_event_handler = Some(Arc::new(handler));
        self
    }

    /// 配置原始字节的非阻塞旁路。
    ///
    /// 库只调用 `try_send`。调用方应在独立任务中快速消费并写入 durable spool；
    /// 可通过 [`EventStream::dropped_raw_captures`] 监控通道溢出。
    pub fn raw_capture(mut self, sender: mpsc::Sender<RawCapture>, mode: RawCaptureMode) -> Self {
        self.raw_capture = Some(RawCaptureTarget::new(sender, mode));
        self
    }

    /// 构建客户端。
    pub async fn build(self) -> Result<BliveDmClient> {
        let room_id = self
            .room_id
            .ok_or(Error::Config("room_id is required".to_string()))?;

        let http_client = HttpClient::new();
        let room_info = get_room_init(&http_client, room_id).await?;
        let danmu_info =
            get_danmu_info(&http_client, room_info.room_id, self.cookie.as_deref()).await?;

        let uid = self
            .cookie
            .as_ref()
            .and_then(|cookie| extract_uid_from_cookie(cookie))
            .unwrap_or(0);
        let buvid = self
            .cookie
            .as_ref()
            .and_then(|cookie| extract_buvid_from_cookie(cookie))
            .unwrap_or_default();

        Ok(BliveDmClient {
            room_info,
            danmu_info,
            uid,
            buvid,
            auto_reconnect: self.auto_reconnect,
            reconnect_interval: if self.reconnect_interval.is_zero() {
                Duration::from_secs(3)
            } else {
                self.reconnect_interval
            },
            raw_event_handler: self.raw_event_handler,
            raw_capture: self.raw_capture,
        })
    }
}

impl BliveDmClient {
    /// 创建 Builder。
    pub fn builder() -> BliveDmClientBuilder {
        BliveDmClientBuilder {
            reconnect_interval: Duration::from_secs(3),
            ..Default::default()
        }
    }

    /// 获取房间信息。
    pub fn room_info(&self) -> &RoomInfo {
        &self.room_info
    }

    /// 获取经过 B 站 `room_init` 解析后的真实房间号。
    ///
    /// Builder 接受短房号或真实房号；归档、去重等需要稳定标识的调用方应使用
    /// 此值，而不是最初传入 Builder 的值。
    pub fn canonical_room_id(&self) -> u64 {
        self.room_info.room_id
    }

    /// 获取房间的短房号。没有短房号时返回 `None`（上游以 `0` 表示不存在）。
    pub fn short_room_id(&self) -> Option<u64> {
        (self.room_info.short_id != 0).then_some(self.room_info.short_id)
    }

    /// 使用默认参数连接，是否重连由 Builder 的 `auto_reconnect` 决定。
    ///
    /// 保留原有 API 和返回时机：后台任务启动后立即返回事件流。需要判断真正在线时，
    /// 请读取 [`EventStream::state`] 或生命周期事件。
    pub async fn connect(self) -> Result<EventStream> {
        let auto_reconnect = self.auto_reconnect;
        self.start(ConnectionOptions::default(), auto_reconnect)
    }

    /// 使用指定参数连接，是否重连由 Builder 的 `auto_reconnect` 决定。
    pub async fn connect_with_options(self, options: ConnectionOptions) -> Result<EventStream> {
        let auto_reconnect = self.auto_reconnect;
        self.start(options, auto_reconnect)
    }

    /// 只执行一次 WebSocket 会话，从不在库内自动重连。
    ///
    /// 该入口适合由上层 supervisor 重新获取 host/token、选择 Cookie 并实施退避。
    pub async fn connect_once(self) -> Result<EventStream> {
        self.start(ConnectionOptions::default(), false)
    }

    /// 使用指定参数执行一次 WebSocket 会话，从不在库内自动重连。
    pub async fn connect_once_with_options(
        self,
        options: ConnectionOptions,
    ) -> Result<EventStream> {
        self.start(options, false)
    }

    fn start(self, options: ConnectionOptions, auto_reconnect: bool) -> Result<EventStream> {
        options.validate()?;
        if self.danmu_info.host_list.is_empty() {
            return Err(Error::Config(
                "danmaku server host list is empty".to_string(),
            ));
        }

        let (event_tx, event_rx) = mpsc::channel(options.event_buffer_capacity);
        let (lifecycle_tx, lifecycle_rx) = broadcast::channel(options.lifecycle_buffer_capacity);
        let (state_tx, state_rx) = watch::channel(ConnectionState::Starting);
        let cancellation = CancellationToken::new();
        let dropped_raw_captures = self
            .raw_capture
            .as_ref()
            .map(RawCaptureTarget::dropped_counter)
            .unwrap_or_else(|| Arc::new(AtomicU64::new(0)));

        let connection = ConnectionTask::new(
            Arc::new(self),
            options,
            auto_reconnect,
            ConnectionChannels::new(event_tx, lifecycle_tx.clone(), state_tx),
            cancellation.clone(),
        );
        let task = tokio::spawn(connection.run());

        Ok(EventStream {
            rx: event_rx,
            lifecycle_tx,
            initial_lifecycle_rx: Some(lifecycle_rx),
            state_rx,
            cancellation,
            task: Some(task),
            dropped_raw_captures,
        })
    }
}

/// 业务事件流及其连接控制句柄。
pub struct EventStream {
    rx: mpsc::Receiver<Result<Event>>,
    lifecycle_tx: broadcast::Sender<LifecycleEvent>,
    initial_lifecycle_rx: Option<broadcast::Receiver<LifecycleEvent>>,
    state_rx: watch::Receiver<ConnectionState>,
    cancellation: CancellationToken,
    task: Option<JoinHandle<()>>,
    dropped_raw_captures: Arc<AtomicU64>,
}

impl EventStream {
    /// 检查后台事件通道是否仍打开。
    ///
    /// 为兼容旧语义，该方法不代表已经通过入房认证。精确状态请使用 [`Self::state`]。
    pub fn is_connected(&self) -> bool {
        !self.rx.is_closed()
    }

    /// 返回当前最新连接状态。
    pub fn state(&self) -> ConnectionState {
        self.state_rx.borrow().clone()
    }

    /// 订阅最新连接状态。
    pub fn subscribe_state(&self) -> watch::Receiver<ConnectionState> {
        self.state_rx.clone()
    }

    /// 取得从连接任务启动前就已注册的生命周期接收端。
    ///
    /// 这可避免“订阅发生在事件之后”的窗口，但接收端仍遵循 Tokio `broadcast`
    /// 的有界滞后语义，调用方必须处理 `RecvError::Lagged`。后续调用返回 `None`；
    /// 如需额外订阅者，使用 [`Self::subscribe_lifecycle`]。
    pub fn take_lifecycle_receiver(&mut self) -> Option<broadcast::Receiver<LifecycleEvent>> {
        self.initial_lifecycle_rx.take()
    }

    /// 从调用时刻开始订阅生命周期事件。
    pub fn subscribe_lifecycle(&self) -> broadcast::Receiver<LifecycleEvent> {
        self.lifecycle_tx.subscribe()
    }

    /// 返回可由上层 supervisor 克隆和组合的取消令牌。
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// 请求本次连接及其自动重连循环退出。
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// 原始旁路通道因满或关闭而丢弃的数据条数。
    pub fn dropped_raw_captures(&self) -> u64 {
        self.dropped_raw_captures.load(Ordering::Relaxed)
    }
}

impl Stream for EventStream {
    type Item = Result<Event>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(task) = self.task.take() {
            // 连接循环没有子任务；abort 会立即释放 socket，防止调用方丢弃流后留下任务。
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::DanmuHost;

    #[test]
    fn exposes_stable_room_identity_without_changing_room_info_api() {
        let client = BliveDmClient {
            room_info: RoomInfo {
                room_id: 6_789,
                short_id: 123,
                uid: 2,
                live_status: 1,
                title: "test".to_string(),
            },
            danmu_info: DanmuServerInfo {
                token: "token".to_string(),
                host_list: vec![DanmuHost {
                    host: "localhost".to_string(),
                    port: 0,
                    wss_port: 0,
                    ws_port: 0,
                }],
            },
            uid: 0,
            buvid: String::new(),
            auto_reconnect: false,
            reconnect_interval: Duration::from_millis(10),
            raw_event_handler: None,
            raw_capture: None,
        };

        assert_eq!(client.canonical_room_id(), 6_789);
        assert_eq!(client.short_room_id(), Some(123));
        assert_eq!(client.room_info().room_id, 6_789);
    }
}
