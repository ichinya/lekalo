//! The production admission oracle for the adapter-SDK vector corpus
//! (issue #106 review r2 #10, extended per review r3 #8): the
//! cross-language parity gate must compare the SDK codecs against
//! production on the full corpus, not a ten-row upgrade slice.
//!
//! Modes, selected per input line by a leading tag:
//!
//!   `B<tab><hex(request-bytes)>` — RAW request admission: the bytes
//!   decode to `wire::decode_request` untouched (duplicate-key,
//!   numeric-spelling, CR/LF, and non-UTF-8 evidence preserved).
//!   Prints `{"mode":"B","accepted":...,"reason":...}`.
//!
//!   `V<tab><hex(response-bytes)>` — RAW response admission: the
//!   bytes decode to `wire::decode_response` (shape + bounds). Prints
//!   `{"mode":"V","accepted":...,"reason":...}`.
//!
//!   `C<tab><hex(describe-request)><tab><hex(response)>` — real production
//!   TargetClient describe handshake against an exact-byte replay child.
//!   The context must match the deterministic base describe request and its
//!   limits; the child verifies the actual request rather than rewriting any
//!   response bytes. Exercises echo, evidence identity, negotiation and scopes.
//!
//!   `R<tab><raw-request-bytes>` — legacy single-line raw mode; a bare
//!   document keeps the legacy structured behavior so old callers keep
//!   working. CR/LF framing strips newlines, so byte-exact corpora must
//!   use the hex modes above.
//!
//! One JSON verdict per input line, in order. No network or project data.
//! B/V are pure wire admission; C uses a disposable empty synthetic project
//! and the real confined process client, never a handwritten validator.

use std::io::{BufRead, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};

use lekalo_core::target_protocol::{
    transport::{AdapterCommand, TransportLimits},
    wire, TargetClient, TargetFailure,
};
use serde_json::Value;

fn request_verdict(bytes: &[u8]) -> Value {
    match wire::decode_request(bytes) {
        Ok(_) => serde_json::json!({"accepted": true}),
        Err(failure) => {
            let reason = match &failure {
                TargetFailure::ProtocolMismatch { detail } => detail.detail(),
                TargetFailure::RequestInvalid { detail } => detail,
                _ => "request-invalid",
            };
            serde_json::json!({"accepted": false, "reason": reason})
        }
    }
}

fn response_verdict(bytes: &[u8]) -> Value {
    match wire::decode_response(bytes) {
        Ok(_) => serde_json::json!({"accepted": true}),
        Err(invalidity) => serde_json::json!({"accepted": false, "reason": invalidity.detail()}),
    }
}

/// Byte-preserving hex framing (issue #106 review r3 #8): corpus bytes
/// — including CR/LF and non-UTF-8 spellings — travel line-safely as
/// lowercase hex and reach the decoder unchanged.
fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let bytes = text.as_bytes();
    let nibble = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in bytes.chunks(2) {
        out.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    Some(out)
}

/// This helper is an adapter executable, not a second validation authority:
/// it checks the client delivered the declared context and writes the original
/// response bytes unchanged. All response decisions belong to TargetClient.
fn replay_child(args: &[String]) -> i32 {
    let Some(expected) = args.get(2).and_then(|s| decode_hex(s)) else {
        return 1;
    };
    let Some(response) = args.get(3).and_then(|s| decode_hex(s)) else {
        return 1;
    };
    let mut actual = Vec::new();
    if std::io::stdin()
        .take(1_048_577)
        .read_to_end(&mut actual)
        .is_err()
    {
        return 1;
    }
    let expected = serde_json::from_slice::<Value>(&expected);
    let actual = serde_json::from_slice::<Value>(&actual);
    if expected.is_err() || actual.is_err() || expected.ok() != actual.ok() {
        eprintln!("oracle request context mismatch");
        return 1;
    }
    if std::io::stdout().write_all(&response).is_err() {
        return 1;
    }
    0
}

struct ScratchProject(std::path::PathBuf);
impl Drop for ScratchProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn client_verdict(payload: &str) -> Value {
    let invalid = |reason| serde_json::json!({"accepted": false, "reason": reason});
    let Some((request_hex, response_hex)) = payload.split_once('\t') else {
        return invalid("oracle-input");
    };
    let Some(request_bytes) = decode_hex(request_hex) else {
        return invalid("oracle-input");
    };
    let Some(_) = decode_hex(response_hex) else {
        return invalid("oracle-input");
    };
    let Ok(request) = wire::decode_request(&request_bytes) else {
        return invalid("oracle-context");
    };
    if request.operation != wire::Operation::Describe
        || request.protocol_version != "0.3.2"
        || request.request_id != wire::request_id(&request)
    {
        return invalid("oracle-context");
    }
    let Some(limits) = request.limits.as_ref() else {
        return invalid("oracle-context");
    };
    let (Some(timeout_ms), Some(max_output_bytes)) = (limits.timeout_ms, limits.max_output_bytes)
    else {
        return invalid("oracle-context");
    };
    let transport = TransportLimits {
        timeout_ms,
        max_output_bytes: max_output_bytes as usize,
        ..TransportLimits::default()
    };
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "lekalo-sdk-client-oracle-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    if std::fs::create_dir(&path).is_err() {
        return invalid("oracle-setup");
    }
    let scratch = ScratchProject(path);
    let Ok(program) = std::env::current_exe() else {
        return invalid("oracle-setup");
    };
    let command = AdapterCommand {
        program,
        args: vec![
            "--client-replay".to_owned(),
            request_hex.to_owned(),
            response_hex.to_owned(),
        ],
    };
    let mut client = TargetClient::new(transport);
    match client.describe(&command, &scratch.0) {
        Ok(_) => serde_json::json!({"accepted": true}),
        Err(failure) => {
            let reason = match failure {
                TargetFailure::ResponseInvalid { detail } => detail.detail(),
                TargetFailure::ProtocolMismatch { detail } => detail.detail(),
                TargetFailure::CapabilityUnsupported { detail } => detail,
                TargetFailure::OperationFailed { .. } => "operation-failed",
                TargetFailure::ScopeViolation { detail, .. } => detail,
                TargetFailure::Crash { .. } => "crash",
                TargetFailure::Timeout => "deadline",
                _ => "client-failure",
            };
            invalid(reason)
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--client-replay") {
        std::process::exit(replay_child(&args));
    }
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => std::process::exit(1),
        };
        if line.trim().is_empty() {
            continue;
        }
        // `B<tab>hex` = raw request bytes (byte-exact, line-safe);
        // `V<tab>hex` = raw response bytes; `R<tab>bytes` = legacy raw
        // request mode (CR/LF framing strips newlines); a bare document
        // keeps the legacy structured behavior so old callers keep
        // working.
        let verdict = match line.split_once('\t') {
            Some((tag, hex)) if tag == "B" || tag == "V" => match decode_hex(hex) {
                Some(bytes) if tag == "B" => request_verdict(&bytes),
                Some(bytes) => response_verdict(&bytes),
                None => serde_json::json!({"accepted": false, "reason": "oracle-input"}),
            },
            Some(("C", rest)) => client_verdict(rest),
            Some(("R", rest)) => request_verdict(rest.as_bytes()),
            Some(("S", rest)) => structured_verdict(rest),
            _ => structured_verdict(line.as_str()),
        };
        let _ = writeln!(
            stdout,
            "{}",
            serde_json::to_string(&verdict).unwrap_or_default()
        );
    }
}

fn structured_verdict(payload: &str) -> Value {
    let parsed = match serde_json::from_str::<Value>(payload) {
        Ok(document) => document,
        Err(_) => {
            return serde_json::json!({"accepted": false, "reason": "oracle-input"});
        }
    };
    let Some(request) = parsed.get("request") else {
        return serde_json::json!({"accepted": false, "reason": "oracle-input"});
    };
    match serde_json::to_vec(request) {
        Ok(bytes) => request_verdict(&bytes),
        Err(_) => serde_json::json!({"accepted": false, "reason": "oracle-input"}),
    }
}
