//! Canonical serialization of the trace manifest (issue #22).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, object keys in fixed
//! contract order (serde_json serializes struct fields in declaration
//! order and every wire struct here declares exactly the contract order),
//! set-like arrays in their canonical sort. The bytes are path-independent:
//! no physical root, raw source, timestamp, host, locale, or provider
//! transcript ever enters them. The CLI adds exactly one trailing LF;
//! these functions never do, and the digest is `sha256:` over the bytes
//! exactly as produced here.

use sha2::{Digest, Sha256};

use super::version;
use super::Manifest;

/// Serialize the whole manifest to canonical bytes, or refuse beyond the
/// export bound.
pub(super) fn manifest_bytes(
    manifest: &Manifest,
) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let bytes =
        serde_json::to_string(manifest).map_err(|_| crate::diagnostics::DiagnosticSet::empty())?;
    if bytes.len() > version::MAX_EXPORT_BYTES {
        return Err(super::diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The `sha256:<64 lowercase hex>` digest of exactly these bytes.
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}
