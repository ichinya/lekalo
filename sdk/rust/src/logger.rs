//! The bounded, redacted stderr logger (issue #106). Logs never touch
//! stdout, never carry raw request, source, or environment content, and
//! stay under the bounded stream the host captures. Messages must
//! already be redacted: pass bounded tokens and counts, never payloads.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_MESSAGE_CHARS: usize = 256;
const MAX_STDERR_BYTES: u64 = 65_536;

static EMITTED_BYTES: AtomicU64 = AtomicU64::new(0);

fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    // Deterministic fixed-width UTC timestamp (no host timezone data).
    let days = seconds / 86_400;
    let rem = seconds % 86_400;
    format!(
        "{days:05}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn emit(level: &str, tag: &str, message: &str) {
    if message.is_empty() || message.contains('\n') || message.contains('\r') {
        return;
    }
    let text: String = if message.chars().count() > MAX_MESSAGE_CHARS {
        message.chars().take(MAX_MESSAGE_CHARS).collect()
    } else {
        message.to_owned()
    };
    let line = format!("{} {level}[{tag}]: {text}\n", timestamp());
    let bytes = line.len() as u64;
    if EMITTED_BYTES.fetch_add(bytes, Ordering::SeqCst) + bytes > MAX_STDERR_BYTES {
        return;
    }
    let _ = std::io::stderr().write_all(line.as_bytes());
}

/// Log one bounded debug line.
pub fn debug(tag: &str, message: &str) {
    emit("debug", tag, message);
}

/// Log one bounded error line.
pub fn error(tag: &str, message: &str) {
    emit("error", tag, message);
}

/// Log one bounded info line.
pub fn info(tag: &str, message: &str) {
    emit("info", tag, message);
}

/// Log one bounded warning line.
pub fn warn(tag: &str, message: &str) {
    emit("warn", tag, message);
}

/// Whether a redaction candidate is safe to log: bounded and single-line.
pub fn is_loggable_token(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 128
        && !value.contains('\n')
        && !value.contains('\r')
}
