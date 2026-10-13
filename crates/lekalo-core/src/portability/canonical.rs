//! Canonical report bytes and digest (issue #19).
//!
//! The canonical form is compact UTF-8 JSON with sorted object keys and
//! byte-sorted set-like members. Wall-clock timings, absolute paths, and
//! unordered native output never enter the assessment, so two runs over
//! the same pins and evidence produce byte-identical output.

use super::wire::Report;
use crate::digest::sha256_hex;

/// The exact canonical compact JSON bytes of one report.
pub fn canonical_bytes(report: &Report) -> String {
    serde_json::to_string(report).expect("typed report serializes")
}

/// The `sha256:…` digest over the canonical report bytes.
pub fn report_digest(report: &Report) -> String {
    format!("sha256:{}", sha256_hex(canonical_bytes(report).as_bytes()))
}

/// Whether two reports are byte-identical in canonical form.
pub fn bytes_equal(left: &Report, right: &Report) -> bool {
    canonical_bytes(left) == canonical_bytes(right)
}
