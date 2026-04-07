//! 配置管理模块
//! 处理应用配置目录和文件路径

use std::fs;
use std::path::PathBuf;

/// 获取配置目录
pub fn get_config_dir() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("danmuji-next");

    // 确保目录存在
    fs::create_dir_all(&config_dir).ok();

    config_dir
}

/// 获取主配置文件路径
pub fn get_config_path() -> PathBuf {
    get_config_dir().join("config.json")
}

/// 获取窗口 KV 存储文件路径
pub fn get_window_kv_path() -> PathBuf {
    get_config_dir().join("window_states.json")
}

/// 获取点播数据 KV 存储文件路径
pub fn get_video_request_kv_path() -> PathBuf {
    get_config_dir().join("video_requests.json")
}

/// 获取存档数据库文件路径
pub fn get_archive_db_path() -> PathBuf {
    get_config_dir().join("archives.db")
}
