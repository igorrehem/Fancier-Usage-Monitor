use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

struct DiagnoseState {
    file: Mutex<File>,
}

static DIAGNOSE_STATE: OnceLock<DiagnoseState> = OnceLock::new();

/// Rotation threshold shared by the diagnostic log and the always-on journal:
/// once a file crosses this size we rotate it out of the way instead of
/// letting it grow without bound across the lifetime of a long-running widget.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Format a single log/journal line. Pulled out as a pure function so the
/// exact format (used by both the opt-in log and the always-on journal) is
/// unit-testable without touching the filesystem.
fn format_line(timestamp: u64, pid: u32, message: &str) -> String {
    format!("[{timestamp} pid={pid}] {message}")
}

/// If `path` exists and exceeds `max_len` bytes, rename it to "<name>.old"
/// (replacing any existing .old) so the caller can start a fresh file. Never
/// fails the caller: rename errors are swallowed and the caller falls back to
/// appending to (and thus continuing to grow) the oversized file.
fn rotate_if_oversized(path: &Path, max_len: u64) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    if metadata.len() <= max_len {
        return;
    }

    let mut old_name = path.file_name().unwrap_or_default().to_os_string();
    old_name.push(".old");
    let old_path = path.with_file_name(old_name);
    // Rust's Windows `rename` uses MOVEFILE_REPLACE_EXISTING, so this replaces
    // a pre-existing .old file. On failure we just keep appending to the
    // oversized file rather than losing new log lines.
    let _ = std::fs::rename(path, &old_path);
}

pub fn init() -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join("fancier-usage-monitor.log");
    rotate_if_oversized(&path, MAX_LOG_BYTES);

    // Append rather than truncate: a relaunched child process must not wipe
    // out the log lines its predecessor wrote before dying.
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Unable to open diagnostic log file {}: {e}", path.display()))?;

    let _ = DIAGNOSE_STATE.set(DiagnoseState {
        file: Mutex::new(file),
    });

    log("diagnostic logging enabled");
    Ok(path)
}

pub fn is_enabled() -> bool {
    DIAGNOSE_STATE.get().is_some()
}

pub fn log(message: impl AsRef<str>) {
    let Some(state) = DIAGNOSE_STATE.get() else {
        return;
    };

    let line = format_line(now_unix_secs(), std::process::id(), message.as_ref());

    if let Ok(mut file) = state.file.lock() {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

pub fn log_error(context: &str, error: impl std::fmt::Display) {
    log(format!("{context}: {error}"));
}

/// Core of `journal`, taking an explicit path so tests can point it at a
/// scratch file instead of the real journal location.
fn journal_to(path: &Path, message: &str) {
    rotate_if_oversized(path, MAX_LOG_BYTES);

    // Opened per-call (journal events are rare) rather than cached behind a
    // OnceLock like the diagnostic log: this keeps it robust when a parent
    // and a freshly relaunched child are both writing during a relaunch.
    let file = OpenOptions::new().create(true).append(true).open(path);
    if let Ok(mut file) = file {
        let line = format_line(now_unix_secs(), std::process::id(), message);
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

/// Record a critical lifecycle event to the always-on journal, regardless of
/// whether `--diagnose` is active. This is the evidence of last resort for a
/// process that dies silently before anyone thought to turn on `--diagnose`.
pub fn journal(message: impl AsRef<str>) {
    let path = std::env::temp_dir().join("fancier-usage-monitor-journal.log");
    journal_to(&path, message.as_ref());

    // Keep the verbose diagnostic log complete when it's active.
    if is_enabled() {
        log(message.as_ref());
    }
}

/// Install a panic hook that journals the panic before the process aborts.
///
/// The workspace release profile uses `panic = "abort"`, so this hook runs
/// immediately before the abort -- it is the only chance to record that a
/// panic (as opposed to a clean exit) is what ended the process. Kept
/// deliberately minimal and panic-safe: no unwraps, nothing that can itself
/// panic while already unwinding the stack of a panicking thread.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        journal(format!("PANIC: {info}"));
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_line_includes_timestamp_pid_and_message() {
        let line = format_line(12345, 6789, "hello world");
        assert_eq!(line, "[12345 pid=6789] hello world");
    }

    #[test]
    fn rotate_if_oversized_renames_large_file_to_old() {
        let path = std::env::temp_dir().join("ccum-diagnose-test-rotate-large.log");
        let old_path = std::env::temp_dir().join("ccum-diagnose-test-rotate-large.log.old");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&old_path);

        std::fs::write(&path, vec![0u8; 200]).expect("write scratch file");
        rotate_if_oversized(&path, 100);

        assert!(!path.exists(), "oversized file should have been rotated away");
        assert!(old_path.exists(), ".old file should exist after rotation");

        let _ = std::fs::remove_file(&old_path);
    }

    #[test]
    fn rotate_if_oversized_leaves_small_file_in_place() {
        let path = std::env::temp_dir().join("ccum-diagnose-test-rotate-small.log");
        let _ = std::fs::remove_file(&path);

        std::fs::write(&path, vec![0u8; 10]).expect("write scratch file");
        rotate_if_oversized(&path, 100);

        assert!(path.exists(), "small file should not be rotated");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn journal_to_appends_each_call_as_its_own_line_with_pid() {
        let path = std::env::temp_dir().join("ccum-diagnose-test-journal.log");
        let _ = std::fs::remove_file(&path);

        journal_to(&path, "first event");
        journal_to(&path, "second event");

        let contents = std::fs::read_to_string(&path).expect("read scratch journal");
        let lines: Vec<&str> = contents.lines().collect();

        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("pid="));
        assert!(lines[1].contains("pid="));
        assert!(lines[0].contains("first event"));
        assert!(lines[1].contains("second event"));

        let _ = std::fs::remove_file(&path);
    }
}
