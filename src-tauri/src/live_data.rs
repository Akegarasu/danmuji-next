//! 直播数据状态管理
//!
//! LiveData 持有所有实时直播数据（弹幕、礼物、SC、统计等），
//! 负责事件处理、数据聚合、礼物合并等逻辑。
//! 扩展功能（点播、投票等）通过独立 Manager 管理。

use std::collections::{HashMap, HashSet, VecDeque};

use tokio::sync::mpsc;

use crate::archive::ArchiveEvent;
use blivedm::api::ContributionRankUser;
use blivedm::{
    CoinType, Danmaku, DanmakuType, Gift, GuardBuy, GuardLevel, InteractWord, OnlineRankCount,
    OnlineRankUser, OnlineRankV2, OnlineRankV3, SuperChat,
};
use crate::kv_store::{VideoRequestStore, VotingStore};
use crate::live_types::*;
use crate::video_info::VideoInfo;
use crate::video_request::VideoRequestManager;
use crate::voting::VotingManager;

// ==================== 窗口订阅 ====================

/// 窗口订阅信息
#[derive(Debug, Clone, Default)]
pub struct WindowSubscription {
    pub event_types: HashSet<EventType>,
}

// ==================== 数据状态 ====================

/// 直播数据状态
pub struct LiveData {
    /// 弹幕列表
    pub(crate) danmaku_list: VecDeque<ProcessedDanmaku>,
    /// 礼物列表
    pub(crate) gift_list: VecDeque<ProcessedGift>,
    /// 礼物合并索引: merge_key -> list index
    gift_merge_index: HashMap<String, usize>,
    /// 已处理的上游礼物交易 ID，用于断线重发和双协议重复包去重
    seen_gift_transactions: HashSet<String>,
    /// 交易 ID 插入顺序，用于限制去重缓存大小
    seen_gift_transaction_order: VecDeque<String>,
    /// 普通礼物本地序号；没有上游交易 ID 时仍保证每条礼物独立
    next_gift_sequence: u64,
    /// SC 列表
    pub(crate) superchat_list: Vec<ProcessedSuperChat>,
    /// 高能用户排行
    pub(crate) online_rank: Vec<ProcessedOnlineRankUser>,
    /// 贡献排行完整列表（API）
    pub(crate) contribution_rank_full: Vec<ContributionRankUser>,
    /// 用户贡献 map: uid -> contribution
    user_contributions: HashMap<u64, UserContribution>,
    /// 进入直播间列表
    pub(crate) interact_word_list: VecDeque<ProcessedInteractWord>,
    /// 统计数据
    pub(crate) stats: LiveStats,

    /// 点播请求管理器
    pub(crate) video_requests: VideoRequestManager,

    /// 投票管理器
    pub(crate) voting: VotingManager,

    /// 待发送的更新
    pub(crate) pending_updates: Vec<DataUpdate>,
    /// 待发送的弹幕（批量）
    pending_danmaku: Vec<ProcessedDanmaku>,
    /// 待发送的礼物更新（批量）
    pending_gift_upserts: Vec<GiftUpsert>,
    /// 待发送的入场通知（批量）
    pending_interact_words: Vec<ProcessedInteractWord>,
    /// 统计是否有变化
    stats_dirty: bool,
    /// 贡献排行是否有变化
    contributions_dirty: bool,
    /// 存档 channel sender（连接时设置）
    pub(crate) archive_tx: Option<mpsc::UnboundedSender<ArchiveEvent>>,
}

impl Default for LiveData {
    fn default() -> Self {
        Self {
            danmaku_list: VecDeque::with_capacity(MAX_DANMAKU_LIST),
            gift_list: VecDeque::with_capacity(MAX_GIFT_LIST),
            gift_merge_index: HashMap::new(),
            seen_gift_transactions: HashSet::new(),
            seen_gift_transaction_order: VecDeque::new(),
            next_gift_sequence: 0,
            superchat_list: Vec::with_capacity(MAX_SUPERCHAT_LIST),
            interact_word_list: VecDeque::with_capacity(MAX_INTERACT_WORD_LIST),
            online_rank: Vec::new(),
            contribution_rank_full: Vec::new(),
            user_contributions: HashMap::new(),
            stats: LiveStats::default(),
            video_requests: VideoRequestManager::default(),
            voting: VotingManager::default(),
            pending_updates: Vec::new(),
            pending_danmaku: Vec::new(),
            pending_gift_upserts: Vec::new(),
            pending_interact_words: Vec::new(),
            stats_dirty: false,
            contributions_dirty: false,
            archive_tx: None,
        }
    }
}

impl LiveData {
    /// 创建新实例，附带扩展持久化存储
    pub(crate) fn new(vr_store: VideoRequestStore, voting_store: VotingStore) -> Self {
        let mut video_requests = VideoRequestManager::new(vr_store);
        video_requests.load();

        let mut voting = VotingManager::new(voting_store);
        voting.load();

        Self {
            video_requests,
            voting,
            ..Self::default()
        }
    }

    /// 清空直播数据（保留扩展管理器状态和存储引用）
    pub fn clear(&mut self) {
        let video_requests = std::mem::take(&mut self.video_requests);
        let voting = std::mem::take(&mut self.voting);
        *self = Self::default();
        self.video_requests = video_requests;
        self.voting = voting;
    }

    /// 从 KV Store 加载点播数据
    pub fn load_video_requests(&mut self) {
        self.video_requests.load();
    }

    /// 生成数据快照
    pub fn snapshot(&self, event_types: &HashSet<EventType>) -> DataSnapshot {
        DataSnapshot {
            danmaku_list: if event_types.contains(&EventType::Danmaku) {
                Some(self.danmaku_list.iter().cloned().collect())
            } else {
                None
            },
            gift_list: if event_types.contains(&EventType::Gift) {
                Some(self.gift_list.iter().cloned().collect())
            } else {
                None
            },
            superchat_list: if event_types.contains(&EventType::SuperChat) {
                Some(self.superchat_list.clone())
            } else {
                None
            },
            contribution_rank_live: if event_types.contains(&EventType::ContributionRank) {
                Some(self.online_rank.clone())
            } else {
                None
            },
            contribution_rank_full: if event_types.contains(&EventType::ContributionRank) {
                Some(self.contribution_rank_full.clone())
            } else {
                None
            },
            contributions: if event_types.contains(&EventType::ContributionRank) {
                let mut contributions: Vec<_> = self.user_contributions.values().cloned().collect();
                contributions.sort_by(|a, b| b.total_value.cmp(&a.total_value));
                contributions.truncate(50);
                Some(contributions)
            } else {
                None
            },
            stats: if event_types.contains(&EventType::Stats) {
                Some(self.stats.clone())
            } else {
                None
            },
            video_requests: if event_types.contains(&EventType::VideoRequest) {
                Some(self.video_requests.get_all())
            } else {
                None
            },
            voting_polls: if event_types.contains(&EventType::Voting) {
                Some(self.voting.get_all_polls_for_snapshot())
            } else {
                None
            },
            interact_word_list: if event_types.contains(&EventType::InteractWord) {
                Some(self.interact_word_list.iter().cloned().collect())
            } else {
                None
            },
        }
    }

    // ==================== 事件处理 ====================

    /// 处理弹幕，返回需要异步获取视频信息的列表
    pub fn process_danmaku(&mut self, danmaku: Danmaku) -> Vec<(String, String, u64, Option<u64>)> {
        let processed = ProcessedDanmaku {
            id: format!("dm_{}_{}", danmaku.timestamp, danmaku.sender.uid),
            content: danmaku.content,
            user: convert_user(&danmaku.sender),
            timestamp: danmaku.timestamp,
            is_emoticon: danmaku.r#type == DanmakuType::Emoticon,
            emoticon_url: danmaku.emoticon.map(|e| e.url),
        };

        self.danmaku_list.push_back(processed.clone());
        if self.danmaku_list.len() > MAX_DANMAKU_LIST {
            self.danmaku_list.pop_front();
        }

        if let Some(tx) = &self.archive_tx {
            let _ = tx.send(ArchiveEvent::Danmaku(processed.clone()));
        }

        let (detected, vr_updates) = self.video_requests.detect_and_add(
            &processed.content,
            &processed.user.name,
            processed.user.uid,
            VideoRequestSource::Danmaku,
            None,
            processed.timestamp,
        );
        self.pending_updates.extend(vr_updates);

        // 投票匹配（仅在有活跃投票时）
        if self.voting.has_active_polls() {
            let vote_updates = self.voting.try_vote(
                &processed.content,
                processed.user.uid,
                &processed.user.name,
                processed.timestamp,
            );
            self.pending_updates.extend(vote_updates);
        }

        self.pending_danmaku.push(processed);
        detected
    }

    /// 处理礼物
    pub fn process_gift(&mut self, gift: Gift) {
        let has_transaction_id = gift.transaction_id.is_some();
        if let Some(transaction_id) = gift.transaction_id.as_deref() {
            if !self.remember_gift_transaction(transaction_id) {
                return;
            }
        }

        let is_combo = gift.is_combo();
        let (id, merge_key) = if let Some(batch_combo_id) = gift.batch_combo_id.as_deref() {
            let key = format!(
                "combo:{}:{}:{}",
                gift.sender_uid, gift.gift_id, batch_combo_id
            );
            (format!("gift:{key}"), key)
        } else if let Some(transaction_id) = gift.transaction_id.as_deref() {
            let key = format!("gift:tid:{transaction_id}");
            (key.clone(), key)
        } else {
            let sequence = self.take_next_gift_sequence();
            let key = format!("gift:local:{sequence}");
            (key.clone(), key)
        };

        let is_paid = gift.coin_type == CoinType::Gold;
        let display_value = if is_paid {
            gift.revealed_total_coin() / 100
        } else {
            0
        };
        let revenue_value = if is_paid { gift.total_coin / 100 } else { 0 };
        let combo_display_value = if is_paid {
            gift.combo_display_total_coin().map(|value| value / 100)
        } else {
            None
        };
        let combo_total_num = gift.combo_total_num();
        let processed_combo = convert_gift_combo(&gift);
        let processed_blind_gift = gift
            .blind_gift
            .as_ref()
            .map(|blind_gift| ProcessedBlindGift {
                gift_id: blind_gift.original_gift_id,
                gift_name: blind_gift.original_gift_name.clone(),
                total_value: revenue_value,
            });

        let sender_uid = gift.sender_uid;
        let sender_name = gift.sender_name.clone();
        let sender_face = gift.sender_face.clone();
        let guard_level = gift.guard_level.clone();

        let existing_index = is_combo
            .then(|| self.gift_merge_index.get(&merge_key).copied())
            .flatten()
            .filter(|index| self.gift_list.get(*index).is_some());

        let (processed, action) = if let Some(index) = existing_index {
            let existing = self
                .gift_list
                .get(index)
                .expect("validated gift merge index");

            // 没有 tid 时，用服务端累计进度挡住重复/过期快照。没有累计字段
            // 则宁可保留事件，也不猜测两个同秒同金额的礼物是重复包。
            if !has_transaction_id
                && combo_has_progress_marker(&gift)
                && !combo_snapshot_progresses(existing, &gift)
            {
                return;
            }

            let existing = self
                .gift_list
                .get_mut(index)
                .expect("validated gift merge index");
            existing.num = match combo_total_num {
                Some(total) => existing.num.max(saturating_u32(total)),
                None => existing.num.saturating_add(gift.num),
            };
            existing.total_value = match combo_display_value {
                Some(total) => existing.total_value.max(total),
                None => existing.total_value.saturating_add(display_value),
            };
            existing.revenue_value = existing.revenue_value.saturating_add(revenue_value);
            existing.timestamp = existing.timestamp.max(gift.timestamp);

            if existing.gift_name.is_empty() && !gift.gift_name.is_empty() {
                existing.gift_name = gift.gift_name.clone();
            }
            if existing.gift_icon.is_empty() && !gift.gift_icon.is_empty() {
                existing.gift_icon = gift.gift_icon.clone();
            }
            if existing.user.name.is_empty() && !gift.sender_name.is_empty() {
                existing.user.name = gift.sender_name.clone();
            }
            if existing.user.face.is_none() {
                existing.user.face = gift.sender_face.clone();
            }
            if existing.user.medal.is_none() {
                existing.user.medal = gift.medal.as_ref().map(convert_medal);
            }
            if existing.user.guard_level == 0 {
                existing.user.guard_level = guard_level_to_u8(&gift.guard_level);
            }
            merge_gift_combo(&mut existing.combo, processed_combo);

            if existing.blind_gift.is_none() {
                existing.blind_gift = processed_blind_gift;
            }
            if let Some(blind_gift) = existing.blind_gift.as_mut() {
                blind_gift.total_value = existing.revenue_value;
            }

            (existing.clone(), UpsertAction::Update)
        } else {
            let initial_num = combo_total_num
                .map(saturating_u32)
                .unwrap_or(gift.num);
            let processed = ProcessedGift {
                id,
                merge_key: merge_key.clone(),
                gift_id: gift.gift_id,
                gift_name: gift.gift_name,
                gift_icon: gift.gift_icon,
                num: initial_num,
                total_value: combo_display_value.unwrap_or(display_value),
                revenue_value,
                is_paid,
                combo: processed_combo,
                blind_gift: processed_blind_gift,
                user: ProcessedUser {
                    uid: gift.sender_uid,
                    name: gift.sender_name,
                    face: gift.sender_face,
                    medal: gift.medal.map(|m| convert_medal(&m)),
                    guard_level: guard_level_to_u8(&gift.guard_level),
                    is_admin: false,
                },
                timestamp: gift.timestamp,
                guard_level: None,
            };

            let index = self.gift_list.len();
            self.gift_list.push_back(processed.clone());
            self.gift_merge_index.insert(merge_key.clone(), index);

            if self.gift_list.len() > MAX_GIFT_LIST {
                self.gift_list.pop_front();
                self.rebuild_gift_index();
            }

            (processed, UpsertAction::Insert)
        };

        // 活跃 combo 应随最新一包移动到列表末尾，否则长 combo 会一直停在
        // 首次出现的位置，并破坏互动页三路时间线的升序前提。
        if let Some(index) = existing_index {
            self.gift_list.remove(index);
            self.gift_list.push_back(processed.clone());
            self.rebuild_gift_index();
        }

        // combo 的每个累计快照都交给归档；归档层按 original_id 原位更新。
        if let Some(tx) = &self.archive_tx {
            let _ = tx.send(ArchiveEvent::Gift(processed.clone()));
        }

        self.pending_gift_upserts.push(GiftUpsert {
            merge_key,
            gift: processed,
            action,
        });

        if is_paid && revenue_value > 0 {
            self.stats.gift_revenue += revenue_value;
            self.stats.total_revenue += revenue_value;
            self.stats_dirty = true;

            self.update_user_contribution(
                sender_uid,
                &sender_name,
                sender_face.as_deref(),
                revenue_value,
                &guard_level,
            );
        }
    }

    /// 处理 SC，返回需要异步获取视频信息的列表
    pub fn process_superchat(&mut self, sc: SuperChat) -> Vec<(String, String, u64, Option<u64>)> {
        let price = (sc.price as u64) * 10;

        let sender_uid = sc.sender_uid;
        let sender_name = sc.sender_name.clone();
        let sender_face = sc.sender_face.clone();
        let guard_level = sc.guard_level.clone();

        let processed = ProcessedSuperChat {
            id: format!("sc_{}", sc.id),
            content: sc.message,
            price,
            user: ProcessedUser {
                uid: sc.sender_uid,
                name: sc.sender_name,
                face: sc.sender_face,
                medal: sc.medal.map(|m| convert_medal(&m)),
                guard_level: guard_level_to_u8(&sc.guard_level),
                is_admin: false,
            },
            background_color: sc.background_color,
            duration: sc.duration,
            start_time: sc.start_time,
        };

        self.superchat_list.insert(0, processed.clone());
        if self.superchat_list.len() > MAX_SUPERCHAT_LIST {
            self.superchat_list.pop();
        }

        if let Some(tx) = &self.archive_tx {
            let _ = tx.send(ArchiveEvent::SuperChat(processed.clone()));
        }

        self.stats.sc_revenue += price;
        self.stats.total_revenue += price;
        self.stats_dirty = true;

        self.update_user_contribution(
            sender_uid,
            &sender_name,
            sender_face.as_deref(),
            price,
            &guard_level,
        );

        let (detected, vr_updates) = self.video_requests.detect_and_add(
            &processed.content,
            &processed.user.name,
            processed.user.uid,
            VideoRequestSource::Superchat,
            Some(price),
            processed.start_time,
        );
        self.pending_updates.extend(vr_updates);

        self.pending_updates
            .push(DataUpdate::SuperChatAppend(processed));

        detected
    }

    /// 处理大航海
    pub fn process_guard_buy(&mut self, guard: GuardBuy) {
        let mut value = guard.price / 100;
        let guard_level_u8 = guard_level_to_u8(&guard.guard_level);
        let timestamp = guard.start_time;

        if guard.num > 1 {
            value *= guard.num as u64;
        }

        let sequence = self.take_next_gift_sequence();
        let id = format!("guard:{}:{}:{}", timestamp, guard.uid, sequence);
        let merge_key = id.clone();

        let processed = ProcessedGift {
            id,
            merge_key: merge_key.clone(),
            gift_id: guard.gift_id,
            gift_name: guard.guard_name().to_string(),
            gift_icon: "".to_string(),
            num: guard.num,
            total_value: value,
            revenue_value: value,
            is_paid: true,
            combo: None,
            blind_gift: None,
            user: ProcessedUser {
                uid: guard.uid,
                name: guard.username.clone(),
                face: None,
                medal: None,
                guard_level: guard_level_u8,
                is_admin: false,
            },
            timestamp,
            guard_level: Some(guard_level_u8),
        };

        if let Some(tx) = &self.archive_tx {
            let _ = tx.send(ArchiveEvent::Gift(processed.clone()));
        }

        let index = self.gift_list.len();
        self.gift_list.push_back(processed.clone());
        self.gift_merge_index.insert(merge_key, index);

        if self.gift_list.len() > MAX_GIFT_LIST {
            self.gift_list.pop_front();
            self.rebuild_gift_index();
        }

        self.pending_gift_upserts.push(GiftUpsert {
            merge_key: processed.merge_key.clone(),
            gift: processed,
            action: UpsertAction::Insert,
        });

        self.stats.guard_revenue += value;
        self.stats.total_revenue += value;
        self.stats_dirty = true;

        self.update_user_contribution(guard.uid, &guard.username, None, value, &guard.guard_level);
    }

    /// 处理贡献排行实时更新（ONLINE_RANK_V2）
    pub fn process_online_rank(&mut self, rank: OnlineRankV2) {
        self.process_online_rank_users(rank.online_list);
    }

    /// 处理贡献排行实时更新（ONLINE_RANK_V3）
    pub fn process_online_rank_v3(&mut self, rank: OnlineRankV3) {
        self.process_online_rank_users(rank.into_online_users());
    }

    fn process_online_rank_users(&mut self, users: Vec<OnlineRankUser>) {
        self.online_rank = users
            .into_iter()
            .map(|u| ProcessedOnlineRankUser {
                uid: u.uid,
                name: u.name,
                face: u.face,
                rank: u.rank,
                score: u.score,
                guard_level: guard_level_to_u8(&u.guard_level),
            })
            .collect();

        self.pending_updates
            .push(DataUpdate::ContributionRankLive(self.online_rank.clone()));
    }

    /// 处理在线人数
    pub fn process_online_count(&mut self, count: OnlineRankCount) {
        self.stats.online_count = count.online_count;
        self.stats_dirty = true;
    }

    /// 处理进入直播间
    pub fn process_interact_word(&mut self, iw: InteractWord) {
        let processed = ProcessedInteractWord {
            id: format!("iw_{}_{}", iw.timestamp, iw.user.uid),
            user: convert_user(&iw.user),
            timestamp: iw.timestamp,
            msg_type: iw.msg_type,
        };

        self.interact_word_list.push_back(processed.clone());
        if self.interact_word_list.len() > MAX_INTERACT_WORD_LIST {
            self.interact_word_list.pop_front();
        }

        self.pending_interact_words.push(processed);
    }

    /// 设置贡献排行完整列表
    pub fn set_contribution_rank_full(&mut self, rank: Vec<ContributionRankUser>) {
        self.contribution_rank_full = rank.clone();
        self.pending_updates
            .push(DataUpdate::ContributionRankFull(rank));
    }

    // ==================== 内部方法 ====================

    /// 更新用户贡献
    fn update_user_contribution(
        &mut self,
        uid: u64,
        name: &str,
        face: Option<&str>,
        value: u64,
        guard_level: &GuardLevel,
    ) {
        let entry = self
            .user_contributions
            .entry(uid)
            .or_insert_with(|| UserContribution {
                uid,
                name: name.to_string(),
                face: face.map(String::from),
                total_value: 0,
                guard_level: guard_level_to_u8(guard_level),
            });
        entry.total_value += value;
        entry.name = name.to_string();
        if let Some(f) = face {
            entry.face = Some(f.to_string());
        }
        self.contributions_dirty = true;
    }

    /// 记录礼物交易 ID。返回 `false` 表示该交易已经处理过。
    fn remember_gift_transaction(&mut self, transaction_id: &str) -> bool {
        if !self
            .seen_gift_transactions
            .insert(transaction_id.to_owned())
        {
            return false;
        }

        self.seen_gift_transaction_order
            .push_back(transaction_id.to_owned());
        while self.seen_gift_transaction_order.len() > MAX_SEEN_GIFT_TRANSACTIONS {
            if let Some(expired) = self.seen_gift_transaction_order.pop_front() {
                self.seen_gift_transactions.remove(&expired);
            }
        }
        true
    }

    fn take_next_gift_sequence(&mut self) -> u64 {
        let sequence = self.next_gift_sequence;
        self.next_gift_sequence = self.next_gift_sequence.wrapping_add(1);
        sequence
    }

    /// 重建礼物合并索引
    fn rebuild_gift_index(&mut self) {
        self.gift_merge_index.clear();
        for (i, gift) in self.gift_list.iter().enumerate() {
            self.gift_merge_index.insert(gift.merge_key.clone(), i);
        }
    }

    // ==================== 点播请求（代理到 Manager）====================

    /// 更新点播请求的视频信息
    pub fn update_video_request_info(&mut self, request_id: &str, info: Result<VideoInfo, String>) {
        if let Some(update) = self.video_requests.update_info(request_id, info) {
            self.pending_updates.push(update);
        }
    }

    /// 标记点播为已看/未看
    pub fn set_video_watched(&mut self, request_id: &str, watched: bool) {
        let update = self.video_requests.set_watched(request_id, watched);
        self.pending_updates.push(update);
    }

    /// 删除点播请求
    pub fn remove_video_request(&mut self, request_id: &str) {
        let update = self.video_requests.remove(request_id);
        self.pending_updates.push(update);
    }

    /// 清空已看的点播
    pub fn clear_watched_videos(&mut self) {
        let update = self.video_requests.clear_watched();
        self.pending_updates.push(update);
    }

    /// 清空所有点播
    pub fn clear_all_videos(&mut self) {
        let update = self.video_requests.clear_all();
        self.pending_updates.push(update);
    }

    // ==================== 更新收集 ====================

    /// 获取待发送的更新，并清空缓冲区
    pub fn take_pending_updates(&mut self) -> Vec<DataUpdate> {
        let mut updates = std::mem::take(&mut self.pending_updates);

        if !self.pending_danmaku.is_empty() {
            updates.push(DataUpdate::DanmakuAppend(std::mem::take(
                &mut self.pending_danmaku,
            )));
        }

        if !self.pending_gift_upserts.is_empty() {
            updates.push(DataUpdate::GiftUpsert(std::mem::take(
                &mut self.pending_gift_upserts,
            )));
        }

        if !self.pending_interact_words.is_empty() {
            updates.push(DataUpdate::InteractWordAppend(std::mem::take(
                &mut self.pending_interact_words,
            )));
        }

        if self.stats_dirty {
            updates.push(DataUpdate::StatsUpdate(self.stats.clone()));
            self.stats_dirty = false;
        }

        if self.contributions_dirty {
            let mut contributions: Vec<_> = self.user_contributions.values().cloned().collect();
            contributions.sort_by(|a, b| b.total_value.cmp(&a.total_value));
            contributions.truncate(50);
            updates.push(DataUpdate::ContributionsUpdate(contributions));
            self.contributions_dirty = false;
        }

        // 检查定时结束的投票
        let expired = self.voting.check_expired_polls();
        updates.extend(expired);

        updates
    }
}

fn convert_gift_combo(gift: &Gift) -> Option<ProcessedGiftCombo> {
    Some(ProcessedGiftCombo {
        batch_combo_id: gift.batch_combo_id.clone()?,
        combo_total_coin: gift.combo_total_coin,
        super_batch_gift_num: gift.super_batch_gift_num,
        combo_resources_id: gift.combo_resources_id,
        combo_stay_time: gift.combo_stay_time,
        show_batch_combo_send: gift.show_batch_combo_send,
    })
}

fn combo_has_progress_marker(gift: &Gift) -> bool {
    gift.combo_total_coin.is_some_and(|value| value > 0)
        || gift.combo_total_num().is_some_and(|value| value > 0)
}

fn combo_snapshot_progresses(existing: &ProcessedGift, gift: &Gift) -> bool {
    let existing_combo = existing.combo.as_ref();
    let coin_progressed = gift
        .combo_total_coin
        .filter(|value| *value > 0)
        .is_some_and(|incoming| {
            existing_combo
                .and_then(|combo| combo.combo_total_coin)
                .map_or(true, |current| incoming > current)
        });
    let num_progressed = gift
        .combo_total_num()
        .filter(|value| *value > 0)
        .is_some_and(|incoming| incoming > u64::from(existing.num));

    coin_progressed || num_progressed
}

fn merge_gift_combo(
    existing: &mut Option<ProcessedGiftCombo>,
    incoming: Option<ProcessedGiftCombo>,
) {
    let Some(incoming) = incoming else {
        return;
    };
    let Some(existing) = existing.as_mut() else {
        *existing = Some(incoming);
        return;
    };

    existing.combo_total_coin = max_optional(existing.combo_total_coin, incoming.combo_total_coin);
    existing.super_batch_gift_num = max_optional(
        existing.super_batch_gift_num,
        incoming.super_batch_gift_num,
    );
    if let Some(value) = incoming.combo_resources_id {
        if value != 0 || existing.combo_resources_id.is_none() {
            existing.combo_resources_id = Some(value);
        }
    }
    if let Some(value) = incoming.combo_stay_time {
        if value != 0 || existing.combo_stay_time.is_none() {
            existing.combo_stay_time = Some(value);
        }
    }
    if incoming.show_batch_combo_send.is_some() {
        existing.show_batch_combo_send = incoming.show_batch_combo_send;
    }
}

fn max_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn saturating_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::LiveData;
    use crate::live_types::UpsertAction;
    use blivedm::Gift;
    use serde_json::json;

    fn blind_gift_fixture() -> Gift {
        let raw: serde_json::Value = serde_json::from_str(include_str!(
            "../../crates/blivedm/blind_gift.fixture.json"
        ))
        .expect("valid blind gift fixture");
        Gift::parse(&raw).expect("blind gift should parse")
    }

    fn regular_gift(transaction_id: Option<&str>) -> Gift {
        let mut data = json!({
            "giftId": 1,
            "giftName": "辣条",
            "num": 1,
            "price": 100,
            "total_coin": 100,
            "coin_type": "gold",
            "uid": 42,
            "uname": "tester",
            "timestamp": 1_700_000_000,
            "blind_gift": null
        });
        if let Some(transaction_id) = transaction_id {
            data["tid"] = json!(transaction_id);
        }
        Gift::parse(&json!({ "data": data })).expect("regular gift")
    }

    #[test]
    fn keeps_ordinary_gifts_independent_without_transaction_ids() {
        let gift = regular_gift(None);
        let mut data = LiveData::default();

        data.process_gift(gift.clone());
        data.process_gift(gift);

        assert_eq!(data.gift_list.len(), 2);
        assert_ne!(data.gift_list[0].id, data.gift_list[1].id);
        assert_eq!(data.stats.gift_revenue, 2);
    }

    #[test]
    fn ignores_duplicate_gift_transactions() {
        let gift = regular_gift(Some("txn-1"));
        let mut data = LiveData::default();

        data.process_gift(gift.clone());
        data.process_gift(gift);

        assert_eq!(data.gift_list.len(), 1);
        assert_eq!(data.stats.gift_revenue, 1);
    }

    #[test]
    fn ignores_non_progressing_combo_snapshot_without_transaction_id() {
        let mut gift = regular_gift(None);
        gift.batch_combo_id = Some("combo-without-tid".to_owned());
        gift.combo_total_coin = Some(100);
        gift.super_batch_gift_num = Some(1);
        let mut data = LiveData::default();

        data.process_gift(gift.clone());
        data.process_gift(gift);

        assert_eq!(data.gift_list.len(), 1);
        assert_eq!(data.stats.gift_revenue, 1);
    }

    #[test]
    fn merges_only_matching_batch_combo_and_uses_cumulative_totals() {
        let mut first = regular_gift(Some("combo-txn-1"));
        first.batch_combo_id = Some("combo-1".to_owned());
        first.combo_total_coin = Some(100);
        first.super_batch_gift_num = Some(1);

        let mut second = first.clone();
        second.transaction_id = Some("combo-txn-2".to_owned());
        second.timestamp += 1;
        second.combo_total_coin = Some(200);
        second.super_batch_gift_num = Some(2);

        let mut other_combo = second.clone();
        other_combo.transaction_id = Some("combo-txn-3".to_owned());
        other_combo.batch_combo_id = Some("combo-2".to_owned());
        other_combo.combo_total_coin = Some(100);
        other_combo.super_batch_gift_num = Some(1);

        let mut data = LiveData::default();
        data.process_gift(first);
        data.process_gift(other_combo);
        data.process_gift(second);

        assert_eq!(data.gift_list.len(), 2);
        let processed = data.gift_list.back().expect("updated combo gift");
        assert!(processed.merge_key.ends_with("combo-1"));
        assert_eq!(processed.num, 2);
        assert_eq!(processed.total_value, 2);
        assert_eq!(processed.revenue_value, 2);
        assert_eq!(data.stats.gift_revenue, 3);
        assert!(matches!(
            &data.pending_gift_upserts[0].action,
            UpsertAction::Insert
        ));
        assert!(matches!(
            &data.pending_gift_upserts[1].action,
            UpsertAction::Insert
        ));
        assert!(matches!(
            &data.pending_gift_upserts[2].action,
            UpsertAction::Update
        ));
    }

    #[test]
    fn processes_and_merges_blind_gift_values() {
        let first = blind_gift_fixture();
        let mut second = first.clone();
        second.transaction_id = Some("blind-txn-2".to_owned());
        second.timestamp += 1;
        second.combo_total_coin = Some(32_000);
        second.super_batch_gift_num = Some(2);
        second
            .batch_combo_send
            .as_mut()
            .unwrap()
            .batch_combo_num = 2;

        let mut data = LiveData::default();
        data.process_gift(first);
        data.process_gift(second);

        assert_eq!(data.gift_list.len(), 1);
        let processed = data.gift_list.front().expect("processed gift");
        assert_eq!(processed.gift_name, "爱心抱枕");
        assert_eq!(processed.num, 2);
        assert_eq!(processed.total_value, 320);
        assert_eq!(processed.revenue_value, 300);
        assert_eq!(
            processed.blind_gift.as_ref().map(|gift| gift.gift_id),
            Some(32251)
        );
        assert_eq!(
            processed
                .blind_gift
                .as_ref()
                .map(|gift| gift.gift_name.as_str()),
            Some("心动盲盒")
        );
        assert_eq!(
            processed.blind_gift.as_ref().map(|gift| gift.total_value),
            Some(300)
        );
        assert_eq!(data.stats.gift_revenue, 300);
        assert_eq!(data.stats.total_revenue, 300);
    }
}
