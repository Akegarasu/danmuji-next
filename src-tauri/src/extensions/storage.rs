//! 扩展 JSON 检查点存储，损坏文件仅在主动操作前备份。
use serde_json::Value;
use std::path::PathBuf;

pub(super) struct CheckpointStore {
    path: PathBuf,
    last_saved: Option<Value>,
    load_failed: bool,
}

impl CheckpointStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            last_saved: None,
            load_failed: false,
        }
    }
    pub fn load(&mut self) -> Result<Option<Value>, String> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&self.path).map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        self.last_saved = Some(value.clone());
        Ok(Some(value))
    }
    pub fn mark_invalid(&mut self) {
        self.load_failed = true;
    }
    pub fn prepare_request(&mut self) -> Result<(), String> {
        if self.load_failed {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            std::fs::copy(
                &self.path,
                self.path.with_extension(format!("json.invalid-{suffix}")),
            )
            .map_err(|e| format!("备份损坏配置失败：{e}"))?;
        }
        Ok(())
    }
    pub fn accept_request(&mut self) {
        if self.load_failed {
            self.load_failed = false;
            self.last_saved = None;
        }
    }
    pub fn save(&mut self, value: Value) -> Result<(), String> {
        if self.load_failed || self.last_saved.as_ref() == Some(&value) {
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(temporary, &self.path).map_err(|e| e.to_string())?;
        self.last_saved = Some(value);
        Ok(())
    }
}
