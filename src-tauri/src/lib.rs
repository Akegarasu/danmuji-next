//! 弹幕姬 Tauri 后端
//!
//! 模块结构：
//! - `config`: 配置目录和文件路径管理
//! - `kv_store`: 线程安全的键值存储
//! - `window_state`: 窗口状态定义和配置
//! - `commands`: Tauri 命令实现
//! - `blivedm`: Bilibili 直播弹幕协议库
//! - `blive_service`: 弹幕服务管理器
//! - `lock_state`: 窗口锁定状态管理

mod auth;
mod archive;
mod blive_service;
pub mod blivedm;
mod commands;
mod config;
mod crypto;
mod kv_store;
mod lock_state;
mod video_info;
mod window_state;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use blive_service::BliveService;
use config::{get_archive_db_path, get_window_kv_path};
use kv_store::KVStore;
use lock_state::LockStateManager;
use archive::ArchiveManager;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, RunEvent,
};

/// 全局 dev mode 标志，通过环境变量 DANMUJI_NEXT_DEV 启用
pub static DEV_MODE: AtomicBool = AtomicBool::new(false);

/// 检查是否处于 dev mode
pub fn is_dev_mode() -> bool {
    DEV_MODE.load(Ordering::Relaxed)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 检查 dev mode
    if std::env::var("DANMUJI_NEXT_DEV").is_ok() {
        DEV_MODE.store(true, Ordering::Relaxed);
        eprintln!("[DEV] DANMUJI_NEXT_DEV is set, dev mode enabled — events will be dumped to file");
    }

    // 初始化窗口 KV 存储
    let window_kv_store = KVStore::new(get_window_kv_path());

    // 初始化存档管理器
    let archive_manager = Arc::new(
        ArchiveManager::new(get_archive_db_path()).expect("初始化存档数据库失败"),
    );

    // 初始化弹幕服务
    let blive_service = Arc::new(BliveService::new());

    // 初始化窗口锁定状态管理器，并从 KV 存储加载保存的状态
    let lock_manager = LockStateManager::new();
    lock_manager.load_from_kv(&window_kv_store);

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .manage(window_kv_store)
        .manage(archive_manager)
        .manage(blive_service)
        .manage(lock_manager)
        .setup(|app| {
            // 恢复上次异常退出未关闭的存档会话
            let archive_for_recovery = app.state::<Arc<ArchiveManager>>().inner().clone();
            tauri::async_runtime::spawn(async move {
                match archive_for_recovery.recover_orphaned_sessions().await {
                    Ok(0) => {}
                    Ok(n) => log::info!("Recovered {} orphaned archive session(s)", n),
                    Err(e) => log::error!("Failed to recover orphaned sessions: {}", e),
                }
            });

            // 创建托盘菜单
            let unlock_all = MenuItem::with_id(app, "unlock_all", "解锁所有窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&unlock_all, &quit])?;

            // 创建系统托盘
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("弹幕姬")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "unlock_all" => {
                        // 获取锁定管理器和 KV 存储，解锁所有窗口
                        let kv_store = app.state::<KVStore>();
                        let lock_manager = app.state::<LockStateManager>();
                        let locked_windows = lock_manager.unlock_all(&kv_store);

                        // 解锁每个窗口并发送事件（使用窗口标签作为事件名后缀）
                        for label in locked_windows {
                            if let Some(window) = app.get_webview_window(&label) {
                                window.set_ignore_cursor_events(false).ok();
                                let event_name = format!("window-lock-change:{}", label);
                                window.emit(&event_name, false).ok();
                            }
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 窗口创建和管理
            commands::create_tab_window,
            commands::create_settings_window,
            commands::close_window,
            commands::set_always_on_top,
            commands::get_all_windows,
            commands::window_exists,
            commands::focus_window,
            // 窗口状态
            commands::get_saved_window_state,
            commands::save_window_state,
            commands::get_current_window_state,
            commands::set_window_state,
            commands::set_window_open_state,
            commands::get_previously_open_windows,
            // 配置文件
            commands::save_config,
            commands::load_config,
            // KV 存储
            commands::kv_get,
            commands::kv_set,
            commands::kv_remove,
            // 工具
            commands::open_url,
            commands::exit_app,
            // 弹幕服务
            commands::connect_room,
            commands::disconnect_room,
            commands::get_connection_status,
            commands::get_current_room_info,
            // 事件订阅
            commands::subscribe_events,
            commands::unsubscribe_events,
            commands::get_data_snapshot,
            // 贡献排行榜
            commands::refresh_contribution_rank,
            // 登录
            commands::generate_login_qrcode,
            commands::poll_login_status,
            commands::get_user_info,
            commands::validate_cookie,
            // 房管操作
            commands::add_silent_user,
            commands::get_shield_keyword_list,
            commands::add_shield_keyword,
            commands::del_shield_keyword,
            // 窗口锁定
            commands::lock_window,
            commands::unlock_window,
            commands::unlock_all_windows,
            commands::get_window_lock_state,
            commands::get_locked_windows,
            // 存档
            commands::get_archive_sessions,
            commands::get_archive_session_detail,
            commands::search_archive_danmaku,
            commands::search_archive_gifts,
            commands::search_archive_superchat,
            commands::delete_archive_session,
            commands::create_archive_window,
            // 扩展
            commands::create_extension_window,
            // 视频信息
            commands::fetch_video_info,
            commands::mark_video_watched,
            commands::remove_video_request,
            commands::clear_watched_videos,
            commands::clear_all_videos,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let RunEvent::Exit = event {
                // 应用退出时，确保存档会话正常关闭
                let service = app_handle.state::<Arc<BliveService>>().inner().clone();
                let archive = app_handle.state::<Arc<ArchiveManager>>().inner().clone();
                tauri::async_runtime::block_on(async move {
                    // 断开连接（会触发 archive end_session）
                    service.disconnect().await;
                    // 兜底：恢复可能残留的孤立会话
                    if let Err(e) = archive.recover_orphaned_sessions().await {
                        log::error!("Failed to recover sessions on exit: {}", e);
                    }
                });
            }
        });
}
