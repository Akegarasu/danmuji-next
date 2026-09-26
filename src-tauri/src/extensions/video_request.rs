//! 点播请求管理器
//!
//! 从弹幕/SC 中检测 BV/AV 号，产生领域变更与视频信息获取任务。

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::{video_info::VideoInfo, EffectResult, Extension, ExtensionEffect};
use crate::live_events::{ReceivedText, TextSource};
use serde_json::{json, Value};
use std::time::Instant;

/// 点播请求（发送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoRequestItem {
    pub id: String,
    pub video_id: String,
    pub username: String,
    pub uid: u64,
    pub source: VideoRequestSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sc_price: Option<u64>,
    pub timestamp: i64,
    pub watched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_info: Option<VideoInfo>,
    pub loading: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 点播来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoRequestSource {
    Danmaku,
    Superchat,
}

/// BV/AV 号匹配正则（编译一次，全局复用）
///
/// 只提取 ID 本身，允许前后紧邻其他字符，避免依赖边界判断漏匹配。
/// 使用 find_iter 而非 captures_iter，减少不必要的捕获开销。
static VIDEO_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i:BV[A-Z0-9]{10}|AV[0-9]+)").unwrap());

/// 点播请求管理器
pub struct VideoRequestManager {
    /// 点播请求列表
    requests: Vec<VideoRequestItem>,
    /// 已见过的视频 ID（用于去重，小写）
    seen_ids: HashSet<String>,
    effects: Vec<ExtensionEffect>,
}

impl Default for VideoRequestManager {
    fn default() -> Self {
        Self {
            requests: Vec::new(),
            seen_ids: HashSet::new(),
            effects: Vec::new(),
        }
    }
}

impl VideoRequestManager {
    /// 从文本中检测 BV/AV号，创建点播请求
    /// 返回异步任务，宿主负责执行与状态发布。
    fn detect_and_add(
        &mut self,
        content: &str,
        username: &str,
        uid: u64,
        source: VideoRequestSource,
        sc_price: Option<u64>,
        timestamp: i64,
    ) -> Vec<ExtensionEffect> {
        let mut to_fetch = Vec::new();

        for matched in VIDEO_ID_RE.find_iter(content) {
            let video_id = matched.as_str().to_string();
            let key = video_id.to_lowercase();

            if self.seen_ids.contains(&key) {
                continue;
            }
            self.seen_ids.insert(key);

            let id = super::new_id("vr");
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

            self.requests.insert(0, item);
            to_fetch.push(ExtensionEffect::FetchVideoInfo {
                request_id: id,
                video_id,
            });
        }

        to_fetch
    }

    /// 更新点播请求的视频信息
    fn update_info(&mut self, request_id: &str, info: Result<VideoInfo, String>) -> bool {
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
            true
        } else {
            false
        }
    }

    /// 标记点播为已看/未看
    fn set_watched(&mut self, request_id: &str, watched: bool) {
        if let Some(item) = self.requests.iter_mut().find(|r| r.id == request_id) {
            item.watched = watched;
        }
    }

    /// 删除点播请求
    fn remove(&mut self, request_id: &str) {
        if let Some(item) = self.requests.iter().find(|r| r.id == request_id) {
            self.seen_ids.remove(&item.video_id.to_lowercase());
        }
        self.requests.retain(|r| r.id != request_id);
    }

    /// 清空已看的点播
    fn clear_watched(&mut self) {
        for item in self.requests.iter().filter(|r| r.watched) {
            self.seen_ids.remove(&item.video_id.to_lowercase());
        }
        self.requests.retain(|r| !r.watched);
    }

    /// 清空所有点播
    fn clear_all(&mut self) {
        self.requests.clear();
        self.seen_ids.clear();
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    MarkWatched { request_id: String, watched: bool },
    Remove { request_id: String },
    ClearWatched,
    ClearAll,
}

impl Extension for VideoRequestManager {
    fn id(&self) -> &'static str {
        "video-request"
    }
    fn snapshot(&self, _now: Instant) -> Value {
        json!(self.requests)
    }
    fn checkpoint(&self, _now: Instant) -> Value {
        json!(self.requests)
    }
    fn restore(&mut self, value: Value, _now: Instant) -> Result<(), String> {
        let requests: Vec<VideoRequestItem> =
            serde_json::from_value(value).map_err(|e| e.to_string())?;
        let mut ids = HashSet::new();
        if requests.iter().any(|r| !ids.insert(&r.id)) {
            return Err("点播请求 ID 重复".into());
        }
        self.seen_ids = requests.iter().map(|r| r.video_id.to_lowercase()).collect();
        self.requests = requests;
        // 上次退出时未完成的抓取由独立任务层恢复。
        self.effects = self
            .requests
            .iter()
            .filter(|r| r.loading)
            .map(|r| ExtensionEffect::FetchVideoInfo {
                request_id: r.id.clone(),
                video_id: r.video_id.clone(),
            })
            .collect();
        Ok(())
    }
    fn request(&mut self, request: Value, _now: Instant) -> Result<Value, String> {
        let request: Request = serde_json::from_value(request).map_err(|e| e.to_string())?;
        match request {
            Request::MarkWatched {
                request_id,
                watched,
            } => self.set_watched(&request_id, watched),
            Request::Remove { request_id } => self.remove(&request_id),
            Request::ClearWatched => self.clear_watched(),
            Request::ClearAll => self.clear_all(),
        };
        // 尚未开始的已删除点播无需抓取。
        self.effects.retain(|effect| match effect {
            ExtensionEffect::FetchVideoInfo { request_id, .. } => {
                self.requests.iter().any(|r| r.id == *request_id)
            }
        });
        Ok(Value::Null)
    }
    fn on_text(&mut self, text: &ReceivedText, _now: Instant) -> bool {
        let source = match text.source {
            TextSource::Danmaku => VideoRequestSource::Danmaku,
            TextSource::Superchat => VideoRequestSource::Superchat,
        };
        let effects = self.detect_and_add(
            &text.content,
            &text.username,
            text.uid,
            source,
            text.sc_price,
            text.timestamp,
        );
        let changed = !effects.is_empty();
        self.effects.extend(effects);
        changed
    }
    fn take_effects(&mut self) -> Vec<ExtensionEffect> {
        std::mem::take(&mut self.effects)
    }
    fn complete_effect(&mut self, result: EffectResult) -> bool {
        match result {
            EffectResult::VideoInfo { request_id, result } => self.update_info(&request_id, result),
        }
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
