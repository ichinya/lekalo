//! The client-SDK source seam (issue #72).
//!
//! The conventional per-project query-model home is exactly
//! `lekalo/query-model.yaml` (the `lekalo/transport.yaml` precedent).
//! [`read_query_model`] performs the only file access in this module:
//! a bounded, fail-closed read plus the same spanned frontend the
//! loader uses, then the closed [`QueryModelAttachment::from_value`]
//! gate. `Ok(None)` means the project declares no query-model home —
//! the SDK evidence derivation is then skipped honestly by the
//! orchestration caller, never silently narrowed.

use std::path::Path;

use crate::diagnostics::DiagnosticSet;
use crate::loader::frontends::{parse_document, Node, Parsed, Value};
use crate::query_model::QueryModelAttachment;

/// The conventional per-project query-model home.
pub const QUERY_MODEL_SOURCE_PATH: &str = "lekalo/query-model.yaml";

/// The query-model source byte bound (one MiB, the canonical payload
/// bound).
const SOURCE_BYTES: usize = 1024 * 1024;

/// Read and parse the query-model home under one project root. A
/// MISSING home is `Ok(None)` (the derivation skips honestly); every
/// other metadata failure — permission, IO, a directory in the slot —
/// propagates as a refusal, never a silent skip (issue #72 round 2).
pub fn read_query_model(root: &Path) -> Result<Option<QueryModelAttachment>, DiagnosticSet> {
    let path = root.join(QUERY_MODEL_SOURCE_PATH);
    match std::fs::metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(super::diagnostic::io_failure("source-not-file"));
            }
            if metadata.len() as usize > SOURCE_BYTES {
                return Err(super::diagnostic::io_failure("source-bytes"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(super::diagnostic::io_failure("source-unreadable")),
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return Err(super::diagnostic::io_failure("source-unreadable")),
    };
    let index = crate::loader::source::LineIndex::new(&text);
    let parsed = match parse_document(&text, &index) {
        Ok(parsed) => parsed,
        Err(_) => return Err(super::diagnostic::io_failure("source-syntax")),
    };
    match parsed {
        Parsed::Empty => Err(super::diagnostic::io_failure("source-empty")),
        Parsed::Root(node) => {
            let mut duplicates = Vec::new();
            if has_duplicate_keys(&node, &mut duplicates) {
                return Err(super::diagnostic::io_failure("source-duplicate-key"));
            }
            QueryModelAttachment::from_value(&node_to_json(&node)).map(Some)
        }
    }
}

/// Detect duplicate keys at every level (fail closed before typing).
fn has_duplicate_keys(node: &Node, seen: &mut Vec<String>) -> bool {
    match &node.value {
        Value::Map(entries) => {
            let mut keys: Vec<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
            let before = keys.len();
            keys.sort_unstable();
            keys.dedup();
            if keys.len() != before {
                seen.push(
                    entries
                        .iter()
                        .map(|entry| entry.key.clone())
                        .next_back()
                        .unwrap_or_default(),
                );
                return true;
            }
            entries
                .iter()
                .any(|entry| has_duplicate_keys(&entry.value, seen))
        }
        Value::Seq(items) => items.iter().any(|item| has_duplicate_keys(item, seen)),
        Value::Scalar(_) => false,
    }
}

/// Convert one frontend node into its JSON form.
fn node_to_json(node: &Node) -> serde_json::Value {
    match &node.value {
        Value::Seq(items) => serde_json::Value::Array(items.iter().map(node_to_json).collect()),
        Value::Map(entries) => {
            let mut object = serde_json::Map::new();
            for entry in entries {
                object.insert(entry.key.clone(), node_to_json(&entry.value));
            }
            serde_json::Value::Object(object)
        }
        Value::Scalar(scalar) => match scalar {
            crate::loader::frontends::Scalar::Null => serde_json::Value::Null,
            crate::loader::frontends::Scalar::Bool(value) => serde_json::Value::Bool(*value),
            crate::loader::frontends::Scalar::Int(value) => match i64::try_from(*value) {
                Ok(small) => serde_json::Value::Number(small.into()),
                Err(_) => serde_json::Value::Null,
            },
            crate::loader::frontends::Scalar::Float(value) => serde_json::Number::from_f64(*value)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            crate::loader::frontends::Scalar::Str(text) => serde_json::Value::String(text.clone()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_home_is_absent_but_a_present_home_refuses_when_unreadable() {
        let root = std::env::temp_dir().join(format!(
            "lekalo-client-sdk-source-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp root");
        // Absent home: the honest skip.
        assert!(read_query_model(&root).expect("absent home").is_none());
        // A present home that is a DIRECTORY refuses — never a skip.
        std::fs::create_dir_all(root.join(QUERY_MODEL_SOURCE_PATH)).expect("dir slot");
        let set = read_query_model(&root).expect_err("directory home refuses");
        assert_eq!(
            set.as_slice()[0].id(),
            "client.input-invalid",
            "the refusal is the registered family rule"
        );
        // A malformed present home refuses with the syntax rule.
        std::fs::remove_dir_all(root.join(QUERY_MODEL_SOURCE_PATH)).expect("cleanup slot");
        std::fs::write(root.join(QUERY_MODEL_SOURCE_PATH), "???").expect("malformed bytes");
        let set = read_query_model(&root).expect_err("malformed home refuses");
        assert_eq!(set.as_slice()[0].id(), "query.input-invalid");
        let _ = std::fs::remove_dir_all(&root);
    }
}
