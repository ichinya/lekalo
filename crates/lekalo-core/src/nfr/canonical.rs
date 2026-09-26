//! Canonical bytes of the NFR contracts (issue #85).
//!
//! Attachment and evidence canonical bytes are compact UTF-8 JSON with
//! byte-sorted object keys, canonical collections, and no trailing LF.
//! The same rule is proven independently by the Node contract suite
//! and by the digest sidecars of the committed goldens.

use serde::Serialize;

use super::version;

/// Serialize any value into compact JSON with byte-sorted object keys
/// (the round trip through `serde_json::Value`'s ordered map), which
/// is the attachment and evidence canonical form.
pub(super) fn canonical_value_bytes<T: Serialize>(value: &T) -> String {
    let dynamic = serde_json::to_value(value).expect("nfr wire values serialize");
    serde_json::to_string(&dynamic).expect("canonical JSON bytes fit in memory")
}

/// The bare 64 lowercase hex SHA-256 of some bytes.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    crate::digest::sha256_hex(bytes)
}

/// Whether the canonical payload of `bytes` stays inside the export
/// bound, or the export-limit refusal.
pub(super) fn check_export_bound(bytes: &str) -> Result<(), crate::diagnostics::DiagnosticSet> {
    if bytes.len() > version::MAX_EXPORT_BYTES {
        return Err(super::diagnostic::export_limit(
            "canonical-bytes",
            &format!("bytes={}", bytes.len()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Probe {
        zeta: u32,
        alpha: &'static str,
        nested: Nested,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Nested {
        second: u32,
        first: u32,
    }

    #[test]
    fn keys_are_byte_sorted_and_compact() {
        let probe = Probe {
            zeta: 1,
            alpha: "a",
            nested: Nested {
                second: 2,
                first: 1,
            },
        };
        assert_eq!(
            canonical_value_bytes(&probe),
            r#"{"alpha":"a","nested":{"first":1,"second":2},"zeta":1}"#
        );
    }

    #[test]
    fn digests_are_exact_sha256() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn export_bound_rejects_over_limit_payloads() {
        let small = "x".to_owned();
        assert!(check_export_bound(&small).is_ok());
        let huge = "x".repeat(version::MAX_EXPORT_BYTES + 1);
        assert!(check_export_bound(&huge).is_err());
    }
}
