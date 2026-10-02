//! 录制资源的唯一所有者。发送端不能被克隆或取出，会话结束由写入任务确认。
use std::{sync::Arc, time::Duration};

use tokio::{sync::mpsc, task::JoinHandle};

use super::ArchiveManager;
use crate::live_types::{
    LiveRecord, LiveStats, ProcessedDanmaku, ProcessedGift, ProcessedSuperChat,
};

// 队列几乎全部是记录；保留按值传递，避免仅为一条 Finish 消息让每条记录额外分配。
#[allow(clippy::large_enum_variant)]
enum Message {
    Record(LiveRecord),
    Finish(LiveStats),
}

pub struct Recording {
    sender: mpsc::UnboundedSender<Message>,
    task: JoinHandle<Result<(), String>>,
}

impl Recording {
    pub async fn start(
        archive: Arc<ArchiveManager>,
        room_id: u64,
        room_title: &str,
        streamer_uid: u64,
    ) -> Result<Self, String> {
        let session_id = archive
            .start_session(room_id, room_title, streamer_uid)
            .await?;
        // 创建会话与启动写入任务之间没有挂起点，取消调用不会留下无人持有的录制。
        let (sender, receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(archive, session_id, receiver));
        Ok(Self { sender, task })
    }

    pub fn record(&self, record: LiveRecord) -> Result<(), String> {
        self.sender
            .send(Message::Record(record))
            .map_err(|_| "存档写入任务已停止".to_owned())
    }

    /// 消费所有权后不能再提交事件。即使等待方被取消，任务也会独立完成排空和结算。
    pub async fn finish(self, stats: LiveStats) -> Result<(), String> {
        let Self { sender, task } = self;
        let sent = sender.send(Message::Finish(stats));
        drop(sender);
        task.await
            .map_err(|error| format!("存档写入任务异常结束: {error}"))??;
        sent.map_err(|_| "存档写入任务已停止".to_owned())
    }
}

#[derive(Default)]
struct Buffer {
    danmaku: Vec<ProcessedDanmaku>,
    gifts: Vec<ProcessedGift>,
    superchats: Vec<ProcessedSuperChat>,
}

impl Buffer {
    fn push(&mut self, record: LiveRecord) {
        match record {
            LiveRecord::Danmaku(message) => self.danmaku.push(message),
            LiveRecord::Gift(gift) => self.gifts.push(gift),
            LiveRecord::SuperChat(message) => self.superchats.push(message),
        }
    }

    fn is_full(&self) -> bool {
        self.danmaku.len() >= 100 || self.gifts.len() >= 50 || self.superchats.len() >= 20
    }

    async fn flush(
        &mut self,
        archive: &ArchiveManager,
        session_id: i64,
        failure: &mut Option<String>,
    ) {
        if !self.danmaku.is_empty() {
            let items = std::mem::take(&mut self.danmaku);
            remember_failure(
                failure,
                archive.save_danmaku_batch(session_id, &items).await,
            );
        }
        for gift in self.gifts.drain(..) {
            remember_failure(failure, archive.save_gift(session_id, &gift).await);
        }
        for message in self.superchats.drain(..) {
            remember_failure(failure, archive.save_superchat(session_id, &message).await);
        }
    }
}

fn remember_failure(failure: &mut Option<String>, result: Result<(), String>) {
    if let Err(error) = result {
        log::error!("存档写入失败: {error}");
        failure.get_or_insert(error);
    }
}

async fn run(
    archive: Arc<ArchiveManager>,
    session_id: i64,
    mut receiver: mpsc::UnboundedReceiver<Message>,
) -> Result<(), String> {
    let mut buffer = Buffer::default();
    let mut failure = None;
    // 保持原有 500ms 空闲刷新和各类型批量阈值，不改变写入/查询的粒度。
    let final_stats = loop {
        match tokio::time::timeout(Duration::from_millis(500), receiver.recv()).await {
            Ok(Some(Message::Record(record))) => {
                buffer.push(record);
                if buffer.is_full() {
                    buffer.flush(&archive, session_id, &mut failure).await;
                }
            }
            Ok(Some(Message::Finish(stats))) => break Some(stats),
            Ok(None) => {
                log::warn!("录制 {} 未显式结束，排空后按落盘记录恢复统计", session_id);
                break None;
            }
            Err(_) => buffer.flush(&archive, session_id, &mut failure).await,
        }
    };
    buffer.flush(&archive, session_id, &mut failure).await;
    remember_failure(
        &mut failure,
        archive
            .finish_session(session_id, final_stats.as_ref())
            .await,
    );
    log::info!("Archive writer task exited for session {}", session_id);
    failure.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_data::LiveData;
    use serde_json::json;

    fn record(index: u64) -> LiveRecord {
        let raw = json!({"cmd":"SUPER_CHAT_MESSAGE","data":{
            "id":index,"message":"录制测试","price":30,"uid":42,
            "user_info":{"uname":"观众"},"start_time":1700000000,"time":60
        }});
        LiveData::default()
            .process(blivedm::parse_notification(&serde_json::to_vec(&raw).unwrap(), None).unwrap())
            .records
            .pop()
            .unwrap()
    }

    async fn new_recording() -> (Arc<ArchiveManager>, Recording, i64) {
        let archive = Arc::new(ArchiveManager::new(":memory:".into()).unwrap());
        let recording = Recording::start(archive.clone(), 1, "测试", 42)
            .await
            .unwrap();
        let id = archive.get_sessions().await.unwrap()[0].id;
        (archive, recording, id)
    }

    #[tokio::test]
    async fn finish_waits_for_blocked_writer_and_all_batches_before_counting() {
        let (archive, recording, id) = new_recording().await;
        let database = archive.db.lock().await;
        for index in 0..137 {
            recording.record(record(index)).unwrap();
        }
        let (started, ready) = tokio::sync::oneshot::channel();
        let finish = tokio::spawn(async move {
            let _ = started.send(());
            recording
                .finish(LiveStats {
                    sc_revenue: 41_100,
                    total_revenue: 41_100,
                    ..Default::default()
                })
                .await
        });
        ready.await.unwrap();
        tokio::task::yield_now().await;
        assert!(!finish.is_finished());
        assert!(database.active_sessions.contains(&id));
        drop(database);
        finish.await.unwrap().unwrap();
        let saved = archive.get_session_detail(id).await.unwrap();
        assert_eq!(saved.sc_count, 137);
        assert_eq!(saved.sc_revenue, 41_100);
        assert!(saved.end_time.is_some());
        archive.delete_session(id).await.unwrap();
    }

    #[tokio::test]
    async fn cancelling_finish_waiter_does_not_cancel_finalization() {
        let (archive, recording, id) = new_recording().await;
        recording.record(record(1)).unwrap();
        let database = archive.db.lock().await;
        {
            let finish = recording.finish(LiveStats {
                total_revenue: 999,
                ..Default::default()
            });
            tokio::pin!(finish);
            // 先轮询 finish，确保结束消息已提交，然后取消等待方。
            tokio::select! {
                biased;
                result = &mut finish => panic!("数据库尚未释放: {result:?}"),
                _ = std::future::ready(()) => {},
            }
        }
        drop(database);
        let saved = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let saved = archive.get_session_detail(id).await.unwrap();
                if saved.end_time.is_some() {
                    break saved;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(saved.sc_count, 1);
        assert_eq!(saved.total_revenue, 999);
        archive.delete_session(id).await.unwrap();
    }

    #[tokio::test]
    async fn closing_sender_without_finish_drains_and_recovers_persisted_revenue() {
        let (archive, recording, id) = new_recording().await;
        recording.record(record(1)).unwrap();
        let Recording { sender, task } = recording;
        drop(sender);
        task.await.unwrap().unwrap();
        let saved = archive.get_session_detail(id).await.unwrap();
        assert!(saved.end_time.is_some());
        assert_eq!(
            (saved.sc_count, saved.sc_revenue, saved.total_revenue),
            (1, 300, 300)
        );
    }

    #[tokio::test]
    async fn write_failure_is_reported_and_other_records_are_still_drained() {
        let (archive, recording, id) = new_recording().await;
        archive
            .db
            .lock()
            .await
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_one_sc BEFORE INSERT ON super_chats
             WHEN NEW.original_id = 'sc_1' BEGIN SELECT RAISE(FAIL, '模拟写入失败'); END;",
            )
            .unwrap();
        recording.record(record(1)).unwrap();
        recording.record(record(2)).unwrap();
        let error = recording.finish(LiveStats::default()).await.unwrap_err();
        assert!(error.contains("模拟写入失败"));
        let saved = archive.get_session_detail(id).await.unwrap();
        assert_eq!(saved.sc_count, 1);
        assert!(saved.end_time.is_some());
        archive.delete_session(id).await.unwrap();
    }

    #[tokio::test]
    async fn recovery_and_cleanup_ignore_active_recordings_and_finish_uses_its_own_id() {
        let (archive, first, first_id) = new_recording().await;
        let second = Recording::start(archive.clone(), 2, "第二场", 43)
            .await
            .unwrap();
        let second_id = archive
            .get_sessions()
            .await
            .unwrap()
            .into_iter()
            .find(|session| session.room_id == 2)
            .unwrap()
            .id;
        {
            let database = archive.db.lock().await;
            database
                .connection
                .execute(
                    "INSERT INTO sessions (id, room_id, start_time) VALUES (99, 99, 1700000000)",
                    [],
                )
                .unwrap();
        }
        assert_eq!(archive.recover_orphaned_sessions().await.unwrap(), 1);
        assert!(archive
            .get_session_detail(first_id)
            .await
            .unwrap()
            .end_time
            .is_none());
        assert!(archive
            .get_session_detail(second_id)
            .await
            .unwrap()
            .end_time
            .is_none());
        assert_eq!(archive.prune_empty_sessions().await.unwrap(), 1);
        assert!(archive.delete_session(first_id).await.is_err());
        assert!(archive.delete_session(second_id).await.is_err());
        first.record(record(1)).unwrap();
        first
            .finish(LiveStats {
                total_revenue: 300,
                sc_revenue: 300,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            archive.get_session_detail(first_id).await.unwrap().sc_count,
            1
        );
        assert!(archive
            .get_session_detail(second_id)
            .await
            .unwrap()
            .end_time
            .is_none());
        second.finish(LiveStats::default()).await.unwrap();
        assert_eq!(
            archive
                .get_session_detail(second_id)
                .await
                .unwrap()
                .total_revenue,
            0
        );
        assert_eq!(archive.prune_empty_sessions().await.unwrap(), 1);
    }
}
