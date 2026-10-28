//! Deterministic digest domains for the Python observed scanner (issue #146).
//! Native identity deliberately excludes source location; an occurrence
//! ordinal disambiguates same-name redeclarations in one lexical owner.

use lekalo_adapter_sdk::ids::sha256_hex;
use lekalo_adapter_sdk::json::{canonical, Value};
use sha2::Digest;

pub const NATIVE_DOMAIN: &str = "lekalo.py.native.v1";
pub const SIGNATURE_DOMAIN: &str = "lekalo.py.signature.v1";

/// `py1-<sha256>` native key: stable across rescans of identical evidence.
pub fn native_id(
    path: &str,
    qualified_name: &str,
    family: &str,
    slot: &str,
    occurrence: u64,
) -> String {
    let tuple = Value::Array(vec![
        Value::Str(NATIVE_DOMAIN.to_owned()),
        Value::Str(path.to_owned()),
        Value::Str(qualified_name.to_owned()),
        Value::Str(family.to_owned()),
        Value::Str(slot.to_owned()),
        Value::Int(occurrence as i64),
    ]);
    format!("py1-{}", sha256_hex(&canonical(&tuple)))
}

/// `sha256:<hex>` structural signature digest over one canonical tuple.
pub fn signature(parts: Value) -> String {
    format!("sha256:{}", sha256_hex(&canonical(&parts)))
}

/// `sha256:<hex>` source digest over raw file bytes (never lossy text).
pub fn source_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", digest_hex(bytes))
}

/// Bare lowercase hex of one sha256 digest.
pub fn digest_hex(bytes: &[u8]) -> String {
    hex_lower(sha2::Sha256::digest(bytes))
}

fn hex_lower(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in bytes.as_ref() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
