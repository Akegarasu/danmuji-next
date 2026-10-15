//! 同一份资源表用于 HTTP 输出、离线预缓存和内容版本计算。
use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::OnceLock;

struct Asset {
    path: &'static str,
    mime: &'static str,
    bytes: &'static [u8],
}

macro_rules! asset {
    ($path:literal, $file:literal, $mime:literal) => {
        Asset {
            path: concat!("/overlays/", $path),
            mime: $mime,
            bytes: include_bytes!(concat!("../../../public/overlays/", $file)),
        }
    };
}

const ASSETS: &[Asset] = &[
    asset!(
        "shared/style.js",
        "shared/style.js",
        "text/javascript; charset=utf-8"
    ),
    asset!(
        "shared/connection.js",
        "shared/connection.js",
        "text/javascript; charset=utf-8"
    ),
    asset!(
        "overtime/",
        "overtime/index.html",
        "text/html; charset=utf-8"
    ),
    asset!(
        "overtime/overlay.js",
        "overtime/overlay.js",
        "text/javascript; charset=utf-8"
    ),
    asset!(
        "overtime/overlay.css",
        "overtime/overlay.css",
        "text/css; charset=utf-8"
    ),
    asset!(
        "overtime/timer-heavy.otf",
        "overtime/timer-heavy.otf",
        "font/otf"
    ),
    asset!(
        "song-request/",
        "song-request/index.html",
        "text/html; charset=utf-8"
    ),
    asset!(
        "song-request/overlay.js",
        "song-request/overlay.js",
        "text/javascript; charset=utf-8"
    ),
    asset!(
        "song-request/overlay.css",
        "song-request/overlay.css",
        "text/css; charset=utf-8"
    ),
    asset!(
        "wish-machine/",
        "wish-machine/index.html",
        "text/html; charset=utf-8"
    ),
    asset!(
        "wish-machine/overlay.js",
        "wish-machine/overlay.js",
        "text/javascript; charset=utf-8"
    ),
    asset!(
        "wish-machine/overlay.css",
        "wish-machine/overlay.css",
        "text/css; charset=utf-8"
    ),
    asset!(
        "wish-machine/theme-test-a.css",
        "wish-machine/theme-test-a.css",
        "text/css; charset=utf-8"
    ),
];
const WORKER: &str = include_str!("../../../public/overlays/sw.js");

fn version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        // FNV-1a 仅用作缓存内容标识，不承担签名或安全校验。
        let mut hash = 0xcbf29ce484222325u64;
        for bytes in ASSETS
            .iter()
            .flat_map(|asset| [asset.path.as_bytes(), asset.mime.as_bytes(), asset.bytes])
            .chain(std::iter::once(WORKER.as_bytes()))
        {
            for byte in bytes {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
            }
        }
        format!("{hash:016x}")
    })
}

pub(super) fn asset(id: &str, file: &str) -> Response {
    let path = if file == "index.html" {
        format!("/overlays/{id}/")
    } else {
        format!("/overlays/{id}/{file}")
    };
    let Some(asset) = ASSETS.iter().find(|asset| asset.path == path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    (
        [
            (header::CONTENT_TYPE.as_str(), asset.mime),
            // HTTP 缓存需校验；离线页面只由已完整安装的 Service Worker 提供。
            (header::CACHE_CONTROL.as_str(), "no-cache"),
            ("x-danmuji-overlay-version", version()),
        ],
        asset.bytes,
    )
        .into_response()
}

pub(super) async fn worker() -> Response {
    let paths: Vec<_> = ASSETS.iter().map(|asset| asset.path).collect();
    let script = format!(
        "const VERSION = {};\nconst ASSETS = {};\n{WORKER}",
        serde_json::to_string(version()).unwrap(),
        serde_json::to_string(&paths).unwrap(),
    );
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        script,
    )
        .into_response()
}
