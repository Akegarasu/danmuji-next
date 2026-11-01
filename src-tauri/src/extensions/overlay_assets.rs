//! 资源表只保存元数据；正文由前端资源包或开发目录提供，不在 Rust 中重复嵌入。
use axum::{
    body::Bytes,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc};
#[cfg(any(dev, test))]
use std::{path::PathBuf, sync::Mutex, time::SystemTime};

struct AssetSpec {
    path: &'static str,
    file: &'static str,
    mime: &'static str,
}

macro_rules! asset {
    ($path:literal, $file:literal, $mime:literal) => {
        AssetSpec {
            path: concat!("/overlays/", $path),
            file: $file,
            mime: $mime,
        }
    };
}

static ASSETS: &[AssetSpec] = &[
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
const WORKER_FILE: &str = "sw.js";

struct Snapshot {
    files: BTreeMap<&'static str, Bytes>,
    version: String,
    worker: Bytes,
}

impl Snapshot {
    fn load(mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Result<Self, String> {
        let mut files = BTreeMap::new();
        // FNV-1a 仅用作缓存内容标识，不承担签名或安全校验。
        let mut hash = 0xcbf29ce484222325u64;
        let mut hash_bytes = |bytes: &[u8]| {
            for byte in bytes {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
            }
        };
        for spec in ASSETS {
            let bytes = read(spec.file)
                .ok_or_else(|| format!("无法读取扩展资源：overlays/{}", spec.file))?;
            hash_bytes(spec.path.as_bytes());
            hash_bytes(spec.mime.as_bytes());
            hash_bytes(&bytes);
            files.insert(spec.path, Bytes::from(bytes));
        }
        let worker = read(WORKER_FILE).ok_or("无法读取扩展资源：overlays/sw.js")?;
        hash_bytes(&worker);
        let worker = std::str::from_utf8(&worker).map_err(|_| "扩展离线脚本编码无效")?;
        let version = format!("{hash:016x}");
        let paths: Vec<_> = ASSETS.iter().map(|asset| asset.path).collect();
        let worker = format!(
            "const VERSION = {};\nconst ASSETS = {};\n{worker}",
            serde_json::to_string(&version).unwrap(),
            serde_json::to_string(&paths).unwrap(),
        )
        .into();
        Ok(Self {
            files,
            version,
            worker,
        })
    }
}

enum Source {
    #[cfg(any(not(dev), test))]
    Bundled(Result<Arc<Snapshot>, String>),
    #[cfg(any(dev, test))]
    Directory {
        root: PathBuf,
        cache: Mutex<Option<DirectoryCache>>,
    },
}

#[cfg(any(dev, test))]
struct DirectoryCache {
    stamps: Vec<(u64, SystemTime)>,
    snapshot: Arc<Snapshot>,
}

pub(crate) struct OverlayAssets {
    source: Source,
}

impl OverlayAssets {
    // 启动时只解压一次，HTTP 响应通过 Bytes 共享正文，避免每次请求复制整个字体。
    #[cfg(any(not(dev), test))]
    pub(crate) fn bundled(read: impl FnMut(&str) -> Option<Vec<u8>>) -> Self {
        Self {
            source: Source::Bundled(Snapshot::load(read).map(Arc::new)),
        }
    }

    #[cfg(any(dev, test))]
    pub(crate) fn directory(root: PathBuf) -> Self {
        Self {
            source: Source::Directory {
                root,
                cache: Mutex::new(None),
            },
        }
    }

    fn snapshot(&self) -> Result<Arc<Snapshot>, String> {
        match &self.source {
            #[cfg(any(not(dev), test))]
            Source::Bundled(snapshot) => snapshot.clone(),
            #[cfg(any(dev, test))]
            Source::Directory { root, cache } => {
                let mut cache = cache.lock().map_err(|_| "扩展资源缓存不可用")?;
                let stamps: Vec<_> = ASSETS
                    .iter()
                    .map(|asset| asset.file)
                    .chain(std::iter::once(WORKER_FILE))
                    .map(|file| {
                        let metadata = std::fs::metadata(root.join(file))
                            .map_err(|_| format!("无法读取扩展资源：overlays/{file}"))?;
                        Ok((
                            metadata.len(),
                            metadata.modified().map_err(|e| e.to_string())?,
                        ))
                    })
                    .collect::<Result<_, String>>()?;
                if let Some(cached) = cache.as_ref().filter(|cached| cached.stamps == stamps) {
                    return Ok(cached.snapshot.clone());
                }
                // 修改源文件后生成完整快照；下载跨越两个版本时由 Worker 拒绝激活。
                let snapshot =
                    Arc::new(Snapshot::load(|file| std::fs::read(root.join(file)).ok())?);
                *cache = Some(DirectoryCache {
                    stamps,
                    snapshot: snapshot.clone(),
                });
                Ok(snapshot)
            }
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        self.snapshot().map(|_| ())
    }

    pub(super) fn asset(&self, id: &str, file: &str) -> Response {
        let path = if file == "index.html" {
            format!("/overlays/{id}/")
        } else {
            format!("/overlays/{id}/{file}")
        };
        // 只允许资源表内的精确路径，未知路径不能回退到应用首页。
        let Some(spec) = ASSETS.iter().find(|asset| asset.path == path) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let snapshot = match self.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => return unavailable(error),
        };
        (
            [
                (header::CONTENT_TYPE.as_str(), spec.mime),
                (header::CACHE_CONTROL.as_str(), "no-cache"),
                ("x-danmuji-overlay-version", snapshot.version.as_str()),
            ],
            snapshot.files[spec.path].clone(),
        )
            .into_response()
    }

    pub(super) fn worker(&self) -> Response {
        let snapshot = match self.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => return unavailable(error),
        };
        (
            [
                (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            snapshot.worker.clone(),
        )
            .into_response()
    }
}

fn unavailable(error: String) -> Response {
    log::error!("{error}");
    (StatusCode::SERVICE_UNAVAILABLE, "扩展资源暂时不可用").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, fs, path::Path, time::UNIX_EPOCH};

    fn source_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../public/overlays")
    }

    fn fixture_files() -> BTreeMap<String, Vec<u8>> {
        ASSETS
            .iter()
            .map(|asset| (asset.file.to_string(), asset.file.as_bytes().to_vec()))
            .chain(std::iter::once((
                WORKER_FILE.into(),
                b"/* worker */".to_vec(),
            )))
            .collect()
    }

    #[test]
    fn manifest_covers_all_frontend_overlay_files() {
        fn collect(root: &Path, dir: &Path, files: &mut BTreeSet<String>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    collect(root, &path, files);
                } else {
                    files.insert(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        let mut actual = BTreeSet::new();
        collect(&source_root(), &source_root(), &mut actual);
        let expected = ASSETS
            .iter()
            .map(|asset| asset.file.to_string())
            .chain(std::iter::once(WORKER_FILE.to_string()))
            .collect();
        assert_eq!(
            actual, expected,
            "新增前端资源必须同时注册 HTTP/离线缓存清单"
        );
        assert_eq!(
            ASSETS
                .iter()
                .map(|asset| asset.path)
                .collect::<BTreeSet<_>>()
                .len(),
            ASSETS.len()
        );
    }

    #[tokio::test]
    async fn bundled_resources_are_loaded_once_and_unknown_paths_do_not_fall_back() {
        let mut reads = 0;
        let files = fixture_files();
        let assets = OverlayAssets::bundled(|file| {
            reads += 1;
            files.get(file).cloned()
        });
        assert_eq!(reads, ASSETS.len() + 1);
        let first = assets.snapshot().unwrap();
        assert!(Arc::ptr_eq(&first, &assets.snapshot().unwrap()));
        let page = assets.asset("wish-machine", "index.html");
        assert_eq!(page.headers()["x-danmuji-overlay-version"], first.version);
        assert_eq!(
            axum::body::to_bytes(page.into_body(), usize::MAX)
                .await
                .unwrap(),
            "wish-machine/index.html"
        );
        for (id, file) in [
            ("wish-machine", "missing.css"),
            ("..", "index.html"),
            ("overtime", "../sw.js"),
        ] {
            assert_eq!(assets.asset(id, file).status(), StatusCode::NOT_FOUND);
        }
    }

    #[test]
    fn incomplete_bundle_fails_explicitly_without_serving_partial_cache() {
        let files = fixture_files();
        let assets = OverlayAssets::bundled(|file| {
            if file.ends_with(".otf") {
                None
            } else {
                files.get(file).cloned()
            }
        });
        assert!(assets.validate().unwrap_err().contains("timer-heavy.otf"));
        assert_eq!(assets.worker().status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            assets.asset("overtime", "index.html").status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn dev_source_changes_refresh_the_entire_snapshot_and_worker_version() {
        let root = std::env::temp_dir().join(format!(
            "danmuji-overlay-assets-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for (file, bytes) in fixture_files() {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let assets = OverlayAssets::directory(root.clone());
        let first = assets.snapshot().unwrap();
        assert!(Arc::ptr_eq(&first, &assets.snapshot().unwrap()));
        fs::write(
            root.join("overtime/overlay.css"),
            "/* edited without rebuilding Rust or dist */",
        )
        .unwrap();
        let second = assets.snapshot().unwrap();
        assert_ne!(first.version, second.version);
        assert_ne!(
            first.files["/overlays/overtime/overlay.css"],
            second.files["/overlays/overtime/overlay.css"]
        );
        fs::write(root.join(WORKER_FILE), "/* worker edit */").unwrap();
        let third = assets.snapshot().unwrap();
        assert_ne!(second.version, third.version);
        assert!(std::str::from_utf8(&third.worker)
            .unwrap()
            .contains(&third.version));
        let font_path = root.join("overtime/timer-heavy.otf");
        fs::remove_file(&font_path).unwrap();
        assert_eq!(assets.worker().status(), StatusCode::SERVICE_UNAVAILABLE);
        fs::write(font_path, b"restored font").unwrap();
        assert!(assets.validate().is_ok());
        fs::remove_dir_all(root).unwrap();
    }
}
