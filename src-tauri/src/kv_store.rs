//! KV 存储模块
//! 提供线程安全的键值存储，用于窗口状态等数据的持久化

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

/// KV 存储结构
#[derive(Debug)]
pub struct KVStore {
    data: Mutex<HashMap<String, Value>>,
    file_path: PathBuf,
}

impl KVStore {
    /// 创建新的 KV 存储实例
    pub fn new(file_path: PathBuf) -> Self {
        let data = if file_path.exists() {
            fs::read_to_string(&file_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            HashMap::new()
        };

        Self {
            data: Mutex::new(data),
            file_path,
        }
    }

    /// 获取值
    pub fn get(&self, key: &str) -> Option<Value> {
        self.data.lock().unwrap().get(key).cloned()
    }

    /// 设置值
    pub fn set(&self, key: String, value: Value) -> Result<(), String> {
        {
            let mut data = self.data.lock().unwrap();
            data.insert(key, value);
        }
        self.save()
    }

    /// 删除值
    pub fn remove(&self, key: &str) -> Result<(), String> {
        {
            let mut data = self.data.lock().unwrap();
            data.remove(key);
        }
        self.save()
    }

    /// 获取所有以指定前缀开头的键值对
    pub fn get_by_prefix(&self, prefix: &str) -> HashMap<String, Value> {
        let data = self.data.lock().unwrap();
        data.iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// 保存到文件
    fn save(&self) -> Result<(), String> {
        let data = self.data.lock().unwrap();
        let json = serde_json::to_string_pretty(&*data).map_err(|e| e.to_string())?;
        fs::write(&self.file_path, json).map_err(|e| e.to_string())
    }
}

/// 点播数据专用 KV 存储（与窗口状态分离）
#[derive(Debug)]
pub struct VideoRequestStore(KVStore);

impl VideoRequestStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self(KVStore::new(file_path))
    }
}

impl std::ops::Deref for VideoRequestStore {
    type Target = KVStore;
    fn deref(&self) -> &KVStore {
        &self.0
    }
}

/// 投票数据专用 KV 存储
#[derive(Debug)]
pub struct VotingStore(KVStore);

impl VotingStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self(KVStore::new(file_path))
    }
}

impl std::ops::Deref for VotingStore {
    type Target = KVStore;
    fn deref(&self) -> &KVStore {
        &self.0
    }
}
