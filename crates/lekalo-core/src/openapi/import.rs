//! The bounded document import of the checked mode (issue #46,
//! plan §4.3/§8).
//!
//! `openapi.json` imports natively through the strict JSON parser;
//! `openapi.yaml` imports through the same closed block-style frontend
//! the loader uses — no anchors, no aliases, no tags, no flow
//! collections, no multi-document streams. Anything outside the subset
//! refuses as `openapi.input-invalid` with the frontend's stable
//! detail token; real-world documents relying on YAML anchors need
//! JSON (flagged for owner review, ADR-0043).

use serde_json::{json, Map, Value as Json};

use crate::diagnostics::DiagnosticSet;

use crate::loader::frontends::{parse_document, Node, Parsed, Scalar, Value};
use crate::loader::source::LineIndex;

use super::diagnostic;
use super::version::MAX_DOCUMENT_BYTES;

/// The imported document byte bound (the rendered-document bound).
pub const DOCUMENT_BYTES: usize = MAX_DOCUMENT_BYTES;

/// Parse one OpenAPI document from YAML or JSON text. The frontend
/// selection matches the loader: a leading `{` or `[` selects the
/// strict JSON frontend, anything else the YAML frontend.
///
/// The closed loader frontend refuses flow style wholesale, but block
/// YAML cannot spell an empty collection at all (`paths: {}`,
/// `security: []`) and the projection emits both. The import therefore
/// makes one bounded exception: the two *empty* flow literals outside
/// quoted scalars are rewritten to reserved placeholder tokens before
/// the closed frontend runs and mapped back after. Any other flow
/// content, anchors, aliases, tags, and multi-document streams still
/// refuse with the frontend's stable tokens.
pub fn parse_document_text(text: &str) -> Result<Json, DiagnosticSet> {
    if text.len() > DOCUMENT_BYTES {
        return Err(diagnostic::input_invalid("document-bytes"));
    }
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str(text).map_err(|_| diagnostic::input_invalid("invalid-json"));
    }
    if let Ok(document) = closed_frontend_parse(text) {
        return Ok(document);
    }
    let (rewritten, replaced) = rewrite_empty_flow(text);
    if replaced == 0 {
        return closed_frontend_parse(text);
    }
    let document = closed_frontend_parse(&rewritten)?;
    let restored = restore_empty_flow(&document);
    if restored.1 != replaced {
        // A quoted scalar carried the placeholder text: the rewrite is
        // ambiguous, so the refusal stands.
        return Err(diagnostic::input_invalid("placeholder-collision"));
    }
    Ok(restored.0)
}

/// The reserved placeholder of one empty flow mapping.
const EMPTY_OBJECT_TOKEN: &str = "lekalo-empty-map-7f3a";
/// The reserved placeholder of one empty flow sequence.
const EMPTY_ARRAY_TOKEN: &str = "lekalo-empty-seq-7f3a";

/// The closed loader frontend parse (block style, quoted scalars,
/// duplicate-key detection). Frontend rejections surface with their
/// stable detail token (`numeric-form`, `flow-style`, `anchor`, …) so
/// the refusal tells the operator exactly which spellings the closed
/// subset refuses — e.g. the OpenAPI version must be quoted
/// (`openapi: "3.1.0"`), because a bare dotted numeral is a refused
/// float-like form.
fn closed_frontend_parse(text: &str) -> Result<Json, DiagnosticSet> {
    let index = LineIndex::new(text);
    match parse_document(text, &index) {
        Ok(Parsed::Empty) => Err(diagnostic::input_invalid("empty")),
        Ok(Parsed::Root(node)) => {
            let mut duplicates = Vec::new();
            if has_duplicate_keys(&node, &mut duplicates) {
                return Err(diagnostic::input_invalid("duplicate-key"));
            }
            node_to_json(&node)
        }
        Err(diagnostics) => {
            let detail = diagnostics
                .first()
                .and_then(|diagnostic| diagnostic.data.as_ref())
                .and_then(|data| data.get("detail"))
                .and_then(|value| value.as_str())
                .unwrap_or("syntax");
            Err(diagnostic::input_invalid(detail))
        }
    }
}

/// Rewrite the two empty flow literals (`{}` and `[]`) to their
/// reserved placeholder tokens outside quoted scalars. Returns the
/// rewritten text and the replacement count.
fn rewrite_empty_flow(text: &str) -> (String, usize) {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut replaced = 0usize;
    let mut index = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'\'' if !in_double => {
                in_single = !in_single;
                out.push('\'');
                index += 1;
            }
            b'"' if !in_single => {
                in_double = !in_double;
                out.push('"');
                index += 1;
            }
            b'\\' if in_double => {
                // The escaped character of a double-quoted scalar.
                out.push('\\');
                if index + 1 < bytes.len() {
                    out.push(bytes[index + 1] as char);
                }
                index += 2;
            }
            b'{' if !in_single && !in_double && bytes.get(index + 1) == Some(&b'}') => {
                out.push_str(EMPTY_OBJECT_TOKEN);
                replaced += 1;
                index += 2;
            }
            b'[' if !in_single && !in_double && bytes.get(index + 1) == Some(&b']') => {
                out.push_str(EMPTY_ARRAY_TOKEN);
                replaced += 1;
                index += 2;
            }
            _ => {
                // Multi-byte UTF-8 sequences pass through byte-safe:
                // every non-ASCII byte lands in this arm unchanged.
                out.push(byte as char);
                index += 1;
            }
        }
    }
    (out, replaced)
}

/// Map the placeholder tokens back to their empty containers,
/// returning the document and the restoration count.
fn restore_empty_flow(document: &Json) -> (Json, usize) {
    match document {
        Json::String(text) if text == EMPTY_OBJECT_TOKEN => (json!({}), 1),
        Json::String(text) if text == EMPTY_ARRAY_TOKEN => (json!([]), 1),
        Json::Array(items) => {
            let mut restored = Vec::new();
            let mut count = 0;
            for item in items {
                let (value, inner) = restore_empty_flow(item);
                restored.push(value);
                count += inner;
            }
            (Json::Array(restored), count)
        }
        Json::Object(entries) => {
            let mut object = Map::new();
            let mut count = 0;
            for (key, value) in entries {
                let (mapped, inner) = restore_empty_flow(value);
                object.insert(key.clone(), mapped);
                count += inner;
            }
            (Json::Object(object), count)
        }
        other => (other.clone(), 0),
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

/// Convert one frontend node into its JSON form. Scalar typing follows
/// the closed frontend surface; numerals outside the JSON envelope
/// (i128 beyond i64, non-finite floats) are a bounded refusal — never a
/// silent lossy null.
fn node_to_json(node: &Node) -> Result<Json, DiagnosticSet> {
    match &node.value {
        Value::Scalar(scalar) => match scalar {
            Scalar::Null => Ok(Json::Null),
            Scalar::Bool(value) => Ok(Json::Bool(*value)),
            Scalar::Int(value) => match i64::try_from(*value) {
                Ok(small) => Ok(Json::Number(small.into())),
                Err(_) => Err(diagnostic::input_invalid("integer-out-of-range")),
            },
            Scalar::Float(value) => serde_json::Number::from_f64(*value)
                .map(Json::Number)
                .map(Ok)
                .unwrap_or_else(|| Err(diagnostic::input_invalid("float-out-of-range"))),
            Scalar::Str(text) => Ok(Json::String(text.clone())),
        },
        Value::Seq(items) => {
            let mut converted = Vec::new();
            for item in items {
                converted.push(node_to_json(item)?);
            }
            Ok(Json::Array(converted))
        }
        Value::Map(entries) => {
            let mut object = Map::new();
            for entry in entries {
                object.insert(entry.key.clone(), node_to_json(&entry.value)?);
            }
            Ok(Json::Object(object))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_documents_import_natively() {
        let document = parse_document_text(r#"{"openapi":"3.1.0","paths":{}}"#).expect("parses");
        assert_eq!(document["openapi"], "3.1.0");
    }

    #[test]
    fn block_style_yaml_imports_through_the_closed_frontend() {
        let text = "openapi: \"3.1.0\"\ninfo:\n  title: planner\n  version: \"0.4.0\"\npaths:\n  /tasks:\n    get:\n      operationId: plannerListTasks\n";
        let document = parse_document_text(text).expect("parses");
        assert_eq!(document["info"]["title"], "planner");
        assert_eq!(document["openapi"], "3.1.0");
        assert_eq!(
            document["paths"]["/tasks"]["get"]["operationId"],
            "plannerListTasks"
        );
    }

    #[test]
    fn empty_flow_collections_round_trip() {
        // The two empty flow literals are the bounded exception: block
        // YAML cannot spell them, and the projection emits both.
        let text = "openapi: \"3.1.0\"\npaths: {}\nsecurity: []\n";
        let document = parse_document_text(text).expect("parses");
        assert_eq!(document["paths"], json!({}));
        assert_eq!(document["security"], json!([]));
    }

    #[test]
    fn frontend_refusals_carry_their_stable_detail() {
        // A bare dotted numeral is a refused float-like form; the
        // refusal names it instead of a generic syntax tag.
        let set = parse_document_text("openapi: 3.1.0\n").expect_err("refuses");
        assert_eq!(set.as_slice()[0].id.as_str(), "openapi.input-invalid");
        let detail = match set.as_slice()[0].data.get("detail") {
            Some(crate::diagnostics::DataValue::Token(text)) => text.clone(),
            other => format!("{:?}", other),
        };
        assert!(detail.contains("numeric"), "{}", detail);
    }

    #[test]
    fn flow_collections_with_content_and_anchors_refuse() {
        // Flow mapping with content — outside the closed subset.
        assert!(parse_document_text("openapi: \"3.1.0\"\nversion: {v: 1}\n").is_err());
        // Anchors and aliases — refused.
        assert!(parse_document_text("a: &anchor 1\nb: *anchor\n").is_err());
        // Multi-document streams — refused.
        assert!(parse_document_text("openapi: \"3.1.0\"\n---\nsecond: doc\n").is_err());
    }

    #[test]
    fn a_placeholder_collision_refuses() {
        // A quoted scalar carrying the reserved token makes the
        // rewrite ambiguous; the refusal stands.
        let text = "openapi: 3.1.0\nnote: \"lekalo-empty-map-7f3a\"\npaths: {}\n";
        assert!(parse_document_text(text).is_err());
    }

    #[test]
    fn duplicate_keys_refuse() {
        let text = "openapi: 3.1.0\nopenapi: 3.0.0\n";
        assert!(parse_document_text(text).is_err());
    }

    #[test]
    fn out_of_range_numerals_refuse_instead_of_nulling() {
        // An i128 beyond i64 and a non-finite float are bounded
        // refusals — never a silent lossy null.
        let set = parse_document_text(
            "big: 99999999999999999999999
",
        )
        .expect_err("refuses");
        let detail = match set.as_slice()[0].data.get("detail") {
            Some(crate::diagnostics::DataValue::Token(text)) => text.clone(),
            other => format!("{:?}", other),
        };
        assert!(detail.contains("integer-out-of-range"), "{}", detail);
    }

    #[test]
    fn malformed_documents_refuse_with_the_registered_rule() {
        let set = parse_document_text("").expect_err("empty refuses");
        assert_eq!(set.as_slice()[0].id.as_str(), "openapi.input-invalid");
        let set = parse_document_text("openapi: [unclosed").expect_err("syntax refuses");
        assert_eq!(set.as_slice()[0].id.as_str(), "openapi.input-invalid");
    }
}
