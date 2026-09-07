//! Canonical serialization of the extended-effects attachment
//! (issue #26).
//!
//! Canonical bytes are compact UTF-8 JSON with byte-sorted object keys,
//! no BOM, and no trailing LF; `None` members are dropped entirely.
//! Set-like collections (contracts, cases, requirement records,
//! capability references, error references, key fields, cache
//! operations, expected outcomes, join sets) normalize to unsigned
//! UTF-8-byte order; schedule nodes order canonically by node id while
//! their join sets keep the declared partial order. The output is
//! path-independent and byte-identical for value-equal attachments.

/// The canonical payload bytes, or the typed over-bound refusal.
pub fn attachment_bytes(
    attachment: &super::ExtendedEffectsAttachment,
) -> Result<String, DiagnosticSet> {
    let bytes = attachment_payload(attachment);
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(super::diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

use crate::diagnostics::DiagnosticSet;

/// The full canonical attachment payload.
fn attachment_payload(attachment: &super::ExtendedEffectsAttachment) -> String {
    let mut events: Vec<&super::EventContract> = attachment.events().iter().collect();
    events.sort_by_key(|contract| contract.common.contract_ref.as_str());
    let mut jobs: Vec<&super::JobContract> = attachment.jobs().iter().collect();
    jobs.sort_by_key(|contract| contract.common.contract_ref.as_str());
    let mut calls: Vec<&super::CallContract> = attachment.calls().iter().collect();
    calls.sort_by_key(|contract| contract.common.contract_ref.as_str());
    let mut caches: Vec<&super::CacheContract> = attachment.caches().iter().collect();
    caches.sort_by_key(|contract| contract.common.contract_ref.as_str());
    let mut publications: Vec<&super::PublicationContract> =
        attachment.publications().iter().collect();
    publications.sort_by_key(|contract| contract.common.contract_ref.as_str());
    let mut cases: Vec<&super::PartialFailureCase> = attachment.cases().iter().collect();
    cases.sort_by_key(|case| case.case_id.as_str());
    let mut requirements: Vec<&super::CapabilityRequirement> =
        attachment.capability_requirements().iter().collect();
    requirements.sort_by_key(|requirement| requirement.requirement_id.as_str());
    object(vec![
        (
            "schemaVersion",
            Some(string(super::version::SCHEMA_VERSION)),
        ),
        ("identity", Some(string(super::version::IDENTITY))),
        ("projectId", Some(string(attachment.project_id().as_str()))),
        ("modelRef", Some(model_ref_payload(attachment.model_ref()))),
        (
            "irRef",
            Some(digest_payload(
                "dev.lekalo.ir@0.1.0",
                attachment.ir_digest().as_str(),
            )),
        ),
        (
            "effectGraphRef",
            attachment
                .effect_graph_digest()
                .map(|digest| digest_payload("dev.lekalo.effects@1.0.0", digest.as_str())),
        ),
        (
            "events",
            Some(array(
                &events
                    .iter()
                    .map(|event| event_payload(event))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "jobs",
            Some(array(
                &jobs
                    .iter()
                    .map(|job| job_payload(job))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "calls",
            Some(array(
                &calls
                    .iter()
                    .map(|call| call_payload(call))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "cacheContracts",
            Some(array(
                &caches
                    .iter()
                    .map(|cache| cache_payload(cache))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "publications",
            Some(array(
                &publications
                    .iter()
                    .map(|publication| publication_payload(publication))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "partialFailureCases",
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

/// One canonical `{identity, digest}` reference.
fn digest_payload(identity: &str, digest: &str) -> String {
    object(vec![
        ("identity", Some(string(identity))),
        ("digest", Some(string(digest))),
    ])
}

/// The canonical common contract members.
fn common_payload(common: &super::contract::Common) -> Vec<(&'static str, Option<String>)> {
    vec![
        ("contractRef", Some(string(common.contract_ref.as_str()))),
        (
            "contractVersion",
            Some(string(common.contract_version.as_str())),
        ),
        ("effectRef", Some(string(&common.effect.as_str()))),
        (
            "securityGate",
            common
                .security_gate
                .as_ref()
                .map(|gate| object(vec![("reviewRef", Some(string(gate.review_ref())))])),
        ),
        ("sensitivity", Some(string(common.sensitivity.key()))),
        ("portability", Some(string(common.portability.key()))),
        (
            "targetProfileRef",
            common
                .target_profile
                .as_ref()
                .map(|reference| string(reference.as_str())),
        ),
        (
            "contributions",
            Some(contributions_payload(&common.contributions)),
        ),
        (
            "capabilityRequirementRefs",
            Some(array(
                &common
                    .capability_refs
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
    ]
}

/// The canonical contribution matrix.
fn contributions_payload(contributions: &super::Contributions) -> String {
    object(vec![
        ("graph", Some(contributions.graph.to_string())),
        ("impact", Some(contributions.impact.to_string())),
        ("context", Some(contributions.context.to_string())),
        ("scenarios", Some(contributions.scenarios.to_string())),
    ])
}

/// One canonical event contract.
fn event_payload(event: &super::EventContract) -> String {
    let mut members = common_payload(&event.common);
    members.push(("eventVersion", Some(string(event.event_version.as_str()))));
    members.push(("schemaDigest", Some(string(event.schema_digest.as_str()))));
    members.push(("delivery", Some(string(event.delivery.key()))));
    members.push(("ordering", Some(string(event.ordering.key()))));
    members.push((
        "deduplication",
        Some(object(vec![
            ("mode", Some(string(event.deduplication.mode.key()))),
            (
                "keyField",
                event
                    .deduplication
                    .key_field
                    .as_ref()
                    .map(|field| string(field.as_str())),
            ),
        ])),
    ));
    members.push((
        "correlationId",
        event
            .correlation_id
            .as_ref()
            .map(|field| string(field.as_str())),
    ));
    members.push((
        "causationId",
        event
            .causation_id
            .as_ref()
            .map(|field| string(field.as_str())),
    ));
    object(members)
}

/// One canonical job contract.
fn job_payload(job: &super::JobContract) -> String {
    let mut members = common_payload(&job.common);
    members.push(("payloadDigest", Some(string(job.payload_digest.as_str()))));
    members.push(("queueClass", Some(string(job.queue_class.as_str()))));
    members.push((
        "retry",
        Some(object(vec![
            ("maxAttempts", Some(job.retry.max_attempts.to_string())),
            ("backoff", Some(string(job.retry.backoff.key()))),
            ("capMillis", job.retry.cap_millis.map(|cap| cap.to_string())),
        ])),
    ));
    members.push((
        "idempotency",
        Some(object(vec![
            ("mode", Some(string(job.idempotency.mode.key()))),
            (
                "keyField",
                job.idempotency
                    .key_field
                    .as_ref()
                    .map(|field| string(field.as_str())),
            ),
        ])),
    ));
    members.push(("timeoutMillis", Some(job.timeout_millis.to_string())));
    members.push(("deadLetter", Some(string(job.dead_letter.key()))));
    object(members)
}

/// One canonical external-call contract.
fn call_payload(call: &super::CallContract) -> String {
    let mut members = common_payload(&call.common);
    members.push(("providerContract", Some(string(call.provider.as_str()))));
    members.push(("requestDigest", Some(string(call.request_digest.as_str()))));
    members.push((
        "responseDigest",
        Some(string(call.response_digest.as_str())),
    ));
    members.push((
        "errorRefs",
        Some(array(
            &call
                .error_refs
                .iter()
                .map(|error| string(error.as_str()))
                .collect::<Vec<String>>(),
        )),
    ));
    members.push(("timeoutMillis", Some(call.timeout_millis.to_string())));
    members.push((
        "retry",
        Some(object(vec![
            ("maxAttempts", Some(call.retry.max_attempts.to_string())),
            ("safety", Some(string(call.retry.safety.key()))),
        ])),
    ));
    members.push(("classification", Some(string(call.classification.key()))));
    members.push((
        "compensationRef",
        call.compensation
            .as_ref()
            .map(|operation| string(operation.as_str())),
    ));
    object(members)
}

/// One canonical cache contract.
fn cache_payload(cache: &super::CacheContract) -> String {
    let mut members = common_payload(&cache.common);
    members.push((
        "keyContract",
        Some(object(vec![
            ("keyVersion", Some(string(cache.key.key_version.as_str()))),
            (
                "keyFields",
                Some(array(
                    &cache
                        .key
                        .key_fields
                        .iter()
                        .map(|field| string(field.as_str()))
                        .collect::<Vec<String>>(),
                )),
            ),
        ])),
    ));
    members.push((
        "operations",
        Some(array(
            &cache
                .operations
                .iter()
                .map(|operation| string(operation.key()))
                .collect::<Vec<String>>(),
        )),
    ));
    members.push(("consistency", Some(string(cache.consistency.key()))));
    members.push((
        "freshness",
        cache.freshness.map(|freshness| {
            object(vec![
                ("ttlMillis", freshness.ttl_millis.map(|ttl| ttl.to_string())),
                (
                    "maxStaleMillis",
                    freshness.max_stale_millis.map(|stale| stale.to_string()),
                ),
            ])
        }),
    ));
    object(members)
}

/// One canonical publication contract.
fn publication_payload(publication: &super::PublicationContract) -> String {
    let mut members = common_payload(&publication.common);
    members.push((
        "destination",
        Some(string(publication.destination.as_str())),
    ));
    members.push((
        "consent",
        Some(object(vec![
            ("optIn", Some("true".to_owned())),
            ("approval", Some(string(publication.consent.approval.key()))),
            (
                "approverRef",
                publication
                    .consent
                    .approver
                    .as_ref()
                    .map(|reference| string(reference.as_str())),
            ),
        ])),
    ));
    members.push((
        "snapshot",
        Some(object(vec![
            ("mode", Some(string("revisioned"))),
            (
                "revisionField",
                Some(string(publication.snapshot.revision_field.as_str())),
            ),
        ])),
    ));
    object(members)
}

/// One canonical partial-failure case.
fn case_payload(case: &super::PartialFailureCase) -> String {
    object(vec![
        ("caseId", Some(string(case.case_id.as_str()))),
        (
            "scenarioRef",
            Some(object(vec![
                (
                    "scenarioId",
                    Some(string(case.scenario.scenario_id.as_str())),
                ),
                (
                    "scenarioVersion",
                    Some(string(case.scenario.scenario_version.as_str())),
                ),
                ("irDigest", Some(string(case.scenario.ir_digest.as_str()))),
            ])),
        ),
        (
            "steps",
            Some(array(
                &case
                    .steps
                    .iter()
                    .map(|step| {
                        object(vec![
                            ("stepId", Some(string(step.step_id.as_str()))),
                            ("contractRef", Some(string(step.contract_ref.as_str()))),
                            ("fault", Some(string(step.fault.key()))),
                        ])
                    })
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "schedule",
            Some(array(
                &case
                    .schedule
                    .iter()
                    .map(|node| {
                        object(vec![
                            ("nodeId", Some(string(node.node_id.as_str()))),
                            ("kind", Some(string(node.kind.key()))),
                            (
                                "stepId",
                                node.step_id.as_ref().map(|step| string(step.as_str())),
                            ),
                            (
                                "joins",
                                Some(array(
                                    &node
                                        .joins
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
            "expectedOutcomes",
            Some(array(
                &case
                    .outcomes
                    .iter()
                    .map(|outcome| {
                        object(vec![
                            ("contractRef", Some(string(outcome.contract_ref.as_str()))),
                            ("outcome", Some(string(outcome.outcome.key()))),
                        ])
                    })
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "capabilityRequirementRefs",
            Some(array(
                &case
                    .capability_refs
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
    ])
}

/// One canonical capability-requirement record.
fn requirement_payload(requirement: &super::CapabilityRequirement) -> String {
    object(vec![
        (
            "requirementId",
            Some(string(requirement.requirement_id.as_str())),
        ),
        (
            "capability",
            Some(string(&requirement.capability.to_wire())),
        ),
        ("minimum", Some(string(requirement.minimum.key()))),
        ("reason", Some(string(requirement.reason()))),
    ])
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
