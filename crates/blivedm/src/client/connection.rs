use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{interval_at, sleep_until, timeout, Instant, MissedTickBehavior};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, WebSocketStream};

use crate::error::{Error, Result};
use crate::message::{parse_notification, Event};
use crate::packet::{Operation, Packet};

use super::capture::DecodeStage;
use super::lifecycle::{
    CancellationToken, ConnectionState, DisconnectReason, LifecycleEvent, OnlineTrigger,
};
use super::options::ConnectionOptions;
use super::BliveDmClient;

const CANCEL_CLOSE_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) struct ConnectionChannels {
    event_tx: mpsc::Sender<Result<Event>>,
    lifecycle_tx: broadcast::Sender<LifecycleEvent>,
    state_tx: watch::Sender<ConnectionState>,
}

impl ConnectionChannels {
    pub(super) fn new(
        event_tx: mpsc::Sender<Result<Event>>,
        lifecycle_tx: broadcast::Sender<LifecycleEvent>,
        state_tx: watch::Sender<ConnectionState>,
    ) -> Self {
        Self {
            event_tx,
            lifecycle_tx,
            state_tx,
        }
    }
}

/// 持有单个客户端连接循环所需的共享上下文，避免在各层函数间重复传递参数。
pub(super) struct ConnectionTask {
    client: Arc<BliveDmClient>,
    options: ConnectionOptions,
    auto_reconnect: bool,
    event_tx: mpsc::Sender<Result<Event>>,
    lifecycle_tx: broadcast::Sender<LifecycleEvent>,
    state_tx: watch::Sender<ConnectionState>,
    cancellation: CancellationToken,
}

impl ConnectionTask {
    pub(super) fn new(
        client: Arc<BliveDmClient>,
        options: ConnectionOptions,
        auto_reconnect: bool,
        channels: ConnectionChannels,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            client,
            options,
            auto_reconnect,
            event_tx: channels.event_tx,
            lifecycle_tx: channels.lifecycle_tx,
            state_tx: channels.state_tx,
            cancellation,
        }
    }

    pub(super) async fn run(self) {
        let mut attempt = 1_u64;

        loop {
            if self.cancellation.is_cancelled() {
                self.finish_cancelled();
                break;
            }
            if self.event_tx.is_closed() {
                self.finish_closed(Some(DisconnectReason::EventConsumerDropped));
                break;
            }

            let endpoint = self.endpoint_for_attempt(attempt);
            log::info!("Connecting to {} (attempt {})", endpoint, attempt);
            self.publish_connecting(attempt, &endpoint);

            let failure = match self.connect_and_run(attempt, &endpoint).await {
                Ok(()) => {
                    self.finish_closed(None);
                    break;
                }
                Err(failure) => failure,
            };

            if failure.reason == DisconnectReason::Cancelled {
                self.finish_cancelled();
                break;
            }
            if failure.reason == DisconnectReason::EventConsumerDropped {
                self.finish_closed(Some(failure.reason));
                break;
            }

            log::warn!("Connection attempt {} ended: {:?}", attempt, failure.reason);
            let _ = self.lifecycle_tx.send(LifecycleEvent::Disconnected {
                attempt,
                reason: failure.reason.clone(),
            });

            if !self.auto_reconnect {
                let reason = failure.reason.clone();
                if let Some(error) = failure.public_error {
                    tokio::select! {
                        _ = self.cancellation.cancelled() => {
                            self.finish_cancelled();
                            break;
                        }
                        result = self.event_tx.send(Err(error)) => {
                            if result.is_err() {
                                self.finish_closed(Some(
                                    DisconnectReason::EventConsumerDropped,
                                ));
                                break;
                            }
                        }
                    }
                }
                self.finish_closed(Some(reason));
                break;
            }

            let next_attempt = attempt.saturating_add(1);
            let delay = self.client.reconnect_interval;
            let reason = failure.reason;
            let _ = self.state_tx.send_replace(ConnectionState::Backoff {
                next_attempt,
                delay,
                reason: reason.clone(),
            });
            let _ = self.lifecycle_tx.send(LifecycleEvent::ReconnectScheduled {
                next_attempt,
                delay,
                reason,
            });

            tokio::select! {
                _ = self.cancellation.cancelled() => {
                    self.finish_cancelled();
                    break;
                }
                _ = self.event_tx.closed() => {
                    self.finish_closed(Some(DisconnectReason::EventConsumerDropped));
                    break;
                }
                _ = tokio::time::sleep(delay) => {
                    attempt = next_attempt;
                }
            }
        }
    }

    fn endpoint_for_attempt(&self, attempt: u64) -> String {
        let host_index = ((attempt - 1) as usize) % self.client.danmu_info.host_list.len();
        let host = &self.client.danmu_info.host_list[host_index];
        format!("wss://{}:{}/sub", host.host, host.wss_port)
    }

    fn publish_connecting(&self, attempt: u64, endpoint: &str) {
        let endpoint = endpoint.to_string();
        let _ = self.state_tx.send_replace(ConnectionState::Connecting {
            attempt,
            endpoint: endpoint.clone(),
        });
        let _ = self
            .lifecycle_tx
            .send(LifecycleEvent::Connecting { attempt, endpoint });
    }

    fn finish_cancelled(&self) {
        let _ = self.state_tx.send_replace(ConnectionState::Cancelled);
        let _ = self.lifecycle_tx.send(LifecycleEvent::Cancelled);
    }

    fn finish_closed(&self, reason: Option<DisconnectReason>) {
        let _ = self.state_tx.send_replace(ConnectionState::Closed {
            reason: reason.clone(),
        });
        let _ = self.lifecycle_tx.send(LifecycleEvent::Closed { reason });
    }

    async fn connect_and_run(
        &self,
        attempt: u64,
        endpoint: &str,
    ) -> std::result::Result<(), SessionFailure> {
        let connect = timeout(self.options.connect_timeout, connect_async(endpoint));
        tokio::pin!(connect);

        let ws_stream = tokio::select! {
            _ = self.cancellation.cancelled() => return Err(SessionFailure::cancelled()),
            _ = self.event_tx.closed() => return Err(SessionFailure::consumer_dropped()),
            result = &mut connect => {
                match result {
                    Ok(Ok((stream, _response))) => stream,
                    Ok(Err(error)) => return Err(SessionFailure::transport(Error::from(error))),
                    Err(_) => return Err(SessionFailure::connect_timeout()),
                }
            }
        };

        let endpoint = endpoint.to_string();
        let _ = self.lifecycle_tx.send(LifecycleEvent::WebSocketConnected {
            attempt,
            endpoint: endpoint.clone(),
        });
        let _ = self.state_tx.send_replace(ConnectionState::Joining {
            attempt,
            endpoint: endpoint.clone(),
        });

        self.run_websocket(ws_stream, attempt, endpoint).await
    }

    async fn run_websocket<S>(
        &self,
        mut websocket: WebSocketStream<S>,
        attempt: u64,
        endpoint: String,
    ) -> std::result::Result<(), SessionFailure>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let enter_packet = Packet::enter_room(
            self.client.uid,
            &self.client.buvid,
            self.client.room_info.room_id,
            &self.client.danmu_info.token,
        );

        tokio::select! {
            _ = self.cancellation.cancelled() => return Err(SessionFailure::cancelled()),
            _ = self.event_tx.closed() => return Err(SessionFailure::consumer_dropped()),
            result = websocket.send(Message::Binary(enter_packet.to_bytes())) => {
                result.map_err(|error| SessionFailure::transport(Error::from(error)))?;
            }
        }
        let _ = self
            .lifecycle_tx
            .send(LifecycleEvent::EnterRoomSent { attempt });

        let mut heartbeat = interval_at(
            Instant::now() + self.options.heartbeat_interval,
            self.options.heartbeat_interval,
        );
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let join_deadline = sleep_until(Instant::now() + self.options.join_timeout);
        tokio::pin!(join_deadline);

        let idle_deadline = sleep_until(
            Instant::now()
                + self
                    .options
                    .read_idle_timeout
                    .unwrap_or(Duration::from_secs(24 * 60 * 60)),
        );
        tokio::pin!(idle_deadline);
        let mut online = false;

        loop {
            tokio::select! {
                biased;

                _ = self.cancellation.cancelled() => {
                    let _ = timeout(
                        CANCEL_CLOSE_TIMEOUT,
                        websocket.send(Message::Close(None)),
                    )
                    .await;
                    return Err(SessionFailure::cancelled());
                }
                _ = self.event_tx.closed() => {
                    let _ = timeout(
                        CANCEL_CLOSE_TIMEOUT,
                        websocket.send(Message::Close(None)),
                    )
                    .await;
                    return Err(SessionFailure::consumer_dropped());
                }
                _ = &mut join_deadline, if !online => {
                    return Err(SessionFailure::join_timeout());
                }
                _ = &mut idle_deadline,
                    if online && self.options.read_idle_timeout.is_some() =>
                {
                    return Err(SessionFailure::read_idle_timeout());
                }
                _ = heartbeat.tick() => {
                    let packet = Packet::heartbeat();
                    tokio::select! {
                        _ = self.cancellation.cancelled() => {
                            return Err(SessionFailure::cancelled());
                        }
                        result = websocket.send(Message::Binary(packet.to_bytes())) => {
                            result.map_err(|error| {
                                SessionFailure::transport(Error::from(error))
                            })?;
                        }
                    }
                    let _ = self
                        .lifecycle_tx
                        .send(LifecycleEvent::HeartbeatSent { attempt });
                    log::debug!("Sent heartbeat");
                }
                message = websocket.next() => {
                    let message = match message {
                        Some(Ok(message)) => message,
                        Some(Err(error)) => {
                            return Err(SessionFailure::transport(Error::from(error)));
                        }
                        None => {
                            return Err(SessionFailure::remote_closed(None, String::new()));
                        }
                    };

                    match message {
                        Message::Binary(data) => {
                            let frame: Arc<[u8]> = Arc::from(data.into_boxed_slice());
                            let activity = self.process_frame(frame, attempt).await?;

                            if !online {
                                if let Some(trigger) = activity.online_trigger() {
                                    online = true;
                                    let _ = self.state_tx.send_replace(ConnectionState::Online {
                                        attempt,
                                        endpoint: endpoint.clone(),
                                    });
                                    let _ = self.lifecycle_tx.send(LifecycleEvent::Online {
                                        attempt,
                                        trigger,
                                    });
                                }
                            }

                            if online && activity.is_valid_response() {
                                if let Some(read_idle_timeout) = self.options.read_idle_timeout {
                                    idle_deadline
                                        .as_mut()
                                        .reset(Instant::now() + read_idle_timeout);
                                }
                            }
                        }
                        Message::Ping(data) => {
                            tokio::select! {
                                _ = self.cancellation.cancelled() => {
                                    return Err(SessionFailure::cancelled());
                                }
                                result = websocket.send(Message::Pong(data)) => {
                                    result.map_err(|error| {
                                        SessionFailure::transport(Error::from(error))
                                    })?;
                                }
                            }
                        }
                        Message::Pong(_) => {}
                        Message::Close(frame) => {
                            let (code, reason) = frame
                                .map(|frame| {
                                    (Some(u16::from(frame.code)), frame.reason.to_string())
                                })
                                .unwrap_or((None, String::new()));
                            return Err(SessionFailure::remote_closed(code, reason));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    async fn process_frame(
        &self,
        frame: Arc<[u8]>,
        attempt: u64,
    ) -> std::result::Result<FrameActivity, SessionFailure> {
        if let Some(target) = &self.client.raw_capture {
            target.capture_frame(Arc::clone(&frame));
        }

        let packet = match Packet::from_bytes(&frame) {
            Ok(packet) => packet,
            Err(error) => {
                self.report_decode_error(
                    attempt,
                    DecodeStage::PacketEnvelope,
                    frame,
                    error.to_string(),
                );
                return Ok(FrameActivity::default());
            }
        };

        let packets = match packet.parse() {
            Ok(packets) => packets,
            Err(error) => {
                self.report_decode_error(
                    attempt,
                    DecodeStage::PacketPayload,
                    frame,
                    error.to_string(),
                );
                return Ok(FrameActivity::default());
            }
        };

        let mut activity = FrameActivity::default();
        for packet in packets {
            match packet.operation {
                Operation::EnterRoomReply => {
                    let body: Arc<[u8]> = Arc::from(packet.body.into_boxed_slice());
                    let _ = self.lifecycle_tx.send(LifecycleEvent::EnterRoomReply {
                        attempt,
                        body: Arc::clone(&body),
                    });
                    if let Some(message) = enter_room_error(&body) {
                        return Err(SessionFailure::authentication(message));
                    }
                    activity.enter_room_reply = true;
                }
                Operation::HeartbeatReply => {
                    let popularity = packet
                        .body
                        .get(..4)
                        .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
                    let _ = self.lifecycle_tx.send(LifecycleEvent::HeartbeatReply {
                        attempt,
                        popularity,
                    });
                    activity.heartbeat_reply = true;
                }
                Operation::Notification => {
                    let body: Arc<[u8]> = Arc::from(packet.body.into_boxed_slice());
                    if let Some(target) = &self.client.raw_capture {
                        target.capture_notification(Arc::clone(&body));
                    }

                    match parse_notification(&body, self.client.raw_event_handler.as_deref()) {
                        Ok(event) => {
                            activity.notification = true;
                            if let Some(target) = &self.client.raw_capture {
                                target.capture_event_notification(Arc::clone(&body));
                            }
                            tokio::select! {
                                _ = self.cancellation.cancelled() => {
                                    return Err(SessionFailure::cancelled());
                                }
                                result = self.event_tx.send(Ok(event)) => {
                                    if result.is_err() {
                                        return Err(SessionFailure::consumer_dropped());
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            self.report_decode_error(
                                attempt,
                                DecodeStage::Notification,
                                body,
                                error.to_string(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(activity)
    }

    fn report_decode_error(
        &self,
        attempt: u64,
        stage: DecodeStage,
        bytes: Arc<[u8]>,
        error: String,
    ) {
        log::warn!("Failed to decode {:?}: {}", stage, error);
        if let Some(target) = &self.client.raw_capture {
            target.capture_error(stage, bytes, error.clone());
        }
        let _ = self.lifecycle_tx.send(LifecycleEvent::DecodeError {
            attempt,
            stage,
            error,
        });
    }
}

#[derive(Debug)]
struct SessionFailure {
    reason: DisconnectReason,
    public_error: Option<Error>,
}

impl SessionFailure {
    fn cancelled() -> Self {
        Self {
            reason: DisconnectReason::Cancelled,
            public_error: None,
        }
    }

    fn consumer_dropped() -> Self {
        Self {
            reason: DisconnectReason::EventConsumerDropped,
            public_error: None,
        }
    }

    fn connect_timeout() -> Self {
        Self {
            reason: DisconnectReason::ConnectTimeout,
            public_error: Some(Error::ConnectionClosed),
        }
    }

    fn join_timeout() -> Self {
        Self {
            reason: DisconnectReason::JoinTimeout,
            public_error: Some(Error::ConnectionClosed),
        }
    }

    fn read_idle_timeout() -> Self {
        Self {
            reason: DisconnectReason::ReadIdleTimeout,
            public_error: Some(Error::ConnectionClosed),
        }
    }

    fn transport(error: Error) -> Self {
        Self {
            reason: DisconnectReason::Transport(error.to_string()),
            public_error: Some(error),
        }
    }

    fn remote_closed(code: Option<u16>, reason: String) -> Self {
        Self {
            reason: DisconnectReason::RemoteClosed { code, reason },
            public_error: Some(Error::ConnectionClosed),
        }
    }

    fn authentication(message: String) -> Self {
        Self {
            reason: DisconnectReason::Authentication(message.clone()),
            public_error: Some(Error::AuthFailed(message)),
        }
    }
}

#[derive(Default)]
struct FrameActivity {
    enter_room_reply: bool,
    heartbeat_reply: bool,
    notification: bool,
}

impl FrameActivity {
    fn online_trigger(&self) -> Option<OnlineTrigger> {
        if self.enter_room_reply {
            Some(OnlineTrigger::EnterRoomReply)
        } else if self.heartbeat_reply {
            Some(OnlineTrigger::HeartbeatReply)
        } else if self.notification {
            Some(OnlineTrigger::Notification)
        } else {
            None
        }
    }

    fn is_valid_response(&self) -> bool {
        self.enter_room_reply || self.heartbeat_reply || self.notification
    }
}

fn enter_room_error(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let value: Value = serde_json::from_slice(body).ok()?;
    let code = value.get("code").and_then(Value::as_i64)?;
    (code != 0).then(|| {
        value
            .get("message")
            .or_else(|| value.get("msg"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("enter room rejected with code {code}"))
    })
}

#[cfg(test)]
mod tests {
    use super::super::capture::{RawCapture, RawCaptureMode, RawCaptureTarget};
    use super::*;
    use crate::api::{DanmuHost, DanmuServerInfo, RoomInfo};
    use crate::packet::ProtocolVersion;
    use tokio::io::duplex;
    use tokio_tungstenite::tungstenite::protocol::Role;

    struct TestContext {
        task: ConnectionTask,
        event_rx: mpsc::Receiver<Result<Event>>,
        lifecycle_rx: broadcast::Receiver<LifecycleEvent>,
        state_rx: watch::Receiver<ConnectionState>,
        cancellation: CancellationToken,
    }

    fn test_client(raw_capture: Option<RawCaptureTarget>) -> Arc<BliveDmClient> {
        Arc::new(BliveDmClient {
            room_info: RoomInfo {
                room_id: 1,
                short_id: 0,
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
            raw_capture,
        })
    }

    fn test_context(
        raw_capture: Option<RawCaptureTarget>,
        options: ConnectionOptions,
        event_capacity: usize,
        lifecycle_capacity: usize,
    ) -> TestContext {
        let (event_tx, event_rx) = mpsc::channel(event_capacity);
        let (lifecycle_tx, lifecycle_rx) = broadcast::channel(lifecycle_capacity);
        let (state_tx, state_rx) = watch::channel(ConnectionState::Starting);
        let cancellation = CancellationToken::new();
        let task = ConnectionTask::new(
            test_client(raw_capture),
            options,
            false,
            ConnectionChannels::new(event_tx, lifecycle_tx, state_tx),
            cancellation.clone(),
        );

        TestContext {
            task,
            event_rx,
            lifecycle_rx,
            state_rx,
            cancellation,
        }
    }

    async fn websocket_pair() -> (
        WebSocketStream<tokio::io::DuplexStream>,
        WebSocketStream<tokio::io::DuplexStream>,
    ) {
        let (client_io, server_io) = duplex(16 * 1024);
        tokio::join!(
            WebSocketStream::from_raw_socket(client_io, Role::Client, None),
            WebSocketStream::from_raw_socket(server_io, Role::Server, None),
        )
    }

    fn ack_packet() -> Message {
        Message::Binary(
            Packet::new(
                ProtocolVersion::Plain,
                Operation::EnterRoomReply,
                br#"{"code":0}"#.to_vec(),
            )
            .to_bytes(),
        )
    }

    #[tokio::test]
    async fn cancellation_stops_an_online_session() {
        let (client_ws, mut server_ws) = websocket_pair().await;
        let options = ConnectionOptions::default()
            .with_join_timeout(Duration::from_secs(1))
            .with_read_idle_timeout(Some(Duration::from_secs(1)))
            .with_heartbeat_interval(Duration::from_secs(60));
        let TestContext {
            task,
            event_rx: _event_rx,
            lifecycle_rx: _lifecycle_rx,
            mut state_rx,
            cancellation,
        } = test_context(None, options, 4, 16);

        let server = tokio::spawn(async move {
            assert!(matches!(
                server_ws.next().await,
                Some(Ok(Message::Binary(_)))
            ));
            server_ws.send(ack_packet()).await.unwrap();
            let _ = server_ws.next().await;
        });

        let session = tokio::spawn(async move {
            task.run_websocket(client_ws, 1, "mock://room".to_string())
                .await
        });

        timeout(Duration::from_secs(1), async {
            loop {
                if matches!(&*state_rx.borrow(), ConnectionState::Online { .. }) {
                    break;
                }
                state_rx.changed().await.unwrap();
            }
        })
        .await
        .unwrap();

        cancellation.cancel();
        let failure = timeout(Duration::from_secs(1), session)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(failure.reason, DisconnectReason::Cancelled);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn online_session_fails_after_read_idle_timeout() {
        let (client_ws, mut server_ws) = websocket_pair().await;
        let options = ConnectionOptions::default()
            .with_join_timeout(Duration::from_secs(1))
            .with_read_idle_timeout(Some(Duration::from_millis(40)))
            .with_heartbeat_interval(Duration::from_secs(60));
        let TestContext {
            task,
            event_rx: _event_rx,
            lifecycle_rx: _lifecycle_rx,
            state_rx: _state_rx,
            cancellation: _cancellation,
        } = test_context(None, options, 4, 16);

        let server = tokio::spawn(async move {
            assert!(matches!(
                server_ws.next().await,
                Some(Ok(Message::Binary(_)))
            ));
            server_ws.send(ack_packet()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        let failure = timeout(
            Duration::from_secs(1),
            task.run_websocket(client_ws, 1, "mock://room".to_string()),
        )
        .await
        .unwrap()
        .unwrap_err();

        assert_eq!(failure.reason, DisconnectReason::ReadIdleTimeout);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn remote_close_ends_the_single_session() {
        let (client_ws, mut server_ws) = websocket_pair().await;
        let options = ConnectionOptions::default()
            .with_join_timeout(Duration::from_secs(1))
            .with_heartbeat_interval(Duration::from_secs(60));
        let TestContext {
            task,
            event_rx: _event_rx,
            lifecycle_rx: _lifecycle_rx,
            state_rx: _state_rx,
            cancellation: _cancellation,
        } = test_context(None, options, 4, 16);

        let server = tokio::spawn(async move {
            assert!(matches!(
                server_ws.next().await,
                Some(Ok(Message::Binary(_)))
            ));
            server_ws.send(Message::Close(None)).await.unwrap();
        });

        let failure = timeout(
            Duration::from_secs(1),
            task.run_websocket(client_ws, 1, "mock://room".to_string()),
        )
        .await
        .unwrap()
        .unwrap_err();

        assert!(matches!(
            failure.reason,
            DisconnectReason::RemoteClosed { .. }
        ));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn decode_errors_are_reported_without_blocking_the_event_path() {
        let (raw_tx, mut raw_rx) = mpsc::channel(1);
        let raw_capture = RawCaptureTarget::new(raw_tx, RawCaptureMode::ErrorsOnly);
        let TestContext {
            task,
            event_rx: _event_rx,
            mut lifecycle_rx,
            state_rx: _state_rx,
            cancellation: _cancellation,
        } = test_context(Some(raw_capture), ConnectionOptions::default(), 1, 4);

        let activity = task
            .process_frame(Arc::from(&b"invalid"[..]), 1)
            .await
            .unwrap();

        assert!(!activity.is_valid_response());
        assert!(matches!(
            raw_rx.recv().await,
            Some(RawCapture::DecodeError {
                stage: DecodeStage::PacketEnvelope,
                ..
            })
        ));
        assert!(matches!(
            lifecycle_rx.recv().await,
            Ok(LifecycleEvent::DecodeError {
                stage: DecodeStage::PacketEnvelope,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn malformed_notification_reports_its_decompressed_body() {
        let (raw_tx, mut raw_rx) = mpsc::channel(1);
        let raw_capture = RawCaptureTarget::new(raw_tx, RawCaptureMode::ErrorsOnly);
        let context = test_context(Some(raw_capture), ConnectionOptions::default(), 1, 4);
        let malformed_body = b"{not-json";
        let frame: Arc<[u8]> = Arc::from(
            Packet::new(
                ProtocolVersion::Plain,
                Operation::Notification,
                malformed_body.to_vec(),
            )
            .to_bytes()
            .into_boxed_slice(),
        );

        let activity = context.task.process_frame(frame, 1).await.unwrap();

        assert!(!activity.is_valid_response());
        match raw_rx.recv().await {
            Some(RawCapture::DecodeError {
                stage: DecodeStage::Notification,
                bytes,
                ..
            }) => assert_eq!(&*bytes, malformed_body),
            other => panic!("unexpected raw capture: {other:?}"),
        }
    }

    #[tokio::test]
    async fn notification_capture_preserves_exact_body_bytes() {
        let (raw_tx, mut raw_rx) = mpsc::channel(2);
        let raw_capture = RawCaptureTarget::new(raw_tx, RawCaptureMode::Notifications);
        let TestContext {
            task,
            mut event_rx,
            lifecycle_rx: _lifecycle_rx,
            state_rx: _state_rx,
            cancellation: _cancellation,
        } = test_context(Some(raw_capture), ConnectionOptions::default(), 1, 4);
        let body = br#"{"cmd":"UNDERLINE_TEST","value":1}"#.to_vec();
        let frame: Arc<[u8]> = Arc::from(
            Packet::new(
                ProtocolVersion::Plain,
                Operation::Notification,
                body.clone(),
            )
            .to_bytes()
            .into_boxed_slice(),
        );

        let activity = task.process_frame(frame, 1).await.unwrap();

        assert!(activity.notification);
        match raw_rx.recv().await {
            Some(RawCapture::Notification { bytes }) => assert_eq!(&*bytes, &body),
            other => panic!("unexpected raw capture: {other:?}"),
        }
        assert!(matches!(
            event_rx.recv().await,
            Some(Ok(Event::Raw { cmd, .. })) if cmd == "UNDERLINE_TEST"
        ));
    }

    #[tokio::test]
    async fn event_notification_capture_pairs_known_and_unknown_events_in_order() {
        let (raw_tx, mut raw_rx) = mpsc::channel(2);
        let raw_capture = RawCaptureTarget::new(raw_tx, RawCaptureMode::EventNotifications);
        let TestContext {
            task,
            mut event_rx,
            lifecycle_rx: _lifecycle_rx,
            state_rx: _state_rx,
            cancellation: _cancellation,
        } = test_context(Some(raw_capture), ConnectionOptions::default(), 2, 4);

        for body in [
            br#"{"cmd":"WATCHED_CHANGE"}"#.as_slice(),
            br#"{"cmd":"UNDERLINE_TEST","value":1}"#.as_slice(),
        ] {
            let frame: Arc<[u8]> = Arc::from(
                Packet::new(
                    ProtocolVersion::Plain,
                    Operation::Notification,
                    body.to_vec(),
                )
                .to_bytes()
                .into_boxed_slice(),
            );
            task.process_frame(frame, 1)
                .await
                .expect("process notification");
        }

        for expected in [
            br#"{"cmd":"WATCHED_CHANGE"}"#.as_slice(),
            br#"{"cmd":"UNDERLINE_TEST","value":1}"#.as_slice(),
        ] {
            match raw_rx.recv().await {
                Some(RawCapture::Notification { bytes }) => assert_eq!(&*bytes, expected),
                other => panic!("unexpected raw capture: {other:?}"),
            }
        }
        assert!(raw_rx.try_recv().is_err());
        for expected in ["WATCHED_CHANGE", "UNDERLINE_TEST"] {
            assert!(matches!(
                event_rx.recv().await,
                Some(Ok(Event::Raw { cmd, .. })) if cmd == expected
            ));
        }
        assert!(event_rx.try_recv().is_err());
    }
}
