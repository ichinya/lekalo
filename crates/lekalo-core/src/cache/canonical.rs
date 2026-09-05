//! Canonical bytes for cache keys and records (issue #20).
//!
//! Canonical bytes are compact UTF-8 JSON with byte-sorted object keys and
//! no insignificant whitespace, matching the other Lekalo canonicalizers.
//! Every key digest and payload digest is SHA-256 over such bytes; set-like
//! collections (dependency edges, parsed digest sets, payload fields) are
//! sorted by their typed order before serialization, while ordered
//! occurrences preserve their order.

use serde::Serialize;
use sha2::{Digest, Sha256};

/// Serialize one typed value into canonical bytes: compact JSON with
/// byte-sorted object keys (the `serde_json` map is a `BTreeMap`).
pub(crate) fn canonical_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    let dynamic = serde_json::to_value(value).expect("cache wire values serialize");
    serde_json::to_vec(&dynamic).expect("canonical JSON bytes fit in memory")
}

/// The lowercase hexadecimal SHA-256 of `bytes`.
pub(crate) fn hex_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// The `sha256:<64 lowercase hex>` spelling of the digest of `bytes`.
pub(crate) fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex_digest(bytes))
}

/// Whether `text` is a well-formed `sha256:<64 lowercase hex>` digest.
pub(crate) fn is_digest(text: &str) -> bool {
    let Some(hex) = text.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Sample {
        zeta: u32,
        alpha: &'static str,
    }

    #[test]
    fn canonical_bytes_sort_object_keys_and_stay_compact() {
        let bytes = canonical_bytes(&Sample {
            zeta: 1,
            alpha: "x",
        });
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            r#"{"alpha":"x","zeta":1}"#
        );
    }

    #[test]
    fn digests_are_prefixed_lowercase_hex() {
        let digest = sha256_digest(b"abc");
        assert_eq!(
            digest,
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(is_digest(&digest));
        assert!(!is_digest("sha256:BA7816BF"));
        assert!(!is_digest(
            "sha7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
        assert!(!is_digest("sha256:7816bf"));
    }
}
