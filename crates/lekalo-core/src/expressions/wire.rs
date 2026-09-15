//! Wire normalization of the expressions attachment (issue #66).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`ExpressionsAttachment`]. It fails closed before semantic
//! processing: unknown or missing fields, wrong identities,
//! malformed identifiers, digests, literals, spans, and out-of-bound
//! trees each return one typed registered diagnostic and no partial
//! attachment. The exhaustive static typing (operator/type
//! combinations, complexity bounds, kind/target coherence) runs as
//! the attachment's semantic self-check; a body that exceeds the
//! closed grammar rejects with `expression.complexity-limit`, which
//! routes to the foreign implementation family (#30).

use crate::diagnostics::{DiagnosticSet, Range, SourceLocation};
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{FieldName, SemanticId};
use serde_json::Value as Json;

use super::ast::{ExprNode, RefScope};
use super::builtin::EvalError;
use super::diagnostic;
use super::types::{
    datetime_parse, duration_in_range, int_in_range, string_is_canonical, ExprType, Scalar,
};
use super::version::{
    self, BUILTIN_SEMANTICS_VERSION, IDENTITY, IR_IDENTITY, MAX_DESCRIPTION_BYTES, MAX_EXPRESSIONS,
    MAX_LITERAL_BYTES, MAX_NODES, MAX_OPERANDS, MAX_PARAMS, MAX_SET_ITEMS, MAX_SPAN_PATH_BYTES,
    MAX_SPAN_POSITION, SCHEMA_VERSION, SUPPORT_SCHEMA_VERSION, VECTORS_SCHEMA_VERSION,
};
use super::{
    AssignmentTarget, ExpressionKind, ExpressionRecord, ExpressionsAttachment, ModelPin, ParamDecl,
};

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "builtinSemantics",
    "expressions",
];

/// The closed expression-record member set.
const RECORD_KEYS: &[&str] = &[
    "id",
    "kind",
    "description",
    "params",
    "result",
    "target",
    "body",
    "span",
];

/// The required expression-record members.
const RECORD_REQUIRED_KEYS: &[&str] = &["id", "kind", "params", "result", "body"];

/// The closed param member set.
const PARAM_KEYS: &[&str] = &["scope", "field", "type", "nullable"];

/// The closed target member set.
const TARGET_KEYS: &[&str] = &["scope", "field", "type"];

/// The closed span member set.
const SPAN_KEYS: &[&str] = &[
    "file",
    "startByte",
    "startLine",
    "startColumn",
    "endByte",
    "endLine",
    "endColumn",
];

/// The closed top-level member set of one evaluation-vector document.
const VECTOR_KEYS: &[&str] = &["id", "expression", "clock", "bindings", "expect"];

/// Normalize one wire document into a validated attachment, or
/// return the typed rejection set with no partial attachment. Pure:
/// no source, model, cache, report, network, process, or target
/// access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<ExpressionsAttachment, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("top-level-shape"))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    for key in TOP_LEVEL_KEYS {
        if !object.contains_key(*key) {
            return Err(diagnostic::input_invalid("missing-field"));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str) != Some(SCHEMA_VERSION) {
        return Err(diagnostic::input_invalid("schema-version"));
    }
    if object.get("identity").and_then(Json::as_str) != Some(IDENTITY) {
        return Err(diagnostic::input_invalid("contract-identity"));
    }
    let attachment_revision = SemVer::parse(
        object
            .get("attachmentRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("attachment-revision"))?,
    )
    .map_err(|_| diagnostic::input_invalid("attachment-revision"))?;
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("project-id"))?,
    )
    .map_err(|_| diagnostic::input_invalid("project-id"))?;
    let model_ref = model_pin(
        object
            .get("modelRef")
            .ok_or_else(|| diagnostic::input_invalid("model-ref"))?,
    )?;
    let (ir_identity, ir_digest) = ir_ref(
        object
            .get("irRef")
            .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?,
    )?;
    if object.get("builtinSemantics").and_then(Json::as_str) != Some(BUILTIN_SEMANTICS_VERSION) {
        return Err(diagnostic::input_invalid("builtin-semantics"));
    }
    let raw_expressions = object
        .get("expressions")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("expression-list"))?;
    if raw_expressions.is_empty() || raw_expressions.len() > MAX_EXPRESSIONS {
        return Err(diagnostic::input_invalid("expression-list"));
    }
    let mut expressions = Vec::with_capacity(raw_expressions.len());
    for raw in raw_expressions {
        expressions.push(record(raw)?);
    }
    let mut seen: Vec<&str> = expressions.iter().map(|r| r.id.as_str()).collect();
    seen.sort_unstable();
    for pair in seen.windows(2) {
        if pair[0] == pair[1] {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "expression-duplicate",
                None,
                None,
            ));
        }
    }
    expressions.sort_by(|left, right| left.id.cmp(&right.id));
    let attachment = ExpressionsAttachment::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_identity,
        ir_digest,
        BUILTIN_SEMANTICS_VERSION.to_owned(),
        expressions,
    );
    for record in attachment.expressions() {
        super::typing::check_record(record)?;
    }
    Ok(attachment)
}

/// Decode the closed model pin.
fn model_pin(json: &Json) -> Result<ModelPin, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("model-ref"))?;
    if object
        .keys()
        .any(|key| !["modelVersion", "digest"].contains(&key.as_str()))
    {
        return Err(diagnostic::input_invalid("model-ref"));
    }
    let model_version = object
        .get("modelVersion")
        .and_then(Json::as_str)
        .and_then(|text| SemVer::parse(text).ok())
        .ok_or_else(|| diagnostic::input_invalid("model-ref"))?;
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("model-ref"))?,
    )
    .map_err(|_| diagnostic::input_invalid("model-ref"))?;
    Ok(ModelPin {
        model_version,
        digest,
    })
}

/// Decode the closed IR reference.
fn ir_ref(json: &Json) -> Result<(String, Sha256Digest), DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?;
    if object
        .keys()
        .any(|key| !["identity", "digest"].contains(&key.as_str()))
    {
        return Err(diagnostic::input_invalid("ir-ref"));
    }
    if object.get("identity").and_then(Json::as_str) != Some(IR_IDENTITY) {
        return Err(diagnostic::input_invalid("ir-ref"));
    }
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?,
    )
    .map_err(|_| diagnostic::input_invalid("ir-ref"))?;
    Ok((IR_IDENTITY.to_owned(), digest))
}

/// Decode one expression record.
fn record(json: &Json) -> Result<ExpressionRecord, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("record-shape"))?;
    for key in object.keys() {
        if !RECORD_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("record-unknown-field"));
        }
    }
    for key in RECORD_REQUIRED_KEYS {
        if !object.contains_key(*key) {
            return Err(diagnostic::input_invalid("record-missing-field"));
        }
    }
    let id = object
        .get("id")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("expression-id"))?;
    if crate::invariant_transition::id::ExpressionRef::parse(id).is_err() {
        return Err(diagnostic::rule_invalid(
            diagnostic::CONTRACT_INVALID,
            "expression-id",
            None,
            None,
        ));
    }
    let kind = ExpressionKind::parse(
        object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("kind"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("kind"))?;
    let description = match object.get("description") {
        Some(value) => {
            let text = value
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("description"))?;
            if text.len() > MAX_DESCRIPTION_BYTES || !string_is_canonical(text) {
                return Err(diagnostic::input_invalid("description"));
            }
            Some(text.to_owned())
        }
        None => None,
    };
    let params = params(object.get("params"))?;
    let result = type_member(object.get("result"))?;
    let target = match object.get("target") {
        Some(value) => Some(target(value)?),
        None => None,
    };
    let span = match object.get("span") {
        Some(value) => Some(span(value)?),
        None => None,
    };
    let mut nodes = 0usize;
    let body = node(
        object
            .get("body")
            .ok_or_else(|| diagnostic::input_invalid("record-missing-field"))?,
        0,
        &mut nodes,
    )?;
    Ok(ExpressionRecord {
        id: id.to_owned(),
        kind,
        description,
        params,
        result,
        target,
        body,
        span,
    })
}

/// Decode the declared reference list.
fn params(json: Option<&Json>) -> Result<Vec<ParamDecl>, DiagnosticSet> {
    let Some(json) = json else {
        return Err(diagnostic::input_invalid("param-shape"));
    };
    let array = json
        .as_array()
        .ok_or_else(|| diagnostic::input_invalid("param-shape"))?;
    if array.len() > MAX_PARAMS {
        return Err(diagnostic::rule_invalid(
            diagnostic::CONTRACT_INVALID,
            "param-bound",
            None,
            None,
        ));
    }
    let mut params = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("param-shape"))?;
        for key in object.keys() {
            if !PARAM_KEYS.contains(&key.as_str()) {
                return Err(diagnostic::input_invalid("param-unknown-field"));
            }
        }
        if !object.contains_key("scope") || !object.contains_key("field") {
            return Err(diagnostic::input_invalid("param-shape"));
        }
        let scope = RefScope::parse(
            object
                .get("scope")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("param-shape"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("param-shape"))?;
        let field = FieldName::parse(
            object
                .get("field")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("param-shape"))?,
        )
        .map_err(|_| diagnostic::input_invalid("param-shape"))?;
        let ty = type_member(object.get("type"))?;
        let nullable = match object.get("nullable") {
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("param-shape"))?,
            None => false,
        };
        params.push(ParamDecl {
            scope,
            field: field.as_str().to_owned(),
            ty,
            nullable,
        });
    }
    for index in 0..params.len() {
        for other in index + 1..params.len() {
            if params[index].scope == params[other].scope
                && params[index].field == params[other].field
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CONTRACT_INVALID,
                    "param-duplicate",
                    None,
                    None,
                ));
            }
        }
    }
    Ok(params)
}

/// Decode the assignment target.
fn target(json: &Json) -> Result<AssignmentTarget, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("target-shape"))?;
    for key in object.keys() {
        if !TARGET_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("target-shape"));
        }
    }
    if !object.contains_key("scope") || !object.contains_key("field") {
        return Err(diagnostic::input_invalid("target-shape"));
    }
    let scope = RefScope::parse(
        object
            .get("scope")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("target-shape"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("target-shape"))?;
    let field = FieldName::parse(
        object
            .get("field")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("target-shape"))?,
    )
    .map_err(|_| diagnostic::input_invalid("target-shape"))?;
    let ty = type_member(object.get("type"))?;
    Ok(AssignmentTarget {
        scope,
        field: field.as_str().to_owned(),
        ty,
    })
}

/// Decode the declared source span into the shared diagnostic
/// location shape.
fn span(json: &Json) -> Result<SourceLocation, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("span-shape"))?;
    for key in object.keys() {
        if !SPAN_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("span-shape"));
        }
    }
    if object.len() != SPAN_KEYS.len() {
        return Err(diagnostic::input_invalid("span-shape"));
    }
    let file = object
        .get("file")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("span-shape"))?;
    if file.is_empty()
        || file.len() > MAX_SPAN_PATH_BYTES
        || !file.bytes().all(|byte| {
            matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'/' | b'_' | b'-')
        })
        || file.starts_with('/')
        || file.split('/').any(|segment| segment.is_empty() || segment == ".." || segment == ".")
    {
        return Err(diagnostic::input_invalid("span-shape"));
    }
    let position = |key: &str, minimum: usize| -> Result<usize, DiagnosticSet> {
        let value = object
            .get(key)
            .and_then(Json::as_u64)
            .ok_or_else(|| diagnostic::input_invalid("span-shape"))?;
        if value < minimum as u64 || value > MAX_SPAN_POSITION as u64 {
            return Err(diagnostic::input_invalid("span-shape"));
        }
        Ok(value as usize)
    };
    let start_byte = position("startByte", 0)?;
    let start_line = position("startLine", 1)?;
    let start_column = position("startColumn", 1)?;
    let end_byte = position("endByte", 0)?;
    let end_line = position("endLine", 1)?;
    let end_column = position("endColumn", 1)?;
    if start_byte > end_byte || (start_line, start_column) > (end_line, end_column) {
        return Err(diagnostic::input_invalid("span-shape"));
    }
    Ok(SourceLocation {
        path: Some(file.to_owned()),
        range: Some(Range {
            start: super::super::diagnostics::Position {
                byte: start_byte,
                line: start_line,
                column: start_column,
            },
            end: super::super::diagnostics::Position {
                byte: end_byte,
                line: end_line,
                column: end_column,
            },
        }),
    })
}

/// Decode one type member.
fn type_member(json: Option<&Json>) -> Result<ExprType, DiagnosticSet> {
    let ty = json
        .and_then(Json::as_str)
        .and_then(ExprType::parse)
        .ok_or_else(|| diagnostic::input_invalid("type-member"))?;
    Ok(ty)
}

/// Decode one body node with a wire-level node budget.
fn node(json: &Json, depth: usize, nodes: &mut usize) -> Result<ExprNode, DiagnosticSet> {
    *nodes += 1;
    if *nodes > MAX_NODES || depth > version::MAX_DEPTH {
        return Err(diagnostic::rule_invalid(
            diagnostic::COMPLEXITY_LIMIT,
            "nodes",
            None,
            None,
        ));
    }
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("node-shape"))?;
    let op = object
        .get("op")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("node-shape"))?;
    let keys = |allowed: &[&str]| -> Result<(), DiagnosticSet> {
        if object.len() != allowed.len() {
            return Err(diagnostic::input_invalid("node-shape"));
        }
        for key in object.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(diagnostic::input_invalid("node-shape"));
            }
        }
        Ok(())
    };
    let member = |name: &str| -> Result<&Json, DiagnosticSet> {
        object
            .get(name)
            .ok_or_else(|| diagnostic::input_invalid("node-shape"))
    };
    let mut binary = |left_name: &str,
                      right_name: &str|
     -> Result<(Box<ExprNode>, Box<ExprNode>), DiagnosticSet> {
        let left = Box::new(node(member(left_name)?, depth + 1, nodes)?);
        let right = Box::new(node(member(right_name)?, depth + 1, nodes)?);
        Ok((left, right))
    };
    match op {
        "lit-bool" => {
            keys(&["op", "value"])?;
            Ok(ExprNode::Bool(
                object
                    .get("value")
                    .and_then(Json::as_bool)
                    .ok_or_else(|| diagnostic::input_invalid("literal-shape"))?,
            ))
        }
        "lit-int" => {
            keys(&["op", "value"])?;
            let value = object
                .get("value")
                .and_then(Json::as_i64)
                .ok_or_else(|| diagnostic::input_invalid("literal-shape"))?;
            if !int_in_range(value) {
                return Err(diagnostic::input_invalid("literal-shape"));
            }
            Ok(ExprNode::Int(value))
        }
        "lit-string" => {
            keys(&["op", "value"])?;
            let value = object
                .get("value")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("literal-shape"))?;
            if value.len() > MAX_LITERAL_BYTES || !string_is_canonical(value) {
                return Err(diagnostic::input_invalid("literal-shape"));
            }
            Ok(ExprNode::Str(value.to_owned()))
        }
        "lit-datetime" => {
            keys(&["op", "value"])?;
            let value = object
                .get("value")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("literal-shape"))?;
            let seconds =
                datetime_parse(value).ok_or_else(|| diagnostic::input_invalid("literal-shape"))?;
            Ok(ExprNode::DateTime(seconds))
        }
        "lit-duration" => {
            keys(&["op", "value"])?;
            let value = object
                .get("value")
                .and_then(Json::as_i64)
                .ok_or_else(|| diagnostic::input_invalid("literal-shape"))?;
            if !duration_in_range(value) {
                return Err(diagnostic::input_invalid("literal-shape"));
            }
            Ok(ExprNode::Duration(value))
        }
        "lit-set" => {
            keys(&["op", "items"])?;
            let items = object
                .get("items")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("set-shape"))?;
            if items.is_empty() || items.len() > MAX_SET_ITEMS {
                return Err(diagnostic::input_invalid("set-shape"));
            }
            let mut scalars = Vec::with_capacity(items.len());
            for item in items {
                scalars.push(set_item(item)?);
            }
            let inner = scalars[0].ty();
            if scalars.iter().any(|scalar| scalar.ty() != inner) {
                return Err(diagnostic::input_invalid("set-shape"));
            }
            scalars.sort_by_key(|left| left.cmp_key());
            for pair in scalars.windows(2) {
                if pair[0] == pair[1] {
                    return Err(diagnostic::input_invalid("set-duplicate"));
                }
            }
            Ok(ExprNode::Set(scalars))
        }
        "ref" => {
            keys(&["op", "scope", "field"])?;
            let scope = RefScope::parse(
                object
                    .get("scope")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid("node-shape"))?,
            )
            .ok_or_else(|| diagnostic::input_invalid("node-shape"))?;
            let field = FieldName::parse(
                object
                    .get("field")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid("node-shape"))?,
            )
            .map_err(|_| diagnostic::input_invalid("node-shape"))?;
            Ok(ExprNode::Ref {
                scope,
                field: field.as_str().to_owned(),
            })
        }
        "now" => {
            keys(&["op"])?;
            Ok(ExprNode::Now)
        }
        "eq" | "ne" | "lt" | "le" | "gt" | "ge" | "add" | "sub" | "mul" | "div" | "mod" => {
            keys(&["op", "left", "right"])?;
            let (left, right) = binary("left", "right")?;
            Ok(match op {
                "eq" => ExprNode::Equal { left, right },
                "ne" => ExprNode::NotEqual { left, right },
                "lt" => ExprNode::Less { left, right },
                "le" => ExprNode::LessEqual { left, right },
                "gt" => ExprNode::Greater { left, right },
                "ge" => ExprNode::GreaterEqual { left, right },
                "add" => ExprNode::Add { left, right },
                "sub" => ExprNode::Subtract { left, right },
                "mul" => ExprNode::Multiply { left, right },
                "div" => ExprNode::Divide { left, right },
                _ => ExprNode::Modulo { left, right },
            })
        }
        "is-null" | "not-null" | "not" => {
            keys(&["op", "operand"])?;
            let operand = Box::new(node(member("operand")?, depth + 1, nodes)?);
            Ok(match op {
                "is-null" => ExprNode::IsNull { operand },
                "not-null" => ExprNode::NotNull { operand },
                _ => ExprNode::Not { operand },
            })
        }
        "in-set" | "not-in-set" => {
            keys(&["op", "operand", "set"])?;
            let operand = Box::new(node(member("operand")?, depth + 1, nodes)?);
            let set = Box::new(node(member("set")?, depth + 1, nodes)?);
            Ok(if op == "in-set" {
                ExprNode::InSet { operand, set }
            } else {
                ExprNode::NotInSet { operand, set }
            })
        }
        "and" | "or" => {
            keys(&["op", "operands"])?;
            let operands = object
                .get("operands")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("operands-shape"))?;
            if operands.len() < 2 || operands.len() > MAX_OPERANDS {
                return Err(diagnostic::input_invalid("operands-shape"));
            }
            let mut parsed = Vec::with_capacity(operands.len());
            for operand in operands {
                parsed.push(node(operand, depth + 1, nodes)?);
            }
            Ok(if op == "and" {
                ExprNode::And(parsed)
            } else {
                ExprNode::Or(parsed)
            })
        }
        "if" => {
            keys(&["op", "condition", "then", "else"])?;
            let condition = Box::new(node(member("condition")?, depth + 1, nodes)?);
            let then = Box::new(node(member("then")?, depth + 1, nodes)?);
            let otherwise = Box::new(node(member("else")?, depth + 1, nodes)?);
            Ok(ExprNode::If {
                condition,
                then,
                otherwise,
            })
        }
        "builtin" => {
            keys(&["op", "name", "args"])?;
            let name = object
                .get("name")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("builtin-shape"))?;
            if name.is_empty()
                || name.len() > 64
                || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
            {
                return Err(diagnostic::input_invalid("builtin-shape"));
            }
            let args_json = object
                .get("args")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("builtin-shape"))?;
            if args_json.len() > 8 {
                return Err(diagnostic::input_invalid("builtin-shape"));
            }
            let mut args = Vec::with_capacity(args_json.len());
            for arg in args_json {
                args.push(node(arg, depth + 1, nodes)?);
            }
            Ok(ExprNode::Builtin {
                name: name.to_owned(),
                args,
            })
        }
        _ => Err(diagnostic::input_invalid("node-shape")),
    }
}

/// Decode one set-literal member.
fn set_item(json: &Json) -> Result<Scalar, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("set-shape"))?;
    if object.len() != 2 || !object.contains_key("kind") || !object.contains_key("value") {
        return Err(diagnostic::input_invalid("set-shape"));
    }
    let scalar = match object.get("kind").and_then(Json::as_str) {
        Some("bool") => Scalar::Bool(
            object
                .get("value")
                .and_then(Json::as_bool)
                .ok_or_else(|| diagnostic::input_invalid("set-shape"))?,
        ),
        Some("int") => {
            let value = object
                .get("value")
                .and_then(Json::as_i64)
                .ok_or_else(|| diagnostic::input_invalid("set-shape"))?;
            if !int_in_range(value) {
                return Err(diagnostic::input_invalid("set-shape"));
            }
            Scalar::Int(value)
        }
        Some("string") => {
            let value = object
                .get("value")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("set-shape"))?;
            if value.len() > MAX_LITERAL_BYTES || !string_is_canonical(value) {
                return Err(diagnostic::input_invalid("set-shape"));
            }
            Scalar::Str(value.to_owned())
        }
        Some("datetime") => {
            let value = object
                .get("value")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("set-shape"))?;
            Scalar::DateTime(
                datetime_parse(value).ok_or_else(|| diagnostic::input_invalid("set-shape"))?,
            )
        }
        Some("duration") => {
            let value = object
                .get("value")
                .and_then(Json::as_i64)
                .ok_or_else(|| diagnostic::input_invalid("set-shape"))?;
            if !duration_in_range(value) {
                return Err(diagnostic::input_invalid("set-shape"));
            }
            Scalar::Duration(value)
        }
        _ => return Err(diagnostic::input_invalid("set-shape")),
    };
    Ok(scalar)
}

/// One validated built-in capability snapshot (the managed-mode
/// support declaration a target or adapter supplies).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltinSupport {
    /// The declared supported capability tokens, sorted and
    /// duplicate-free.
    pub supported: Vec<String>,
}

impl BuiltinSupport {
    /// Decode one support document
    /// (`lekalo/expressions/builtin-support/v1.0.0`).
    pub fn from_value(json: &Json) -> Result<Self, DiagnosticSet> {
        let object = json
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("support-shape"))?;
        if object.len() != 2 {
            return Err(diagnostic::input_invalid("support-shape"));
        }
        if object.get("schemaVersion").and_then(Json::as_str) != Some(SUPPORT_SCHEMA_VERSION) {
            return Err(diagnostic::input_invalid("support-shape"));
        }
        let tokens = object
            .get("supported")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("support-shape"))?;
        let mut supported = Vec::with_capacity(tokens.len());
        for token in tokens {
            let text = token
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("support-shape"))?;
            if text.is_empty()
                || text.len() > 128
                || !text.bytes().all(|b| {
                    b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b == b'.'
                        || b == b'/'
                        || b == b'-'
                })
            {
                return Err(diagnostic::input_invalid("support-shape"));
            }
            supported.push(text.to_owned());
        }
        supported.sort();
        supported.dedup();
        Ok(Self { supported })
    }

    /// Whether one capability token is declared.
    pub fn has(&self, token: &str) -> bool {
        self.supported.iter().any(|entry| entry == token)
    }
}

/// The managed-mode gate: every capability the attachment requires
/// must appear in the declared support snapshot. A missing built-in
/// blocks managed generation for that target; the escape hatch is
/// the foreign implementation family (#30).
pub fn check_builtin_support(
    attachment: &ExpressionsAttachment,
    support: &BuiltinSupport,
) -> Result<(), DiagnosticSet> {
    for token in attachment.required_capabilities() {
        if !support.has(&token) {
            let subject = attachment
                .expressions()
                .iter()
                .find(|record| {
                    record
                        .used_builtins()
                        .iter()
                        .any(|name| super::builtin::capability(name) == token)
                })
                .map(|record| record.id().to_owned())
                .unwrap_or_else(|| attachment.project_id().as_str().to_owned());
            return Err(diagnostic::rule_invalid(
                diagnostic::BUILTIN_UNSUPPORTED,
                "builtin-missing",
                Some(&subject),
                None,
            ));
        }
    }
    Ok(())
}

/// One shared evaluation-vector expectation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VectorExpect {
    /// The expected value.
    Value(super::Value),
    /// The expected deterministic evaluation error token.
    Error(EvalError),
}

/// One shared evaluation-vector case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorCase {
    /// The vector identity (bounded).
    pub id: String,
    /// The referenced expression identity.
    pub expression: String,
    /// The injected deterministic clock (required when the body uses
    /// `now`).
    pub clock: Option<i64>,
    /// The raw binding scopes, validated against the record's
    /// declared references at evaluation time.
    pub bindings: Json,
    /// The expectation.
    pub expect: VectorExpect,
}

/// One validated evaluation-vector document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorsDocument {
    /// The ordered vector cases.
    pub vectors: Vec<VectorCase>,
}

impl VectorsDocument {
    /// Decode one vectors document
    /// (`lekalo/expressions/vectors/v1.0.0`).
    pub fn from_value(json: &Json) -> Result<Self, DiagnosticSet> {
        let object = json
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("vectors-shape"))?;
        if object.len() != 2 {
            return Err(diagnostic::input_invalid("vectors-shape"));
        }
        if object.get("schemaVersion").and_then(Json::as_str) != Some(VECTORS_SCHEMA_VERSION) {
            return Err(diagnostic::input_invalid("vectors-shape"));
        }
        let cases = object
            .get("vectors")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("vectors-shape"))?;
        if cases.is_empty() || cases.len() > super::version::MAX_VECTORS {
            return Err(diagnostic::input_invalid("vectors-shape"));
        }
        let mut vectors = Vec::with_capacity(cases.len());
        for case in cases {
            let entry = case
                .as_object()
                .ok_or_else(|| diagnostic::input_invalid("vector-shape"))?;
            for key in entry.keys() {
                if !VECTOR_KEYS.contains(&key.as_str()) {
                    return Err(diagnostic::input_invalid("vector-shape"));
                }
            }
            for key in ["id", "expression", "bindings", "expect"] {
                if !entry.contains_key(key) {
                    return Err(diagnostic::input_invalid("vector-shape"));
                }
            }
            let id = entry
                .get("id")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("vector-shape"))?;
            if id.is_empty() || id.len() > 64 {
                return Err(diagnostic::input_invalid("vector-shape"));
            }
            let expression = entry
                .get("expression")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("vector-shape"))?;
            if crate::invariant_transition::id::ExpressionRef::parse(expression).is_err() {
                return Err(diagnostic::input_invalid("vector-shape"));
            }
            let clock = match entry.get("clock") {
                Some(value) => {
                    let text = value
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("vector-shape"))?;
                    Some(
                        datetime_parse(text)
                            .ok_or_else(|| diagnostic::input_invalid("vector-shape"))?,
                    )
                }
                None => None,
            };
            let bindings = entry
                .get("bindings")
                .cloned()
                .ok_or_else(|| diagnostic::input_invalid("vector-shape"))?;
            if !bindings.is_object() {
                return Err(diagnostic::input_invalid("vector-shape"));
            }
            let expect_json = entry
                .get("expect")
                .and_then(Json::as_object)
                .ok_or_else(|| diagnostic::input_invalid("vector-shape"))?;
            if expect_json.len() != 1 {
                return Err(diagnostic::input_invalid("vector-shape"));
            }
            let expect = if let Some(value) = expect_json.get("value") {
                VectorExpect::Value(
                    json_value(value).ok_or_else(|| diagnostic::input_invalid("vector-shape"))?,
                )
            } else if let Some(error) = expect_json.get("error").and_then(Json::as_str) {
                VectorExpect::Error(
                    EvalError::parse(error)
                        .ok_or_else(|| diagnostic::input_invalid("vector-shape"))?,
                )
            } else {
                return Err(diagnostic::input_invalid("vector-shape"));
            };
            vectors.push(VectorCase {
                id: id.to_owned(),
                expression: expression.to_owned(),
                clock,
                bindings,
                expect,
            });
        }
        Ok(Self { vectors })
    }
}

/// Decode one typed vector expectation value.
fn json_value(json: &Json) -> Option<super::Value> {
    match json {
        Json::Bool(value) => Some(super::Value::Scalar(Scalar::Bool(*value))),
        Json::Number(number) => {
            let value = number.as_i64()?;
            if !int_in_range(value) {
                return None;
            }
            if json_is_duration(json) {
                return Some(super::Value::Scalar(Scalar::Duration(value)));
            }
            Some(super::Value::Scalar(Scalar::Int(value)))
        }
        Json::String(text) => {
            if let Some(seconds) = datetime_parse(text) {
                return Some(super::Value::Scalar(Scalar::DateTime(seconds)));
            }
            if !string_is_canonical(text) || text.len() > MAX_LITERAL_BYTES {
                return None;
            }
            Some(super::Value::Scalar(Scalar::Str(text.clone())))
        }
        Json::Array(items) => {
            let mut scalars = Vec::with_capacity(items.len());
            for item in items {
                let Some(super::Value::Scalar(scalar)) = json_value(item) else {
                    return None;
                };
                scalars.push(scalar);
            }
            let inner = scalars.first()?.ty();
            if scalars.iter().any(|scalar| scalar.ty() != inner) {
                return None;
            }
            Some(super::Value::Set(scalars))
        }
        _ => None,
    }
}

/// Vector expectation numbers are integers or durations; the
/// distinction is contextual (the declared result type decides), and
/// both share the integer encoding, so this helper always reports
/// `false` and exists only to keep the decoder total.
fn json_is_duration(_json: &Json) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_document() -> Json {
        serde_json::json!({
            "schemaVersion": SCHEMA_VERSION,
            "identity": IDENTITY,
            "attachmentRevision": "1.0.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "builtinSemantics": BUILTIN_SEMANTICS_VERSION,
            "expressions": [
                {
                    "id": "expr.planner/overdue-check",
                    "kind": "condition",
                    "params": [
                        {"scope": "input", "field": "due", "type": "datetime"},
                        {"scope": "input", "field": "state", "type": "string"}
                    ],
                    "result": "bool",
                    "body": {
                        "op": "and",
                        "operands": [

                            {"op": "ne", "left": {"op": "ref", "scope": "input", "field": "state"}, "right": {"op": "lit-string", "value": "done"}},
                            {"op": "lt", "left": {"op": "ref", "scope": "input", "field": "due"}, "right": {"op": "now"}}
                        ]
                    }
                }
            ]
        })
    }

    #[test]
    fn a_valid_document_parses_with_normalized_order() {
        let attachment = ExpressionsAttachment::from_value(&base_document()).expect("valid");
        assert_eq!(attachment.expressions().len(), 1);
        assert_eq!(
            attachment.expressions()[0].id(),
            "expr.planner/overdue-check"
        );
        assert_eq!(attachment.required_capabilities().len(), 1);
        assert_eq!(attachment.required_capabilities()[0], "expression.core");
    }

    #[test]
    fn unknown_fields_and_wrong_identities_reject() {
        let mut document = base_document();
        document["extra"] = serde_json::json!(1);
        let rejection = ExpressionsAttachment::from_value(&document).expect_err("unknown field");
        assert!(rejection
            .as_slice()
            .iter()
            .any(|d| d.id() == "expression.input-invalid"));
        let mut document = base_document();
        document["identity"] = serde_json::json!("dev.lekalo.expressions@9.9.9");
        assert!(ExpressionsAttachment::from_value(&document).is_err());
    }

    #[test]
    fn set_literals_normalize_to_sorted_duplicate_free_form() {
        let mut document = base_document();
        document["expressions"][0]["body"] = serde_json::json!({
            "op": "in-set",
            "operand": {"op": "ref", "scope": "input", "field": "state"},
            "set": {"op": "lit-set", "items": [
                {"kind": "string", "value": "todo"},
                {"kind": "string", "value": "backlog"}
            ]}
        });
        let attachment = ExpressionsAttachment::from_value(&document).expect("valid");
        let ExprNode::InSet { set, .. } = attachment.expressions()[0].body() else {
            panic!("membership");
        };
        let ExprNode::Set(items) = set.as_ref() else {
            panic!("set literal");
        };
        assert_eq!(items[0], Scalar::Str("backlog".to_owned()));
        // Duplicates reject.
        let mut document = base_document();
        document["expressions"][0]["body"] = serde_json::json!({
            "op": "in-set",
            "operand": {"op": "ref", "scope": "input", "field": "state"},
            "set": {"op": "lit-set", "items": [
                {"kind": "string", "value": "todo"},
                {"kind": "string", "value": "todo"}
            ]}
        });
        assert!(ExpressionsAttachment::from_value(&document).is_err());
    }

    #[test]
    fn builtin_support_blocks_missing_capabilities() {
        let attachment = ExpressionsAttachment::from_value(&base_document()).expect("valid");
        let core_only = BuiltinSupport {
            supported: vec!["expression.core".to_owned()],
        };
        assert!(check_builtin_support(&attachment, &core_only).is_ok());
        let empty = BuiltinSupport { supported: vec![] };
        let rejection = check_builtin_support(&attachment, &empty).expect_err("missing core");
        assert!(rejection
            .as_slice()
            .iter()
            .any(|d| d.id() == "expression.builtin-unsupported"));
    }
}
