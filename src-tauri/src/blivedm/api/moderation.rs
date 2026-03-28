use serde::{Deserialize, Serialize};

use crate::blivedm::error::{Error, Result};
use super::{ApiResponse, USER_AGENT};
use super::cookie::extract_cookie_value;

/// 禁言响应
#[derive(Serialize)]
pub struct SilentUserResponse {
    pub success: bool,
    pub code: i64,
    pub message: String,
}

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

    let encoded_keyword = urlencoding::encode(&keyword).into_owned();
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
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
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

    let encoded_keyword = urlencoding::encode(&keyword).into_owned();
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
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
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

/// 禁言用户
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
