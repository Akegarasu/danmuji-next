//! Windows 原生窗口置顶操作。

use tauri::{Runtime, WebviewWindow};

/// 通过 Win32 API 设置窗口的 TOPMOST 状态。
///
/// 该操作绕过 Tauri/Tao 的窗口状态缓存，并同步返回 `SetWindowPos` 的结果。
#[cfg(windows)]
pub fn set_topmost<R: Runtime>(window: &WebviewWindow<R>, enabled: bool) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    let hwnd = window
        .hwnd()
        .map_err(|e| format!("获取原生窗口句柄失败: {e}"))?;
    let insert_after = if enabled {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };

    unsafe {
        SetWindowPos(
            hwnd,
            Some(insert_after),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    }
    .map_err(|e| {
        format!(
            "SetWindowPos 设置 TOPMOST 失败: {e} (HRESULT: 0x{:08X})",
            e.code().0 as u32
        )
    })
}

/// Win32 置顶只在 Windows 上可用。
#[cfg(not(windows))]
pub fn set_topmost<R: Runtime>(_window: &WebviewWindow<R>, _enabled: bool) -> Result<(), String> {
    Err("Win32 窗口置顶操作仅支持 Windows".to_string())
}
