//! 本地只读 OBS 服务。控制入口仅存在于 Tauri IPC 中。
use super::ExtensionHost;
use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Json, Router,
};
use futures_util::stream;
use serde::Serialize;
use serde_json::{json, Value};
use std::{convert::Infallible, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{watch, Mutex},
    task::JoinHandle,
};

#[derive(Clone)]
struct WebState {
    host: Arc<ExtensionHost>,
    shutdown: watch::Receiver<bool>,
    port: u16,
}

#[derive(Clone, Serialize)]
pub struct ServerInfo {
    pub port: u16,
    pub url: Option<String>,
    pub error: Option<String>,
    pub persistence_error: Option<String>,
}

struct RunningServer {
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

pub struct OverlayServer {
    info: Mutex<ServerInfo>,
    running: Mutex<Option<RunningServer>>,
    operation: Mutex<()>,
    settings_path: PathBuf,
}

impl OverlayServer {
    pub fn new(settings_path: PathBuf) -> Self {
        let port = std::fs::read(&settings_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|value| value["port"].as_u64())
            .filter(|port| (1024..=65535).contains(port))
            .unwrap_or(17654) as u16;
        Self {
            info: Mutex::new(ServerInfo {
                port,
                url: None,
                error: None,
                persistence_error: None,
            }),
            running: Mutex::new(None),
            operation: Mutex::new(()),
            settings_path,
        }
    }

    pub async fn info(&self, host: &ExtensionHost) -> ServerInfo {
        let mut info = self.info.lock().await.clone();
        info.persistence_error = host.last_error();
        info
    }

    pub async fn start(
        &self,
        host: Arc<ExtensionHost>,
        port: Option<u16>,
    ) -> Result<ServerInfo, String> {
        let _operation = self.operation.lock().await;
        let mut info = self.info.lock().await;
        let port = port.unwrap_or(info.port);
        if port < 1024 {
            return Err("端口必须在 1024 到 65535 之间".into());
        }
        if info.port == port && info.url.is_some() {
            info.error = None;
            return Ok(info.clone());
        }
        let listener = match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await {
            Ok(listener) => listener,
            Err(error) => {
                let message = format!(
                    "无法启动 OBS 服务（127.0.0.1:{port}）：{error}。请关闭占用程序或更换端口。"
                );
                info.error = Some(message.clone());
                return Err(message);
            }
        };
        if let Some(parent) = self.settings_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.settings_path, json!({"port":port}).to_string())
            .map_err(|e| e.to_string())?;
        let (shutdown, receiver) = watch::channel(false);
        let app = router(WebState {
            host,
            shutdown: receiver.clone(),
            port,
        });
        let task = tokio::spawn(async move {
            let mut receiver = receiver;
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = receiver.changed().await;
                })
                .await;
        });
        if let Some(mut previous) = self
            .running
            .lock()
            .await
            .replace(RunningServer { shutdown, task })
        {
            previous.shutdown.send_replace(true);
            if tokio::time::timeout(Duration::from_secs(2), &mut previous.task)
                .await
                .is_err()
            {
                previous.task.abort();
            }
        }
        info.port = port;
        info.url = Some(format!("http://127.0.0.1:{port}/overlays/overtime/"));
        info.error = None;
        Ok(info.clone())
    }

    pub async fn shutdown(&self) {
        let _operation = self.operation.lock().await;
        if let Some(mut running) = self.running.lock().await.take() {
            running.shutdown.send_replace(true);
            if tokio::time::timeout(Duration::from_secs(2), &mut running.task)
                .await
                .is_err()
            {
                running.task.abort();
            }
        }
        self.info.lock().await.url = None;
    }
}

fn router(state: WebState) -> Router {
    Router::new()
        .route("/api/extensions", get(catalog))
        .route("/api/extensions/{id}/state", get(snapshot))
        .route("/api/extensions/{id}/events", get(events))
        .route("/overlays/overtime/", get(|| async { asset("index.html") }))
        .route(
            "/overlays/overtime/{file}",
            get(|Path(file): Path<String>| async move { asset(&file) }),
        )
        .layer(middleware::from_fn_with_state(state.clone(), local_only))
        .with_state(state)
}

async fn local_only(State(state): State<WebState>, request: Request, next: Next) -> Response {
    let authority = format!("127.0.0.1:{}", state.port);
    let localhost = format!("localhost:{}", state.port);
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if (host != authority && host != localhost)
        || origin.is_some_and(|origin| {
            origin != format!("http://{authority}") && origin != format!("http://{localhost}")
        })
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; object-src 'none'".parse().unwrap());
    response
}

async fn catalog(State(state): State<WebState>) -> Json<Value> {
    Json(state.host.catalog())
}

async fn snapshot(State(state): State<WebState>, Path(id): Path<String>) -> Response {
    match state.host.snapshot(&id) {
        Ok(value) => Json(value).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn events(State(state): State<WebState>, Path(id): Path<String>) -> Response {
    let receiver = match state.host.subscribe(&id) {
        Ok(receiver) => receiver,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    // watch 保留最新完整状态，慢客户端和断线重连均不依赖丢失的增量。
    let stream = stream::unfold(
        (receiver, state.shutdown, true),
        |(mut receiver, mut shutdown, first)| async move {
            if *shutdown.borrow() {
                return None;
            }
            if !first {
                tokio::select! {
                    changed = receiver.changed() => { if changed.is_err() { return None; } }
                    _ = shutdown.changed() => return None,
                }
            }
            let value = receiver.borrow_and_update().clone();
            let event = Event::default().event("snapshot").data(value.to_string());
            Some((Ok::<_, Infallible>(event), (receiver, shutdown, false)))
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn asset(file: &str) -> Response {
    let (mime, bytes): (&str, &'static [u8]) = match file {
        "index.html" => (
            "text/html; charset=utf-8",
            include_bytes!("../../../public/overlays/overtime/index.html"),
        ),
        "overlay.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../../../public/overlays/overtime/overlay.js"),
        ),
        "overlay.css" => (
            "text/css; charset=utf-8",
            include_bytes!("../../../public/overlays/overtime/overlay.css"),
        ),
        "timer-heavy.otf" => (
            "font/otf",
            include_bytes!("../../../public/overlays/overtime/timer-heavy.otf"),
        ),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([(header::CONTENT_TYPE, mime)], bytes).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_types::ReceivedGift;
    #[tokio::test]
    async fn occupied_port_keeps_existing_service_and_shutdown_closes_streams() {
        let directory = std::env::temp_dir().join(format!(
            "danmuji-server-lifecycle-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let host = Arc::new(ExtensionHost::new(directory.clone()));
        let server = OverlayServer::new(directory.join("server.json"));
        let reservation = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let info = server.start(host.clone(), Some(port)).await.unwrap();
        assert!(info.url.unwrap().contains(&port.to_string()));
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let mut stream = client
            .get(format!(
                "http://127.0.0.1:{port}/api/extensions/overtime/events"
            ))
            .send()
            .await
            .unwrap();
        stream.chunk().await.unwrap().unwrap();
        let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
        assert!(server
            .start(host.clone(), Some(occupied.local_addr().unwrap().port()))
            .await
            .is_err());
        let info = server.info(&host).await;
        assert_eq!(info.port, port);
        assert!(info.url.is_some());
        assert!(info.error.is_some());
        assert!(server
            .start(host.clone(), Some(port))
            .await
            .unwrap()
            .error
            .is_none());
        server.shutdown().await;
        assert!(tokio::time::timeout(Duration::from_secs(2), stream.chunk())
            .await
            .unwrap()
            .unwrap()
            .is_none());
        assert!(server.info(&host).await.url.is_none());
        assert_eq!(
            OverlayServer::new(directory.join("server.json"))
                .info(&host)
                .await
                .port,
            port
        );
        assert!(TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .is_ok());
        drop(stream);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn http_assets_sse_filtering_and_read_only_boundary() {
        let directory = std::env::temp_dir().join(format!(
            "danmuji-overlay-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let host = Arc::new(ExtensionHost::new(directory.clone()));
        host.request("overtime", json!({"type":"configure","config":{
            "enabled":true,"initial_seconds":3600,"rules":[{"id":"heart","enabled":true,"gift_id":1,
            "gift_name":"小心心","action":"add","value":10,"per_gift":true}]
        }})).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (shutdown, receiver) = watch::channel(false);
        let app = router(WebState {
            host: host.clone(),
            shutdown: receiver,
            port,
        });
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let page = client
            .get(format!("{base}/overlays/overtime/"))
            .send()
            .await
            .unwrap();
        assert!(page.status().is_success());
        assert!(page.text().await.unwrap().contains("overlay.js"));
        for file in ["overlay.js", "overlay.css", "timer-heavy.otf"] {
            assert!(client
                .get(format!("{base}/overlays/overtime/{file}"))
                .send()
                .await
                .unwrap()
                .status()
                .is_success());
        }
        let endpoint = format!("{base}/api/extensions/overtime/events");
        let mut stream = client.get(&endpoint).send().await.unwrap();
        let first = stream.chunk().await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&first).contains("event: snapshot"));
        host.dispatch_gift(&ReceivedGift {
            gift_id: 2,
            gift_name: "无关礼物".into(),
            sender_name: "不应输出".into(),
            num: 1,
        });
        host.dispatch_gift(&ReceivedGift {
            gift_id: 1,
            gift_name: "小心心".into(),
            sender_name: "测试用户".into(),
            num: 2,
        });
        let update = tokio::time::timeout(Duration::from_secs(2), stream.chunk())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let update = String::from_utf8_lossy(&update);
        assert!(update.contains("3620000"));
        assert!(!update.contains("不应输出"));
        let mut reconnect = client.get(&endpoint).send().await.unwrap();
        assert!(
            String::from_utf8_lossy(&reconnect.chunk().await.unwrap().unwrap()).contains("3620000")
        );
        assert_eq!(
            client.post(&endpoint).send().await.unwrap().status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            client
                .get(&endpoint)
                .header("Host", "evil.example")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            client
                .get(&endpoint)
                .header("Origin", "https://evil.example")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            client
                .get(format!("{base}/api/extensions/missing/state"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        shutdown.send_replace(true);
        drop(stream);
        drop(reconnect);
        task.abort();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
