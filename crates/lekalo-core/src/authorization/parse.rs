//! The authorization source seam (issue #25).
//!
//! The canonical per-project source is exactly `lekalo/authorization.yaml`
//! (the accepted #4 successor path). [`read_document`] performs the only
//! file access in this module: a bounded, fail-closed read plus the same
//! spanned frontend the loader uses, then a duplicate-key check and the
//! closed [`Document::from_json`] gate. The evaluator and review stay
//! pure; only this seam touches the filesystem.

use super::diagnostic as diag;
use super::document::Document;
use super::version::limits;
use crate::diagnostics::DiagnosticSet;
use crate::loader::frontends::{parse_document, Node, Parsed, Scalar, Value};
use crate::loader::source::LineIndex;
use std::path::Path;

/// Read and parse `lekalo/authorization.yaml` under one project root.
/// `Ok(None)` means the project declares no authorization document; any
/// other outcome is a bounded registered diagnostic set.
pub fn read_document(root: &Path) -> Result<Option<Document>, DiagnosticSet> {
    let path = root.join("lekalo").join("authorization.yaml");
    let Ok(metadata) = std::fs::metadata(&path) else {
        return Ok(None);
    };
    if !metadata.is_file() {
        return Err(diag::invalid_set(
            diag::document_invalid("source-not-file", None)
                .into_iter()
                .collect(),
        ));
    }
    if metadata.len() as usize > limits::SOURCE_BYTES {
        return Err(diag::invalid_set(
            diag::limit_exceeded("source-bytes", metadata.len())
                .into_iter()
                .collect(),
        ));
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => {
            return Err(diag::invalid_set(
                diag::document_invalid("source-encoding", None)
                    .into_iter()
                    .collect(),
            ));
        }
    };
    parse_text(&text).map(Some)
}

/// Parse one authorization document from YAML or JSON text. The
/// frontend selection matches the loader: a leading `{` or `[` selects
/// the strict JSON frontend, anything else the YAML frontend.
pub fn parse_text(text: &str) -> Result<Document, DiagnosticSet> {
    let index = LineIndex::new(text);
    let parsed = match parse_document(text, &index) {
        Ok(parsed) => parsed,
        Err(_) => {
            return Err(diag::invalid_set(
                diag::document_invalid("syntax", None).into_iter().collect(),
            ));
        }
    };
    match parsed {
        Parsed::Empty => Err(diag::invalid_set(
            diag::document_invalid("empty", None).into_iter().collect(),
        )),
        Parsed::Root(node) => {
            let mut duplicates = Vec::new();
            if has_duplicate_keys(&node, &mut duplicates) {
                return Err(diag::invalid_set(
                    diag::document_invalid("duplicate-key", duplicates.first().map(String::as_str))
                        .into_iter()
                        .collect(),
                ));
            }
            let value = node_to_json(&node);
            Document::from_json(&value)
        }
    }
}

/// Reject duplicate mapping keys anywhere in the tree: duplicates are
/// never silently merged.
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

/// Convert one frontend node into its JSON form. Scalar typing follows
/// the closed frontend surface: null, booleans, base-10 integers, and
/// strings.
fn node_to_json(node: &Node) -> serde_json::Value {
    match &node.value {
        Value::Scalar(scalar) => match scalar {
            Scalar::Null => serde_json::Value::Null,
            Scalar::Bool(value) => serde_json::Value::Bool(*value),
            Scalar::Int(value) => match i64::try_from(*value) {
                Ok(small) => serde_json::Value::Number(small.into()),
                Err(_) => serde_json::Value::Null,
            },
            Scalar::Float(value) => serde_json::Number::from_f64(*value)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Scalar::Str(text) => serde_json::Value::String(text.clone()),
        },
        Value::Seq(items) => serde_json::Value::Array(items.iter().map(node_to_json).collect()),
        Value::Map(entries) => {
            let mut object = serde_json::Map::new();
            for entry in entries {
                object.insert(entry.key.clone(), node_to_json(&entry.value));
            }
            serde_json::Value::Object(object)
        }
    }
}
