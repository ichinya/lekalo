//! Wire normalization of the NFR attachment and evidence documents
//! (issue #85).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`NfrAttachment`]; [`evidence_from_value`] is the entry to the
//! typed [`EvidenceSet`]. Both fail closed before any semantic
//! processing: unknown or missing fields, wrong identities, malformed
//! identifiers, digests, dates, bounds, closed-vocabulary violations,
//! and duplicate declarations each return one typed registered
//! diagnostic and no partial value. Dimension/kind coherence,
//! kind/field coherence, comparator/value coherence, the measurable
//! gateRef requirement, and the declaration-only-under-advisory rule
//! are enforced here, mirroring the published schema exactly; the
//! scope resolution against the bound IR is a validation-time concern.

use serde::Serialize;
use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::{NamespacedId, SemanticId};

use super::constraint::{
    CapabilityRequirement, Comparator, Constraint, Dimension, Enforcement, Kind, Measurement,
    Method, Mode, OpenQuestion, Percentile, Requirement, RequirementValue, Resource, RuntimeKind,
    Scope, ScopeKind, Support, Unit, Validity, WindowUnit,
};
use super::diagnostic;
use super::environment::{Environment, OwnerRef, Token};
use super::evidence::EvidenceSet;
use super::evidence::{
    EvidenceResult, MeasuredValue, Measurement as Measured, ResultStatus, ScenarioRef,
};
use super::id::{ConstraintId, Decimal, IsoDate};
use super::version;

/// The closed top-level member set of the attachment.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "constraints",
    "openQuestions",
];

/// The closed constraint member set.
const CONSTRAINT_KEYS: &[&str] = &[
    "constraintId",
    "dimension",
    "kind",
    "scope",
    "requirement",
    "enforcement",
    "measurement",
    "environments",
    "capabilities",
    "validity",
    "sourceRequirement",
];

/// The closed requirement member set.
const REQUIREMENT_KEYS: &[&str] = &[
    "metric",
    "percentile",
    "comparator",
    "value",
    "range",
    "unit",
    "resource",
    "mode",
    "tokens",
    "maxAttempts",
    "window",
    "windowUnit",
    "reference",
];

/// The closed measurement member set.
const MEASUREMENT_KEYS: &[&str] = &["method", "gateRef", "evidenceKinds"];

/// The closed scope member set.
const SCOPE_KEYS: &[&str] = &["kind", "ref"];

/// The closed environment member set.
const ENVIRONMENT_KEYS: &[&str] = &[
    "envId",
    "profileRef",
    "adapterRef",
    "runtime",
    "platform",
    "labels",
];

/// The closed owner-ref member set.
const OWNER_REF_KEYS: &[&str] = &["id", "digest"];

/// The closed validity member set.
const VALIDITY_KEYS: &[&str] = &["revision", "validFrom", "validUntil"];

/// The closed source-requirement member set.
const SOURCE_REQUIREMENT_KEYS: &[&str] = &["source", "requirement"];

/// The closed capability member set.
const CAPABILITY_KEYS: &[&str] = &["id", "minimum"];

/// The closed open-question member set.
const OPEN_QUESTION_KEYS: &[&str] = &["questionId", "relatedConstraint"];

/// The closed top-level member set of the evidence document.
const EVIDENCE_TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "environment",
    "results",
];

/// The closed result member set.
const RESULT_KEYS: &[&str] = &[
    "constraintId",
    "constraintRevision",
    "method",
    "measuredOn",
    "sourceRevision",
    "runRef",
    "gateRef",
    "scenarioRef",
    "measurements",
    "resultStatus",
    "evidenceDigest",
    "expiresOn",
];

/// The closed measured-value member set.
const MEASURED_VALUE_KEYS: &[&str] = &["state", "value"];

/// The closed measurement-row member set.
const MEASUREMENT_ROW_KEYS: &[&str] = &["metric", "percentile", "value", "unit"];

/// The closed scenario-ref member set.
const SCENARIO_REF_KEYS: &[&str] = &["scenarioId", "scenarioVersion", "irDigest"];

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial attachment. Pure: no
/// model, filesystem, cache, report, network, process, or target
/// access of any kind.
pub(super) fn from_value(json: &Json) -> Result<NfrAttachment, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("top-level-shape", None))?;
    check_members(object, TOP_LEVEL_KEYS)?;
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::document_invalid("schema-version", None));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
        return Err(diagnostic::document_invalid("contract-identity", None));
    }
    let attachment_revision = semver(
        object
            .get("attachmentRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("attachment-revision", None))?,
        "attachment-revision",
    )?;
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("project-id", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("project-id", None))?;
    let model_ref = model_pin(
        object
            .get("modelRef")
            .ok_or_else(|| diagnostic::document_invalid("model-ref", None))?,
    )?;
    let ir_ref = ir_pin(
        object
            .get("irRef")
            .ok_or_else(|| diagnostic::document_invalid("ir-ref", None))?,
    )?;
    let constraints = constraints(
        object
            .get("constraints")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("constraint-list", None))?,
    )?;
    let open_questions = open_questions(
        object
            .get("openQuestions")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("open-question-list", None))?,
    )?;
    Ok(NfrAttachment::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_ref,
        constraints,
        open_questions,
    ))
}

/// Normalize one wire document into a validated evidence set.
pub(super) fn evidence_from_value(json: &Json) -> Result<EvidenceSet, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("top-level-shape", None))?;
    check_members(object, EVIDENCE_TOP_LEVEL_KEYS)?;
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::EVIDENCE_SCHEMA_VERSION)
    {
        return Err(diagnostic::document_invalid("schema-version", None));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::EVIDENCE_IDENTITY) {
        return Err(diagnostic::document_invalid("contract-identity", None));
    }
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("project-id", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("project-id", None))?;
    let environment = environment(
        object
            .get("environment")
            .ok_or_else(|| diagnostic::document_invalid("environment", None))?,
    )?;
    let results = results(
        object
            .get("results")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("result-list", None))?,
    )?;
    Ok(EvidenceSet::assemble(project_id, environment, results))
}

/// Reject unknown members and missing required members.
fn check_members(
    object: &serde_json::Map<String, Json>,
    keys: &[&str],
) -> Result<(), DiagnosticSet> {
    for key in object.keys() {
        if !keys.contains(&key.as_str()) {
            return Err(diagnostic::document_invalid("unknown-field", None));
        }
    }
    Ok(())
}

/// The bound Model pin: exact accepted version plus payload digest.
fn model_pin(json: &Json) -> Result<(String, Sha256Digest), DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("model-ref", None))?;
    check_members(object, &["modelVersion", "digest"])?;
    let model_version = object
        .get("modelVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("model-version", None))?;
    if model_version != "0.2.16" {
        return Err(diagnostic::document_invalid("model-version", None));
    }
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("model-digest", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("model-digest", None))?;
    Ok((model_version.to_owned(), digest))
}

/// The bound IR pin: exact accepted version plus canonical IR digest.
fn ir_pin(json: &Json) -> Result<(String, Sha256Digest), DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("ir-ref", None))?;
    check_members(object, &["irVersion", "digest"])?;
    let ir_version = object
        .get("irVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("ir-version", None))?;
    if ir_version != "0.2.16" {
        return Err(diagnostic::document_invalid("ir-version", None));
    }
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("ir-digest", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("ir-digest", None))?;
    Ok((ir_version.to_owned(), digest))
}

/// One canonical SemVer spelling without build metadata.
pub(crate) fn semver(text: &str, detail: &'static str) -> Result<String, DiagnosticSet> {
    let parsed =
        semver::Version::parse(text).map_err(|_| diagnostic::document_invalid(detail, None))?;
    if parsed.build != semver::BuildMetadata::EMPTY || parsed.to_string() != text {
        return Err(diagnostic::document_invalid(detail, None));
    }
    Ok(text.to_owned())
}

/// Parse every constraint and enforce the closed coherence rules.
fn constraints(json: &[Json]) -> Result<Vec<Constraint>, DiagnosticSet> {
    if json.len() > version::MAX_CONSTRAINTS {
        return Err(diagnostic::export_limit(
            "constraints",
            &format!("max={}", version::MAX_CONSTRAINTS),
        ));
    }
    let mut parsed = Vec::new();
    for value in json {
        parsed.push(constraint(value)?);
    }
    parsed.sort_by(|left, right| {
        left.constraint_id()
            .as_str()
            .cmp(right.constraint_id().as_str())
    });
    for pair in parsed.windows(2) {
        if pair[0].constraint_id().as_str() == pair[1].constraint_id().as_str() {
            return Err(diagnostic::document_invalid(
                "duplicate-constraint",
                Some(pair[0].constraint_id().as_str()),
            ));
        }
    }
    Ok(parsed)
}

/// Parse one constraint.
fn constraint(json: &Json) -> Result<Constraint, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("constraint-shape", None))?;
    check_members(object, CONSTRAINT_KEYS)?;
    for required in [
        "constraintId",
        "dimension",
        "kind",
        "scope",
        "requirement",
        "enforcement",
        "measurement",
        "validity",
    ] {
        if !object.contains_key(required) {
            return Err(diagnostic::document_invalid(
                "missing-field",
                Some(required),
            ));
        }
    }
    let constraint_id = ConstraintId::parse(
        object
            .get("constraintId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("constraint-id", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("constraint-id", None))?;
    let dimension = Dimension::parse(
        object
            .get("dimension")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("dimension", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("dimension", None))?;
    let kind = Kind::parse(
        object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("kind", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("kind", None))?;
    // Dimension/kind partition: the two dimensions can never mix.
    if kind.dimension() != dimension {
        return Err(diagnostic::document_invalid("kind-dimension", None));
    }
    let scope = scope(
        object
            .get("scope")
            .ok_or_else(|| diagnostic::document_invalid("scope", None))?,
    )?;
    let requirement = requirement(
        object
            .get("requirement")
            .ok_or_else(|| diagnostic::document_invalid("requirement", None))?,
        kind,
    )?;
    let enforcement = Enforcement::parse(
        object
            .get("enforcement")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("enforcement", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("enforcement", None))?;
    let measurement = measurement(
        object
            .get("measurement")
            .ok_or_else(|| diagnostic::document_invalid("measurement", None))?,
        kind,
        enforcement,
    )?;
    let environments = match object.get("environments") {
        Some(Json::Array(entries)) => {
            if entries.len() > version::MAX_ENVIRONMENTS {
                return Err(diagnostic::export_limit(
                    "environments",
                    &format!("max={}", version::MAX_ENVIRONMENTS),
                ));
            }
            let mut parsed = Vec::new();
            for entry in entries {
                parsed.push(environment(entry)?);
            }
            parsed.sort_by_key(super::environment::Environment::env_key);
            for pair in parsed.windows(2) {
                if pair[0].env_key() == pair[1].env_key() {
                    return Err(diagnostic::document_invalid(
                        "duplicate-environment",
                        Some(pair[0].env_id()),
                    ));
                }
            }
            parsed
        }
        Some(_) => return Err(diagnostic::document_invalid("environment-list", None)),
        None => Vec::new(),
    };
    let capabilities = match object.get("capabilities") {
        Some(Json::Array(entries)) => {
            if entries.len() > version::MAX_CAPABILITIES {
                return Err(diagnostic::export_limit(
                    "capabilities",
                    &format!("max={}", version::MAX_CAPABILITIES),
                ));
            }
            let mut parsed = Vec::new();
            for entry in entries {
                parsed.push(capability(entry)?);
            }
            parsed.sort();
            parsed.dedup();
            parsed
        }
        Some(_) => return Err(diagnostic::document_invalid("capability-list", None)),
        None => Vec::new(),
    };
    let validity = validity(
        object
            .get("validity")
            .ok_or_else(|| diagnostic::document_invalid("validity", None))?,
    )?;
    let source_requirement = match object.get("sourceRequirement") {
        Some(value) => {
            let object = value
                .as_object()
                .ok_or_else(|| diagnostic::document_invalid("source-requirement", None))?;
            check_members(object, SOURCE_REQUIREMENT_KEYS)?;
            let source = object
                .get("source")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("source-id", None))?;
            if !is_source_id(source) {
                return Err(diagnostic::document_invalid("source-id", Some(source)));
            }
            let requirement = object
                .get("requirement")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("requirement-id", None))?;
            if !is_requirement_id(requirement) {
                return Err(diagnostic::document_invalid(
                    "requirement-id",
                    Some(requirement),
                ));
            }
            Some((source.to_owned(), requirement.to_owned()))
        }
        None => None,
    };
    Ok(Constraint::assemble(
        constraint_id,
        kind,
        scope,
        requirement,
        enforcement,
        measurement,
        environments,
        capabilities,
        validity,
        source_requirement,
    ))
}

/// Parse one scope.
fn scope(json: &Json) -> Result<Scope, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("scope", None))?;
    check_members(object, SCOPE_KEYS)?;
    let kind = ScopeKind::parse(
        object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("scope-kind", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("scope-kind", None))?;
    let reference = object
        .get("ref")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("scope-ref", None))?;
    if !crate::trace::id::is_semantic_id(reference) {
        return Err(diagnostic::document_invalid("scope-ref", Some(reference)));
    }
    Ok(Scope::assemble(kind, reference.to_owned()))
}

/// Parse one requirement with the kind-specific coherence rules.
fn requirement(json: &Json, kind: Kind) -> Result<Requirement, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("requirement", None))?;
    check_members(object, REQUIREMENT_KEYS)?;
    let metric = match object.get("metric") {
        Some(value) => Some(
            Token::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("metric", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("metric", None))?,
        ),
        None => None,
    };
    let percentile = match object.get("percentile") {
        Some(value) => Some(
            Percentile::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("percentile", None))?,
            )
            .ok_or_else(|| diagnostic::document_invalid("percentile", None))?,
        ),
        None => None,
    };
    // Percentile is a latency-only member.
    if percentile.is_some() && !matches!(kind, Kind::Runtime(RuntimeKind::Latency)) {
        return Err(diagnostic::document_invalid("percentile-kind", None));
    }
    if matches!(kind, Kind::Runtime(RuntimeKind::Latency)) && percentile.is_none() {
        return Err(diagnostic::document_invalid("percentile-required", None));
    }
    let comparator = match object.get("comparator") {
        Some(value) => Some(
            Comparator::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("comparator", None))?,
            )
            .ok_or_else(|| diagnostic::document_invalid("comparator", None))?,
        ),
        None => None,
    };
    let value = match (object.get("value"), object.get("range")) {
        (Some(_), Some(_)) => {
            return Err(diagnostic::document_invalid("value-range-exclusive", None));
        }
        (Some(value), None) => Some(RequirementValue::Scalar(
            Decimal::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("value", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("value", None))?,
        )),
        (None, Some(range)) => {
            let range = range
                .as_object()
                .ok_or_else(|| diagnostic::document_invalid("range", None))?;
            check_members(range, &["min", "max"])?;
            let min = Decimal::parse(
                range
                    .get("min")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::document_invalid("range-min", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("range-min", None))?;
            let max = Decimal::parse(
                range
                    .get("max")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::document_invalid("range-max", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("range-max", None))?;
            if min.cmp_value(&max) == std::cmp::Ordering::Greater {
                return Err(diagnostic::document_invalid("range-order", None));
            }
            Some(RequirementValue::Range { min, max })
        }
        (None, None) => None,
    };
    // Comparator/value coherence: within/outside take ranges, the rest
    // take scalars; a value or range always carries a comparator.
    match (comparator, value.as_ref()) {
        (Some(Comparator::Within | Comparator::Outside), Some(RequirementValue::Range { .. }))
        | (
            Some(
                Comparator::Lt
                | Comparator::Lte
                | Comparator::Eq
                | Comparator::Gte
                | Comparator::Gt,
            ),
            Some(RequirementValue::Scalar(_)),
        ) => {}
        (None, None) => {}
        _ => return Err(diagnostic::document_invalid("comparator-value", None)),
    }
    let unit = match object.get("unit") {
        Some(value) => Some(
            Unit::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("unit", None))?,
            )
            .ok_or_else(|| diagnostic::document_invalid("unit", None))?,
        ),
        None => None,
    };
    if value.is_some() != unit.is_some() {
        return Err(diagnostic::document_invalid("value-unit", None));
    }
    if value.is_some() && !kind.is_numeric() {
        return Err(diagnostic::document_invalid("numeric-kind", None));
    }
    let resource = match object.get("resource") {
        Some(value) => Some(
            Resource::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("resource", None))?,
            )
            .ok_or_else(|| diagnostic::document_invalid("resource", None))?,
        ),
        None => None,
    };
    if resource.is_some() != matches!(kind, Kind::Runtime(RuntimeKind::ResourceLimit)) {
        return Err(diagnostic::document_invalid("resource-kind", None));
    }
    let mode = match object.get("mode") {
        Some(value) => Some(
            Mode::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("mode", None))?,
            )
            .ok_or_else(|| diagnostic::document_invalid("mode", None))?,
        ),
        None => None,
    };
    let mode_kind = matches!(
        kind,
        Kind::Runtime(RuntimeKind::Consistency) | Kind::Runtime(RuntimeKind::Deployment)
    );
    if mode.is_some() != mode_kind {
        return Err(diagnostic::document_invalid("mode-kind", None));
    }
    // Consistency modes are the state words; deployment modes are the
    // placement words; neither kind accepts the other's.
    if let Some(mode) = mode {
        let legal = match kind {
            Kind::Runtime(RuntimeKind::Consistency) => {
                matches!(mode, Mode::Strong | Mode::Bounded | Mode::StaleOk)
            }
            _ => matches!(mode, Mode::Platform | Mode::Topology | Mode::Region),
        };
        if !legal {
            return Err(diagnostic::document_invalid("mode-kind", None));
        }
    }
    let tokens = match object.get("tokens") {
        Some(Json::Array(entries)) => {
            if entries.is_empty() || entries.len() > 64 {
                return Err(diagnostic::document_invalid("tokens", None));
            }
            let mut parsed = Vec::new();
            for entry in entries {
                parsed.push(
                    Token::parse(
                        entry
                            .as_str()
                            .ok_or_else(|| diagnostic::document_invalid("tokens", None))?,
                    )
                    .map_err(|_| diagnostic::document_invalid("tokens", None))?,
                );
            }
            parsed.sort();
            parsed.dedup();
            parsed
        }
        Some(_) => return Err(diagnostic::document_invalid("tokens", None)),
        None => Vec::new(),
    };
    let token_kind = matches!(
        kind,
        Kind::Runtime(RuntimeKind::Deployment) | Kind::Runtime(RuntimeKind::RuntimeConstraint)
    );
    if (!tokens.is_empty()) != token_kind {
        return Err(diagnostic::document_invalid("tokens-kind", None));
    }
    let max_attempts = match object.get("maxAttempts") {
        Some(value) => Some(
            Decimal::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("max-attempts", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("max-attempts", None))?,
        ),
        None => None,
    };
    let window = match object.get("window") {
        Some(value) => {
            let amount = Decimal::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("window", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("window", None))?;
            let unit = WindowUnit::parse(
                object
                    .get("windowUnit")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::document_invalid("window-unit", None))?,
            )
            .ok_or_else(|| diagnostic::document_invalid("window-unit", None))?;
            Some((amount, unit))
        }
        None => None,
    };
    if object.contains_key("window") != object.contains_key("windowUnit") {
        return Err(diagnostic::document_invalid("window-unit", None));
    }
    let retry_kind = matches!(kind, Kind::Runtime(RuntimeKind::RetryBudget));
    let retry_members = max_attempts.is_some() || window.is_some();
    if retry_members != retry_kind {
        return Err(diagnostic::document_invalid("retry-kind", None));
    }
    let reference = match object.get("reference") {
        Some(value) => {
            Some(owner_ref(value).map_err(|_| diagnostic::document_invalid("reference", None))?)
        }
        None => None,
    };
    if reference.is_some() != kind.is_reference() {
        return Err(diagnostic::document_invalid("reference-kind", None));
    }
    Ok(Requirement::assemble(
        metric,
        percentile,
        comparator,
        value,
        unit,
        resource,
        mode,
        tokens,
        max_attempts,
        window,
        reference,
    ))
}

/// Parse one measurement with the gate and declaration coherence
/// rules.
fn measurement(
    json: &Json,
    kind: Kind,
    enforcement: Enforcement,
) -> Result<Measurement, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("measurement", None))?;
    check_members(object, MEASUREMENT_KEYS)?;
    let method = Method::parse(
        object
            .get("method")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("method", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("method", None))?;
    // A declaration without measurement is unverified by design; it
    // may never pretend to be a mandatory gate.
    if method == Method::Declaration && enforcement == Enforcement::Mandatory {
        return Err(diagnostic::document_invalid("declaration-mandatory", None));
    }
    let gate_ref = match object.get("gateRef") {
        Some(value) => Some(
            NamespacedId::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("gate-ref", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("gate-ref", None))?,
        ),
        None => None,
    };
    if kind.requires_gate_ref() != gate_ref.is_some() {
        return Err(diagnostic::document_invalid("gate-ref", None));
    }
    let evidence_kinds = match object.get("evidenceKinds") {
        Some(Json::Array(entries)) => {
            if entries.is_empty() || entries.len() > 16 {
                return Err(diagnostic::document_invalid("evidence-kinds", None));
            }
            let mut parsed = Vec::new();
            for entry in entries {
                let kind = Method::parse(
                    entry
                        .as_str()
                        .ok_or_else(|| diagnostic::document_invalid("evidence-kinds", None))?,
                )
                .ok_or_else(|| diagnostic::document_invalid("evidence-kinds", None))?;
                if !parsed.contains(&kind) {
                    parsed.push(kind);
                }
            }
            parsed.sort();
            parsed
        }
        Some(_) => return Err(diagnostic::document_invalid("evidence-kinds", None)),
        None => Vec::new(),
    };
    if !evidence_kinds.is_empty() && !evidence_kinds.contains(&method) {
        return Err(diagnostic::document_invalid("evidence-kinds", None));
    }
    Ok(Measurement::assemble(method, gate_ref, evidence_kinds))
}

/// Parse one environment record.
fn environment(json: &Json) -> Result<Environment, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("environment", None))?;
    check_members(object, ENVIRONMENT_KEYS)?;
    for required in ["envId", "profileRef", "adapterRef", "runtime", "platform"] {
        if !object.contains_key(required) {
            return Err(diagnostic::document_invalid(
                "missing-field",
                Some(required),
            ));
        }
    }
    let env_id = Token::parse(
        object
            .get("envId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("env-id", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("env-id", None))?;
    let profile_ref = owner_ref(
        object
            .get("profileRef")
            .ok_or_else(|| diagnostic::document_invalid("profile-ref", None))?,
    )?;
    let adapter_ref = owner_ref(
        object
            .get("adapterRef")
            .ok_or_else(|| diagnostic::document_invalid("adapter-ref", None))?,
    )?;
    let runtime = Token::parse(
        object
            .get("runtime")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("runtime", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("runtime", None))?;
    let platform = Token::parse(
        object
            .get("platform")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("platform", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("platform", None))?;
    let labels = match object.get("labels") {
        Some(Json::Array(entries)) => {
            let mut parsed = Vec::new();
            for entry in entries {
                parsed.push(
                    Token::parse(
                        entry
                            .as_str()
                            .ok_or_else(|| diagnostic::document_invalid("labels", None))?,
                    )
                    .map_err(|_| diagnostic::document_invalid("labels", None))?,
                );
            }
            parsed
        }
        Some(_) => return Err(diagnostic::document_invalid("labels", None)),
        None => Vec::new(),
    };
    Ok(Environment::assemble(
        env_id,
        profile_ref,
        adapter_ref,
        runtime,
        platform,
        labels,
    ))
}

/// Parse one owner-held reference.
fn owner_ref(json: &Json) -> Result<OwnerRef, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("owner-ref", None))?;
    check_members(object, OWNER_REF_KEYS)?;
    let id = NamespacedId::parse(
        object
            .get("id")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("owner-ref-id", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("owner-ref-id", None))?;
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("owner-ref-digest", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("owner-ref-digest", None))?;
    Ok(OwnerRef::new(id, digest))
}

/// Parse one validity window.
fn validity(json: &Json) -> Result<Validity, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("validity", None))?;
    check_members(object, VALIDITY_KEYS)?;
    let revision = semver(
        object
            .get("revision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("revision", None))?,
        "revision",
    )?;
    let valid_from = match object.get("validFrom") {
        Some(value) => Some(
            IsoDate::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("valid-from", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("valid-from", None))?,
        ),
        None => None,
    };
    let valid_until = match object.get("validUntil") {
        Some(value) => Some(
            IsoDate::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("valid-until", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("valid-until", None))?,
        ),
        None => None,
    };
    if valid_from.is_some() && valid_until.is_none() {
        return Err(diagnostic::document_invalid("valid-until", None));
    }
    if let (Some(from), Some(until)) = (valid_from.as_ref(), valid_until.as_ref()) {
        if from > until {
            return Err(diagnostic::document_invalid("validity-order", None));
        }
    }
    Ok(Validity::assemble(revision, valid_from, valid_until))
}

/// Parse one capability requirement.
fn capability(json: &Json) -> Result<CapabilityRequirement, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("capability", None))?;
    check_members(object, CAPABILITY_KEYS)?;
    let id = object
        .get("id")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("capability-id", None))?;
    let bytes = id.as_bytes();
    let legal = (3..=64).contains(&id.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[1..].iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-' || *byte == b'.'
        })
        && id.contains('.')
        && !id.contains("..")
        && !id.ends_with('.');
    if !legal {
        return Err(diagnostic::document_invalid("capability-id", Some(id)));
    }
    let minimum = Support::parse(
        object
            .get("minimum")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("capability-minimum", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("capability-minimum", None))?;
    Ok(CapabilityRequirement::assemble(id.to_owned(), minimum))
}

/// Parse every open question.
fn open_questions(json: &[Json]) -> Result<Vec<OpenQuestion>, DiagnosticSet> {
    if json.len() > version::MAX_OPEN_QUESTIONS {
        return Err(diagnostic::export_limit(
            "open-questions",
            &format!("max={}", version::MAX_OPEN_QUESTIONS),
        ));
    }
    let mut parsed = Vec::new();
    for value in json {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("open-question", None))?;
        check_members(object, OPEN_QUESTION_KEYS)?;
        let question_id = ConstraintId::parse(
            object
                .get("questionId")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("question-id", None))?,
        )
        .map_err(|_| diagnostic::document_invalid("question-id", None))?;
        let related_constraint = match object.get("relatedConstraint") {
            Some(value) => Some(
                ConstraintId::parse(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::document_invalid("related-constraint", None))?,
                )
                .map_err(|_| diagnostic::document_invalid("related-constraint", None))?,
            ),
            None => None,
        };
        parsed.push(OpenQuestion::assemble(question_id, related_constraint));
    }
    parsed.sort_by(|left, right| left.question_id().cmp(right.question_id()));
    for pair in parsed.windows(2) {
        if pair[0].question_id() == pair[1].question_id() {
            return Err(diagnostic::document_invalid(
                "duplicate-question",
                Some(pair[0].question_id().as_str()),
            ));
        }
    }
    Ok(parsed)
}

/// Parse every evidence result.
fn results(json: &[Json]) -> Result<Vec<EvidenceResult>, DiagnosticSet> {
    if json.len() > version::MAX_RESULTS {
        return Err(diagnostic::export_limit(
            "results",
            &format!("max={}", version::MAX_RESULTS),
        ));
    }
    let mut parsed = Vec::new();
    for value in json {
        parsed.push(result(value)?);
    }
    parsed.sort_by(|left, right| {
        (
            left.constraint_id().as_str(),
            left.constraint_revision(),
            left.method().as_str(),
            left.measured_on().as_str(),
            left.evidence_digest(),
        )
            .cmp(&(
                right.constraint_id().as_str(),
                right.constraint_revision(),
                right.method().as_str(),
                right.measured_on().as_str(),
                right.evidence_digest(),
            ))
    });
    for pair in parsed.windows(2) {
        if pair[0].constraint_id() == pair[1].constraint_id()
            && pair[0].constraint_revision() == pair[1].constraint_revision()
            && pair[0].method() == pair[1].method()
            && pair[0].measured_on() == pair[1].measured_on()
            && pair[0].evidence_digest() == pair[1].evidence_digest()
        {
            return Err(diagnostic::document_invalid(
                "duplicate-result",
                Some(pair[0].constraint_id().as_str()),
            ));
        }
    }
    Ok(parsed)
}

/// Parse one evidence result.
fn result(json: &Json) -> Result<EvidenceResult, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("result-shape", None))?;
    check_members(object, RESULT_KEYS)?;
    for required in [
        "constraintId",
        "constraintRevision",
        "method",
        "measuredOn",
        "sourceRevision",
        "measurements",
        "resultStatus",
        "evidenceDigest",
    ] {
        if !object.contains_key(required) {
            return Err(diagnostic::document_invalid(
                "missing-field",
                Some(required),
            ));
        }
    }
    let constraint_id = ConstraintId::parse(
        object
            .get("constraintId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("constraint-id", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("constraint-id", None))?;
    let constraint_revision = semver(
        object
            .get("constraintRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("constraint-revision", None))?,
        "constraint-revision",
    )?;
    let method = Method::parse(
        object
            .get("method")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("method", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("method", None))?;
    let measured_on = IsoDate::parse(
        object
            .get("measuredOn")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("measured-on", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("measured-on", None))?;
    let source_revision = revision_token(
        object
            .get("sourceRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("source-revision", None))?,
    )?;
    let run_ref = match object.get("runRef") {
        Some(value) => Some(
            NamespacedId::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("run-ref", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("run-ref", None))?,
        ),
        None => None,
    };
    let gate_ref = match object.get("gateRef") {
        Some(value) => Some(
            NamespacedId::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("gate-ref", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("gate-ref", None))?,
        ),
        None => None,
    };
    let scenario_ref = match object.get("scenarioRef") {
        Some(value) => {
            let object = value
                .as_object()
                .ok_or_else(|| diagnostic::document_invalid("scenario-ref", None))?;
            check_members(object, SCENARIO_REF_KEYS)?;
            let scenario_id = object
                .get("scenarioId")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("scenario-id", None))?;
            if !crate::trace::id::is_semantic_id(scenario_id) {
                return Err(diagnostic::document_invalid(
                    "scenario-id",
                    Some(scenario_id),
                ));
            }
            let scenario_version = revision_token(
                object
                    .get("scenarioVersion")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::document_invalid("scenario-version", None))?,
            )?;
            let ir_digest = Sha256Digest::parse(
                object
                    .get("irDigest")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::document_invalid("ir-digest", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("ir-digest", None))?;
            Some(ScenarioRef::assemble(
                scenario_id.to_owned(),
                scenario_version,
                ir_digest,
            ))
        }
        None => None,
    };
    let measurements = match object.get("measurements") {
        Some(Json::Array(entries)) => {
            if entries.is_empty() || entries.len() > version::MAX_MEASUREMENTS {
                return Err(diagnostic::document_invalid("measurements", None));
            }
            let mut parsed = Vec::new();
            for entry in entries {
                parsed.push(measurement_row(entry)?);
            }
            parsed
        }
        _ => return Err(diagnostic::document_invalid("measurements", None)),
    };
    let result_status = ResultStatus::parse(
        object
            .get("resultStatus")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("result-status", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("result-status", None))?;
    let evidence_digest = Sha256Digest::parse(
        object
            .get("evidenceDigest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("evidence-digest", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("evidence-digest", None))?;
    let expires_on = match object.get("expiresOn") {
        Some(value) => Some(
            IsoDate::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("expires-on", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("expires-on", None))?,
        ),
        None => None,
    };
    Ok(EvidenceResult::assemble(
        constraint_id,
        constraint_revision,
        method,
        measured_on,
        source_revision,
        run_ref,
        gate_ref,
        scenario_ref,
        measurements,
        result_status,
        evidence_digest,
        expires_on,
    ))
}

/// Parse one measured value with the valueState rules: a known value
/// carries the number, every absence carries none.
fn measured_value(json: &Json) -> Result<MeasuredValue, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("measured-value", None))?;
    check_members(object, MEASURED_VALUE_KEYS)?;
    let state = object
        .get("state")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("value-state", None))?;
    let has_value = object.contains_key("value");
    match state {
        "known" => {
            if !has_value {
                return Err(diagnostic::document_invalid("value-required", None));
            }
            let value = Decimal::parse(
                object
                    .get("value")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::document_invalid("value", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("value", None))?;
            Ok(MeasuredValue::Known(value))
        }
        "unknown" | "unsupported" | "withheld" => {
            if has_value {
                return Err(diagnostic::document_invalid("value-forbidden", None));
            }
            Ok(match state {
                "unknown" => MeasuredValue::Unknown,
                "unsupported" => MeasuredValue::Unsupported,
                _ => MeasuredValue::Withheld,
            })
        }
        _ => Err(diagnostic::document_invalid("value-state", None)),
    }
}

/// Parse one measurement row.
fn measurement_row(json: &Json) -> Result<Measured, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("measurement-row", None))?;
    check_members(object, MEASUREMENT_ROW_KEYS)?;
    let metric = match object.get("metric") {
        Some(value) => value
            .as_str()
            .ok_or_else(|| diagnostic::document_invalid("metric", None))?,
        None => {
            return Err(diagnostic::document_invalid(
                "missing-field",
                Some("metric"),
            ))
        }
    };
    if metric.is_empty()
        || metric.len() > 64
        || !metric
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        || !metric
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(diagnostic::document_invalid("metric", Some(metric)));
    }
    let percentile = match object.get("percentile") {
        Some(value) => Some(
            Percentile::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("percentile", None))?,
            )
            .ok_or_else(|| diagnostic::document_invalid("percentile", None))?,
        ),
        None => None,
    };
    let value = measured_value(
        object
            .get("value")
            .ok_or_else(|| diagnostic::document_invalid("measured-value", None))?,
    )?;
    let unit = match object.get("unit") {
        Some(value) => value
            .as_str()
            .ok_or_else(|| diagnostic::document_invalid("unit", None))?,
        None => return Err(diagnostic::document_invalid("missing-field", Some("unit"))),
    };
    if unit.is_empty() || unit.len() > 64 {
        return Err(diagnostic::document_invalid("unit", None));
    }
    Ok(Measured::assemble(
        metric.to_owned(),
        percentile,
        value,
        unit.to_owned(),
    ))
}

/// One bounded owner revision token (`1`, `2026.1`, `run-42`).
fn revision_token(text: &str) -> Result<String, DiagnosticSet> {
    let ok = !text.is_empty()
        && text.len() <= 128
        && text
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
        });
    if !ok {
        return Err(diagnostic::document_invalid("source-revision", None));
    }
    Ok(text.to_owned())
}

/// Whether `text` is a legal source namespace id (the #36 grammar).
fn is_source_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && rest.len() <= 31
        && rest
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// Whether `text` is a legal requirement id (the #36 grammar).
fn is_requirement_id(text: &str) -> bool {
    let Some((capability, slug)) = text.split_once(".REQ-") else {
        return false;
    };
    let ok_part = |bytes: &[u8]| {
        !bytes.is_empty()
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    };
    text.len() <= 128
        && capability.len() <= 63
        && slug.len() <= 64
        && capability
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && slug
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && ok_part(capability.as_bytes())
        && ok_part(slug.as_bytes())
}

/// The finished NFR attachment: immutable, deterministically ordered,
/// and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NfrAttachment {
    attachment_revision: String,
    project_id: SemanticId,
    model_ref: (String, Sha256Digest),
    ir_ref: (String, Sha256Digest),
    constraints: Vec<Constraint>,
    open_questions: Vec<OpenQuestion>,
}

impl NfrAttachment {
    /// Assemble from validated parts (wire internal); collections are
    /// stored in the caller's canonical order.
    pub(crate) fn assemble(
        attachment_revision: String,
        project_id: SemanticId,
        model_ref: (String, Sha256Digest),
        ir_ref: (String, Sha256Digest),
        constraints: Vec<Constraint>,
        open_questions: Vec<OpenQuestion>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_ref,
            constraints,
            open_questions,
        }
    }

    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set.
    pub fn from_value(json: &Json) -> Result<Self, DiagnosticSet> {
        from_value(json)
    }

    /// Parse one attachment document (exact UTF-8 JSON bytes) or
    /// return the terminal rejection set.
    pub fn parse(bytes: &[u8]) -> Result<Self, DiagnosticSet> {
        if bytes.len() > version::MAX_DOC_BYTES {
            return Err(diagnostic::export_limit(
                "document-bytes",
                &format!("max={}", version::MAX_DOC_BYTES),
            ));
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| diagnostic::io_failure("invalid-encoding"))?;
        let json = super::json::parse(text)?;
        from_value(&json)
    }

    /// The exact attachment revision.
    pub fn attachment_revision(&self) -> &str {
        &self.attachment_revision
    }

    /// The stable project identity.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The bound Model pin `(modelVersion, digest)`.
    pub const fn model_ref(&self) -> &(String, Sha256Digest) {
        &self.model_ref
    }

    /// The bound IR pin `(irVersion, digest)`.
    pub const fn ir_ref(&self) -> &(String, Sha256Digest) {
        &self.ir_ref
    }

    /// Every constraint, canonically ordered by constraint id.
    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    /// Every registered open question, canonically ordered.
    pub fn open_questions(&self) -> &[OpenQuestion] {
        &self.open_questions
    }
}

/// The canonical wire view of the attachment: borrowed strings in the
/// closed member set, serialized with byte-sorted object keys by the
/// canonical writer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AttachmentWire<'a> {
    schema_version: &'static str,
    identity: &'static str,
    attachment_revision: &'a str,
    project_id: &'a str,
    model_ref: ModelRefWire<'a>,
    ir_ref: IrRefWire<'a>,
    constraints: Vec<ConstraintWire<'a>>,
    open_questions: Vec<OpenQuestionWire<'a>>,
}

/// The canonical wire view of the Model pin.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelRefWire<'a> {
    model_version: &'a str,
    digest: &'a str,
}

/// The canonical wire view of the IR pin.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IrRefWire<'a> {
    ir_version: &'a str,
    digest: &'a str,
}

/// The canonical wire view of one constraint.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConstraintWire<'a> {
    constraint_id: &'a str,
    dimension: &'static str,
    kind: &'static str,
    scope: ScopeWire<'a>,
    requirement: RequirementWire<'a>,
    enforcement: &'static str,
    measurement: MeasurementWire<'a>,
    environments: Vec<EnvironmentWire<'a>>,
    capabilities: Vec<CapabilityWire<'a>>,
    validity: ValidityWire<'a>,
    source_requirement: Option<SourceRequirementWire<'a>>,
}

/// The canonical wire view of one scope.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeWire<'a> {
    kind: &'static str,
    r#ref: &'a str,
}

/// The canonical wire view of one requirement.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RequirementWire<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    metric: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    percentile: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comparator: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    range: Option<RangeWire<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tokens: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_attempts: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_unit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<OwnerRefWire<'a>>,
}

/// The canonical wire view of one range.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeWire<'a> {
    min: &'a str,
    max: &'a str,
}

/// The canonical wire view of one owner reference.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OwnerRefWire<'a> {
    id: &'a str,
    digest: &'a str,
}

/// The canonical wire view of one measurement.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasurementWire<'a> {
    method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    gate_ref: Option<&'a str>,
    evidence_kinds: Vec<&'static str>,
}

/// The canonical wire view of one environment.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentWire<'a> {
    env_id: &'a str,
    profile_ref: OwnerRefWire<'a>,
    adapter_ref: OwnerRefWire<'a>,
    runtime: &'a str,
    platform: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    labels: Vec<&'a str>,
}

/// The canonical wire view of one capability requirement.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CapabilityWire<'a> {
    id: &'a str,
    minimum: &'static str,
}

/// The canonical wire view of one validity window.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidityWire<'a> {
    revision: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    valid_from: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    valid_until: Option<&'a str>,
}

/// The canonical wire view of one source requirement.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceRequirementWire<'a> {
    source: &'a str,
    requirement: &'a str,
}

/// The canonical wire view of one open question.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenQuestionWire<'a> {
    question_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    related_constraint: Option<&'a str>,
}

impl NfrAttachment {
    /// The canonical wire view of this attachment.
    pub(super) fn wire(&self) -> AttachmentWire<'_> {
        AttachmentWire {
            schema_version: version::SCHEMA_VERSION,
            identity: version::IDENTITY,
            attachment_revision: &self.attachment_revision,
            project_id: self.project_id.as_str(),
            model_ref: ModelRefWire {
                model_version: &self.model_ref.0,
                digest: self.model_ref.1.as_str(),
            },
            ir_ref: IrRefWire {
                ir_version: &self.ir_ref.0,
                digest: self.ir_ref.1.as_str(),
            },
            constraints: self
                .constraints
                .iter()
                .map(|constraint| {
                    let requirement = constraint.requirement();
                    let value = requirement.value();
                    let window = requirement.window();
                    ConstraintWire {
                        constraint_id: constraint.constraint_id().as_str(),
                        dimension: constraint.dimension().as_str(),
                        kind: constraint.kind().as_str(),
                        scope: ScopeWire {
                            kind: constraint.scope().kind().as_str(),
                            r#ref: constraint.scope().reference(),
                        },
                        requirement: RequirementWire {
                            metric: requirement.metric(),
                            percentile: requirement.percentile().map(Percentile::as_str),
                            comparator: requirement.comparator().map(Comparator::as_str),
                            value: match value {
                                Some(RequirementValue::Scalar(decimal)) => Some(decimal.as_str()),
                                _ => None,
                            },
                            range: match value {
                                Some(RequirementValue::Range { min, max }) => Some(RangeWire {
                                    min: min.as_str(),
                                    max: max.as_str(),
                                }),
                                _ => None,
                            },
                            unit: requirement.unit().map(Unit::as_str),
                            resource: requirement.resource().map(Resource::as_str),
                            mode: requirement.mode().map(Mode::as_str),
                            tokens: requirement.tokens().iter().map(Token::as_str).collect(),
                            max_attempts: requirement.max_attempts().map(Decimal::as_str),
                            window: window.map(|(amount, _)| amount.as_str()),
                            window_unit: window.map(|(_, unit)| unit.as_str()),
                            reference: requirement.reference().map(|reference| OwnerRefWire {
                                id: reference.id(),
                                digest: reference.digest(),
                            }),
                        },
                        enforcement: constraint.enforcement().as_str(),
                        measurement: MeasurementWire {
                            method: constraint.measurement().method().as_str(),
                            gate_ref: constraint
                                .measurement()
                                .gate_ref()
                                .map(NamespacedId::as_str),
                            evidence_kinds: constraint
                                .measurement()
                                .evidence_kinds()
                                .iter()
                                .map(|method| method.as_str())
                                .collect(),
                        },
                        environments: constraint
                            .environments()
                            .iter()
                            .map(|environment| EnvironmentWire {
                                env_id: environment.env_id(),
                                profile_ref: OwnerRefWire {
                                    id: environment.profile_ref().id(),
                                    digest: environment.profile_ref().digest(),
                                },
                                adapter_ref: OwnerRefWire {
                                    id: environment.adapter_ref().id(),
                                    digest: environment.adapter_ref().digest(),
                                },
                                runtime: environment.runtime(),
                                platform: environment.platform(),
                                labels: environment.labels().iter().map(Token::as_str).collect(),
                            })
                            .collect(),
                        capabilities: constraint
                            .capabilities()
                            .iter()
                            .map(|capability| CapabilityWire {
                                id: capability.id(),
                                minimum: capability.minimum().as_str(),
                            })
                            .collect(),
                        validity: ValidityWire {
                            revision: constraint.validity().revision(),
                            valid_from: constraint.validity().valid_from().map(IsoDate::as_str),
                            valid_until: constraint.validity().valid_until().map(IsoDate::as_str),
                        },
                        source_requirement: constraint.source_requirement().map(
                            |(source, requirement)| SourceRequirementWire {
                                source,
                                requirement,
                            },
                        ),
                    }
                })
                .collect(),
            open_questions: self
                .open_questions
                .iter()
                .map(|question| OpenQuestionWire {
                    question_id: question.question_id().as_str(),
                    related_constraint: question.related_constraint().map(ConstraintId::as_str),
                })
                .collect(),
        }
    }
}

/// The canonical wire view of one evidence set.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EvidenceWire<'a> {
    schema_version: &'static str,
    identity: &'static str,
    project_id: &'a str,
    environment: EnvironmentWire<'a>,
    results: Vec<ResultWire<'a>>,
}

/// The canonical wire view of one measured value.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredValueWire<'a> {
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
}

/// The canonical wire view of one measurement row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasurementRowWire<'a> {
    metric: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    percentile: Option<&'static str>,
    value: MeasuredValueWire<'a>,
    unit: &'a str,
}

/// The canonical wire view of one scenario reference.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioRefWire<'a> {
    scenario_id: &'a str,
    scenario_version: &'a str,
    ir_digest: &'a str,
}

/// The canonical wire view of one result.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultWire<'a> {
    constraint_id: &'a str,
    constraint_revision: &'a str,
    method: &'static str,
    measured_on: &'a str,
    source_revision: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_ref: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gate_ref: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario_ref: Option<ScenarioRefWire<'a>>,
    measurements: Vec<MeasurementRowWire<'a>>,
    result_status: &'static str,
    evidence_digest: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_on: Option<&'a str>,
}

impl EvidenceSet {
    /// The canonical wire view of this evidence set.
    pub(super) fn evidence_wire(&self) -> EvidenceWire<'_> {
        EvidenceWire {
            schema_version: version::EVIDENCE_SCHEMA_VERSION,
            identity: version::EVIDENCE_IDENTITY,
            project_id: self.project_id().as_str(),
            environment: EnvironmentWire {
                env_id: self.environment().env_id(),
                profile_ref: OwnerRefWire {
                    id: self.environment().profile_ref().id(),
                    digest: self.environment().profile_ref().digest(),
                },
                adapter_ref: OwnerRefWire {
                    id: self.environment().adapter_ref().id(),
                    digest: self.environment().adapter_ref().digest(),
                },
                runtime: self.environment().runtime(),
                platform: self.environment().platform(),
                labels: self
                    .environment()
                    .labels()
                    .iter()
                    .map(Token::as_str)
                    .collect(),
            },
            results: self
                .results()
                .iter()
                .map(|result| ResultWire {
                    constraint_id: result.constraint_id().as_str(),
                    constraint_revision: result.constraint_revision(),
                    method: result.method().as_str(),
                    measured_on: result.measured_on().as_str(),
                    source_revision: result.source_revision(),
                    run_ref: result.run_ref().map(NamespacedId::as_str),
                    gate_ref: result.gate_ref().map(NamespacedId::as_str),
                    scenario_ref: result.scenario_ref().map(|scenario| ScenarioRefWire {
                        scenario_id: scenario.scenario_id(),
                        scenario_version: scenario.scenario_version(),
                        ir_digest: scenario.ir_digest(),
                    }),
                    measurements: result
                        .measurements()
                        .iter()
                        .map(|measurement| MeasurementRowWire {
                            metric: measurement.metric(),
                            percentile: measurement.percentile().map(Percentile::as_str),
                            value: MeasuredValueWire {
                                state: measurement.value().state(),
                                value: measurement.value().known().map(Decimal::as_str),
                            },
                            unit: measurement.unit(),
                        })
                        .collect(),
                    result_status: result.result_status().as_str(),
                    evidence_digest: result.evidence_digest(),
                    expires_on: result.expires_on().map(IsoDate::as_str),
                })
                .collect(),
        }
    }
}

/// Normalize one wire document into a validated evidence set, or
/// return the typed rejection set.
impl EvidenceSet {
    /// Normalize one wire document into a validated evidence set.
    pub fn from_value(json: &Json) -> Result<Self, DiagnosticSet> {
        evidence_from_value(json)
    }

    /// Parse one evidence document (exact UTF-8 JSON bytes) or return
    /// the terminal rejection set.
    pub fn parse(bytes: &[u8]) -> Result<Self, DiagnosticSet> {
        if bytes.len() > version::MAX_DOC_BYTES {
            return Err(diagnostic::export_limit(
                "document-bytes",
                &format!("max={}", version::MAX_DOC_BYTES),
            ));
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| diagnostic::io_failure("invalid-encoding"))?;
        let json = super::json::parse(text)?;
        evidence_from_value(&json)
    }
}
