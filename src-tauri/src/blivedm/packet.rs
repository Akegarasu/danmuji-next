//! 数据包编解码
//!
//! Bilibili 直播弹幕协议包格式：
//! ```text
//! +------------+------------+---------------+-------------+------------+----------+
//! | 4 bytes    | 2 bytes    | 2 bytes       | 4 bytes     | 4 bytes    | N bytes  |
//! | 包总长度    | 头长度(16)  | 协议版本       | 操作码       | 序列ID(1)  | Body     |
//! +------------+------------+---------------+-------------+------------+----------+
//! ```

use std::io::{Cursor, Read};

use brotli::Decompressor;
use flate2::read::ZlibDecoder;

use crate::blivedm::error::{Error, Result};

/// 头部长度固定为 16 字节
pub const HEADER_LENGTH: usize = 16;

/// 协议版本
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ProtocolVersion {
    /// 纯文本 JSON
    Plain = 0,
    /// 人气值（心跳响应）
    Popularity = 1,
    /// Zlib 压缩
    Zlib = 2,
    /// Brotli 压缩
    Brotli = 3,
}

impl From<u16> for ProtocolVersion {
    fn from(v: u16) -> Self {
        match v {
            0 => Self::Plain,
            1 => Self::Popularity,
            2 => Self::Zlib,
            3 => Self::Brotli,
            _ => Self::Plain,
        }
    }
}

/// 操作码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Operation {
    Handshake = 0,
    HandshakeReply = 1,
    Heartbeat = 2,
    HeartbeatReply = 3,
    Notification = 5,
    EnterRoom = 7,
    EnterRoomReply = 8,
    Unknown(u32),
}

impl From<u32> for Operation {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Handshake,
            1 => Self::HandshakeReply,
            2 => Self::Heartbeat,
            3 => Self::HeartbeatReply,
            5 => Self::Notification,
            7 => Self::EnterRoom,
            8 => Self::EnterRoomReply,
            other => Self::Unknown(other),
        }
    }
}

impl From<Operation> for u32 {
    fn from(op: Operation) -> Self {
        match op {
            Operation::Handshake => 0,
            Operation::HandshakeReply => 1,
            Operation::Heartbeat => 2,
            Operation::HeartbeatReply => 3,
            Operation::Notification => 5,
            Operation::EnterRoom => 7,
            Operation::EnterRoomReply => 8,
            Operation::Unknown(v) => v,
        }
    }
}

/// 数据包
#[derive(Debug, Clone)]
pub struct Packet {
    pub protocol_version: ProtocolVersion,
    pub operation: Operation,
    pub body: Vec<u8>,
}

impl Packet {
    /// 创建新数据包
    pub fn new(protocol_version: ProtocolVersion, operation: Operation, body: Vec<u8>) -> Self {
        Self {
            protocol_version,
            operation,
            body,
        }
    }

    /// 创建心跳包
    pub fn heartbeat() -> Self {
        Self::new(ProtocolVersion::Popularity, Operation::Heartbeat, vec![])
    }

    /// 创建进入房间包
    pub fn enter_room(uid: u64, buvid: &str, room_id: u64, token: &str) -> Self {
        let payload = serde_json::json!({
            "uid": uid,
            "buvid": buvid,
            "roomid": room_id,
            "protover": 3,
            "platform": "danmuji",
            "type": 2,
            "key": token,
        });
        let body = serde_json::to_vec(&payload).unwrap_or_default();
        Self::new(ProtocolVersion::Popularity, Operation::EnterRoom, body)
    }

    /// 从字节解析数据包
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_LENGTH {
            return Err(Error::PacketParse("data too short".to_string()));
        }

        let packet_length = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if packet_length != data.len() {
            return Err(Error::PacketParse(format!(
                "packet length mismatch: expected {}, got {}",
                packet_length,
                data.len()
            )));
        }

        let protocol_version = u16::from_be_bytes([data[6], data[7]]);
        let operation = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let body = data[HEADER_LENGTH..].to_vec();

        Ok(Self {
            protocol_version: protocol_version.into(),
            operation: operation.into(),
            body,
        })
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let body_len = self.body.len();
        let packet_len = HEADER_LENGTH + body_len;

        let mut buf = Vec::with_capacity(packet_len);

        // 包长度 (4 bytes)
        buf.extend_from_slice(&(packet_len as u32).to_be_bytes());
        // 头长度 (2 bytes)
        buf.extend_from_slice(&(HEADER_LENGTH as u16).to_be_bytes());
        // 协议版本 (2 bytes)
        buf.extend_from_slice(&(self.protocol_version as u16).to_be_bytes());
        // 操作码 (4 bytes)
        buf.extend_from_slice(&u32::from(self.operation).to_be_bytes());
        // 序列ID (4 bytes) - 固定为 1
        buf.extend_from_slice(&1u32.to_be_bytes());
        // Body
        buf.extend_from_slice(&self.body);

        buf
    }

    /// 解析数据包（处理压缩）
    /// 返回解压后的多个数据包
    pub fn parse(self) -> Result<Vec<Packet>> {
        match self.protocol_version {
            ProtocolVersion::Plain | ProtocolVersion::Popularity => Ok(vec![self]),
            ProtocolVersion::Zlib => {
                let decompressed = decompress_zlib(&self.body)?;
                slice_packets(&decompressed)
            }
            ProtocolVersion::Brotli => {
                let decompressed = decompress_brotli(&self.body)?;
                slice_packets(&decompressed)
            }
        }
    }

    /// 获取 body 作为字符串
    pub fn body_as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.body).ok()
    }
}

/// Zlib 解压
fn decompress_zlib(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

/// Brotli 解压
fn decompress_brotli(data: &[u8]) -> Result<Vec<u8>> {
    let mut decompressor = Decompressor::new(Cursor::new(data), 4096);
    let mut decompressed = Vec::new();
    decompressor.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

/// 切分多个数据包
fn slice_packets(data: &[u8]) -> Result<Vec<Packet>> {
    let mut packets = Vec::new();
    let mut cursor = 0;
    let total = data.len();

    while cursor < total {
        if cursor + 4 > total {
            break;
        }

        let packet_len = u32::from_be_bytes([
            data[cursor],
            data[cursor + 1],
            data[cursor + 2],
            data[cursor + 3],
        ]) as usize;

        if packet_len == 0 || cursor + packet_len > total {
            break;
        }

        let packet = Packet::from_bytes(&data[cursor..cursor + packet_len])?;
        packets.push(packet);
        cursor += packet_len;
    }

    Ok(packets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_packet() {
        let packet = Packet::heartbeat();
        let bytes = packet.to_bytes();

        assert_eq!(bytes.len(), HEADER_LENGTH);
        assert_eq!(&bytes[0..4], &[0, 0, 0, 16]); // packet length
        assert_eq!(&bytes[4..6], &[0, 16]); // header length
    }

    #[test]
    fn test_packet_roundtrip() {
        let original = Packet::new(
            ProtocolVersion::Plain,
            Operation::Notification,
            b"hello".to_vec(),
        );

        let bytes = original.to_bytes();
        let parsed = Packet::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.protocol_version, ProtocolVersion::Plain);
        assert_eq!(parsed.operation, Operation::Notification);
        assert_eq!(parsed.body, b"hello");
    }
}
