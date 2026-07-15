//! 应用侧的 blivedm 原始事件调试输出。

use std::io::Write;
use std::sync::OnceLock;

use serde_json::Value;

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

pub fn dump(value: &Value) {
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
