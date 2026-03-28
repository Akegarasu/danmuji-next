//! Cookie 加密模块
//!
//! 使用 Windows DPAPI (Data Protection API) 加密 cookie，
//! 类似 Chrome 的 cookie 加密机制。
//! - User Scope: 仅同一 Windows 用户可解密
//! - 存储格式: "DPAPI:<base64-ciphertext>"
//! - 兼容明文 cookie（无前缀时直接返回）

use base64::Engine;

const DPAPI_PREFIX: &str = "DPAPI:";

/// 加密 cookie 字符串
///
/// - 空字符串直接返回
/// - Windows: 使用 DPAPI User Scope 加密后 base64 编码，加前缀 "DPAPI:"
/// - 非 Windows: 直接返回原文
pub fn encrypt_cookie(plaintext: &str) -> Result<String, String> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }

    #[cfg(windows)]
    {
        use windows_dpapi::{encrypt_data, Scope};

        let encrypted =
            encrypt_data(plaintext.as_bytes(), Scope::User, None).map_err(|e| format!("DPAPI 加密失败: {}", e))?;

        let b64 = base64::engine::general_purpose::STANDARD.encode(&encrypted);
        Ok(format!("{}{}", DPAPI_PREFIX, b64))
    }

    #[cfg(not(windows))]
    {
        Ok(plaintext.to_string())
    }
}

/// 解密 cookie 字符串
///
/// - 空字符串直接返回
/// - 带 "DPAPI:" 前缀: 解码 base64 后用 DPAPI 解密
/// - 无前缀: 视为明文（兼容旧版本），直接返回
pub fn decrypt_cookie(stored: &str) -> Result<String, String> {
    if stored.is_empty() {
        return Ok(String::new());
    }

    if !stored.starts_with(DPAPI_PREFIX) {
        // 明文 cookie（旧版本或非 Windows 平台），直接返回
        return Ok(stored.to_string());
    }

    #[cfg(windows)]
    {
        use windows_dpapi::{decrypt_data, Scope};

        let b64_data = &stored[DPAPI_PREFIX.len()..];
        let encrypted = base64::engine::general_purpose::STANDARD
            .decode(b64_data)
            .map_err(|e| format!("Base64 解码失败: {}", e))?;

        let decrypted =
            decrypt_data(&encrypted, Scope::User, None).map_err(|e| format!("DPAPI 解密失败: {}", e))?;

        String::from_utf8(decrypted).map_err(|e| format!("UTF-8 解码失败: {}", e))
    }

    #[cfg(not(windows))]
    {
        // 非 Windows 平台遇到 DPAPI 前缀，无法解密
        Err("非 Windows 平台无法解密 DPAPI 数据".to_string())
    }
}
