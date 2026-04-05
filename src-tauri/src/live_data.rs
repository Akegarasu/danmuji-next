//! 直播数据状态管理
//!
//! LiveData 持有所有实时直播数据（弹幕、礼物、SC、统计等），
//! 负责事件处理、数据聚合、礼物合并、点播检测等逻辑。

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::LazyLock;

use regex::Regex;
use tokio::sync::mpsc;

use crate::archive::ArchiveEvent;
use crate::blivedm::api::ContributionRankUser;
use crate::blivedm::{
    CoinType, Danmaku, DanmakuType, Gift, GuardBuy, GuardLevel, OnlineRankCount, OnlineRankV2,
    SuperChat,
};
use crate::live_types::*;
use crate::video_info::VideoInfo;

/// BV/AV 号匹配正则（编译一次，全局复用）
static VIDEO_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(BV[a-zA-Z0-9]{10}|av\d+)").unwrap());

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
    /// SC 列表
    pub(crate) superchat_list: Vec<ProcessedSuperChat>,
    /// 高能用户排行
    pub(crate) online_rank: Vec<ProcessedOnlineRankUser>,
    /// 贡献排行完整列表（API）
    pub(crate) contribution_rank_full: Vec<ContributionRankUser>,
    /// 用户贡献 map: uid -> contribution
    user_contributions: HashMap<u64, UserContribution>,
    /// 统计数据
    pub(crate) stats: LiveStats,

    /// 点播请求列表
    pub(crate) video_requests: Vec<VideoRequestItem>,
    /// 已见过的视频 ID（用于去重，小写）
    video_request_ids: HashSet<String>,

    /// 待发送的更新
    pub(crate) pending_updates: Vec<DataUpdate>,
    /// 待发送的弹幕（批量）
    pending_danmaku: Vec<ProcessedDanmaku>,
    /// 待发送的礼物更新（批量）
    pending_gift_upserts: Vec<GiftUpsert>,
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
            superchat_list: Vec::with_capacity(MAX_SUPERCHAT_LIST),
            online_rank: Vec::new(),
            contribution_rank_full: Vec::new(),
            user_contributions: HashMap::new(),
            stats: LiveStats::default(),
            video_requests: Vec::new(),
            video_request_ids: HashSet::new(),
            pending_updates: Vec::new(),
            pending_danmaku: Vec::new(),
            pending_gift_upserts: Vec::new(),
            stats_dirty: false,
            contributions_dirty: false,
            archive_tx: None,
        }
    }
}

impl LiveData {
    /// 清空所有数据
    pub fn clear(&mut self) {
        *self = Self::default();
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
                Some(self.video_requests.clone())
            } else {
                None
            },
        }
    }

    // ==================== 事件处理 ====================

    /// 处理弹幕，返回需要异步获取视频信息的列表
    pub fn process_danmaku(
        &mut self,
        danmaku: Danmaku,
    ) -> Vec<(String, String, u64, Option<u64>)> {
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

        let detected = self.detect_and_add_video_requests(
            &processed.content,
            &processed.user.name,
            processed.user.uid,
            VideoRequestSource::Danmaku,
            None,
            processed.timestamp,
        );

        self.pending_danmaku.push(processed);
        detected
    }

    /// 处理礼物
    pub fn process_gift(&mut self, gift: Gift) {
        let time_bucket = gift.timestamp / GIFT_MERGE_WINDOW_SECS;
        let merge_key = format!("{}_{}_{}", gift.sender_uid, gift.gift_id, time_bucket);

        let value = if gift.coin_type == CoinType::Gold {
            gift.price as u64 / 100
        } else {
            0
        };
        let is_paid = gift.coin_type == CoinType::Gold;

        let sender_uid = gift.sender_uid;
        let sender_name = gift.sender_name.clone();
        let sender_face = gift.sender_face.clone();
        let guard_level = gift.guard_level.clone();

        if let Some(&index) = self.gift_merge_index.get(&merge_key) {
            if let Some(existing) = self.gift_list.get_mut(index) {
                existing.num += gift.num;
                existing.total_value += value;
                existing.timestamp = gift.timestamp;

                self.pending_gift_upserts.push(GiftUpsert {
                    merge_key: merge_key.clone(),
                    gift: existing.clone(),
                    action: UpsertAction::Update,
                });
            }
        } else {
            let processed = ProcessedGift {
                id: format!("gift_{}_{}", gift.timestamp, gift.sender_uid),
                merge_key: merge_key.clone(),
                gift_id: gift.gift_id,
                gift_name: gift.gift_name,
                gift_icon: gift.gift_icon,
                num: gift.num,
                total_value: value,
                is_paid,
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

            if let Some(tx) = &self.archive_tx {
                let _ = tx.send(ArchiveEvent::Gift(processed.clone()));
            }

            let index = self.gift_list.len();
            self.gift_list.push_back(processed.clone());
            self.gift_merge_index.insert(merge_key.clone(), index);

            if self.gift_list.len() > MAX_GIFT_LIST {
                self.gift_list.pop_front();
                self.rebuild_gift_index();
            }

            self.pending_gift_upserts.push(GiftUpsert {
                merge_key,
                gift: processed,
                action: UpsertAction::Insert,
            });
        }

        if is_paid && value > 0 {
            self.stats.gift_revenue += value;
            self.stats.total_revenue += value;
            self.stats_dirty = true;

            self.update_user_contribution(
                sender_uid,
                &sender_name,
                sender_face.as_deref(),
                value,
                &guard_level,
            );
        }
    }

    /// 处理 SC，返回需要异步获取视频信息的列表
    pub fn process_superchat(
        &mut self,
        sc: SuperChat,
    ) -> Vec<(String, String, u64, Option<u64>)> {
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

        let detected = self.detect_and_add_video_requests(
            &processed.content,
            &processed.user.name,
            processed.user.uid,
            VideoRequestSource::Superchat,
            Some(price),
            processed.start_time,
        );

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

        let id = format!("guard_{}_{}", timestamp, guard.uid);
        let merge_key = id.clone();

        let processed = ProcessedGift {
            id,
            merge_key: merge_key.clone(),
            gift_id: guard.gift_id,
            gift_name: guard.guard_name().to_string(),
            gift_icon: "".to_string(),
            num: guard.num,
            total_value: value,
            is_paid: true,
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
        self.online_rank = rank
            .online_list
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

    /// 重建礼物合并索引
    fn rebuild_gift_index(&mut self) {
        self.gift_merge_index.clear();
        for (i, gift) in self.gift_list.iter().enumerate() {
            self.gift_merge_index.insert(gift.merge_key.clone(), i);
        }
    }

    // ==================== 点播请求 ====================

    /// 从文本中检测 BV/AV号，创建点播请求
    /// 返回需要异步获取视频信息的列表: (request_id, video_id, uid, sc_price)
    fn detect_and_add_video_requests(
        &mut self,
        content: &str,
        username: &str,
        uid: u64,
        source: VideoRequestSource,
        sc_price: Option<u64>,
        timestamp: i64,
    ) -> Vec<(String, String, u64, Option<u64>)> {
        let mut to_fetch = Vec::new();

        for cap in VIDEO_ID_RE.captures_iter(content) {
            let video_id = cap[1].to_string();
            let key = video_id.to_lowercase();

            if self.video_request_ids.contains(&key) {
                continue;
            }
            self.video_request_ids.insert(key);

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

            self.video_requests.insert(0, item.clone());
            self.pending_updates
                .push(DataUpdate::VideoRequestAppend(item));
            to_fetch.push((id, video_id, uid, sc_price));
        }

        to_fetch
    }

    /// 更新点播请求的视频信息
    pub fn update_video_request_info(&mut self, request_id: &str, info: Result<VideoInfo, String>) {
        if let Some(item) = self.video_requests.iter_mut().find(|r| r.id == request_id) {
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
            self.pending_updates
                .push(DataUpdate::VideoRequestUpdate(item.clone()));
        }
    }

    /// 标记点播为已看/未看
    pub fn set_video_watched(&mut self, request_id: &str, watched: bool) {
        if let Some(item) = self.video_requests.iter_mut().find(|r| r.id == request_id) {
            item.watched = watched;
        }
        self.pending_updates
            .push(DataUpdate::VideoRequestSync(self.video_requests.clone()));
    }

    /// 删除点播请求
    pub fn remove_video_request(&mut self, request_id: &str) {
        self.video_requests.retain(|r| r.id != request_id);
        self.pending_updates
            .push(DataUpdate::VideoRequestSync(self.video_requests.clone()));
    }

    /// 清空已看的点播
    pub fn clear_watched_videos(&mut self) {
        self.video_requests.retain(|r| !r.watched);
        self.pending_updates
            .push(DataUpdate::VideoRequestSync(self.video_requests.clone()));
    }

    /// 清空所有点播
    pub fn clear_all_videos(&mut self) {
        self.video_requests.clear();
        self.video_request_ids.clear();
        self.pending_updates
            .push(DataUpdate::VideoRequestSync(self.video_requests.clone()));
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

        updates
    }
}
