//! 窗口状态模块
//! 定义窗口状态结构和相关操作

use serde::{Deserialize, Serialize};

/// 窗口状态
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub is_open: bool,
}

impl WindowState {
    /// 检查窗口状态是否有效（宽高大于100像素）
    pub fn is_valid(&self) -> bool {
        self.width > 100 && self.height > 100
    }

    /// 检查位置是否有效（非默认位置）
    pub fn has_valid_position(&self) -> bool {
        self.x != 0 || self.y != 0
    }
}

/// 窗口配置
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub label: String,
    pub title: String,
    pub url: String,
    pub default_width: f64,
    pub default_height: f64,
    pub min_width: f64,
    pub min_height: f64,
    pub transparent: bool,
    pub decorations: bool,
    pub always_on_top: bool,
    pub resizable: bool,
    pub shadow: bool,
}

impl WindowConfig {
    /// 创建 Tab 窗口配置
    pub fn tab(label: &str, title: &str, tab_type: &str) -> Self {
        Self {
            label: label.to_string(),
            title: title.to_string(),
            url: format!("/#/tab/{}", tab_type),
            default_width: 350.0,
            default_height: 500.0,
            min_width: 250.0,
            min_height: 150.0,
            transparent: true,
            decorations: false,
            always_on_top: true,
            resizable: true,
            shadow: false,
        }
    }

    /// 创建设置窗口配置
    pub fn settings() -> Self {
        Self {
            label: "settings".to_string(),
            title: "设置".to_string(),
            url: "/#/settings".into(),
            default_width: 400.0,
            default_height: 550.0,
            min_width: 350.0,
            min_height: 400.0,
            transparent: true,
            decorations: false,
            always_on_top: true,
            resizable: true,
            shadow: false,
        }
    }

    /// 创建存档窗口配置
    pub fn archive() -> Self {
        Self {
            label: "archive".to_string(),
            title: "存档".to_string(),
            url: "/#/archive".into(),
            default_width: 700.0,
            default_height: 550.0,
            min_width: 550.0,
            min_height: 400.0,
            transparent: true,
            decorations: false,
            always_on_top: false,
            resizable: true,
            shadow: false,
        }
    }
}
