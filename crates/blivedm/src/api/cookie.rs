/// 从 Cookie 中提取 UID
pub fn extract_uid_from_cookie(cookie: &str) -> Option<u64> {
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

/// 从 Cookie 中提取指定字段值
pub(crate) fn extract_cookie_value(cookie: &str, name: &str) -> Option<String> {
    cookie
        .split(';')
        .map(|pair| pair.trim())
        .find(|pair| pair.starts_with(&format!("{}=", name)))
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| value.to_string())
}
