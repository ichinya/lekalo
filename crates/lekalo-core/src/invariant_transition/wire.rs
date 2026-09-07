//! Wire normalization of the invariant-transition attachment (issue
//! #63).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`InvariantTransitionAttachment`](super::InvariantTransitionAttachment).
//! It fails closed before semantic processing: unknown or missing
//! fields, wrong identities, malformed identifiers, digests, bounds,
//! and out-of-bound predicate trees each return one typed registered
//! diagnostic and no partial attachment. Semantic rules (kind/field
//! coherence, reference resolution, precondition-invariant-policy
//! separation, reachability, cycles, mapping evidence) live in the
//! attachment's semantic self-check.

use crate::diagnostics::DiagnosticSet;
use crate::extended_effects::identity::ErrorRef;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{FieldName, NamespacedId, SemanticId};
use serde_json::{Map, Value as Json};

/// The parsed wire object type.
type WireMap = Map<String, Json>;

use super::capability::{is_provider_capability, CapabilityRequirement, RequirementLevel};
use super::diagnostic;
use super::expr::{value_depth, PredicateNode};
use super::id::{ExpressionRef, StateRef};
use super::invariant::{FieldRef, Invariant, InvariantKind};
use super::state::{
    CyclePolicy, DeadPolicy, Duration, DurationUnit, StateSpace, StateState, ValueNode,
};
use super::trace::{
    EnforcementLayer, Evidence, EvidenceStatus, MappingConfidence, MappingRelation, MappingStatus,
    PropertyHint, PropertyKind, ScenarioRef, Subject, SubjectKind, TargetKind, TargetRef,
    VerificationMapping,
};
use super::transition::{Assignment, AssignmentValue, Transition};
use super::version;
use super::{InvariantTransitionAttachment, ModelPin};

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "sourceMapRef",
    "stateSpaces",
    "invariants",
    "transitions",
    "verificationMappings",
    "propertyHints",
];

/// The required top-level members (`sourceMapRef` and `propertyHints`
/// are optional).
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "stateSpaces",
    "invariants",
    "transitions",
    "verificationMappings",
];

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial attachment. Pure: no source,
/// model, cache, report, network, process, or target access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<InvariantTransitionAttachment, DiagnosticSet> {
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
        "dev.lekalo.ir@0.1.0",
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
    let state_spaces = state_spaces(
        object
            .get("stateSpaces")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("state-space-list"))?,
    )?;
    let invariants = invariants(
        object
            .get("invariants")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("invariant-list"))?,
    )?;
    let transitions = transitions(
        object
            .get("transitions")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("transition-list"))?,
    )?;
    let mappings = verification_mappings(
        object
            .get("verificationMappings")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("mapping-list"))?,
    )?;
    let property_hints = match object.get("propertyHints") {
        Some(value) => property_hints(
            value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("hint-list"))?,
        )?,
        None => Vec::new(),
    };
    let attachment = InvariantTransitionAttachment::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_digest,
        source_map_ref,
        state_spaces,
        invariants,
        transitions,
        mappings,
        property_hints,
    );
    attachment.semantic_self_check()?;
    Ok(attachment)
}

/// Parse the bound source Model pin.
fn model_pin(json: &Json) -> Result<ModelPin, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("model-ref"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "modelVersion" | "digest") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let pin_version = object
        .get("modelVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("model-version"))?;
    let pin = match pin_version {
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
    Ok(ModelPin {
        version: pin,
        digest,
    })
}

/// Parse one `{identity, digest}` contract reference.
fn digest_member(json: &Json, identity: &str) -> Result<Sha256Digest, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "identity" | "digest") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    if object.get("identity").and_then(Json::as_str) != Some(identity) {
        return Err(diagnostic::input_invalid("contract-identity"));
    }
    Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("ir-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("ir-digest"))
}

/// One bounded string member.
fn string_member<'a>(object: &'a WireMap, key: &str) -> Result<&'a str, DiagnosticSet> {
    object
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("member"))
}

/// One bounded signed integer member.
fn integer_member(object: &WireMap, key: &str) -> Result<i64, DiagnosticSet> {
    let value = object
        .get(key)
        .and_then(Json::as_i64)
        .ok_or_else(|| diagnostic::input_invalid("integer"))?;
    if value.abs() > version::MAX_INTEGER {
        Err(diagnostic::input_invalid("integer-bound"))
    } else {
        Ok(value)
    }
}

/// One optional bounded string member.
fn optional_bounded_string(object: &WireMap, key: &str) -> Result<Option<String>, DiagnosticSet> {
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => {
            let text = value
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("member"))?;
            if text.is_empty() || text.len() > version::MAX_LITERAL_BYTES {
                return Err(diagnostic::input_invalid("text-bound"));
            }
            let first = text.as_bytes()[0];
            let rest_ok = text.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b' ' | b'.' | b',' | b':' | b';' | b'(' | b')' | b'/' | b'_' | b'-'
                    )
            });
            if !(first.is_ascii_alphanumeric() && rest_ok) {
                return Err(diagnostic::input_invalid("text-shape"));
            }
            Ok(Some(text.to_owned()))
        }
    }
}

/// One bounded array member with the closed length bound.
fn bounded_array<'a>(
    object: &'a WireMap,
    key: &str,
    bound: usize,
) -> Result<&'a Vec<Json>, DiagnosticSet> {
    let array = object
        .get(key)
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("array"))?;
    if array.len() > bound {
        return Err(diagnostic::input_invalid("array-bound"));
    }
    Ok(array)
}

/// One optional bounded array member.
fn optional_bounded_array<'a>(
    object: &'a WireMap,
    key: &str,
    bound: usize,
) -> Result<Option<&'a Vec<Json>>, DiagnosticSet> {
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => {
            let array = value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("array"))?;
            if array.len() > bound {
                return Err(diagnostic::input_invalid("array-bound"));
            }
            Ok(Some(array))
        }
    }
}

/// One typed identifier member parsed through the accepted grammar.
fn id_member<T>(
    object: &WireMap,
    key: &str,
    parse: fn(&str) -> Result<T, crate::scenario::id::IdError>,
) -> Result<T, DiagnosticSet> {
    let text = string_member(object, key)?;
    parse(text).map_err(|_| diagnostic::input_invalid("identifier"))
}

/// One opaque error reference member.
fn error_member(object: &WireMap, key: &str) -> Result<ErrorRef, DiagnosticSet> {
    ErrorRef::parse(string_member(object, key)?).map_err(|_| diagnostic::input_invalid("error-ref"))
}

/// Parse every declared state space.
fn state_spaces(array: &[Json]) -> Result<Vec<StateSpace>, DiagnosticSet> {
    if array.len() > version::MAX_STATE_SPACES {
        return Err(diagnostic::input_invalid("state-space-bound"));
    }
    let mut spaces = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("state-space-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "stateSpaceId" | "entity" | "states" | "cyclePolicy" | "deadPolicy"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let state_space_id = id_member(object, "stateSpaceId", SemanticId::parse)?;
        let entity = id_member(object, "entity", SemanticId::parse)?;
        let state_bound = version::MAX_STATES;
        let states_json = bounded_array(object, "states", state_bound)?;
        if states_json.is_empty() {
            return Err(diagnostic::input_invalid("empty-state-space"));
        }
        let mut states = Vec::with_capacity(states_json.len());
        for state in states_json {
            let state_object = state
                .as_object()
                .ok_or_else(|| diagnostic::input_invalid("state-shape"))?;
            for key in state_object.keys() {
                if !matches!(key.as_str(), "stateId" | "initial" | "terminal") {
                    return Err(diagnostic::input_invalid("unknown-field"));
                }
            }
            let state_id = id_member(state_object, "stateId", StateRef::parse)?;
            let initial = match state_object.get("initial") {
                None | Some(Json::Null) => false,
                Some(value) => value
                    .as_bool()
                    .ok_or_else(|| diagnostic::input_invalid("flag"))?,
            };
            let terminal = match state_object.get("terminal") {
                None | Some(Json::Null) => false,
                Some(value) => value
                    .as_bool()
                    .ok_or_else(|| diagnostic::input_invalid("flag"))?,
            };
            states.push(StateState {
                state_id,
                initial,
                terminal,
            });
        }
        let cycle_policy = CyclePolicy::from_key(string_member(object, "cyclePolicy")?)
            .ok_or_else(|| diagnostic::input_invalid("cycle-policy"))?;
        let dead_policy = DeadPolicy::from_key(string_member(object, "deadPolicy")?)
            .ok_or_else(|| diagnostic::input_invalid("dead-policy"))?;
        spaces.push(StateSpace {
            state_space_id,
            entity,
            states,
            cycle_policy,
            dead_policy,
        });
    }
    Ok(spaces)
}

/// Parse one closed value node.
fn value_node(json: &Json, depth: usize) -> Result<ValueNode, DiagnosticSet> {
    if depth > version::MAX_PREDICATE_DEPTH + 1 {
        return Err(diagnostic::input_invalid("value-depth"));
    }
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("value-shape"))?;
    let kind = string_member(object, "kind")?;
    let child = depth + 1;
    let node = match kind {
        "null" => {
            if object.len() != 1 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            ValueNode::Null
        }
        "boolean" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            ValueNode::Boolean(
                object
                    .get("value")
                    .and_then(Json::as_bool)
                    .ok_or_else(|| diagnostic::input_invalid("value"))?,
            )
        }
        "integer" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let value = object
                .get("value")
                .and_then(Json::as_i64)
                .ok_or_else(|| diagnostic::input_invalid("value"))?;
            if value.abs() > version::MAX_INTEGER {
                return Err(diagnostic::input_invalid("integer-bound"));
            }
            ValueNode::Integer(value)
        }
        "string" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let value = string_member(object, "value")?;
            if value.len() > version::MAX_LITERAL_BYTES {
                return Err(diagnostic::input_invalid("string-bound"));
            }
            ValueNode::String(value.to_owned())
        }
        "decimal" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let value = string_member(object, "value")?;
            if value.len() > 34 || !is_canonical_decimal(value) {
                return Err(diagnostic::input_invalid("decimal"));
            }
            ValueNode::Decimal(value.to_owned())
        }
        "date" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let value = string_member(object, "value")?;
            if !is_date(value) {
                return Err(diagnostic::input_invalid("date"));
            }
            ValueNode::Date(value.to_owned())
        }
        "datetime" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let value = string_member(object, "value")?;
            if value.len() > 32 || !is_datetime(value) {
                return Err(diagnostic::input_invalid("datetime"));
            }
            ValueNode::DateTime(value.to_owned())
        }
        "uuid" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let value = string_member(object, "value")?;
            if !is_uuid(value) {
                return Err(diagnostic::input_invalid("uuid"));
            }
            ValueNode::Uuid(value.to_owned())
        }
        "uri" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let value = string_member(object, "value")?;
            if value.len() > version::MAX_LITERAL_BYTES || !is_uri(value) {
                return Err(diagnostic::input_invalid("uri"));
            }
            ValueNode::Uri(value.to_owned())
        }
        "enum_member" => {
            if object.len() != 3 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let type_ref = id_member(object, "typeRef", SemanticId::parse)?;
            let value = string_member(object, "value")?;
            if value.is_empty() || value.len() > 64 {
                return Err(diagnostic::input_invalid("enum-member"));
            }
            ValueNode::EnumMember {
                type_ref,
                value: value.to_owned(),
            }
        }
        "field" => {
            if object.len() > 3 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let entity = match object.get("entity") {
                None => None,
                Some(_) => Some(id_member(object, "entity", SemanticId::parse)?),
            };
            ValueNode::Field {
                entity,
                field: id_member(object, "field", FieldName::parse)?,
            }
        }
        "input" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            ValueNode::Input {
                field: id_member(object, "field", FieldName::parse)?,
            }
        }
        "prior" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            ValueNode::Prior {
                field: id_member(object, "field", FieldName::parse)?,
            }
        }
        "now" => {
            if object.len() != 1 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            ValueNode::Now
        }
        "list" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let items = bounded_array(object, "items", version::MAX_VALUE_ITEMS)?;
            let mut parsed = Vec::with_capacity(items.len());
            for item in items {
                parsed.push(value_node(item, child)?);
            }
            ValueNode::List(parsed)
        }
        "object" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let entries = bounded_array(object, "entries", version::MAX_VALUE_ENTRIES)?;
            let mut parsed = Vec::with_capacity(entries.len());
            for entry in entries {
                let entry_object = entry
                    .as_object()
                    .ok_or_else(|| diagnostic::input_invalid("entry-shape"))?;
                for key in entry_object.keys() {
                    if !matches!(key.as_str(), "key" | "value") {
                        return Err(diagnostic::input_invalid("unknown-field"));
                    }
                }
                let key = string_member(entry_object, "key")?.to_owned();
                let value = value_node(
                    entry_object
                        .get("value")
                        .ok_or_else(|| diagnostic::input_invalid("entry-value"))?,
                    child,
                )?;
                parsed.push((key, value));
            }
            ValueNode::Object(parsed)
        }
        _ => return Err(diagnostic::input_invalid("value-kind")),
    };
    if value_depth(&node) > version::MAX_PREDICATE_DEPTH + 1 {
        return Err(diagnostic::input_invalid("value-depth"));
    }
    Ok(node)
}

/// Parse one closed predicate node with its depth bound.
fn predicate_node(json: &Json, depth: usize) -> Result<PredicateNode, DiagnosticSet> {
    if depth >= version::MAX_PREDICATE_DEPTH {
        return Err(diagnostic::input_invalid("predicate-depth"));
    }
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("predicate-shape"))?;
    let op = string_member(object, "op")?;
    let child = depth + 1;
    let node = match op {
        "equal" | "not_equal" | "before" | "after" => {
            if object.len() != 3 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let left = value_node(
                object
                    .get("left")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            let right = value_node(
                object
                    .get("right")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            match op {
                "equal" => PredicateNode::Equal { left, right },
                "not_equal" => PredicateNode::NotEqual { left, right },
                "before" => PredicateNode::Before { left, right },
                _ => PredicateNode::After { left, right },
            }
        }
        "is_null" | "not_null" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let operand = value_node(
                object
                    .get("operand")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            if op == "is_null" {
                PredicateNode::IsNull { operand }
            } else {
                PredicateNode::NotNull { operand }
            }
        }
        "in_set" => {
            if object.len() != 3 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let operand = value_node(
                object
                    .get("operand")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            let values = bounded_array(object, "values", version::MAX_VALUE_ITEMS)?;
            let mut parsed = Vec::with_capacity(values.len());
            for value in values {
                parsed.push(value_node(value, child)?);
            }
            PredicateNode::InSet {
                operand,
                values: parsed,
            }
        }
        "all" | "any" => {
            if object.len() != 3 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let from = value_node(
                object
                    .get("from")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            let predicate = predicate_node(
                object
                    .get("predicate")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            let predicate = Box::new(predicate);
            if op == "all" {
                PredicateNode::All { from, predicate }
            } else {
                PredicateNode::Any { from, predicate }
            }
        }
        "and" | "or" => {
            if object.len() != 2 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let operands = bounded_array(object, "operands", version::MAX_PREDICATE_OPERANDS)?;
            if operands.len() < 2 {
                return Err(diagnostic::input_invalid("operand-count"));
            }
            let mut parsed = Vec::with_capacity(operands.len());
            for operand in operands {
                parsed.push(predicate_node(operand, child)?);
            }
            if op == "and" {
                PredicateNode::And { operands: parsed }
            } else {
                PredicateNode::Or { operands: parsed }
            }
        }
        "within" => {
            if object.len() != 3 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let left = value_node(
                object
                    .get("left")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            let duration_object = object
                .get("duration")
                .ok_or_else(|| diagnostic::input_invalid("operand"))?
                .as_object()
                .ok_or_else(|| diagnostic::input_invalid("duration-shape"))?;
            for key in duration_object.keys() {
                if !matches!(key.as_str(), "unit" | "amount") {
                    return Err(diagnostic::input_invalid("unknown-field"));
                }
            }
            let unit = DurationUnit::from_key(string_member(duration_object, "unit")?)
                .ok_or_else(|| diagnostic::input_invalid("duration-unit"))?;
            let amount = integer_member(duration_object, "amount")?;
            if !(1..=31_536_000).contains(&amount) {
                return Err(diagnostic::input_invalid("duration-bound"));
            }
            PredicateNode::Within {
                left,
                duration: Duration { unit, amount },
            }
        }
        "count" => {
            if object.len() != 4 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let from = value_node(
                object
                    .get("from")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            let min = integer_member(object, "min")?;
            let max = integer_member(object, "max")?;
            if min < 0
                || max < 0
                || min > version::MAX_CARDINALITY
                || max > version::MAX_CARDINALITY
            {
                return Err(diagnostic::input_invalid("cardinality-bound"));
            }
            PredicateNode::Count { from, min, max }
        }
        "member_of" => {
            if object.len() != 3 {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            let operand = value_node(
                object
                    .get("operand")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            let set = value_node(
                object
                    .get("set")
                    .ok_or_else(|| diagnostic::input_invalid("operand"))?,
                child,
            )?;
            PredicateNode::MemberOf { operand, set }
        }
        _ => return Err(diagnostic::input_invalid("predicate-op")),
    };
    Ok(node)
}

/// Parse every declared invariant.
fn invariants(array: &[Json]) -> Result<Vec<Invariant>, DiagnosticSet> {
    if array.len() > version::MAX_INVARIANTS {
        return Err(diagnostic::input_invalid("invariant-bound"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("invariant-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "invariantId"
                    | "stateSpaceId"
                    | "kind"
                    | "description"
                    | "fields"
                    | "predicate"
                    | "min"
                    | "max"
                    | "triggerState"
                    | "allowedStates"
                    | "partition"
                    | "maxActive"
                    | "aggregateRef"
                    | "requiredFields"
                    | "capabilityRequirement"
                    | "conflictErrorRef"
                    | "errorRefs"
                    | "requirementRefs"
                    | "scenarioRefs"
                    | "capabilityRefs"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let invariant_id = id_member(object, "invariantId", SemanticId::parse)?;
        let state_space_id = id_member(object, "stateSpaceId", SemanticId::parse)?;
        let kind = InvariantKind::from_key(string_member(object, "kind")?)
            .ok_or_else(|| diagnostic::input_invalid("invariant-kind"))?;
        let description = optional_bounded_string(object, "description")?;
        let fields = match optional_bounded_array(object, "fields", version::MAX_INVARIANT_FIELDS)?
        {
            Some(entries) => {
                let mut fields = Vec::with_capacity(entries.len());
                for entry in entries {
                    let field_object = entry
                        .as_object()
                        .ok_or_else(|| diagnostic::input_invalid("field-ref-shape"))?;
                    for key in field_object.keys() {
                        if !matches!(key.as_str(), "entity" | "field") {
                            return Err(diagnostic::input_invalid("unknown-field"));
                        }
                    }
                    let entity = match field_object.get("entity") {
                        None => None,
                        Some(_) => Some(id_member(field_object, "entity", SemanticId::parse)?),
                    };
                    fields.push(FieldRef {
                        entity,
                        field: id_member(field_object, "field", FieldName::parse)?,
                    });
                }
                fields
            }
            None => Vec::new(),
        };
        let predicate = match object.get("predicate") {
            Some(value) => Some(predicate_node(value, 0)?),
            None => None,
        };
        let min = match object.get("min") {
            None | Some(Json::Null) => None,
            Some(_) => {
                let value = integer_member(object, "min")?;
                if !(0..=version::MAX_CARDINALITY).contains(&value) {
                    return Err(diagnostic::input_invalid("cardinality-bound"));
                }
                Some(value)
            }
        };
        let max = match object.get("max") {
            None | Some(Json::Null) => None,
            Some(_) => {
                let value = integer_member(object, "max")?;
                if !(0..=version::MAX_CARDINALITY).contains(&value) {
                    return Err(diagnostic::input_invalid("cardinality-bound"));
                }
                Some(value)
            }
        };
        let trigger_state = match object.get("triggerState") {
            None | Some(Json::Null) => None,
            Some(_) => Some(id_member(object, "triggerState", StateRef::parse)?),
        };
        let allowed_states =
            match optional_bounded_array(object, "allowedStates", version::MAX_ALLOWED_STATES)? {
                Some(entries) => {
                    let mut states = Vec::with_capacity(entries.len());
                    for entry in entries {
                        states.push(
                            StateRef::parse(
                                entry
                                    .as_str()
                                    .ok_or_else(|| diagnostic::input_invalid("member"))?,
                            )
                            .map_err(|_| diagnostic::input_invalid("state-ref"))?,
                        );
                    }
                    states.sort();
                    let before = states.len();
                    states.dedup();
                    if states.len() != before {
                        return Err(diagnostic::input_invalid("duplicate-ref"));
                    }
                    states
                }
                None => Vec::new(),
            };
        let partition = match object.get("partition") {
            None | Some(Json::Null) => None,
            Some(value) => {
                let partition_object = value
                    .as_object()
                    .ok_or_else(|| diagnostic::input_invalid("field-ref-shape"))?;
                for key in partition_object.keys() {
                    if !matches!(key.as_str(), "entity" | "field") {
                        return Err(diagnostic::input_invalid("unknown-field"));
                    }
                }
                let entity = match partition_object.get("entity") {
                    None => None,
                    Some(_) => Some(id_member(partition_object, "entity", SemanticId::parse)?),
                };
                Some(FieldRef {
                    entity,
                    field: id_member(partition_object, "field", FieldName::parse)?,
                })
            }
        };
        let max_active = match object.get("maxActive") {
            None | Some(Json::Null) => false,
            Some(value) => {
                if value.as_i64() != Some(1) {
                    return Err(diagnostic::input_invalid("max-active"));
                }
                true
            }
        };
        let aggregate_ref = match object.get("aggregateRef") {
            None | Some(Json::Null) => None,
            Some(_) => Some(id_member(object, "aggregateRef", SemanticId::parse)?),
        };
        let required_fields = match optional_bounded_array(
            object,
            "requiredFields",
            version::MAX_INVARIANT_FIELDS,
        )? {
            Some(entries) => {
                let mut fields = Vec::with_capacity(entries.len());
                for entry in entries {
                    fields.push(
                        FieldName::parse(
                            entry
                                .as_str()
                                .ok_or_else(|| diagnostic::input_invalid("member"))?,
                        )
                        .map_err(|_| diagnostic::input_invalid("identifier"))?,
                    );
                }
                fields
            }
            None => Vec::new(),
        };
        let capability_requirement = match object.get("capabilityRequirement") {
            None | Some(Json::Null) => None,
            Some(value) => Some(capability_requirement(value)?),
        };
        let conflict_error_ref = match object.get("conflictErrorRef") {
            None | Some(Json::Null) => None,
            Some(_) => Some(error_member(object, "conflictErrorRef")?),
        };
        let error_refs = error_refs_of(object, "errorRefs")?;
        let requirement_refs = ref_list(
            object,
            "requirementRefs",
            version::MAX_REQUIREMENT_REFS,
            NamespacedId::parse,
        )?;
        let scenario_refs = scenario_refs(object)?;
        let capability_refs = ref_list(
            object,
            "capabilityRefs",
            version::MAX_CAPABILITY_REFS,
            NamespacedId::parse,
        )?;
        parsed.push(Invariant {
            invariant_id,
            state_space_id,
            kind,
            description,
            fields,
            predicate,
            min,
            max,
            trigger_state,
            allowed_states,
            partition,
            max_active,
            aggregate_ref,
            required_fields,
            capability_requirement,
            conflict_error_ref,
            error_refs,
            requirement_refs,
            scenario_refs,
            capability_refs,
        });
    }
    Ok(parsed)
}

/// Parse one target-capability requirement record.
fn capability_requirement(json: &Json) -> Result<CapabilityRequirement, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "requirementId" | "capability" | "minimum" | "reason"
        ) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let requirement_id = id_member(object, "requirementId", NamespacedId::parse)?;
    let capability = string_member(object, "capability")?.to_owned();
    if !super::capability::CAPABILITIES.contains(&capability.as_str())
        && !is_provider_capability(&capability)
    {
        return Err(diagnostic::input_invalid("capability"));
    }
    let minimum = RequirementLevel::from_key(string_member(object, "minimum")?)
        .ok_or_else(|| diagnostic::input_invalid("capability-minimum"))?;
    let reason = optional_bounded_string(object, "reason")?
        .ok_or_else(|| diagnostic::input_invalid("capability-reason"))?;
    Ok(CapabilityRequirement {
        requirement_id,
        capability,
        minimum,
        reason,
    })
}

/// Parse one list of typed reference strings with uniqueness.
fn ref_list<T: Ord>(
    object: &WireMap,
    key: &str,
    bound: usize,
    parse: fn(&str) -> Result<T, crate::scenario::id::IdError>,
) -> Result<Vec<T>, DiagnosticSet> {
    let entries = match optional_bounded_array(object, key, bound)? {
        Some(entries) => entries,
        None => return Ok(Vec::new()),
    };
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        parsed.push(
            parse(
                entry
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("member"))?,
            )
            .map_err(|_| diagnostic::input_invalid("identifier"))?,
        );
    }
    parsed.sort();
    let before = parsed.len();
    parsed.dedup();
    if parsed.len() != before {
        return Err(diagnostic::input_invalid("duplicate-ref"));
    }
    Ok(parsed)
}

/// Parse one list of opaque error references with uniqueness.
fn error_refs_of(object: &WireMap, key: &str) -> Result<Vec<ErrorRef>, DiagnosticSet> {
    let entries = match optional_bounded_array(object, key, version::MAX_ERROR_REFS)? {
        Some(entries) => entries,
        None => return Ok(Vec::new()),
    };
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        parsed.push(
            ErrorRef::parse(
                entry
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("member"))?,
            )
            .map_err(|_| diagnostic::input_invalid("error-ref"))?,
        );
    }
    parsed.sort();
    let before = parsed.len();
    parsed.dedup();
    if parsed.len() != before {
        return Err(diagnostic::input_invalid("duplicate-ref"));
    }
    Ok(parsed)
}

/// Parse the pinned scenario reference list with uniqueness.
fn scenario_refs(object: &WireMap) -> Result<Vec<ScenarioRef>, DiagnosticSet> {
    let entries = match optional_bounded_array(object, "scenarioRefs", version::MAX_SCENARIO_REFS)?
    {
        Some(entries) => entries,
        None => return Ok(Vec::new()),
    };
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        let scenario = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("scenario-shape"))?;
        for key in scenario.keys() {
            if !matches!(key.as_str(), "scenarioId" | "scenarioVersion" | "irDigest") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let scenario_id = string_member(scenario, "scenarioId")?.to_owned();
        if scenario_id.len() < 3 || scenario_id.len() > 191 {
            return Err(diagnostic::input_invalid("scenario-id"));
        }
        let scenario_version = SemVer::parse(string_member(scenario, "scenarioVersion")?)
            .map_err(|_| diagnostic::input_invalid("scenario-version"))?;
        let ir_digest = Sha256Digest::parse(string_member(scenario, "irDigest")?)
            .map_err(|_| diagnostic::input_invalid("scenario-digest"))?;
        parsed.push(ScenarioRef {
            scenario_id,
            scenario_version: scenario_version.as_str().to_owned(),
            ir_digest,
        });
    }
    parsed.sort();
    let before = parsed.len();
    parsed.dedup();
    if parsed.len() != before {
        return Err(diagnostic::input_invalid("duplicate-ref"));
    }
    Ok(parsed)
}

/// Parse every declared transition.
fn transitions(array: &[Json]) -> Result<Vec<Transition>, DiagnosticSet> {
    if array.len() > version::MAX_TRANSITIONS {
        return Err(diagnostic::input_invalid("transition-bound"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("transition-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "transitionId"
                    | "stateSpaceId"
                    | "fromStates"
                    | "toState"
                    | "command"
                    | "assignments"
                    | "parallel"
                    | "preconditions"
                    | "policyRef"
                    | "errorRefs"
                    | "requirementRefs"
                    | "scenarioRefs"
                    | "capabilityRefs"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let transition_id = id_member(object, "transitionId", SemanticId::parse)?;
        let state_space_id = id_member(object, "stateSpaceId", SemanticId::parse)?;
        let from_json = bounded_array(object, "fromStates", version::MAX_FROM_STATES)?;
        let mut from_states = Vec::with_capacity(from_json.len());
        for state in from_json {
            from_states.push(
                StateRef::parse(
                    state
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("member"))?,
                )
                .map_err(|_| diagnostic::input_invalid("state-ref"))?,
            );
        }
        from_states.sort();
        let before = from_states.len();
        from_states.dedup();
        if from_states.len() != before || from_states.is_empty() {
            return Err(diagnostic::input_invalid("from-states"));
        }
        let to_state = id_member(object, "toState", StateRef::parse)?;
        let command = id_member(object, "command", SemanticId::parse)?;
        let assignments =
            match optional_bounded_array(object, "assignments", version::MAX_ASSIGNMENTS)? {
                Some(entries) => {
                    let mut assignments = Vec::with_capacity(entries.len());
                    for entry in entries {
                        let assignment = entry
                            .as_object()
                            .ok_or_else(|| diagnostic::input_invalid("assignment-shape"))?;
                        for key in assignment.keys() {
                            if !matches!(key.as_str(), "field" | "value") {
                                return Err(diagnostic::input_invalid("unknown-field"));
                            }
                        }
                        let field = id_member(assignment, "field", FieldName::parse)?;
                        let value_object = assignment
                            .get("value")
                            .ok_or_else(|| diagnostic::input_invalid("assignment-value"))?
                            .as_object()
                            .ok_or_else(|| diagnostic::input_invalid("assignment-value"))?;
                        let kind = string_member(value_object, "kind")?;
                        let value = match kind {
                            "literal" => {
                                if value_object.len() != 2 {
                                    return Err(diagnostic::input_invalid("unknown-field"));
                                }
                                AssignmentValue::Literal(value_node(
                                    value_object.get("value").ok_or_else(|| {
                                        diagnostic::input_invalid("assignment-value")
                                    })?,
                                    0,
                                )?)
                            }
                            "input" => {
                                if value_object.len() != 2 {
                                    return Err(diagnostic::input_invalid("unknown-field"));
                                }
                                AssignmentValue::Input {
                                    field: id_member(value_object, "field", FieldName::parse)?,
                                }
                            }
                            "prior" => {
                                if value_object.len() != 2 {
                                    return Err(diagnostic::input_invalid("unknown-field"));
                                }
                                AssignmentValue::Prior {
                                    field: id_member(value_object, "field", FieldName::parse)?,
                                }
                            }
                            "now" => {
                                if value_object.len() != 1 {
                                    return Err(diagnostic::input_invalid("unknown-field"));
                                }
                                AssignmentValue::Now
                            }
                            "expression" => {
                                if value_object.len() != 2 {
                                    return Err(diagnostic::input_invalid("unknown-field"));
                                }
                                AssignmentValue::Expression {
                                    expression_ref: id_member(
                                        value_object,
                                        "expressionRef",
                                        ExpressionRef::parse,
                                    )?,
                                }
                            }
                            _ => return Err(diagnostic::input_invalid("assignment-kind")),
                        };
                        assignments.push(Assignment { field, value });
                    }
                    assignments
                }
                None => Vec::new(),
            };
        let parallel = match object.get("parallel") {
            None | Some(Json::Null) => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("flag"))?,
        };
        let preconditions =
            match optional_bounded_array(object, "preconditions", version::MAX_PRECONDITIONS)? {
                Some(entries) => {
                    let mut preconditions = Vec::with_capacity(entries.len());
                    for entry in entries {
                        preconditions.push(predicate_node(entry, 0)?);
                    }
                    preconditions
                }
                None => Vec::new(),
            };
        let policy_ref = match object.get("policyRef") {
            None | Some(Json::Null) => None,
            Some(_) => Some(id_member(object, "policyRef", NamespacedId::parse)?),
        };
        let error_refs = error_refs_of(object, "errorRefs")?;
        let requirement_refs = ref_list(
            object,
            "requirementRefs",
            version::MAX_REQUIREMENT_REFS,
            NamespacedId::parse,
        )?;
        let scenario_refs = scenario_refs(object)?;
        let capability_refs = ref_list(
            object,
            "capabilityRefs",
            version::MAX_CAPABILITY_REFS,
            NamespacedId::parse,
        )?;
        parsed.push(Transition {
            transition_id,
            state_space_id,
            from_states,
            to_state,
            command,
            assignments,
            parallel,
            preconditions,
            policy_ref,
            error_refs,
            requirement_refs,
            scenario_refs,
            capability_refs,
        });
    }
    Ok(parsed)
}

/// Parse every declared verification mapping.
fn verification_mappings(array: &[Json]) -> Result<Vec<VerificationMapping>, DiagnosticSet> {
    if array.len() > version::MAX_MAPPINGS {
        return Err(diagnostic::input_invalid("mapping-bound"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("mapping-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "mappingId"
                    | "subject"
                    | "relation"
                    | "targetRef"
                    | "enforcementLayer"
                    | "evidence"
                    | "status"
                    | "confidence"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let mapping_id = id_member(object, "mappingId", SemanticId::parse)?;
        let subject = subject_member(
            object
                .get("subject")
                .ok_or_else(|| diagnostic::input_invalid("subject"))?,
            true,
        )?;
        let relation = MappingRelation::from_key(string_member(object, "relation")?)
            .ok_or_else(|| diagnostic::input_invalid("relation"))?;
        let target = target_ref(
            object
                .get("targetRef")
                .ok_or_else(|| diagnostic::input_invalid("target-ref"))?,
        )?;
        let enforcement_layer =
            EnforcementLayer::from_key(string_member(object, "enforcementLayer")?)
                .ok_or_else(|| diagnostic::input_invalid("enforcement-layer"))?;
        let evidence = match object.get("evidence") {
            None | Some(Json::Null) => None,
            Some(value) => Some(evidence_record(value)?),
        };
        let status = MappingStatus::from_key(string_member(object, "status")?)
            .ok_or_else(|| diagnostic::input_invalid("mapping-status"))?;
        let confidence = MappingConfidence::from_key(string_member(object, "confidence")?)
            .ok_or_else(|| diagnostic::input_invalid("confidence"))?;
        parsed.push(VerificationMapping {
            mapping_id,
            subject,
            relation,
            target_ref: target,
            enforcement_layer,
            evidence,
            status,
            confidence,
        });
    }
    Ok(parsed)
}

/// Parse one typed subject pointer.
fn subject_member(json: &Json, state_space_allowed: bool) -> Result<Subject, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("subject-shape"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "kind" | "id") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let kind = SubjectKind::from_key(string_member(object, "kind")?)
        .filter(|kind| state_space_allowed || *kind != SubjectKind::StateSpace)
        .ok_or_else(|| diagnostic::input_invalid("subject-kind"))?;
    let id = id_member(object, "id", SemanticId::parse)?;
    Ok(Subject { kind, id })
}

/// Parse one typed target reference.
fn target_ref(json: &Json) -> Result<TargetRef, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("target-shape"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "targetKind" | "targetId") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let target_kind = TargetKind::from_key(string_member(object, "targetKind")?)
        .ok_or_else(|| diagnostic::input_invalid("target-kind"))?;
    let target_id = string_member(object, "targetId")?.to_owned();
    let bytes = target_id.as_bytes();
    let valid = bytes.len() >= 3
        && bytes.len() <= 128
        && bytes[0].is_ascii_lowercase()
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-' | b'/')
        })
        && !target_id.contains("..");
    if !valid {
        return Err(diagnostic::input_invalid("target-id"));
    }
    Ok(TargetRef {
        target_kind,
        target_id,
    })
}

/// Parse one typed evidence record.
fn evidence_record(json: &Json) -> Result<Evidence, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("evidence-shape"))?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "profileRef"
                | "adapterRef"
                | "protocolRef"
                | "executableDigest"
                | "sourceRevision"
                | "scenarioRef"
                | "testRef"
                | "gateRef"
                | "resultStatus"
                | "evidenceDigest"
        ) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let namespaced = |key: &str| -> Result<Option<NamespacedId>, DiagnosticSet> {
        match object.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(_) => Ok(Some(id_member(object, key, NamespacedId::parse)?)),
        }
    };
    let profile_ref = namespaced("profileRef")?;
    let adapter_ref = namespaced("adapterRef")?;
    let protocol_ref = namespaced("protocolRef")?;
    let executable_digest = Sha256Digest::parse(string_member(object, "executableDigest")?)
        .map_err(|_| diagnostic::input_invalid("evidence-digest"))?;
    let source_revision = SemVer::parse(string_member(object, "sourceRevision")?)
        .map_err(|_| diagnostic::input_invalid("source-revision"))?;
    let scenario_ref = match object.get("scenarioRef") {
        None | Some(Json::Null) => None,
        Some(value) => {
            let scenario_object = value
                .as_object()
                .ok_or_else(|| diagnostic::input_invalid("scenario-shape"))?;
            let scenario_id = string_member(scenario_object, "scenarioId")?.to_owned();
            if scenario_id.len() < 3 || scenario_id.len() > 191 {
                return Err(diagnostic::input_invalid("scenario-id"));
            }
            let scenario_version =
                SemVer::parse(string_member(scenario_object, "scenarioVersion")?)
                    .map_err(|_| diagnostic::input_invalid("scenario-version"))?;
            let ir_digest = Sha256Digest::parse(string_member(scenario_object, "irDigest")?)
                .map_err(|_| diagnostic::input_invalid("scenario-digest"))?;
            Some(ScenarioRef {
                scenario_id,
                scenario_version: scenario_version.as_str().to_owned(),
                ir_digest,
            })
        }
    };
    let test_ref = namespaced("testRef")?;
    let gate_ref = namespaced("gateRef")?;
    let result_status = EvidenceStatus::from_key(string_member(object, "resultStatus")?)
        .ok_or_else(|| diagnostic::input_invalid("result-status"))?;
    let evidence_digest = Sha256Digest::parse(string_member(object, "evidenceDigest")?)
        .map_err(|_| diagnostic::input_invalid("evidence-digest"))?;
    Ok(Evidence {
        profile_ref,
        adapter_ref,
        protocol_ref,
        executable_digest,
        source_revision: source_revision.as_str().to_owned(),
        scenario_ref,
        test_ref,
        gate_ref,
        result_status,
        evidence_digest,
    })
}

/// Parse every declared property hint.
fn property_hints(array: &[Json]) -> Result<Vec<PropertyHint>, DiagnosticSet> {
    if array.len() > version::MAX_PROPERTY_HINTS {
        return Err(diagnostic::input_invalid("hint-bound"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("hint-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "hintId" | "subject" | "property" | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let hint_id = id_member(object, "hintId", SemanticId::parse)?;
        let subject = subject_member(
            object
                .get("subject")
                .ok_or_else(|| diagnostic::input_invalid("subject"))?,
            true,
        )?;
        let property = PropertyKind::from_key(string_member(object, "property")?)
            .ok_or_else(|| diagnostic::input_invalid("property"))?;
        let description = optional_bounded_string(object, "description")?;
        parsed.push(PropertyHint {
            hint_id,
            subject,
            property,
            description,
        });
    }
    Ok(parsed)
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
    let day_bytes = text.as_bytes();
    let day_ok = matches!(
        (day_bytes[8], day_bytes[9]),
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
