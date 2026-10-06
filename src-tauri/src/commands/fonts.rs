//! 本机字体列表；只返回字体名称，不读取或传输字体文件。
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SystemFont {
    pub family: String,
    pub label: String,
}

#[tauri::command]
pub async fn get_system_fonts() -> Result<Vec<SystemFont>, String> {
    tauri::async_runtime::spawn_blocking(enumerate_fonts)
        .await
        .map_err(|error| format!("读取本机字体失败：{error}"))?
}

#[cfg(not(windows))]
fn enumerate_fonts() -> Result<Vec<SystemFont>, String> {
    Err("当前平台暂不支持读取字体列表，请手动输入字体名称".into())
}

#[cfg(windows)]
fn enumerate_fonts() -> Result<Vec<SystemFont>, String> {
    use std::collections::BTreeMap;
    use windows::{
        core::{w, BOOL, PCWSTR},
        Win32::Graphics::DirectWrite::{
            DWriteCreateFactory, IDWriteFactory, IDWriteLocalizedStrings,
            DWRITE_FACTORY_TYPE_SHARED,
        },
    };

    // DirectWrite 字体族对应 CSS font-family；不把字重变体当作独立字体。
    fn localized_name(
        names: &IDWriteLocalizedStrings,
        locale: PCWSTR,
    ) -> windows::core::Result<String> {
        unsafe {
            let mut index = 0;
            let mut exists = BOOL::default();
            names.FindLocaleName(locale, &mut index, &mut exists)?;
            if !exists.as_bool() {
                names.FindLocaleName(w!("en-us"), &mut index, &mut exists)?;
            }
            if !exists.as_bool() {
                index = 0;
            }
            let length = names.GetStringLength(index)?;
            let mut buffer = vec![0u16; length as usize + 1];
            names.GetString(index, &mut buffer)?;
            Ok(String::from_utf16_lossy(&buffer[..length as usize]))
        }
    }

    let read = || -> windows::core::Result<Vec<SystemFont>> {
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let mut collection = None;
            factory.GetSystemFontCollection(&mut collection, true)?;
            let collection = collection.ok_or_else(windows::core::Error::from_win32)?;
            let mut fonts = BTreeMap::new();
            for index in 0..collection.GetFontFamilyCount() {
                let names = collection.GetFontFamily(index)?.GetFamilyNames()?;
                let family = localized_name(&names, w!("en-us"))?;
                let label = localized_name(&names, w!("zh-cn"))?;
                if family.trim().is_empty()
                    || family.starts_with('@')
                    || family.chars().count() > 100
                    || family.chars().any(char::is_control)
                {
                    continue;
                }
                fonts
                    .entry(family.to_lowercase())
                    .or_insert(SystemFont { family, label });
            }
            Ok(fonts.into_values().collect())
        }
    };
    read().map_err(|error| format!("读取本机字体失败：{error}"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn enumerates_installed_families_with_display_names_without_duplicates() {
        let fonts = enumerate_fonts().unwrap();
        assert!(!fonts.is_empty());
        let mut unique = std::collections::HashSet::new();
        for font in &fonts {
            assert!(!font.family.trim().is_empty());
            assert!(!font.label.trim().is_empty());
            assert!(unique.insert(font.family.to_lowercase()));
        }
        println!("读取到 {} 个本机字体族", fonts.len());
    }
}
