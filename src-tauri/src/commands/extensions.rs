//! 扩展统一桌面协议：状态、操作、查询，与直播 IPC 独立。
use crate::extensions::{ExtensionHost, ExtensionState};
use serde_json::Value;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn get_extension_snapshot(
    host: State<'_, Arc<crate::extensions::ExtensionHost>>,
    extension_id: String,
) -> Result<ExtensionState, String> {
    host.state(&extension_id)
}

#[tauri::command]
pub fn extension_request(
    host: State<'_, Arc<crate::extensions::ExtensionHost>>,
    extension_id: String,
    request: Value,
) -> Result<Value, String> {
    host.request(&extension_id, request)
}

#[tauri::command]
pub fn extension_query(
    host: State<'_, Arc<ExtensionHost>>,
    extension_id: String,
    query: Value,
) -> Result<Value, String> {
    host.query(&extension_id, query)
}

#[tauri::command]
pub async fn get_overlay_server(
    server: State<'_, Arc<crate::extensions::server::OverlayServer>>,
    host: State<'_, Arc<crate::extensions::ExtensionHost>>,
) -> Result<crate::extensions::server::ServerInfo, String> {
    Ok(server.info(&host).await)
}

#[tauri::command]
pub async fn start_overlay_server(
    server: State<'_, Arc<crate::extensions::server::OverlayServer>>,
    host: State<'_, Arc<crate::extensions::ExtensionHost>>,
    port: u16,
) -> Result<crate::extensions::server::ServerInfo, String> {
    server.start(host.inner().clone(), Some(port)).await
}
