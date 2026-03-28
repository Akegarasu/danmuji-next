use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::blivedm::error::{Error, Result};
use super::{ApiResponse, USER_AGENT};

/// 房间信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub room_id: u64,
    pub short_id: u64,
    pub uid: u64,
    pub live_status: i32,
    pub title: String,
}

#[derive(Debug, Deserialize)]
struct RoomInitData {
    room_id: u64,
    short_id: u64,
    uid: u64,
    live_status: i32,
}

#[derive(Debug, Deserialize)]
struct RoomInfoData {
    room_info: RoomInfoInner,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RoomInfoInner {
    room_id: u64,
    short_id: u64,
    uid: u64,
    title: String,
    live_status: i32,
}

/// 获取房间初始化信息（获取真实房间号）
pub async fn get_room_init(client: &Client, room_id: u64) -> Result<RoomInfo> {
    let url = format!(
        "https://api.live.bilibili.com/room/v1/Room/room_init?id={}",
        room_id
    );

    let resp: ApiResponse<RoomInitData> = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .json()
        .await?;

    if resp.code != 0 {
        return Err(Error::Api {
            code: resp.code,
            message: resp.message,
        });
    }

    // 获取更详细的房间信息（包括标题）
    let title = get_room_title(client, resp.data.room_id).await.ok();

    Ok(RoomInfo {
        room_id: resp.data.room_id,
        short_id: resp.data.short_id,
        uid: resp.data.uid,
        live_status: resp.data.live_status,
        title: title.unwrap_or_default(),
    })
}

/// 获取房间标题
async fn get_room_title(client: &Client, room_id: u64) -> Result<String> {
    let url = format!(
        "https://api.live.bilibili.com/xlive/web-room/v1/index/getInfoByRoom?room_id={}",
        room_id
    );

    let resp: ApiResponse<RoomInfoData> = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .json()
        .await?;

    if resp.code != 0 {
        return Err(Error::Api {
            code: resp.code,
            message: resp.message,
        });
    }

    Ok(resp.data.room_info.title)
}
