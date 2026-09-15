//! Canonical serialization of the query-model attachment (issue #64).
//!
//! Compact UTF-8 JSON with byte-sorted object keys: tenancy
//! declarations are sorted by entity, queries by id, parameters by
//! name, includes by path, and policy/scenario references
//! lexicographically; sort keys and selection fields keep their
//! declared behavioral order, as does the filter tree. Repeated
//! runs are byte-identical.

use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::filter::{FilterExpr, FilterValue, Literal};

use super::query::QueryDecl;
use super::{QueryModelAttachment, TenancyDecl};
use crate::diagnostics::DiagnosticSet;

/// The canonical payload bytes, or the typed export-limit refusal.
pub(crate) fn attachment_bytes(attachment: &QueryModelAttachment) -> Result<String, DiagnosticSet> {
    let mut root = Map::new();
    root.insert(
        "schemaVersion".to_owned(),
        Json::String(super::version::SCHEMA_VERSION.to_owned()),
    );
    root.insert(
        "identity".to_owned(),
        Json::String(super::version::IDENTITY.to_owned()),
    );
    root.insert(
        "attachmentRevision".to_owned(),
        Json::String(attachment.attachment_revision.as_str().to_owned()),
    );
    root.insert(
        "projectId".to_owned(),
        Json::String(attachment.project_id.as_str().to_owned()),
    );
    let mut model_ref = Map::new();
    model_ref.insert(
        "modelVersion".to_owned(),
        Json::String(attachment.model_ref.version().as_str().to_owned()),
    );
    model_ref.insert(
        "digest".to_owned(),
        Json::String(attachment.model_ref.digest().as_str().to_owned()),
    );
    root.insert("modelRef".to_owned(), Json::Object(model_ref));
    let mut ir_ref = Map::new();
    ir_ref.insert(
        "identity".to_owned(),
        Json::String(super::version::IR_IDENTITY.to_owned()),
    );
    ir_ref.insert(
        "digest".to_owned(),
        Json::String(attachment.ir_digest.as_str().to_owned()),
    );
    root.insert("irRef".to_owned(), Json::Object(ir_ref));
    if let Some(source_map) = &attachment.source_map_ref {
        root.insert(
            "sourceMapRef".to_owned(),
            Json::String(source_map.as_str().to_owned()),
        );
    }
    root.insert(
        "tenancy".to_owned(),
        Json::Array(
            attachment
                .tenancy()
                .iter()
                .map(tenancy_json)
                .collect::<Vec<_>>(),
        ),
    );
    root.insert(
        "queries".to_owned(),
        Json::Array(
            attachment
                .queries()
                .iter()
                .map(query_json)
                .collect::<Vec<_>>(),
        ),
    );
    let bytes = Json::Object(root).to_string();
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// One canonical tenancy declaration.
fn tenancy_json(declaration: &TenancyDecl) -> Json {
    let mut object = Map::new();
    object.insert(
        "entity".to_owned(),
        Json::String(declaration.entity.as_str().to_owned()),
    );
    object.insert(
        "field".to_owned(),
        Json::String(declaration.field.as_str().to_owned()),
    );
    Json::Object(object)
}

/// One canonical query declaration.
fn query_json(decl: &QueryDecl) -> Json {
    let mut object = Map::new();
    object.insert(
        "query".to_owned(),
        Json::String(decl.query.as_str().to_owned()),
    );
    object.insert(
        "source".to_owned(),
        Json::String(decl.source.as_str().to_owned()),
    );
    object.insert(
        "cardinality".to_owned(),
        Json::String(decl.cardinality.as_str().to_owned()),
    );
    object.insert(
        "consistency".to_owned(),
        Json::String(decl.consistency.as_str().to_owned()),
    );
    if let Some(seconds) = decl.max_staleness {
        object.insert("maxStaleness".to_owned(), Json::from(seconds));
    }
    if !decl.parameters.is_empty() {
        object.insert(
            "parameters".to_owned(),
            Json::Array(
                decl.parameters
                    .iter()
                    .map(|parameter| {
                        let mut parameter_object = Map::new();
                        parameter_object.insert(
                            "name".to_owned(),
                            Json::String(parameter.name.as_str().to_owned()),
                        );
                        parameter_object.insert(
                            "type".to_owned(),
                            Json::String(parameter.parameter_type.clone()),
                        );
                        Json::Object(parameter_object)
                    })
                    .collect::<Vec<_>>(),
            ),
        );
    }
    if let Some(filter) = &decl.filter {
        object.insert("filter".to_owned(), filter_json(filter));
    }
    if let Some(sort) = &decl.sort {
        object.insert(
            "sort".to_owned(),
            Json::Array(
                sort.iter()
                    .map(|key| {
                        let mut key_object = Map::new();
                        key_object.insert(
                            "field".to_owned(),
                            Json::String(key.field.as_str().to_owned()),
                        );
                        key_object.insert(
                            "direction".to_owned(),
                            Json::String(key.direction.as_str().to_owned()),
                        );
                        Json::Object(key_object)
                    })
                    .collect::<Vec<_>>(),
            ),
        );
    }
    if let Some(pagination) = &decl.pagination {
        let mut pagination_object = Map::new();
        pagination_object.insert(
            "strategy".to_owned(),
            Json::String(pagination.strategy.as_str().to_owned()),
        );
        pagination_object.insert("limit".to_owned(), Json::from(pagination.limit));
        if let Some(offset) = pagination.offset {
            pagination_object.insert("offset".to_owned(), Json::from(offset));
        }
        if let Some(key) = &pagination.key_parameter {
            pagination_object.insert(
                "keyParameter".to_owned(),
                Json::String(key.as_str().to_owned()),
            );
        }
        object.insert("pagination".to_owned(), Json::Object(pagination_object));
    }
    if let Some(selection) = &decl.selection {
        object.insert(
            "selection".to_owned(),
            Json::Array(
                selection
                    .iter()
                    .map(|field| {
                        let mut field_object = Map::new();
                        field_object.insert(
                            "field".to_owned(),
                            Json::String(field.field.as_str().to_owned()),
                        );
                        if let Some(alias) = &field.alias {
                            field_object
                                .insert("as".to_owned(), Json::String(alias.as_str().to_owned()));
                        }
                        Json::Object(field_object)
                    })
                    .collect::<Vec<_>>(),
            ),
        );
    }
    if !decl.includes.is_empty() {
        object.insert(
            "includes".to_owned(),
            Json::Array(
                decl.includes
                    .iter()
                    .map(|include| {
                        let mut include_object = Map::new();
                        include_object.insert(
                            "path".to_owned(),
                            Json::Array(
                                include
                                    .path
                                    .iter()
                                    .map(|segment| Json::String(segment.as_str().to_owned()))
                                    .collect::<Vec<_>>(),
                            ),
                        );
                        if let Some(alias) = &include.alias {
                            include_object
                                .insert("as".to_owned(), Json::String(alias.as_str().to_owned()));
                        }
                        Json::Object(include_object)
                    })
                    .collect::<Vec<_>>(),
            ),
        );
    }
    if !decl.policies.is_empty() {
        object.insert(
            "policies".to_owned(),
            Json::Array(
                decl.policies
                    .iter()
                    .map(|policy| Json::String(policy.as_str().to_owned()))
                    .collect::<Vec<_>>(),
            ),
        );
    }
    if !decl.scenarios.is_empty() {
        object.insert(
            "scenarios".to_owned(),
            Json::Array(
                decl.scenarios
                    .iter()
                    .map(|scenario| Json::String(scenario.as_str().to_owned()))
                    .collect::<Vec<_>>(),
            ),
        );
    }
    if let Some(cost) = &decl.cost {
        let mut cost_object = Map::new();
        if let Some(max_rows) = cost.max_rows {
            cost_object.insert("maxRows".to_owned(), Json::from(max_rows));
        }
        if let Some(expensive) = cost.expensive {
            cost_object.insert("expensive".to_owned(), Json::Bool(expensive));
        }
        object.insert("cost".to_owned(), Json::Object(cost_object));
    }
    if let Some(foreign) = &decl.foreign {
        let mut foreign_object = Map::new();
        foreign_object.insert(
            "capability".to_owned(),
            Json::String(foreign.capability.clone()),
        );
        foreign_object.insert("reason".to_owned(), Json::String(foreign.reason.clone()));
        object.insert("foreign".to_owned(), Json::Object(foreign_object));
    }
    Json::Object(object)
}

/// One canonical filter subtree.
fn filter_json(filter: &FilterExpr) -> Json {
    match filter {
        FilterExpr::Leaf(leaf) => {
            let mut object = Map::new();
            object.insert(
                "field".to_owned(),
                Json::String(leaf.field.as_str().to_owned()),
            );
            object.insert("op".to_owned(), Json::String(leaf.op.as_str().to_owned()));
            if let Some(value) = &leaf.value {
                object.insert("value".to_owned(), filter_value_json(value));
            }
            Json::Object(object)
        }
        FilterExpr::And(operands) => operand_list_json("and", operands),
        FilterExpr::Or(operands) => operand_list_json("or", operands),
        FilterExpr::Not(operand) => {
            let mut object = Map::new();
            object.insert("not".to_owned(), filter_json(operand));
            Json::Object(object)
        }
    }
}

/// One canonical logical operand list.
fn operand_list_json(key: &str, operands: &[FilterExpr]) -> Json {
    let mut object = Map::new();
    object.insert(
        key.to_owned(),
        Json::Array(operands.iter().map(filter_json).collect::<Vec<_>>()),
    );
    Json::Object(object)
}

/// One canonical filter value.
fn filter_value_json(value: &FilterValue) -> Json {
    let mut object = Map::new();
    match value {
        FilterValue::Param(name) => {
            object.insert("param".to_owned(), Json::String(name.as_str().to_owned()));
        }
        FilterValue::Literal(literal) => {
            object.insert("literal".to_owned(), literal_json(literal));
        }
    }
    Json::Object(object)
}

/// One canonical literal node.
fn literal_json(literal: &Literal) -> Json {
    let mut object = Map::new();
    let (kind, value) = match literal {
        Literal::Bool(value) => ("boolean", Json::Bool(*value)),
        Literal::Integer(value) => ("integer", Json::from(*value)),
        Literal::Decimal(text) => ("decimal", Json::String(text.clone())),
        Literal::Str(text) => ("string", Json::String(text.clone())),
        Literal::Date(text) => ("date", Json::String(text.clone())),
        Literal::DateTime(text) => ("datetime", Json::String(text.clone())),
        Literal::Uuid(text) => ("uuid", Json::String(text.clone())),
        Literal::Uri(text) => ("uri", Json::String(text.clone())),
        Literal::Enum(text) => ("enum-member", Json::String(text.clone())),
        Literal::Set(items) => (
            "set",
            Json::Array(items.iter().map(literal_json).collect::<Vec<_>>()),
        ),
    };
    object.insert("kind".to_owned(), Json::String(kind.to_owned()));
    object.insert("value".to_owned(), value);
    Json::Object(object)
}

/// The canonical JSON of one filter subtree, for pure structural
/// comparisons in the diff.
pub(crate) fn filter_json_for_diff(filter: &FilterExpr) -> Json {
    filter_json(filter)
}

#[cfg(test)]
mod tests {
    use super::super::filter::FilterOp;
    use crate::query_model::PaginationStrategy;

    /// The closed operator spellings, pinned for the round-trip test.
    const OPERATOR_SPELLINGS: &[&str] = &[
        "eq",
        "ne",
        "lt",
        "le",
        "gt",
        "ge",
        "in",
        "not-in",
        "is-null",
        "is-not-null",
    ];

    #[test]
    fn operator_spellings_match_the_closed_vocabulary() {
        let spellings: Vec<&str> = [
            FilterOp::Eq,
            FilterOp::Ne,
            FilterOp::Lt,
            FilterOp::Le,
            FilterOp::Gt,
            FilterOp::Ge,
            FilterOp::In,
            FilterOp::NotIn,
            FilterOp::IsNull,
            FilterOp::IsNotNull,
        ]
        .iter()
        .map(|op| op.as_str())
        .collect();
        assert_eq!(spellings, OPERATOR_SPELLINGS.to_vec());
        assert_eq!(PaginationStrategy::Offset.as_str(), "offset");
    }
}
