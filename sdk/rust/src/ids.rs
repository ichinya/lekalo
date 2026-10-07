//! Deterministic identifiers and digest grammar (issue #106): SHA-256
//! digests, `req-`/`plan-` identifiers, and the canonical bytes they
//! hash. Framing mirrors the core exactly.

use sha2::{Digest, Sha256};

use crate::json::{canonical, Value};

/// Lowercase hex SHA-256 of the UTF-8 bytes of the text.
pub fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The `sha256:<64 hex>` digest spelling of the UTF-8 bytes of the text.
pub fn sha256(text: &str) -> String {
    format!("sha256:{}", sha256_hex(text))
}

pub const REQUEST_ID_PREFIX: &str = "req-";
pub const PLAN_ID_PREFIX: &str = "plan-";

fn is_prefixed_hex(value: &str, prefix: &str) -> bool {
    let Some(hex) = value.strip_prefix(prefix) else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn is_sha256_digest(value: &str) -> bool {
    is_prefixed_hex(value, "sha256:")
}

pub fn is_request_id(value: &str) -> bool {
    is_prefixed_hex(value, REQUEST_ID_PREFIX)
}

pub fn is_plan_id(value: &str) -> bool {
    is_prefixed_hex(value, PLAN_ID_PREFIX)
}

/// The deterministic request identifier over the canonical JSON of the
/// envelope with its `request_id` member absent.
pub fn request_id(envelope: &Value) -> String {
    let mut copy = envelope.clone();
    if let Value::Object(members) = &mut copy {
        members.retain(|(key, _)| key != "request_id");
    }
    format!("{REQUEST_ID_PREFIX}{}", sha256_hex(&canonical(&copy)))
}

/// The deterministic plan identifier over the canonical bytes of one sorted plan.
pub fn plan_id(entries: &Value) -> String {
    format!("{PLAN_ID_PREFIX}{}", sha256_hex(&canonical(entries)))
}
