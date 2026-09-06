//! Wire normalization and semantic validation of the transaction-
//! concurrency attachment (issue #24).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`TransactionConcurrencyAttachment`](super::TransactionConcurrencyAttachment).
//! It fails closed before semantic processing: unknown or missing fields,
//! wrong identities, malformed identifiers, digests, bounds, duplicate
//! identities, foreign effect references, external effects inside local
//! groups, unacknowledged partial failure, contradictory idempotency or
//! retry declarations, missing capability records, and invalid race-case
//! schedules each return one typed registered diagnostic and no partial
//! attachment.

use crate::diagnostics::DiagnosticSet;
use crate::effects::{FieldName, OperationId, TransactionGroupId};
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{NamespacedId, SemanticId, StepId};
use serde_json::Value as Json;

use super::diagnostic;
use super::effect_group::{
    AtomicEffectGroup, BoundaryKind, BoundarySubject, CommitBoundary, FailureBoundary,
    OperationContract, TransactionMode,
};
use super::identity::{EffectRef, ErrorRef, EtagRef, OrderKey, RefError, ResourceRef};
use super::isolation::IsolationLevel;
use super::lock::{LockAcquisition, LockMode, LockScope, LockTimeout};
use super::operation::{
    DuplicatePolicy, Idempotency, IdempotencyMode, IdempotencyRecord, Retry, RetryCondition,
    RetryPhase, RetrySafety,
};
use super::precondition::{
    CapabilityId, CapabilityRequirement, Enforcement, EtagPrecondition, Invariant,
    InvariantPredicate, Literal, LockRequirement, Precondition, RequirementLevel, TokenSupply,
    VersionPrecondition,
};
use super::scenario::{
    Barrier, ConcurrencyCase, ExpectedOutcome, Invocation, OutcomeExpectation, Participant,
    ScenarioRef, ScheduleNode,
};
use super::version;
use super::{ModelPin, TransactionConcurrencyAttachment};

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "modelRef",
    "irRef",
    "effectGraphRef",
    "operations",
    "invariants",
    "concurrencyCases",
    "capabilityRequirements",
];

/// The required top-level members (`effectGraphRef` is optional).
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "modelRef",
    "irRef",
    "operations",
    "invariants",
    "concurrencyCases",
    "capabilityRequirements",
];

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial attachment. Pure: no source,
/// model, cache, report, network, process, or target access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<TransactionConcurrencyAttachment, DiagnosticSet> {
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
    let model_ref = model_pin(
        object
            .get("modelRef")
            .ok_or_else(|| diagnostic::input_invalid("model-ref", None))?,
    )?;
    let ir_ref = digest_member(
        object
            .get("irRef")
            .ok_or_else(|| diagnostic::input_invalid("ir-ref", None))?,
        "dev.lekalo.ir@0.1.0",
        "ir-ref",
    )?;
    let effect_graph = match object.get("effectGraphRef") {
        Some(value) => Some(digest_member(
            value,
            "dev.lekalo.effects@1.0.0",
            "effect-graph-ref",
        )?),
        None => None,
    };
    let capability_requirements = capability_requirements(
        object
            .get("capabilityRequirements")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("requirement-list", None))?,
    )?;
    let operations = operations(
        object
            .get("operations")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("operation-list", None))?,
        &capability_requirements,
    )?;
    let invariants = invariants(
        object
            .get("invariants")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("invariant-list", None))?,
    )?;
    let cases = concurrency_cases(
        object
            .get("concurrencyCases")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("case-list", None))?,
        &invariants,
        &capability_requirements,
    )?;
    let attachment = TransactionConcurrencyAttachment::assemble(
        project_id,
        model_ref,
        ir_ref,
        effect_graph,
        operations,
        invariants,
        cases,
        capability_requirements,
    );
    attachment.semantic_self_check()?;
    Ok(attachment)
}

/// Parse the bound source Model pin.
fn model_pin(json: &Json) -> Result<ModelPin, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("model-ref", None))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "modelVersion" | "digest") {
            return Err(diagnostic::input_invalid("unknown-field", Some(key)));
        }
    }
    let version = object
        .get("modelVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("model-version", None))?;
    let pin = match version {
        "0.1.0" => crate::scenario::ModelPin::V0_1_0,
        "1.0.0" => crate::scenario::ModelPin::V1_0_0,
        _ => return Err(diagnostic::input_invalid("model-version", Some(version))),
    };
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("model-digest", None))?,
    )
    .map_err(|_| diagnostic::input_invalid("model-digest", None))?;
    Ok(ModelPin {
        version: pin,
        digest,
    })
}

/// Parse one `{identity, digest}` contract reference.
fn digest_member(
    json: &Json,
    identity: &str,
    tag: &'static str,
) -> Result<Sha256Digest, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid(tag, None))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "identity" | "digest") {
            return Err(diagnostic::input_invalid("unknown-field", Some(key)));
        }
    }
    if object.get("identity").and_then(Json::as_str) != Some(identity) {
        return Err(diagnostic::input_invalid("contract-identity", Some(tag)));
    }
    Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("digest", Some(tag)))?,
    )
    .map_err(|_| diagnostic::input_invalid("digest", Some(tag)))
}

/// The wire names of one precondition member set.
fn closed_keys(object: &Json, allowed: &[&str], tag: &'static str) -> Result<(), DiagnosticSet> {
    let object = object
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid(tag, None))?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field", Some(key)));
        }
    }
    Ok(())
}

/// One bounded string member.
fn string_member<'a>(
    object: &'a Json,
    key: &str,
    tag: &'static str,
) -> Result<&'a str, DiagnosticSet> {
    object
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid(tag, Some(key)))
}

/// Map an identity-layer rejection to its registered rule.
fn ref_error(tag: &'static str, error: RefError) -> DiagnosticSet {
    match error {
        RefError::Kind => diagnostic::input_invalid("effect-kind", Some(tag)),
        RefError::Subject => diagnostic::input_invalid("effect-subject", Some(tag)),
        RefError::Occurrence => diagnostic::input_invalid("effect-occurrence", Some(tag)),
        RefError::Shape => diagnostic::input_invalid(tag, None),
    }
}

/// Parse the capability-requirement records.
fn capability_requirements(items: &[Json]) -> Result<Vec<CapabilityRequirement>, DiagnosticSet> {
    if items.len() > version::MAX_CAPABILITY_REQUIREMENTS {
        return Err(diagnostic::input_invalid("requirement-limit", None));
    }
    let mut records = Vec::with_capacity(items.len());
    for item in items {
        closed_keys(
            item,
            &["requirementId", "capability", "minimum", "reason"],
            "requirement",
        )?;
        let requirement_id =
            NamespacedId::parse(string_member(item, "requirementId", "requirement-id")?)
                .map_err(|_| diagnostic::input_invalid("requirement-id", None))?;
        let capability =
            CapabilityId::parse(string_member(item, "capability", "requirement-capability")?)
                .ok_or_else(|| {
                    diagnostic::rule_invalid(
                        diagnostic::CAPABILITY_MISSING,
                        "unknown-capability",
                        None,
                    )
                })?;
        let minimum =
            RequirementLevel::from_key(string_member(item, "minimum", "requirement-minimum")?)
                .ok_or_else(|| diagnostic::input_invalid("requirement-minimum", None))?;
        let reason = string_member(item, "reason", "requirement-reason")?;
        let characters = reason.chars().count();
        if characters == 0 || characters > 256 {
            return Err(diagnostic::input_invalid("requirement-reason", None));
        }
        records.push(CapabilityRequirement {
            requirement_id,
            capability,
            minimum,
            reason: reason.to_owned(),
        });
    }
    Ok(records)
}

/// Parse every operation contract.
fn operations(
    items: &[Json],
    requirements: &[CapabilityRequirement],
) -> Result<Vec<OperationContract>, DiagnosticSet> {
    if items.is_empty() || items.len() > version::MAX_OPERATIONS {
        return Err(diagnostic::input_invalid("operation-limit", None));
    }
    let mut operations = Vec::with_capacity(items.len());
    for item in items {
        closed_keys(
            item,
            &[
                "operationRef",
                "operationVersion",
                "transaction",
                "isolationRequirement",
                "atomicEffectGroups",
                "preconditions",
                "failureBoundaries",
                "idempotency",
                "retry",
                "capabilityRequirements",
            ],
            "operation",
        )?;
        let operation_ref =
            OperationId::from_qualified(string_member(item, "operationRef", "operation-ref")?)
                .ok_or_else(|| diagnostic::input_invalid("operation-ref", None))?;
        let operation_version = SemVer::parse(string_member(
            item,
            "operationVersion",
            "operation-version",
        )?)
        .map_err(|_| diagnostic::input_invalid("operation-version", None))?;
        let transaction =
            TransactionMode::from_key(string_member(item, "transaction", "transaction-mode")?)
                .ok_or_else(|| diagnostic::input_invalid("transaction-mode", None))?;
        let isolation = IsolationLevel::from_key(string_member(
            item,
            "isolationRequirement",
            "isolation-level",
        )?)
        .ok_or_else(|| diagnostic::input_invalid("isolation-level", None))?;
        let groups = effect_groups(
            item.get("atomicEffectGroups")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("group-list", None))?,
            &operation_ref,
        )?;
        let preconditions = preconditions(
            item.get("preconditions")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("precondition-list", None))?,
        )?;
        let failure_boundaries = failure_boundaries(
            item.get("failureBoundaries")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("boundary-list", None))?,
        )?;
        let idempotency = idempotency(
            item.get("idempotency")
                .ok_or_else(|| diagnostic::input_invalid("idempotency", None))?,
        )?;
        let retry = retry(
            item.get("retry")
                .ok_or_else(|| diagnostic::input_invalid("retry", None))?,
        )?;
        let capability_refs = requirement_refs(
            item.get("capabilityRequirements")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("capability-ref-list", None))?,
            requirements,
        )?;
        operations.push(OperationContract {
            operation_ref,
            operation_version,
            transaction,
            isolation,
            groups,
            preconditions,
            failure_boundaries,
            idempotency,
            retry,
            capability_refs,
        });
    }
    Ok(operations)
}

/// Parse the atomic effect groups of one operation.
fn effect_groups(
    items: &[Json],
    operation: &OperationId,
) -> Result<Vec<AtomicEffectGroup>, DiagnosticSet> {
    if items.len() > version::MAX_GROUPS {
        return Err(diagnostic::input_invalid("group-limit", None));
    }
    let mut groups = Vec::with_capacity(items.len());
    for item in items {
        closed_keys(
            item,
            &[
                "groupId",
                "effectRefs",
                "scope",
                "atomicity",
                "commitBoundary",
                "failurePolicy",
            ],
            "group",
        )?;
        let group_id = TransactionGroupId::new(string_member(item, "groupId", "group-id")?)
            .ok_or_else(|| diagnostic::input_invalid("group-id", None))?;
        if string_member(item, "scope", "group-scope")? != "local" {
            return Err(diagnostic::input_invalid("group-scope", None));
        }
        if string_member(item, "atomicity", "group-atomicity")? != "all_or_nothing" {
            return Err(diagnostic::input_invalid("group-atomicity", None));
        }
        let commit_boundary = CommitBoundary::from_key(string_member(
            item,
            "commitBoundary",
            "group-commit-boundary",
        )?)
        .ok_or_else(|| diagnostic::input_invalid("group-commit-boundary", None))?;
        if string_member(item, "failurePolicy", "group-failure-policy")? != "abort" {
            return Err(diagnostic::input_invalid("group-failure-policy", None));
        }
        let effect_items = item
            .get("effectRefs")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("group-effect-list", None))?;
        if effect_items.is_empty() || effect_items.len() > version::MAX_EFFECTS_PER_GROUP {
            return Err(diagnostic::input_invalid("group-effect-limit", None));
        }
        let mut effect_refs = Vec::with_capacity(effect_items.len());
        for effect in effect_items {
            let text = effect
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("effect-ref", None))?;
            let parsed = EffectRef::parse(text).map_err(|error| ref_error("effect-ref", error))?;
            if parsed.operation() != operation {
                return Err(diagnostic::rule_invalid(
                    diagnostic::GROUP_OVERLAP,
                    "foreign-effect-ref",
                    Some(text),
                ));
            }
            if parsed.is_external() {
                return Err(diagnostic::rule_invalid(
                    diagnostic::EXTERNAL_ATOMIC,
                    "external-effect-in-group",
                    Some(text),
                ));
            }
            effect_refs.push(parsed);
        }
        groups.push(AtomicEffectGroup {
            group_id,
            effect_refs,
            commit_boundary,
        });
    }
    Ok(groups)
}

/// Parse one optimistic or pessimistic precondition.
fn precondition(item: &Json) -> Result<Precondition, DiagnosticSet> {
    let kind = string_member(item, "kind", "precondition-kind")?;
    match kind {
        "version" => {
            closed_keys(
                item,
                &[
                    "kind",
                    "resourceRef",
                    "tokenFieldRef",
                    "suppliedBy",
                    "compare",
                    "check",
                    "mismatch",
                ],
                "precondition",
            )?;
            if string_member(item, "compare", "precondition-compare")? != "exact"
                || string_member(item, "check", "precondition-check")? != "at_commit"
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::PRECONDITION_INVALID,
                    "not-commit-cas",
                    None,
                ));
            }
            let resource = ResourceRef::parse(string_member(item, "resourceRef", "resource-ref")?)
                .map_err(|error| ref_error("resource-ref", error))?;
            let token_field =
                FieldName::new(string_member(item, "tokenFieldRef", "token-field-ref")?)
                    .ok_or_else(|| diagnostic::input_invalid("token-field-ref", None))?;
            let supplied_by =
                TokenSupply::from_key(string_member(item, "suppliedBy", "precondition-supply")?)
                    .ok_or_else(|| diagnostic::input_invalid("precondition-supply", None))?;
            let mismatch = ErrorRef::parse(string_member(item, "mismatch", "error-ref")?)
                .map_err(|error| ref_error("error-ref", error))?;
            Ok(Precondition::Version(VersionPrecondition {
                resource,
                token_field,
                supplied_by,
                mismatch,
            }))
        }
        "etag" => {
            closed_keys(
                item,
                &[
                    "kind",
                    "resourceRef",
                    "etagRef",
                    "suppliedBy",
                    "compare",
                    "check",
                    "mismatch",
                ],
                "precondition",
            )?;
            if string_member(item, "compare", "precondition-compare")? != "exact"
                || string_member(item, "check", "precondition-check")? != "at_commit"
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::PRECONDITION_INVALID,
                    "not-commit-cas",
                    None,
                ));
            }
            let resource = ResourceRef::parse(string_member(item, "resourceRef", "resource-ref")?)
                .map_err(|error| ref_error("resource-ref", error))?;
            let etag_ref = EtagRef::parse(string_member(item, "etagRef", "etag-ref")?)
                .map_err(|error| ref_error("etag-ref", error))?;
            let supplied_by =
                TokenSupply::from_key(string_member(item, "suppliedBy", "precondition-supply")?)
                    .ok_or_else(|| diagnostic::input_invalid("precondition-supply", None))?;
            let mismatch = ErrorRef::parse(string_member(item, "mismatch", "error-ref")?)
                .map_err(|error| ref_error("error-ref", error))?;
            Ok(Precondition::Etag(EtagPrecondition {
                resource,
                etag_ref,
                supplied_by,
                mismatch,
            }))
        }
        "lock" => {
            closed_keys(
                item,
                &[
                    "kind",
                    "resourceRef",
                    "scope",
                    "mode",
                    "acquisition",
                    "orderKey",
                    "timeoutPolicy",
                ],
                "precondition",
            )?;
            let resource = ResourceRef::parse(string_member(item, "resourceRef", "resource-ref")?)
                .map_err(|error| ref_error("resource-ref", error))?;
            let scope = LockScope::from_key(string_member(item, "scope", "lock-scope")?)
                .ok_or_else(|| diagnostic::input_invalid("lock-scope", None))?;
            let mode = LockMode::from_key(string_member(item, "mode", "lock-mode")?)
                .ok_or_else(|| diagnostic::input_invalid("lock-mode", None))?;
            let acquisition =
                LockAcquisition::from_key(string_member(item, "acquisition", "lock-acquisition")?)
                    .ok_or_else(|| diagnostic::input_invalid("lock-acquisition", None))?;
            let order_key = OrderKey::parse(string_member(item, "orderKey", "lock-order-key")?)
                .map_err(|error| ref_error("lock-order-key", error))?;
            let timeout =
                LockTimeout::from_key(string_member(item, "timeoutPolicy", "lock-timeout")?)
                    .ok_or_else(|| diagnostic::input_invalid("lock-timeout", None))?;
            Ok(Precondition::Lock(LockRequirement {
                resource,
                scope,
                mode,
                acquisition,
                order_key,
                timeout,
            }))
        }
        _ => Err(diagnostic::input_invalid("precondition-kind", Some(kind))),
    }
}

/// Parse the preconditions of one operation.
fn preconditions(items: &[Json]) -> Result<Vec<Precondition>, DiagnosticSet> {
    if items.len() > version::MAX_PRECONDITIONS {
        return Err(diagnostic::input_invalid("precondition-limit", None));
    }
    items.iter().map(precondition).collect()
}

/// Parse one failure boundary.
fn failure_boundary(item: &Json) -> Result<FailureBoundary, DiagnosticSet> {
    closed_keys(
        item,
        &[
            "boundaryId",
            "kind",
            "subjectRef",
            "compensationRef",
            "outcome",
        ],
        "boundary",
    )?;
    let boundary_id = NamespacedId::parse(string_member(item, "boundaryId", "boundary-id")?)
        .map_err(|_| diagnostic::input_invalid("boundary-id", None))?;
    let kind = BoundaryKind::from_key(string_member(item, "kind", "boundary-kind")?)
        .ok_or_else(|| diagnostic::input_invalid("boundary-kind", None))?;
    let subject_text = string_member(item, "subjectRef", "boundary-subject")?;
    let subject = match kind {
        BoundaryKind::AtomicGroup => {
            let group = TransactionGroupId::new(subject_text)
                .ok_or_else(|| diagnostic::input_invalid("boundary-subject", Some(subject_text)))?;
            BoundarySubject::Group(group)
        }
        BoundaryKind::NonAtomicEffect => BoundarySubject::Effect(
            EffectRef::parse(subject_text).map_err(|error| ref_error("effect-ref", error))?,
        ),
        BoundaryKind::Compensation => {
            let operation = OperationId::from_qualified(subject_text)
                .ok_or_else(|| diagnostic::input_invalid("boundary-subject", Some(subject_text)))?;
            BoundarySubject::Operation(operation)
        }
    };
    let compensation_ref = match item.get("compensationRef") {
        Some(value) => Some(
            OperationId::from_qualified(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("compensation-ref", None))?,
            )
            .ok_or_else(|| diagnostic::input_invalid("compensation-ref", None))?,
        ),
        None => None,
    };
    let recovery_declared = match item.get("outcome") {
        Some(value) => {
            if value.as_str() != Some("recovery_required") {
                return Err(diagnostic::input_invalid("boundary-outcome", None));
            }
            true
        }
        None => false,
    };
    if kind == BoundaryKind::Compensation && !recovery_declared {
        return Err(diagnostic::rule_invalid(
            diagnostic::RETRY_CONFLICT,
            "compensation-outcome-missing",
            Some(boundary_id.as_str()),
        ));
    }
    Ok(FailureBoundary {
        boundary_id,
        kind,
        subject,
        compensation_ref,
        recovery_declared,
    })
}

/// Parse the failure boundaries of one operation.
fn failure_boundaries(items: &[Json]) -> Result<Vec<FailureBoundary>, DiagnosticSet> {
    if items.len() > version::MAX_FAILURE_BOUNDARIES {
        return Err(diagnostic::input_invalid("boundary-limit", None));
    }
    items.iter().map(failure_boundary).collect()
}

/// Parse the separate idempotency declaration.
fn idempotency(item: &Json) -> Result<Idempotency, DiagnosticSet> {
    closed_keys(
        item,
        &[
            "mode",
            "keyRef",
            "scope",
            "record",
            "duplicate",
            "sameKeyDifferentRequest",
        ],
        "idempotency",
    )?;
    let mode = IdempotencyMode::from_key(string_member(item, "mode", "idempotency-mode")?)
        .ok_or_else(|| diagnostic::input_invalid("idempotency-mode", None))?;
    let key_field = match item.get("keyRef") {
        Some(value) => Some(
            FieldName::new(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("idempotency-key-ref", None))?,
            )
            .ok_or_else(|| diagnostic::input_invalid("idempotency-key-ref", None))?,
        ),
        None => None,
    };
    let scope = NamespacedId::parse(string_member(item, "scope", "idempotency-scope")?)
        .map_err(|_| diagnostic::input_invalid("idempotency-scope", None))?;
    let record = IdempotencyRecord::from_key(string_member(item, "record", "idempotency-record")?)
        .ok_or_else(|| diagnostic::input_invalid("idempotency-record", None))?;
    let duplicate =
        DuplicatePolicy::from_key(string_member(item, "duplicate", "idempotency-duplicate")?)
            .ok_or_else(|| diagnostic::input_invalid("idempotency-duplicate", None))?;
    if string_member(item, "sameKeyDifferentRequest", "idempotency-conflict")? != "conflict" {
        return Err(diagnostic::rule_invalid(
            diagnostic::RETRY_CONFLICT,
            "same-key-conflict-required",
            None,
        ));
    }
    Ok(Idempotency {
        mode,
        key_field,
        scope,
        record,
        duplicate,
    })
}

/// Parse the separate retry declaration.
fn retry(item: &Json) -> Result<Retry, DiagnosticSet> {
    closed_keys(item, &["safety", "condition", "phases"], "retry")?;
    let safety = RetrySafety::from_key(string_member(item, "safety", "retry-safety")?)
        .ok_or_else(|| diagnostic::input_invalid("retry-safety", None))?;
    let condition = RetryCondition::from_key(string_member(item, "condition", "retry-condition")?)
        .ok_or_else(|| diagnostic::input_invalid("retry-condition", None))?;
    let phase_items = item
        .get("phases")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("retry-phase-list", None))?;
    if phase_items.is_empty() || phase_items.len() > 3 {
        return Err(diagnostic::input_invalid("retry-phase-limit", None));
    }
    let mut phases = Vec::with_capacity(phase_items.len());
    for phase in phase_items {
        let parsed = RetryPhase::from_key(
            phase
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("retry-phase", None))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("retry-phase", None))?;
        if phases.contains(&parsed) {
            return Err(diagnostic::input_invalid("duplicate-retry-phase", None));
        }
        phases.push(parsed);
    }
    phases.sort_by_key(|phase| phase.rank());
    Ok(Retry {
        safety,
        condition,
        phases,
    })
}

/// Parse and resolve one set of capability-requirement references.
fn requirement_refs(
    items: &[Json],
    requirements: &[CapabilityRequirement],
) -> Result<Vec<NamespacedId>, DiagnosticSet> {
    if items.len() > version::MAX_CAPABILITY_REFS {
        return Err(diagnostic::input_invalid("capability-ref-limit", None));
    }
    let mut refs = Vec::with_capacity(items.len());
    for item in items {
        let text = item
            .as_str()
            .ok_or_else(|| diagnostic::input_invalid("capability-ref", None))?;
        let parsed = NamespacedId::parse(text)
            .map_err(|_| diagnostic::input_invalid("capability-ref", Some(text)))?;
        if !requirements
            .iter()
            .any(|record| record.requirement_id().as_str() == parsed.as_str())
        {
            return Err(diagnostic::rule_invalid(
                diagnostic::CAPABILITY_MISSING,
                "unresolved-requirement-ref",
                Some(text),
            ));
        }
        if refs.contains(&parsed) {
            return Err(diagnostic::input_invalid(
                "duplicate-capability-ref",
                Some(text),
            ));
        }
        refs.push(parsed);
    }
    refs.sort();
    Ok(refs)
}

/// Parse one typed predicate literal.
fn literal(value: &Json) -> Result<Literal, DiagnosticSet> {
    match value {
        Json::Bool(flag) => Ok(Literal::Boolean(*flag)),
        Json::Number(number) => number
            .as_i64()
            .map(Literal::Integer)
            .ok_or_else(|| diagnostic::input_invalid("noncanonical-literal", None)),
        Json::String(text) => {
            let bytes = text.as_bytes();
            let valid = !text.is_empty()
                && text.len() <= 256
                && bytes[0].is_ascii_alphanumeric()
                && bytes[1..].iter().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b':' | b'@' | b'-')
                });
            if valid {
                Ok(Literal::Text(text.clone()))
            } else {
                Err(diagnostic::input_invalid("noncanonical-literal", None))
            }
        }
        _ => Err(diagnostic::input_invalid("noncanonical-literal", None)),
    }
}

/// Parse one unique invariant.
fn invariant(item: &Json) -> Result<Invariant, DiagnosticSet> {
    closed_keys(
        item,
        &[
            "invariantId",
            "kind",
            "resourceRef",
            "keyFields",
            "predicate",
            "violation",
            "enforcement",
        ],
        "invariant",
    )?;
    if string_member(item, "kind", "invariant-kind")? != "unique" {
        return Err(diagnostic::rule_invalid(
            diagnostic::INVARIANT_INVALID,
            "kind",
            None,
        ));
    }
    let invariant_id = NamespacedId::parse(string_member(item, "invariantId", "invariant-id")?)
        .map_err(|_| diagnostic::input_invalid("invariant-id", None))?;
    let resource = ResourceRef::parse(string_member(item, "resourceRef", "resource-ref")?)
        .map_err(|error| ref_error("resource-ref", error))?;
    let field_items = item
        .get("keyFields")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("key-field-list", None))?;
    if field_items.is_empty() || field_items.len() > version::MAX_KEY_FIELDS {
        return Err(diagnostic::input_invalid("key-field-limit", None));
    }
    let mut key_fields = Vec::with_capacity(field_items.len());
    for field in field_items {
        let parsed = FieldName::new(
            field
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("key-field", None))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("key-field", None))?;
        if key_fields.contains(&parsed) {
            return Err(diagnostic::rule_invalid(
                diagnostic::INVARIANT_INVALID,
                "duplicate-key-field",
                Some(invariant_id.as_str()),
            ));
        }
        key_fields.push(parsed);
    }
    key_fields.sort();
    let predicate_json = item
        .get("predicate")
        .ok_or_else(|| diagnostic::input_invalid("invariant-predicate", None))?;
    closed_keys(predicate_json, &["op", "field", "value"], "predicate")?;
    let predicate = match string_member(predicate_json, "op", "predicate-op")? {
        "field_not_null" => {
            if predicate_json.get("value").is_some() {
                return Err(diagnostic::input_invalid("unknown-field", Some("value")));
            }
            InvariantPredicate::FieldNotNull(
                FieldName::new(string_member(predicate_json, "field", "predicate-field")?)
                    .ok_or_else(|| diagnostic::input_invalid("predicate-field", None))?,
            )
        }
        "field_equals" => InvariantPredicate::FieldEquals {
            field: FieldName::new(string_member(predicate_json, "field", "predicate-field")?)
                .ok_or_else(|| diagnostic::input_invalid("predicate-field", None))?,
            literal: literal(
                predicate_json
                    .get("value")
                    .ok_or_else(|| diagnostic::input_invalid("predicate-value", None))?,
            )?,
        },
        other => {
            return Err(diagnostic::rule_invalid(
                diagnostic::INVARIANT_INVALID,
                "predicate-op",
                Some(other),
            ))
        }
    };
    let violation = ErrorRef::parse(string_member(item, "violation", "error-ref")?)
        .map_err(|error| ref_error("error-ref", error))?;
    let enforcement = Enforcement::from_key(string_member(item, "enforcement", "enforcement")?)
        .ok_or_else(|| diagnostic::input_invalid("enforcement", None))?;
    Ok(Invariant {
        invariant_id,
        resource,
        key_fields,
        predicate,
        violation,
        enforcement,
    })
}

/// Parse every invariant.
fn invariants(items: &[Json]) -> Result<Vec<Invariant>, DiagnosticSet> {
    if items.len() > version::MAX_INVARIANTS {
        return Err(diagnostic::input_invalid("invariant-limit", None));
    }
    items.iter().map(invariant).collect()
}

/// Parse one concurrency case.
fn concurrency_case(
    item: &Json,
    invariants: &[Invariant],
    requirements: &[CapabilityRequirement],
) -> Result<ConcurrencyCase, DiagnosticSet> {
    closed_keys(
        item,
        &[
            "caseId",
            "scenarioRef",
            "participants",
            "invocations",
            "schedule",
            "barriers",
            "expectedInvariantRefs",
            "expectedOutcomes",
            "capabilityRequirementRefs",
        ],
        "case",
    )?;
    let case_id = NamespacedId::parse(string_member(item, "caseId", "case-id")?)
        .map_err(|_| diagnostic::input_invalid("case-id", None))?;
    let scenario_json = item
        .get("scenarioRef")
        .ok_or_else(|| diagnostic::input_invalid("case-scenario-ref", None))?;
    closed_keys(
        scenario_json,
        &["scenarioId", "scenarioVersion", "irDigest"],
        "scenario-ref",
    )?;
    let scenario_ref = ScenarioRef {
        scenario_id: SemanticId::parse(string_member(scenario_json, "scenarioId", "scenario-id")?)
            .map_err(|_| diagnostic::input_invalid("scenario-id", None))?,
        scenario_version: SemVer::parse(string_member(
            scenario_json,
            "scenarioVersion",
            "scenario-version",
        )?)
        .map_err(|_| diagnostic::input_invalid("scenario-version", None))?,
        ir_digest: Sha256Digest::parse(string_member(
            scenario_json,
            "irDigest",
            "scenario-digest",
        )?)
        .map_err(|_| diagnostic::input_invalid("scenario-digest", None))?,
    };
    let participants = members(
        item,
        "participants",
        version::MAX_PARTICIPANTS,
        "participant-list",
    )?;
    let mut participant_records = Vec::with_capacity(participants.len());
    for participant in participants {
        closed_keys(participant, &["participantId", "stepId"], "participant")?;
        let participant_id = StepId::parse(string_member(
            participant,
            "participantId",
            "participant-id",
        )?)
        .map_err(|_| diagnostic::input_invalid("participant-id", None))?;
        let step_id = StepId::parse(string_member(participant, "stepId", "participant-step")?)
            .map_err(|_| diagnostic::input_invalid("participant-step", None))?;
        participant_records.push(Participant {
            participant_id,
            step_id,
        });
    }
    let invocations = members(
        item,
        "invocations",
        version::MAX_PARTICIPANTS,
        "invocation-list",
    )?;
    let mut invocation_records = Vec::with_capacity(invocations.len());
    for invocation in invocations {
        closed_keys(
            invocation,
            &["invocationId", "participantId", "stepId"],
            "invocation",
        )?;
        let invocation_id =
            StepId::parse(string_member(invocation, "invocationId", "invocation-id")?)
                .map_err(|_| diagnostic::input_invalid("invocation-id", None))?;
        let participant_id = StepId::parse(string_member(
            invocation,
            "participantId",
            "invocation-participant",
        )?)
        .map_err(|_| diagnostic::input_invalid("invocation-participant", None))?;
        let step_id = StepId::parse(string_member(invocation, "stepId", "invocation-step")?)
            .map_err(|_| diagnostic::input_invalid("invocation-step", None))?;
        invocation_records.push(Invocation {
            invocation_id,
            participant_id,
            step_id,
        });
    }
    let schedule_items = members(
        item,
        "schedule",
        version::MAX_SCHEDULE_NODES,
        "schedule-list",
    )?;
    let mut schedule = Vec::with_capacity(schedule_items.len());
    for node in schedule_items {
        closed_keys(
            node,
            &["nodeId", "kind", "invocationId", "joins"],
            "schedule-node",
        )?;
        let node_id = StepId::parse(string_member(node, "nodeId", "node-id")?)
            .map_err(|_| diagnostic::input_invalid("node-id", None))?;
        let kind = string_member(node, "kind", "node-kind")?;
        let invocation = match kind {
            "invoke" => Some(
                StepId::parse(string_member(node, "invocationId", "node-invocation")?)
                    .map_err(|_| diagnostic::input_invalid("node-invocation", None))?,
            ),
            "barrier" => None,
            other => return Err(diagnostic::input_invalid("node-kind", Some(other))),
        };
        let joins = match node.get("joins") {
            Some(Json::Array(items)) => {
                if items.len() > version::MAX_PARTICIPANTS {
                    return Err(diagnostic::input_invalid("join-limit", None));
                }
                let mut joins = Vec::with_capacity(items.len());
                for join in items {
                    let text = join
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("join-ref", None))?;
                    let parsed = StepId::parse(text)
                        .map_err(|_| diagnostic::input_invalid("join-ref", None))?;
                    if !joins.contains(&parsed) {
                        joins.push(parsed);
                    }
                }
                joins.sort();
                joins
            }
            Some(_) => return Err(diagnostic::input_invalid("join-list", None)),
            None => Vec::new(),
        };
        schedule.push(ScheduleNode {
            node_id,
            invocation,
            joins,
        });
    }
    let barrier_items = item
        .get("barriers")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("barrier-list", None))?;
    if barrier_items.len() > version::MAX_BARRIERS {
        return Err(diagnostic::input_invalid("barrier-limit", None));
    }
    let mut barriers = Vec::with_capacity(barrier_items.len());
    for barrier in barrier_items {
        closed_keys(barrier, &["barrierId", "waitsFor"], "barrier")?;
        let barrier_id = StepId::parse(string_member(barrier, "barrierId", "barrier-id")?)
            .map_err(|_| diagnostic::input_invalid("barrier-id", None))?;
        let waits = members(
            barrier,
            "waitsFor",
            version::MAX_PARTICIPANTS,
            "barrier-waits",
        )?;
        let mut waits_for = Vec::with_capacity(waits.len());
        for wait in waits {
            let parsed = StepId::parse(
                wait.as_str()
                    .ok_or_else(|| diagnostic::input_invalid("barrier-wait", None))?,
            )
            .map_err(|_| diagnostic::input_invalid("barrier-wait", None))?;
            if !waits_for.contains(&parsed) {
                waits_for.push(parsed);
            }
        }
        if waits_for.len() < 2 {
            return Err(diagnostic::input_invalid("barrier-waits", None));
        }
        waits_for.sort();
        barriers.push(Barrier {
            barrier_id,
            waits_for,
        });
    }
    let expected_invariants = match item.get("expectedInvariantRefs") {
        Some(Json::Array(items)) => {
            if items.len() > version::MAX_INVARIANTS {
                return Err(diagnostic::input_invalid("expected-invariant-limit", None));
            }
            let mut refs = Vec::with_capacity(items.len());
            for value in items {
                let text = value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("expected-invariant", None))?;
                let parsed = NamespacedId::parse(text)
                    .map_err(|_| diagnostic::input_invalid("expected-invariant", None))?;
                if !invariants
                    .iter()
                    .any(|record| record.invariant_id().as_str() == parsed.as_str())
                {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::INVARIANT_INVALID,
                        "unresolved-invariant-ref",
                        Some(text),
                    ));
                }
                refs.push(parsed);
            }
            refs.sort();
            refs
        }
        Some(_) => return Err(diagnostic::input_invalid("expected-invariant-list", None)),
        None => {
            return Err(diagnostic::input_invalid(
                "missing-field",
                Some("expectedInvariantRefs"),
            ))
        }
    };
    let outcome_items = members(
        item,
        "expectedOutcomes",
        version::MAX_PARTICIPANTS,
        "outcome-list",
    )?;
    let mut expected_outcomes = Vec::with_capacity(outcome_items.len());
    for outcome in outcome_items {
        closed_keys(outcome, &["participantId", "outcome"], "outcome")?;
        let participant_id = StepId::parse(string_member(
            outcome,
            "participantId",
            "outcome-participant",
        )?)
        .map_err(|_| diagnostic::input_invalid("outcome-participant", None))?;
        let outcome = ExpectedOutcome::from_key(string_member(outcome, "outcome", "outcome-kind")?)
            .ok_or_else(|| diagnostic::input_invalid("outcome-kind", None))?;
        expected_outcomes.push(OutcomeExpectation {
            participant_id,
            outcome,
        });
    }
    let capability_refs = requirement_refs(
        item.get("capabilityRequirementRefs")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("capability-ref-list", None))?,
        requirements,
    )?;
    Ok(ConcurrencyCase {
        case_id,
        scenario_ref,
        participants: participant_records,
        invocations: invocation_records,
        schedule,
        barriers,
        expected_invariants,
        expected_outcomes,
        capability_refs,
    })
}

/// A bounded homogeneous array member.
fn members<'a>(
    parent: &'a Json,
    key: &str,
    limit: usize,
    tag: &'static str,
) -> Result<&'a [Json], DiagnosticSet> {
    let items = parent
        .get(key)
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid(tag, None))?;
    if items.is_empty() || items.len() > limit {
        return Err(diagnostic::input_invalid(tag, None));
    }
    Ok(items)
}

/// Parse every concurrency case.
fn concurrency_cases(
    items: &[Json],
    invariants: &[Invariant],
    requirements: &[CapabilityRequirement],
) -> Result<Vec<ConcurrencyCase>, DiagnosticSet> {
    if items.len() > version::MAX_CASES {
        return Err(diagnostic::input_invalid("case-limit", None));
    }
    items
        .iter()
        .map(|item| concurrency_case(item, invariants, requirements))
        .collect()
}
