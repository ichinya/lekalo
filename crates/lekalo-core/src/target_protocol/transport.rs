//! The bounded process transport of the target protocol (issue #27).
//!
//! The adapter executable is spawned directly from an argv vector — never
//! through a shell, never with interpolation — in the project root. The
//! request is delivered over stdin (or, when the adapter declared only the
//! `file` transport, through a bounded temporary file whose path is appended
//! to the argv as `--lekalo-request-file <PATH>`). The response envelope is
//! read from stdout with a hard byte cap; stderr is captured only as
//! bounded diagnostics evidence and never parsed as protocol. The wait loop
//! enforces the deadline and a caller cancel flag, killing the child on
//! either; every refusal is classified infrastructure.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The direct adapter invocation: program plus argv vector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterCommand {
    /// The executable; spawned directly, PATH lookup included, no shell.
    pub program: PathBuf,
    /// Extra adapter arguments (never containing the request itself).
    pub args: Vec<String>,
}

/// The transport limits of one exchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportLimits {
    /// Operation deadline in milliseconds.
    pub timeout_ms: u64,
    /// Hard cap for the stdout envelope.
    pub max_output_bytes: usize,
    /// Hard cap for the captured stderr stream.
    pub max_stderr_bytes: usize,
    /// Hard cap for the serialized request.
    pub max_request_bytes: usize,
}

impl Default for TransportLimits {
    fn default() -> Self {
        Self {
            timeout_ms: super::version::DEFAULT_TIMEOUT_MS,
            max_output_bytes: super::version::DEFAULT_MAX_OUTPUT_BYTES as usize,
            max_stderr_bytes: super::version::MAX_STDERR_BYTES,
            max_request_bytes: super::version::MAX_REQUEST_BYTES,
        }
    }
}

/// Why the transport refused or aborted the exchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportFailure {
    /// The executable could not be spawned.
    Spawn,
    /// The request could not be delivered (broken pipe, oversize).
    RequestWrite,
    /// The deadline elapsed; the child was killed.
    Timeout,
    /// The caller cancelled; the child was killed.
    Cancelled,
    /// A response stream exceeded its cap; the child was killed.
    OutputLimit { stream: Stream },
}

impl TransportFailure {
    /// The bounded wire token carried in diagnostic data.
    pub fn detail(self) -> &'static str {
        match self {
            Self::Spawn => "spawn",
            Self::RequestWrite => "request-write",
            Self::Timeout => "deadline",
            Self::Cancelled => "caller",
            Self::OutputLimit { .. } => "output-limit",
        }
    }
}

/// The stream that exceeded its cap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    /// The bounded wire token carried in diagnostic data.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

/// One completed exchange: bounded output streams and the child exit code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportSuccess {
    /// The child's exit code (platform-specific for abnormal terminations).
    pub exit_code: i32,
    /// The stdout bytes up to the cap.
    pub stdout: Vec<u8>,
    /// The stderr bytes up to the cap, diagnostics evidence only.
    pub stderr: Vec<u8>,
    /// Whether stdout exceeded its cap (the exchange is still refused).
    pub stdout_truncated: bool,
    /// Whether stderr was cut at the cap (evidence only).
    pub stderr_truncated: bool,
}

/// Run one adapter exchange to completion.
///
/// `request` is the serialized request envelope; `file_transport` switches
/// delivery from stdin to the temporary-file convention.
pub fn run(
    command: &AdapterCommand,
    request: &[u8],
    limits: &TransportLimits,
    cwd: &Path,
    file_transport: bool,
    cancel: Option<&AtomicBool>,
) -> Result<TransportSuccess, TransportFailure> {
    if request.len() > limits.max_request_bytes {
        return Err(TransportFailure::RequestWrite);
    }
    let mut argv: Vec<String> = Vec::new();
    let mut temp_path: Option<PathBuf> = None;
    if file_transport {
        let path = write_request_file(request)?;
        argv.push("--lekalo-request-file".to_owned());
        argv.push(path.display().to_string());
        temp_path = Some(path);
    }
    let mut child = std::process::Command::new(&command.program)
        .args(&command.args)
        .args(&argv)
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| TransportFailure::Spawn)?;

    let result = pump(&mut child, request, limits, cancel);
    let _ = child.wait();
    if let Some(path) = temp_path {
        let _ = std::fs::remove_file(path);
    }
    result
}

/// Deliver stdin, collect bounded output, and enforce deadline/cancel while
/// the child runs.
fn pump(
    child: &mut std::process::Child,
    request: &[u8],
    limits: &TransportLimits,
    cancel: Option<&AtomicBool>,
) -> Result<TransportSuccess, TransportFailure> {
    let mut stdin = child.stdin.take().ok_or(TransportFailure::Spawn)?;
    let stdout = child.stdout.take().ok_or(TransportFailure::Spawn)?;
    let stderr = child.stderr.take().ok_or(TransportFailure::Spawn)?;

    // The threads own copied bounds and bytes: no borrowed data escapes.
    let request: Vec<u8> = request.to_vec();
    let max_output_bytes = limits.max_output_bytes;
    let max_stderr_bytes = limits.max_stderr_bytes;

    let (stdin_tx, stdin_rx) = mpsc::channel::<Result<(), std::io::ErrorKind>>();
    let stdin_handle = std::thread::spawn(move || {
        let result = stdin.write_all(&request).and_then(|()| stdin.flush());
        // Dropping stdin closes the pipe so the child sees EOF.
        let _ = stdin_tx.send(result.map_err(|e| e.kind()));
    });
    let (stdout_tx, stdout_rx) = mpsc::channel::<(Vec<u8>, bool)>();
    let stdout_handle = std::thread::spawn(move || {
        let (bytes, truncated) = read_bounded(stdout, max_output_bytes);
        let _ = stdout_tx.send((bytes, truncated));
    });
    let (stderr_tx, stderr_rx) = mpsc::channel::<(Vec<u8>, bool)>();
    let stderr_handle = std::thread::spawn(move || {
        let (bytes, truncated) = read_bounded(stderr, max_stderr_bytes);
        let _ = stderr_tx.send((bytes, truncated));
    });

    let deadline = Instant::now() + Duration::from_millis(limits.timeout_ms);
    let mut stdout_result: Option<(Vec<u8>, bool)> = None;
    let mut stderr_result: Option<(Vec<u8>, bool)> = None;
    let mut stdin_result: Option<Result<(), std::io::ErrorKind>> = None;
    let mut status: Option<std::process::ExitStatus> = None;
    let refusal: TransportFailure;

    loop {
        if let Some(flag) = cancel {
            if flag.load(Ordering::Relaxed) {
                refusal = TransportFailure::Cancelled;
                break;
            }
        }
        if Instant::now() >= deadline {
            refusal = TransportFailure::Timeout;
            break;
        }
        if stdin_result.is_none() {
            if let Ok(result) = stdin_rx.try_recv() {
                stdin_result = Some(result);
            } else if stdin_handle.is_finished() {
                // The writer exited without a delivery: broken pipe.
                stdin_result = Some(Err(std::io::ErrorKind::BrokenPipe));
            }
        }
        if stdout_result.is_none() {
            if let Ok(payload) = stdout_rx.try_recv() {
                stdout_result = Some(payload);
            } else if stdout_handle.is_finished() {
                stdout_result = Some((Vec::new(), false));
            }
        }
        if stdout_result
            .as_ref()
            .is_some_and(|(_, truncated)| *truncated)
        {
            refusal = TransportFailure::OutputLimit {
                stream: Stream::Stdout,
            };
            break;
        }
        if stderr_result.is_none() {
            if let Ok(payload) = stderr_rx.try_recv() {
                stderr_result = Some(payload);
            } else if stderr_handle.is_finished() {
                stderr_result = Some((Vec::new(), false));
            }
        }
        if stderr_result
            .as_ref()
            .is_some_and(|(_, truncated)| *truncated)
        {
            refusal = TransportFailure::OutputLimit {
                stream: Stream::Stderr,
            };
            break;
        }
        if status.is_none() {
            status = child.try_wait().map_err(|_| TransportFailure::Spawn)?;
        }
        if let (Some(status), Some(stdin), Some(stdout_result), Some(stderr_result)) = (
            status.as_ref(),
            stdin_result.as_ref(),
            stdout_result.as_ref(),
            stderr_result.as_ref(),
        ) {
            let exit_code = status.code().unwrap_or(-1);
            if exit_code != 0 && *stdin == Err(std::io::ErrorKind::BrokenPipe) {
                return Err(TransportFailure::RequestWrite);
            }
            let (stdout_bytes, stdout_truncated) = stdout_result.clone();
            let (stderr_bytes, stderr_truncated) = stderr_result.clone();
            return Ok(TransportSuccess {
                exit_code,
                stdout: stdout_bytes,
                stderr: stderr_bytes,
                stdout_truncated,
                stderr_truncated,
            });
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    // A deadline/cancel/output-limit refusal: kill the child, then drain the
    // channels so their writer threads can exit and be joined.
    let _ = child.kill();
    let _ = child.wait();
    let _ = stdin_rx.recv_timeout(Duration::from_secs(1));
    let _ = stdout_rx.recv_timeout(Duration::from_secs(1));
    let _ = stderr_rx.recv_timeout(Duration::from_secs(1));
    let _ = stdin_handle.join();
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();
    Err(refusal)
}

/// Read one stream to EOF or the cap, reporting whether the cap was hit.
fn read_bounded(mut stream: impl Read, cap: usize) -> (Vec<u8>, bool) {
    let mut bytes = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let room = cap.saturating_sub(bytes.len());
                if room == 0 {
                    truncated = true;
                    break;
                }
                let take = n.min(room);
                bytes.extend_from_slice(&chunk[..take]);
                if take < n {
                    truncated = true;
                    break;
                }
            }
            Err(_) => break,
        }
    }
    (bytes, truncated)
}

/// Write the request to one bounded temporary file outside the project.
fn write_request_file(request: &[u8]) -> Result<PathBuf, TransportFailure> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "lekalo-target-request-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, request).map_err(|_| TransportFailure::RequestWrite)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_stops_at_the_cap() {
        let payload = vec![7u8; 100];
        let (bytes, truncated) = read_bounded(payload.as_slice(), 64);
        assert_eq!(bytes.len(), 64);
        assert!(truncated);
        let (bytes, truncated) = read_bounded(payload.as_slice(), 128);
        assert_eq!(bytes.len(), 100);
        assert!(!truncated);
    }

    #[test]
    fn oversize_requests_are_refused_before_spawn() {
        let limits = TransportLimits {
            max_request_bytes: 4,
            ..TransportLimits::default()
        };
        let command = AdapterCommand {
            program: PathBuf::from("lekalo-definitely-not-an-executable"),
            args: Vec::new(),
        };
        let error =
            run(&command, b"12345", &limits, Path::new("."), false, None).expect_err("refused");
        assert_eq!(error, TransportFailure::RequestWrite);
    }

    #[test]
    fn spawn_failures_classify_as_spawn() {
        let limits = TransportLimits::default();
        let command = AdapterCommand {
            program: PathBuf::from("lekalo-definitely-not-an-executable"),
            args: Vec::new(),
        };
        let error =
            run(&command, b"{}", &limits, Path::new("."), false, None).expect_err("refused");
        assert_eq!(error, TransportFailure::Spawn);
    }
}
