//! Closed trace identity grammars (issue #22).
//!
//! Every identity is validated before it may enter a manifest: node ids,
//! external ids, revisions, digests, contract versions, adapter ids,
//! occurrences, and privacy-safe logical paths. Identity never derives
//! from display text, paths, HTTP status, exception classes, or rendered
//! prose, and a logical path is lexical data only — it is never resolved
//! against the filesystem here.

use super::diagnostic::PathViolation;
use crate::project_fs;

/// The exact `sha256:<64 lowercase hex>` digest grammar.
pub fn is_digest(text: &str) -> bool {
    let Some(hex) = text.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// The exact revision grammar: 40 hex (SHA-1) or 64 hex (SHA-256),
/// lowercase only.
pub fn is_revision(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// The canonical `X.Y.Z` contract version grammar.
pub fn is_contract_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The #6 semantic id grammar, shared verbatim with the #13 graph.
pub fn is_semantic_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 192
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte == b'_'
                || byte == b'.'
                || byte == b':'
                || byte == b'-'
        })
        && text
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// The manifest-local node identity grammar.
pub fn is_node_id(text: &str) -> bool {
    is_semantic_id(text)
}

/// The stable logical export id grammar.
pub fn is_manifest_id(text: &str) -> bool {
    (3..=128).contains(&text.len())
        && text.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
        })
        && text
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

/// The stable artifact-entry identity grammar.
pub fn is_artifact_id(text: &str) -> bool {
    (1..=128).contains(&text.len())
        && text.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'.'
                || byte == b'_'
                || byte == b'-'
        })
        && text
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

/// The namespaced provider/adapter identity grammar, shared with the #13
/// graph adapter provenance.
pub fn is_adapter_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    let is_lower_digit = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if bytes.len() > 128 || bytes.is_empty() || !is_lower_digit(bytes[0]) {
        return false;
    }
    let Some(last) = bytes.last() else {
        return false;
    };
    if !is_lower_digit(*last) {
        return false;
    }
    // The dotted namespace chain is mandatory: at least two names before
    // any `/` suffix (the accepted graph adapter grammar).
    let Some(first) = text.split('/').next() else {
        return false;
    };
    if first.split('.').count() < 2 {
        return false;
    }
    // Every dotted name and `/`-suffix name must be lowercase grammar.
    for segment in text.split('/') {
        for part in segment.split('.') {
            if part.is_empty() {
                return false;
            }
            let part_bytes = part.as_bytes();
            if !is_lower_digit(part_bytes[0]) {
                return false;
            }
            if !part_bytes
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
            {
                return false;
            }
        }
    }
    true
}

/// The occurrence grammar: a stable parent member/role/ordinal token,
/// never an array position.
pub fn is_occurrence(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' || byte == b'-'
        })
        && text
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// A privacy-safe project-relative POSIX logical path.
///
/// The lexical rules are the accepted structure contract rules
/// (`project_fs::path_violation`): absolute spellings, drives, UNC and
/// device paths, backslashes, `.`/`..` segments, empty or control
/// segments, trailing dot/space, DOS device names, short-name-like
/// aliases, and non-NFC spellings are all rejected before any other
/// validation runs. Physical containment is an upstream filesystem gate;
/// lexical acceptance never claims it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct LogicalPath(String);

impl LogicalPath {
    /// Validate and return the path, or the lexical violation.
    pub fn parse(text: &str) -> Result<Self, PathViolation> {
        match project_fs::path_violation(text) {
            Some(code) => Err(PathViolation(code)),
            None => Ok(Self(text.to_owned())),
        }
    }

    /// The validated path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_are_exact_sha256_spellings() {
        let hex = "a".repeat(64);
        assert!(is_digest(&format!("sha256:{hex}")));
        assert!(!is_digest(&format!("sha256:{}", hex.to_uppercase())));
        assert!(!is_digest(&format!("sha256:{}", "a".repeat(63))));
        assert!(!is_digest(&format!("md5:{hex}")));
        assert!(!is_digest(&hex));
    }

    #[test]
    fn revisions_are_40_or_64_lowercase_hex() {
        assert!(is_revision(&"a".repeat(40)));
        assert!(is_revision(&"b".repeat(64)));
        assert!(!is_revision(&"a".repeat(41)));
        assert!(!is_revision(&"A".repeat(40)));
    }

    #[test]
    fn contract_versions_are_canonical_triplets() {
        assert!(is_contract_version("1.0.0"));
        assert!(is_contract_version("0.1.0"));
        assert!(!is_contract_version("1.0"));
        assert!(is_contract_version("01.0.0")); // digits-only triplets, matching the graph protocol grammar
        assert!(!is_contract_version("v1.0.0"));
        assert!(!is_contract_version("1.0.0+meta"));
    }

    #[test]
    fn semantic_ids_match_the_shared_grammar() {
        assert!(is_semantic_id("planner.focus_task"));
        assert!(is_semantic_id("PLANNER-REQ-001"));
        assert!(is_semantic_id("_private"));
        assert!(!is_semantic_id(".hidden"));
        assert!(!is_semantic_id(""));
        assert!(!is_semantic_id(&"a".repeat(193)));
    }

    #[test]
    fn manifest_ids_are_lowercase_logical_labels() {
        assert!(is_manifest_id("planner-trace"));
        assert!(is_manifest_id("ab1"));
        assert!(!is_manifest_id("no"));
        assert!(!is_manifest_id("-leading"));
        assert!(!is_manifest_id("has space"));
        assert!(!is_manifest_id("Uppercase"));
    }

    #[test]
    fn adapter_ids_are_namespaced_lowercase() {
        assert!(is_adapter_id("lekalo.core"));
        assert!(is_adapter_id("dev.lekalo/openspec-adapter"));
        assert!(!is_adapter_id("single"));
        assert!(!is_adapter_id(" leading."));
        assert!(!is_adapter_id("a..b"));
    }

    #[test]
    fn occurrences_are_stable_role_tokens() {
        assert!(is_occurrence("implements.0"));
        assert!(is_occurrence("requirements_focus-task"));
        assert!(!is_occurrence(".leading"));
        assert!(!is_occurrence(""));
        assert!(!is_occurrence(&"a".repeat(65)));
    }

    #[test]
    fn logical_paths_reject_every_absolute_or_traversal_spelling() {
        assert!(LogicalPath::parse("apps/api/src/focus-task.ts").is_ok());
        for hostile in [
            "/etc/passwd",
            r"C:\temp\secret",
            "\\\\server/share",
            "a/../b",
            "./a",
            "a/./b",
            "a\\b",
            "a ",
            "a.",
            "CON",
            "spec~1",
            "a%20b",
            "",
            "leader:",
        ] {
            assert!(
                LogicalPath::parse(hostile).is_err(),
                "expected rejection: {hostile:?}"
            );
        }
    }
}
