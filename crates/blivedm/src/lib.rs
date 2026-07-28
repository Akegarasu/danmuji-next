//! Bilibili 直播弹幕协议库
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use blivedm::{BliveDmClient, Event};
//! use futures_util::StreamExt;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = BliveDmClient::builder()
//!     .room_id(12345)
//!     .build()
//!     .await?;
//!
//! let mut stream = client.connect().await?;
//!
//! while let Some(event) = stream.next().await {
//!     match event {
//!         Ok(Event::Danmaku(dm)) => println!("{}: {}", dm.sender.name, dm.content),
//!         Ok(Event::Gift(gift)) => println!("礼物: {}", gift.gift_name),
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```

pub mod api;
mod client;
mod error;
mod message;
pub mod packet;

pub use api::RoomInfo;
pub use client::{
    BliveDmClient, BliveDmClientBuilder, CancellationToken, ConnectionOptions, ConnectionState,
    DecodeStage, DisconnectReason, EventStream, LifecycleEvent, OnlineTrigger, RawCapture,
    RawCaptureMode, RawEventHandler,
};
pub use error::{Error, Result};
pub use message::*;
