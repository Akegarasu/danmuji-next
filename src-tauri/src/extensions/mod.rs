//! 扩展宿主：领域插件、持久化、订阅分发与传输适配相互独立。
//! 内置扩展在此注册；新插件无需修改直播聚合状态或窗口订阅枚举。
pub mod overtime;
pub mod server;

use crate::live_types::ReceivedGift;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex, time::Instant};
use tokio::sync::watch;

pub trait Extension: Send {
    fn id(&self) -> &'static str;
    fn snapshot(&self, now: Instant) -> Value;
    fn checkpoint(&self, now: Instant) -> Value;
    fn restore(&mut self, value: Value, now: Instant) -> Result<(), String>;
    fn request(&mut self, request: Value, now: Instant) -> Result<(), String>;
    fn on_gift(&mut self, gift: &ReceivedGift, now: Instant) -> bool;
}

struct Entry {
    plugin: Box<dyn Extension>,
    updates: watch::Sender<Value>,
    load_failed: bool,
}

pub struct ExtensionHost {
    entries: Mutex<BTreeMap<String, Entry>>,
    directory: PathBuf,
    last_error: Mutex<Option<String>>,
}

impl ExtensionHost {
    pub fn new(directory: PathBuf) -> Self {
        let host = Self {
            entries: Mutex::new(BTreeMap::new()),
            directory,
            last_error: Mutex::new(None),
        };
        host.register(Box::new(overtime::Overtime::new(Instant::now())));
        host
    }

    fn register(&self, mut plugin: Box<dyn Extension>) {
        let path = self.path(plugin.id());
        let mut load_failed = false;
        if path.exists() {
            let loaded = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|data| serde_json::from_slice(&data).map_err(|e| e.to_string()))
                .and_then(|value| plugin.restore(value, Instant::now()));
            if let Err(error) = loaded {
                load_failed = true;
                self.report(format!("读取 {} 配置失败：{error}", plugin.id()));
            }
        }
        let (updates, _) = watch::channel(plugin.snapshot(Instant::now()));
        self.entries.lock().unwrap().insert(
            plugin.id().into(),
            Entry {
                plugin,
                updates,
                load_failed,
            },
        );
    }

    fn path(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.json"))
    }

    fn persist(&self, entry: &Entry, now: Instant) -> Result<(), String> {
        if entry.load_failed {
            return Ok(());
        }
        std::fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        let path = self.path(entry.plugin.id());
        let temporary = path.with_extension("json.tmp");
        let bytes =
            serde_json::to_vec_pretty(&entry.plugin.checkpoint(now)).map_err(|e| e.to_string())?;
        std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(temporary, path).map_err(|e| e.to_string())
    }

    fn report(&self, error: String) {
        log::error!("[Extensions] {error}");
        *self.last_error.lock().unwrap() = Some(error);
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    pub fn snapshot(&self, id: &str) -> Result<Value, String> {
        let entries = self.entries.lock().unwrap();
        let entry = entries.get(id).ok_or("扩展不存在")?;
        Ok(entry.plugin.snapshot(Instant::now()))
    }

    pub fn subscribe(&self, id: &str) -> Result<watch::Receiver<Value>, String> {
        let entries = self.entries.lock().unwrap();
        let entry = entries.get(id).ok_or("扩展不存在")?;
        entry
            .updates
            .send_replace(entry.plugin.snapshot(Instant::now()));
        Ok(entry.updates.subscribe())
    }

    pub fn request(&self, id: &str, request: Value) -> Result<Value, String> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get_mut(id).ok_or("扩展不存在")?;
        let now = Instant::now();
        // 用户主动修改前保留读入失败的文件；后台 tick 不覆盖损坏配置。
        if entry.load_failed {
            let path = self.path(id);
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            std::fs::copy(&path, path.with_extension(format!("json.invalid-{suffix}")))
                .map_err(|e| format!("备份损坏配置失败：{e}"))?;
            entry.load_failed = false;
        }
        entry.plugin.request(request, now)?;
        let snapshot = entry.plugin.snapshot(now);
        entry.updates.send_replace(snapshot.clone());
        if let Err(error) = self.persist(entry, now) {
            let message = format!("操作已生效，但扩展状态保存失败：{error}");
            self.report(message.clone());
            return Err(message);
        }
        *self.last_error.lock().unwrap() = None;
        Ok(snapshot)
    }

    /// 只接收已去重的单次礼物数量。各插件自行筛选订阅内容。
    pub fn dispatch_gift(&self, gift: &ReceivedGift) {
        let mut entries = self.entries.lock().unwrap();
        let now = Instant::now();
        for entry in entries.values_mut() {
            if entry.plugin.on_gift(gift, now) {
                entry.updates.send_replace(entry.plugin.snapshot(now));
                if let Err(error) = self.persist(entry, now) {
                    self.report(error);
                }
            }
        }
    }

    /// 由宿主独立驱动，关闭扩展窗口或 OBS 不会停止时钟。
    pub fn tick(&self, save: bool) {
        let entries = self.entries.lock().unwrap();
        let now = Instant::now();
        for entry in entries.values() {
            entry.updates.send_replace(entry.plugin.snapshot(now));
            if save {
                if let Err(error) = self.persist(entry, now) {
                    self.report(error);
                }
            }
        }
    }

    pub fn catalog(&self) -> Value {
        json!({"protocol_version": 1, "extensions": self.entries.lock().unwrap().keys().cloned().collect::<Vec<_>>()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoints_restore_and_invalid_configuration_is_preserved() {
        let directory = std::env::temp_dir().join(format!(
            "danmuji-extension-store-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let host = ExtensionHost::new(directory.clone());
        host.request(
            "overtime",
            json!({"type":"configure","config":{"enabled":true,"initial_seconds":120}}),
        )
        .unwrap();
        host.request("overtime", json!({"type":"reset"})).unwrap();
        host.request("overtime", json!({"type":"start"})).unwrap();
        host.tick(true);
        let restored = ExtensionHost::new(directory.clone());
        let state = restored.snapshot("overtime").unwrap();
        assert_eq!(state["running"], false);
        assert_eq!(state["config"]["initial_seconds"], 120.0);
        assert!(state["remaining_ms"].as_f64().unwrap() > 119_000.0);
        let path = directory.join("overtime.json");
        std::fs::write(&path, "broken configuration").unwrap();
        let broken = ExtensionHost::new(directory.clone());
        assert!(broken.last_error().is_some());
        broken.tick(true);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "broken configuration"
        );
        broken.request("overtime", json!({"type":"reset"})).unwrap();
        assert!(broken.last_error().is_none());
        let backups: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|file| file.file_name().to_string_lossy().contains("invalid-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            std::fs::read_to_string(backups[0].path()).unwrap(),
            "broken configuration"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
