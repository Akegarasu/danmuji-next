//! Bilibili 扫码登录模块

use reqwest::{cookie::Jar, Client};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 默认 User-Agent
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

// ============== API 响应结构 ==============

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    code: i32,
    #[allow(dead_code)]
    message: String,
    data: T,
}

// ============== 二维码相关 ==============

/// 二维码生成响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QRCodeData {
    /// 二维码内容 URL
    pub url: String,
    /// 二维码 key (用于轮询状态)
    pub qrcode_key: String,
}

#[derive(Debug, Deserialize)]
struct QRCodeGenerateData {
    url: String,
    qrcode_key: String,
}

/// 生成登录二维码
pub async fn generate_qrcode(client: &Client) -> Result<QRCodeData, String> {
    let url = "https://passport.bilibili.com/x/passport-login/web/qrcode/generate?source=main-fe-header&go_url=https://www.bilibili.com/&web_location=333.1007";

    let resp: ApiResponse<QRCodeGenerateData> = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Referer", "https://www.bilibili.com/")
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    if resp.code != 0 {
        return Err(format!("API错误: {}", resp.message));
    }

    Ok(QRCodeData {
        url: resp.data.url,
        qrcode_key: resp.data.qrcode_key,
    })
}

// ============== 扫码状态轮询 ==============

/// 扫码状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QRCodeStatus {
    /// 状态码
    /// - 86101: 未扫码
    /// - 86090: 已扫码未确认
    /// - 86038: 二维码已过期
    /// - 0: 登录成功
    pub code: i32,
    /// 状态信息
    pub message: String,
    /// 登录成功时的 refresh_token
    pub refresh_token: Option<String>,
    /// 登录成功时的 Cookie
    pub cookie: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QRCodePollData {
    code: i32,
    message: String,
    refresh_token: String,
    #[allow(dead_code)]
    url: String,
}

/// 轮询扫码状态
pub async fn poll_qrcode_status(qrcode_key: &str) -> Result<QRCodeStatus, String> {
    // 创建带 Cookie Jar 的客户端以捕获登录后的 Cookie
    let jar = Arc::new(Jar::default());
    let client = Client::builder()
        .cookie_store(true)
        .cookie_provider(jar.clone())
        .build()
        .map_err(|e| format!("创建客户端失败: {}", e))?;

    let url = format!(
        "https://passport.bilibili.com/x/passport-login/web/qrcode/poll?qrcode_key={}&source=main-fe-header",
        qrcode_key
    );

    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Referer", "https://www.bilibili.com/")
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    // 获取响应中的 Set-Cookie
    let cookies: Vec<String> = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| {
            // 只保留 cookie 名=值 部分
            s.split(';').next().unwrap_or("").to_string()
        })
        .collect();

    let data: ApiResponse<QRCodePollData> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let mut status = QRCodeStatus {
        code: data.data.code,
        message: data.data.message,
        refresh_token: None,
        cookie: None,
    };

    // 登录成功
    if data.data.code == 0 {
        status.refresh_token = Some(data.data.refresh_token);
        // 组合 Cookie
        if !cookies.is_empty() {
            status.cookie = Some(cookies.join("; "));
        }
    }

    Ok(status)
}

// ============== 用户信息 ==============

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// 用户 ID
    pub uid: u64,
    /// 用户名
    pub uname: String,
    /// 头像 URL
    pub face: String,
    /// 是否登录
    pub is_login: bool,
}

#[derive(Debug, Deserialize)]
struct NavData {
    #[serde(default)]
    mid: u64,
    #[serde(default)]
    uname: String,
    #[serde(default)]
    face: String,
    #[serde(default, rename = "isLogin")]
    is_login: bool,
}

/// 获取用户信息
pub async fn get_user_info(client: &Client, cookie: &str) -> Result<UserInfo, String> {
    let url = "https://api.bilibili.com/x/web-interface/nav";

    let resp: ApiResponse<NavData> = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Cookie", cookie)
        .header("Referer", "https://www.bilibili.com/")
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    if resp.code != 0 {
        return Err(format!("API错误: {}", resp.message));
    }

    Ok(UserInfo {
        uid: resp.data.mid,
        uname: resp.data.uname,
        face: resp.data.face,
        is_login: resp.data.is_login,
    })
}

/// 验证 Cookie 是否有效
pub async fn validate_cookie(client: &Client, cookie: &str) -> Result<bool, String> {
    match get_user_info(client, cookie).await {
        Ok(info) => Ok(info.is_login),
        Err(_) => Ok(false),
    }
}
