//! 弹幕客户端核心

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, Stream, StreamExt};
use reqwest::Client as HttpClient;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::blivedm::api::{
    extract_buvid_from_cookie, extract_uid_from_cookie, get_danmu_info, get_room_init,
    DanmuServerInfo, RoomInfo,
};
use crate::blivedm::error::{Error, Result};
use crate::blivedm::message::{parse_event, Event};
use crate::blivedm::packet::Packet;

/// 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// 连接超时
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// 弹幕客户端
pub struct BliveDmClient {
    room_info: RoomInfo,
    danmu_info: DanmuServerInfo,
    uid: u64,
    buvid: String,
    auto_reconnect: bool,
    reconnect_interval: Duration,
}

/// 客户端配置 Builder
#[derive(Default)]
pub struct BliveDmClientBuilder {
    room_id: Option<u64>,
    cookie: Option<String>,
    auto_reconnect: bool,
    reconnect_interval: Duration,
}

impl BliveDmClientBuilder {
    /// 设置房间号（必需）
    pub fn room_id(mut self, room_id: u64) -> Self {
        self.room_id = Some(room_id);
        self
    }

    /// 设置 Cookie（可选，用于获取更完整的弹幕服务器信息）
    pub fn cookie(mut self, cookie: impl Into<String>) -> Self {
        self.cookie = Some(cookie.into());
        self
    }

    /// 是否自动重连（默认 false）
    pub fn auto_reconnect(mut self, enabled: bool) -> Self {
        self.auto_reconnect = enabled;
        self
    }

    /// 重连间隔（默认 3 秒）
    pub fn reconnect_interval(mut self, duration: Duration) -> Self {
        self.reconnect_interval = duration;
        self
    }

    /// 构建客户端
    pub async fn build(self) -> Result<BliveDmClient> {
        let room_id = self
            .room_id
            .ok_or(Error::Config("room_id is required".to_string()))?;

        let http_client = HttpClient::new();

        // 获取房间信息
        let room_info = get_room_init(&http_client, room_id).await?;

        // 获取弹幕服务器信息
        let danmu_info =
            get_danmu_info(&http_client, room_info.room_id, self.cookie.as_deref()).await?;

        // 从 cookie 提取用户信息
        let uid = self
            .cookie
            .as_ref()
            .and_then(|c| extract_uid_from_cookie(c))
            .unwrap_or(0);
        let buvid = self
            .cookie
            .as_ref()
            .and_then(|c| extract_buvid_from_cookie(c))
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
        })
    }
}

impl Default for BliveDmClient {
    fn default() -> Self {
        Self {
            room_info: RoomInfo {
                room_id: 0,
                short_id: 0,
                uid: 0,
                live_status: 0,
                title: String::new(),
            },
            danmu_info: DanmuServerInfo {
                token: String::new(),
                host_list: vec![],
            },
            uid: 0,
            buvid: String::new(),
            auto_reconnect: false,
            reconnect_interval: Duration::from_secs(3),
        }
    }
}

impl BliveDmClient {
    /// 创建 Builder
    pub fn builder() -> BliveDmClientBuilder {
        BliveDmClientBuilder {
            reconnect_interval: Duration::from_secs(3),
            ..Default::default()
        }
    }

    /// 获取房间信息
    pub fn room_info(&self) -> &RoomInfo {
        &self.room_info
    }

    /// 连接并返回事件流
    pub async fn connect(self) -> Result<EventStream> {
        let (event_tx, event_rx) = mpsc::channel(256);

        let client = Arc::new(self);
        let client_clone = Arc::clone(&client);

        // 启动连接任务
        tokio::spawn(async move {
            connection_loop(client_clone, event_tx).await;
        });

        Ok(EventStream {
            rx: event_rx,
            _client: client,
        })
    }
}

/// 连接循环
async fn connection_loop(client: Arc<BliveDmClient>, event_tx: mpsc::Sender<Result<Event>>) {
    let mut retry_count = 0;

    loop {
        let host_index = retry_count % client.danmu_info.host_list.len();
        let host = &client.danmu_info.host_list[host_index];
        let ws_url = format!("wss://{}:{}/sub", host.host, host.wss_port);

        log::info!("Connecting to {} (attempt {})", ws_url, retry_count + 1);

        match connect_and_run(&client, &ws_url, &event_tx).await {
            Ok(()) => {
                // 正常关闭
                log::info!("Connection closed normally");
                break;
            }
            Err(e) => {
                log::error!("Connection error: {}", e);

                if !client.auto_reconnect {
                    let _ = event_tx.send(Err(e)).await;
                    break;
                }

                // 等待后重试
                tokio::time::sleep(client.reconnect_interval).await;
                retry_count += 1;
            }
        }
    }
}

/// 连接并运行
async fn connect_and_run(
    client: &BliveDmClient,
    ws_url: &str,
    event_tx: &mpsc::Sender<Result<Event>>,
) -> Result<()> {
    // 连接 WebSocket
    let ws_stream = timeout(CONNECT_TIMEOUT, connect_async(ws_url))
        .await
        .map_err(|_| Error::ConnectionClosed)?
        .map_err(Error::WebSocket)?
        .0;

    let (mut write, read) = ws_stream.split();

    // 发送进入房间包
    let enter_packet = Packet::enter_room(
        client.uid,
        &client.buvid,
        client.room_info.room_id,
        &client.danmu_info.token,
    );
    write
        .send(Message::Binary(enter_packet.to_bytes()))
        .await
        .map_err(Error::WebSocket)?;

    log::debug!("Sent enter room packet");

    // 启动心跳任务
    let (heartbeat_tx, heartbeat_rx) = mpsc::channel::<()>(1);
    let heartbeat_handle = tokio::spawn(heartbeat_loop(write, heartbeat_rx));

    // 消息处理循环
    let result = message_loop(read, event_tx).await;

    // 停止心跳
    drop(heartbeat_tx);
    let _ = heartbeat_handle.await;

    result
}

/// 心跳循环
async fn heartbeat_loop(
    mut write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    mut stop_rx: mpsc::Receiver<()>,
) {
    let mut heartbeat_interval = interval(HEARTBEAT_INTERVAL);

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                let heartbeat = Packet::heartbeat();
                if write.send(Message::Binary(heartbeat.to_bytes())).await.is_err() {
                    break;
                }
                log::debug!("Sent heartbeat");
            }
            _ = stop_rx.recv() => {
                break;
            }
        }
    }
}

/// 消息处理循环
async fn message_loop(
    mut read: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    event_tx: &mpsc::Sender<Result<Event>>,
) -> Result<()> {
    while let Some(msg_result) = read.next().await {
        let msg = msg_result.map_err(Error::WebSocket)?;

        match msg {
            Message::Binary(data) => {
                if let Err(e) = process_message(&data, event_tx).await {
                    log::error!("Failed to process message: {}", e);
                }
            }
            Message::Close(_) => {
                log::info!("Received close frame");
                return Err(Error::ConnectionClosed);
            }
            Message::Ping(data) => {
                log::debug!("Received ping");
                // Pong 由 tungstenite 自动处理
                let _ = data;
            }
            _ => {}
        }
    }

    Err(Error::ConnectionClosed)
}

/// 处理消息
async fn process_message(data: &[u8], event_tx: &mpsc::Sender<Result<Event>>) -> Result<()> {
    let packet = Packet::from_bytes(data)?;

    // 解压并切分数据包
    let packets = packet.parse()?;

    for pkt in packets {
        if let Some(event) = parse_event(&pkt) {
            log::debug!("Event: {:?}", event);

            if event_tx.send(Ok(event)).await.is_err() {
                // 接收端已关闭
                return Err(Error::ConnectionClosed);
            }
        }
    }

    Ok(())
}

/// 事件流
pub struct EventStream {
    rx: mpsc::Receiver<Result<Event>>,
    _client: Arc<BliveDmClient>,
}

impl EventStream {
    /// 检查连接状态
    pub fn is_connected(&self) -> bool {
        !self.rx.is_closed()
    }
}

impl Stream for EventStream {
    type Item = Result<Event>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

// Drop 时自动清理（通过 Arc 引用计数）
