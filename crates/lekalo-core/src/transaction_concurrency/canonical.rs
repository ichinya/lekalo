//! Canonical serialization of the transaction-concurrency attachment
//! (issue #24).
//!
//! Compact UTF-8 JSON with no insignificant whitespace. Every object's
//! members are written in unsigned UTF-8 byte order of their keys;
//! set-like collections (operations, invariants, cases, requirement
//! records, group lists, key fields, capability references, expected
//! invariants, outcomes, joins, barrier waits) are written sorted;
//! semantically ordered collections (group membership, preconditions in
//! wire-registry order, failure boundaries) keep their exact declared or
//! canonical order — canonicalization never sorts away meaning. The
//! bytes are path-independent: no physical root, raw source, timestamp,
//! host, locale, runtime value, lock token, ETag, or adapter transcript
//! ever enters them, and the writer appends no trailing LF.

use super::effect_group::{AtomicEffectGroup, BoundarySubject, FailureBoundary, OperationContract};
use super::operation::{Idempotency, Retry};
use super::precondition::{
    CapabilityRequirement, Invariant, InvariantPredicate, Literal, Precondition,
};
use super::scenario::ConcurrencyCase;
use super::version;
use super::TransactionConcurrencyAttachment;
use crate::diagnostics::DiagnosticSet;

/// Serialize one whole attachment to canonical bytes, or refuse beyond
/// the payload bound.
pub fn attachment_bytes(
    attachment: &TransactionConcurrencyAttachment,
) -> Result<String, DiagnosticSet> {
    let bytes = attachment_payload(attachment);
    if bytes.len() > version::MAX_CANONICAL_BYTES {
        return Err(super::diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The canonical payload of one attachment.
fn attachment_payload(attachment: &TransactionConcurrencyAttachment) -> String {
    let mut operations: Vec<&OperationContract> = attachment.operations().iter().collect();
    operations.sort_by_key(|operation| operation.operation_ref().as_str());
    let mut invariants: Vec<&Invariant> = attachment.invariants().iter().collect();
    invariants.sort_by_key(|invariant| invariant.invariant_id().as_str());
    let mut cases: Vec<&ConcurrencyCase> = attachment.cases().iter().collect();
    cases.sort_by_key(|case| case.case_id.as_str());
    let mut requirements: Vec<&CapabilityRequirement> =
        attachment.capability_requirements().iter().collect();
    requirements.sort_by_key(|requirement| requirement.requirement_id().as_str());
    object(vec![
        ("schemaVersion", Some(string(version::SCHEMA_VERSION))),
        ("identity", Some(string(version::IDENTITY))),
        ("projectId", Some(string(attachment.project_id().as_str()))),
        ("modelRef", Some(model_ref_payload(attachment.model_ref()))),
        (
            "irRef",
            Some(contract_ref_payload(
                "dev.lekalo.ir@0.1.0",
                attachment.ir_digest().as_str(),
            )),
        ),
        (
            "effectGraphRef",
            attachment
                .effect_graph_digest()
                .map(|digest| contract_ref_payload("dev.lekalo.effects@1.0.0", digest.as_str())),
        ),
        (
            "operations",
            Some(array(
                &operations
                    .iter()
                    .map(|operation| operation_payload(operation))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "invariants",
            Some(array(
                &invariants
                    .iter()
                    .map(|invariant| invariant_payload(invariant))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "concurrencyCases",
            Some(array(
                &cases
                    .iter()
                    .map(|case| case_payload(case))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "capabilityRequirements",
            Some(array(
                &requirements
                    .iter()
                    .map(|requirement| requirement_payload(requirement))
                    .collect::<Vec<String>>(),
            )),
        ),
    ])
}

/// The canonical Model reference.
fn model_ref_payload(model: &super::ModelPin) -> String {
    object(vec![
        ("modelVersion", Some(string(model.version().as_str()))),
        ("digest", Some(string(model.digest().as_str()))),
    ])
}

/// The canonical `{identity, digest}` contract reference.
fn contract_ref_payload(identity: &str, digest: &str) -> String {
    object(vec![
        ("identity", Some(string(identity))),
        ("digest", Some(string(digest))),
    ])
}

/// The canonical operation contract.
fn operation_payload(operation: &OperationContract) -> String {
    let mut groups: Vec<&AtomicEffectGroup> = operation.groups().iter().collect();
    groups.sort_by_key(|group| group.group_id().as_str());
    object(vec![
        (
            "operationRef",
            Some(string(operation.operation_ref().as_str())),
        ),
        (
            "operationVersion",
            Some(string(operation.operation_version().as_str())),
        ),
        ("transaction", Some(string(operation.transaction().key()))),
        (
            "isolationRequirement",
            Some(string(operation.isolation().key())),
        ),
        (
            "atomicEffectGroups",
            Some(array(
                &groups
                    .iter()
                    .map(|group| group_payload(group))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "preconditions",
            Some(array(&precondition_payloads(operation.preconditions()))),
        ),
        (
            "failureBoundaries",
            Some(array(
                &operation
                    .failure_boundaries()
                    .iter()
                    .map(boundary_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "idempotency",
            Some(idempotency_payload(operation.idempotency())),
        ),
        ("retry", Some(retry_payload(operation.retry()))),
        (
            "capabilityRequirements",
            Some(array(
                &operation
                    .capability_refs()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
    ])
}

/// The canonical atomic effect group.
fn group_payload(group: &AtomicEffectGroup) -> String {
    object(vec![
        ("groupId", Some(string(group.group_id().as_str()))),
        (
            "effectRefs",
            Some(array(
                &group
                    .effect_refs()
                    .iter()
                    .map(|effect| string(&effect.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        ("scope", Some(string("local"))),
        ("atomicity", Some(string("all_or_nothing"))),
        (
            "commitBoundary",
            Some(string(group.commit_boundary().key())),
        ),
        ("failurePolicy", Some(string("abort"))),
    ])
}

/// The canonical preconditions: set-like, keyed and ordered by
/// `(kind, resource, discriminator)` in unsigned byte order.
fn precondition_payloads(preconditions: &[Precondition]) -> Vec<String> {
    let mut payloads: Vec<(String, String)> = Vec::with_capacity(preconditions.len());
    for precondition in preconditions {
        match precondition {
            Precondition::Version(version) => payloads.push((
                format!("version|{}", version.resource().as_str()),
                object(vec![
                    ("kind", Some(string("version"))),
                    ("resourceRef", Some(string(version.resource().as_str()))),
                    (
                        "tokenFieldRef",
                        Some(string(version.token_field().as_str())),
                    ),
                    ("suppliedBy", Some(string(version.supplied_by().key()))),
                    ("compare", Some(string("exact"))),
                    ("check", Some(string("at_commit"))),
                    ("mismatch", Some(string(version.mismatch().as_str()))),
                ]),
            )),
            Precondition::Etag(etag) => payloads.push((
                format!(
                    "etag|{}|{}",
                    etag.resource().as_str(),
                    etag.etag_ref().as_str()
                ),
                object(vec![
                    ("kind", Some(string("etag"))),
                    ("resourceRef", Some(string(etag.resource().as_str()))),
                    ("etagRef", Some(string(etag.etag_ref().as_str()))),
                    ("suppliedBy", Some(string(etag.supplied_by().key()))),
                    ("compare", Some(string("exact"))),
                    ("check", Some(string("at_commit"))),
                    ("mismatch", Some(string(etag.mismatch().as_str()))),
                ]),
            )),
            Precondition::Lock(lock) => payloads.push((
                format!(
                    "lock|{}|{}|{}",
                    lock.resource().as_str(),
                    lock.scope().key(),
                    lock.mode().key()
                ),
                object(vec![
                    ("kind", Some(string("lock"))),
                    ("resourceRef", Some(string(lock.resource().as_str()))),
                    ("scope", Some(string(lock.scope().key()))),
                    ("mode", Some(string(lock.mode().key()))),
                    ("acquisition", Some(string(lock.acquisition().key()))),
                    ("orderKey", Some(string(lock.order_key().as_str()))),
                    ("timeoutPolicy", Some(string(lock.timeout().key()))),
                ]),
            )),
        }
    }
    payloads.sort();
    payloads.into_iter().map(|(_, payload)| payload).collect()
}

/// The canonical failure boundary (declared order).
fn boundary_payload(boundary: &FailureBoundary) -> String {
    let subject = match boundary.subject() {
        BoundarySubject::Group(group) => string(group.as_str()),
        BoundarySubject::Effect(effect) => string(&effect.as_str()),
        BoundarySubject::Operation(operation) => string(operation.as_str()),
    };
    object(vec![
        ("boundaryId", Some(string(boundary.boundary_id().as_str()))),
        ("kind", Some(string(boundary.kind().key()))),
        ("subjectRef", Some(subject)),
        (
            "compensationRef",
            boundary
                .compensation_ref()
                .map(|reference| string(reference.as_str())),
        ),
        (
            "outcome",
            boundary
                .recovery_declared()
                .then(|| string("recovery_required")),
        ),
    ])
}

/// The canonical idempotency declaration.
fn idempotency_payload(idempotency: &Idempotency) -> String {
    object(vec![
        ("mode", Some(string(idempotency.mode().key()))),
        (
            "keyRef",
            idempotency.key_field().map(|field| string(field.as_str())),
        ),
        ("scope", Some(string(idempotency.scope().as_str()))),
        ("record", Some(string(idempotency.record().key()))),
        ("duplicate", Some(string(idempotency.duplicate().key()))),
        ("sameKeyDifferentRequest", Some(string("conflict"))),
    ])
}

/// The canonical retry declaration (phases in registry order).
fn retry_payload(retry: &Retry) -> String {
    object(vec![
        ("safety", Some(string(retry.safety().key()))),
        ("condition", Some(string(retry.condition().key()))),
        (
            "phases",
            Some(array(
                &retry
                    .phases()
                    .iter()
                    .map(|phase| string(phase.key()))
                    .collect::<Vec<String>>(),
            )),
        ),
    ])
}

/// The canonical invariant.
fn invariant_payload(invariant: &Invariant) -> String {
    let predicate = match invariant.predicate() {
        InvariantPredicate::FieldNotNull(field) => object(vec![
            ("op", Some(string("field_not_null"))),
            ("field", Some(string(field.as_str()))),
        ]),
        InvariantPredicate::FieldEquals { field, literal } => object(vec![
            ("op", Some(string("field_equals"))),
            ("field", Some(string(field.as_str()))),
            ("value", Some(literal_wire(literal))),
        ]),
    };
    object(vec![
        (
            "invariantId",
            Some(string(invariant.invariant_id().as_str())),
        ),
        ("kind", Some(string("unique"))),
        ("resourceRef", Some(string(invariant.resource().as_str()))),
        (
            "keyFields",
            Some(array(
                &invariant
                    .key_fields()
                    .iter()
                    .map(|field| string(field.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        ("predicate", Some(predicate)),
        ("violation", Some(string(invariant.violation().as_str()))),
        ("enforcement", Some(string(invariant.enforcement().key()))),
    ])
}

/// The canonical concurrency case.
fn case_payload(case: &ConcurrencyCase) -> String {
    let mut participants: Vec<&super::scenario::Participant> = case.participants().iter().collect();
    participants.sort_by_key(|participant| participant.participant_id().as_str());
    let mut invocations: Vec<&super::scenario::Invocation> = case.invocations().iter().collect();
    invocations.sort_by_key(|invocation| invocation.invocation_id().as_str());
    let mut schedule: Vec<&super::scenario::ScheduleNode> = case.schedule().iter().collect();
    schedule.sort_by_key(|node| node.node_id().as_str());
    let mut barriers: Vec<&super::scenario::Barrier> = case.barriers().iter().collect();
    barriers.sort_by_key(|barrier| barrier.barrier_id().as_str());
    let mut outcomes: Vec<&super::scenario::OutcomeExpectation> =
        case.expected_outcomes().iter().collect();
    outcomes.sort_by_key(|outcome| outcome.participant_id().as_str());
    object(vec![
        ("caseId", Some(string(case.case_id().as_str()))),
        (
            "scenarioRef",
            Some(object(vec![
                (
                    "scenarioId",
                    Some(string(case.scenario_ref().scenario_id().as_str())),
                ),
                (
                    "scenarioVersion",
                    Some(string(case.scenario_ref().scenario_version().as_str())),
                ),
                (
                    "irDigest",
                    Some(string(case.scenario_ref().ir_digest().as_str())),
                ),
            ])),
        ),
        (
            "participants",
            Some(array(
                &participants
                    .iter()
                    .map(|participant| {
                        object(vec![
                            (
                                "participantId",
                                Some(string(participant.participant_id().as_str())),
                            ),
                            ("stepId", Some(string(participant.step_id().as_str()))),
                        ])
                    })
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "invocations",
            Some(array(
                &invocations
                    .iter()
                    .map(|invocation| {
                        object(vec![
                            (
                                "invocationId",
                                Some(string(invocation.invocation_id().as_str())),
                            ),
                            (
                                "participantId",
                                Some(string(invocation.participant_id().as_str())),
                            ),
                            ("stepId", Some(string(invocation.step_id().as_str()))),
                        ])
                    })
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "schedule",
            Some(array(
                &schedule
                    .iter()
                    .map(|node| {
                        object(vec![
                            ("nodeId", Some(string(node.node_id().as_str()))),
                            (
                                "kind",
                                Some(string(if node.invocation().is_some() {
                                    "invoke"
                                } else {
                                    "barrier"
                                })),
                            ),
                            (
                                "invocationId",
                                node.invocation()
                                    .map(|invocation| string(invocation.as_str())),
                            ),
                            (
                                "joins",
                                Some(array(
                                    &node
                                        .joins()
                                        .iter()
                                        .map(|join| string(join.as_str()))
                                        .collect::<Vec<String>>(),
                                )),
                            ),
                        ])
                    })
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "barriers",
            Some(array(
                &barriers
                    .iter()
                    .map(|barrier| {
                        object(vec![
                            ("barrierId", Some(string(barrier.barrier_id().as_str()))),
                            (
                                "waitsFor",
                                Some(array(
                                    &barrier
                                        .waits_for()
                                        .iter()
                                        .map(|wait| string(wait.as_str()))
                                        .collect::<Vec<String>>(),
                                )),
                            ),
                        ])
                    })
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "expectedInvariantRefs",
            Some(array(
                &case
                    .expected_invariants()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "expectedOutcomes",
            Some(array(
                &outcomes
                    .iter()
                    .map(|outcome| {
                        object(vec![
                            (
                                "participantId",
                                Some(string(outcome.participant_id().as_str())),
                            ),
                            ("outcome", Some(string(outcome.outcome().key()))),
                        ])
                    })
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "capabilityRequirementRefs",
            Some(array(
                &case
                    .capability_refs()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
    ])
}

/// The canonical capability requirement.
fn requirement_payload(requirement: &CapabilityRequirement) -> String {
    object(vec![
        (
            "requirementId",
            Some(string(requirement.requirement_id().as_str())),
        ),
        (
            "capability",
            Some(string(&requirement.capability().to_wire())),
        ),
        ("minimum", Some(string(requirement.minimum().key()))),
        ("reason", Some(string(requirement.reason()))),
    ])
}

/// The typed literal wire form.
fn literal_wire(literal: &Literal) -> String {
    match literal {
        Literal::Boolean(flag) => flag.to_string(),
        Literal::Integer(value) => value.to_string(),
        Literal::Text(text) => string(text),
    }
}

/// One canonical JSON string value.
fn string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// One canonical JSON array.
fn array(members: &[String]) -> String {
    format!("[{}]", members.join(","))
}

/// One canonical JSON object with byte-sorted keys; `None` members are
/// dropped entirely.
fn object(members: Vec<(&str, Option<String>)>) -> String {
    let mut members: Vec<(String, String)> = members
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value)))
        .collect();
    members.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let rendered: Vec<String> = members
        .iter()
        .map(|(key, value)| format!("{}:{}", string(key), value))
        .collect();
    format!("{{{}}}", rendered.join(","))
}
