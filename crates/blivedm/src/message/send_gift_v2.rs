//! `SEND_GIFT_V2` protobuf 礼物消息

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde_json::Value;

use super::{BlindGift, CoinType, Gift, GuardLevel, Medal};

impl Gift {
    /// 从 `SEND_GIFT_V2.data.pb` 解析并归一化礼物。
    ///
    /// 与 Bilibili 当前网页端一致，只消费 `gift_item[0]`。
    pub fn parse_v2(value: &Value) -> Option<Self> {
        let encoded = value.get("data")?.get("pb")?.as_str()?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        let message = parse_send_gift_v2(&bytes).ok()?;
        normalize_gift(message)
    }
}

#[derive(Debug, Default)]
struct SendGiftV2 {
    uid: u64,
    uname: String,
    face: String,
    guard_level: u64,
    medal_info: Option<MedalInfo>,
    blind_gift: Option<BlindGift>,
    sender_uinfo: Option<SenderUInfo>,
    gift_items: Vec<GiftItem>,
}

#[derive(Debug, Default)]
struct GiftItem {
    gift_id: u32,
    gift_name: String,
    num: u32,
    price: u64,
    total_coin: u64,
    coin_type: String,
    tid: String,
    timestamp: u64,
    super_batch_gift_num: u64,
    batch_combo_id: String,
    combo_resources_id: u64,
    combo_total_coin: u64,
    combo_stay_time: u64,
    show_batch_combo_send: Option<bool>,
    action: String,
    gift_info: Option<GiftInfo>,
    gift_tip_price: u64,
}

#[derive(Debug, Default)]
struct SenderUInfo {
    uid: u64,
    name: String,
    face: String,
}

#[derive(Debug, Default)]
struct MedalInfo {
    ruid: u64,
    level: u32,
    name: String,
    is_lighted: Option<bool>,
}

#[derive(Debug, Default)]
struct GiftInfo {
    img_basic: String,
    webp: String,
    gif: String,
}

fn normalize_gift(message: SendGiftV2) -> Option<Gift> {
    let item = message.gift_items.into_iter().next()?;
    let sender = message.sender_uinfo.unwrap_or_default();

    let sender_uid = if sender.uid != 0 {
        sender.uid
    } else {
        message.uid
    };
    let sender_name = if sender.name.is_empty() {
        message.uname
    } else {
        sender.name
    };
    let sender_face = non_empty(sender.face).or_else(|| non_empty(message.face));

    let coin_type = match item.coin_type.as_str() {
        "" | "gold" => CoinType::Gold,
        "silver" => CoinType::Silver,
        _ => return None,
    };
    let timestamp = if item.timestamp == 0 {
        current_unix_seconds()?
    } else {
        i64::try_from(item.timestamp).ok()?
    };
    let batch_combo_id = non_empty(item.batch_combo_id);
    let transaction_id = non_empty(item.tid);
    let gift_icon = item
        .gift_info
        .map(|info| {
            // `img_basic` 是静态礼物图；动画资源只作为缺失时的兼容兜底。
            non_empty(info.img_basic)
                .or_else(|| non_empty(info.webp))
                .or_else(|| non_empty(info.gif))
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let mut blind_gift = message.blind_gift.filter(|blind_gift| {
        blind_gift.blind_gift_config_id != 0
            || blind_gift.original_gift_id != 0
            || !blind_gift.original_gift_name.is_empty()
    });
    if let Some(blind_gift) = blind_gift.as_mut() {
        blind_gift.gift_tip_price = if item.gift_tip_price > 0 {
            item.gift_tip_price
        } else {
            blind_gift.original_gift_price
        };
    }

    let medal = message.medal_info.and_then(|medal| {
        (medal.level > 0).then_some(Medal {
            name: medal.name,
            level: medal.level,
            color: 0,
            room_id: 0,
            anchor_uid: medal.ruid,
            anchor_name: String::new(),
            is_light: medal.is_lighted.unwrap_or(true),
        })
    });
    let guard_level = i64::try_from(message.guard_level)
        .ok()
        .map(GuardLevel::from)
        .unwrap_or(GuardLevel::None);

    Some(Gift {
        gift_id: u64::from(item.gift_id),
        gift_name: item.gift_name,
        gift_icon,
        num: if item.num == 0 { 1 } else { item.num },
        price: item.price,
        total_coin: item.total_coin,
        coin_type,
        sender_uid,
        sender_name,
        sender_face,
        action: if item.action.is_empty() {
            "--".to_owned()
        } else {
            item.action
        },
        timestamp,
        transaction_id,
        batch_combo_id,
        batch_combo_send: None,
        combo_send: None,
        combo_stay_time: Some(item.combo_stay_time),
        combo_total_coin: Some(item.combo_total_coin),
        super_batch_gift_num: Some(item.super_batch_gift_num),
        combo_resources_id: Some(item.combo_resources_id),
        show_batch_combo_send: item.show_batch_combo_send,
        blind_gift,
        medal,
        guard_level,
    })
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn current_unix_seconds() -> Option<i64> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(seconds).ok()
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

fn parse_send_gift_v2(buf: &[u8]) -> PbResult<SendGiftV2> {
    let mut pb = Pb::new(buf);
    let mut out = SendGiftV2::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 0) => out.uid = pb.varint()?,
            (2, 2) => out.uname = pb.string()?,
            (3, 2) => out.face = pb.string()?,
            (5, 0) => out.guard_level = pb.varint()?,
            (8, 2) => out.medal_info = Some(parse_medal_info(pb.bytes()?)?),
            (9, 2) => out.blind_gift = Some(parse_blind_gift(pb.bytes()?)?),
            (10, 2) => out.gift_items.push(parse_gift_item(pb.bytes()?)?),
            (15, 2) => out.sender_uinfo = Some(parse_sender_uinfo(pb.bytes()?)?),
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_gift_item(buf: &[u8]) -> PbResult<GiftItem> {
    let mut pb = Pb::new(buf);
    let mut out = GiftItem::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 0) => out.gift_id = pb.u32()?,
            (2, 2) => out.gift_name = pb.string()?,
            (3, 0) => out.num = pb.u32()?,
            (5, 0) => out.price = pb.varint()?,
            (7, 0) => out.total_coin = pb.varint()?,
            (8, 2) => out.coin_type = pb.string()?,
            (9, 2) => out.tid = pb.string()?,
            (10, 0) => out.timestamp = pb.varint()?,
            (11, 0) => out.super_batch_gift_num = pb.varint()?,
            (12, 2) => out.batch_combo_id = pb.string()?,
            (13, 0) => out.combo_resources_id = pb.varint()?,
            (14, 0) => out.combo_total_coin = pb.varint()?,
            (15, 0) => out.combo_stay_time = pb.varint()?,
            (17, 0) => out.show_batch_combo_send = Some(pb.varint()? != 0),
            (18, 2) => out.action = pb.string()?,
            (35, 2) => out.gift_info = Some(parse_gift_info(pb.bytes()?)?),
            (36, 0) => out.gift_tip_price = pb.varint()?,
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_sender_uinfo(buf: &[u8]) -> PbResult<SenderUInfo> {
    let mut pb = Pb::new(buf);
    let mut out = SenderUInfo::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 0) => out.uid = pb.varint()?,
            (2, 2) => {
                let (name, face) = parse_user_base(pb.bytes()?)?;
                out.name = name;
                out.face = face;
            }
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_user_base(buf: &[u8]) -> PbResult<(String, String)> {
    let mut pb = Pb::new(buf);
    let mut name = String::new();
    let mut face = String::new();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 2) => name = pb.string()?,
            (2, 2) => face = pb.string()?,
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok((name, face))
}

fn parse_medal_info(buf: &[u8]) -> PbResult<MedalInfo> {
    let mut pb = Pb::new(buf);
    let mut out = MedalInfo::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 0) => out.ruid = pb.varint()?,
            (5, 0) => out.level = pb.u32()?,
            (6, 2) => out.name = pb.string()?,
            (11, 0) => out.is_lighted = Some(pb.varint()? != 0),
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

fn parse_gift_info(buf: &[u8]) -> PbResult<GiftInfo> {
    let mut pb = Pb::new(buf);
    let mut out = GiftInfo::default();

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 2) => out.img_basic = pb.string()?,
            (2, 2) => out.webp = pb.string()?,
            (5, 2) => out.gif = pb.string()?,
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

// `gift_item.gift_tip_price` 会在归一化阶段提供爆出礼物单价。
fn parse_blind_gift(buf: &[u8]) -> PbResult<BlindGift> {
    let mut pb = Pb::new(buf);
    let mut out = BlindGift {
        blind_gift_config_id: 0,
        from: 0,
        gift_action: String::new(),
        gift_tip_price: 0,
        original_gift_id: 0,
        original_gift_name: String::new(),
        original_gift_price: 0,
    };

    while !pb.eof() {
        let tag = pb.u32()?;
        match (tag >> 3, tag & 7) {
            (1, 0) => out.blind_gift_config_id = pb.varint()?,
            (2, 0) => out.original_gift_id = pb.varint()?,
            (3, 2) => out.original_gift_name = pb.string()?,
            (4, 0) => out.from = pb.varint()?,
            (5, 2) => out.gift_action = pb.string()?,
            (6, 0) => out.original_gift_price = pb.varint()?,
            (7, 0) => out.gift_tip_price = pb.varint()?,
            (_, wire) => pb.skip(wire)?,
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use serde_json::json;

    use super::Gift;

    fn encode_varint(value: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
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

    fn field_varint(field: u32, value: u64) -> Vec<u8> {
        let mut bytes = encode_varint(u64::from(field) << 3);
        bytes.extend(encode_varint(value));
        bytes
    }

    fn field_bytes(field: u32, value: &[u8]) -> Vec<u8> {
        let mut bytes = encode_varint((u64::from(field) << 3) | 2);
        bytes.extend(encode_varint(value.len() as u64));
        bytes.extend(value);
        bytes
    }

    #[test]
    fn parses_first_v2_gift_and_prefers_nested_sender() {
        let mut base = field_bytes(1, "嵌套用户".as_bytes());
        base.extend(field_bytes(2, b"https://example.invalid/avatar.webp"));
        let mut sender = field_varint(1, 42);
        sender.extend(field_bytes(2, &base));

        let mut info = field_bytes(1, b"https://example.invalid/basic.png");
        info.extend(field_bytes(2, b"https://example.invalid/gift.webp"));

        let mut item = field_varint(1, 100);
        item.extend(field_bytes(2, "小花花".as_bytes()));
        item.extend(field_varint(3, 2));
        item.extend(field_varint(5, 1_000));
        item.extend(field_varint(7, 2_000));
        item.extend(field_bytes(8, b"gold"));
        item.extend(field_bytes(9, b"txn-v2"));
        item.extend(field_varint(10, 1_700_000_000));
        item.extend(field_varint(11, 4));
        item.extend(field_bytes(12, b"combo-v2"));
        item.extend(field_varint(13, 10));
        item.extend(field_varint(14, 4_000));
        item.extend(field_varint(15, 5));
        item.extend(field_varint(17, 1));
        item.extend(field_bytes(18, "投喂".as_bytes()));
        item.extend(field_bytes(35, &info));
        item.extend(field_varint(36, 2_500));

        let mut blind = field_varint(1, 139);
        blind.extend(field_varint(2, 200));
        blind.extend(field_bytes(3, "心动盲盒".as_bytes()));
        blind.extend(field_varint(4, 3));
        blind.extend(field_bytes(5, "爆出".as_bytes()));
        blind.extend(field_varint(6, 2_000));
        blind.extend(field_varint(7, 9_999));

        let mut message = field_varint(1, 7);
        message.extend(field_bytes(2, "顶层用户".as_bytes()));
        message.extend(field_bytes(9, &blind));
        message.extend(field_bytes(10, &item));
        message.extend(field_bytes(10, &field_varint(1, 999)));
        message.extend(field_bytes(15, &sender));

        let encoded = base64::engine::general_purpose::STANDARD.encode(message);
        let gift =
            Gift::parse_v2(&json!({ "data": { "pb": encoded } })).expect("valid SEND_GIFT_V2");
        let blind = gift.blind_gift.as_ref().expect("blind gift metadata");

        assert_eq!(gift.gift_id, 100);
        assert_eq!(gift.num, 2);
        assert_eq!(gift.sender_uid, 42);
        assert_eq!(gift.sender_name, "嵌套用户");
        assert_eq!(gift.transaction_id.as_deref(), Some("txn-v2"));
        assert_eq!(gift.batch_combo_id.as_deref(), Some("combo-v2"));
        assert_eq!(gift.combo_total_coin, Some(4_000));
        assert_eq!(gift.super_batch_gift_num, Some(4));
        assert_eq!(gift.combo_resources_id, Some(10));
        assert_eq!(gift.show_batch_combo_send, Some(true));
        assert_eq!(gift.gift_icon, "https://example.invalid/basic.png");
        assert_eq!(gift.revealed_total_coin(), 5_000);
        assert_eq!(blind.blind_gift_config_id, 139);
        assert_eq!(blind.original_gift_id, 200);
        assert_eq!(blind.original_gift_name, "心动盲盒");
        assert_eq!(blind.from, 3);
        assert_eq!(blind.gift_action, "爆出");
        assert_eq!(blind.original_gift_price, 2_000);
        assert_eq!(blind.gift_tip_price, 2_500);
    }
}
