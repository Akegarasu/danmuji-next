//! 一次直播的聚合与录制所有权；网络和手动输入共用此路径。
use crate::archive::Recording;
use crate::live_data::LiveData;
use crate::live_events::LiveEvent;
use crate::live_types::ConnectionStatus;

pub struct LiveSession {
    pub data: LiveData,
    pub status: ConnectionStatus,
    pub room_info: Option<blivedm::api::RoomInfo>,
    pub generation: u64,
    pub streamer_uid: u64,
    recording: Option<Recording>,
}

impl Default for LiveSession {
    fn default() -> Self {
        Self {
            data: LiveData::default(),
            status: ConnectionStatus::Disconnected,
            room_info: None,
            generation: 0,
            streamer_uid: 0,
            recording: None,
        }
    }
}

impl LiveSession {
    pub fn start_recording(&mut self, recording: Recording) {
        assert!(self.recording.is_none(), "旧录制必须先结束");
        self.recording = Some(recording);
    }

    /// 聚合和入队在同一状态锁内完成，结束录制不会越过尚未提交的事件。
    pub fn process(&mut self, event: blivedm::Event) -> Vec<LiveEvent> {
        let effects = self.data.process(event);
        if let Some(recording) = &self.recording {
            for record in effects.records {
                if let Err(error) = recording.record(record) {
                    log::error!("提交存档事件失败: {error}");
                }
            }
        }
        // 房间状态和待发布更新在同一个状态锁内推进，订阅者不会读到旧房间状态。
        for event in &effects.events {
            if let Some(room) = &mut self.room_info {
                match event {
                    LiveEvent::Started { .. } => room.live_status = 1,
                    LiveEvent::Stopped { round, .. } => {
                        room.live_status = if *round == 1 { 2 } else { 0 }
                    }
                    _ => {}
                }
            }
        }
        effects.events
    }

    /// 同步截取统计并封闭入口；等待写入完成时不持有直播状态锁。
    pub fn finish_recording(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send + 'static {
        let recording = self.recording.take();
        let stats = self.data.stats.clone();
        async move {
            if let Some(recording) = recording {
                recording.finish(stats).await?;
            }
            Ok(())
        }
    }
}
