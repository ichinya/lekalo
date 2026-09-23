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

/// The honest refusal detail of one frontend rejection: the stable
/// classification when the diagnostic carries one, else the bounded
/// key / limit the loader pinned (duplicate-key and limit-exceeded
/// carry no generic `detail` member), else the generic `syntax` tag.
fn frontend_detail(diagnostic: Option<&crate::loader::error::Diagnostic>) -> String {
    let Some(data) = diagnostic.and_then(|diagnostic| diagnostic.data.as_ref()) else {
        return "syntax".to_owned();
    };
    if let Some(text) = data.get("detail").and_then(|value| value.as_str()) {
        return super::diagnostic::bounded(text);
    }
    if let Some(key) = data.get("key").and_then(|value| value.as_str()) {
        return super::diagnostic::bounded(&format!("duplicate-key:{}", key));
    }
    if let Some(limit) = data.get("limit").and_then(|value| value.as_str()) {
        return super::diagnostic::bounded(&format!("limit:{}", limit));
    }
    "syntax".to_owned()
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
        Ok(Parsed::Root(node)) => node_to_json(&node),
        Err(diagnostics) => Err(diagnostic::input_invalid(&frontend_detail(
            diagnostics.first(),
        ))),
    }
}

/// Rewrite the two empty flow literals (`{}` and `[]`) to their
/// reserved placeholder tokens. The scan is char-exact UTF-8, tracks
/// quoted scalars, skips `#` comments and block-scalar bodies, and
/// rewrites only token-position literals — a `{}` inside a comment, a
/// block scalar, or a plain scalar's prose is left untouched, so the
/// replacement count can never exceed what the parsed tree can
/// restore (r1 F-5). Returns the rewritten text and the replacement
/// count.
fn rewrite_empty_flow(text: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut replaced = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut double_escaped = false;
    // The indent threshold of the active block-scalar header line, if
    // any: its body (and blank separators) rides verbatim.
    let mut block_body_indent: Option<usize> = None;

    for line in text.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        let blank = trimmed.is_empty() || trimmed == "\n" || trimmed == "\r\n";
        if let Some(threshold) = block_body_indent {
            if blank || indent > threshold {
                out.push_str(line);
                continue;
            }
            block_body_indent = None;
        }

        let mut comment = false;
        // The immediately preceding char of this line (for the `#`
        // comment rule) and the last significant (non-blank) char (for
        // the token-position and block-header bounds).
        let mut immediate_prev: Option<char> = None;
        let mut last_significant: Option<char> = None;
        let mut rest = line;
        while let Some(c) = rest.chars().next() {
            rest = &rest[c.len_utf8()..];
            if comment {
                out.push(c);
                continue;
            }
            if in_double {
                out.push(c);
                if double_escaped {
                    double_escaped = false;
                } else if c == '\\' {
                    double_escaped = true;
                } else if c == '"' {
                    in_double = false;
                }
                immediate_prev = Some(c);
                if c != ' ' && c != '\t' {
                    last_significant = Some(c);
                }
                continue;
            }
            if in_single {
                out.push(c);
                if c == '\'' {
                    if rest.starts_with('\'') {
                        // The '' escape of one single quote.
                        out.push('\'');
                        rest = &rest[1..];
                    } else {
                        in_single = false;
                    }
                }
                immediate_prev = Some(c);
                if c != ' ' && c != '\t' {
                    last_significant = Some(c);
                }
                continue;
            }
            match c {
                '"' => {
                    in_double = true;
                    out.push(c);
                }
                '\'' => {
                    in_single = true;
                    out.push(c);
                }
                '#' if immediate_prev.is_none()
                    || immediate_prev == Some(' ')
                    || immediate_prev == Some('\t') =>
                {
                    comment = true;
                    out.push(c);
                }
                '|' | '>'
                    if matches!(last_significant, None | Some(':') | Some('-'))
                        && rest
                            .split('\n')
                            .next()
                            .unwrap_or("")
                            .trim_end_matches('\r')
                            .chars()
                            .all(|header| {
                                header.is_ascii_digit() || header == '+' || header == '-'
                            }) =>
                {
                    // The block-scalar header: the rest of the line is
                    // only indent/chomping indicators, so the body —
                    // every following line more indented than this
                    // one — rides verbatim.
                    out.push(c);
                    out.push_str(rest);
                    rest = "";
                    block_body_indent = Some(indent);
                }
                '{' if rest.starts_with('}') => {
                    if matches!(
                        last_significant,
                        None | Some(':') | Some(',') | Some('[') | Some('{') | Some('-')
                    ) {
                        out.push_str(EMPTY_OBJECT_TOKEN);
                        replaced += 1;
                    } else {
                        out.push_str("{}");
                    }
                    rest = &rest[1..];
                    last_significant = Some('}');
                    immediate_prev = Some('}');
                    continue;
                }
                '[' if rest.starts_with(']') => {
                    if matches!(
                        last_significant,
                        None | Some(':') | Some(',') | Some('[') | Some('{') | Some('-')
                    ) {
                        out.push_str(EMPTY_ARRAY_TOKEN);
                        replaced += 1;
                    } else {
                        out.push_str("[]");
                    }
                    rest = &rest[1..];
                    last_significant = Some(']');
                    immediate_prev = Some(']');
                    continue;
                }
                _ => out.push(c),
            }
            immediate_prev = Some(c);
            if c != ' ' && c != '\t' {
                last_significant = Some(c);
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
    fn non_ascii_yaml_imports_byte_exact_through_the_empty_flow_rewrite() {
        // Every multi-byte char rides the rewrite unchanged — the old
        // byte-at-a-time `as char` widening mojibake'd exactly these.
        let text = "openapi: \"3.1.0\"\ninfo:\n  title: Планировщик задач — 计划器\npaths: {}\n";
        let document = parse_document_text(text).expect("parses");
        assert_eq!(document["info"]["title"], "Планировщик задач — 计划器");
        assert_eq!(document["paths"], json!({}));
    }

    #[test]
    fn comments_and_block_scalars_survive_the_empty_flow_rewrite() {
        // A `{}` inside a comment or a block-scalar body is prose, not
        // a flow literal: it must neither be rewritten (the parsed tree
        // could never restore it) nor trigger a placeholder refusal.
        let text = concat!(
            "openapi: \"3.1.0\"\n",
            "# note: emit {} when empty and [] likewise\n",
            "info:\n",
            "  title: planner\n",
            "  description: |\n",
            "    The shape stays {} and the list stays [] here.\n",
            "    Still inside: {}\n",
            "  version: \"0.4.0\"\n",
            "paths: {}\n",
            "security: []\n",
        );
        let document = parse_document_text(text).expect("parses");
        assert_eq!(
            document["info"]["description"],
            "The shape stays {} and the list stays [] here.\nStill inside: {}\n"
        );
        assert_eq!(document["paths"], json!({}));
        assert_eq!(document["security"], json!([]));
    }

    #[test]
    fn a_plain_scalar_prose_pair_stays_literal() {
        // `{}` in plain-scalar prose is not in token position: the
        // bounded rewrite leaves it, so no phantom collision arises.
        let text = "openapi: \"3.1.0\"\nnote: emit {} when empty\n";
        let document = parse_document_text(text).expect("parses");
        assert_eq!(document["note"], "emit {} when empty");
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
    fn duplicate_keys_name_the_key_in_the_detail() {
        // The loader pins the duplicate key; the refusal surfaces it
        // instead of the generic syntax tag (r1 devin F-10).
        let set =
            parse_document_text("openapi: \"3.1.0\"\nopenapi: \"3.0.0\"\n").expect_err("refuses");
        let detail = match set.as_slice()[0].data.get("detail") {
            Some(crate::diagnostics::DataValue::Token(text)) => text.clone(),
            other => format!("{:?}", other),
        };
        assert!(
            detail.starts_with("duplicate-key:"),
            "honest duplicate-key detail, got {}",
            detail
        );
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
