//! 直播间高能用户排行榜 V3 消息

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GuardLevel, OnlineRankUser};

/// 直播间高能用户排行榜 V3。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnlineRankV3 {
    pub rank_type: String,
    pub online_list: Vec<OnlineRankV3User>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnlineRankV3User {
    pub uid: u64,
    pub face: String,
    pub score: String,
    pub uname: String,
    pub rank: u32,
    pub guard_level: Option<u32>,
    pub is_mystery: Option<bool>,
    pub uinfo: Option<OnlineRankV3UInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnlineRankV3UInfo {
    pub uid: u64,
    pub base: Option<OnlineRankV3Base>,
    pub guard: Option<OnlineRankV3Guard>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnlineRankV3Base {
    pub name: String,
    pub face: String,
    pub name_color: u32,
    pub is_mystery: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnlineRankV3Guard {
    pub level: u32,
    pub expired_str: String,
}

impl OnlineRankV3 {
    /// 从 `ONLINE_RANK_V3.data.pb` 解析榜单。
    pub fn parse(value: &Value) -> Option<Self> {
        let pb = value.get("data")?.get("pb")?.as_str()?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(pb).ok()?;
        parse_online_rank_v3(&bytes).ok()
    }

    /// 归一化为 V2 已使用的公共榜单用户结构。
    pub fn into_online_users(self) -> Vec<OnlineRankUser> {
        self.online_list
            .into_iter()
            .map(OnlineRankV3User::into_online_user)
            .collect()
    }
}

impl OnlineRankV3User {
    fn into_online_user(self) -> OnlineRankUser {
        let uinfo_uid = self.uinfo.as_ref().map(|uinfo| uinfo.uid).unwrap_or_default();
        let base = self.uinfo.as_ref().and_then(|uinfo| uinfo.base.as_ref());
        let nested_guard_level = self
            .uinfo
            .as_ref()
            .and_then(|uinfo| uinfo.guard.as_ref())
            .map(|guard| guard.level);

        let uid = if self.uid != 0 { self.uid } else { uinfo_uid };
        let name = if self.uname.is_empty() {
            base.map(|base| base.name.clone()).unwrap_or_default()
        } else {
            self.uname
        };
        let face = if self.face.is_empty() {
            base.and_then(|base| non_empty_string(&base.face))
        } else {
            Some(self.face)
        };
        let guard_level = self.guard_level.or(nested_guard_level).unwrap_or_default();

        OnlineRankUser {
            uid,
            name,
            face,
            rank: self.rank,
            score: self.score,
            guard_level: GuardLevel::from(i64::from(guard_level)),
        }
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

#[derive(Debug)]
enum PbError {
    Eof,
    BadVarint,
    BadUtf8,
    BadLength,
    IntegerOverflow,
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
        for index in 0..10 {
            let byte = *self.buf.get(self.pos).ok_or(PbError::Eof)?;
            self.pos += 1;
            if index == 9 && byte > 1 {
                return Err(PbError::BadVarint);
            }
            out |= u64::from(byte & 0x7f) << (index * 7);
            if byte & 0x80 == 0 {
                return Ok(out);
            }
        }
        Err(PbError::BadVarint)
    }

    fn u32(&mut self) -> PbResult<u32> {
        u32::try_from(self.varint()?).map_err(|_| PbError::IntegerOverflow)
    }

    fn bytes(&mut self) -> PbResult<&'a [u8]> {
        let len = usize::try_from(self.varint()?).map_err(|_| PbError::BadLength)?;
        let end = self.pos.checked_add(len).ok_or(PbError::BadLength)?;
        if end > self.buf.len() {
            return Err(PbError::Eof);
        }
        let value = &self.buf[self.pos..end];
        self.pos = end;
        Ok(value)
    }

    fn string(&mut self) -> PbResult<String> {
        std::str::from_utf8(self.bytes()?)
            .map(str::to_owned)
            .map_err(|_| PbError::BadUtf8)
    }

    fn skip(&mut self, wire: u32) -> PbResult<()> {
        match wire {
            0 => self.varint().map(|_| ()),
            1 => self.skip_n(8),
            2 => self.bytes().map(|_| ()),
            5 => self.skip_n(4),
            _ => Err(PbError::UnsupportedWireType),
        }
    }

    fn skip_n(&mut self, len: usize) -> PbResult<()> {
        let end = self.pos.checked_add(len).ok_or(PbError::BadLength)?;
        if end > self.buf.len() {
            return Err(PbError::Eof);
        }
        self.pos = end;
        Ok(())
    }
}

fn parse_online_rank_v3(buf: &[u8]) -> PbResult<OnlineRankV3> {
    let mut pb = Pb::new(buf);
    let mut out = OnlineRankV3::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 2) => out.rank_type = pb.string()?,
            (3, 2) => out.online_list.push(parse_online_rank_user(pb.bytes()?)?),
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_online_rank_user(buf: &[u8]) -> PbResult<OnlineRankV3User> {
    let mut pb = Pb::new(buf);
    let mut out = OnlineRankV3User::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 0) => out.uid = pb.varint()?,
            (2, 2) => out.face = pb.string()?,
            (3, 2) => out.score = pb.string()?,
            (4, 2) => out.uname = pb.string()?,
            (5, 0) => out.rank = pb.u32()?,
            (6, 0) => out.guard_level = Some(pb.u32()?),
            (7, 0) => out.is_mystery = Some(pb.varint()? != 0),
            (8, 2) => out.uinfo = Some(parse_online_rank_uinfo(pb.bytes()?)?),
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_online_rank_uinfo(buf: &[u8]) -> PbResult<OnlineRankV3UInfo> {
    let mut pb = Pb::new(buf);
    let mut out = OnlineRankV3UInfo::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 0) => out.uid = pb.varint()?,
            (2, 2) => out.base = Some(parse_online_rank_base(pb.bytes()?)?),
            (6, 2) => out.guard = Some(parse_online_rank_guard(pb.bytes()?)?),
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_online_rank_base(buf: &[u8]) -> PbResult<OnlineRankV3Base> {
    let mut pb = Pb::new(buf);
    let mut out = OnlineRankV3Base::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 2) => out.name = pb.string()?,
            (2, 2) => out.face = pb.string()?,
            (3, 0) => out.name_color = pb.u32()?,
            (4, 0) => out.is_mystery = pb.varint()? != 0,
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_online_rank_guard(buf: &[u8]) -> PbResult<OnlineRankV3Guard> {
    let mut pb = Pb::new(buf);
    let mut out = OnlineRankV3Guard::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 0) => out.level = pb.u32()?,
            (2, 2) => out.expired_str = pb.string()?,
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use serde_json::json;

    use super::OnlineRankV3;

    fn field_varint(field: u8, value: u64) -> Vec<u8> {
        let mut bytes = vec![field << 3];
        let mut value = value;
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if value == 0 {
                break;
            }
        }
        bytes
    }

    fn field_bytes(field: u8, value: &[u8]) -> Vec<u8> {
        let mut bytes = vec![(field << 3) | 2];
        bytes.extend(field_varint(0, value.len() as u64).into_iter().skip(1));
        bytes.extend(value);
        bytes
    }

    #[test]
    fn parses_and_normalizes_v3_rank_user() {
        let mut base = field_bytes(1, "嵌套昵称".as_bytes());
        base.extend(field_bytes(2, b"https://example.invalid/avatar.png"));
        base.extend(field_varint(4, 1));

        let mut guard = field_varint(1, 3);
        guard.extend(field_bytes(2, b"2099-01-01"));

        let mut uinfo = field_varint(1, 42);
        uinfo.extend(field_bytes(2, &base));
        uinfo.extend(field_bytes(6, &guard));

        let mut user = field_varint(1, 0);
        user.extend(field_bytes(3, b"12345"));
        user.extend(field_varint(5, 1));
        user.extend(field_varint(7, 1));
        user.extend(field_bytes(8, &uinfo));

        let mut message = field_bytes(1, b"online_rank");
        message.extend(field_bytes(3, &user));
        let encoded = base64::engine::general_purpose::STANDARD.encode(message);
        let event = json!({ "data": { "pb": encoded } });

        let rank = OnlineRankV3::parse(&event).expect("valid v3 rank");
        assert_eq!(rank.rank_type, "online_rank");
        assert_eq!(rank.online_list[0].is_mystery, Some(true));

        let users = rank.into_online_users();
        assert_eq!(users[0].uid, 42);
        assert_eq!(users[0].name, "嵌套昵称");
        assert_eq!(users[0].score, "12345");
        assert_eq!(users[0].rank, 1);
        assert_eq!(users[0].face.as_deref(), Some("https://example.invalid/avatar.png"));
    }
}
