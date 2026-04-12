//! 投票管理器
//!
//! 支持发起弹幕投票，通过字母或数字选项匹配弹幕内容。
//! 支持多个并发投票、UID 去重、定时结束。
//! 遵循扩展 Manager 模式。

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::kv_store::VotingStore;
use crate::live_types::DataUpdate;

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
    /// 创建不含 voters 的精简副本（用于推送）
    fn to_push_view(&self) -> Poll {
        Poll {
            options: self.options.iter().map(|o| PollOption {
                key: o.key.clone(),
                label: o.label.clone(),
                vote_count: o.vote_count,
                voters: Vec::new(), // 推送时不含投票者列表
            }).collect(),
            ..self.clone()
        }
    }
}

// ==================== 持久化 ====================

/// KV Store 中投票数据的 key
const VOTING_KV_KEY: &str = "voting_data";

/// 投票持久化数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedVoting {
    polls: Vec<Poll>,
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
    /// 持久化存储
    store: Option<VotingStore>,
}

impl Default for VotingManager {
    fn default() -> Self {
        Self {
            polls: Vec::new(),
            active_poll_ids: HashSet::new(),
            active_option_keys: HashMap::new(),
            store: None,
        }
    }
}

impl VotingManager {
    /// 创建新实例，附带持久化存储
    pub fn new(store: VotingStore) -> Self {
        Self {
            store: Some(store),
            ..Self::default()
        }
    }

    /// 从 KV Store 加载投票数据
    pub fn load(&mut self) {
        let Some(store) = &self.store else { return };
        if let Some(val) = store.get(VOTING_KV_KEY) {
            match serde_json::from_value::<PersistedVoting>(val) {
                Ok(persisted) => {
                    self.polls = persisted.polls;
                    self.rebuild_indexes();
                    log::info!(
                        "Loaded {} polls ({} active) from KV store",
                        self.polls.len(),
                        self.active_poll_ids.len()
                    );
                }
                Err(e) => {
                    log::warn!("Failed to parse persisted voting data: {}", e);
                }
            }
        }
    }

    /// 保存投票数据到 KV Store
    fn save(&self) {
        let Some(store) = &self.store else { return };
        let persisted = PersistedVoting {
            polls: self.polls.clone(),
        };
        match serde_json::to_value(&persisted) {
            Ok(val) => {
                if let Err(e) = store.set(VOTING_KV_KEY.to_string(), val) {
                    log::error!("Failed to save voting data to KV store: {}", e);
                }
            }
            Err(e) => {
                log::error!("Failed to serialize voting data: {}", e);
            }
        }
    }

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
    pub fn has_active_polls(&self) -> bool {
        !self.active_poll_ids.is_empty()
    }

    /// 创建投票
    pub fn create_poll(
        &mut self,
        title: String,
        options: Vec<(String, String)>,
        key_type: VoteKeyType,
        duration_secs: Option<u64>,
    ) -> Poll {
        let now = Utc::now().timestamp_millis();
        let end_at = duration_secs.map(|secs| now + (secs as i64) * 1000);

        let poll = Poll {
            id: format!("poll_{}_{}", now, self.polls.len()),
            title,
            key_type,
            options: options
                .into_iter()
                .map(|(key, label)| PollOption {
                    key,
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
        self.save();

        poll.to_push_view()
    }

    /// 结束投票
    pub fn end_poll(&mut self, poll_id: &str) -> Option<Poll> {
        let poll = self.polls.iter_mut().find(|p| p.id == poll_id)?;
        if poll.status != PollStatus::Active {
            return None;
        }
        poll.status = PollStatus::Ended;
        let result = poll.to_push_view();
        self.remove_from_indexes(&result);
        self.save();
        Some(result)
    }

    /// 删除投票
    pub fn delete_poll(&mut self, poll_id: &str) {
        if let Some(poll) = self.polls.iter().find(|p| p.id == poll_id) {
            if poll.status == PollStatus::Active {
                self.remove_from_indexes(&poll.clone());
            }
        }
        self.polls.retain(|p| p.id != poll_id);
        self.save();
    }

    /// 获取所有投票（快照用，不含 voters）
    pub fn get_all_polls_for_snapshot(&self) -> Vec<Poll> {
        self.polls.iter().map(|p| p.to_push_view()).collect()
    }

    /// 获取投票选项的投票者列表
    pub fn get_poll_voters(&self, poll_id: &str, option_key: &str) -> Option<Vec<Voter>> {
        let poll = self.polls.iter().find(|p| p.id == poll_id)?;
        let option = poll.options.iter().find(|o| o.key == option_key)?;
        Some(option.voters.clone())
    }

    /// 尝试匹配弹幕为投票
    /// 返回需要推送的 DataUpdate 列表
    pub fn try_vote(
        &mut self,
        content: &str,
        uid: u64,
        username: &str,
        timestamp: i64,
    ) -> Vec<DataUpdate> {
        let trimmed = content.trim().to_uppercase();
        let uid_str = uid.to_string();

        // O(1) 查找匹配的活跃投票 ID 列表
        let poll_ids = match self.active_option_keys.get(&trimmed) {
            Some(ids) => ids.clone(),
            None => return Vec::new(),
        };

        let mut updates = Vec::new();

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

            if let Some(option) = poll.options.iter_mut().find(|o| o.key.to_uppercase() == trimmed) {
                option.vote_count += 1;
                option.voters.push(Voter {
                    uid,
                    username: username.to_string(),
                    timestamp,
                });
            }

            updates.push(DataUpdate::VotingUpdate(poll.to_push_view()));
        }

        if !updates.is_empty() {
            self.save();
        }

        updates
    }

    /// 检查定时结束的投票，返回需要推送的更新
    pub fn check_expired_polls(&mut self) -> Vec<DataUpdate> {
        let now = Utc::now().timestamp_millis();
        let mut updates = Vec::new();

        // 收集需要结束的投票 ID
        let expired_ids: Vec<String> = self
            .polls
            .iter()
            .filter(|p| {
                p.status == PollStatus::Active
                    && p.end_at.map_or(false, |end| now >= end)
            })
            .map(|p| p.id.clone())
            .collect();

        for poll_id in expired_ids {
            if let Some(poll) = self.end_poll(&poll_id) {
                updates.push(DataUpdate::VotingUpdate(poll));
            }
        }

        updates
    }
}
