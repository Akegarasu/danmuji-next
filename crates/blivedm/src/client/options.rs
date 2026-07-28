use std::time::Duration;

use crate::error::{Error, Result};

const DEFAULT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_JOIN_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_READ_IDLE_TIMEOUT: Duration = Duration::from_secs(75);

/// 单次 WebSocket 连接的运行参数。
#[derive(Debug, Clone)]
pub struct ConnectionOptions {
    pub(super) connect_timeout: Duration,
    pub(super) join_timeout: Duration,
    pub(super) read_idle_timeout: Option<Duration>,
    pub(super) heartbeat_interval: Duration,
    pub(super) event_buffer_capacity: usize,
    pub(super) lifecycle_buffer_capacity: usize,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            join_timeout: DEFAULT_JOIN_TIMEOUT,
            read_idle_timeout: Some(DEFAULT_READ_IDLE_TIMEOUT),
            heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
            event_buffer_capacity: 256,
            lifecycle_buffer_capacity: 128,
        }
    }
}

impl ConnectionOptions {
    /// 设置 TCP/TLS/WebSocket 握手超时。
    pub fn with_connect_timeout(mut self, value: Duration) -> Self {
        self.connect_timeout = value;
        self
    }

    /// 设置进入房间后等待首个有效上行响应的超时。
    pub fn with_join_timeout(mut self, value: Duration) -> Self {
        self.join_timeout = value;
        self
    }

    /// 设置在线后的读空闲超时。传入 `None` 可禁用半开连接检测。
    pub fn with_read_idle_timeout(mut self, value: Option<Duration>) -> Self {
        self.read_idle_timeout = value;
        self
    }

    /// 设置心跳发送间隔。
    pub fn with_heartbeat_interval(mut self, value: Duration) -> Self {
        self.heartbeat_interval = value;
        self
    }

    /// 设置业务事件通道容量。
    pub fn with_event_buffer_capacity(mut self, value: usize) -> Self {
        self.event_buffer_capacity = value;
        self
    }

    /// 设置生命周期广播通道容量。
    pub fn with_lifecycle_buffer_capacity(mut self, value: usize) -> Self {
        self.lifecycle_buffer_capacity = value;
        self
    }

    pub(super) fn validate(&self) -> Result<()> {
        validate_non_zero(self.connect_timeout, "connect_timeout")?;
        validate_non_zero(self.join_timeout, "join_timeout")?;
        if let Some(read_idle_timeout) = self.read_idle_timeout {
            validate_non_zero(read_idle_timeout, "read_idle_timeout")?;
        }
        validate_non_zero(self.heartbeat_interval, "heartbeat_interval")?;
        validate_capacity(self.event_buffer_capacity, "event_buffer_capacity")?;
        validate_capacity(self.lifecycle_buffer_capacity, "lifecycle_buffer_capacity")?;
        Ok(())
    }
}

fn validate_non_zero(value: Duration, name: &str) -> Result<()> {
    if value.is_zero() {
        return Err(Error::Config(format!("{name} must be greater than zero")));
    }
    Ok(())
}

fn validate_capacity(value: usize, name: &str) -> Result<()> {
    if value == 0 {
        return Err(Error::Config(format!("{name} must be greater than zero")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_durations_and_capacities() {
        assert!(ConnectionOptions::default()
            .with_connect_timeout(Duration::ZERO)
            .validate()
            .is_err());
        assert!(ConnectionOptions::default()
            .with_join_timeout(Duration::ZERO)
            .validate()
            .is_err());
        assert!(ConnectionOptions::default()
            .with_read_idle_timeout(Some(Duration::ZERO))
            .validate()
            .is_err());
        assert!(ConnectionOptions::default()
            .with_heartbeat_interval(Duration::ZERO)
            .validate()
            .is_err());
        assert!(ConnectionOptions::default()
            .with_event_buffer_capacity(0)
            .validate()
            .is_err());
        assert!(ConnectionOptions::default()
            .with_lifecycle_buffer_capacity(0)
            .validate()
            .is_err());
    }
}
