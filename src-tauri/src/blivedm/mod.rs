//! Bilibili 直播弹幕协议库
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use blivedm::{BliveDmClient, Event};
//! use futures_util::StreamExt;
//!
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
//! ```

pub mod api;
mod client;
mod error;
mod message;
mod packet;
mod wbi;

pub use api::RoomInfo;
pub use client::{BliveDmClient, BliveDmClientBuilder, EventStream};
pub use error::{Error, Result};
pub use message::*;
