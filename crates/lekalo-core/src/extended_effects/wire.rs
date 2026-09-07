//! Wire normalization of the extended-effects attachment (issue #26).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`ExtendedEffectsAttachment`](super::ExtendedEffectsAttachment). It
//! fails closed before semantic processing: unknown or missing fields,
//! wrong identities, malformed identifiers, digests, bounds, malformed
//! effect bindings, and bound violations each return one typed
//! registered diagnostic and no partial attachment. Conditional
//! semantic rules (deduplication keys, retry-idempotency coherence,
//! capability coverage, consent, review gates, schedules) live in the
//! attachment's semantic self-check.

use crate::diagnostics::DiagnosticSet;
use crate::effects::{FieldName, OperationId};
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{NamespacedId, SemanticId, StepId};
use serde_json::Value as Json;

use super::capability::{Capability, CapabilityRequirement, RequirementLevel};
use super::case::{
    CaseNode, CaseStep, ExpectedOutcome, Fault, NodeKind, Outcome, PartialFailureCase, ScenarioRef,
};
use super::contract::{
    Approval, Backoff, CacheContract, CacheFreshness, CacheKeyContract, CacheOperation,
    CallContract, CallRetry, CallSafety, Classification, Common, Consent, Consistency,
    Contributions, DeadLetter, Deduplication, DeduplicationMode, Delivery, EventContract,
    JobContract, JobIdempotency, JobIdempotencyMode, JobRetry, Ordering, Portability,
    PublicationContract, SecurityGate, Sensitivity, Snapshot,
};
use super::diagnostic;
use super::identity::{EffectRef, ErrorRef, ProviderContract};
use super::version;
use super::{ExtendedEffectsAttachment, ModelPin};

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "modelRef",
    "irRef",
    "effectGraphRef",
    "events",
    "jobs",
    "calls",
    "cacheContracts",
    "publications",
    "partialFailureCases",
    "capabilityRequirements",
];

/// The required top-level members (`effectGraphRef` is optional).
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "modelRef",
    "irRef",
    "events",
    "jobs",
    "calls",
    "cacheContracts",
    "publications",
    "partialFailureCases",
    "capabilityRequirements",
];

/// The common contract member keys shared by every kind.
const COMMON_KEYS: &[&str] = &[
    "contractRef",
    "contractVersion",
    "effectRef",
    "sensitivity",
    "securityGate",
    "portability",
    "targetProfileRef",
    "contributions",
    "capabilityRequirementRefs",
];

/// The exact #14 effect-kind keys each contract family may bind.
pub(crate) const EVENT_KINDS: &[&str] = &["emit-event"];
pub(crate) const JOB_KINDS: &[&str] = &["enqueue-job"];
pub(crate) const CALL_KINDS: &[&str] = &["external-call"];
pub(crate) const CACHE_KINDS: &[&str] = &["cache-read", "cache-write", "cache-invalidate"];
pub(crate) const PUBLICATION_KINDS: &[&str] = &["publish-output"];

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial attachment. Pure: no source,
/// model, cache, report, network, process, or target access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<ExtendedEffectsAttachment, DiagnosticSet> {
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
    let events = events(
        object
            .get("events")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("event-list", None))?,
    )?;
    let jobs = jobs(
        object
            .get("jobs")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("job-list", None))?,
    )?;
    let calls = calls(
        object
            .get("calls")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("call-list", None))?,
    )?;
    let caches = caches(
        object
            .get("cacheContracts")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("cache-list", None))?,
    )?;
    let publications = publications(
        object
            .get("publications")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("publication-list", None))?,
    )?;
    let cases = partial_failure_cases(
        object
            .get("partialFailureCases")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("case-list", None))?,
    )?;
    let attachment = ExtendedEffectsAttachment::assemble(
        project_id,
        model_ref,
        ir_ref,
        effect_graph,
        events,
        jobs,
        calls,
        caches,
        publications,
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

/// The wire names of one member set.
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

/// One bounded integer member.
fn integer_member(object: &Json, key: &str, tag: &'static str) -> Result<u64, DiagnosticSet> {
    object
        .get(key)
        .and_then(Json::as_u64)
        .ok_or_else(|| diagnostic::input_invalid(tag, Some(key)))
}

/// One optional string member.
fn optional_string<'a>(object: &'a Json, key: &str) -> Result<Option<&'a str>, DiagnosticSet> {
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| diagnostic::input_invalid("field", Some(key))),
    }
}

/// One optional unsigned integer member.
fn optional_integer(object: &Json, key: &str) -> Result<Option<u64>, DiagnosticSet> {
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| diagnostic::input_invalid("integer", Some(key))),
    }
}

/// Parse one typed identifier member.
fn id_member<T>(
    object: &Json,
    key: &str,
    tag: &'static str,
    parse: fn(&str) -> Result<T, crate::scenario::id::IdError>,
) -> Result<T, DiagnosticSet> {
    parse(string_member(object, key, tag)?).map_err(|_| diagnostic::input_invalid(tag, None))
}

/// Parse the closed sensitivity key.
fn sensitivity_member(object: &Json) -> Result<Sensitivity, DiagnosticSet> {
    Sensitivity::from_key(string_member(object, "sensitivity", "sensitivity")?)
        .ok_or_else(|| diagnostic::input_invalid("sensitivity", None))
}

/// Parse the closed portability key.
fn portability_member(object: &Json) -> Result<Portability, DiagnosticSet> {
    Portability::from_key(string_member(object, "portability", "portability")?)
        .ok_or_else(|| diagnostic::input_invalid("portability", None))
}

/// Parse the security gate hint, when present.
fn security_gate_member(object: &Json) -> Result<Option<SecurityGate>, DiagnosticSet> {
    let Some(gate) = object.get("securityGate") else {
        return Ok(None);
    };
    closed_keys(gate, &["reviewRef"], "security-gate")?;
    Ok(Some(SecurityGate::new(id_member(
        gate,
        "reviewRef",
        "gate-ref",
        NamespacedId::parse,
    )?)))
}

/// Parse the explicit contribution matrix.
fn contributions_member(object: &Json) -> Result<Contributions, DiagnosticSet> {
    let contributions = object
        .get("contributions")
        .ok_or_else(|| diagnostic::input_invalid("contributions", None))?;
    closed_keys(contributions, &Contributions::KEYS, "contributions")?;
    let flag = |key: &str| -> Result<bool, DiagnosticSet> {
        contributions
            .get(key)
            .and_then(Json::as_bool)
            .ok_or_else(|| diagnostic::input_invalid("contribution-flag", Some(key)))
    };
    Ok(Contributions {
        graph: flag("graph")?,
        impact: flag("impact")?,
        context: flag("context")?,
        scenarios: flag("scenarios")?,
    })
}

/// Parse the sorted capability-requirement references.
fn capability_refs_member(object: &Json) -> Result<Vec<NamespacedId>, DiagnosticSet> {
    let items = object
        .get("capabilityRequirementRefs")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("capability-refs", None))?;
    if items.len() > version::MAX_CAPABILITY_REFS {
        return Err(diagnostic::input_invalid("capability-ref-limit", None));
    }
    let mut references = Vec::with_capacity(items.len());
    for item in items {
        references.push(
            NamespacedId::parse(
                item.as_str()
                    .ok_or_else(|| diagnostic::input_invalid("capability-ref", None))?,
            )
            .map_err(|_| diagnostic::input_invalid("capability-ref", None))?,
        );
    }
    references.sort();
    Ok(references)
}

/// Parse the common contract members shared by every kind.
fn common_members(
    object: &Json,
    specific_keys: &[&str],
    allowed_kinds: &[&str],
    family_rule: &'static str,
) -> Result<Common, DiagnosticSet> {
    let mut allowed: Vec<&str> = COMMON_KEYS.to_vec();
    allowed.extend_from_slice(specific_keys);
    closed_keys(object, &allowed, "contract")?;
    let contract_ref = id_member(object, "contractRef", "contract-ref", NamespacedId::parse)?;
    let contract_version = SemVer::parse(string_member(
        object,
        "contractVersion",
        "contract-version",
    )?)
    .map_err(|_| diagnostic::input_invalid("contract-version", None))?;
    let effect =
        EffectRef::parse(string_member(object, "effectRef", "effect-ref")?).map_err(|error| {
            match error {
                super::identity::RefError::Kind => {
                    diagnostic::rule_invalid(family_rule, "effect-kind", None)
                }
                super::identity::RefError::Subject => {
                    diagnostic::rule_invalid(family_rule, "effect-subject", None)
                }
                super::identity::RefError::Occurrence => {
                    diagnostic::rule_invalid(family_rule, "effect-occurrence", None)
                }
                super::identity::RefError::Shape => {
                    diagnostic::rule_invalid(family_rule, "bad-effect-ref", None)
                }
            }
        })?;
    if !allowed_kinds.contains(&effect.kind_key()) {
        return Err(diagnostic::rule_invalid(
            family_rule,
            "effect-kind-binding",
            Some(effect.kind_key()),
        ));
    }
    let sensitivity = sensitivity_member(object)?;
    let security_gate = security_gate_member(object)?;
    let portability = portability_member(object)?;
    let target_profile = match optional_string(object, "targetProfileRef")? {
        None => None,
        Some(text) => Some(
            NamespacedId::parse(text)
                .map_err(|_| diagnostic::input_invalid("target-profile", None))?,
        ),
    };
    let contributions = contributions_member(object)?;
    let capability_refs = capability_refs_member(object)?;
    Ok(Common {
        contract_ref,
        contract_version,
        effect,
        sensitivity,
        security_gate,
        portability,
        target_profile,
        contributions,
        capability_refs,
    })
}

/// Parse every event contract.
fn events(items: &[Json]) -> Result<Vec<EventContract>, DiagnosticSet> {
    if items.len() > version::MAX_CONTRACTS {
        return Err(diagnostic::input_invalid("event-limit", None));
    }
    const KEYS: &[&str] = &[
        "eventVersion",
        "schemaDigest",
        "delivery",
        "ordering",
        "deduplication",
        "correlationId",
        "causationId",
    ];
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let common = common_members(item, KEYS, EVENT_KINDS, diagnostic::EVENT_INVALID)?;
        let event_version = SemVer::parse(string_member(item, "eventVersion", "event-version")?)
            .map_err(|_| diagnostic::input_invalid("event-version", None))?;
        let schema_digest =
            Sha256Digest::parse(string_member(item, "schemaDigest", "schema-digest")?)
                .map_err(|_| diagnostic::input_invalid("schema-digest", None))?;
        let delivery = Delivery::from_key(string_member(item, "delivery", "delivery")?)
            .ok_or_else(|| diagnostic::input_invalid("delivery", None))?;
        let ordering = Ordering::from_key(string_member(item, "ordering", "ordering")?)
            .ok_or_else(|| diagnostic::input_invalid("ordering", None))?;
        let deduplication = deduplication_member(item)?;
        let correlation_id = optional_field(item, "correlationId")?;
        let causation_id = optional_field(item, "causationId")?;
        parsed.push(EventContract {
            common,
            event_version,
            schema_digest,
            delivery,
            ordering,
            deduplication,
            correlation_id,
            causation_id,
        });
    }
    Ok(parsed)
}

/// Parse the deduplication declaration.
fn deduplication_member(object: &Json) -> Result<Deduplication, DiagnosticSet> {
    let deduplication = object
        .get("deduplication")
        .ok_or_else(|| diagnostic::input_invalid("deduplication", None))?;
    closed_keys(deduplication, &["mode", "keyField"], "deduplication")?;
    let mode = DeduplicationMode::from_key(string_member(deduplication, "mode", "dedup-mode")?)
        .ok_or_else(|| diagnostic::input_invalid("dedup-mode", None))?;
    let key_field = optional_field(deduplication, "keyField")?;
    Ok(Deduplication { mode, key_field })
}

/// One optional field-name member.
fn optional_field(object: &Json, key: &str) -> Result<Option<FieldName>, DiagnosticSet> {
    match optional_string(object, key)? {
        None => Ok(None),
        Some(text) => FieldName::new(text)
            .map(Some)
            .ok_or_else(|| diagnostic::input_invalid("field", Some(key))),
    }
}

/// Parse every job contract.
fn jobs(items: &[Json]) -> Result<Vec<JobContract>, DiagnosticSet> {
    if items.len() > version::MAX_CONTRACTS {
        return Err(diagnostic::input_invalid("job-limit", None));
    }
    const KEYS: &[&str] = &[
        "payloadDigest",
        "queueClass",
        "retry",
        "idempotency",
        "timeoutMillis",
        "deadLetter",
    ];
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let common = common_members(item, KEYS, JOB_KINDS, diagnostic::JOB_INVALID)?;
        let payload_digest =
            Sha256Digest::parse(string_member(item, "payloadDigest", "payload-digest")?)
                .map_err(|_| diagnostic::input_invalid("payload-digest", None))?;
        let queue_class = id_member(item, "queueClass", "queue-class", NamespacedId::parse)?;
        let retry = job_retry_member(item)?;
        let idempotency = job_idempotency_member(item)?;
        let timeout_millis = bounded_integer(item, "timeoutMillis", "timeout", 1, 86_400_000)?;
        let dead_letter = DeadLetter::from_key(string_member(item, "deadLetter", "dead-letter")?)
            .ok_or_else(|| diagnostic::input_invalid("dead-letter", None))?;
        parsed.push(JobContract {
            common,
            payload_digest,
            queue_class,
            retry,
            idempotency,
            timeout_millis,
            dead_letter,
        });
    }
    Ok(parsed)
}

/// Parse the job retry declaration.
fn job_retry_member(object: &Json) -> Result<JobRetry, DiagnosticSet> {
    let retry = object
        .get("retry")
        .ok_or_else(|| diagnostic::input_invalid("retry", None))?;
    closed_keys(retry, &["maxAttempts", "backoff", "capMillis"], "retry")?;
    let max_attempts = bounded_integer(retry, "maxAttempts", "max-attempts", 1, 16)? as u8;
    let backoff = Backoff::from_key(string_member(retry, "backoff", "backoff")?)
        .ok_or_else(|| diagnostic::input_invalid("backoff", None))?;
    let cap_millis = match optional_integer(retry, "capMillis")? {
        None => None,
        Some(value) => {
            if value == 0 || value > 3_600_000 {
                return Err(diagnostic::input_invalid("backoff-cap", None));
            }
            Some(value)
        }
    };
    Ok(JobRetry {
        max_attempts,
        backoff,
        cap_millis,
    })
}

/// Parse the job idempotency declaration.
fn job_idempotency_member(object: &Json) -> Result<JobIdempotency, DiagnosticSet> {
    let idempotency = object
        .get("idempotency")
        .ok_or_else(|| diagnostic::input_invalid("idempotency", None))?;
    closed_keys(idempotency, &["mode", "keyField"], "idempotency")?;
    let mode =
        JobIdempotencyMode::from_key(string_member(idempotency, "mode", "idempotency-mode")?)
            .ok_or_else(|| diagnostic::input_invalid("idempotency-mode", None))?;
    let key_field = optional_field(idempotency, "keyField")?;
    Ok(JobIdempotency { mode, key_field })
}

/// Parse every external-call contract.
fn calls(items: &[Json]) -> Result<Vec<CallContract>, DiagnosticSet> {
    if items.len() > version::MAX_CONTRACTS {
        return Err(diagnostic::input_invalid("call-limit", None));
    }
    const KEYS: &[&str] = &[
        "providerContract",
        "requestDigest",
        "responseDigest",
        "errorRefs",
        "timeoutMillis",
        "retry",
        "classification",
        "compensationRef",
    ];
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let common = common_members(item, KEYS, CALL_KINDS, diagnostic::CALL_INVALID)?;
        let provider = ProviderContract::parse(string_member(
            item,
            "providerContract",
            "provider-contract",
        )?)
        .map_err(|_| diagnostic::input_invalid("provider-contract", None))?;
        let request_digest =
            Sha256Digest::parse(string_member(item, "requestDigest", "request-digest")?)
                .map_err(|_| diagnostic::input_invalid("request-digest", None))?;
        let response_digest =
            Sha256Digest::parse(string_member(item, "responseDigest", "response-digest")?)
                .map_err(|_| diagnostic::input_invalid("response-digest", None))?;
        let error_refs = error_refs_member(item)?;
        let timeout_millis = bounded_integer(item, "timeoutMillis", "timeout", 1, 3_600_000)?;
        let retry = call_retry_member(item)?;
        let classification =
            Classification::from_key(string_member(item, "classification", "classification")?)
                .ok_or_else(|| diagnostic::input_invalid("classification", None))?;
        let compensation = match optional_string(item, "compensationRef")? {
            None => None,
            Some(text) => Some(
                OperationId::from_qualified(text)
                    .ok_or_else(|| diagnostic::input_invalid("compensation-ref", None))?,
            ),
        };
        parsed.push(CallContract {
            common,
            provider,
            request_digest,
            response_digest,
            error_refs,
            timeout_millis,
            retry,
            classification,
            compensation,
        });
    }
    Ok(parsed)
}

/// Parse the closed typed error vocabulary.
fn error_refs_member(object: &Json) -> Result<Vec<ErrorRef>, DiagnosticSet> {
    let items = object
        .get("errorRefs")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("error-refs", None))?;
    if items.is_empty() || items.len() > version::MAX_ERROR_REFS {
        return Err(diagnostic::input_invalid("error-ref-limit", None));
    }
    let mut references = Vec::with_capacity(items.len());
    for item in items {
        references.push(
            ErrorRef::parse(
                item.as_str()
                    .ok_or_else(|| diagnostic::input_invalid("error-ref", None))?,
            )
            .map_err(|_| diagnostic::input_invalid("error-ref", None))?,
        );
    }
    references.sort();
    Ok(references)
}

/// Parse the external-call retry declaration.
fn call_retry_member(object: &Json) -> Result<CallRetry, DiagnosticSet> {
    let retry = object
        .get("retry")
        .ok_or_else(|| diagnostic::input_invalid("retry", None))?;
    closed_keys(retry, &["maxAttempts", "safety"], "retry")?;
    let max_attempts = bounded_integer(retry, "maxAttempts", "max-attempts", 1, 16)? as u8;
    let safety = CallSafety::from_key(string_member(retry, "safety", "retry-safety")?)
        .ok_or_else(|| diagnostic::input_invalid("retry-safety", None))?;
    Ok(CallRetry {
        max_attempts,
        safety,
    })
}

/// Parse every cache contract.
fn caches(items: &[Json]) -> Result<Vec<CacheContract>, DiagnosticSet> {
    if items.len() > version::MAX_CONTRACTS {
        return Err(diagnostic::input_invalid("cache-limit", None));
    }
    const KEYS: &[&str] = &["keyContract", "operations", "consistency", "freshness"];
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let common = common_members(item, KEYS, CACHE_KINDS, diagnostic::CACHE_INVALID)?;
        let key = cache_key_member(item)?;
        let operations = cache_operations_member(item)?;
        let consistency = Consistency::from_key(string_member(item, "consistency", "consistency")?)
            .ok_or_else(|| diagnostic::input_invalid("consistency", None))?;
        let freshness = match item.get("freshness") {
            None => None,
            Some(value) => {
                closed_keys(value, &["ttlMillis", "maxStaleMillis"], "freshness")?;
                let ttl_millis = bounded_millis(value, "ttlMillis", 1, 31_536_000_000)?;
                let max_stale_millis = bounded_millis(value, "maxStaleMillis", 1, 31_536_000_000)?;
                if ttl_millis.is_none() && max_stale_millis.is_none() {
                    return Err(diagnostic::input_invalid("freshness", None));
                }
                Some(CacheFreshness {
                    ttl_millis,
                    max_stale_millis,
                })
            }
        };
        parsed.push(CacheContract {
            common,
            key,
            operations,
            consistency,
            freshness,
        });
    }
    Ok(parsed)
}

/// Parse the cache key contract.
fn cache_key_member(object: &Json) -> Result<CacheKeyContract, DiagnosticSet> {
    let key = object
        .get("keyContract")
        .ok_or_else(|| diagnostic::input_invalid("key-contract", None))?;
    closed_keys(key, &["keyVersion", "keyFields"], "key-contract")?;
    let key_version = SemVer::parse(string_member(key, "keyVersion", "key-version")?)
        .map_err(|_| diagnostic::input_invalid("key-version", None))?;
    let fields = key
        .get("keyFields")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("key-fields", None))?;
    if fields.is_empty() || fields.len() > version::MAX_KEY_FIELDS {
        return Err(diagnostic::input_invalid("key-field-limit", None));
    }
    let mut key_fields = Vec::with_capacity(fields.len());
    for field in fields {
        key_fields.push(
            FieldName::new(
                field
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("key-field", None))?,
            )
            .ok_or_else(|| diagnostic::input_invalid("key-field", None))?,
        );
    }
    key_fields.sort();
    Ok(CacheKeyContract {
        key_version,
        key_fields,
    })
}

/// Parse the declared cache operations.
fn cache_operations_member(object: &Json) -> Result<Vec<CacheOperation>, DiagnosticSet> {
    let items = object
        .get("operations")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("cache-operations", None))?;
    if items.is_empty() || items.len() > 3 {
        return Err(diagnostic::input_invalid("cache-operation-limit", None));
    }
    let mut operations = Vec::with_capacity(items.len());
    for item in items {
        operations.push(
            CacheOperation::from_key(
                item.as_str()
                    .ok_or_else(|| diagnostic::input_invalid("cache-operation", None))?,
            )
            .ok_or_else(|| diagnostic::input_invalid("cache-operation", None))?,
        );
    }
    operations.sort_by(|left, right| left.key().cmp(right.key()));
    Ok(operations)
}

/// Parse every publication contract.
fn publications(items: &[Json]) -> Result<Vec<PublicationContract>, DiagnosticSet> {
    if items.len() > version::MAX_CONTRACTS {
        return Err(diagnostic::input_invalid("publication-limit", None));
    }
    const KEYS: &[&str] = &["destination", "consent", "snapshot"];
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let common = common_members(item, KEYS, PUBLICATION_KINDS, diagnostic::INPUT_INVALID)?;
        let destination = id_member(item, "destination", "destination", NamespacedId::parse)?;
        let consent = consent_member(item)?;
        let snapshot = snapshot_member(item)?;
        parsed.push(PublicationContract {
            common,
            destination,
            consent,
            snapshot,
        });
    }
    Ok(parsed)
}

/// Parse the explicit opt-in and approval contract.
fn consent_member(object: &Json) -> Result<Consent, DiagnosticSet> {
    let consent = object
        .get("consent")
        .ok_or_else(|| diagnostic::input_invalid("consent", None))?;
    closed_keys(consent, &["optIn", "approval", "approverRef"], "consent")?;
    match consent.get("optIn") {
        Some(Json::Bool(true)) => {}
        _ => return Err(diagnostic::input_invalid("consent-opt-in", None)),
    }
    let approval = Approval::from_key(string_member(consent, "approval", "approval")?)
        .ok_or_else(|| diagnostic::input_invalid("approval", None))?;
    let approver = match optional_string(consent, "approverRef")? {
        None => None,
        Some(text) => Some(
            NamespacedId::parse(text)
                .map_err(|_| diagnostic::input_invalid("approver-ref", None))?,
        ),
    };
    Ok(Consent { approval, approver })
}

/// Parse the immutable snapshot declaration.
fn snapshot_member(object: &Json) -> Result<Snapshot, DiagnosticSet> {
    let snapshot = object
        .get("snapshot")
        .ok_or_else(|| diagnostic::input_invalid("snapshot", None))?;
    closed_keys(snapshot, &["mode", "revisionField"], "snapshot")?;
    if string_member(snapshot, "mode", "snapshot-mode")? != "revisioned" {
        return Err(diagnostic::input_invalid("snapshot-mode", None));
    }
    let revision_field =
        FieldName::new(string_member(snapshot, "revisionField", "revision-field")?)
            .ok_or_else(|| diagnostic::input_invalid("revision-field", None))?;
    Ok(Snapshot { revision_field })
}

/// Parse every partial-failure case.
fn partial_failure_cases(items: &[Json]) -> Result<Vec<PartialFailureCase>, DiagnosticSet> {
    if items.len() > version::MAX_CASES {
        return Err(diagnostic::input_invalid("case-limit", None));
    }
    const KEYS: &[&str] = &[
        "caseId",
        "scenarioRef",
        "steps",
        "schedule",
        "expectedOutcomes",
        "capabilityRequirementRefs",
    ];
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        closed_keys(item, KEYS, "case")?;
        let case_id = id_member(item, "caseId", "case-id", NamespacedId::parse)?;
        let scenario = scenario_ref_member(item)?;
        let steps = case_steps_member(item)?;
        let schedule = case_schedule_member(item)?;
        let outcomes = case_outcomes_member(item)?;
        let capability_refs = capability_refs_member(item)?;
        parsed.push(PartialFailureCase {
            case_id,
            scenario,
            steps,
            schedule,
            outcomes,
            capability_refs,
        });
    }
    Ok(parsed)
}

/// Parse the typed Scenario IR pin.
fn scenario_ref_member(object: &Json) -> Result<ScenarioRef, DiagnosticSet> {
    let scenario = object
        .get("scenarioRef")
        .ok_or_else(|| diagnostic::input_invalid("scenario-ref", None))?;
    closed_keys(
        scenario,
        &["scenarioId", "scenarioVersion", "irDigest"],
        "scenario-ref",
    )?;
    let scenario_id = SemanticId::parse(string_member(scenario, "scenarioId", "scenario-id")?)
        .map_err(|_| diagnostic::input_invalid("scenario-id", None))?;
    let scenario_version = SemVer::parse(string_member(
        scenario,
        "scenarioVersion",
        "scenario-version",
    )?)
    .map_err(|_| diagnostic::input_invalid("scenario-version", None))?;
    let ir_digest = Sha256Digest::parse(string_member(scenario, "irDigest", "ir-digest")?)
        .map_err(|_| diagnostic::input_invalid("ir-digest", None))?;
    Ok(ScenarioRef {
        scenario_id,
        scenario_version,
        ir_digest,
    })
}

/// Parse the declared steps.
fn case_steps_member(object: &Json) -> Result<Vec<CaseStep>, DiagnosticSet> {
    let items = object
        .get("steps")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("case-steps", None))?;
    if items.is_empty() || items.len() > version::MAX_STEPS {
        return Err(diagnostic::input_invalid("case-step-limit", None));
    }
    let mut steps = Vec::with_capacity(items.len());
    for item in items {
        closed_keys(item, &["stepId", "contractRef", "fault"], "case-step")?;
        let step_id = id_member(item, "stepId", "step-id", StepId::parse)?;
        let contract_ref = id_member(item, "contractRef", "step-contract", NamespacedId::parse)?;
        let fault = Fault::from_key(string_member(item, "fault", "step-fault")?)
            .ok_or_else(|| diagnostic::input_invalid("step-fault", None))?;
        steps.push(CaseStep {
            step_id,
            contract_ref,
            fault,
        });
    }
    steps.sort_by(|left, right| left.step_id.as_str().cmp(right.step_id.as_str()));
    Ok(steps)
}

/// Parse the deterministic schedule.
fn case_schedule_member(object: &Json) -> Result<Vec<CaseNode>, DiagnosticSet> {
    let items = object
        .get("schedule")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("case-schedule", None))?;
    if items.is_empty() || items.len() > version::MAX_SCHEDULE_NODES {
        return Err(diagnostic::input_invalid("case-schedule-limit", None));
    }
    let mut nodes = Vec::with_capacity(items.len());
    for item in items {
        closed_keys(item, &["nodeId", "kind", "stepId", "joins"], "case-node")?;
        let node_id = id_member(item, "nodeId", "node-id", StepId::parse)?;
        let kind = NodeKind::from_key(string_member(item, "kind", "node-kind")?)
            .ok_or_else(|| diagnostic::input_invalid("node-kind", None))?;
        let step_id = match optional_string(item, "stepId")? {
            None => None,
            Some(text) => Some(
                StepId::parse(text).map_err(|_| diagnostic::input_invalid("node-step", None))?,
            ),
        };
        let joins = match item.get("joins") {
            None | Some(Json::Null) => Vec::new(),
            Some(values) => {
                let items = values
                    .as_array()
                    .ok_or_else(|| diagnostic::input_invalid("node-joins", None))?;
                if items.len() > version::MAX_JOINS {
                    return Err(diagnostic::input_invalid("node-join-limit", None));
                }
                let mut joins = Vec::with_capacity(items.len());
                for join in items {
                    joins.push(
                        StepId::parse(
                            join.as_str()
                                .ok_or_else(|| diagnostic::input_invalid("node-join", None))?,
                        )
                        .map_err(|_| diagnostic::input_invalid("node-join", None))?,
                    );
                }
                joins.sort();
                joins
            }
        };
        match kind {
            NodeKind::Barrier if step_id.is_some() => {
                return Err(diagnostic::input_invalid("barrier-step", None));
            }
            NodeKind::Invoke | NodeKind::Fault if step_id.is_none() => {
                return Err(diagnostic::input_invalid("node-step", None));
            }
            _ => {}
        }
        nodes.push(CaseNode {
            node_id,
            kind,
            step_id,
            joins,
        });
    }
    nodes.sort_by(|left, right| left.node_id.as_str().cmp(right.node_id.as_str()));
    Ok(nodes)
}

/// Parse the expected outcomes.
fn case_outcomes_member(object: &Json) -> Result<Vec<ExpectedOutcome>, DiagnosticSet> {
    let items = object
        .get("expectedOutcomes")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("case-outcomes", None))?;
    if items.is_empty() || items.len() > version::MAX_OUTCOMES {
        return Err(diagnostic::input_invalid("case-outcome-limit", None));
    }
    let mut outcomes = Vec::with_capacity(items.len());
    for item in items {
        closed_keys(item, &["contractRef", "outcome"], "expected-outcome")?;
        let contract_ref = id_member(item, "contractRef", "outcome-contract", NamespacedId::parse)?;
        let outcome = Outcome::from_key(string_member(item, "outcome", "outcome")?)
            .ok_or_else(|| diagnostic::input_invalid("outcome", None))?;
        outcomes.push(ExpectedOutcome {
            contract_ref,
            outcome,
        });
    }
    outcomes.sort_by(|left, right| {
        left.contract_ref
            .as_str()
            .cmp(right.contract_ref.as_str())
            .then(left.outcome.key().cmp(right.outcome.key()))
    });
    Ok(outcomes)
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
            id_member(item, "requirementId", "requirement-id", NamespacedId::parse)?;
        let capability =
            Capability::parse(string_member(item, "capability", "requirement-capability")?)
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

/// One bounded integer member.
fn bounded_integer(
    object: &Json,
    key: &str,
    tag: &'static str,
    minimum: u64,
    maximum: u64,
) -> Result<u64, DiagnosticSet> {
    let value = integer_member(object, key, tag)?;
    if value < minimum || value > maximum {
        return Err(diagnostic::input_invalid(tag, None));
    }
    Ok(value)
}

/// One optional bounded millisecond member.
fn bounded_millis(
    object: &Json,
    key: &str,
    minimum: u64,
    maximum: u64,
) -> Result<Option<u64>, DiagnosticSet> {
    match optional_integer(object, key)? {
        None => Ok(None),
        Some(value) if value >= minimum && value <= maximum => Ok(Some(value)),
        Some(_) => Err(diagnostic::input_invalid("freshness-bound", Some(key))),
    }
}
