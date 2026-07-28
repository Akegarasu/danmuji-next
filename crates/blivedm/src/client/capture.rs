use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc;

/// 原始 JSON 事件回调。
///
/// 此 API 为兼容既有调用而保留，回调仍在解析任务中同步执行。回调必须快速返回，
/// 不应直接执行文件或数据库 I/O。新代码应优先使用
/// [`crate::BliveDmClientBuilder::raw_capture`] 的有界非阻塞旁路。
pub type RawEventHandler = dyn Fn(&Value) + Send + Sync + 'static;

/// 原始数据捕获范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RawCaptureMode {
    /// 不捕获。
    Disabled,
    /// 仅捕获无法解码的数据。
    ErrorsOnly,
    /// 捕获成功解析为 [`crate::Event`] 的 notification body，以及解码错误。
    ///
    /// 原始旁路和业务事件使用独立的有界通道，因此这是一种可监控丢弃量的
    /// best-effort 关联，而不是事务性的一一投递。
    EventNotifications,
    /// 捕获解压后的 notification body 和解码错误。
    Notifications,
    /// 捕获全部二进制 WebSocket frame、notification body 和解码错误。
    AllFrames,
}

/// 数据解码失败的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeStage {
    /// 外层协议包头或长度无效。
    PacketEnvelope,
    /// 压缩数据或内层包切分失败。
    PacketPayload,
    /// notification 不是有效 JSON 或无法解析为对应事件。
    Notification,
}

/// 从 WebSocket 读取路径旁路出的原始数据。
///
/// 数据通过调用方提供的有界 `mpsc` 通道以 `try_send` 投递；通道满时会丢弃旁路数据，
/// 但绝不会阻塞心跳或 WebSocket 读取。规范化事件仍通过 [`crate::EventStream`] 传递。
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RawCapture {
    /// 完整二进制 WebSocket frame。
    WebSocketFrame { bytes: Arc<[u8]> },
    /// 解压、切包后的原始 notification body。
    Notification { bytes: Arc<[u8]> },
    /// 解码失败及发生失败的最小可用原始数据。
    DecodeError {
        stage: DecodeStage,
        bytes: Arc<[u8]>,
        error: String,
    },
}

#[derive(Clone)]
pub(super) struct RawCaptureTarget {
    mode: RawCaptureMode,
    sender: mpsc::Sender<RawCapture>,
    dropped: Arc<AtomicU64>,
}

impl RawCaptureTarget {
    pub(super) fn new(sender: mpsc::Sender<RawCapture>, mode: RawCaptureMode) -> Self {
        Self {
            mode,
            sender,
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(super) fn dropped_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.dropped)
    }

    fn emit(&self, capture: RawCapture) {
        if self.sender.try_send(capture).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn capture_frame(&self, bytes: Arc<[u8]>) {
        if self.mode == RawCaptureMode::AllFrames {
            self.emit(RawCapture::WebSocketFrame { bytes });
        }
    }

    pub(super) fn capture_notification(&self, bytes: Arc<[u8]>) {
        if matches!(
            self.mode,
            RawCaptureMode::Notifications | RawCaptureMode::AllFrames
        ) {
            self.emit(RawCapture::Notification { bytes });
        }
    }

    pub(super) fn capture_event_notification(&self, bytes: Arc<[u8]>) {
        if self.mode == RawCaptureMode::EventNotifications {
            self.emit(RawCapture::Notification { bytes });
        }
    }

    pub(super) fn capture_error(&self, stage: DecodeStage, bytes: Arc<[u8]>, error: String) {
        if self.mode != RawCaptureMode::Disabled {
            self.emit(RawCapture::DecodeError {
                stage,
                bytes,
                error,
            });
        }
    }
}
