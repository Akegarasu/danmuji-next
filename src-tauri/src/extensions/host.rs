//! 扩展注册与生命周期，不持有直播聚合器或桌面窗口。
use super::{
    overtime, storage::CheckpointStore, video_request, voting, EffectResult, Extension,
    ExtensionEffect, ExtensionState,
};
use crate::live_events::{ReceivedGift, ReceivedText};
use serde_json::json;
use serde_json::Value;
use std::time::Instant;
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};
use tokio::sync::watch;

struct Entry {
    plugin: Box<dyn Extension>,
    updates: watch::Sender<Value>,
    store: CheckpointStore,
    revision: u64,
    dirty: bool,
}

impl Entry {
    fn broadcast(&self, now: Instant) {
        // OBS 无订阅时不生成额外快照；桌面更新每个推送周期合并一次。
        if self.updates.receiver_count() > 0 {
            self.updates.send_replace(self.plugin.snapshot(now));
        }
    }
}

pub struct ExtensionHost {
    entries: Mutex<BTreeMap<String, Entry>>,
    last_error: Mutex<Option<String>>,
}

impl ExtensionHost {
    /// 所有内置扩展统一存入 extensions/{id}.json。
    pub fn new(directory: PathBuf) -> Self {
        let host = Self {
            entries: Mutex::new(BTreeMap::new()),
            last_error: Mutex::new(None),
        };
        let directory = directory.join("extensions");
        for plugin in [
            Box::new(overtime::Overtime::new(Instant::now())) as Box<dyn Extension>,
            Box::new(video_request::VideoRequestManager::default()),
            Box::new(voting::VotingManager::default()),
        ] {
            let store = CheckpointStore::new(directory.join(format!("{}.json", plugin.id())));
            host.register(plugin, store);
        }
        host
    }
    fn register(&self, mut plugin: Box<dyn Extension>, mut store: CheckpointStore) {
        let loaded = store.load().and_then(|value| {
            if let Some(value) = value {
                plugin.restore(value, Instant::now())
            } else {
                Ok(())
            }
        });
        if let Err(error) = loaded {
            store.mark_invalid();
            self.report(format!("读取 {} 配置失败：{error}", plugin.id()));
        }
        let (updates, _) = watch::channel(plugin.snapshot(Instant::now()));
        self.entries.lock().unwrap().insert(
            plugin.id().into(),
            Entry {
                plugin,
                updates,
                store,
                revision: 0,
                dirty: false,
            },
        );
    }
    fn report(&self, error: String) {
        log::error!("[Extensions] {error}");
        *self.last_error.lock().unwrap() = Some(error);
    }
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }
    pub fn state(&self, id: &str) -> Result<ExtensionState, String> {
        let entries = self.entries.lock().unwrap();
        let entry = entries.get(id).ok_or("扩展不存在")?;
        Ok(ExtensionState {
            extension_id: id.into(),
            revision: entry.revision,
            state: entry.plugin.snapshot(Instant::now()),
        })
    }
    /// 合并同一周期内的多次变化，不为每条弹幕复制完整扩展状态。
    pub fn take_changes(&self) -> Vec<ExtensionState> {
        let mut entries = self.entries.lock().unwrap();
        let now = Instant::now();
        entries
            .iter_mut()
            .filter_map(|(id, entry)| {
                if !entry.dirty {
                    return None;
                }
                entry.dirty = false;
                Some(ExtensionState {
                    extension_id: id.clone(),
                    revision: entry.revision,
                    state: entry.plugin.snapshot(now),
                })
            })
            .collect()
    }
    pub fn query(&self, id: &str, query: Value) -> Result<Value, String> {
        let entries = self.entries.lock().unwrap();
        entries.get(id).ok_or("扩展不存在")?.plugin.query(query)
    }
    pub fn browser_snapshot(&self, id: &str) -> Result<Value, String> {
        let entries = self.entries.lock().unwrap();
        let entry = entries
            .get(id)
            .filter(|e| e.plugin.browser_visible())
            .ok_or("扩展不存在")?;
        Ok(entry.plugin.snapshot(Instant::now()))
    }
    pub fn subscribe(&self, id: &str) -> Result<watch::Receiver<Value>, String> {
        let entries = self.entries.lock().unwrap();
        let entry = entries
            .get(id)
            .filter(|e| e.plugin.browser_visible())
            .ok_or("扩展不存在")?;
        entry
            .updates
            .send_replace(entry.plugin.snapshot(Instant::now()));
        Ok(entry.updates.subscribe())
    }
    fn publish(&self, entry: &mut Entry, now: Instant, save: bool) {
        entry.revision += 1;
        entry.dirty = true;
        entry.broadcast(now);
        if save {
            if let Err(error) = entry.store.save(entry.plugin.checkpoint(now)) {
                self.report(error);
            }
        }
    }
    pub fn request(&self, id: &str, request: Value) -> Result<Value, String> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get_mut(id).ok_or("扩展不存在")?;
        let now = Instant::now();
        entry.store.prepare_request()?;
        let response = entry.plugin.request(request, now)?;
        entry.store.accept_request();
        entry.revision += 1;
        entry.dirty = true;
        entry.broadcast(now);
        if let Err(error) = entry.store.save(entry.plugin.checkpoint(now)) {
            let message = format!("操作已生效，但扩展状态保存失败：{error}");
            self.report(message.clone());
            return Err(message);
        }
        *self.last_error.lock().unwrap() = None;
        Ok(response)
    }
    pub fn dispatch_gift(&self, gift: &ReceivedGift) {
        let mut entries = self.entries.lock().unwrap();
        let now = Instant::now();
        for entry in entries.values_mut() {
            if entry.plugin.on_gift(gift, now) {
                self.publish(entry, now, true);
            }
        }
    }
    pub fn dispatch_text(&self, text: &ReceivedText) {
        let mut entries = self.entries.lock().unwrap();
        let now = Instant::now();
        for entry in entries.values_mut() {
            if entry.plugin.on_text(text, now) {
                self.publish(entry, now, true);
            }
        }
    }
    /// 独立于直播连接；静态扩展仅在状态变化时发布并落盘。
    pub fn tick(&self, save: bool) {
        let mut entries = self.entries.lock().unwrap();
        let now = Instant::now();
        for entry in entries.values_mut() {
            let changed = entry.plugin.tick(now);
            if changed || entry.plugin.browser_visible() {
                self.publish(entry, now, save || changed);
            } else if save {
                if let Err(error) = entry.store.save(entry.plugin.checkpoint(now)) {
                    self.report(error);
                }
            }
        }
    }
    pub fn take_effects(&self) -> Vec<(String, ExtensionEffect)> {
        self.entries
            .lock()
            .unwrap()
            .iter_mut()
            .flat_map(|(id, entry)| {
                entry
                    .plugin
                    .take_effects()
                    .into_iter()
                    .map(|effect| (id.clone(), effect))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
    pub fn complete_effect(&self, id: &str, result: EffectResult) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get_mut(id) {
            if entry.plugin.complete_effect(result) {
                self.publish(entry, Instant::now(), true);
            }
        }
    }
    pub fn catalog(&self) -> Value {
        let entries = self.entries.lock().unwrap();
        json!({"protocol_version": 1, "extensions": entries.iter().filter(|(_, e)| e.plugin.browser_visible()).map(|(id, _)| id).collect::<Vec<_>>()})
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
        let state = restored.state("overtime").unwrap().state;
        assert_eq!(state["running"], false);
        assert_eq!(state["config"]["initial_seconds"], 120.0);
        assert!(state["remaining_ms"].as_f64().unwrap() > 119_000.0);
        let path = directory.join("extensions/overtime.json");
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
        let backups: Vec<_> = std::fs::read_dir(directory.join("extensions"))
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
