use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use super::wbi::get_wbi_keys;
use super::{ApiResponse, USER_AGENT};
use crate::error::{Error, Result};

/// 贡献排行榜类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionRankType {
    /// 当前在线贡献榜。
    Online,
    /// 当日贡献榜。
    Daily,
    /// 本周贡献榜。
    Weekly,
    /// 本月贡献榜。
    Monthly,
}

impl ContributionRankType {
    fn api_params(self) -> (&'static str, &'static str) {
        match self {
            Self::Online => ("online_rank", "contribution_rank"),
            Self::Daily => ("daily_rank", "today_rank"),
            Self::Weekly => ("weekly_rank", "current_week_rank"),
            Self::Monthly => ("monthly_rank", "current_month_rank"),
        }
    }
}

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
    /// 当前排行榜类型。
    pub rank_type: ContributionRankType,
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
    get_contribution_rank_by_type(
        client,
        room_id,
        ruid,
        cookie,
        ContributionRankType::Online,
        page,
        page_size,
    )
    .await
}

/// 获取指定类型的贡献排行榜（需要 WBI 签名）。
pub async fn get_contribution_rank_by_type(
    client: &Client,
    room_id: u64,
    ruid: u64,
    cookie: Option<&str>,
    rank_type: ContributionRankType,
    page: u32,
    page_size: u32,
) -> Result<ContributionRankResponse> {
    // 获取 WBI 密钥
    let wbi_keys = get_wbi_keys(client).await?;

    let (type_param, switch_param) = rank_type.api_params();

    // 构建 URL
    let mut url = Url::parse(
        "https://api.live.bilibili.com/xlive/general-interface/v1/rank/queryContributionRank",
    )
    .map_err(|e| Error::Config(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("ruid", &ruid.to_string())
        .append_pair("room_id", &room_id.to_string())
        .append_pair("page", &page.max(1).to_string())
        .append_pair("page_size", &page_size.clamp(1, 100).to_string())
        .append_pair("type", type_param)
        .append_pair("switch", switch_param)
        .append_pair("platform", "web")
        .append_pair("web_location", "0.0");

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
        rank_type,
        count: resp.data.count,
        list,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_rank_types_to_api_parameters() {
        assert_eq!(
            ContributionRankType::Online.api_params(),
            ("online_rank", "contribution_rank")
        );
        assert_eq!(
            ContributionRankType::Daily.api_params(),
            ("daily_rank", "today_rank")
        );
        assert_eq!(
            ContributionRankType::Weekly.api_params(),
            ("weekly_rank", "current_week_rank")
        );
        assert_eq!(
            ContributionRankType::Monthly.api_params(),
            ("monthly_rank", "current_month_rank")
        );
    }
}
