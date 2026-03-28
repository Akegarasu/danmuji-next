//! 窗口锁定状态管理
//! 
//! 管理各窗口的鼠标穿透（锁定）状态，支持持久化

use std::collections::HashMap;
use std::sync::Mutex;

use crate::kv_store::KVStore;

/// 锁定状态在 KV 存储中的键前缀
const LOCK_STATE_PREFIX: &str = "window_lock_";

/// 窗口锁定状态管理器
pub struct LockStateManager {
    /// 内存中的窗口锁定状态映射 (window_label -> is_locked)
    states: Mutex<HashMap<String, bool>>,
}

impl LockStateManager {
    pub fn new() -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
        }
    }

    /// 从 KV 存储加载锁定状态
    pub fn load_from_kv(&self, kv_store: &KVStore) {
        let saved_states = kv_store.get_by_prefix(LOCK_STATE_PREFIX);
        let mut states = self.states.lock().unwrap();
        
        for (key, value) in saved_states {
            if let Some(label) = key.strip_prefix(LOCK_STATE_PREFIX) {
                if let Some(locked) = value.as_bool() {
                    states.insert(label.to_string(), locked);
                }
            }
        }
    }

    /// 设置窗口锁定状态（同时保存到 KV 存储）
    pub fn set_locked(&self, label: &str, locked: bool, kv_store: &KVStore) {
        {
            let mut states = self.states.lock().unwrap();
            states.insert(label.to_string(), locked);
        }
        
        // 保存到 KV 存储
        let key = format!("{}{}", LOCK_STATE_PREFIX, label);
        kv_store.set(key, serde_json::Value::Bool(locked)).ok();
    }

    /// 获取窗口锁定状态
    pub fn is_locked(&self, label: &str) -> bool {
        let states = self.states.lock().unwrap();
        states.get(label).copied().unwrap_or(false)
    }

    /// 获取所有锁定的窗口
    pub fn get_locked_windows(&self) -> Vec<String> {
        let states = self.states.lock().unwrap();
        states
            .iter()
            .filter(|(_, &locked)| locked)
            .map(|(label, _)| label.clone())
            .collect()
    }

    /// 解锁所有窗口（同时保存到 KV 存储）
    pub fn unlock_all(&self, kv_store: &KVStore) -> Vec<String> {
        let locked: Vec<String>;
        {
            let mut states = self.states.lock().unwrap();
            locked = states
                .iter()
                .filter(|(_, &is_locked)| is_locked)
                .map(|(label, _)| label.clone())
                .collect();
            
            for label in &locked {
                states.insert(label.clone(), false);
            }
        }
        
        // 保存到 KV 存储
        for label in &locked {
            let key = format!("{}{}", LOCK_STATE_PREFIX, label);
            kv_store.set(key, serde_json::Value::Bool(false)).ok();
        }
        
        locked
    }

    /// 移除窗口状态（窗口关闭时调用）
    #[allow(dead_code)]
    pub fn remove(&self, label: &str, kv_store: &KVStore) {
        {
            let mut states = self.states.lock().unwrap();
            states.remove(label);
        }
        
        // 从 KV 存储移除
        let key = format!("{}{}", LOCK_STATE_PREFIX, label);
        kv_store.remove(&key).ok();
    }
}

impl Default for LockStateManager {
    fn default() -> Self {
        Self::new()
    }
}
