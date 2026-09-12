//! Wire normalization of the query-model attachment (issue #64).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`QueryModelAttachment`](super::QueryModelAttachment). It fails
//! closed before semantic processing: unknown or missing fields, wrong
//! identities, malformed identifiers, digests, bounds, and out-of-bound
//! filter trees each return one typed registered diagnostic and no
//! partial attachment. Semantic rules (cardinality coherence, sort
//! determinism, foreign escapes) live in the attachment's semantic
//! self-check and the model resolution.

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{FieldName, NamespacedId, SemanticId};
use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::filter::{
    literal_within_bounds, FilterExpr, FilterLeaf, FilterOp, FilterValue, Literal,
};
use super::id::ParameterName;
use super::query::{
    Cardinality, Consistency, CostHint, Direction, ForeignRef, Include, Pagination,
    PaginationStrategy, QueryDecl, SelectionField, SortKey,
};
use super::version::{
    self, IR_IDENTITY, MAX_COST_ROWS, MAX_INCLUDES, MAX_INCLUDE_PATH, MAX_LIMIT, MAX_OFFSET,
    MAX_PARAMETERS, MAX_POLICY_REFS, MAX_QUERIES, MAX_REASON_BYTES, MAX_SELECTION, MAX_SORT_KEYS,
    MAX_STALENESS_SECONDS, MAX_TENANCY,
};
use super::{ModelPin, QueryModelAttachment, TenancyDecl};

/// The parsed wire object type.
type WireMap = Map<String, Json>;

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "sourceMapRef",
    "tenancy",
    "queries",
];

/// The required top-level members (`sourceMapRef` is optional).
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "tenancy",
    "queries",
];

/// The closed query-declaration member set.
const QUERY_KEYS: &[&str] = &[
    "query",
    "source",
    "cardinality",
    "consistency",
    "maxStaleness",
    "parameters",
    "filter",
    "sort",
    "pagination",
    "selection",
    "includes",
    "policies",
    "scenarios",
    "cost",
    "foreign",
];

/// The required query-declaration members.
const QUERY_REQUIRED_KEYS: &[&str] = &["query", "source", "cardinality", "consistency"];

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial attachment. Pure: no source,
/// model, cache, report, network, process, or target access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<QueryModelAttachment, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("top-level-shape"))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    for required in REQUIRED_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::input_invalid("missing-field"));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::input_invalid("schema-version"));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
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
    let ir_digest = digest_member(
        object
            .get("irRef")
            .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?,
    )?;
    let source_map_ref = match object.get("sourceMapRef") {
        Some(value) => Some(
            NamespacedId::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("source-map-ref"))?,
            )
            .map_err(|_| diagnostic::input_invalid("source-map-ref"))?,
        ),
        None => None,
    };
    let tenancy = tenancy(
        object
            .get("tenancy")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("tenancy-list"))?,
    )?;
    let queries = queries(
        object
            .get("queries")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("query-list"))?,
    )?;
    let attachment = QueryModelAttachment::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_digest,
        source_map_ref,
        tenancy,
        queries,
    );
    attachment.semantic_self_check()?;
    Ok(attachment)
}

/// Decode the bound source Model contract.
fn model_pin(value: &Json) -> Result<ModelPin, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("model-ref-shape"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "modelVersion" | "digest") {
            return Err(diagnostic::input_invalid("model-ref-field"));
        }
    }
    let version = object
        .get("modelVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("model-version"))?;
    let pin = match version {
        "0.1.0" => crate::scenario::ModelPin::V0_1_0,
        "1.0.0" => crate::scenario::ModelPin::V1_0_0,
        _ => return Err(diagnostic::input_invalid("model-version")),
    };
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("model-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("model-digest"))?;
    Ok(ModelPin::new(pin, digest))
}

/// Decode the bound IR digest.
fn digest_member(value: &Json) -> Result<Sha256Digest, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("ir-ref-shape"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "identity" | "digest") {
            return Err(diagnostic::input_invalid("ir-ref-field"));
        }
    }
    if object.get("identity").and_then(Json::as_str) != Some(IR_IDENTITY) {
        return Err(diagnostic::input_invalid("ir-identity"));
    }
    Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("ir-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("ir-digest"))
}

/// Decode the tenant-scoped entity declarations.
fn tenancy(items: &[Json]) -> Result<Vec<TenancyDecl>, DiagnosticSet> {
    if items.len() > MAX_TENANCY {
        return Err(diagnostic::input_invalid("tenancy-bound"));
    }
    let mut declarations = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("tenancy-shape"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "entity" | "field") {
                return Err(diagnostic::input_invalid("tenancy-field"));
            }
        }
        if object.len() != 2 {
            return Err(diagnostic::input_invalid("tenancy-missing"));
        }
        let entity = SemanticId::parse(
            object
                .get("entity")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("tenancy-entity"))?,
        )
        .map_err(|_| diagnostic::input_invalid("tenancy-entity"))?;
        let field = FieldName::parse(
            object
                .get("field")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("tenancy-field-name"))?,
        )
        .map_err(|_| diagnostic::input_invalid("tenancy-field-name"))?;
        declarations.push(TenancyDecl { entity, field });
    }
    declarations.sort_by(|left, right| left.entity.as_str().cmp(right.entity.as_str()));
    let mut sorted = declarations.clone();
    sorted.dedup_by(|left, right| left.entity == right.entity);
    if sorted.len() != declarations.len() {
        return Err(diagnostic::input_invalid("tenancy-duplicate"));
    }
    Ok(declarations)
}

/// Decode every query declaration, canonically ordered by query id.
fn queries(items: &[Json]) -> Result<Vec<QueryDecl>, DiagnosticSet> {
    if items.len() > MAX_QUERIES {
        return Err(diagnostic::input_invalid("query-bound"));
    }
    let mut declarations = Vec::with_capacity(items.len());
    for item in items {
        declarations.push(query(
            item.as_object()
                .ok_or_else(|| diagnostic::input_invalid("query-shape"))?,
        )?);
    }
    declarations.sort_by(|left, right| left.sort_key().cmp(right.sort_key()));
    let mut deduped = declarations.clone();
    deduped.dedup_by(|left, right| left.query == right.query);
    if deduped.len() != declarations.len() {
        return Err(diagnostic::input_invalid("query-duplicate"));
    }
    Ok(declarations)
}

/// Decode one query declaration object.
fn query(object: &WireMap) -> Result<QueryDecl, DiagnosticSet> {
    for key in object.keys() {
        if !QUERY_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    for required in QUERY_REQUIRED_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::input_invalid("missing-field"));
        }
    }
    let query_id = SemanticId::parse(
        object
            .get("query")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("query-id"))?,
    )
    .map_err(|_| diagnostic::input_invalid("query-id"))?;
    let source = SemanticId::parse(
        object
            .get("source")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("query-source"))?,
    )
    .map_err(|_| diagnostic::input_invalid("query-source"))?;
    let cardinality = Cardinality::parse(
        object
            .get("cardinality")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("query-cardinality"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("query-cardinality"))?;
    let consistency = Consistency::parse(
        object
            .get("consistency")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("query-consistency"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("query-consistency"))?;
    let max_staleness = match object.get("maxStaleness") {
        Some(value) => {
            let seconds = value
                .as_i64()
                .filter(|seconds| (1..=MAX_STALENESS_SECONDS).contains(seconds))
                .ok_or_else(|| diagnostic::input_invalid("staleness-bound"))?;
            Some(seconds)
        }
        None => None,
    };
    let parameters = parameters(object.get("parameters").and_then(Json::as_array))?;
    let filter = match object.get("filter") {
        Some(value) => Some(filter_expr(value, 0)?),
        None => None,
    };
    let sort = match object.get("sort") {
        Some(value) => Some(sort_keys(
            value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("sort-list"))?,
        )?),
        None => None,
    };
    let pagination = match object.get("pagination") {
        Some(value) => {
            Some(pagination(value.as_object().ok_or_else(|| {
                diagnostic::input_invalid("pagination-shape")
            })?)?)
        }
        None => None,
    };
    let selection = match object.get("selection") {
        Some(value) => {
            Some(selection_fields(value.as_array().ok_or_else(|| {
                diagnostic::input_invalid("selection-list")
            })?)?)
        }
        None => None,
    };
    let includes = includes(object.get("includes").and_then(Json::as_array))?;
    let policies = semantic_refs(
        object.get("policies").and_then(Json::as_array),
        MAX_POLICY_REFS,
        "policy-ref",
    )?;
    let scenarios = semantic_refs(
        object.get("scenarios").and_then(Json::as_array),
        super::version::MAX_SCENARIO_REFS,
        "scenario-ref",
    )?;
    let cost = match object.get("cost") {
        Some(value) => Some(cost_hint(
            value
                .as_object()
                .ok_or_else(|| diagnostic::input_invalid("cost-shape"))?,
        )?),
        None => None,
    };
    let foreign = match object.get("foreign") {
        Some(value) => {
            Some(foreign_ref(value.as_object().ok_or_else(|| {
                diagnostic::input_invalid("foreign-shape")
            })?)?)
        }
        None => None,
    };
    Ok(QueryDecl {
        query: query_id,
        source,
        cardinality,
        consistency,
        max_staleness,
        parameters,
        filter,
        sort,
        pagination,
        selection,
        includes,
        policies,
        scenarios,
        cost,
        foreign,
    })
}

/// Decode the declared input parameters, canonically sorted by name.
fn parameters(items: Option<&Vec<Json>>) -> Result<Vec<super::query::Parameter>, DiagnosticSet> {
    let items = match items {
        Some(items) => items,
        None => return Ok(Vec::new()),
    };
    if items.len() > MAX_PARAMETERS {
        return Err(diagnostic::input_invalid("parameter-bound"));
    }
    let mut parameters = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("parameter-shape"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "name" | "type") {
                return Err(diagnostic::input_invalid("parameter-field"));
            }
        }
        if object.len() != 2 {
            return Err(diagnostic::input_invalid("parameter-missing"));
        }
        let name = ParameterName::parse(
            object
                .get("name")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("parameter-name"))?,
        )
        .map_err(|_| diagnostic::input_invalid("parameter-name"))?;
        let parameter_type = type_expression(
            object
                .get("type")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("parameter-type"))?,
        )?;
        parameters.push(super::query::Parameter {
            name,
            parameter_type,
        });
    }
    parameters.sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
    let mut deduped = parameters.clone();
    deduped.dedup_by(|left, right| left.name == right.name);
    if deduped.len() != parameters.len() {
        return Err(diagnostic::input_invalid("parameter-duplicate"));
    }
    Ok(parameters)
}

/// The bounded model type-expression text of one parameter. The closed
/// grammar and symbol resolution are checked against the bound Model
/// at resolution time; the wire level refuses control characters,
/// whitespace noise, and oversize text.
fn type_expression(text: &str) -> Result<String, DiagnosticSet> {
    if text.is_empty() || text.len() > 128 {
        return Err(diagnostic::input_invalid("type-expression"));
    }
    if !text.bytes().all(|byte| {
        byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'.' | b'<' | b'>' | b'?' | b'_')
    }) {
        return Err(diagnostic::input_invalid("type-expression"));
    }
    Ok(text.to_owned())
}
/// Decode one filter AST with incremental bound checks (depth, leaf
/// count, and set size are enforced while decoding, so a hostile wire
/// can never allocate a deep tree first).
fn filter_expr(value: &Json, depth: usize) -> Result<FilterExpr, DiagnosticSet> {
    if depth >= super::version::MAX_FILTER_DEPTH {
        return Err(diagnostic::input_invalid("filter-depth"));
    }
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("filter-shape"))?;
    let logical = |key: &str| -> Result<Option<FilterExpr>, DiagnosticSet> {
        if !object.contains_key(key) {
            return Ok(None);
        }
        if object.len() != 1 {
            return Err(diagnostic::input_invalid("filter-shape"));
        }
        let items = object
            .get(key)
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("filter-operands"))?;
        if items.is_empty() || items.len() > super::version::MAX_FILTER_OPERANDS {
            return Err(diagnostic::input_invalid("filter-operand-count"));
        }
        let mut operands = Vec::with_capacity(items.len());
        for item in items {
            operands.push(filter_expr(item, depth + 1)?);
        }
        Ok(Some(match key {
            "and" => FilterExpr::And(operands),
            _ => FilterExpr::Or(operands),
        }))
    };
    if let Some(expr) = logical("and")? {
        return Ok(expr);
    }
    if let Some(expr) = logical("or")? {
        return Ok(expr);
    }
    if object.contains_key("not") {
        if object.len() != 1 {
            return Err(diagnostic::input_invalid("filter-shape"));
        }
        let inner = object
            .get("not")
            .ok_or_else(|| diagnostic::input_invalid("filter-shape"))?;
        return Ok(FilterExpr::Not(Box::new(filter_expr(inner, depth + 1)?)));
    }
    if object.len() < 2
        || object.len() > 3
        || !object.contains_key("field")
        || !object.contains_key("op")
    {
        return Err(diagnostic::input_invalid("filter-leaf-shape"));
    }
    let field = FieldName::parse(
        object
            .get("field")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("filter-field"))?,
    )
    .map_err(|_| diagnostic::input_invalid("filter-field"))?;
    let op = FilterOp::parse(
        object
            .get("op")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("filter-op"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("filter-op"))?;
    let filter_value = match object.get("value") {
        Some(value) => Some(filter_value(value)?),
        None => None,
    };
    match (op.carries_value(), filter_value.is_some()) {
        (true, false) => return Err(diagnostic::input_invalid("filter-value-missing")),
        (false, true) => return Err(diagnostic::input_invalid("filter-value-forbidden")),
        (false, false) | (true, true) => {}
    }
    if let Some(value) = &filter_value {
        if op.expects_set() {
            let is_set = matches!(
                value,
                FilterValue::Literal(Literal::Set(_)) | FilterValue::Param(_)
            );
            if !is_set {
                return Err(diagnostic::input_invalid("filter-set-expected"));
            }
        } else if let FilterValue::Literal(Literal::Set(_)) = value {
            return Err(diagnostic::input_invalid("filter-set-unexpected"));
        }
    }
    Ok(FilterExpr::Leaf(FilterLeaf {
        field,
        op,
        value: filter_value,
    }))
}

/// Decode one filter value: a typed literal or a parameter reference.
fn filter_value(value: &Json) -> Result<FilterValue, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("filter-value-shape"))?;
    if object.len() != 1 {
        return Err(diagnostic::input_invalid("filter-value-shape"));
    }
    if let Some(param) = object.get("param") {
        return Ok(FilterValue::Param(
            ParameterName::parse(
                param
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("filter-param"))?,
            )
            .map_err(|_| diagnostic::input_invalid("filter-param"))?,
        ));
    }
    if let Some(literal) = object.get("literal") {
        return Ok(FilterValue::Literal(literal_node(literal)?));
    }
    Err(diagnostic::input_invalid("filter-value-shape"))
}

/// Decode one typed literal node.
fn literal_node(value: &Json) -> Result<Literal, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("literal-shape"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "kind" | "value") {
            return Err(diagnostic::input_invalid("literal-field"));
        }
    }
    if object.len() != 2 {
        return Err(diagnostic::input_invalid("literal-shape"));
    }
    let kind = object
        .get("kind")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("literal-kind"))?;
    let value = object
        .get("value")
        .ok_or_else(|| diagnostic::input_invalid("literal-value"))?;
    let literal = match kind {
        "boolean" => Literal::Bool(
            value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("literal-boolean"))?,
        ),
        "integer" => Literal::Integer(
            value
                .as_i64()
                .filter(|value| value.abs() <= 9_007_199_254_740_991)
                .ok_or_else(|| diagnostic::input_invalid("literal-integer"))?,
        ),
        "decimal" => {
            let text = value
                .as_str()
                .filter(|text| text.len() <= 34 && is_canonical_decimal(text))
                .ok_or_else(|| diagnostic::input_invalid("literal-decimal"))?;
            Literal::Decimal(text.to_owned())
        }
        "string" => Literal::Str(
            value
                .as_str()
                .filter(|text| text.len() <= super::version::MAX_LITERAL_BYTES)
                .ok_or_else(|| diagnostic::input_invalid("literal-string"))?
                .to_owned(),
        ),
        "date" => Literal::Date(
            value
                .as_str()
                .filter(|text| is_date(text))
                .ok_or_else(|| diagnostic::input_invalid("literal-date"))?
                .to_owned(),
        ),
        "datetime" => Literal::DateTime(
            value
                .as_str()
                .filter(|text| is_datetime(text))
                .ok_or_else(|| diagnostic::input_invalid("literal-datetime"))?
                .to_owned(),
        ),
        "uuid" => Literal::Uuid(
            value
                .as_str()
                .filter(|text| is_uuid(text))
                .ok_or_else(|| diagnostic::input_invalid("literal-uuid"))?
                .to_owned(),
        ),
        "uri" => Literal::Uri(
            value
                .as_str()
                .filter(|text| text.len() <= super::version::MAX_LITERAL_BYTES && is_uri(text))
                .ok_or_else(|| diagnostic::input_invalid("literal-uri"))?
                .to_owned(),
        ),
        "enum-member" => Literal::Enum(
            value
                .as_str()
                .filter(|text| !text.is_empty() && text.len() <= super::version::MAX_LITERAL_BYTES)
                .ok_or_else(|| diagnostic::input_invalid("literal-enum"))?
                .to_owned(),
        ),
        "set" => {
            let items = value
                .as_array()
                .filter(|items| !items.is_empty() && items.len() <= super::version::MAX_SET_ITEMS)
                .ok_or_else(|| diagnostic::input_invalid("literal-set"))?;
            let mut members = Vec::with_capacity(items.len());
            for item in items {
                let member = literal_node(item)?;
                if matches!(member, Literal::Set(_)) {
                    return Err(diagnostic::input_invalid("literal-set-nested"));
                }
                members.push(member);
            }
            let mut sorted = members.clone();
            sorted.sort();
            sorted.dedup();
            if sorted.len() != members.len() {
                return Err(diagnostic::input_invalid("literal-set-duplicate"));
            }
            Literal::Set(members)
        }
        _ => return Err(diagnostic::input_invalid("literal-kind")),
    };
    if !literal_within_bounds(&literal) {
        return Err(diagnostic::input_invalid("literal-bound"));
    }
    Ok(literal)
}

/// Decode the ordered sort keys.
fn sort_keys(items: &[Json]) -> Result<Vec<SortKey>, DiagnosticSet> {
    if items.is_empty() || items.len() > MAX_SORT_KEYS {
        return Err(diagnostic::input_invalid("sort-bound"));
    }
    let mut keys = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("sort-shape"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "field" | "direction") {
                return Err(diagnostic::input_invalid("sort-field"));
            }
        }
        if object.len() != 2 {
            return Err(diagnostic::input_invalid("sort-missing"));
        }
        let field = FieldName::parse(
            object
                .get("field")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("sort-key-field"))?,
        )
        .map_err(|_| diagnostic::input_invalid("sort-key-field"))?;
        let direction = Direction::parse(
            object
                .get("direction")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("sort-direction"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("sort-direction"))?;
        keys.push(SortKey { field, direction });
    }
    let mut deduped = keys.clone();
    deduped.sort_by(|left, right| left.field.as_str().cmp(right.field.as_str()));
    deduped.dedup_by(|left, right| left.field == right.field);
    if deduped.len() != keys.len() {
        return Err(diagnostic::input_invalid("sort-duplicate"));
    }
    Ok(keys)
}

/// Decode one pagination block.
fn pagination(object: &WireMap) -> Result<Pagination, DiagnosticSet> {
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "strategy" | "limit" | "offset" | "keyParameter"
        ) {
            return Err(diagnostic::input_invalid("pagination-field"));
        }
    }
    let strategy = PaginationStrategy::parse(
        object
            .get("strategy")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("pagination-strategy"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("pagination-strategy"))?;
    let limit = object
        .get("limit")
        .and_then(Json::as_i64)
        .filter(|limit| (1..=MAX_LIMIT).contains(limit))
        .ok_or_else(|| diagnostic::input_invalid("pagination-limit"))?;
    let offset = match object.get("offset") {
        Some(value) => {
            if strategy != PaginationStrategy::Offset {
                return Err(diagnostic::input_invalid("pagination-offset-strategy"));
            }
            Some(
                value
                    .as_i64()
                    .filter(|offset| (0..=MAX_OFFSET).contains(offset))
                    .ok_or_else(|| diagnostic::input_invalid("pagination-offset"))?,
            )
        }
        None => None,
    };
    let key_parameter = match object.get("keyParameter") {
        Some(value) => {
            if strategy != PaginationStrategy::Cursor {
                return Err(diagnostic::input_invalid("pagination-key-strategy"));
            }
            Some(
                ParameterName::parse(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("pagination-key-param"))?,
                )
                .map_err(|_| diagnostic::input_invalid("pagination-key-param"))?,
            )
        }
        None => None,
    };
    Ok(Pagination {
        strategy,
        limit,
        offset,
        key_parameter,
    })
}

/// Decode the selected projection fields, keeping the declared output
/// order (projection order is behavior).
fn selection_fields(items: &[Json]) -> Result<Vec<SelectionField>, DiagnosticSet> {
    if items.is_empty() || items.len() > MAX_SELECTION {
        return Err(diagnostic::input_invalid("selection-bound"));
    }
    let mut fields = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("selection-shape"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "field" | "as") {
                return Err(diagnostic::input_invalid("selection-field"));
            }
        }
        let field = FieldName::parse(
            object
                .get("field")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("selection-key"))?,
        )
        .map_err(|_| diagnostic::input_invalid("selection-key"))?;
        let alias = match object.get("as") {
            Some(value) => Some(
                FieldName::parse(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("selection-alias"))?,
                )
                .map_err(|_| diagnostic::input_invalid("selection-alias"))?,
            ),
            None => None,
        };
        fields.push(SelectionField { field, alias });
    }
    let mut deduped: Vec<&SelectionField> = fields.iter().collect();
    deduped.sort_by(|left, right| left.field.as_str().cmp(right.field.as_str()));
    deduped.dedup_by(|left, right| left.field == right.field);
    if deduped.len() != fields.len() {
        return Err(diagnostic::input_invalid("selection-duplicate"));
    }
    Ok(fields)
}

/// Decode the semantic-link includes, canonically sorted by path.
fn includes(items: Option<&Vec<Json>>) -> Result<Vec<Include>, DiagnosticSet> {
    let items = match items {
        Some(items) => items,
        None => return Ok(Vec::new()),
    };
    if items.len() > MAX_INCLUDES {
        return Err(diagnostic::input_invalid("include-bound"));
    }
    let mut includes = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("include-shape"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "path" | "as") {
                return Err(diagnostic::input_invalid("include-field"));
            }
        }
        let path = object
            .get("path")
            .and_then(Json::as_array)
            .filter(|path| !path.is_empty() && path.len() <= MAX_INCLUDE_PATH)
            .ok_or_else(|| diagnostic::input_invalid("include-path"))?;
        let mut segments = Vec::with_capacity(path.len());
        for segment in path {
            segments.push(
                FieldName::parse(
                    segment
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("include-segment"))?,
                )
                .map_err(|_| diagnostic::input_invalid("include-segment"))?,
            );
        }
        let alias = match object.get("as") {
            Some(value) => Some(
                FieldName::parse(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("include-alias"))?,
                )
                .map_err(|_| diagnostic::input_invalid("include-alias"))?,
            ),
            None => None,
        };
        includes.push(Include {
            path: segments,
            alias,
        });
    }
    includes.sort_by_key(|left| include_path_key(&left.path));
    let mut deduped = includes.clone();
    deduped.dedup_by(|left, right| include_path_key(&left.path) == include_path_key(&right.path));
    if deduped.len() != includes.len() {
        return Err(diagnostic::input_invalid("include-duplicate"));
    }
    Ok(includes)
}

/// The byte key of one include path.
pub(crate) fn include_path_key(path: &[FieldName]) -> String {
    path.iter()
        .map(|segment| segment.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

/// Decode a sorted set of semantic references.
fn semantic_refs(
    items: Option<&Vec<Json>>,
    bound: usize,
    tag: &'static str,
) -> Result<Vec<SemanticId>, DiagnosticSet> {
    let items = match items {
        Some(items) => items,
        None => return Ok(Vec::new()),
    };
    if items.is_empty() || items.len() > bound {
        return Err(diagnostic::input_invalid(tag));
    }
    let mut refs = Vec::with_capacity(items.len());
    for item in items {
        refs.push(
            SemanticId::parse(
                item.as_str()
                    .ok_or_else(|| diagnostic::input_invalid(tag))?,
            )
            .map_err(|_| diagnostic::input_invalid(tag))?,
        );
    }
    refs.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    let mut deduped = refs.clone();
    deduped.dedup();
    if deduped.len() != refs.len() {
        return Err(diagnostic::input_invalid("reference-duplicate"));
    }
    Ok(refs)
}

/// Decode one bounded cost-hint block.
fn cost_hint(object: &WireMap) -> Result<CostHint, DiagnosticSet> {
    for key in object.keys() {
        if !matches!(key.as_str(), "maxRows" | "expensive") {
            return Err(diagnostic::input_invalid("cost-field"));
        }
    }
    if object.is_empty() {
        return Err(diagnostic::input_invalid("cost-missing"));
    }
    let max_rows = match object.get("maxRows") {
        Some(value) => Some(
            value
                .as_i64()
                .filter(|rows| (1..=MAX_COST_ROWS).contains(rows))
                .ok_or_else(|| diagnostic::input_invalid("cost-rows"))?,
        ),
        None => None,
    };
    let expensive = match object.get("expensive") {
        Some(value) => Some(
            value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("cost-expensive"))?,
        ),
        None => None,
    };
    Ok(CostHint {
        max_rows,
        expensive,
    })
}

/// Decode one foreign escape hatch.
fn foreign_ref(object: &WireMap) -> Result<ForeignRef, DiagnosticSet> {
    for key in object.keys() {
        if !matches!(key.as_str(), "capability" | "reason") {
            return Err(diagnostic::input_invalid("foreign-field"));
        }
    }
    if object.len() != 2 {
        return Err(diagnostic::input_invalid("foreign-missing"));
    }
    let capability = object
        .get("capability")
        .and_then(Json::as_str)
        .filter(|capability| {
            !capability.is_empty()
                && capability.len() <= 64
                && capability
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
        .ok_or_else(|| diagnostic::input_invalid("foreign-capability"))?;
    let reason = object
        .get("reason")
        .and_then(Json::as_str)
        .filter(|reason| is_safe_text(reason))
        .ok_or_else(|| diagnostic::input_invalid("foreign-reason"))?;
    Ok(ForeignRef {
        capability: capability.to_owned(),
        reason: reason.to_owned(),
    })
}

/// Whether one text is bounded declaration text: never source text,
/// paths, credentials, URLs, runtime values, or provider output.
fn is_safe_text(text: &str) -> bool {
    if text.is_empty() || text.len() > MAX_REASON_BYTES {
        return false;
    }
    let mut bytes = text.bytes();
    match bytes.next() {
        Some(byte) if byte.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    text.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b' ' | b'.' | b',' | b':' | b';' | b'(' | b')' | b'/' | b'_' | b'-'
            )
    })
}

/// Whether one text is a canonical decimal literal.
fn is_canonical_decimal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    if index < bytes.len() && bytes[index] == b'-' {
        index += 1;
    }
    let int_start = index;
    if index < bytes.len() && bytes[index] == b'0' {
        index += 1;
    } else {
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == int_start {
            return false;
        }
        if index - int_start > 20 {
            return false;
        }
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let frac_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == frac_start || index - frac_start > 12 {
            return false;
        }
    }
    index == bytes.len()
}

/// Whether one text is a calendar date (`YYYY-MM-DD`, months 01-12,
/// days 01-31; month-length nuance stays with the declaring owner).
fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !text[..4].bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let month = &text[5..7];
    let month_ok = month == "01"
        || month == "02"
        || month == "03"
        || month == "04"
        || month == "05"
        || month == "06"
        || month == "07"
        || month == "08"
        || month == "09"
        || month == "10"
        || month == "11"
        || month == "12";
    let day_ok = matches!(
        (bytes[8], bytes[9]),
        (b'0', b'1'..=b'9') | (b'1' | b'2', b'0'..=b'9') | (b'3', b'0' | b'1')
    );
    month_ok && day_ok
}

/// Whether one text is an offset timestamp
/// (`YYYY-MM-DDTHH:MM:SS[.f..](Z|+HH:MM|-HH:MM)`).
fn is_datetime(text: &str) -> bool {
    if text.len() < 20 {
        return false;
    }
    let (day, rest) = text.split_at(10);
    if !rest.starts_with('T') || !is_date(day) {
        return false;
    }
    let time = &rest[1..];
    let bytes = time.as_bytes();
    if bytes.len() < 8 || bytes[2] != b':' || bytes[5] != b':' {
        return false;
    }
    let digits = |slice: &str| slice.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(&time[..2]) || !digits(&time[3..5]) || !digits(&time[6..8]) {
        return false;
    }
    let (hour, minute, second) = (
        time[..2].parse::<u32>().unwrap_or(99),
        time[3..5].parse::<u32>().unwrap_or(99),
        time[6..8].parse::<u32>().unwrap_or(99),
    );
    if hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    let tail = &time[8..];
    match tail.strip_prefix('.') {
        Some(fraction_and_offset) => {
            let digits_end = fraction_and_offset
                .find(|character: char| !character.is_ascii_digit())
                .unwrap_or(fraction_and_offset.len());
            if digits_end == 0 || digits_end > 6 {
                return false;
            }
            is_offset(&fraction_and_offset[digits_end..])
        }
        None => is_offset(tail),
    }
}

/// Whether one text is a `Z` or `+HH:MM`/`-HH:MM` offset.
fn is_offset(text: &str) -> bool {
    if text == "Z" {
        return true;
    }
    let bytes = text.as_bytes();
    if bytes.len() != 6 || (bytes[0] != b'+' && bytes[0] != b'-') || bytes[3] != b':' {
        return false;
    }
    let hour: u32 = text[1..3].parse().unwrap_or(99);
    let minute: u32 = text[4..6].parse().unwrap_or(99);
    hour <= 23 && minute <= 59
}

/// Whether one text is a lowercase UUID.
fn is_uuid(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase() {
                    return false;
                }
            }
        }
    }
    true
}

/// Whether one text is a bounded absolute URI.
fn is_uri(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let colon = match text.find(':') {
        Some(colon) => colon,
        None => return false,
    };
    if colon == 0
        || !text[..colon].bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'+' | b'.' | b'-')
        })
    {
        return false;
    }
    !text.chars().any(|character| {
        character.is_whitespace() || character == '<' || character == '>' || character == '"'
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal well-formed attachment body used across wire tests.
    pub(crate) fn sample_attachment() -> Json {
        json!({
            "schemaVersion": version::SCHEMA_VERSION,
            "identity": version::IDENTITY,
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
            "tenancy": [],
            "queries": []
        })
    }

    #[test]
    fn accepts_the_sample_body() {
        assert!(from_value(&sample_attachment()).is_ok());
    }

    #[test]
    fn refuses_unknown_and_missing_fields() {
        let mut hostile = sample_attachment();
        hostile["extra"] = json!(1);
        assert!(from_value(&hostile).is_err());
        let mut missing = sample_attachment();
        missing.as_object_mut().expect("object").remove("tenancy");
        assert!(from_value(&missing).is_err());
    }

    #[test]
    fn refuses_wrong_identity_or_schema_version() {
        let mut hostile = sample_attachment();
        hostile["identity"] = json!("dev.lekalo.query-model@0.9.0");
        assert!(from_value(&hostile).is_err());
        hostile["identity"] = json!(version::IDENTITY);
        hostile["schemaVersion"] = json!("lekalo/query-model/v0.9.0");
        assert!(from_value(&hostile).is_err());
    }

    #[test]
    fn refuses_deep_filter_bombs_before_allocation() {
        let mut node = json!({"field": "a", "op": "is-null"});
        for _ in 0..super::super::version::MAX_FILTER_DEPTH {
            node = json!({"not": node});
        }
        let deep = node;
        let mut hostile = sample_attachment();
        hostile["queries"] = json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "list",
            "consistency": "strong",
            "filter": deep
        }]);
        assert!(from_value(&hostile).is_err());
    }

    #[test]
    fn refuses_leaf_shape_and_nullness_value_conflicts() {
        let base = |filter: Json| {
            let mut hostile = sample_attachment();
            hostile["queries"] = json!([{
                "query": "planner.q",
                "source": "planner.task",
                "cardinality": "list",
                "consistency": "strong",
                "filter": filter
            }]);
            hostile
        };
        assert!(from_value(&base(json!({"field": "a"}))).is_err());
        assert!(from_value(&base(json!({"field": "a", "op": "is-null", "value": {"literal": {"kind": "boolean", "value": true}}}))).is_err());
        assert!(from_value(&base(json!({"field": "a", "op": "eq"}))).is_err());
        assert!(from_value(&base(json!({"field": "a", "op": "contains", "value": {"literal": {"kind": "string", "value": "x"}}}))).is_err());
    }
}
