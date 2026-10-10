//! The production admission oracle for the adapter-SDK vector corpus
//! (issue #106 review r2 #10, extended per review r3 #8): the
//! cross-language parity gate must compare the SDK codecs against
//! production on the full corpus, not a ten-row upgrade slice.
//!
//! Modes, selected per input line by a leading tag:
//!
//!   `B<tab><base64(request-bytes)>` — RAW request admission: the bytes
//!   decode to `wire::decode_request` untouched (duplicate-key,
//!   numeric-spelling, CR/LF, and non-UTF-8 evidence preserved).
//!   Prints `{"mode":"B","accepted":...,"reason":...}`.
//!
//!   `V<tab><base64(response-bytes)>` — RAW response admission: the
//!   bytes decode to `wire::decode_response` (shape + bounds). Prints
//!   `{"mode":"V","accepted":...,"reason":...}`.
//!
//!   `R<tab><raw-request-bytes>` — legacy single-line raw mode; a bare
//!   document keeps the legacy structured behavior so old callers keep
//!   working. CR/LF framing strips newlines, so byte-exact corpora must
//!   use the base64 modes above.
//!
//! One JSON verdict per input line, in order. Nothing else: no network,
//! no files, no host state.

use std::io::{BufRead, Write};

use lekalo_core::target_protocol::{wire, TargetFailure};
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

fn main() {
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
