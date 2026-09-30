//! 礼物图片的素材读取与 PNG 保存。

use base64::{engine::general_purpose::STANDARD, Engine};
use std::time::Duration;

const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

fn asset_url(value: &str) -> Result<reqwest::Url, String> {
    let normalized = if value.starts_with("//") {
        format!("https:{value}")
    } else {
        value.to_owned()
    };
    let mut url = reqwest::Url::parse(&normalized).map_err(|_| "图片地址无效".to_string())?;
    let host = url.host_str().unwrap_or_default();
    if !matches!(url.scheme(), "http" | "https")
        || !(host == "hdslb.com"
            || host.ends_with(".hdslb.com")
            || host == "biliimg.com"
            || host.ends_with(".biliimg.com"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err("仅支持 B 站图片地址".into());
    }
    url.set_scheme("https")
        .map_err(|_| "图片地址无效".to_string())?;
    Ok(url)
}

#[tauri::command]
pub async fn load_gift_image_asset(url: String) -> Result<String, String> {
    let url = asset_url(&url)?;
    // 禁止跳转到任意站点；不携带用户 Cookie 或 Referer。
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if !matches!(
        mime.as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif" | "image/avif"
    ) {
        return Err("不支持的图片格式".into());
    }
    if response.content_length().unwrap_or(0) > MAX_IMAGE_BYTES as u64 {
        return Err("图片超过 8 MB".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
            return Err("图片超过 8 MB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

#[tauri::command]
pub async fn save_gift_image(
    window: tauri::WebviewWindow,
    png_base64: String,
    filename: String,
) -> Result<bool, String> {
    if png_base64.len() > MAX_IMAGE_BYTES * 4 / 3 + 4 {
        return Err("图片过大".into());
    }
    let bytes = STANDARD
        .decode(png_base64)
        .map_err(|_| "PNG 数据无效".to_string())?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("PNG 数据无效".into());
    }
    let filename: String = filename
        .chars()
        .take(120)
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let file = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_title("保存礼物图片")
        .set_file_name(if filename.is_empty() {
            "礼物图片.png"
        } else {
            &filename
        })
        .add_filter("PNG 图片", &["png"])
        .save_file()
        .await;
    let Some(file) = file else {
        return Ok(false);
    };
    tokio::fs::write(file.path(), bytes)
        .await
        .map_err(|e| format!("保存图片失败：{e}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::asset_url;

    #[test]
    fn accepts_only_bilibili_image_hosts() {
        for url in [
            "https://i0.hdslb.com/bfs/a.png",
            "//i0.hdslb.com/a.webp",
            "http://i0.biliimg.com/a.png",
        ] {
            assert_eq!(asset_url(url).unwrap().scheme(), "https");
        }
        for url in [
            "https://hdslb.com.evil.test/a",
            "https://evilhdslb.com/a",
            "file:///a",
            "https://127.0.0.1/a",
            "https://user@i0.hdslb.com/a",
            "https://i0.hdslb.com:8080/a",
        ] {
            assert!(asset_url(url).is_err(), "{url}");
        }
    }
}
