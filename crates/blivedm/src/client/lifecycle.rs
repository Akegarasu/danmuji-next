use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use super::capture::DecodeStage;

/// 可克隆的协作式取消令牌。
///
/// 令牌是一次触发的：任意克隆调用 [`Self::cancel`] 后，当前及后续
/// [`Self::cancelled`] 等待者都会立即完成。
#[derive(Debug, Clone)]
pub struct CancellationToken {
    sender: Arc<watch::Sender<bool>>,
}

impl CancellationToken {
    /// 创建尚未取消的令牌。
    pub fn new() -> Self {
        let (sender, _receiver) = watch::channel(false);
        Self {
            sender: Arc::new(sender),
        }
    }

    /// 触发取消。重复调用是幂等的。
    pub fn cancel(&self) {
        if !self.is_cancelled() {
            self.sender.send_replace(true);
        }
    }

    /// 是否已经触发取消。
    pub fn is_cancelled(&self) -> bool {
        *self.sender.borrow()
    }

    /// 等待取消。此等待可安全地用于 `tokio::select!`。
    pub async fn cancelled(&self) {
        let mut receiver = self.sender.subscribe();
        loop {
            if *receiver.borrow() {
                return;
            }
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// 首次确认连接可用的上行信号。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnlineTrigger {
    EnterRoomReply,
    HeartbeatReply,
    Notification,
}

/// 一次连接结束或准备重连的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DisconnectReason {
    ConnectTimeout,
    JoinTimeout,
    ReadIdleTimeout,
    RemoteClosed { code: Option<u16>, reason: String },
    Transport(String),
    Authentication(String),
    EventConsumerDropped,
    Cancelled,
}

/// 当前连接状态。`watch` 订阅者始终能读取最新状态。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionState {
    Starting,
    Connecting {
        attempt: u64,
        endpoint: String,
    },
    Joining {
        attempt: u64,
        endpoint: String,
    },
    Online {
        attempt: u64,
        endpoint: String,
    },
    Backoff {
        next_attempt: u64,
        delay: Duration,
        reason: DisconnectReason,
    },
    Closed {
        reason: Option<DisconnectReason>,
    },
    Cancelled,
}

/// 连接生命周期事件。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LifecycleEvent {
    Connecting {
        attempt: u64,
        endpoint: String,
    },
    WebSocketConnected {
        attempt: u64,
        endpoint: String,
    },
    EnterRoomSent {
        attempt: u64,
    },
    EnterRoomReply {
        attempt: u64,
        body: Arc<[u8]>,
    },
    Online {
        attempt: u64,
        trigger: OnlineTrigger,
    },
    HeartbeatSent {
        attempt: u64,
    },
    HeartbeatReply {
        attempt: u64,
        popularity: Option<u32>,
    },
    DecodeError {
        attempt: u64,
        stage: DecodeStage,
        error: String,
    },
    Disconnected {
        attempt: u64,
        reason: DisconnectReason,
    },
    ReconnectScheduled {
        next_attempt: u64,
        delay: Duration,
        reason: DisconnectReason,
    },
    Cancelled,
    Closed {
        reason: Option<DisconnectReason>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_is_visible_to_existing_and_future_waiters() {
        let cancellation = CancellationToken::new();
        let existing_waiter = cancellation.clone();
        let task = tokio::spawn(async move { existing_waiter.cancelled().await });

        cancellation.cancel();
        task.await.unwrap();
        cancellation.cancelled().await;
        assert!(cancellation.is_cancelled());
    }
}
