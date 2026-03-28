//! WBI 签名算法
//!
//! 参考: https://github.com/SocialSisterYi/bilibili-API-collect/blob/master/docs/misc/sign/wbi.md

use std::sync::RwLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::Deserialize;
use url::Url;

use crate::blivedm::error::{Error, Result};

/// WBI 密钥混淆表
const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

/// WBI 密钥缓存过期时间
const WBI_CACHE_TTL: Duration = Duration::from_secs(3600);

/// WBI 密钥
#[derive(Debug, Clone, Default)]
pub struct WbiKeys {
    #[allow(dead_code)]
    pub img_key: String,
    #[allow(dead_code)]
    pub sub_key: String,
    pub mixin_key: String,
}

impl WbiKeys {
    /// 计算混淆后的密钥
    fn compute_mixin_key(img_key: &str, sub_key: &str) -> String {
        let combined = format!("{}{}", img_key, sub_key);
        let bytes = combined.as_bytes();

        MIXIN_KEY_ENC_TAB
            .iter()
            .take(32)
            .filter_map(|&i| bytes.get(i).copied())
            .map(|b| b as char)
            .collect()
    }

    /// 对 URL 进行签名
    pub fn sign_url(&self, url: &mut Url) -> Result<()> {
        let wts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // 获取现有参数
        let mut params: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        // 添加时间戳
        params.push(("wts".to_string(), wts.to_string()));

        // 过滤特殊字符并排序
        params.sort_by(|a, b| a.0.cmp(&b.0));

        // 构建查询字符串
        let query_string: String = params
            .iter()
            .map(|(k, v)| {
                let filtered_v: String = v
                    .chars()
                    .filter(|&c| c != '!' && c != '\'' && c != '(' && c != ')' && c != '*')
                    .collect();
                format!(
                    "{}={}",
                    urlencoding::encode(k),
                    urlencoding::encode(&filtered_v)
                )
            })
            .collect::<Vec<_>>()
            .join("&");

        // 计算 w_rid
        let sign_input = format!("{}{}", query_string, self.mixin_key);
        let w_rid = format!("{:x}", md5::compute(sign_input.as_bytes()));

        // 添加签名参数
        params.push(("w_rid".to_string(), w_rid));

        // 重新构建 URL
        let new_query: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        url.set_query(Some(&new_query));

        Ok(())
    }
}

/// 全局 WBI 密钥缓存
struct WbiCache {
    keys: Option<WbiKeys>,
    last_update: Option<Instant>,
}

static WBI_CACHE: RwLock<WbiCache> = RwLock::new(WbiCache {
    keys: None,
    last_update: None,
});

/// 导航 API 响应
#[derive(Debug, Deserialize)]
struct NavResponse {
    code: i32,
    message: String,
    data: NavData,
}

#[derive(Debug, Deserialize)]
struct NavData {
    wbi_img: WbiImg,
}

#[derive(Debug, Deserialize)]
struct WbiImg {
    img_url: String,
    sub_url: String,
}

/// 从 URL 中提取密钥
fn extract_key_from_url(url: &str) -> String {
    url.rsplit('/')
        .next()
        .and_then(|s| s.strip_suffix(".png"))
        .unwrap_or("")
        .to_string()
}

/// 获取 WBI 密钥（带缓存）
pub async fn get_wbi_keys(client: &Client) -> Result<WbiKeys> {
    // 检查缓存
    {
        let cache = WBI_CACHE.read().unwrap();
        if let (Some(keys), Some(last_update)) = (&cache.keys, cache.last_update) {
            if last_update.elapsed() < WBI_CACHE_TTL {
                return Ok(keys.clone());
            }
        }
    }

    // 缓存过期或不存在，重新获取
    let keys = fetch_wbi_keys(client).await?;

    // 更新缓存
    {
        let mut cache = WBI_CACHE.write().unwrap();
        cache.keys = Some(keys.clone());
        cache.last_update = Some(Instant::now());
    }

    Ok(keys)
}

/// 从 API 获取 WBI 密钥
async fn fetch_wbi_keys(client: &Client) -> Result<WbiKeys> {
    let resp: NavResponse = client
        .get("https://api.bilibili.com/x/web-interface/nav")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .send()
        .await?
        .json()
        .await?;

    // -101 是未登录，但仍然会返回 WBI 密钥
    if resp.code != 0 && resp.code != -101 {
        return Err(Error::Api {
            code: resp.code,
            message: resp.message,
        });
    }

    let img_key = extract_key_from_url(&resp.data.wbi_img.img_url);
    let sub_key = extract_key_from_url(&resp.data.wbi_img.sub_url);

    if img_key.is_empty() || sub_key.is_empty() {
        return Err(Error::Api {
            code: -1,
            message: "Failed to extract WBI keys".to_string(),
        });
    }

    let mixin_key = WbiKeys::compute_mixin_key(&img_key, &sub_key);

    Ok(WbiKeys {
        img_key,
        sub_key,
        mixin_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixin_key() {
        // 测试混淆密钥计算
        let img_key = "7cd084941338484aae1ad9425b84077c";
        let sub_key = "4932caff0ff746eab6f01bf08b70ac45";
        let mixin = WbiKeys::compute_mixin_key(img_key, sub_key);

        assert_eq!(mixin.len(), 32);
    }

    #[test]
    fn test_extract_key() {
        let url = "https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png";
        let key = extract_key_from_url(url);
        assert_eq!(key, "7cd084941338484aae1ad9425b84077c");
    }
}
