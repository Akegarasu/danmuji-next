//! 弹幕姬 Tauri 后端
//!
//! 模块结构：
//! - `config`: 配置目录和文件路径管理
//! - `kv_store`: 线程安全的键值存储
//! - `window_state`: 窗口状态定义和配置
//! - `commands`: Tauri 命令实现
//! - `blivedm`（外部 crate）: Bilibili 直播弹幕协议库
//! - `live_types`: 直播数据公共类型
//! - `live_data`: 直播数据状态管理
//! - `blive_service`: 弹幕服务管理器
//! - `lock_state`: 窗口锁定状态管理

mod archive;
mod archive_migrations;
mod auth;
mod blive_service;
mod commands;
mod config;
mod crypto;
mod extensions;
mod kv_store;
mod live_data;
mod live_events;
mod live_types;
mod lock_state;
mod raw_event_dump;
mod speech;
mod window_state;
mod window_topmost;

#[cfg(test)]
mod extension_pipeline_tests;
#[cfg(test)]
mod gift_pipeline_tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use archive::ArchiveManager;
use blive_service::BliveService;
use config::{get_archive_db_path, get_window_kv_path};
use kv_store::KVStore;
use lock_state::LockStateManager;
use speech::{SpeechRuntimeConfig, SpeechService};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, RunEvent,
};

/// 应用级任务统一退出，防止最后一次检查点之后仍发生异步回写。
#[derive(Default)]
struct BackgroundTasks(std::sync::Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>);
impl BackgroundTasks {
    fn push(&self, task: tauri::async_runtime::JoinHandle<()>) {
        self.0.lock().unwrap().push(task);
    }
    fn take(&self) -> Vec<tauri::async_runtime::JoinHandle<()>> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

/// 全局 dev mode 标志，通过环境变量 DANMUJI_NEXT_DEV 启用
pub static DEV_MODE: AtomicBool = AtomicBool::new(false);

/// 检查是否处于 dev mode
pub fn is_dev_mode() -> bool {
    DEV_MODE.load(Ordering::Relaxed)
}

fn should_reapply_always_on_top(label: &str) -> bool {
    matches!(label, "main" | "settings") || label.starts_with("tab-")
}

#[cfg(test)]
mod tests {
    use super::should_reapply_always_on_top;

    #[test]
    fn reapply_topmost_targets_only_always_on_top_windows() {
        for label in ["main", "settings", "tab-danmaku", "tab-gifts"] {
            assert!(should_reapply_always_on_top(label), "{label}");
        }

        for label in ["archive", "extension", "tab", ""] {
            assert!(!should_reapply_always_on_top(label), "{label}");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 检查 dev mode
    if std::env::var("DANMUJI_NEXT_DEV").is_ok() {
        DEV_MODE.store(true, Ordering::Relaxed);
        eprintln!(
            "[DEV] DANMUJI_NEXT_DEV is set, dev mode enabled — events will be dumped to file"
        );
    }

    // 初始化窗口 KV 存储
    let window_kv_store = KVStore::new(get_window_kv_path());

    // 初始化存档管理器
    let archive_manager =
        Arc::new(ArchiveManager::new(get_archive_db_path()).expect("初始化存档数据库失败"));

    // 初始化语音播报与弹幕服务（语音服务必须是全局单例，避免多窗口重复播报）
    let speech_service = Arc::new(SpeechService::new(SpeechRuntimeConfig::load_from_config()));
    let extensions = Arc::new(extensions::ExtensionHost::new(config::get_config_dir()));
    let overlay_server = Arc::new(extensions::server::OverlayServer::new(
        config::get_config_dir().join("extensions/server.json"),
    ));
    let blive_service = Arc::new(BliveService::new(
        speech_service.clone(),
        extensions.clone(),
    ));

    // 初始化窗口锁定状态管理器，并从 KV 存储加载保存的状态
    let lock_manager = LockStateManager::new();
    lock_manager.load_from_kv(&window_kv_store);

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(window_kv_store)
        .manage(archive_manager)
        .manage(blive_service)
        .manage(speech_service)
        .manage(extensions)
        .manage(overlay_server)
        .manage(lock_manager)
        .manage(BackgroundTasks::default())
        .setup(|app| {
            let extensions = app
                .state::<Arc<extensions::ExtensionHost>>()
                .inner()
                .clone();
            let overlay_server = app
                .state::<Arc<extensions::server::OverlayServer>>()
                .inner()
                .clone();
            let tasks = app.state::<BackgroundTasks>();
            let host_for_server = extensions.clone();
            tasks.push(tauri::async_runtime::spawn(async move {
                if let Err(error) = overlay_server.start(host_for_server, None).await {
                    log::error!("{error}");
                }
            }));
            tasks.push(tauri::async_runtime::spawn(extensions::tasks::run(
                extensions.clone(),
            )));
            let service = app.state::<Arc<BliveService>>().inner().clone();
            let app_for_push = app.handle().clone();
            tasks.push(tauri::async_runtime::spawn(async move {
                let mut push = tokio::time::interval(live_types::DATA_PUSH_INTERVAL);
                let mut clock = tokio::time::interval(std::time::Duration::from_secs(1));
                let mut checkpoint = tokio::time::interval(std::time::Duration::from_secs(5));
                for timer in [&mut push, &mut clock, &mut checkpoint] {
                    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                }
                loop {
                    tokio::select! {
                        _ = push.tick() => {
                            service.push_updates(&app_for_push).await;
                            for state in extensions.take_changes() {
                                let _ = app_for_push.emit(&format!("extension-state:{}", state.extension_id), &state);
                            }
                        },
                        _ = clock.tick() => extensions.tick(false),
                        _ = checkpoint.tick() => extensions.tick(true),
                    }
                }
            }));
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
            let reapply_topmost =
                MenuItem::with_id(app, "reapply_topmost", "重新顶置", true, None::<&str>)?;
            let unlock_all =
                MenuItem::with_id(app, "unlock_all", "解锁所有窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&reapply_topmost, &unlock_all, &quit])?;

            // 创建系统托盘
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("弹幕姬")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "reapply_topmost" => {
                        for (label, window) in app.webview_windows() {
                            if should_reapply_always_on_top(&label) {
                                if let Err(e) = window_topmost::set_topmost(&window, true) {
                                    log::warn!("Failed to reapply topmost for {}: {}", label, e);
                                }
                            }
                        }
                    }
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
            // 语音播报
            commands::get_speech_voices,
            commands::update_speech_settings,
            commands::preview_speech,
            commands::get_speech_status,
            // 手动测试
            commands::process_test_event,
            commands::get_raw_dump_status,
            commands::start_raw_dump,
            commands::stop_raw_dump,
            commands::open_raw_dump_directory,
            // 事件订阅
            commands::subscribe_events,
            commands::unsubscribe_events,
            commands::get_data_snapshot,
            // 贡献排行榜
            commands::refresh_contribution_rank,
            commands::get_gift_effect_config,
            commands::refresh_guard_top_list,
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
            commands::get_archive_overview,
            commands::get_archive_room_sessions,
            commands::get_archive_statistics,
            commands::search_archive,
            commands::get_archive_sessions,
            commands::get_archive_session_detail,
            commands::search_archive_danmaku,
            commands::lookup_archive_user_names,
            commands::search_archive_gifts,
            commands::search_archive_superchat,
            commands::prune_empty_archive_sessions,
            commands::delete_archive_session,
            commands::create_archive_window,
            // 扩展
            commands::create_extension_window,
            commands::extensions::get_extension_snapshot,
            commands::extensions::extension_request,
            commands::extensions::extension_query,
            commands::extensions::get_overlay_server,
            commands::extensions::start_overlay_server,
            // 版本和更新
            commands::get_app_version,
            commands::is_portable,
            commands::check_portable_update,
            commands::install_portable_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let RunEvent::Exit = event {
                // 应用退出时，确保存档会话正常关闭
                let service = app_handle.state::<Arc<BliveService>>().inner().clone();
                let archive = app_handle.state::<Arc<ArchiveManager>>().inner().clone();
                let speech = app_handle.state::<Arc<SpeechService>>().inner().clone();
                let extensions = app_handle
                    .state::<Arc<extensions::ExtensionHost>>()
                    .inner()
                    .clone();
                let overlay_server = app_handle
                    .state::<Arc<extensions::server::OverlayServer>>()
                    .inner()
                    .clone();
                let background_tasks = app_handle.state::<BackgroundTasks>().take();
                tauri::async_runtime::block_on(async move {
                    // 断开连接（会触发 archive end_session）
                    service.disconnect().await;
                    for task in &background_tasks {
                        task.abort();
                    }
                    for task in background_tasks {
                        let _ = task.await;
                    }
                    extensions.tick(true);
                    overlay_server.shutdown().await;
                    // 兜底：恢复可能残留的孤立会话
                    if let Err(e) = archive.recover_orphaned_sessions().await {
                        log::error!("Failed to recover sessions on exit: {}", e);
                    }
                });
                crate::raw_event_dump::stop();
                speech.shutdown();
            }
        });
}
