//! 消息类型定义

mod danmaku;
mod gift;
mod guard;
mod live_status;
mod online_rank_count;
mod online_rank_v2;
mod superchat;
mod user;

pub use danmaku::*;
pub use gift::*;
pub use guard::*;
pub use live_status::*;
pub use online_rank_count::*;
pub use online_rank_v2::*;
pub use superchat::*;
pub use user::*;

use std::io::Write;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::blivedm::packet::{Operation, Packet};

/// 所有事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    /// 弹幕消息
    Danmaku(Danmaku),
    /// 礼物
    Gift(Gift),
    /// 醒目留言
    SuperChat(SuperChat),
    /// 大航海（舰长/提督/总督）
    GuardBuy(GuardBuy),
    /// 开播
    LiveStart(LiveStartData),
    /// 下播
    LiveStop(PreparingData),
    /// 在线人数统计
    OnlineRankCount(OnlineRankCount),
    /// 高能用户排行榜
    OnlineRankV2(OnlineRankV2),
    /// 原始事件（未解析的 CMD）
    Raw { cmd: String, payload: Value },
}

/// 从数据包解析事件
pub fn parse_event(packet: &Packet) -> Option<Event> {
    match packet.operation {
        Operation::HeartbeatReply => {
            // 心跳响应，忽略（人气值已废弃）
            None
        }
        Operation::Notification => {
            // 通知消息，body 是 JSON
            let json_str = std::str::from_utf8(&packet.body).ok()?;
            parse_notification(json_str)
        }
        Operation::EnterRoomReply => {
            // 进入房间响应，通常忽略
            None
        }
        _ => None,
    }
}

/// 解析通知消息
fn parse_notification(json_str: &str) -> Option<Event> {
    let value: Value = serde_json::from_str(json_str).ok()?;

    if crate::is_dev_mode() {
        dump_raw_value(&value);
    }

    let cmd = value.get("cmd")?.as_str()?;

    // 处理带参数的 CMD（例如 "DANMU_MSG:4:0:2:2:2:0"）
    let cmd_base = cmd.split(':').next().unwrap_or(cmd);

    match cmd_base {
        "DANMU_MSG" => {
            let danmaku = Danmaku::parse(&value)?;
            Some(Event::Danmaku(danmaku))
        }
        "SEND_GIFT" => {
            let gift = Gift::parse(&value)?;
            Some(Event::Gift(gift))
        }
        "SUPER_CHAT_MESSAGE" => {
            let superchat = SuperChat::parse(&value)?;
            Some(Event::SuperChat(superchat))
        }
        "GUARD_BUY" => {
            let guard = GuardBuy::parse(&value)?;
            Some(Event::GuardBuy(guard))
        }
        "LIVE" => {
            let data = LiveStartData::parse(&value)?;
            Some(Event::LiveStart(data))
        }
        "PREPARING" => {
            let data = PreparingData::parse(&value)?;
            Some(Event::LiveStop(data))
        }
        "ONLINE_RANK_COUNT" => {
            let online_rank_count = OnlineRankCount::parse(&value)?;
            Some(Event::OnlineRankCount(online_rank_count))
        }
        "ONLINE_RANK_V2" => {
            let online_rank_v2 = OnlineRankV2::parse(&value)?;
            Some(Event::OnlineRankV2(online_rank_v2))
        }
        // 其他已知但不处理的 CMD
        "INTERACT_WORD"
        | "ENTRY_EFFECT"
        | "COMBO_SEND"
        | "WATCHED_CHANGE"
        | "STOP_LIVE_ROOM_LIST"
        | "WIDGET_BANNER"
        | "HOT_RANK_CHANGED"
        | "HOT_RANK_CHANGED_V2"
        | "LIKE_INFO_V3_CLICK"
        | "LIKE_INFO_V3_UPDATE"
        | "COMMON_NOTICE_DANMAKU"
        | "ROOM_REAL_TIME_MESSAGE_UPDATE"
        | "POPULARITY_RED_POCKET_START"
        | "POPULARITY_RED_POCKET_WINNER_LIST" => None,
        // 未知 CMD，返回原始数据
        _ => Some(Event::Raw {
            cmd: cmd.to_string(),
            payload: value,
        }),
    }
}

// ==================== Dev Mode: Raw Value Dump ====================

fn get_dump_file() -> &'static std::sync::Mutex<Option<std::fs::File>> {
    static DUMP_FILE: OnceLock<std::sync::Mutex<Option<std::fs::File>>> = OnceLock::new();
    DUMP_FILE.get_or_init(|| {
        let path = crate::config::get_config_dir().join("raw_dump.txt");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path);
        match file {
            Ok(f) => {
                eprintln!("[DEV] Raw dump file: {}", path.display());
                std::sync::Mutex::new(Some(f))
            }
            Err(e) => {
                eprintln!("[DEV] Failed to open raw dump file: {}", e);
                std::sync::Mutex::new(None)
            }
        }
    })
}

fn dump_raw_value(value: &Value) {
    let guard = get_dump_file().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref file) = *guard {
        let mut writer = std::io::BufWriter::new(file);
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let json = serde_json::to_string(value).unwrap_or_default();
        let _ = writeln!(writer, "[{}] {}", timestamp, json);
        let _ = writer.flush();
    }
}
