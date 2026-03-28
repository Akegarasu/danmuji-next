//! Bilibili HTTP API 调用

use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::blivedm::error::{Error, Result};
use crate::blivedm::wbi::get_wbi_keys;

/// 默认 User-Agent (与 blivedm-go 保持一致)
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:137.0) Gecko/20100101 Firefox/137.0";

/// 房间信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub room_id: u64,
    pub short_id: u64,
    pub uid: u64,
    pub live_status: i32,
    pub title: String,
}

/// 弹幕服务器信息
#[derive(Debug, Clone)]
pub struct DanmuServerInfo {
    pub token: String,
    pub host_list: Vec<DanmuHost>,
}

/// 弹幕服务器主机
#[derive(Debug, Clone)]
pub struct DanmuHost {
    pub host: String,
    #[allow(dead_code)]
    pub port: u16,
    pub wss_port: u16,
    #[allow(dead_code)]
    pub ws_port: u16,
}

// ============== API 响应结构 ==============

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    code: i32,
    message: String,
    data: T,
}

/// 基础响应（用于先检查 code）
#[derive(Debug, Deserialize)]
struct BaseResponse {
    code: i32,
    message: String,
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

#[derive(Debug, Deserialize)]
struct DanmuInfoData {
    token: String,
    host_list: Vec<DanmuHostData>,
}

#[derive(Debug, Deserialize)]
struct DanmuHostData {
    host: String,
    port: u16,
    wss_port: u16,
    ws_port: u16,
}

// ============== API 函数 ==============

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

/// 获取弹幕服务器信息（需要 WBI 签名）
pub async fn get_danmu_info(
    client: &Client,
    room_id: u64,
    cookie: Option<&str>,
) -> Result<DanmuServerInfo> {
    // 获取 WBI 密钥
    let wbi_keys = get_wbi_keys(client).await?;

    // 构建 URL 并签名
    let mut url = Url::parse(&format!(
        "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo?id={}&type=0",
        room_id
    ))
    .map_err(|e| Error::Config(e.to_string()))?;

    wbi_keys.sign_url(&mut url)?;

    // 发送请求
    let mut req = client.get(url.as_str()).header("User-Agent", USER_AGENT);

    if let Some(cookie) = cookie {
        req = req.header("Cookie", cookie);
    }

    let response = req.send().await?;

    let body = response.text().await?;

    // 先解析基础响应检查 code
    let base: BaseResponse = serde_json::from_str(&body)?;

    // -352 错误码表示风控，降级使用默认服务器
    if base.code == -352 || base.code == 352 {
        println!("⚠️  收到 {} 错误，使用默认弹幕服务器", base.code);
        return Ok(DanmuServerInfo {
            token: String::new(),
            host_list: vec![DanmuHost {
                host: "broadcastlv.chat.bilibili.com".to_string(),
                port: 2243,
                wss_port: 443,
                ws_port: 2244,
            }],
        });
    }

    // 其他错误
    if base.code != 0 {
        return Err(Error::Api {
            code: base.code,
            message: base.message,
        });
    }

    // 成功时解析完整响应
    let resp: ApiResponse<DanmuInfoData> = serde_json::from_str(&body)?;

    // 如果 host_list 为空，使用默认服务器
    let host_list = if resp.data.host_list.is_empty() {
        vec![DanmuHost {
            host: "broadcastlv.chat.bilibili.com".to_string(),
            port: 2243,
            wss_port: 443,
            ws_port: 2244,
        }]
    } else {
        resp.data
            .host_list
            .into_iter()
            .map(|h| DanmuHost {
                host: h.host,
                port: h.port,
                wss_port: h.wss_port,
                ws_port: h.ws_port,
            })
            .collect()
    };

    Ok(DanmuServerInfo {
        token: resp.data.token,
        host_list,
    })
}

/// 从 Cookie 中提取 UID
pub fn extract_uid_from_cookie(cookie: &str) -> Option<u64> {
    // 查找 DedeUserID=xxxxx
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("DedeUserID=") {
            return value.parse().ok();
        }
    }
    None
}

/// 从 Cookie 中提取 buvid
pub fn extract_buvid_from_cookie(cookie: &str) -> Option<String> {
    // 查找 _uuid=xxxxx 或 buvid3=xxxxx
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("_uuid=") {
            return Some(value.to_string());
        }
        if let Some(value) = part.strip_prefix("buvid3=") {
            return Some(value.to_string());
        }
    }
    None
}

// ============== 贡献排行榜 ==============

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

fn extract_cookie_value(cookie: &str, name: &str) -> Option<String> {
    cookie
        .split(';')
        .map(|pair| pair.trim())
        .find(|pair| pair.starts_with(&format!("{}=", name)))
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| value.to_string())
}
/// 禁言响应
#[derive(serde::Serialize)]
pub struct SilentUserResponse {
    pub success: bool,
    pub code: i64,
    pub message: String,
}

// ============== 屏蔽关键词 ==============

/// 屏蔽关键词条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShieldKeyword {
    pub keyword: String,
    pub uid: u64,
    pub name: String,
    pub is_anchor: i32,
}

/// 屏蔽关键词列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShieldKeywordListResponse {
    pub keyword_list: Vec<ShieldKeyword>,
    pub max_limit: u32,
}

#[derive(Debug, Deserialize)]
struct ShieldKeywordListData {
    keyword_list: Vec<ShieldKeywordItem>,
    max_limit: u32,
}

#[derive(Debug, Deserialize)]
struct ShieldKeywordItem {
    keyword: String,
    uid: u64,
    name: String,
    is_anchor: i32,
}

/// 获取屏蔽关键词列表
pub async fn get_shield_keyword_list(
    room_id: u64,
    cookie: String,
) -> Result<ShieldKeywordListResponse> {
    let csrf = extract_cookie_value(&cookie, "bili_jct")
        .ok_or_else(|| Error::AuthFailed("Missing bili_jct".to_string()))?;

    let params = [
        ("room_id", room_id.to_string()),
        ("csrf_token", csrf.clone()),
        ("csrf", csrf),
        ("visit_id", String::new()),
    ];

    let client = reqwest::Client::new();
    let resp: ApiResponse<ShieldKeywordListData> = client
        .post("https://api.live.bilibili.com/xlive/web-ucenter/v1/banned/GetShieldKeywordList")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, "https://live.bilibili.com/")
        .header(reqwest::header::COOKIE, &cookie)
        .form(&params)
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

    Ok(ShieldKeywordListResponse {
        keyword_list: resp
            .data
            .keyword_list
            .into_iter()
            .map(|item| ShieldKeyword {
                keyword: item.keyword,
                uid: item.uid,
                name: item.name,
                is_anchor: item.is_anchor,
            })
            .collect(),
        max_limit: resp.data.max_limit,
    })
}

/// 添加屏蔽关键词
pub async fn add_shield_keyword(
    room_id: u64,
    keyword: String,
    cookie: String,
) -> Result<SilentUserResponse> {
    let csrf = extract_cookie_value(&cookie, "bili_jct")
        .ok_or_else(|| Error::AuthFailed("Missing bili_jct".to_string()))?;

    let encoded_keyword =
        urlencoding::encode(&keyword).into_owned();

    let body = format!(
        "room_id={}&keyword={}&csrf_token={}&csrf={}",
        room_id, encoded_keyword, csrf, csrf
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.live.bilibili.com/xlive/web-ucenter/v1/banned/AddShieldKeyword")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, "https://link.bilibili.com/")
        .header(reqwest::header::COOKIE, &cookie)
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?;

    let json: serde_json::Value = resp.json().await?;
    let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    let message = json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("请求完成")
        .to_string();

    Ok(SilentUserResponse {
        success: code == 0,
        code,
        message,
    })
}

/// 删除屏蔽关键词
pub async fn del_shield_keyword(
    room_id: u64,
    keyword: String,
    cookie: String,
) -> Result<SilentUserResponse> {
    let csrf = extract_cookie_value(&cookie, "bili_jct")
        .ok_or_else(|| Error::AuthFailed("Missing bili_jct".to_string()))?;

    let encoded_keyword =
        urlencoding::encode(&keyword).into_owned();

    let body = format!(
        "room_id={}&keyword={}&csrf_token={}&csrf={}",
        room_id, encoded_keyword, csrf, csrf
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.live.bilibili.com/xlive/web-ucenter/v1/banned/DelShieldKeyword")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, "https://link.bilibili.com/")
        .header(reqwest::header::COOKIE, &cookie)
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?;

    let json: serde_json::Value = resp.json().await?;
    let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    let message = json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("请求完成")
        .to_string();

    Ok(SilentUserResponse {
        success: code == 0,
        code,
        message,
    })
}

pub async fn add_silent_user(
    room_id: u64,
    tuid: u64,
    cookie: String,
    r#type: i32,
    hour: i32,
    msg: Option<String>,
) -> Result<SilentUserResponse> {
    // 从 Cookie 中提取 bili_jct 作为 csrf
    let csrf = extract_cookie_value(&cookie, "bili_jct")
        .ok_or_else(|| Error::AuthFailed("Missing bili_jct".to_string()))?;

    let referer = format!("https://live.bilibili.com/{}", room_id);

    let params = [
        ("room_id", room_id.to_string()),
        ("tuid", tuid.to_string()),
        ("msg", msg.unwrap_or_default()),
        ("mobile_app", "web".to_string()),
        ("type", r#type.to_string()),
        ("hour", hour.to_string()),
        ("csrf_token", csrf.clone()),
        ("csrf", csrf),
        ("visit_id", String::new()),
    ];

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.live.bilibili.com/xlive/web-ucenter/v1/banned/AddSilentUser")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, referer)
        .header(reqwest::header::COOKIE, &cookie)
        .form(&params)
        .send()
        .await?;

    let status = resp.status();
    let json: serde_json::Value = resp.json().await?;

    let code = json
        .get("code")
        .and_then(|v| v.as_i64())
        .unwrap_or(if status.is_success() { 0 } else { -1 });
    let message = json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("请求完成")
        .to_string();

    Ok(SilentUserResponse {
        success: code == 0,
        code,
        message,
    })
}
