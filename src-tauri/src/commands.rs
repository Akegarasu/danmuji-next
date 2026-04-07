//! Tauri 命令模块
//! 所有暴露给前端的命令

use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::sync::Arc;
use tauri::{
    Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder,
};

use std::sync::Mutex;

use crate::auth::{self, QRCodeData, QRCodeStatus, UserInfo};
use crate::archive::{
    ArchiveManager, ArchiveSession, ArchivedDanmaku, ArchivedGift, ArchivedSuperChat, PagedResult,
};
use crate::blive_service::BliveService;
use crate::live_types::{
    ConnectResult, ConnectionStatus, DataSnapshot, EventType, RoomInfoResponse, VideoRequestItem,
};
use crate::blivedm;
use crate::config::get_config_path;
use crate::crypto;
use crate::kv_store::KVStore;
use crate::lock_state::LockStateManager;
use crate::video_info::{self, VideoInfo};
use crate::window_state::{WindowConfig, WindowState};

// ==================== 配置文件操作 ====================

/// Cookie 加密缓存：(明文, 密文)，避免每次保存都重复加解密
static COOKIE_CACHE: Mutex<Option<(String, String)>> = Mutex::new(None);

/// 查缓存：用 key 匹配元组某一端，返回另一端
fn cache_lookup(key: &str, match_plain: bool) -> Option<String> {
    let cache = COOKIE_CACHE.lock().unwrap();
    cache.as_ref().and_then(|(plain, enc)| {
        let (k, v) = if match_plain { (plain, enc) } else { (enc, plain) };
        (k == key).then(|| v.clone())
    })
}

/// 写入缓存
fn cache_store(plain: String, enc: String) {
    *COOKIE_CACHE.lock().unwrap() = Some((plain, enc));
}

/// 保存配置到文件（自动加密 cookie 字段）
#[tauri::command]
pub fn save_config(config: String) -> Result<(), String> {
    let path = get_config_path();

    let mut value: Value = serde_json::from_str(&config).map_err(|e| e.to_string())?;
    if let Some(cookie) = value.get("cookie").and_then(|v| v.as_str()) {
        if !cookie.is_empty() {
            let encrypted = cache_lookup(cookie, true).unwrap_or_else(|| {
                match crypto::encrypt_cookie(cookie) {
                    Ok(enc) => {
                        cache_store(cookie.to_string(), enc.clone());
                        enc
                    }
                    Err(e) => {
                        log::warn!("Cookie 加密失败，将以明文保存: {}", e);
                        cookie.to_string()
                    }
                }
            });
            value["cookie"] = Value::String(encrypted);
        }
    }

    let output = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(&path, output).map_err(|e| e.to_string())
}

/// 读取配置文件（自动解密 cookie 字段）
#[tauri::command]
pub fn load_config() -> Result<String, String> {
    let path = get_config_path();
    if !path.exists() {
        return Ok("{}".to_string());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut value: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    if let Some(cookie_enc) = value.get("cookie").and_then(|v| v.as_str()) {
        if !cookie_enc.is_empty() {
            let decrypted = cache_lookup(cookie_enc, false).unwrap_or_else(|| {
                match crypto::decrypt_cookie(cookie_enc) {
                    Ok(plain) => {
                        cache_store(plain.clone(), cookie_enc.to_string());
                        plain
                    }
                    Err(e) => {
                        log::warn!("Cookie 解密失败，将清空 cookie: {}", e);
                        String::new()
                    }
                }
            });
            value["cookie"] = Value::String(decrypted);
        }
    }

    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

// ==================== KV 存储操作 ====================

/// 从窗口 KV 存储读取值
#[tauri::command]
pub fn kv_get(kv_store: State<KVStore>, key: String) -> Result<Option<Value>, String> {
    Ok(kv_store.get(&key))
}

/// 写入值到窗口 KV 存储
#[tauri::command]
pub fn kv_set(kv_store: State<KVStore>, key: String, value: Value) -> Result<(), String> {
    kv_store.set(key, value)
}

/// 从窗口 KV 存储删除值
#[tauri::command]
pub fn kv_remove(kv_store: State<KVStore>, key: String) -> Result<(), String> {
    kv_store.remove(&key)
}

// ==================== 窗口状态操作 ====================

/// 生成窗口状态的存储键
fn window_state_key(label: &str) -> String {
    format!("window_state_{}", label)
}

/// 获取保存的窗口状态（从 KV 存储）
#[tauri::command]
pub fn get_saved_window_state(
    kv_store: State<KVStore>,
    label: String,
) -> Result<Option<WindowState>, String> {
    let key = window_state_key(&label);
    if let Some(value) = kv_store.get(&key) {
        serde_json::from_value(value)
            .map(Some)
            .map_err(|e| e.to_string())
    } else {
        Ok(None)
    }
}

/// 保存窗口状态（到 KV 存储）
#[tauri::command]
pub fn save_window_state(
    kv_store: State<KVStore>,
    label: String,
    state: WindowState,
) -> Result<(), String> {
    let key = window_state_key(&label);
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    kv_store.set(key, value)
}

/// 设置窗口的打开状态
#[tauri::command]
pub fn set_window_open_state(
    kv_store: State<KVStore>,
    label: String,
    is_open: bool,
) -> Result<(), String> {
    let key = window_state_key(&label);
    if let Some(value) = kv_store.get(&key) {
        if let Ok(mut state) = serde_json::from_value::<WindowState>(value) {
            state.is_open = is_open;
            let new_value = serde_json::to_value(state).map_err(|e| e.to_string())?;
            kv_store.set(key, new_value)?;
        }
    }
    Ok(())
}

/// 窗口信息（用于恢复窗口）
#[derive(serde::Serialize)]
pub struct WindowInfo {
    pub label: String,
    pub state: WindowState,
}

/// 获取所有之前打开的窗口（用于恢复）
#[tauri::command]
pub fn get_previously_open_windows(kv_store: State<KVStore>) -> Vec<WindowInfo> {
    let states = kv_store.get_by_prefix("window_state_");

    states
        .into_iter()
        .filter_map(|(key, value)| {
            let label = key.strip_prefix("window_state_")?.to_string();
            // 排除主窗口，主窗口不需要恢复
            if label == "main" {
                return None;
            }
            let state: WindowState = serde_json::from_value(value).ok()?;
            if state.is_open {
                Some(WindowInfo { label, state })
            } else {
                None
            }
        })
        .collect()
}

/// 获取当前窗口状态
#[tauri::command]
pub async fn get_current_window_state(
    app: tauri::AppHandle,
    label: String,
) -> Result<WindowState, String> {
    if let Some(window) = app.get_webview_window(&label) {
        let position = window.outer_position().map_err(|e| e.to_string())?;
        let size = window.outer_size().map_err(|e| e.to_string())?;

        Ok(WindowState {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            is_open: true,
        })
    } else {
        Err("Window not found".to_string())
    }
}

/// 设置窗口状态（位置和大小）
#[tauri::command]
pub async fn set_window_state(
    app: tauri::AppHandle,
    label: String,
    state: WindowState,
) -> Result<(), String> {
    if !state.is_valid() {
        return Ok(()); // 无效状态，跳过
    }

    if let Some(window) = app.get_webview_window(&label) {
        // 先设置位置再设置大小
        if state.has_valid_position() {
            window
                .set_position(PhysicalPosition::new(state.x, state.y))
                .map_err(|e| e.to_string())?;
        }
        window
            .set_size(PhysicalSize::new(state.width, state.height))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ==================== 窗口创建 ====================

/// 创建窗口的通用逻辑
fn build_window(
    app: &tauri::AppHandle,
    config: &WindowConfig,
    saved_state: Option<&WindowState>,
) -> Result<(), String> {
    // 确定窗口尺寸
    let (width, height) = saved_state
        .filter(|s| s.is_valid())
        .map(|s| (s.width as f64, s.height as f64))
        .unwrap_or((config.default_width, config.default_height));

    // 确定窗口位置
    let position = saved_state
        .filter(|s| s.is_valid() && s.has_valid_position())
        .map(|s| (s.x as f64, s.y as f64));

    let mut builder = WebviewWindowBuilder::new(
        app,
        &config.label,
        WebviewUrl::App(config.url.clone().into()),
    )
    .title(&config.title)
    .inner_size(width, height)
    .min_inner_size(config.min_width, config.min_height)
    .transparent(config.transparent)
    .decorations(config.decorations)
    .always_on_top(config.always_on_top)
    .resizable(config.resizable)
    .shadow(config.shadow);

    if let Some((x, y)) = position {
        builder = builder.position(x, y);
    }

    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}

/// 聚焦已存在的窗口
fn focus_existing_window(window: &tauri::WebviewWindow) {
    window.set_focus().ok();
    window.unminimize().ok();
}

/// 创建新的 Tab 窗口
#[tauri::command]
pub async fn create_tab_window(
    app: tauri::AppHandle,
    kv_store: State<'_, KVStore>,
    label: String,
    title: String,
    tab_type: String,
) -> Result<(), String> {
    // 检查窗口是否已存在
    if let Some(existing) = app.get_webview_window(&label) {
        focus_existing_window(&existing);
        return Ok(());
    }

    // 获取窗口配置
    let config = WindowConfig::tab(&label, &title, &tab_type);

    // 从 KV 存储读取保存的窗口状态
    let key = window_state_key(&label);
    let saved_state: Option<WindowState> = kv_store
        .get(&key)
        .and_then(|v| serde_json::from_value(v).ok());

    build_window(&app, &config, saved_state.as_ref())
}

/// 创建设置窗口
#[tauri::command]
pub async fn create_settings_window(
    app: tauri::AppHandle,
    kv_store: State<'_, KVStore>,
) -> Result<(), String> {
    let config = WindowConfig::settings();

    // 检查窗口是否已存在
    if let Some(existing) = app.get_webview_window(&config.label) {
        focus_existing_window(&existing);
        return Ok(());
    }

    // 从 KV 存储读取保存的窗口状态
    let key = window_state_key(&config.label);
    let saved_state: Option<WindowState> = kv_store
        .get(&key)
        .and_then(|v| serde_json::from_value(v).ok());

    build_window(&app, &config, saved_state.as_ref())
}

/// 关闭指定窗口
#[tauri::command]
pub async fn close_window(app: tauri::AppHandle, label: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 设置窗口置顶
#[tauri::command]
pub async fn set_always_on_top(
    app: tauri::AppHandle,
    label: String,
    on_top: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        window
            .set_always_on_top(on_top)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 获取所有窗口标签
#[tauri::command]
pub async fn get_all_windows(app: tauri::AppHandle) -> Vec<String> {
    app.webview_windows().keys().cloned().collect()
}

/// 检查窗口是否存在
#[tauri::command]
pub async fn window_exists(app: tauri::AppHandle, label: String) -> bool {
    app.get_webview_window(&label).is_some()
}

/// 聚焦窗口
#[tauri::command]
pub async fn focus_window(app: tauri::AppHandle, label: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        focus_existing_window(&window);
    }
    Ok(())
}

/// 打开外部链接
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}

/// 退出应用
#[tauri::command]
pub async fn exit_app(app: tauri::AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

// ==================== 弹幕服务操作 ====================

/// 连接到直播间
#[tauri::command]
pub async fn connect_room(
    app: tauri::AppHandle,
    blive_service: State<'_, Arc<BliveService>>,
    room_id: u64,
    cookie: Option<String>,
) -> Result<ConnectResult, String> {
    Ok(blive_service.connect(app, room_id, cookie).await)
}

/// 断开连接
#[tauri::command]
pub async fn disconnect_room(blive_service: State<'_, Arc<BliveService>>) -> Result<(), String> {
    blive_service.disconnect().await;
    Ok(())
}

/// 获取连接状态
#[tauri::command]
pub async fn get_connection_status(
    blive_service: State<'_, Arc<BliveService>>,
) -> Result<ConnectionStatus, String> {
    Ok(blive_service.get_status().await)
}

/// 获取当前房间信息
#[tauri::command]
pub async fn get_current_room_info(
    blive_service: State<'_, Arc<BliveService>>,
) -> Result<Option<RoomInfoResponse>, String> {
    Ok(blive_service.get_room_info().await)
}

/// 刷新贡献排行榜
#[tauri::command]
pub async fn refresh_contribution_rank(
    blive_service: State<'_, Arc<BliveService>>,
    cookie: String,
) -> Result<(), String> {
    blive_service.refresh_contribution_rank(&cookie).await?;
    Ok(())
}

// ==================== 事件订阅操作 ====================

/// 订阅事件（窗口注册感兴趣的事件类型）
#[tauri::command]
pub async fn subscribe_events(
    blive_service: State<'_, Arc<BliveService>>,
    window_label: String,
    event_types: Vec<EventType>,
) -> Result<(), String> {
    let types: HashSet<EventType> = event_types.into_iter().collect();
    blive_service.subscribe(window_label, types).await;
    Ok(())
}

/// 取消订阅（窗口关闭时调用）
#[tauri::command]
pub async fn unsubscribe_events(
    blive_service: State<'_, Arc<BliveService>>,
    window_label: String,
) -> Result<(), String> {
    blive_service.unsubscribe(&window_label).await;
    Ok(())
}

/// 获取数据快照（新窗口获取当前完整数据）
#[tauri::command]
pub async fn get_data_snapshot(
    blive_service: State<'_, Arc<BliveService>>,
    event_types: Vec<EventType>,
) -> Result<DataSnapshot, String> {
    let types: HashSet<EventType> = event_types.into_iter().collect();
    Ok(blive_service.get_snapshot(types).await)
}

// ==================== 登录操作 ====================

/// 生成登录二维码
#[tauri::command]
pub async fn generate_login_qrcode() -> Result<QRCodeData, String> {
    let client = reqwest::Client::new();
    auth::generate_qrcode(&client).await
}

/// 轮询扫码状态
#[tauri::command]
pub async fn poll_login_status(qrcode_key: String) -> Result<QRCodeStatus, String> {
    auth::poll_qrcode_status(&qrcode_key).await
}

/// 获取用户信息
#[tauri::command]
pub async fn get_user_info(cookie: String) -> Result<UserInfo, String> {
    let client = reqwest::Client::new();
    auth::get_user_info(&client, &cookie).await
}

/// 验证 Cookie 是否有效
#[tauri::command]
pub async fn validate_cookie(cookie: String) -> Result<bool, String> {
    let client = reqwest::Client::new();
    auth::validate_cookie(&client, &cookie).await
}

// ==================== 直播间管理操作 ====================

/// 禁言用户
///
/// B 站直播间房管/主播接口，用于禁言指定用户。
/// - `type`:  2 = 仅本场（hour=0），1 = 按小时禁言，-1 = 永久禁言
/// - `hour`:  禁言小时数（type=1 时有效）
/// - `msg`:   禁言原因（可选）
#[tauri::command]
pub async fn add_silent_user(
    room_id: u64,
    tuid: u64,
    cookie: String,
    r#type: i32,
    hour: i32,
    msg: Option<String>,
) -> Result<blivedm::api::SilentUserResponse, String> {
    match blivedm::api::add_silent_user(room_id, tuid, cookie, r#type, hour, msg).await {
        Ok(resp) => Ok(resp),
        Err(e) => Err(e.to_string()),
    }
}

/// 获取屏蔽关键词列表
#[tauri::command]
pub async fn get_shield_keyword_list(
    room_id: u64,
    cookie: String,
) -> Result<blivedm::api::ShieldKeywordListResponse, String> {
    blivedm::api::get_shield_keyword_list(room_id, cookie)
        .await
        .map_err(|e| e.to_string())
}

/// 添加屏蔽关键词
#[tauri::command]
pub async fn add_shield_keyword(
    room_id: u64,
    keyword: String,
    cookie: String,
) -> Result<blivedm::api::SilentUserResponse, String> {
    blivedm::api::add_shield_keyword(room_id, keyword, cookie)
        .await
        .map_err(|e| e.to_string())
}

/// 删除屏蔽关键词
#[tauri::command]
pub async fn del_shield_keyword(
    room_id: u64,
    keyword: String,
    cookie: String,
) -> Result<blivedm::api::SilentUserResponse, String> {
    blivedm::api::del_shield_keyword(room_id, keyword, cookie)
        .await
        .map_err(|e| e.to_string())
}

// ==================== 窗口锁定操作 ====================

/// 锁定窗口（启用鼠标穿透）
#[tauri::command]
pub async fn lock_window(
    app: tauri::AppHandle,
    kv_store: State<'_, KVStore>,
    lock_manager: State<'_, LockStateManager>,
    label: String,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        window
            .set_ignore_cursor_events(true)
            .map_err(|e| e.to_string())?;
        lock_manager.set_locked(&label, true, &kv_store);

        // 发送锁定状态变化事件给该窗口（使用窗口标签作为事件名后缀）
        let event_name = format!("window-lock-change:{}", label);
        window.emit(&event_name, true).ok();
    }
    Ok(())
}

/// 解锁窗口（禁用鼠标穿透）
#[tauri::command]
pub async fn unlock_window(
    app: tauri::AppHandle,
    kv_store: State<'_, KVStore>,
    lock_manager: State<'_, LockStateManager>,
    label: String,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        window
            .set_ignore_cursor_events(false)
            .map_err(|e| e.to_string())?;
        lock_manager.set_locked(&label, false, &kv_store);

        // 发送锁定状态变化事件给该窗口（使用窗口标签作为事件名后缀）
        let event_name = format!("window-lock-change:{}", label);
        window.emit(&event_name, false).ok();
    }
    Ok(())
}

/// 解锁所有窗口
#[tauri::command]
pub async fn unlock_all_windows(
    app: tauri::AppHandle,
    kv_store: State<'_, KVStore>,
    lock_manager: State<'_, LockStateManager>,
) -> Result<Vec<String>, String> {
    let locked_windows = lock_manager.unlock_all(&kv_store);

    for label in &locked_windows {
        if let Some(window) = app.get_webview_window(label) {
            window.set_ignore_cursor_events(false).ok();
            let event_name = format!("window-lock-change:{}", label);
            window.emit(&event_name, false).ok();
        }
    }

    Ok(locked_windows)
}

/// 获取窗口锁定状态
#[tauri::command]
pub async fn get_window_lock_state(
    lock_manager: State<'_, LockStateManager>,
    label: String,
) -> Result<bool, String> {
    Ok(lock_manager.is_locked(&label))
}

/// 获取所有锁定的窗口
#[tauri::command]
pub async fn get_locked_windows(
    lock_manager: State<'_, LockStateManager>,
) -> Result<Vec<String>, String> {
    Ok(lock_manager.get_locked_windows())
}

// ==================== 存档操作 ====================

/// 获取所有存档会话
#[tauri::command]
pub async fn get_archive_sessions(
    archive: State<'_, Arc<ArchiveManager>>,
) -> Result<Vec<ArchiveSession>, String> {
    archive.get_sessions().await
}

/// 获取存档会话详情
#[tauri::command]
pub async fn get_archive_session_detail(
    archive: State<'_, Arc<ArchiveManager>>,
    session_id: i64,
) -> Result<ArchiveSession, String> {
    archive.get_session_detail(session_id).await
}

/// 搜索存档弹幕
#[tauri::command]
pub async fn search_archive_danmaku(
    archive: State<'_, Arc<ArchiveManager>>,
    session_id: i64,
    query: String,
    page: u32,
    page_size: u32,
) -> Result<PagedResult<ArchivedDanmaku>, String> {
    archive
        .search_danmaku(session_id, &query, page, page_size)
        .await
}

/// 搜索存档礼物
#[tauri::command]
pub async fn search_archive_gifts(
    archive: State<'_, Arc<ArchiveManager>>,
    session_id: i64,
    query: String,
    min_price: Option<u64>,
    max_price: Option<u64>,
    page: u32,
    page_size: u32,
) -> Result<PagedResult<ArchivedGift>, String> {
    archive
        .search_gifts(session_id, &query, min_price, max_price, page, page_size)
        .await
}

/// 搜索存档 SC
#[tauri::command]
pub async fn search_archive_superchat(
    archive: State<'_, Arc<ArchiveManager>>,
    session_id: i64,
    query: String,
    min_price: Option<u64>,
    max_price: Option<u64>,
    page: u32,
    page_size: u32,
) -> Result<PagedResult<ArchivedSuperChat>, String> {
    archive
        .search_superchat(session_id, &query, min_price, max_price, page, page_size)
        .await
}

/// 删除存档会话
#[tauri::command]
pub async fn delete_archive_session(
    archive: State<'_, Arc<ArchiveManager>>,
    session_id: i64,
) -> Result<(), String> {
    archive.delete_session(session_id).await
}

/// 创建存档窗口
#[tauri::command]
pub async fn create_archive_window(
    app: tauri::AppHandle,
    kv_store: State<'_, KVStore>,
) -> Result<(), String> {
    let config = WindowConfig::archive();

    // 检查窗口是否已存在
    if let Some(existing) = app.get_webview_window(&config.label) {
        focus_existing_window(&existing);
        return Ok(());
    }

    // 从 KV 存储读取保存的窗口状态
    let key = window_state_key(&config.label);
    let saved_state: Option<WindowState> = kv_store
        .get(&key)
        .and_then(|v| serde_json::from_value(v).ok());

    build_window(&app, &config, saved_state.as_ref())
}

// ==================== 扩展窗口 ====================

/// 创建扩展窗口
#[tauri::command]
pub async fn create_extension_window(
    app: tauri::AppHandle,
    kv_store: State<'_, KVStore>,
) -> Result<(), String> {
    let config = WindowConfig::extension();

    // 检查窗口是否已存在
    if let Some(existing) = app.get_webview_window(&config.label) {
        focus_existing_window(&existing);
        return Ok(());
    }

    let key = window_state_key(&config.label);
    let saved_state: Option<WindowState> = kv_store
        .get(&key)
        .and_then(|v| serde_json::from_value(v).ok());

    build_window(&app, &config, saved_state.as_ref())
}

// ==================== 视频信息 ====================

/// 获取 Bilibili 视频信息
#[tauri::command]
pub async fn fetch_video_info(video_id: String) -> Result<VideoInfo, String> {
    video_info::fetch_video_info(&video_id).await
}

/// 加载持久化的点播数据
#[tauri::command]
pub async fn load_video_requests(
    service: State<'_, Arc<BliveService>>,
) -> Result<Vec<VideoRequestItem>, String> {
    service.load_video_requests().await;
    let snapshot = service.get_snapshot([EventType::VideoRequest].into()).await;
    Ok(snapshot.video_requests.unwrap_or_default())
}

/// 标记点播为已看/未看
#[tauri::command]
pub async fn mark_video_watched(
    service: State<'_, Arc<BliveService>>,
    request_id: String,
    watched: bool,
) -> Result<(), String> {
    service.mark_video_watched(&request_id, watched).await;
    Ok(())
}

/// 删除点播请求
#[tauri::command]
pub async fn remove_video_request(
    service: State<'_, Arc<BliveService>>,
    request_id: String,
) -> Result<(), String> {
    service.remove_video_request(&request_id).await;
    Ok(())
}

/// 清空已看的点播
#[tauri::command]
pub async fn clear_watched_videos(
    service: State<'_, Arc<BliveService>>,
) -> Result<(), String> {
    service.clear_watched_videos().await;
    Ok(())
}

/// 清空所有点播
#[tauri::command]
pub async fn clear_all_videos(
    service: State<'_, Arc<BliveService>>,
) -> Result<(), String> {
    service.clear_all_videos().await;
    Ok(())
}
