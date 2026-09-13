//! Canonical serialization of the expressions attachment (issue #66).
//!
//! One deterministic byte form per semantic attachment: compact
//! UTF-8 JSON with byte-sorted keys, set literals in sorted
//! duplicate-free form, and records ordered by identity. Combinator
//! operand lists, parameter lists, and span fields keep their
//! declared behavioral order. Two semantically equal attachments
//! produce byte-identical canonical output; the digest over these
//! bytes is the attachment identity for diff and impact consumers.

use serde_json::Value as Json;

use super::ast::ExprNode;
use super::types::{datetime_render, Scalar};
use super::version::MAX_CANONICAL_BYTES;
use super::{diagnostic, ExpressionRecord, ExpressionsAttachment};

/// Render the canonical bytes of one validated attachment, or the
/// registered over-limit rejection.
pub fn canonical_bytes(
    attachment: &ExpressionsAttachment,
) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let members: Vec<(String, String)> = vec![
        (
            "attachmentRevision".to_owned(),
            string(attachment.attachment_revision().as_str()),
        ),
        (
            "builtinSemantics".to_owned(),
            string(attachment.builtin_semantics()),
        ),
        (
            "expressions".to_owned(),
            array(
                attachment
                    .expressions()
                    .iter()
                    .map(record_bytes)
                    .collect::<Vec<String>>(),
            ),
        ),
        ("identity".to_owned(), string(super::version::IDENTITY)),
        (
            "irRef".to_owned(),
            object(vec![
                ("digest", string(attachment.ir_digest().as_str())),
                ("identity", string(attachment.ir_identity())),
            ]),
        ),
        (
            "modelRef".to_owned(),
            object(vec![
                ("digest", string(attachment.model_ref().digest().as_str())),
                (
                    "modelVersion",
                    string(attachment.model_ref().model_version().as_str()),
                ),
            ]),
        ),
        (
            "projectId".to_owned(),
            string(attachment.project_id().as_str()),
        ),
        (
            "schemaVersion".to_owned(),
            string(super::version::SCHEMA_VERSION),
        ),
    ];
    let bytes = object(members);
    if bytes.len() > MAX_CANONICAL_BYTES {
        return Err(diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// One canonical record object.
fn record_bytes(record: &ExpressionRecord) -> String {
    let mut members: Vec<(String, String)> = Vec::new();
    members.push(("body".to_owned(), node_bytes(record.body())));
    if let Some(description) = record.description() {
        members.push(("description".to_owned(), string(description)));
    }
    members.push(("id".to_owned(), string(record.id())));
    members.push(("kind".to_owned(), string(record.kind().key())));
    members.push((
        "params".to_owned(),
        array(
            record
                .params()
                .iter()
                .map(|param| {
                    let mut entries: Vec<(String, String)> =
                        vec![("field".to_owned(), string(param.field()))];
                    if param.nullable() {
                        entries.push(("nullable".to_owned(), "true".to_owned()));
                    }
                    entries.push(("scope".to_owned(), string(param.scope().key())));
                    entries.push(("type".to_owned(), string(&param.ty().key())));
                    object(entries)
                })
                .collect(),
        ),
    ));
    members.push(("result".to_owned(), string(&record.result().key())));
    if let Some(span) = record.span() {
        let Some(range) = span.range.as_ref() else {
            unreachable!("wire spans always carry a range");
        };
        members.push((
            "span".to_owned(),
            object(vec![
                ("endByte", range.end.byte.to_string()),
                ("endColumn", range.end.column.to_string()),
                ("endLine", range.end.line.to_string()),
                ("file", string(&span.path.clone().unwrap_or_default())),
                ("startByte", range.start.byte.to_string()),
                ("startColumn", range.start.column.to_string()),
                ("startLine", range.start.line.to_string()),
            ]),
        ));
    }
    if let Some(target) = record.target() {
        members.push((
            "target".to_owned(),
            object(vec![
                ("field", string(target.field())),
                ("scope", string(target.scope().key())),
                ("type", string(&target.ty().key())),
            ]),
        ));
    }
    object(members)
}

/// One canonical AST node object.
fn node_bytes(node: &ExprNode) -> String {
    match node {
        ExprNode::Bool(value) => object(vec![
            ("op", string("lit-bool")),
            ("value", value.to_string()),
        ]),
        ExprNode::Int(value) => object(vec![
            ("op", string("lit-int")),
            ("value", value.to_string()),
        ]),
        ExprNode::Str(value) => {
            object(vec![("op", string("lit-string")), ("value", string(value))])
        }
        ExprNode::DateTime(seconds) => object(vec![
            ("op", string("lit-datetime")),
            ("value", string(&datetime_render(*seconds))),
        ]),
        ExprNode::Duration(seconds) => object(vec![
            ("op", string("lit-duration")),
            ("value", seconds.to_string()),
        ]),
        ExprNode::Set(items) => object(vec![
            (
                "items",
                array(
                    items
                        .iter()
                        .map(|scalar| match scalar {
                            Scalar::Bool(value) => {
                                object(vec![("kind", string("bool")), ("value", value.to_string())])
                            }
                            Scalar::Int(value) => {
                                object(vec![("kind", string("int")), ("value", value.to_string())])
                            }
                            Scalar::Str(value) => {
                                object(vec![("kind", string("string")), ("value", string(value))])
                            }
                            Scalar::DateTime(seconds) => object(vec![
                                ("kind", string("datetime")),
                                ("value", string(&datetime_render(*seconds))),
                            ]),
                            Scalar::Duration(seconds) => object(vec![
                                ("kind", string("duration")),
                                ("value", seconds.to_string()),
                            ]),
                        })
                        .collect(),
                ),
            ),
            ("op", string("lit-set")),
        ]),
        ExprNode::Ref { scope, field } => object(vec![
            ("field", string(field)),
            ("op", string("ref")),
            ("scope", string(scope.key())),
        ]),
        ExprNode::Now => object(vec![("op", string("now"))]),
        ExprNode::Equal { left, right } => binary("eq", left, right),
        ExprNode::NotEqual { left, right } => binary("ne", left, right),
        ExprNode::Less { left, right } => binary("lt", left, right),
        ExprNode::LessEqual { left, right } => binary("le", left, right),
        ExprNode::Greater { left, right } => binary("gt", left, right),
        ExprNode::GreaterEqual { left, right } => binary("ge", left, right),
        ExprNode::Add { left, right } => binary("add", left, right),
        ExprNode::Subtract { left, right } => binary("sub", left, right),
        ExprNode::Multiply { left, right } => binary("mul", left, right),
        ExprNode::Divide { left, right } => binary("div", left, right),
        ExprNode::Modulo { left, right } => binary("mod", left, right),
        ExprNode::IsNull { operand } => unary("is-null", operand),
        ExprNode::NotNull { operand } => unary("not-null", operand),
        ExprNode::Not { operand } => unary("not", operand),
        ExprNode::InSet { operand, set } => object(vec![
            ("operand", node_bytes(operand)),
            ("op", string("in-set")),
            ("set", node_bytes(set)),
        ]),
        ExprNode::NotInSet { operand, set } => object(vec![
            ("operand", node_bytes(operand)),
            ("op", string("not-in-set")),
            ("set", node_bytes(set)),
        ]),
        ExprNode::And(operands) => combinator("and", operands),
        ExprNode::Or(operands) => combinator("or", operands),
        ExprNode::If {
            condition,
            then,
            otherwise,
        } => object(vec![
            ("condition", node_bytes(condition)),
            ("else", node_bytes(otherwise)),
            ("op", string("if")),
            ("then", node_bytes(then)),
        ]),
        ExprNode::Builtin { name, args } => object(vec![
            ("args", array(args.iter().map(node_bytes).collect())),
            ("name", string(name)),
            ("op", string("builtin")),
        ]),
    }
}

/// One canonical binary operator node.
fn binary(op: &str, left: &ExprNode, right: &ExprNode) -> String {
    object(vec![
        ("left", node_bytes(left)),
        ("op", string(op)),
        ("right", node_bytes(right)),
    ])
}

/// One canonical unary operator node.
fn unary(op: &str, operand: &ExprNode) -> String {
    object(vec![("op", string(op)), ("operand", node_bytes(operand))])
}

/// One canonical boolean combinator node.
fn combinator(op: &str, operands: &[ExprNode]) -> String {
    object(vec![
        ("operands", array(operands.iter().map(node_bytes).collect())),
        ("op", string(op)),
    ])
}

/// One compact JSON object with byte-sorted keys.
fn object(members: Vec<(impl AsRef<str>, String)>) -> String {
    let mut members = members;
    members.sort_by(|left, right| left.0.as_ref().as_bytes().cmp(right.0.as_ref().as_bytes()));
    let joined: Vec<String> = members
        .iter()
        .map(|(key, value)| format!("{}:{}", string_key(key.as_ref()), value))
        .collect();
    format!("{{{}}}", joined.join(","))
}

/// One compact JSON array.
fn array(items: Vec<String>) -> String {
    format!("[{}]", items.join(","))
}

/// One canonical JSON string (the serde canonical escaping).
fn string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

/// One canonical JSON string key.
fn string_key(text: &str) -> String {
    string(text)
}

/// The typed wire JSON of one value (shared with the vector
/// expectations); re-exported for the CLI projections.
pub fn value_json(value: &super::Value) -> Json {
    value.to_json()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bytes_are_sorted_and_stable() {
        let document = serde_json::json!({
            "schemaVersion": super::super::version::SCHEMA_VERSION,
            "identity": super::super::version::IDENTITY,
            "attachmentRevision": "1.0.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": super::super::version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "builtinSemantics": super::super::version::BUILTIN_SEMANTICS_VERSION,
            "expressions": [
                {
                    "id": "expr.planner/overdue-check",
                    "kind": "condition",
                    "params": [],
                    "result": "bool",
                    "body": {"op": "not", "operand": {"op": "lit-bool", "value": false}}
                }
            ]
        });
        let attachment = super::super::ExpressionsAttachment::from_value(&document).expect("valid");
        let first = canonical_bytes(&attachment).expect("canonical");
        let second = canonical_bytes(&attachment).expect("canonical");
        assert_eq!(first, second);
        // Round-trip through the wire decoder reproduces the bytes.
        let reparsed: Json = serde_json::from_str(&first).expect("canonical parses");
        let again = super::super::ExpressionsAttachment::from_value(&reparsed).expect("revalid");
        assert_eq!(canonical_bytes(&again).expect("canonical"), first);
    }
}
