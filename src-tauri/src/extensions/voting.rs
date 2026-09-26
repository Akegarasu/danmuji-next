//! 投票管理器
//!
//! 支持发起弹幕投票，通过字母或数字选项匹配弹幕内容。
//! 支持多个并发投票、UID 去重、定时结束。
//! 仅管理投票领域状态，由宿主负责持久化与定时调度。

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::Extension;
use crate::live_events::{ReceivedText, TextSource};
use serde_json::{json, Value};
use std::time::Instant;

// ==================== 数据结构 ====================

/// 投票选项标识类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoteKeyType {
    /// 字母选项 (A, B, C, ...)
    Letter,
    /// 数字选项 (1, 2, 3, ...)
    Number,
}

/// 投票状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PollStatus {
    Active,
    Ended,
}

/// 投票人信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voter {
    pub uid: u64,
    pub username: String,
    pub timestamp: i64,
}

/// 投票选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollOption {
    /// 选项键: "A", "B", "1", "2", etc.
    pub key: String,
    /// 选项描述文本
    pub label: String,
    /// 票数
    pub vote_count: u32,
    /// 投票者列表（推送给前端时不包含，按需加载）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voters: Vec<Voter>,
}

/// 投票（完整状态）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Poll {
    pub id: String,
    pub title: String,
    pub key_type: VoteKeyType,
    pub options: Vec<PollOption>,
    pub status: PollStatus,
    /// 每个 UID 只能投一次：uid(字符串) -> 投给的 key
    #[serde(default)]
    pub voted_uids: HashMap<String, String>,
    pub total_votes: u32,
    pub created_at: i64,
    /// 定时结束（毫秒时间戳），None 表示手动结束
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_at: Option<i64>,
}

impl Poll {
    fn snapshot(&self) -> Value {
        json!({"id": self.id, "title": self.title, "key_type": self.key_type,
            "options": self.options.iter().map(|o| json!({"key":o.key,"label":o.label,"vote_count":o.vote_count})).collect::<Vec<_>>(),
            "status":self.status,"total_votes":self.total_votes,"created_at":self.created_at,"end_at":self.end_at})
    }
}

// ==================== 投票管理器 ====================

/// 投票管理器
pub struct VotingManager {
    /// 所有投票（包括已结束的）
    polls: Vec<Poll>,
    /// 活跃投票 ID 集合
    active_poll_ids: HashSet<String>,
    /// 活跃投票的选项键映射：大写选项键 -> 活跃投票 ID 列表
    active_option_keys: HashMap<String, Vec<String>>,
}

impl Default for VotingManager {
    fn default() -> Self {
        Self {
            polls: Vec::new(),
            active_poll_ids: HashSet::new(),
            active_option_keys: HashMap::new(),
        }
    }
}

impl VotingManager {
    /// 重建活跃索引
    fn rebuild_indexes(&mut self) {
        self.active_poll_ids.clear();
        self.active_option_keys.clear();

        for poll in &self.polls {
            if poll.status == PollStatus::Active {
                self.active_poll_ids.insert(poll.id.clone());
                for option in &poll.options {
                    self.active_option_keys
                        .entry(option.key.to_uppercase())
                        .or_default()
                        .push(poll.id.clone());
                }
            }
        }
    }

    /// 添加单个投票的选项到活跃索引
    fn add_to_indexes(&mut self, poll: &Poll) {
        self.active_poll_ids.insert(poll.id.clone());
        for option in &poll.options {
            self.active_option_keys
                .entry(option.key.to_uppercase())
                .or_default()
                .push(poll.id.clone());
        }
    }

    /// 从活跃索引移除单个投票
    fn remove_from_indexes(&mut self, poll: &Poll) {
        self.active_poll_ids.remove(&poll.id);
        for option in &poll.options {
            let key = option.key.to_uppercase();
            if let Some(ids) = self.active_option_keys.get_mut(&key) {
                ids.retain(|id| id != &poll.id);
                if ids.is_empty() {
                    self.active_option_keys.remove(&key);
                }
            }
        }
    }

    // ==================== 公开接口 ====================

    /// 是否有活跃投票（用于短路弹幕匹配）
    fn has_active_polls(&self) -> bool {
        !self.active_poll_ids.is_empty()
    }

    /// 创建投票
    fn create_poll(
        &mut self,
        title: String,
        options: Vec<(String, String)>,
        key_type: VoteKeyType,
        duration_secs: Option<u64>,
    ) -> Result<Poll, String> {
        let now = Utc::now().timestamp_millis();
        let end_at = duration_secs
            .map(|secs| {
                i64::try_from(secs)
                    .ok()
                    .and_then(|s| s.checked_mul(1000))
                    .and_then(|ms| now.checked_add(ms))
                    .ok_or("投票时长超出范围".to_string())
            })
            .transpose()?;
        if options.is_empty() {
            return Err("请至少添加一个投票选项".into());
        }
        let mut keys = HashSet::new();
        if options
            .iter()
            .any(|(key, _)| key.trim().is_empty() || !keys.insert(key.trim().to_uppercase()))
        {
            return Err("投票选项键不能为空或重复".into());
        }

        let poll = Poll {
            id: super::new_id("poll"),
            title,
            key_type,
            options: options
                .into_iter()
                .map(|(key, label)| PollOption {
                    key: key.trim().to_uppercase(),
                    label,
                    vote_count: 0,
                    voters: Vec::new(),
                })
                .collect(),
            status: PollStatus::Active,
            voted_uids: HashMap::new(),
            total_votes: 0,
            created_at: now,
            end_at,
        };

        self.add_to_indexes(&poll);
        self.polls.insert(0, poll.clone());

        Ok(poll)
    }

    /// 结束投票
    fn end_poll(&mut self, poll_id: &str) -> Option<Poll> {
        let poll = self.polls.iter_mut().find(|p| p.id == poll_id)?;
        if poll.status != PollStatus::Active {
            return None;
        }
        poll.status = PollStatus::Ended;
        let result = poll.clone();
        self.remove_from_indexes(&result);
        Some(result)
    }

    /// 删除投票
    fn delete_poll(&mut self, poll_id: &str) {
        if let Some(poll) = self.polls.iter().find(|p| p.id == poll_id) {
            if poll.status == PollStatus::Active {
                self.remove_from_indexes(&poll.clone());
            }
        }
        self.polls.retain(|p| p.id != poll_id);
    }

    /// 获取投票选项的投票者列表
    fn get_poll_voters(&self, poll_id: &str, option_key: &str) -> Option<Vec<Voter>> {
        let poll = self.polls.iter().find(|p| p.id == poll_id)?;
        let option = poll.options.iter().find(|o| o.key == option_key)?;
        Some(option.voters.clone())
    }

    /// 尝试匹配弹幕为投票
    /// 返回状态是否变化。
    fn try_vote(&mut self, content: &str, uid: u64, username: &str, timestamp: i64) -> bool {
        let mut changed = self.check_expired_polls(Utc::now().timestamp_millis());
        let trimmed = content.trim().to_uppercase();
        let uid_str = uid.to_string();

        // O(1) 查找匹配的活跃投票 ID 列表
        let poll_ids = match self.active_option_keys.get(&trimmed) {
            Some(ids) => ids.clone(),
            None => return changed,
        };

        for poll_id in &poll_ids {
            let Some(poll) = self.polls.iter_mut().find(|p| p.id == *poll_id) else {
                continue;
            };

            // UID 去重：当前投票中每个 UID 只能投一次
            if poll.voted_uids.contains_key(&uid_str) {
                continue;
            }

            // 记录投票
            poll.voted_uids.insert(uid_str.clone(), trimmed.clone());
            poll.total_votes += 1;

            if let Some(option) = poll
                .options
                .iter_mut()
                .find(|o| o.key.to_uppercase() == trimmed)
            {
                option.vote_count += 1;
                option.voters.push(Voter {
                    uid,
                    username: username.to_string(),
                    timestamp,
                });
            }

            changed = true;
        }

        changed
    }

    /// 检查定时结束的投票，返回状态是否变化
    fn check_expired_polls(&mut self, now: i64) -> bool {
        let mut changed = false;

        // 收集需要结束的投票 ID
        let expired_ids: Vec<String> = self
            .polls
            .iter()
            .filter(|p| p.status == PollStatus::Active && p.end_at.map_or(false, |end| now >= end))
            .map(|p| p.id.clone())
            .collect();

        for poll_id in expired_ids {
            changed |= self.end_poll(&poll_id).is_some();
        }

        changed
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    Create {
        title: String,
        options: Vec<(String, String)>,
        key_type: VoteKeyType,
        duration_secs: Option<u64>,
    },
    End {
        poll_id: String,
    },
    Delete {
        poll_id: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Query {
    Voters { poll_id: String, option_key: String },
}

impl Extension for VotingManager {
    fn id(&self) -> &'static str {
        "voting"
    }
    fn snapshot(&self, _now: Instant) -> Value {
        json!(self.polls.iter().map(Poll::snapshot).collect::<Vec<_>>())
    }
    fn checkpoint(&self, _now: Instant) -> Value {
        json!(self.polls)
    }
    fn restore(&mut self, value: Value, _now: Instant) -> Result<(), String> {
        self.polls = serde_json::from_value(value).map_err(|e| e.to_string())?;
        self.rebuild_indexes();
        Ok(())
    }
    fn request(&mut self, request: Value, _now: Instant) -> Result<Value, String> {
        let request: Request = serde_json::from_value(request).map_err(|e| e.to_string())?;
        match request {
            Request::Create {
                title,
                options,
                key_type,
                duration_secs,
            } => {
                let poll = self.create_poll(title, options, key_type, duration_secs)?;
                Ok(poll.snapshot())
            }
            Request::End { poll_id } => {
                let poll = self.end_poll(&poll_id).ok_or("投票不存在或已结束")?;
                Ok(poll.snapshot())
            }
            Request::Delete { poll_id } => {
                self.delete_poll(&poll_id);
                Ok(Value::Null)
            }
        }
    }
    fn query(&self, query: Value) -> Result<Value, String> {
        let query: Query = serde_json::from_value(query).map_err(|e| e.to_string())?;
        match query {
            Query::Voters {
                poll_id,
                option_key,
            } => Ok(json!(self
                .get_poll_voters(&poll_id, &option_key)
                .ok_or("投票或选项不存在")?)),
        }
    }
    fn on_text(&mut self, text: &ReceivedText, _now: Instant) -> bool {
        if text.source != TextSource::Danmaku || !self.has_active_polls() {
            return false;
        }
        self.try_vote(&text.content, text.uid, &text.username, text.timestamp)
    }
    fn tick(&mut self, _now: Instant) -> bool {
        self.check_expired_polls(Utc::now().timestamp_millis())
    }
}
