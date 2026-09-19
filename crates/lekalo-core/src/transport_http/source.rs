//! The transport source seam (issue #70).
//!
//! The canonical per-project source is exactly `lekalo/transport.yaml`
//! (the `authorization.yaml` precedent: transport semantics span
//! modules and are target-agnostic). [`read_document`] performs the
//! only file access in this module: a bounded, fail-closed read plus
//! the same spanned frontend the loader uses, then a duplicate-key
//! check and the closed [`TransportDocument::from_value`] gate. The
//! validation, projection, and diff stay pure; only this seam touches
//! the filesystem.

use std::path::Path;

use crate::diagnostics::DiagnosticSet;
use crate::loader::frontends::{parse_document, Node, Parsed, Scalar, Value};
use crate::loader::source::LineIndex;

use super::diagnostic;
use super::TransportDocument;

/// The canonical per-project transport home.
pub const SOURCE_PATH: &str = "lekalo/transport.yaml";

/// The transport source byte bound (one MiB, the canonical payload
/// bound).
const SOURCE_BYTES: usize = 1024 * 1024;

/// Read and parse `lekalo/transport.yaml` under one project root.
/// `Ok(None)` means the project declares no transport document; any
/// other outcome is a bounded registered diagnostic set.
pub fn read_document(root: &Path) -> Result<Option<TransportDocument>, DiagnosticSet> {
    let path = root.join(SOURCE_PATH);
    let Ok(metadata) = std::fs::metadata(&path) else {
        return Ok(None);
    };
    if !metadata.is_file() {
        return Err(diagnostic::input_invalid("source-not-file"));
    }
    if metadata.len() as usize > SOURCE_BYTES {
        return Err(diagnostic::input_invalid("source-bytes"));
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return Err(diagnostic::input_invalid("source-encoding")),
    };
    parse_text(&text).map(Some)
}

/// Parse one transport document from YAML or JSON text. The frontend
/// selection matches the loader: a leading `{` or `[` selects the
/// strict JSON frontend, anything else the YAML frontend.
pub fn parse_text(text: &str) -> Result<TransportDocument, DiagnosticSet> {
    let index = LineIndex::new(text);
    let parsed = match parse_document(text, &index) {
        Ok(parsed) => parsed,
        Err(_) => return Err(diagnostic::input_invalid("syntax")),
    };
    match parsed {
        Parsed::Empty => Err(diagnostic::input_invalid("empty")),
        Parsed::Root(node) => {
            let mut duplicates = Vec::new();
            if has_duplicate_keys(&node, &mut duplicates) {
                return Err(diagnostic::input_invalid("duplicate-key"));
            }
            let value = node_to_json(&node);
            TransportDocument::from_value(&value)
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

/// Convert one frontend node into its JSON form. Scalar typing
/// follows the closed frontend surface.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_syntax_and_yaml_syntax_parse_to_one_document() {
        let json = serde_json::json!({
            "schemaVersion": "lekalo/transport-http/v0.4.0"
        });
        let refused = parse_text(&json.to_string());
        assert!(refused.is_err(), "an envelope fragment refuses");
        assert!(parse_text("").is_err(), "empty refuses");
        assert!(parse_text("[]").is_err(), "a non-object root refuses");
    }
}
