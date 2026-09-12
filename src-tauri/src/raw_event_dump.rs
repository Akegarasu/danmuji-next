//! 应用侧的 blivedm 原始事件调试输出。

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use serde::Serialize;
use serde_json::Value;

const MAX_DUMP_BYTES: u64 = 100 * 1024 * 1024;
const DUMP_QUEUE_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Default, Serialize)]
pub struct RawDumpStatus {
    pub recording: bool,
    pub path: Option<String>,
    pub event_count: u64,
    pub bytes_written: u64,
    pub dropped_events: u64,
    pub error: Option<String>,
}

struct DumpSession {
    sender: mpsc::SyncSender<String>,
    writer: JoinHandle<()>,
    status: Arc<Mutex<RawDumpStatus>>,
}

impl DumpSession {
    fn finish(self) -> RawDumpStatus {
        // 关闭发送端后排空队列，返回时文件已经 flush。
        drop(self.sender);
        let joined = self.writer.join();
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner()).clone();
        status.recording = false;
        if joined.is_err() {
            status.error = Some("日志写入任务异常退出，文件可能不完整".to_owned());
        }
        status
    }
}

#[derive(Default)]
struct DumpRecorder {
    session: Option<DumpSession>,
    last_status: RawDumpStatus,
}

impl DumpRecorder {
    fn status(&self) -> RawDumpStatus {
        self.session.as_ref().map_or_else(
            || self.last_status.clone(),
            |session| {
                session
                    .status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
            },
        )
    }

    fn start(&mut self, directory: &Path) -> Result<RawDumpStatus, String> {
        if self.status().recording {
            return Ok(self.status());
        }
        self.stop();
        std::fs::create_dir_all(directory).map_err(|e| format!("创建日志目录失败: {e}"))?;
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = directory.join(format!(
            "raw_dump_{}_{}_{}.txt",
            chrono::Local::now().format("%Y%m%d_%H%M%S_%3f"),
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| format!("创建 dump 文件失败: {e}"))?;
        let status = Arc::new(Mutex::new(RawDumpStatus {
            recording: true,
            path: Some(path.to_string_lossy().into_owned()),
            ..RawDumpStatus::default()
        }));
        let (sender, receiver) = mpsc::sync_channel(DUMP_QUEUE_CAPACITY);
        let writer_status = status.clone();
        let writer = std::thread::Builder::new()
            .name("raw-event-dump".to_owned())
            .spawn(move || write_dump(file, receiver, writer_status, MAX_DUMP_BYTES))
            .map_err(|e| format!("启动日志写入失败: {e}"))?;
        self.session = Some(DumpSession {
            sender,
            writer,
            status,
        });
        Ok(self.status())
    }

    fn record(&self, value: &Value) {
        let Some(session) = &self.session else {
            return;
        };
        if !session
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recording
        {
            return;
        }
        let line = format!(
            "[{}] {value}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
        );
        // 只入队，不在 WebSocket 的解析任务里写文件；队列满时明确报告不完整。
        if let Err(error) = session.sender.try_send(line) {
            let mut status = session.status.lock().unwrap_or_else(|e| e.into_inner());
            status.dropped_events += 1;
            if matches!(error, mpsc::TrySendError::Full(_)) {
                status.error = Some(format!(
                    "日志写入跟不上接收速度，已漏记 {} 条消息",
                    status.dropped_events
                ));
            } else {
                status.recording = false;
                if status.error.is_none() {
                    status.error = Some("日志写入任务已停止，文件可能不完整".to_owned());
                }
            }
        }
    }

    fn stop(&mut self) -> RawDumpStatus {
        if let Some(session) = self.session.take() {
            self.last_status = session.finish();
        }
        self.last_status.clone()
    }
}

impl Drop for DumpRecorder {
    fn drop(&mut self) {
        self.stop();
    }
}

fn write_dump(
    file: File,
    receiver: mpsc::Receiver<String>,
    status: Arc<Mutex<RawDumpStatus>>,
    limit: u64,
) {
    let mut writer = BufWriter::new(file);
    for line in receiver {
        let bytes = line.len() as u64;
        if status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .bytes_written
            + bytes
            > limit
        {
            let mut state = status.lock().unwrap_or_else(|e| e.into_inner());
            state.error = Some("日志已达到大小上限，已自动停止；可开始保存新的文件".to_owned());
            break;
        }
        if let Err(error) = writer.write_all(line.as_bytes()) {
            status.lock().unwrap_or_else(|e| e.into_inner()).error =
                Some(format!("写入 dump 失败: {error}"));
            break;
        }
        let mut state = status.lock().unwrap_or_else(|e| e.into_inner());
        state.event_count += 1;
        state.bytes_written += bytes;
    }
    // 不完整原因也写入文件，用户只提供 dump 时仍能判断是否漏记。
    let snapshot = status.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(error) = snapshot.error {
        let notice = serde_json::json!({
            "cmd": "DANMUJI_DUMP_STATUS",
            "data": { "error": error, "dropped_events": snapshot.dropped_events }
        });
        let line = format!(
            "[{}] {notice}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
        );
        if writer.write_all(line.as_bytes()).is_ok() {
            status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .bytes_written += line.len() as u64;
        }
    }
    let result = writer.flush().and_then(|_| writer.get_ref().sync_all());
    let mut state = status.lock().unwrap_or_else(|e| e.into_inner());
    state.recording = false;
    if let Err(error) = result {
        state.error = Some(format!("保存 dump 失败: {error}"));
    }
}

fn recorder() -> &'static Mutex<DumpRecorder> {
    static RECORDER: OnceLock<Mutex<DumpRecorder>> = OnceLock::new();
    RECORDER.get_or_init(|| Mutex::new(DumpRecorder::default()))
}

pub fn directory() -> PathBuf {
    crate::config::get_config_dir().join("dumps")
}

pub fn start() -> Result<RawDumpStatus, String> {
    recorder()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .start(&directory())
}

pub fn stop() -> RawDumpStatus {
    let (session, previous) = {
        let mut recorder = recorder().lock().unwrap_or_else(|e| e.into_inner());
        recorder.last_status = recorder.status();
        recorder.last_status.recording = false;
        (recorder.session.take(), recorder.last_status.clone())
    };
    let Some(session) = session else { return previous; };
    // 等待磁盘写入时不持有全局锁，避免阻塞仍在接收的直播消息。
    let status = session.finish();
    let mut recorder = recorder().lock().unwrap_or_else(|e| e.into_inner());
    if recorder.session.is_none() && recorder.last_status.path == status.path {
        recorder.last_status = status.clone();
    }
    status
}

pub fn status() -> RawDumpStatus {
    recorder()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .status()
}

/// 发布版也始终注册回调；是否保存由 UI 在运行时控制，无需重连。
pub fn dump(value: &Value) {
    recorder()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .record(value);
    if crate::is_dev_mode() {
        dump_dev(value);
    }
}

fn dump_file() -> &'static std::sync::Mutex<Option<std::fs::File>> {
    static DUMP_FILE: OnceLock<std::sync::Mutex<Option<std::fs::File>>> = OnceLock::new();

    DUMP_FILE.get_or_init(|| {
        let path = crate::config::get_config_dir().join("raw_dump.txt");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path);

        match file {
            Ok(file) => {
                eprintln!("[DEV] Raw dump file: {}", path.display());
                std::sync::Mutex::new(Some(file))
            }
            Err(error) => {
                eprintln!("[DEV] Failed to open raw dump file: {error}");
                std::sync::Mutex::new(None)
            }
        }
    })
}

fn dump_dev(value: &Value) {
    let guard = dump_file()
        .lock()
        .unwrap_or_else(|error| error.into_inner());

    if let Some(file) = guard.as_ref() {
        let mut writer = std::io::BufWriter::new(file);
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let json = serde_json::to_string(value).unwrap_or_default();
        let _ = writeln!(writer, "[{timestamp}] {json}");
        let _ = writer.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "danmuji-dump-test-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed),
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            for item in std::fs::read_dir(&self.0).unwrap() {
                std::fs::remove_file(item.unwrap().path()).unwrap();
            }
            std::fs::remove_dir(&self.0).unwrap();
        }
    }

    #[test]
    fn stopping_drains_records_and_starting_again_preserves_the_previous_file() {
        let directory = TestDirectory::new();
        let mut recorder = DumpRecorder::default();
        recorder.record(&json!({"cmd": "BEFORE_START"}));
        let first = recorder.start(&directory.0).unwrap();
        assert_eq!(recorder.start(&directory.0).unwrap().path, first.path);
        let raw: Value = serde_json::from_str(include_str!(
            "../../crates/blivedm/tests/fixtures/ten_blind_gift_v2.json"
        ))
        .unwrap();
        // 在任何业务解析前原样保留 pb，包括已知命令中无法解析的内容。
        recorder.record(&raw);
        recorder.record(&json!({"cmd": "SEND_GIFT_V2", "data": {"pb": "invalid!"}}));
        let stopped = recorder.stop();
        assert!(!stopped.recording);
        assert_eq!(stopped.event_count, 2);
        assert_eq!(stopped.dropped_events, 0);
        assert!(stopped.error.is_none());
        let content = std::fs::read_to_string(first.path.as_ref().unwrap()).unwrap();
        assert_eq!(content.len() as u64, stopped.bytes_written);
        let events: Vec<Value> = content
            .lines()
            .map(|line| serde_json::from_str(line.split_once("] ").unwrap().1).unwrap())
            .collect();
        assert_eq!(events[0], raw);
        assert_eq!(events[1]["data"]["pb"], "invalid!");
        recorder.record(&json!({"cmd": "AFTER_STOP"}));
        assert_eq!(
            std::fs::read_to_string(first.path.as_ref().unwrap()).unwrap(),
            content
        );

        let second = recorder.start(&directory.0).unwrap();
        assert_ne!(second.path, first.path);
        recorder.stop();
        assert_eq!(
            std::fs::read_to_string(first.path.as_ref().unwrap()).unwrap(),
            content
        );
    }

    #[test]
    fn size_limit_stops_at_a_complete_record_and_reports_why() {
        let directory = TestDirectory::new();
        let path = directory.0.join("limited.txt");
        let file = File::create(&path).unwrap();
        let status = Arc::new(Mutex::new(RawDumpStatus {
            recording: true,
            ..RawDumpStatus::default()
        }));
        let (tx, rx) = mpsc::sync_channel(2);
        let line = "[test] {\"cmd\":\"ONE\"}\n";
        tx.send(line.to_owned()).unwrap();
        tx.send(line.to_owned()).unwrap();
        drop(tx);
        write_dump(file, rx, status.clone(), line.len() as u64);
        let status = status.lock().unwrap();
        assert!(!status.recording);
        assert_eq!(status.event_count, 1);
        assert!(status.error.as_ref().unwrap().contains("上限"));
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.starts_with(line));
        assert!(content.contains("DANMUJI_DUMP_STATUS"));
    }

    #[test]
    fn file_write_failure_is_reported_to_the_ui() {
        let directory = TestDirectory::new();
        let path = directory.0.join("readonly.txt");
        std::fs::write(&path, "").unwrap();
        let file = File::open(&path).unwrap();
        let status = Arc::new(Mutex::new(RawDumpStatus {
            recording: true,
            ..RawDumpStatus::default()
        }));
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send("test\n".to_owned()).unwrap();
        drop(tx);
        write_dump(file, rx, status.clone(), 100);
        let status = status.lock().unwrap();
        assert!(!status.recording);
        assert!(status.error.as_ref().unwrap().contains("失败"));
    }

    #[test]
    fn queue_overflow_is_reported_without_blocking_event_processing() {
        let directory = TestDirectory::new();
        let path = directory.0.join("overflow.txt");
        let file = File::create(&path).unwrap();
        let status = Arc::new(Mutex::new(RawDumpStatus {
            recording: true,
            ..RawDumpStatus::default()
        }));
        let (sender, receiver) = mpsc::sync_channel(1);
        sender
            .send("[test] {\"cmd\":\"FIRST\"}\n".to_owned())
            .unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let writer_barrier = barrier.clone();
        let writer_status = status.clone();
        let writer = std::thread::spawn(move || {
            writer_barrier.wait();
            write_dump(file, receiver, writer_status, 10_000);
        });
        let mut recorder = DumpRecorder {
            session: Some(DumpSession {
                sender,
                writer,
                status,
            }),
            last_status: RawDumpStatus::default(),
        };
        recorder.record(&json!({"cmd": "SECOND"}));
        let observed = recorder.status();
        barrier.wait();
        recorder.stop();
        assert_eq!(observed.dropped_events, 1);
        assert!(observed.error.unwrap().contains("漏记 1 条"));
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("DANMUJI_DUMP_STATUS"));
        assert!(content.contains("\"dropped_events\":1"));
    }
}
