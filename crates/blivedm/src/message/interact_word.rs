//! 进入直播间消息

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{parse_bool_flag, GuardLevel, Medal, User};

/// 进入直播间消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractWord {
    /// 用户信息
    pub user: User,
    /// 时间戳（秒或毫秒）
    pub timestamp: i64,
    /// 消息类型 (1=进入直播间, 2=关注, 3=分享, 4=特别关注, 5=互相关注)
    pub msg_type: u32,
}

impl InteractWord {
    /// 从 JSON 解析
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.get("data")?;

        let uid = data.get("uid")?.as_u64()?;
        let uname = data.get("uname")?.as_str()?.to_string();
        let timestamp = data.get("timestamp")?.as_i64()?;
        let msg_type = data.get("msg_type")?.as_u64().unwrap_or(1) as u32;

        // 头像
        let face = data
            .get("uinfo")
            .and_then(|u| u.get("base"))
            .and_then(|b| b.get("face"))
            .and_then(|f| f.as_str())
            .map(String::from);

        // 舰队等级
        let guard_level = data
            .get("uinfo")
            .and_then(|u| u.get("guard"))
            .and_then(|g| g.get("level"))
            .and_then(|l| l.as_i64())
            .map(GuardLevel::from)
            .unwrap_or(GuardLevel::None);

        // 粉丝勋章
        let medal = data.get("fans_medal").and_then(|fm| {
            let level = fm.get("medal_level")?.as_u64()? as u32;
            if level == 0 {
                return None;
            }
            Some(Medal {
                level,
                name: fm.get("medal_name")?.as_str()?.to_string(),
                color: fm.get("medal_color")?.as_u64()? as u32,
                room_id: fm.get("anchor_roomid")?.as_u64()?,
                anchor_uid: fm.get("target_id").and_then(Value::as_u64).unwrap_or(0),
                anchor_name: String::new(),
                is_light: parse_bool_flag(fm.get("is_lighted")).unwrap_or(true),
            })
        });

        Some(InteractWord {
            user: User {
                uid,
                name: uname,
                face,
                medal,
                guard_level,
                user_level: 0,
                is_admin: false,
            },
            timestamp,
            msg_type,
        })
    }

    /// 从 INTERACT_WORD_V2 解析
    pub fn parse_v2(value: &Value) -> Option<Self> {
        let pb = value.get("data")?.get("pb")?.as_str()?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(pb).ok()?;
        let msg = parse_interact_word_v2(&bytes).ok()?;

        let base = msg.uinfo.as_ref().and_then(|u| u.base.as_ref());
        let wealth = msg.uinfo.as_ref().and_then(|u| u.wealth.as_ref());
        let user_medal = msg.uinfo.as_ref().and_then(|u| u.medal.as_ref());
        let timestamp = normalized_interact_word_v2_timestamp(&msg);

        let uid = if msg.uid != 0 {
            msg.uid
        } else {
            msg.uinfo.as_ref().map(|u| u.uid).unwrap_or_default()
        };

        let name = if !msg.username.is_empty() {
            msg.username.clone()
        } else {
            base.map(|b| b.username.clone()).unwrap_or_default()
        };

        let face = base.and_then(|b| {
            if b.avatar.is_empty() {
                None
            } else {
                Some(b.avatar.clone())
            }
        });

        let medal = msg
            .medal
            .as_ref()
            .filter(|m| m.level > 0)
            .map(|m| Medal {
                level: m.level,
                name: if !m.name.is_empty() {
                    m.name.clone()
                } else {
                    user_medal.map(|m| m.name.clone()).unwrap_or_default()
                },
                color: user_medal.map(|m| m.color).unwrap_or_default(),
                room_id: if m.room_id != 0 {
                    m.room_id as u64
                } else {
                    msg.room_id as u64
                },
                anchor_uid: m.ruid,
                anchor_name: String::new(),
                is_light: m.is_lighted.unwrap_or(true),
            })
            .or_else(|| {
                user_medal.filter(|m| m.level > 0).map(|m| Medal {
                    level: m.level,
                    name: m.name.clone(),
                    color: m.color,
                    room_id: msg.room_id as u64,
                    anchor_uid: m.ruid,
                    anchor_name: String::new(),
                    is_light: true,
                })
            });

        let msg_type = if msg.msg_type == 0 { 1 } else { msg.msg_type };

        Some(InteractWord {
            user: User {
                uid,
                name,
                face,
                medal,
                guard_level: GuardLevel::from(msg.guard_type as i64),
                user_level: wealth.map(|w| w.level).unwrap_or_default(),
                is_admin: false,
            },
            timestamp,
            msg_type,
        })
    }
}

#[derive(Debug, Default)]
struct InteractWordV2 {
    uid: u64,
    username: String,
    msg_type: u32,
    room_id: u32,
    timestamp: u32,
    timestamp_ms: u64,
    medal: Option<TopMedal>,
    trigger_time: u64,
    guard_type: u32,
    uinfo: Option<UInfo>,
}

#[derive(Debug, Default)]
struct TopMedal {
    ruid: u64,
    level: u32,
    name: String,
    is_lighted: Option<bool>,
    guard_level: u32,
    room_id: u32,
}

#[derive(Debug, Default)]
struct UInfo {
    uid: u64,
    base: Option<BaseInfo>,
    medal: Option<UserMedal>,
    wealth: Option<Wealth>,
}

#[derive(Debug, Default)]
struct BaseInfo {
    username: String,
    avatar: String,
}

#[derive(Debug, Default)]
struct UserMedal {
    name: String,
    level: u32,
    color: u32,
    ruid: u64,
}

#[derive(Debug, Default)]
struct Wealth {
    level: u32,
}

#[derive(Debug)]
enum PbError {
    Eof,
    BadVarint,
    BadUtf8,
    BadLength,
    UnsupportedWireType,
}

type PbResult<T> = Result<T, PbError>;

struct Pb<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Pb<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn eof(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn varint(&mut self) -> PbResult<u64> {
        let mut out = 0u64;
        for shift in (0..70).step_by(7) {
            let b = *self.buf.get(self.pos).ok_or(PbError::Eof)?;
            self.pos += 1;
            out |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok(out);
            }
        }
        Err(PbError::BadVarint)
    }

    fn bytes(&mut self) -> PbResult<&'a [u8]> {
        let len = self.varint()? as usize;
        let end = self.pos.checked_add(len).ok_or(PbError::BadLength)?;
        if end > self.buf.len() {
            return Err(PbError::Eof);
        }
        let bytes = &self.buf[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn string(&mut self) -> PbResult<String> {
        std::str::from_utf8(self.bytes()?)
            .map(|s| s.to_owned())
            .map_err(|_| PbError::BadUtf8)
    }

    fn fixed32(&mut self) -> PbResult<()> {
        self.skip_n(4)
    }

    fn fixed64(&mut self) -> PbResult<()> {
        self.skip_n(8)
    }

    fn skip_n(&mut self, n: usize) -> PbResult<()> {
        let end = self.pos.checked_add(n).ok_or(PbError::BadLength)?;
        if end > self.buf.len() {
            return Err(PbError::Eof);
        }
        self.pos = end;
        Ok(())
    }

    fn skip(&mut self, wire: u32) -> PbResult<()> {
        match wire {
            0 => self.varint().map(|_| ()),
            1 => self.fixed64(),
            2 => self.bytes().map(|_| ()),
            5 => self.fixed32(),
            _ => Err(PbError::UnsupportedWireType),
        }
    }
}

fn parse_interact_word_v2(buf: &[u8]) -> PbResult<InteractWordV2> {
    let mut p = Pb::new(buf);
    let mut out = InteractWordV2::default();

    while !p.eof() {
        let tag = p.varint()? as u32;
        let field = tag >> 3;
        let wire = tag & 7;

        match (field, wire) {
            (1, 0) => out.uid = p.varint()?,
            (2, 2) => out.username = p.string()?,
            (5, 0) => out.msg_type = p.varint()? as u32,
            (6, 0) => out.room_id = p.varint()? as u32,
            (7, 0) => out.timestamp = p.varint()? as u32,
            (8, 0) => out.timestamp_ms = p.varint()?,
            (9, 2) => out.medal = Some(parse_top_medal(p.bytes()?)?),
            (15, 0) => out.trigger_time = p.varint()?,
            (16, 0) => out.guard_type = p.varint()? as u32,
            (22, 2) => out.uinfo = Some(parse_uinfo(p.bytes()?)?),
            _ => p.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_top_medal(buf: &[u8]) -> PbResult<TopMedal> {
    let mut p = Pb::new(buf);
    let mut out = TopMedal::default();

    while !p.eof() {
        let tag = p.varint()? as u32;
        let field = tag >> 3;
        let wire = tag & 7;

        match (field, wire) {
            (1, 0) => out.ruid = p.varint()?,
            (2, 0) => out.level = p.varint()? as u32,
            (3, 2) => out.name = p.string()?,
            (8, 0) => out.is_lighted = Some(p.varint()? != 0),
            (9, 0) => out.guard_level = p.varint()? as u32,
            (12, 0) => out.room_id = p.varint()? as u32,
            _ => p.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_uinfo(buf: &[u8]) -> PbResult<UInfo> {
    let mut p = Pb::new(buf);
    let mut out = UInfo::default();

    while !p.eof() {
        let tag = p.varint()? as u32;
        let field = tag >> 3;
        let wire = tag & 7;

        match (field, wire) {
            (1, 0) => out.uid = p.varint()?,
            (2, 2) => out.base = Some(parse_base_info(p.bytes()?)?),
            (3, 2) => out.medal = Some(parse_user_medal(p.bytes()?)?),
            (4, 2) => out.wealth = Some(parse_wealth(p.bytes()?)?),
            _ => p.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_base_info(buf: &[u8]) -> PbResult<BaseInfo> {
    let mut p = Pb::new(buf);
    let mut out = BaseInfo::default();

    while !p.eof() {
        let tag = p.varint()? as u32;
        let field = tag >> 3;
        let wire = tag & 7;

        match (field, wire) {
            (1, 2) => out.username = p.string()?,
            (2, 2) => out.avatar = p.string()?,
            _ => p.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_user_medal(buf: &[u8]) -> PbResult<UserMedal> {
    let mut p = Pb::new(buf);
    let mut out = UserMedal::default();

    while !p.eof() {
        let tag = p.varint()? as u32;
        let field = tag >> 3;
        let wire = tag & 7;

        match (field, wire) {
            (1, 2) => out.name = p.string()?,
            (2, 0) => out.level = p.varint()? as u32,
            (6, 0) => out.color = p.varint()? as u32,
            (10, 0) => out.ruid = p.varint()?,
            (18, 0) => out.color = p.varint()? as u32,
            _ => p.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_wealth(buf: &[u8]) -> PbResult<Wealth> {
    let mut p = Pb::new(buf);
    let mut out = Wealth::default();

    while !p.eof() {
        let tag = p.varint()? as u32;
        let field = tag >> 3;
        let wire = tag & 7;

        match (field, wire) {
            (1, 0) => out.level = p.varint()? as u32,
            _ => p.skip(wire)?,
        }
    }

    Ok(out)
}

fn normalized_interact_word_v2_timestamp(msg: &InteractWordV2) -> i64 {
    if msg.trigger_time != 0 {
        ((msg.trigger_time + 500_000) / 1_000_000) as i64
    } else if msg.timestamp_ms != 0 {
        msg.timestamp_ms as i64
    } else {
        msg.timestamp as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_varint(out: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            out.push((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    fn put_tag(out: &mut Vec<u8>, field: u32, wire: u32) {
        put_varint(out, ((field << 3) | wire) as u64);
    }

    fn put_u64(out: &mut Vec<u8>, field: u32, value: u64) {
        put_tag(out, field, 0);
        put_varint(out, value);
    }

    fn put_string(out: &mut Vec<u8>, field: u32, value: &str) {
        put_tag(out, field, 2);
        put_varint(out, value.len() as u64);
        out.extend_from_slice(value.as_bytes());
    }

    fn put_message(out: &mut Vec<u8>, field: u32, value: Vec<u8>) {
        put_tag(out, field, 2);
        put_varint(out, value.len() as u64);
        out.extend_from_slice(&value);
    }

    #[test]
    fn parses_interact_word_v2_pb_payload() {
        let mut base = Vec::new();
        put_string(&mut base, 1, "fallback-name");
        put_string(&mut base, 2, "https://example.com/avatar.jpg");

        let mut user_medal = Vec::new();
        put_string(&mut user_medal, 1, "备用牌");
        put_u64(&mut user_medal, 2, 11);
        put_u64(&mut user_medal, 6, 0x12_34_56);
        put_u64(&mut user_medal, 10, 9988);

        let mut wealth = Vec::new();
        put_u64(&mut wealth, 1, 7);

        let mut uinfo = Vec::new();
        put_u64(&mut uinfo, 1, 12345);
        put_message(&mut uinfo, 2, base);
        put_message(&mut uinfo, 3, user_medal);
        put_message(&mut uinfo, 4, wealth);

        let mut top_medal = Vec::new();
        put_u64(&mut top_medal, 1, 9988);
        put_u64(&mut top_medal, 2, 22);
        put_string(&mut top_medal, 3, "粉丝牌");
        put_u64(&mut top_medal, 12, 456);

        let mut msg = Vec::new();
        put_u64(&mut msg, 1, 12345);
        put_u64(&mut msg, 5, 0);
        put_u64(&mut msg, 6, 456);
        put_message(&mut msg, 9, top_medal);
        put_u64(&mut msg, 15, 1_700_000_000_123_456_789);
        put_u64(&mut msg, 16, 3);
        put_message(&mut msg, 22, uinfo);

        let pb = base64::engine::general_purpose::STANDARD.encode(msg);
        let value = serde_json::json!({
            "cmd": "INTERACT_WORD_V2",
            "data": { "pb": pb }
        });

        let parsed = InteractWord::parse_v2(&value).expect("parse v2");
        assert_eq!(parsed.user.uid, 12345);
        assert_eq!(parsed.user.name, "fallback-name");
        assert_eq!(
            parsed.user.face.as_deref(),
            Some("https://example.com/avatar.jpg")
        );
        assert_eq!(parsed.user.guard_level, GuardLevel::Captain);
        assert_eq!(parsed.user.user_level, 7);
        assert_eq!(parsed.msg_type, 1);
        assert_eq!(parsed.timestamp, 1_700_000_000_123);

        let medal = parsed.user.medal.expect("medal");
        assert_eq!(medal.name, "粉丝牌");
        assert_eq!(medal.level, 22);
        assert_eq!(medal.color, 0x12_34_56);
        assert_eq!(medal.room_id, 456);
        assert_eq!(medal.anchor_uid, 9988);
    }
}
