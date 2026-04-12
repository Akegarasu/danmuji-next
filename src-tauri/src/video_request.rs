//! 点播请求管理器
//!
//! 从弹幕/SC 中检测 BV/AV 号，管理点播请求列表和持久化。
//! 从 live_data 中抽取独立，遵循扩展 Manager 模式。

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::kv_store::VideoRequestStore;
use crate::live_types::*;
use crate::video_info::VideoInfo;

/// BV/AV 号匹配正则（编译一次，全局复用）
///
/// 只提取 ID 本身，允许前后紧邻其他字符，避免依赖边界判断漏匹配。
/// 使用 find_iter 而非 captures_iter，减少不必要的捕获开销。
static VIDEO_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i:BV[A-Z0-9]{10}|AV[0-9]+)").unwrap());

/// 点播持久化数据（存入 KV Store）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedVideoRequests {
    requests: Vec<VideoRequestItem>,
    seen_ids: HashSet<String>,
}

/// KV Store 中点播数据的 key
const VIDEO_REQUESTS_KV_KEY: &str = "video_requests";

/// 点播请求管理器
pub struct VideoRequestManager {
    /// 点播请求列表
    pub(crate) requests: Vec<VideoRequestItem>,
    /// 已见过的视频 ID（用于去重，小写）
    seen_ids: HashSet<String>,
    /// 持久化存储
    store: Option<VideoRequestStore>,
}

impl Default for VideoRequestManager {
    fn default() -> Self {
        Self {
            requests: Vec::new(),
            seen_ids: HashSet::new(),
            store: None,
        }
    }
}

impl VideoRequestManager {
    /// 创建新实例，附带持久化存储
    pub fn new(store: VideoRequestStore) -> Self {
        Self {
            store: Some(store),
            ..Self::default()
        }
    }

    /// 从 KV Store 加载点播数据
    pub fn load(&mut self) {
        let Some(store) = &self.store else { return };
        if let Some(val) = store.get(VIDEO_REQUESTS_KV_KEY) {
            match serde_json::from_value::<PersistedVideoRequests>(val) {
                Ok(persisted) => {
                    self.requests = persisted.requests;
                    self.seen_ids = persisted.seen_ids;
                    log::info!(
                        "Loaded {} video requests from KV store",
                        self.requests.len()
                    );
                }
                Err(e) => {
                    log::warn!("Failed to parse persisted video requests: {}", e);
                }
            }
        }
    }

    /// 保存点播数据到 KV Store
    fn save(&self) {
        let Some(store) = &self.store else { return };
        let persisted = PersistedVideoRequests {
            requests: self.requests.clone(),
            seen_ids: self.seen_ids.clone(),
        };
        match serde_json::to_value(&persisted) {
            Ok(val) => {
                if let Err(e) = store.set(VIDEO_REQUESTS_KV_KEY.to_string(), val) {
                    log::error!("Failed to save video requests to KV store: {}", e);
                }
            }
            Err(e) => {
                log::error!("Failed to serialize video requests: {}", e);
            }
        }
    }

    /// 获取所有点播请求（快照用）
    pub fn get_all(&self) -> Vec<VideoRequestItem> {
        self.requests.clone()
    }

    /// 从文本中检测 BV/AV号，创建点播请求
    /// 返回需要异步获取视频信息的列表: (request_id, video_id, uid, sc_price)
    /// 同时返回需要推入 pending_updates 的 DataUpdate 列表
    pub fn detect_and_add(
        &mut self,
        content: &str,
        username: &str,
        uid: u64,
        source: VideoRequestSource,
        sc_price: Option<u64>,
        timestamp: i64,
    ) -> (Vec<(String, String, u64, Option<u64>)>, Vec<DataUpdate>) {
        let mut to_fetch = Vec::new();
        let mut updates = Vec::new();

        for matched in VIDEO_ID_RE.find_iter(content) {
            let video_id = matched.as_str().to_string();
            let key = video_id.to_lowercase();

            if self.seen_ids.contains(&key) {
                continue;
            }
            self.seen_ids.insert(key);

            let id = format!("vr_{}_{}", timestamp, uid);
            let item = VideoRequestItem {
                id: id.clone(),
                video_id: video_id.clone(),
                username: username.to_string(),
                uid,
                source: source.clone(),
                sc_price,
                timestamp: if timestamp < 1_000_000_000_000 {
                    timestamp * 1000
                } else {
                    timestamp
                },
                watched: false,
                video_info: None,
                loading: true,
                error: None,
            };

            self.requests.insert(0, item.clone());
            updates.push(DataUpdate::VideoRequestAppend(item));
            to_fetch.push((id, video_id, uid, sc_price));
        }

        if !to_fetch.is_empty() {
            self.save();
        }
        (to_fetch, updates)
    }

    /// 更新点播请求的视频信息
    pub fn update_info(&mut self, request_id: &str, info: Result<VideoInfo, String>) -> Option<DataUpdate> {
        if let Some(item) = self.requests.iter_mut().find(|r| r.id == request_id) {
            match info {
                Ok(vi) => {
                    item.video_info = Some(vi);
                    item.loading = false;
                    item.error = None;
                }
                Err(e) => {
                    item.loading = false;
                    item.error = Some(e);
                }
            }
            let update = DataUpdate::VideoRequestUpdate(item.clone());
            self.save();
            Some(update)
        } else {
            None
        }
    }

    /// 标记点播为已看/未看
    pub fn set_watched(&mut self, request_id: &str, watched: bool) -> DataUpdate {
        if let Some(item) = self.requests.iter_mut().find(|r| r.id == request_id) {
            item.watched = watched;
        }
        self.save();
        DataUpdate::VideoRequestSync(self.requests.clone())
    }

    /// 删除点播请求
    pub fn remove(&mut self, request_id: &str) -> DataUpdate {
        if let Some(item) = self.requests.iter().find(|r| r.id == request_id) {
            self.seen_ids.remove(&item.video_id.to_lowercase());
        }
        self.requests.retain(|r| r.id != request_id);
        self.save();
        DataUpdate::VideoRequestSync(self.requests.clone())
    }

    /// 清空已看的点播
    pub fn clear_watched(&mut self) -> DataUpdate {
        for item in self.requests.iter().filter(|r| r.watched) {
            self.seen_ids.remove(&item.video_id.to_lowercase());
        }
        self.requests.retain(|r| !r.watched);
        self.save();
        DataUpdate::VideoRequestSync(self.requests.clone())
    }

    /// 清空所有点播
    pub fn clear_all(&mut self) -> DataUpdate {
        self.requests.clear();
        self.seen_ids.clear();
        self.save();
        DataUpdate::VideoRequestSync(self.requests.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_id_regex() {
        // BV号 = BV + 恰好10位字母数字 = 12字符
        // 嵌在中文中
        assert!(VIDEO_ID_RE
            .find_iter("点播BV1xx411c7mD谢谢")
            .next()
            .is_some());
        assert!(VIDEO_ID_RE.find_iter("看看av12345吧").next().is_some());
        // 独立出现
        assert!(VIDEO_ID_RE.find_iter("BV1xx411c7mD").next().is_some());
        assert!(VIDEO_ID_RE.find_iter("av999").next().is_some());
        // 空格分隔
        assert!(VIDEO_ID_RE
            .find_iter("请看 BV1xx411c7mD 这个")
            .next()
            .is_some());

        assert!(VIDEO_ID_RE.find_iter("abcBV1xx411c7mD").next().is_some());
        assert!(VIDEO_ID_RE.find_iter("9BV1xx411c7mD").next().is_some());

        assert!(VIDEO_ID_RE.find_iter("BV1xx411c7mDXYZ").next().is_some());
        assert!(VIDEO_ID_RE.find_iter("av12345x").next().is_some());

        let danmaku_list = vec![
            "看看这个av123456",
            "kkBV1xx411c7mD",
            "BV1xx411c7mD这个！",
            "！BV1xx411c7mD",
        ];
        for danmaku in danmaku_list {
            assert!(VIDEO_ID_RE.find_iter(danmaku).next().is_some());
        }

        // find_iter 只提取 video ID 本身
        let caps: Vec<String> = VIDEO_ID_RE
            .find_iter("我想点播BV1xx411c7mD和av67890")
            .map(|m| m.as_str().to_string())
            .collect();
        assert_eq!(caps, vec!["BV1xx411c7mD", "av67890"]);
    }
}
