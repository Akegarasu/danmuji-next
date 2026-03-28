//! 错误类型定义

use thiserror::Error;

/// blivedm 库的错误类型
#[derive(Debug, Error)]
pub enum Error {
    /// WebSocket 连接错误
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    /// HTTP 请求错误
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON 解析错误
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// IO 错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 数据包解析错误
    #[error("Packet parse error: {0}")]
    PacketParse(String),

    /// API 错误
    #[error("API error: code={code}, message={message}")]
    Api { code: i32, message: String },

    /// 连接已关闭
    #[error("Connection closed")]
    ConnectionClosed,

    /// 认证失败
    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    /// 无效的房间号
    #[error("Invalid room id: {0}")]
    InvalidRoomId(u64),

    /// 配置错误
    #[error("Configuration error: {0}")]
    Config(String),
}

/// Result 类型别名
pub type Result<T> = std::result::Result<T, Error>;
