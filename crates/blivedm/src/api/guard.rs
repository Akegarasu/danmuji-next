use std::collections::HashSet;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use super::{ApiResponse, USER_AGENT};
use crate::error::{Error, Result};

/// 大航海榜用户。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardTopListUser {
    pub uid: u64,
    pub name: String,
    pub face: String,
    pub rank: u32,
    /// 接口返回的陪伴值。
    pub accompany: u64,
    pub score: u64,
    pub guard_level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal_level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal_color: Option<String>,
}

/// 大航海榜响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardTopListResponse {
    pub count: u32,
    pub total_pages: u32,
    pub current_page: u32,
    pub list: Vec<GuardTopListUser>,
}

#[derive(Debug, Deserialize)]
struct GuardTopListData {
    info: GuardTopListInfo,
    #[serde(default)]
    list: Vec<GuardTopListItem>,
    #[serde(default)]
    top3: Vec<GuardTopListItem>,
}

#[derive(Debug, Deserialize)]
struct GuardTopListInfo {
    num: u32,
    page: u32,
    now: u32,
}

#[derive(Debug, Deserialize)]
struct GuardTopListItem {
    rank: u32,
    #[serde(default)]
    accompany: u64,
    #[serde(default)]
    score: u64,
    uinfo: GuardUserInfo,
}

#[derive(Debug, Deserialize)]
struct GuardUserInfo {
    uid: u64,
    base: GuardUserBase,
    medal: Option<GuardMedal>,
    guard: Option<GuardInfo>,
}

#[derive(Debug, Deserialize)]
struct GuardUserBase {
    name: String,
    face: String,
}

#[derive(Debug, Deserialize)]
struct GuardMedal {
    name: String,
    level: u32,
    color_start: u32,
    #[serde(default)]
    guard_level: u8,
}

#[derive(Debug, Deserialize)]
struct GuardInfo {
    level: u8,
}

/// 获取大航海榜。
///
/// Bilibili 将前三名放在 `top3`、其余用户放在 `list`；此函数会合并、去重并按名次排序。
pub async fn get_guard_top_list(
    client: &Client,
    room_id: u64,
    ruid: u64,
    cookie: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<GuardTopListResponse> {
    let mut url = Url::parse("https://api.live.bilibili.com/xlive/app-room/v2/guardTab/topListNew")
        .map_err(|e| Error::Config(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("roomid", &room_id.to_string())
        .append_pair("page", &page.max(1).to_string())
        .append_pair("ruid", &ruid.to_string())
        .append_pair("page_size", &page_size.clamp(1, 100).to_string())
        .append_pair("typ", "3")
        .append_pair("platform", "web");

    let mut request = client
        .get(url.as_str())
        .header("User-Agent", USER_AGENT)
        .header("Referer", format!("https://live.bilibili.com/{room_id}"));

    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }

    let response: ApiResponse<GuardTopListData> = request.send().await?.json().await?;
    if response.code != 0 {
        return Err(Error::Api {
            code: response.code,
            message: response.message,
        });
    }

    Ok(normalize_response(response.data))
}

/// 获取完整大航海榜，自动遍历所有分页。
pub async fn get_all_guard_top_list(
    client: &Client,
    room_id: u64,
    ruid: u64,
    cookie: Option<&str>,
) -> Result<GuardTopListResponse> {
    const PAGE_SIZE: u32 = 20;

    let mut response = get_guard_top_list(client, room_id, ruid, cookie, 1, PAGE_SIZE).await?;
    let mut seen: HashSet<u64> = response.list.iter().map(|user| user.uid).collect();

    for page in 2..=response.total_pages {
        let next = get_guard_top_list(client, room_id, ruid, cookie, page, PAGE_SIZE).await?;
        response
            .list
            .extend(next.list.into_iter().filter(|user| seen.insert(user.uid)));
    }

    response.list.sort_by_key(|user| user.rank);
    Ok(response)
}

fn normalize_response(data: GuardTopListData) -> GuardTopListResponse {
    let mut seen = HashSet::new();
    let mut list: Vec<_> = data
        .top3
        .into_iter()
        .chain(data.list)
        .filter(|item| seen.insert(item.uinfo.uid))
        .map(|item| {
            let guard_level = item
                .uinfo
                .guard
                .as_ref()
                .map(|guard| guard.level)
                .or_else(|| item.uinfo.medal.as_ref().map(|medal| medal.guard_level))
                .unwrap_or_default();
            let (medal_name, medal_level, medal_color) = item
                .uinfo
                .medal
                .map(|medal| {
                    (
                        Some(medal.name),
                        Some(medal.level),
                        Some(format!("#{:06x}", medal.color_start)),
                    )
                })
                .unwrap_or((None, None, None));

            GuardTopListUser {
                uid: item.uinfo.uid,
                name: item.uinfo.base.name,
                face: item.uinfo.base.face,
                rank: item.rank,
                accompany: item.accompany,
                score: item.score,
                guard_level,
                medal_name,
                medal_level,
                medal_color,
            }
        })
        .collect();
    list.sort_by_key(|user| user.rank);

    GuardTopListResponse {
        count: data.info.num,
        total_pages: data.info.page,
        current_page: data.info.now,
        list,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_top_three_and_regular_list() {
        let json = r#"{
            "info":{"num":4,"page":1,"now":1},
            "top3":[{"rank":1,"accompany":20,"score":0,"uinfo":{"uid":1,"base":{"name":"one","face":"1.jpg"},"medal":{"name":"medal","level":30,"color_start":398668,"guard_level":3},"guard":{"level":3}}}],
            "list":[
                {"rank":1,"accompany":20,"score":0,"uinfo":{"uid":1,"base":{"name":"one","face":"1.jpg"},"medal":null,"guard":{"level":3}}},
                {"rank":4,"accompany":10,"score":0,"uinfo":{"uid":4,"base":{"name":"four","face":"4.jpg"},"medal":null,"guard":{"level":3}}}
            ]
        }"#;
        let data: GuardTopListData = serde_json::from_str(json).unwrap();
        let response = normalize_response(data);

        assert_eq!(response.count, 4);
        assert_eq!(response.list.len(), 2);
        assert_eq!(response.list[0].uid, 1);
        assert_eq!(response.list[0].medal_color.as_deref(), Some("#06154c"));
        assert_eq!(response.list[1].rank, 4);
    }
}
