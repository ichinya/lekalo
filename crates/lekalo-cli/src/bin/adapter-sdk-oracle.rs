//! The production admission oracle for the adapter-SDK vector corpus
//! (issue #106 review r2 #10): the cross-language parity gate must
//! compare the SDK codecs against production on the full corpus, not a
//! ten-row upgrade slice.
//!
//! Two modes, selected per input line by a leading mode tag:
//!
//!   `R<tab><raw-request-bytes>` — RAW admission: the bytes go to
//!   `wire::decode_request` untouched (duplicate-key and spelling
//!   evidence preserved), the verdict mirrors the production refusal
//!   vocabulary. Prints `{"mode":"R","accepted":...,"reason":...}`.
//!
//!   `S<tab><canonical-request-json>` — the legacy upgrade slice: the
//!   line parses as a corpus document, the request member is
//!   canonicalized (duplicate keys already collapsed by the corpus
//!   format) and decoded. Same verdict shape.
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
        // `S<tab>document` = structured corpus row (the upgrade slice);
        // `R<tab>bytes` = raw request bytes; a bare document keeps the
        // legacy structured behavior so old callers keep working.
        let (mode, payload) = match line.split_once('\t') {
            Some(("R", rest)) => ("R", rest),
            Some(("S", rest)) => ("S", rest),
            _ => ("S", line.as_str()),
        };
        let verdict = if mode == "R" {
            request_verdict(payload.as_bytes())
        } else {
            let parsed = match serde_json::from_str::<Value>(payload) {
                Ok(document) => document,
                Err(_) => {
                    let _ = writeln!(stdout, r#"{{"accepted":false,"reason":"oracle-input"}}"#);
                    continue;
                }
            };
            let Some(request) = parsed.get("request") else {
                let _ = writeln!(stdout, r#"{{"accepted":false,"reason":"oracle-input"}}"#);
                continue;
            };
            match serde_json::to_vec(request) {
                Ok(bytes) => request_verdict(&bytes),
                Err(_) => {
                    let _ = writeln!(stdout, r#"{{"accepted":false,"reason":"oracle-input"}}"#);
                    continue;
                }
            }
        };
        let _ = writeln!(
            stdout,
            "{}",
            serde_json::to_string(&verdict).unwrap_or_default()
        );
    }
}
