//! Snapshot identity for the MCP server (issue #33): the MCP snapshot is
//! a separate domain over the sorted admitted logical identities and their
//! exact content digests. Roots, clocks, environment, and host names never
//! enter a deterministic output.

use std::collections::BTreeMap;

use lekalo_core::digest::sha256_hex;

/// One admitted input identity and its exact content digest.
pub type Inputs = BTreeMap<String, String>;

/// The snapshot reference of one admitted input set: `sha256-` plus the
/// digest over the sorted `identity<US>digest` lines (US = 0x1F).
pub fn snapshot_ref(inputs: &Inputs) -> String {
    let mut material = String::new();
    for (identity, digest) in inputs {
        material.push_str(identity);
        material.push('\u{1F}');
        material.push_str(digest);
        material.push('\n');
    }
    format!("sha256-{}", sha256_hex(material.as_bytes()))
}

/// The exact content digest of one domain's bytes.
pub fn domain_digest(domain: &str, bytes: &[u8]) -> String {
    format!("{domain}:{}", sha256_hex(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_refs_are_sorted_and_path_free() {
        let mut inputs = Inputs::new();
        inputs.insert("model:b".to_owned(), "d2".to_owned());
        inputs.insert("model:a".to_owned(), "d1".to_owned());
        let reference = snapshot_ref(&inputs);
        assert!(reference.starts_with("sha256-"));
        let mut reordered = Inputs::new();
        reordered.insert("model:a".to_owned(), "d1".to_owned());
        reordered.insert("model:b".to_owned(), "d2".to_owned());
        assert_eq!(reference, snapshot_ref(&reordered));
        let mut changed = inputs.clone();
        changed.insert("model:a".to_owned(), "dX".to_owned());
        assert_ne!(reference, snapshot_ref(&changed));
        assert!(!reference.contains(':'));
    }

    #[test]
    fn domain_digests_carry_the_domain_prefix() {
        let digest = domain_digest("trace-manifest", b"bytes");
        assert!(digest.starts_with("trace-manifest:"));
        assert_eq!(domain_digest("x", b""), domain_digest("x", b""));
    }
}
