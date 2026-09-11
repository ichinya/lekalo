//! The deterministic, target-neutral read plan of one query
//! declaration (issue #64).
//!
//! The plan is the single closed operator surface every target
//! projection consumes: the Node, PHP, and any other adapter map the
//! same ordered steps, so the pagination contract is identical by
//! construction. The plan is pure declaration — it never executes and
//! never renders SQL, ORM calls, or target-specific text. A foreign
//! query produces no managed steps: managed generation is blocked and
//! the mapping belongs to the foreign implementation (ADR-0033).

use serde_json::{Map, Value as Json};

use super::filter::{FilterExpr, FilterValue, Literal};
use super::id::ParameterName;
use super::query::{PaginationStrategy, QueryDecl, SelectionField};
use crate::scenario::id::{FieldName, SemanticId};

/// One closed plan step in canonical order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanStep {
    /// Apply the closed filter tree.
    Filter(FilterExpr),
    /// Sort by the declared keys, in declared order.
    Sort(Vec<(FieldName, super::query::Direction)>),
    /// Skip the declared offset rows (offset strategy).
    Offset(i64),
    /// Continue after the declared key parameter (cursor strategy).
    Cursor(ParameterName),
    /// Take at most the declared rows.
    Limit(i64),
    /// Project the declared output fields, in declared order.
    Project(Vec<(FieldName, FieldName)>),
    /// Fetch one joined relation path.
    Include(Vec<FieldName>),
}

impl PlanStep {
    /// The closed step token for diagnostics and scenarios.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Filter(_) => "filter",
            Self::Sort(_) => "sort",
            Self::Offset(_) => "offset",
            Self::Cursor(_) => "cursor",
            Self::Limit(_) => "limit",
            Self::Project(_) => "project",
            Self::Include(_) => "include",
        }
    }
}

/// One finished plan: the query, whether the mapping is foreign, and
/// the ordered closed steps.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryPlan {
    query: SemanticId,
    source: SemanticId,
    foreign: bool,
    steps: Vec<PlanStep>,
}

impl QueryPlan {
    /// Build the canonical plan of one validated query declaration.
    /// Pure; independent of any target.
    pub fn build(decl: &QueryDecl) -> Self {
        if decl.foreign.is_some() {
            return Self {
                query: decl.query.clone(),
                source: decl.source.clone(),
                foreign: true,
                steps: Vec::new(),
            };
        }
        let mut steps = Vec::new();
        if let Some(filter) = &decl.filter {
            steps.push(PlanStep::Filter(filter.clone()));
        }
        if let Some(sort) = &decl.sort {
            steps.push(PlanStep::Sort(
                sort.iter()
                    .map(|key| (key.field.clone(), key.direction))
                    .collect::<Vec<_>>(),
            ));
        }
        if let Some(pagination) = &decl.pagination {
            match pagination.strategy {
                PaginationStrategy::Offset => {
                    steps.push(PlanStep::Offset(pagination.offset.unwrap_or(0)));
                }
                PaginationStrategy::Cursor => {
                    if let Some(key) = &pagination.key_parameter {
                        steps.push(PlanStep::Cursor(key.clone()));
                    }
                }
            }
            steps.push(PlanStep::Limit(pagination.limit));
        }
        // Every managed read projects: an absent selection is a
        // full-row projection (empty field list), never a truncated
        // one.
        steps.push(PlanStep::Project(match &decl.selection {
            Some(selection) => selection
                .iter()
                .map(|field: &SelectionField| {
                    let output = field.alias.clone().unwrap_or_else(|| field.field.clone());
                    (field.field.clone(), output)
                })
                .collect::<Vec<_>>(),
            None => Vec::new(),
        }));
        for include in &decl.includes {
            steps.push(PlanStep::Include(include.path.clone()));
        }
        Self {
            query: decl.query.clone(),
            source: decl.source.clone(),
            foreign: false,
            steps,
        }
    }

    /// The planned query id.
    pub fn query(&self) -> &SemanticId {
        &self.query
    }

    /// The source entity of the read.
    pub fn source(&self) -> &SemanticId {
        &self.source
    }

    /// Whether the mapping is foreign (managed generation blocked).
    pub const fn is_foreign(&self) -> bool {
        self.foreign
    }

    /// The ordered closed steps (empty exactly when foreign).
    pub fn steps(&self) -> &[PlanStep] {
        &self.steps
    }

    /// The canonical plan bytes (compact JSON, byte-sorted keys, no
    /// trailing LF). Deterministic across runs and targets.
    pub fn canonical_bytes(&self) -> String {
        let mut object = Map::new();
        object.insert(
            "query".to_owned(),
            Json::String(self.query.as_str().to_owned()),
        );
        object.insert(
            "source".to_owned(),
            Json::String(self.source.as_str().to_owned()),
        );
        object.insert("foreign".to_owned(), Json::Bool(self.foreign));
        object.insert(
            "steps".to_owned(),
            Json::Array(self.steps.iter().map(step_json).collect::<Vec<_>>()),
        );
        Json::Object(object).to_string()
    }
}

/// One canonical step.
fn step_json(step: &PlanStep) -> Json {
    let mut object = Map::new();
    object.insert("kind".to_owned(), Json::String(step.kind().to_owned()));
    match step {
        PlanStep::Filter(filter) => {
            object.insert("filter".to_owned(), filter_json(filter));
        }
        PlanStep::Sort(keys) => {
            object.insert(
                "keys".to_owned(),
                Json::Array(
                    keys.iter()
                        .map(|(field, direction)| {
                            Json::String(format!("{}:{}", field.as_str(), direction.as_str()))
                        })
                        .collect::<Vec<_>>(),
                ),
            );
        }
        PlanStep::Offset(offset) => {
            object.insert("offset".to_owned(), Json::from(*offset));
        }
        PlanStep::Cursor(parameter) => {
            object.insert(
                "keyParameter".to_owned(),
                Json::String(parameter.as_str().to_owned()),
            );
        }
        PlanStep::Limit(limit) => {
            object.insert("limit".to_owned(), Json::from(*limit));
        }
        PlanStep::Project(fields) => {
            object.insert(
                "fields".to_owned(),
                Json::Array(
                    fields
                        .iter()
                        .map(|(field, output)| {
                            if field == output {
                                Json::String(field.as_str().to_owned())
                            } else {
                                Json::String(format!("{}:{}", field.as_str(), output.as_str()))
                            }
                        })
                        .collect::<Vec<_>>(),
                ),
            );
        }
        PlanStep::Include(path) => {
            object.insert(
                "path".to_owned(),
                Json::Array(
                    path.iter()
                        .map(|segment| Json::String(segment.as_str().to_owned()))
                        .collect::<Vec<_>>(),
                ),
            );
        }
    }
    Json::Object(object)
}

/// One canonical filter subtree (mirrors the attachment encoding).
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
                let mut value_object = Map::new();
                match value {
                    FilterValue::Param(name) => {
                        value_object
                            .insert("param".to_owned(), Json::String(name.as_str().to_owned()));
                    }
                    FilterValue::Literal(literal) => {
                        value_object.insert("literal".to_owned(), literal_json(literal));
                    }
                }
                object.insert("value".to_owned(), Json::Object(value_object));
            }
            Json::Object(object)
        }
        FilterExpr::And(operands) => {
            let mut object = Map::new();
            object.insert(
                "and".to_owned(),
                Json::Array(operands.iter().map(filter_json).collect::<Vec<_>>()),
            );
            Json::Object(object)
        }
        FilterExpr::Or(operands) => {
            let mut object = Map::new();
            object.insert(
                "or".to_owned(),
                Json::Array(operands.iter().map(filter_json).collect::<Vec<_>>()),
            );
            Json::Object(object)
        }
        FilterExpr::Not(operand) => {
            let mut object = Map::new();
            object.insert("not".to_owned(), filter_json(operand));
            Json::Object(object)
        }
    }
}

/// One canonical literal node (mirrors the attachment encoding).
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

#[cfg(test)]
mod tests {
    use super::super::version;
    use super::*;

    fn attachment(queries: serde_json::Value) -> super::super::QueryModelAttachment {
        super::super::QueryModelAttachment::from_value(&serde_json::json!({
            "schemaVersion": version::SCHEMA_VERSION,
            "identity": version::IDENTITY,
            "attachmentRevision": "1.0.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "tenancy": [],
            "queries": queries
        }))
        .expect("attachment")
    }

    #[test]
    fn offset_page_plan_is_canonical_and_ordered() {
        let attachment = attachment(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "page",
            "consistency": "strong",
            "sort": [{"field": "due", "direction": "asc"}, {"field": "task_id", "direction": "asc"}],
            "pagination": {"strategy": "offset", "limit": 20, "offset": 40}
        }]));
        let decl = &attachment.queries()[0];
        let plan = QueryPlan::build(decl);
        assert!(!plan.is_foreign());
        let kinds: Vec<&str> = plan.steps().iter().map(|step| step.kind()).collect();
        assert_eq!(kinds, vec!["sort", "offset", "limit", "project"]);
        let bytes = plan.canonical_bytes();
        assert_eq!(bytes, plan.canonical_bytes());
        assert!(bytes.contains("\"offset\":40"));
        assert!(bytes.contains("\"limit\":20"));
    }

    #[test]
    fn foreign_queries_produce_no_managed_steps() {
        let attachment = attachment(serde_json::json!([{
            "query": "planner.search",
            "source": "planner.task",
            "cardinality": "list",
            "consistency": "stale-ok",
            "foreign": {"capability": "full-text-search", "reason": "Managed grammar cannot express ranked search."}
        }]));
        let plan = QueryPlan::build(&attachment.queries()[0]);
        assert!(plan.is_foreign());
        assert!(plan.steps().is_empty());
    }

    #[test]
    fn projection_order_is_behavioral() {
        let attachment = attachment(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "list",
            "consistency": "strong",
            "selection": [{"field": "title", "as": "name"}, {"field": "due"}]
        }]));
        let plan = QueryPlan::build(&attachment.queries()[0]);
        let bytes = plan.canonical_bytes();
        let title_pos = bytes.find("\"fields\":[\"title:name\",\"due\"]");
        assert!(title_pos.is_some());
    }
}
