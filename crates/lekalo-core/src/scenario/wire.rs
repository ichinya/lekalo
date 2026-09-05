//! Wire normalization and scenario-local data-flow validation
//! (issue #23).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`ScenarioIr`](super::ScenarioIr). It fails closed before semantic
//! processing: unknown fields, wrong identities, malformed identifiers,
//! digest bindings, bounds, and shape violations each return one typed
//! diagnostic and no partial IR. The scenario-local data-flow pass then
//! rejects duplicate step identities, dangling outputs, forward
//! step-output references, unobservable assertions, conflicting state
//! setup, idempotency assertions without a key or replay reference, and
//! actor or control references with no established source. Generic
//! symbol resolution and visibility stay with #12, error-union
//! membership with #62, effect meaning with #14, and authorization
//! semantics with #25.

use std::collections::HashMap;

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};

use super::action::InvokeAction;
use super::assertion::Assertion;
use super::binding::Binding;
use super::canonical;
use super::diagnostic;
use super::id::{NamespacedId, SemanticId, Tag};
use super::precondition::Precondition;
use super::reference::{Ref, RefKind, RefTarget};
use super::source_map::SourceMapRef;
use super::step::{
    given_from_json, then_from_json, when_from_json, GivenStep, ThenStep, ValueOrRef, WhenStep,
};
use super::version;
use super::{IrRef, MetadataValue, ModelPin, ModelRef, ScenarioIr};

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "scenarioId",
    "scenarioVersion",
    "summary",
    "irRef",
    "modelRef",
    "given",
    "when",
    "then",
    "bindings",
    "tags",
    "metadata",
    "sourceMapRef",
];

/// The required top-level members (`sourceMapRef` is optional).
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "scenarioId",
    "scenarioVersion",
    "summary",
    "irRef",
    "modelRef",
    "given",
    "when",
    "then",
    "bindings",
    "tags",
    "metadata",
];

/// Normalize one wire document into a validated Scenario IR, or return
/// the typed rejection set with no partial IR.
pub(crate) fn from_value(json: &Json) -> Result<ScenarioIr, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("top-level-shape", None))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field", Some(key)));
        }
    }
    for required in REQUIRED_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::input_invalid("missing-field", Some(required)));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::input_invalid("schema-version", None));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
        return Err(diagnostic::input_invalid("contract-identity", None));
    }
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("project-id", None))?,
    )
    .map_err(|_| diagnostic::input_invalid("project-id", None))?;
    let scenario_id = semantic_member(object.get("scenarioId"), "scenario-id")?;
    let scenario_version = SemVer::parse(
        object
            .get("scenarioVersion")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("scenario-version", None))?,
    )
    .map_err(|_| diagnostic::input_invalid("scenario-version", None))?;
    let summary = object
        .get("summary")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("summary", None))?;
    let summary_characters = summary.chars().count();
    if summary_characters == 0 || summary_characters > 1024 {
        return Err(diagnostic::limit_set("summary-length"));
    }
    let ir_ref = ir_ref_field(object.get("irRef"))?;
    let model_ref = model_ref_field(object.get("modelRef"))?;
    let source_map = match object.get("sourceMapRef") {
        None => None,
        Some(value) => Some(SourceMapRef::from_json(value, "sourceMapRef")?),
    };
    let tags = tags_field(object.get("tags"))?;
    let metadata = metadata_field(object.get("metadata"))?;
    let given: Vec<GivenStep> = steps_field(
        object.get("given"),
        version::MAX_GIVEN_STEPS,
        given_from_json,
        "given",
    )?;
    let when: Vec<WhenStep> = steps_field(
        object.get("when"),
        version::MAX_WHEN_STEPS,
        when_from_json,
        "when",
    )?;
    let then: Vec<ThenStep> = steps_field(
        object.get("then"),
        version::MAX_THEN_STEPS,
        then_from_json,
        "then",
    )?;
    if given.len() + when.len() + then.len() > version::MAX_TOTAL_STEPS {
        return Err(diagnostic::limit_set("total-steps"));
    }
    if when.is_empty() || then.is_empty() {
        return Err(diagnostic::input_invalid("empty-scenario", None));
    }
    let bindings = bindings_field(object.get("bindings"))?;
    validate_id_uniqueness(&given, &when, &then)?;
    validate_data_flow(&given, &when, &then)?;
    let coverage = super::coverage::build(&scenario_id, &when, &then);
    Ok(ScenarioIr {
        project_id,
        scenario_id,
        scenario_version,
        summary: summary.to_owned(),
        ir_ref,
        model_ref,
        given,
        when,
        then,
        bindings,
        tags,
        metadata,
        source_map,
        coverage,
    })
}

/// Parse one required semantic identifier member.
fn semantic_member(json: Option<&Json>, detail: &'static str) -> Result<SemanticId, DiagnosticSet> {
    SemanticId::parse(
        json.and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid(detail, None))?,
    )
    .map_err(|_| diagnostic::input_invalid(detail, None))
}

/// Parse the exact IR contract reference.
fn ir_ref_field(json: Option<&Json>) -> Result<IrRef, DiagnosticSet> {
    let object = json
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("ir-ref-shape", None))?;
    if object
        .keys()
        .map(String::as_str)
        .collect::<Vec<&str>>()
        .as_slice()
        != ["digest", "identity"]
    {
        return Err(diagnostic::input_invalid("ir-ref-shape", None));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IR_IDENTITY) {
        return Err(diagnostic::input_invalid("ir-ref-identity", None));
    }
    Ok(IrRef {
        digest: digest_member(object.get("digest"))?,
    })
}

/// Parse the exact source Model contract reference.
fn model_ref_field(json: Option<&Json>) -> Result<ModelRef, DiagnosticSet> {
    let object = json
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("model-ref-shape", None))?;
    if object
        .keys()
        .map(String::as_str)
        .collect::<Vec<&str>>()
        .as_slice()
        != ["digest", "modelVersion"]
    {
        return Err(diagnostic::input_invalid("model-ref-shape", None));
    }
    let version = match object.get("modelVersion").and_then(Json::as_str) {
        Some("0.1.0") => ModelPin::V0_1_0,
        Some("1.0.0") => ModelPin::V1_0_0,
        _ => return Err(diagnostic::input_invalid("model-ref-version", None)),
    };
    Ok(ModelRef {
        version,
        digest: digest_member(object.get("digest"))?,
    })
}

/// Parse one required digest member.
fn digest_member(json: Option<&Json>) -> Result<Sha256Digest, DiagnosticSet> {
    Sha256Digest::parse(
        json.and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("digest", None))?,
    )
    .map_err(|_| diagnostic::input_invalid("digest", None))
}

/// Parse the sorted set-like tags.
fn tags_field(json: Option<&Json>) -> Result<Vec<Tag>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("tags-shape", None))?;
    if items.len() > version::MAX_TAGS {
        return Err(diagnostic::limit_set("tags"));
    }
    let mut tags = Vec::with_capacity(items.len());
    for item in items {
        let text = item
            .as_str()
            .ok_or_else(|| diagnostic::input_invalid("tag-shape", None))?;
        tags.push(
            Tag::parse(text).map_err(|_| diagnostic::input_invalid("tag-shape", Some(text)))?,
        );
    }
    if super::precondition::has_duplicates(&tags) {
        return Err(diagnostic::input_invalid("duplicate-tag", None));
    }
    tags.sort();
    Ok(tags)
}

/// Parse the sorted namespaced metadata map.
fn metadata_field(json: Option<&Json>) -> Result<Vec<(String, MetadataValue)>, DiagnosticSet> {
    let map = json
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("metadata-shape", None))?;
    if map.len() > version::MAX_METADATA_ENTRIES {
        return Err(diagnostic::limit_set("metadata-entries"));
    }
    let mut entries = Vec::with_capacity(map.len());
    for (key, value) in map {
        if !metadata_key_valid(key) {
            return Err(diagnostic::input_invalid("metadata-key", Some(key)));
        }
        let parsed = match value {
            Json::Null => MetadataValue::Null,
            Json::Bool(flag) => MetadataValue::Boolean(*flag),
            number if number.is_i64() => MetadataValue::Integer(number.as_i64().expect("checked")),
            Json::String(text) => {
                let characters = text.chars().count();
                if characters == 0 || characters > 256 || text.chars().any(char::is_control) {
                    return Err(diagnostic::limit_set("metadata-value"));
                }
                MetadataValue::String(text.clone())
            }
            _ => return Err(diagnostic::input_invalid("metadata-value", Some(key))),
        };
        entries.push((key.clone(), parsed));
    }
    // serde_json maps iterate in sorted key order; the canonical byte
    // bound is enforced on the exact serialized form.
    if canonical::metadata_bytes(&entries).len() > version::MAX_METADATA_BYTES {
        return Err(diagnostic::limit_set("metadata-bytes"));
    }
    Ok(entries)
}

/// The namespaced metadata key grammar (dotted lowercase kebab).
fn metadata_key_valid(text: &str) -> bool {
    if text.is_empty() || text.len() > 129 {
        return false;
    }
    let segments: Vec<&str> = text.split('.').collect();
    segments.len() >= 2
        && segments.iter().all(|segment| {
            let bytes = segment.as_bytes();
            !bytes.is_empty()
                && bytes.len() <= 64
                && bytes[0].is_ascii_lowercase()
                && bytes[1..]
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        })
}

/// Parse one bounded step array with its closed member parser.
fn steps_field<T>(
    json: Option<&Json>,
    bound: usize,
    parse: fn(&Json, &str) -> Result<T, DiagnosticSet>,
    role: &'static str,
) -> Result<Vec<T>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("steps-shape", Some(role)))?;
    if items.len() > bound {
        return Err(diagnostic::limit_set("steps-per-role"));
    }
    let mut steps = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let step_role = format!("{role}[{index}]");
        steps.push(parse(item, &step_role)?);
    }
    Ok(steps)
}

/// Parse the sorted set-like bindings.
fn bindings_field(json: Option<&Json>) -> Result<Vec<Binding>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("bindings-shape", None))?;
    if items.len() > version::MAX_BINDINGS {
        return Err(diagnostic::limit_set("bindings"));
    }
    let mut bindings = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        bindings.push(Binding::from_json(item, &format!("bindings[{index}]"))?);
    }
    bindings.sort_by_key(canonical::binding_sort_key);
    let keys: Vec<String> = bindings.iter().map(canonical::binding_sort_key).collect();
    if keys.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(diagnostic::input_invalid("duplicate-binding", None));
    }
    Ok(bindings)
}

/// Reject duplicate step identities across the whole scenario; step
/// identity is `scenario_id + step_id`, never array position.
fn validate_id_uniqueness(
    given: &[GivenStep],
    when: &[WhenStep],
    then: &[ThenStep],
) -> Result<(), DiagnosticSet> {
    let mut seen: Vec<&str> = Vec::new();
    for step in given {
        seen.push(step.step_id.as_str());
    }
    for step in when {
        seen.push(step.step_id.as_str());
    }
    for step in then {
        seen.push(step.step_id.as_str());
    }
    seen.sort_unstable();
    for pair in seen.windows(2) {
        if pair[0] == pair[1] {
            return Err(diagnostic::input_invalid(
                "duplicate-step-id",
                Some(pair[0]),
            ));
        }
    }
    Ok(())
}

/// The scenario-local data-flow validation over validated steps.
fn validate_data_flow(
    given: &[GivenStep],
    when: &[WhenStep],
    then: &[ThenStep],
) -> Result<(), DiagnosticSet> {
    let mut given_index: HashMap<&str, &Precondition> = HashMap::new();
    for step in given {
        given_index.insert(step.step_id.as_str(), &step.precondition);
    }
    let mut when_index: HashMap<&str, usize> = HashMap::new();
    for (index, step) in when.iter().enumerate() {
        when_index.insert(step.step_id.as_str(), index);
    }
    let declared_actors: Vec<&NamespacedId> = given
        .iter()
        .filter_map(|step| step.precondition.actor_id())
        .collect();

    // Given steps consumed by some action (step reference or actor
    // identity match), used by the then-reachability rule.
    let mut referenced_given: Vec<String> = Vec::new();
    for (position, step) in when.iter().enumerate() {
        check_action_refs(
            &step.action,
            &given_index,
            &when_index,
            position,
            &declared_actors,
            &mut referenced_given,
        )?;
        if let Some(replay) = &step.replay {
            let prior =
                matches!(when_index.get(replay.of.as_str()), Some(target) if *target < position);
            if !prior {
                return Err(diagnostic::input_invalid(
                    "replay-not-prior-action",
                    Some(step.step_id.as_str()),
                ));
            }
        }
        enforce_step_ref_bound(step_ref_count_when(step))?;
    }
    for step in given {
        enforce_step_ref_bound(step_ref_count_given(step))?;
    }

    for step in then {
        enforce_step_ref_bound(step_ref_count_then(step))?;
        // The observation must trace to an action: either a when step,
        // or a given step some action consumed.
        if !when_index.contains_key(step.observes.as_str()) {
            let consumed = given_index.contains_key(step.observes.as_str())
                && referenced_given
                    .iter()
                    .any(|name| name == step.observes.as_str());
            if !consumed {
                let dangling = !given_index.contains_key(step.observes.as_str());
                let detail = if dangling {
                    "observes-dangling"
                } else {
                    "then-unreachable"
                };
                return Err(diagnostic::input_invalid(
                    detail,
                    Some(step.observes.as_str()),
                ));
            }
        }
        if let Assertion::Idempotency { replay, .. } = &step.assertion {
            match when_index.get(replay.as_str()) {
                Some(target) => {
                    let target_step = &when[*target];
                    if target_step.action.idempotency_key.is_none() && target_step.replay.is_none()
                    {
                        return Err(diagnostic::input_invalid(
                            "idempotency-without-control",
                            Some(step.step_id.as_str()),
                        ));
                    }
                }
                None => {
                    return Err(diagnostic::input_invalid(
                        "idempotency-not-action",
                        Some(step.step_id.as_str()),
                    ))
                }
            }
        }
        if let Assertion::DeterministicFixture {
            clock, id_source, ..
        } = &step.assertion
        {
            check_control_ref(clock.as_ref(), &given_index)?;
            check_control_ref(id_source.as_ref(), &given_index)?;
        }
    }
    reject_conflicting_setup(given)
}

/// Validate one action's typed references against the step index.
fn check_action_refs(
    action: &InvokeAction,
    given_index: &HashMap<&str, &Precondition>,
    when_index: &HashMap<&str, usize>,
    position: usize,
    declared_actors: &[&NamespacedId],
    referenced_given: &mut Vec<String>,
) -> Result<(), DiagnosticSet> {
    if let Some(actor) = &action.actor {
        require_declared_actor(actor, declared_actors)?;
    }
    if let Some(clock) = &action.clock {
        check_control_ref(Some(clock), given_index)?;
    }
    let mut leaves: Vec<&ValueOrRef> = action.input.iter().map(|(_, leaf)| leaf).collect();
    if let Some(key) = &action.idempotency_key {
        leaves.push(key);
    }
    for leaf in leaves {
        check_leaf_ref(leaf, given_index, when_index, position, referenced_given)?;
    }
    Ok(())
}

/// Validate one leaf reference for step-local kinds.
fn check_leaf_ref(
    leaf: &ValueOrRef,
    given_index: &HashMap<&str, &Precondition>,
    when_index: &HashMap<&str, usize>,
    position: usize,
    referenced_given: &mut Vec<String>,
) -> Result<(), DiagnosticSet> {
    let ValueOrRef::Reference(reference) = leaf else {
        return Ok(());
    };
    match reference.kind {
        RefKind::GivenValue => require_given(reference, given_index, referenced_given)?,
        RefKind::StepOutput => {
            let RefTarget::Step(step) = &reference.id else {
                return Err(diagnostic::input_invalid("ref-step-id", None));
            };
            let prior = matches!(when_index.get(step.as_str()), Some(target) if *target < position);
            if !prior {
                return Err(diagnostic::input_invalid(
                    "output-not-prior-action",
                    Some(step.as_str()),
                ));
            }
        }
        RefKind::Clock | RefKind::IdSource => check_control_ref(Some(reference), given_index)?,
        _ => {}
    }
    Ok(())
}

/// Require that a given-value reference targets an existing given step.
fn require_given(
    reference: &Ref,
    given_index: &HashMap<&str, &Precondition>,
    referenced_given: &mut Vec<String>,
) -> Result<(), DiagnosticSet> {
    let RefTarget::Step(step) = &reference.id else {
        return Err(diagnostic::input_invalid("ref-step-id", None));
    };
    if !given_index.contains_key(step.as_str()) {
        return Err(diagnostic::input_invalid(
            "given-dangling",
            Some(step.as_str()),
        ));
    }
    referenced_given.push(step.as_str().to_owned());
    Ok(())
}

/// Require that a clock or id-source reference targets an established
/// given step of the matching kind.
fn check_control_ref(
    reference: Option<&Ref>,
    given_index: &HashMap<&str, &Precondition>,
) -> Result<(), DiagnosticSet> {
    let Some(reference) = reference else {
        return Ok(());
    };
    let RefTarget::Step(step) = &reference.id else {
        return Err(diagnostic::input_invalid("ref-step-id", None));
    };
    let Some(precondition) = given_index.get(step.as_str()) else {
        return Err(diagnostic::input_invalid(
            "control-dangling",
            Some(step.as_str()),
        ));
    };
    let established = match reference.kind {
        RefKind::Clock => precondition.is_clock(),
        RefKind::IdSource => precondition.is_id_source(),
        _ => false,
    };
    if !established {
        return Err(diagnostic::input_invalid(
            "control-kind",
            Some(step.as_str()),
        ));
    }
    Ok(())
}

/// Require that an actor reference names an actor declared by a given
/// step of the same scenario.
fn require_declared_actor(
    reference: &Ref,
    declared_actors: &[&NamespacedId],
) -> Result<(), DiagnosticSet> {
    let declared = declared_actors.iter().any(|actor| match &reference.id {
        RefTarget::Namespaced(id) => id.as_str() == actor.as_str(),
        RefTarget::Semantic(id) => id.as_str() == actor.as_str(),
        RefTarget::Step(_) => false,
    });
    if !declared {
        return Err(diagnostic::input_invalid("actor-undeclared", None));
    }
    Ok(())
}

/// Reject two state setups that claim the same entity row.
fn reject_conflicting_setup(given: &[GivenStep]) -> Result<(), DiagnosticSet> {
    // Group the state setups by the exact row they claim (entity plus
    // selector terms with their pinned values); two steps claiming the
    // same row and touching at least one shared field conflict — either
    // a duplicate or a contradictory setup.
    let mut rows: HashMap<String, Vec<Vec<&str>>> = HashMap::new();
    for step in given {
        if let Some((entity, terms, names)) = step.precondition.state_setup() {
            let row = format!("{}|{}", entity.as_str(), terms.join(","));
            rows.entry(row).or_default().push(names);
        }
    }
    let overlapping = rows.values().any(|setups| {
        (0..setups.len()).any(|left| {
            ((left + 1)..setups.len())
                .any(|right| setups[left].iter().any(|name| setups[right].contains(name)))
        })
    });
    if overlapping {
        return Err(diagnostic::input_invalid("conflicting-setup", None));
    }
    Ok(())
}

/// Enforce the per-step typed value bound.
fn enforce_step_ref_bound(count: usize) -> Result<(), DiagnosticSet> {
    if count > version::MAX_REFS_PER_STEP {
        return Err(diagnostic::limit_set("step-refs"));
    }
    Ok(())
}

/// The typed value volume of one given step.
fn step_ref_count_given(step: &GivenStep) -> usize {
    match &step.precondition {
        Precondition::State {
            selector, fields, ..
        } => {
            selector
                .iter()
                .map(|term| term.equals.count())
                .sum::<usize>()
                + fields.iter().map(|(_, leaf)| leaf.count()).sum::<usize>()
        }
        _ => 0,
    }
}

/// The typed value volume of one when step.
fn step_ref_count_when(step: &WhenStep) -> usize {
    step.action
        .input
        .iter()
        .map(|(_, leaf)| leaf.count())
        .sum::<usize>()
        + step
            .action
            .idempotency_key
            .as_ref()
            .map(ValueOrRef::count)
            .unwrap_or(0)
}

/// The typed value volume of one then step.
fn step_ref_count_then(step: &ThenStep) -> usize {
    match &step.assertion {
        Assertion::Result { value, .. } => value.as_ref().map(ValueOrRef::count).unwrap_or(0),
        Assertion::Error { payload, .. } => {
            payload.iter().map(|(_, leaf)| leaf.count()).sum::<usize>()
        }
        Assertion::EntityState {
            selector, fields, ..
        } => {
            selector
                .iter()
                .map(|term| term.equals.count())
                .sum::<usize>()
                + fields
                    .iter()
                    .map(|(_, expectation)| match expectation {
                        super::assertion::FieldExpectation::Value(leaf) => leaf.count(),
                        super::assertion::FieldExpectation::Match(_) => 1,
                    })
                    .sum::<usize>()
        }
        _ => 0,
    }
}
