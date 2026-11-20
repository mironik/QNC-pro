//! The one log of a process. The program says once where the log folder is (the settings block
//! knows the folder; this block only writes). Until it has, lines go nowhere, which is what a
//! test wants. Every line is one `write_all`, because several processes append to the same file
//! and interleaved writes would mix their lines.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const FILE: &str = "qnc.log";

struct Log {
    folder: PathBuf,
    program: &'static str,
}

static LOG: OnceLock<Mutex<Option<Log>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Log>> {
    LOG.get_or_init(|| Mutex::new(None))
}

/// Where this process logs from now on. The folder is made when it is missing.
pub fn init(folder: &Path, program: &'static str) {
    let _ = fs::create_dir_all(folder);
    if let Ok(mut slot) = slot().lock() {
        *slot = Some(Log { folder: folder.to_path_buf(), program });
    }
}

/// The log file of this process, once `init` was called.
pub fn path() -> Option<PathBuf> {
    slot().lock().ok()?.as_ref().map(|log| log.folder.join(FILE))
}

fn unix_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// One line: `<unix ms> <program> <message>`. Nothing happens before `init`.
pub fn log(message: impl AsRef<str>) {
    let Ok(slot) = slot().lock() else { return };
    let Some(log) = slot.as_ref() else { return };
    let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(log.folder.join(FILE)) else {
        return;
    };
    let line = format!("{} {} {}\n", unix_ms(), log.program, message.as_ref());
    let _ = file.write_all(line.as_bytes());
}

/// Every panic of this process is logged (place, message, backtrace) before the usual hook.
pub fn log_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let trace = std::backtrace::Backtrace::force_capture().to_string().replace('\n', " | ");
        log(format!("panic place={place} message={message:?} backtrace={trace}"));
        previous(info);
    }));
}

/// The last `count` lines of a log file.
pub fn recent(file: &Path, count: usize) -> Vec<String> {
    let text = fs::read_to_string(file).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines.iter().rev().take(count).rev().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_written_only_after_init_and_carries_time_and_program() {
        log("before init goes nowhere");
        let folder = std::env::temp_dir().join(format!("qnc-diagnostics-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        init(&folder, "qnc-test");
        log("hello");
        let lines = recent(&path().unwrap(), 10);
        assert_eq!(lines.len(), 1);
        let mut parts = lines[0].splitn(3, ' ');
        assert!(parts.next().unwrap().parse::<u128>().is_ok());
        assert_eq!(parts.next(), Some("qnc-test"));
        assert_eq!(parts.next(), Some("hello"));
        let _ = fs::remove_dir_all(&folder);
    }
}
