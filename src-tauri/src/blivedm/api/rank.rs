use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::blivedm::error::{Error, Result};
use super::wbi::get_wbi_keys;
use super::{ApiResponse, USER_AGENT};

/// 贡献排行榜用户
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionRankUser {
    pub uid: u64,
    pub name: String,
    pub face: String,
    pub rank: u32,
    pub score: u64,
    pub guard_level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal_level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medal_color: Option<String>,
}

/// 贡献排行榜响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionRankResponse {
    /// 总人数
    pub count: u32,
    /// 排行榜用户列表
    pub list: Vec<ContributionRankUser>,
}

#[derive(Debug, Deserialize)]
struct ContributionRankData {
    count: u32,
    item: Vec<ContributionRankItem>,
}

#[derive(Debug, Deserialize)]
struct ContributionRankItem {
    uid: u64,
    name: String,
    face: String,
    rank: u32,
    score: u64,
    guard_level: u8,
    medal_info: Option<ContributionMedalInfo>,
}

#[derive(Debug, Deserialize)]
struct ContributionMedalInfo {
    medal_name: String,
    level: u32,
    medal_color_start: u32,
}

/// 获取贡献排行榜（需要 WBI 签名）
///
/// # Arguments
/// * `client` - HTTP 客户端
/// * `room_id` - 房间号
/// * `ruid` - 主播 UID
/// * `cookie` - 用户 Cookie
/// * `page` - 页码（从 1 开始）
/// * `page_size` - 每页数量（最大 100）
pub async fn get_contribution_rank(
    client: &Client,
    room_id: u64,
    ruid: u64,
    cookie: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<ContributionRankResponse> {
    // 获取 WBI 密钥
    let wbi_keys = get_wbi_keys(client).await?;

    // 构建 URL
    let mut url = Url::parse(&format!(
        "https://api.live.bilibili.com/xlive/general-interface/v1/rank/queryContributionRank?ruid={}&room_id={}&page={}&page_size={}&type=online_rank&switch=contribution_rank&platform=web&web_location=0.0",
        ruid, room_id, page, page_size
    ))
    .map_err(|e| Error::Config(e.to_string()))?;

    // 签名
    wbi_keys.sign_url(&mut url)?;

    // 发送请求
    let mut req = client
        .get(url.as_str())
        .header("User-Agent", USER_AGENT)
        .header("Referer", "https://live.bilibili.com/");

    if let Some(cookie) = cookie {
        req = req.header("Cookie", cookie);
    }

    let resp: ApiResponse<ContributionRankData> = req.send().await?.json().await?;

    if resp.code != 0 {
        return Err(Error::Api {
            code: resp.code,
            message: resp.message,
        });
    }

    // 转换数据
    let list = resp
        .data
        .item
        .into_iter()
        .map(|item| {
            let (medal_name, medal_level, medal_color) = item
                .medal_info
                .map(|m| {
                    let color = format!("#{:06x}", m.medal_color_start);
                    (Some(m.medal_name), Some(m.level), Some(color))
                })
                .unwrap_or((None, None, None));

            ContributionRankUser {
                uid: item.uid,
                name: item.name,
                face: item.face,
                rank: item.rank,
                score: item.score,
                guard_level: item.guard_level,
                medal_name,
                medal_level,
                medal_color,
            }
        })
        .collect();

    Ok(ContributionRankResponse {
        count: resp.data.count,
        list,
    })
}
