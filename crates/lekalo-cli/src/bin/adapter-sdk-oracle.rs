//! The production admission oracle for the adapter-SDK vector corpus
//! (issue #106 review: the cross-language parity gate must compare the
//! SDK codecs against production, not only against each other).
//!
//! Reads one JSON vector document per line on stdin (the upgrade-family
//! shape: `{request, ...}`) and prints one JSON verdict per line:
//! `{accepted, reason}` where `reason` mirrors the production refusal
//! vocabulary (`token`, `version`, `lint-request`, `native-request`,
//! `operation`, ...). Nothing else: no network, no files, no host state.

use std::io::{BufRead, Write};

use lekalo_core::target_protocol::wire;
use serde_json::Value;

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
        let vector: Value = match serde_json::from_str(&line) {
            Ok(vector) => vector,
            Err(_) => {
                let _ = writeln!(stdout, r#"{{"accepted":false,"reason":"oracle-input"}}"#);
                continue;
            }
        };
        let Some(request) = vector.get("request") else {
            let _ = writeln!(stdout, r#"{{"accepted":false,"reason":"oracle-input"}}"#);
            continue;
        };
        let bytes = serde_json::to_vec(request).unwrap_or_default();
        let verdict = match wire::decode_request(&bytes) {
            Ok(_) => serde_json::json!({"accepted": true}),
            Err(failure) => {
                let reason = match &failure {
                    lekalo_core::target_protocol::TargetFailure::ProtocolMismatch { detail } => {
                        detail.detail()
                    }
                    lekalo_core::target_protocol::TargetFailure::RequestInvalid { detail } => {
                        detail
                    }
                    _ => "request-invalid",
                };
                serde_json::json!({"accepted": false, "reason": reason})
            }
        };
        let _ = writeln!(
            stdout,
            "{}",
            serde_json::to_string(&verdict).unwrap_or_default()
        );
    }
}
