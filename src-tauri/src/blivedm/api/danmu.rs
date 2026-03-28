use reqwest::Client;
use serde::Deserialize;
use url::Url;

use crate::blivedm::error::{Error, Result};
use super::wbi::get_wbi_keys;
use super::{ApiResponse, BaseResponse, USER_AGENT};

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
