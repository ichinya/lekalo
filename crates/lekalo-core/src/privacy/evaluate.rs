//! The Rust evaluator: the exact port of the frozen #120 decision
//! semantics (issue #119, plan S2).
//!
//! [`evaluate_decision`] ports `evaluateDecision` from
//! `scripts/check-privacy.mjs` rule for rule: the validator
//! (`validate_decision_input`), the authorizing-evidence semantics
//! (`authorizing_evidence_semantic_error`), the context coherence
//! checks, the derived-artifact validation, the path and value rules,
//! and the disposition/destination/sensitivity/constraint precedence —
//! in the exact order, with the exact reason-code spellings. The
//! reason strings are the contract.
//!
//! Pure metadata evaluation: the function reads only the declared
//! input and the custody-verified [`TrustedContext`]; it performs no
//! filesystem, network, or process access.

use serde_json::{Map, Value as Json};

use super::canonical::{canonical, compare_unicode_code_points};
use super::context::{same_object, TrustedContext};
use super::input::ExportDecisionInput;
use super::output::{ExportDecisionOutput, DECISION_VERSION};
use super::refs;
use super::types::{EvidenceSpec, ValueState};
use super::vocab::{
    Audience, ConflictState, ConstraintScope, DataSensitivity, ExportDisposition, OperationId,
    ProvenanceOrigin, RepositoryRelation, RepositoryRole, SensitivityState, TenantRelation,
    TransformId, TrustBoundary,
};
use unicode_normalization::UnicodeNormalization;

/// One evaluated decision: the malformed flag (which selects the
/// separate exit-1 protocol) and the exact closed decision output.
#[derive(Clone, Debug, PartialEq)]
pub struct DecisionEvaluation {
    /// Whether the input was shape-valid but semantically malformed;
    /// the output still denies.
    pub malformed: bool,
    /// The exact decision output.
    pub output: ExportDecisionOutput,
}

// ---------------------------------------------------------------------------
// Closed member sets and primitive shapes (the reference helpers).
// ---------------------------------------------------------------------------

/// Whether `value` is an object whose key set is exactly `required`
/// (plus `optional`), with every required member present.
fn exact_keys(value: &Json, required: &[&str], optional: &[&str]) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    required.iter().all(|key| object.contains_key(*key))
        && object
            .keys()
            .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str()))
}

/// The closed policy vocabulary by wire name (empty when absent; the
/// custody-verified policy always carries every vocabulary).
fn policy_vocabulary<'a>(context: &'a TrustedContext, name: &str) -> &'a [Json] {
    context.vocabulary(name).map(Vec::as_slice).unwrap_or(&[])
}

/// Whether one JSON string value is a member of a vocabulary array.
fn in_vocabulary(vocabulary: &[Json], value: &Json) -> bool {
    value
        .as_str()
        .is_some_and(|text| vocabulary.iter().any(|entry| entry.as_str() == Some(text)))
}

/// A non-empty unique array of known vocabulary strings.
fn unique_known(values: &Json, vocabulary: &[Json], nonempty: bool) -> bool {
    let Some(items) = values.as_array() else {
        return false;
    };
    (!nonempty || !items.is_empty())
        && items.iter().all(|item| in_vocabulary(vocabulary, item))
        && (1..items.len()).all(|index| items[..index].iter().all(|prior| prior != &items[index]))
}

/// `^sha256:[0-9a-f]{64}$` and every other opaque `<prefix>-sha256:`
/// digest spelling.
fn is_opaque_ref(value: &Json, prefix: &str) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    let marker = if prefix.is_empty() {
        "sha256:".to_owned()
    } else {
        format!("{prefix}-sha256:")
    };
    let Some(rest) = text.strip_prefix(&marker) else {
        return false;
    };
    rest.len() == 64
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// `^sha256:[0-9a-f]{64}$`.
fn is_digest(value: &Json) -> bool {
    is_opaque_ref(value, "")
}

/// `^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$`.
fn is_semver(value: &Json) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    let segments: Vec<&str> = text.split('.').collect();
    segments.len() == 3
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment.bytes().all(|byte| byte.is_ascii_digit())
                && (*segment == "0" || !segment.starts_with('0'))
        })
}

/// The audit reference `{id, version, evidenceDigest}`.
fn audit_ref(value: &Json) -> bool {
    exact_keys(value, &["id", "version", "evidenceDigest"], &[])
        && value
            .get("id")
            .and_then(Json::as_str)
            .is_some_and(|id| !id.is_empty())
        && is_semver(value.get("version").unwrap_or(&Json::Null))
        && is_digest(value.get("evidenceDigest").unwrap_or(&Json::Null))
}

/// `{subjectProfileRef, subjectDigest}` with the exact frozen profile
/// reference.
fn evidence_binding_shape(value: &Json) -> bool {
    exact_keys(value, &["subjectProfileRef", "subjectDigest"], &[])
        && same_object(
            value.get("subjectProfileRef").unwrap_or(&Json::Null),
            &profile_ref_json(),
        )
        && is_opaque_ref(value.get("subjectDigest").unwrap_or(&Json::Null), "subject")
}

/// The frozen subject-profile reference as wire JSON.
fn profile_ref_json() -> Json {
    serde_json::json!({
        "profileId": refs::AUTHORIZATION_SUBJECT_PROFILE_ID,
        "version": refs::DECISION_FAMILY_VERSION,
        "digest": format!("sha256:{}", refs::AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256),
    })
}

/// The frozen authorizing-evidence contract reference as wire JSON.
fn evidence_contract_ref_json() -> Json {
    serde_json::json!({
        "contractId": refs::AUTHORIZING_EVIDENCE_CONTRACT_ID,
        "version": refs::DECISION_FAMILY_VERSION,
        "digest": format!("sha256:{}", refs::AUTHORIZING_EVIDENCE_RAW_SHA256),
    })
}

/// The frozen policy reference as wire JSON.
fn policy_ref_json() -> Json {
    serde_json::json!({
        "policyId": refs::POLICY_ID,
        "version": refs::POLICY_VERSION,
        "digest": refs::POLICY_DIGEST,
    })
}

/// The frozen authority reference as wire JSON.
fn authority_ref_json() -> Json {
    serde_json::json!({
        "contractId": refs::AUTHORITY_CONTRACT_ID,
        "version": refs::AUTHORITY_VERSION,
        "digest": format!("sha256:{}", refs::AUTHORITY_RAW_SHA256),
    })
}

/// The frozen decision contract reference as wire JSON.
fn decision_ref_json() -> Json {
    serde_json::json!({
        "contractId": refs::DECISION_CONTRACT_ID,
        "version": refs::DECISION_FAMILY_VERSION,
    })
}

/// One authorizing evidence record at one of the nine closed
/// positions: the exact contract triple, kind/purpose, outcome,
/// opaque identity, verification/freshness states, and binding.
fn authorizing_evidence_shape(value: &Json, spec: &EvidenceSpec, context: &TrustedContext) -> bool {
    let allowed_outcomes: Vec<Json> = spec
        .outcomes
        .iter()
        .map(|outcome| Json::String(outcome.as_str().to_owned()))
        .collect();
    let verification = context
        .evidence_vocabulary("verificationState")
        .cloned()
        .unwrap_or_default();
    let freshness = context
        .evidence_vocabulary("freshnessState")
        .cloned()
        .unwrap_or_default();
    exact_keys(
        value,
        &[
            "contractId",
            "version",
            "digest",
            "evidenceKind",
            "purpose",
            "outcome",
            "evidenceId",
            "verificationState",
            "freshnessState",
            "binding",
        ],
        &[],
    ) && value.get("contractId").and_then(Json::as_str)
        == Some(refs::AUTHORIZING_EVIDENCE_CONTRACT_ID)
        && value.get("version").and_then(Json::as_str) == Some(refs::DECISION_FAMILY_VERSION)
        && value.get("digest").and_then(Json::as_str)
            == Some(&format!("sha256:{}", refs::AUTHORIZING_EVIDENCE_RAW_SHA256))
        && value.get("evidenceKind").and_then(Json::as_str) == Some(spec.evidence_kind)
        && value.get("purpose").and_then(Json::as_str) == Some(spec.purpose)
        && in_vocabulary(
            &allowed_outcomes,
            value.get("outcome").unwrap_or(&Json::Null),
        )
        && is_opaque_ref(value.get("evidenceId").unwrap_or(&Json::Null), "evidence")
        && in_vocabulary(
            &verification,
            value.get("verificationState").unwrap_or(&Json::Null),
        )
        && in_vocabulary(
            &freshness,
            value.get("freshnessState").unwrap_or(&Json::Null),
        )
        && evidence_binding_shape(value.get("binding").unwrap_or(&Json::Null))
}

fn nullable_authorizing_evidence(
    value: &Json,
    spec: &EvidenceSpec,
    context: &TrustedContext,
) -> bool {
    value.is_null() || authorizing_evidence_shape(value, spec, context)
}

/// The exact classification-custody reference.
fn exact_classification_ref(value: &Json) -> bool {
    exact_keys(
        value,
        &[
            "contractId",
            "version",
            "digest",
            "decisionId",
            "evidenceDigest",
        ],
        &[],
    ) && value.get("contractId").and_then(Json::as_str) == Some(refs::CLASSIFICATION_CONTRACT_ID)
        && value.get("version").and_then(Json::as_str) == Some(refs::DECISION_FAMILY_VERSION)
        && value.get("digest").and_then(Json::as_str)
            == Some(&format!(
                "sha256:{}",
                refs::CLASSIFICATION_CONTRACT_RAW_SHA256
            ))
        && is_opaque_ref(
            value.get("decisionId").unwrap_or(&Json::Null),
            "classification",
        )
        && is_digest(value.get("evidenceDigest").unwrap_or(&Json::Null))
}

/// The four-state path representation.
fn path_shape(value: &Json) -> bool {
    let Some(state) = value.get("state").and_then(Json::as_str) else {
        return false;
    };
    if !["known", "unknown", "withheld", "unsupported"].contains(&state) {
        return false;
    }
    if state == "known" {
        return exact_keys(value, &["state", "value"], &[])
            && value
                .get("value")
                .and_then(Json::as_str)
                .is_some_and(|text| !text.is_empty());
    }
    exact_keys(value, &["state"], &[])
}

/// The four-state measured-value representation; only `known` carries
/// a finite JSON scalar.
fn value_state_shape(value: &Json) -> bool {
    let Some(state) = value.get("state").and_then(Json::as_str) else {
        return false;
    };
    if !["known", "unknown", "withheld", "unsupported"].contains(&state) {
        return false;
    }
    if state != "known" {
        return exact_keys(value, &["state"], &[]);
    }
    let carried = value.get("value").unwrap_or(&Json::Null);
    let scalar =
        carried.is_null() || carried.is_number() || carried.is_string() || carried.is_boolean();
    scalar && exact_keys(value, &["state", "value"], &[]) && !is_non_finite_number(carried)
}

/// JS `Number.isFinite` distinguishes non-finite floats; serde_json
/// can only carry them through `f64`.
fn is_non_finite_number(value: &Json) -> bool {
    value.as_f64().is_some_and(|number| !number.is_finite()) && value.is_number()
}

/// A non-empty unique sensitivity array.
fn classification_shape(value: &Json, context: &TrustedContext) -> bool {
    unique_known(value, policy_vocabulary(context, "dataSensitivity"), true)
}

/// A repository ref is null or an opaque `repo-sha256:` identity.
fn repository_ref_shape(value: &Json) -> bool {
    value.is_null() || is_opaque_ref(value, "repo")
}

// ---------------------------------------------------------------------------
// Decision-input validation (the exact `validateDecisionInput` order).
// ---------------------------------------------------------------------------

/// The exact ordered validation of one decision input. Returns the
/// first failing reason code, mirroring the reference validator.
pub fn validate_decision_input(input: &Json, context: &TrustedContext) -> Option<String> {
    let required = [
        "decisionContractRef",
        "authorityRef",
        "policyRef",
        "authorizingEvidenceContractRef",
        "authorizationSubjectProfileRef",
        "artifactKind",
        "artifactRef",
        "operation",
        "source",
        "destination",
        "audience",
        "dataSensitivity",
        "exportDisposition",
        "provenance",
        "resourcePath",
        "conflictResolution",
        "constraints",
        "broadeningGrant",
    ];
    if !exact_keys(input, &required, &["valueState", "derivedArtifact"]) {
        return Some("input.shape".to_owned());
    }
    let ref_failures = [
        (
            "decisionContractRef",
            &decision_ref_json(),
            "input.decision-ref",
        ),
        ("authorityRef", &authority_ref_json(), "input.authority-ref"),
        ("policyRef", &policy_ref_json(), "input.policy-ref"),
        (
            "authorizingEvidenceContractRef",
            &evidence_contract_ref_json(),
            "input.authorizing-evidence-ref",
        ),
        (
            "authorizationSubjectProfileRef",
            &profile_ref_json(),
            "input.authorization-subject-profile-ref",
        ),
    ];
    for (key, expected, code) in ref_failures {
        let member = input.get(key).unwrap_or(&Json::Null);
        let matches = member.is_null() || same_object(member, expected);
        if !matches {
            return Some(code.to_owned());
        }
    }
    if input
        .get("artifactKind")
        .and_then(Json::as_str)
        .map(|kind| kind.is_empty())
        .unwrap_or(true)
    {
        return Some("input.artifact-kind".to_owned());
    }
    if !is_opaque_ref(input.get("artifactRef").unwrap_or(&Json::Null), "artifact") {
        return Some("input.artifact-ref".to_owned());
    }
    let operation = input.get("operation").unwrap_or(&Json::Null);
    let operation_version = context
        .policy()
        .pointer("/contractVersions/operationVocabulary")
        .and_then(Json::as_str)
        .unwrap_or_default();
    if !exact_keys(operation, &["id", "version"], &[])
        || !in_vocabulary(
            policy_vocabulary(context, "operation"),
            operation.get("id").unwrap_or(&Json::Null),
        )
        || operation.get("version").and_then(Json::as_str) != Some(operation_version)
    {
        return Some("input.operation".to_owned());
    }
    let source = input.get("source").unwrap_or(&Json::Null);
    if !exact_keys(source, &["repositoryRole", "repositoryRef"], &[])
        || !in_vocabulary(
            policy_vocabulary(context, "repositoryRole"),
            source.get("repositoryRole").unwrap_or(&Json::Null),
        )
        || !repository_ref_shape(source.get("repositoryRef").unwrap_or(&Json::Null))
    {
        return Some("input.source".to_owned());
    }
    let destination = input.get("destination").unwrap_or(&Json::Null);
    if !exact_keys(
        destination,
        &[
            "repositoryRole",
            "repositoryRef",
            "trustBoundary",
            "repositoryRelation",
            "tenantRelation",
        ],
        &[],
    ) || !in_vocabulary(
        policy_vocabulary(context, "repositoryRole"),
        destination.get("repositoryRole").unwrap_or(&Json::Null),
    ) || !repository_ref_shape(destination.get("repositoryRef").unwrap_or(&Json::Null))
        || !in_vocabulary(
            policy_vocabulary(context, "trustBoundary"),
            destination.get("trustBoundary").unwrap_or(&Json::Null),
        )
        || !in_vocabulary(
            policy_vocabulary(context, "repositoryRelation"),
            destination.get("repositoryRelation").unwrap_or(&Json::Null),
        )
        || !in_vocabulary(
            policy_vocabulary(context, "tenantRelation"),
            destination.get("tenantRelation").unwrap_or(&Json::Null),
        )
    {
        return Some("input.destination".to_owned());
    }
    if !in_vocabulary(
        policy_vocabulary(context, "audience"),
        input.get("audience").unwrap_or(&Json::Null),
    ) {
        return Some("input.audience".to_owned());
    }
    if !classification_shape(input.get("dataSensitivity").unwrap_or(&Json::Null), context) {
        return Some("input.data-sensitivity".to_owned());
    }
    if !in_vocabulary(
        policy_vocabulary(context, "exportDisposition"),
        input.get("exportDisposition").unwrap_or(&Json::Null),
    ) {
        return Some("input.export-disposition".to_owned());
    }
    if !provenance_shape(input.get("provenance").unwrap_or(&Json::Null), context) {
        return Some("input.provenance".to_owned());
    }
    if !path_shape(input.get("resourcePath").unwrap_or(&Json::Null)) {
        return Some("input.resource-path".to_owned());
    }
    if input.get("valueState").is_some()
        && !value_state_shape(input.get("valueState").unwrap_or(&Json::Null))
    {
        return Some("input.value-state".to_owned());
    }
    let conflict = input.get("conflictResolution").unwrap_or(&Json::Null);
    if !exact_keys(conflict, &["state", "decisionRef"], &[])
        || !in_vocabulary(
            policy_vocabulary(context, "conflictState"),
            conflict.get("state").unwrap_or(&Json::Null),
        )
        || !nullable_authorizing_evidence(
            conflict.get("decisionRef").unwrap_or(&Json::Null),
            &EvidenceSpec::CONFLICT_RESOLUTION,
            context,
        )
    {
        return Some("input.conflict-resolution".to_owned());
    }
    let constraints = input.get("constraints").unwrap_or(&Json::Null);
    let constraint_entries = constraints.as_array();
    let any_invalid = !constraint_entries.is_some_and(|entries| {
        !entries.iter().any(|constraint| {
            !exact_keys(
                constraint,
                &[
                    "scope",
                    "constraintRef",
                    "allowedOperations",
                    "allowedTrustBoundaries",
                    "allowedAudiences",
                ],
                &[],
            ) || !in_vocabulary(
                policy_vocabulary(context, "constraintScope"),
                constraint.get("scope").unwrap_or(&Json::Null),
            ) || !audit_ref(constraint.get("constraintRef").unwrap_or(&Json::Null))
                || !unique_known(
                    constraint.get("allowedOperations").unwrap_or(&Json::Null),
                    policy_vocabulary(context, "operation"),
                    false,
                )
                || !unique_known(
                    constraint
                        .get("allowedTrustBoundaries")
                        .unwrap_or(&Json::Null),
                    policy_vocabulary(context, "trustBoundary"),
                    false,
                )
                || !unique_known(
                    constraint.get("allowedAudiences").unwrap_or(&Json::Null),
                    policy_vocabulary(context, "audience"),
                    false,
                )
        })
    });
    let duplicates = constraint_entries.is_some_and(|entries| {
        let canonical_entries: Vec<String> = entries.iter().map(canonical).collect();
        canonical_entries
            .iter()
            .collect::<std::collections::BTreeSet<&String>>()
            .len()
            != canonical_entries.len()
    });
    if any_invalid || duplicates {
        return Some("input.constraints".to_owned());
    }
    if !input.get("broadeningGrant").is_some_and(Json::is_null) {
        return Some("input.broadening-grant".to_owned());
    }
    if input.get("derivedArtifact").is_some()
        && !derived_shape(input.get("derivedArtifact").unwrap_or(&Json::Null), context)
    {
        return Some("input.derived-artifact".to_owned());
    }
    None
}

/// The provenance block: origin, custody, origin booleans, the exact
/// classification reference, and the six nullable evidence positions.
fn provenance_shape(value: &Json, context: &TrustedContext) -> bool {
    let allowed_origins = [
        "synthetic",
        "consumer-repository",
        "lekalo-repository",
        "tool-runtime",
        "derived",
        "external",
    ];
    exact_keys(
        value,
        &[
            "origin",
            "repositoryRole",
            "repositoryRef",
            "synthetic",
            "derived",
            "classificationRef",
            "publicFixturePermissionRef",
            "publicFixtureLicenseRef",
            "publicFixtureConsentRef",
            "consumerAclPermissionRef",
            "consumerRepositoryConsentRef",
            "exportTransferConsentRef",
        ],
        &[],
    ) && value
        .get("origin")
        .and_then(Json::as_str)
        .is_some_and(|origin| allowed_origins.contains(&origin))
        && in_vocabulary(
            policy_vocabulary(context, "repositoryRole"),
            value.get("repositoryRole").unwrap_or(&Json::Null),
        )
        && repository_ref_shape(value.get("repositoryRef").unwrap_or(&Json::Null))
        && value.get("synthetic").is_some_and(Json::is_boolean)
        && value.get("derived").is_some_and(Json::is_boolean)
        && exact_classification_ref(value.get("classificationRef").unwrap_or(&Json::Null))
        && nullable_authorizing_evidence(
            value
                .get("publicFixturePermissionRef")
                .unwrap_or(&Json::Null),
            &EvidenceSpec::PUBLIC_FIXTURE_PERMISSION,
            context,
        )
        && nullable_authorizing_evidence(
            value.get("publicFixtureLicenseRef").unwrap_or(&Json::Null),
            &EvidenceSpec::PUBLIC_FIXTURE_LICENSE,
            context,
        )
        && nullable_authorizing_evidence(
            value.get("publicFixtureConsentRef").unwrap_or(&Json::Null),
            &EvidenceSpec::PUBLIC_FIXTURE_CONSENT,
            context,
        )
        && nullable_authorizing_evidence(
            value.get("consumerAclPermissionRef").unwrap_or(&Json::Null),
            &EvidenceSpec::CONSUMER_ACL_PERMISSION,
            context,
        )
        && nullable_authorizing_evidence(
            value
                .get("consumerRepositoryConsentRef")
                .unwrap_or(&Json::Null),
            &EvidenceSpec::CONSUMER_REPOSITORY_CONSENT,
            context,
        )
        && nullable_authorizing_evidence(
            value.get("exportTransferConsentRef").unwrap_or(&Json::Null),
            &EvidenceSpec::EXPORT_TRANSFER_CONSENT,
            context,
        )
}

/// One derived source artifact.
fn source_artifact_shape(value: &Json, context: &TrustedContext) -> bool {
    exact_keys(
        value,
        &[
            "sourceRef",
            "artifactKind",
            "authorityRef",
            "policyRef",
            "classificationRef",
            "dataSensitivity",
            "exportDisposition",
        ],
        &[],
    ) && value
        .get("sourceRef")
        .and_then(Json::as_str)
        .is_some_and(|source_ref| is_opaque_ref(&Json::String(source_ref.to_owned()), "source"))
        && value
            .get("artifactKind")
            .and_then(Json::as_str)
            .is_some_and(|kind| !kind.is_empty())
        && same_object(
            value.get("authorityRef").unwrap_or(&Json::Null),
            &authority_ref_json(),
        )
        && same_object(
            value.get("policyRef").unwrap_or(&Json::Null),
            &policy_ref_json(),
        )
        && exact_classification_ref(value.get("classificationRef").unwrap_or(&Json::Null))
        && classification_shape(value.get("dataSensitivity").unwrap_or(&Json::Null), context)
        && in_vocabulary(
            policy_vocabulary(context, "exportDisposition"),
            value.get("exportDisposition").unwrap_or(&Json::Null),
        )
}

/// One applied transform with the exact vocabulary version.
fn transform_shape(value: &Json, context: &TrustedContext) -> bool {
    let transform_version = context
        .policy()
        .pointer("/contractVersions/transformVocabulary")
        .and_then(Json::as_str)
        .unwrap_or_default();
    exact_keys(value, &["transformId", "version", "evidenceDigest"], &[])
        && in_vocabulary(
            policy_vocabulary(context, "transform"),
            value.get("transformId").unwrap_or(&Json::Null),
        )
        && value.get("version").and_then(Json::as_str) == Some(transform_version)
        && is_digest(value.get("evidenceDigest").unwrap_or(&Json::Null))
}

/// One declassification or aggregation decision record.
fn decision_shape(value: &Json, aggregation: bool, context: &TrustedContext) -> bool {
    let required: &[&str] = if aggregation {
        &[
            "policyRef",
            "decisionRef",
            "version",
            "outcome",
            "removesSourceRows",
            "removesSourceIdentities",
        ]
    } else {
        &[
            "policyRef",
            "decisionRef",
            "version",
            "outcome",
            "removedSensitivities",
        ]
    };
    let spec = if aggregation {
        &EvidenceSpec::AGGREGATION
    } else {
        &EvidenceSpec::DECLASSIFICATION
    };
    let allowed_outcomes = [
        Json::String("approved".to_owned()),
        Json::String("rejected".to_owned()),
    ];
    exact_keys(value, required, &[])
        && same_object(
            value.get("policyRef").unwrap_or(&Json::Null),
            &policy_ref_json(),
        )
        && authorizing_evidence_shape(
            value.get("decisionRef").unwrap_or(&Json::Null),
            spec,
            context,
        )
        && value.get("version").and_then(Json::as_str) == Some(DECISION_VERSION)
        && in_vocabulary(
            &allowed_outcomes,
            value.get("outcome").unwrap_or(&Json::Null),
        )
        && (!aggregation
            || (value.get("removesSourceRows") == Some(&Json::Bool(true))
                && value.get("removesSourceIdentities") == Some(&Json::Bool(true))))
        && (!aggregation
            || unique_known(
                value.get("removedSensitivities").unwrap_or(&Json::Null),
                policy_vocabulary(context, "dataSensitivity"),
                true,
            ))
}

/// The derived-artifact block.
fn derived_shape(value: &Json, context: &TrustedContext) -> bool {
    if !exact_keys(
        value,
        &[
            "sourceArtifacts",
            "appliedTransforms",
            "declassificationDecision",
            "aggregationDecision",
            "containsSourceRows",
            "containsSourceIdentities",
            "reevaluated",
        ],
        &[],
    ) {
        return false;
    }
    let sources = value.get("sourceArtifacts").unwrap_or(&Json::Null);
    let transforms = value.get("appliedTransforms").unwrap_or(&Json::Null);
    let sources_ok = sources.as_array().is_some_and(|entries| {
        !entries.is_empty()
            && entries
                .iter()
                .all(|source| source_artifact_shape(source, context))
            && {
                let canonical_sources: Vec<String> = entries.iter().map(canonical).collect();
                canonical_sources
                    .iter()
                    .collect::<std::collections::BTreeSet<&String>>()
                    .len()
                    == canonical_sources.len()
            }
    });
    let transforms_ok = transforms.as_array().is_some_and(|entries| {
        !entries.is_empty()
            && entries
                .iter()
                .all(|transform| transform_shape(transform, context))
            && {
                let canonical_transforms: Vec<String> = entries.iter().map(canonical).collect();
                canonical_transforms
                    .iter()
                    .collect::<std::collections::BTreeSet<&String>>()
                    .len()
                    == canonical_transforms.len()
            }
    });
    let declassification = value.get("declassificationDecision").unwrap_or(&Json::Null);
    let aggregation = value.get("aggregationDecision").unwrap_or(&Json::Null);
    sources_ok
        && transforms_ok
        && (declassification.is_null() || decision_shape(declassification, false, context))
        && (aggregation.is_null() || decision_shape(aggregation, true, context))
        && value
            .get("containsSourceRows")
            .is_some_and(Json::is_boolean)
        && value
            .get("containsSourceIdentities")
            .is_some_and(Json::is_boolean)
        && value.get("reevaluated").is_some_and(Json::is_boolean)
}

// ---------------------------------------------------------------------------
// Subject projection and evidence semantics.
// ---------------------------------------------------------------------------

/// One projection path segment: the member key plus whether it fans
/// out over array elements.
struct PathSegment {
    key: String,
    each: bool,
}

fn path_segments(path: &str) -> Vec<PathSegment> {
    path.split('.')
        .map(|part| {
            if let Some(stripped) = part.strip_suffix("[]") {
                PathSegment {
                    key: stripped.to_owned(),
                    each: true,
                }
            } else {
                PathSegment {
                    key: part.to_owned(),
                    each: false,
                }
            }
        })
        .collect()
}

/// Walk to the declared path and visit the owning object and member
/// (delete or in-place mutation). Mirrors the reference `visitPath`.
fn visit_path(root: &mut Json, path: &str, visitor: &mut impl FnMut(&mut Map<String, Json>, &str)) {
    let segments = path_segments(path);
    visit_node(root, &segments, 0, visitor);
}

fn visit_node(
    current: &mut Json,
    segments: &[PathSegment],
    index: usize,
    visitor: &mut impl FnMut(&mut Map<String, Json>, &str),
) {
    let mut absent = Json::Null;
    let Some(object) = current.as_object_mut() else {
        return;
    };
    let segment = &segments[index];
    if !object.contains_key(&segment.key) {
        return;
    }
    if index == segments.len() - 1 {
        visitor(object, &segment.key);
        return;
    }
    let next = object.get_mut(&segment.key).unwrap_or(&mut absent);
    if segment.each {
        if let Some(items) = next.as_array_mut() {
            for item in items.iter_mut() {
                visit_node(item, segments, index + 1, visitor);
            }
        }
    } else {
        visit_node(next, segments, index + 1, visitor);
    }
}

/// The canonical subject projection: the complete input minus the
/// nine authorizing evidence objects and the non-authorizing
/// constraint refs, with the declared set canonicalization applied.
fn authorization_subject_projection(input: &Json, profile: &Json) -> Json {
    let mut projected = input.clone();
    if let Some(excluded) = profile
        .pointer("/projection/excludedPaths")
        .and_then(Json::as_array)
    {
        for path in excluded {
            if let Some(path) = path.as_str() {
                visit_path(&mut projected, path, &mut |object, key| {
                    object.remove(key);
                });
            }
        }
    }
    if let Some(rules) = profile
        .pointer("/projection/setCanonicalizationRules")
        .and_then(Json::as_array)
    {
        for rule in rules {
            let Some(path) = rule.get("path").and_then(Json::as_str) else {
                continue;
            };
            let strategy = rule
                .get("strategy")
                .and_then(Json::as_str)
                .unwrap_or_default();
            let rule_key = rule.get("key").and_then(Json::as_str).unwrap_or_default();
            visit_path(&mut projected, path, &mut |_object, key| {
                sort_projected_array(
                    _object.get_mut(key).unwrap_or(&mut Json::Null),
                    strategy,
                    rule_key,
                );
            });
        }
    }
    projected
}

/// Apply one declared canonicalization strategy to one array in place.
fn sort_projected_array(value: &mut Json, strategy: &str, rule_key: &str) {
    let Some(items) = value.as_array_mut() else {
        return;
    };
    match strategy {
        "unicode-code-point-lexicographic" => {
            items.sort_by(|left, right| {
                compare_unicode_code_points(
                    left.as_str().unwrap_or_default(),
                    right.as_str().unwrap_or_default(),
                )
            });
        }
        "object-key-then-canonical" => {
            items.sort_by(|left, right| {
                let left_key = left
                    .get(rule_key)
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let right_key = right
                    .get(rule_key)
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned();
                compare_unicode_code_points(&left_key, &right_key)
                    .then_with(|| compare_unicode_code_points(&canonical(left), &canonical(right)))
            });
        }
        "canonical-json-lexicographic" => {
            items.sort_by(|left, right| {
                compare_unicode_code_points(&canonical(left), &canonical(right))
            });
        }
        other => {
            // The pinned profile declares only the three strategies
            // above; custody verification pins its exact bytes.
            panic!("custody.subject-profile-unknown-canonicalization: {other}");
        }
    }
}

/// The opaque `subject-sha256:` digest of the canonical subject
/// projection under the exact pinned profile.
pub fn authorization_subject_digest(input: &Json, profile: &Json) -> String {
    if profile.get("profileId").and_then(Json::as_str)
        != Some(refs::AUTHORIZATION_SUBJECT_PROFILE_ID)
        || profile.get("version").and_then(Json::as_str) != Some(refs::DECISION_FAMILY_VERSION)
    {
        panic!("custody.subject-profile-ref-mismatch");
    }
    let projected = authorization_subject_projection(input, profile);
    format!(
        "subject-sha256:{}",
        crate::digest::sha256_hex(canonical(&projected).as_bytes())
    )
}

/// The declared evidence entries of one input, in reference order.
fn authorizing_evidence_entries(
    input: &Json,
) -> Vec<(&'static str, Option<&Json>, &'static EvidenceSpec)> {
    let mut entries = Vec::new();
    let provenance = input.get("provenance").unwrap_or(&Json::Null);
    for (name, spec) in [
        (
            "publicFixturePermissionRef",
            &EvidenceSpec::PUBLIC_FIXTURE_PERMISSION,
        ),
        (
            "publicFixtureLicenseRef",
            &EvidenceSpec::PUBLIC_FIXTURE_LICENSE,
        ),
        (
            "publicFixtureConsentRef",
            &EvidenceSpec::PUBLIC_FIXTURE_CONSENT,
        ),
        (
            "consumerAclPermissionRef",
            &EvidenceSpec::CONSUMER_ACL_PERMISSION,
        ),
        (
            "consumerRepositoryConsentRef",
            &EvidenceSpec::CONSUMER_REPOSITORY_CONSENT,
        ),
        (
            "exportTransferConsentRef",
            &EvidenceSpec::EXPORT_TRANSFER_CONSENT,
        ),
    ] {
        if provenance.get(name).is_some() {
            entries.push((name, provenance.get(name), spec as &EvidenceSpec));
        }
    }
    let conflict_ref = input.pointer("/conflictResolution/decisionRef");
    entries.push((
        "conflictResolution.decisionRef",
        conflict_ref,
        &EvidenceSpec::CONFLICT_RESOLUTION,
    ));
    if input
        .pointer("/derivedArtifact/declassificationDecision")
        .is_some_and(|decision| !decision.is_null())
    {
        entries.push((
            "derivedArtifact.declassificationDecision.decisionRef",
            input.pointer("/derivedArtifact/declassificationDecision/decisionRef"),
            &EvidenceSpec::DECLASSIFICATION,
        ));
    }
    if input
        .pointer("/derivedArtifact/aggregationDecision")
        .is_some_and(|decision| !decision.is_null())
    {
        entries.push((
            "derivedArtifact.aggregationDecision.decisionRef",
            input.pointer("/derivedArtifact/aggregationDecision/decisionRef"),
            &EvidenceSpec::AGGREGATION,
        ));
    }
    entries
}

/// The semantic evidence checks: identity reuse, verification,
/// freshness, subject binding, and outcome coherence.
fn authorizing_evidence_semantic_error(input: &Json, context: &TrustedContext) -> Option<String> {
    let entries = authorizing_evidence_entries(input);
    let mut evidence_ids: Vec<&str> = Vec::new();
    let profile = context.authorization_subject_profile();
    let expected_subject_digest = authorization_subject_digest(input, profile);
    for (name, evidence, spec) in entries {
        let Some(evidence) = evidence.filter(|evidence| !evidence.is_null()) else {
            continue;
        };
        let evidence_id = evidence
            .get("evidenceId")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if evidence_ids.contains(&evidence_id) {
            return Some("evidence.identity-reused".to_owned());
        }
        evidence_ids.push(evidence_id);
        if evidence.get("verificationState").and_then(Json::as_str) != Some("verified") {
            return Some("evidence.unverified".to_owned());
        }
        let freshness = evidence
            .get("freshnessState")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if freshness != "current" {
            return Some(format!("evidence.{freshness}"));
        }
        let binding = evidence.get("binding").cloned().unwrap_or(Json::Null);
        if !same_object(
            binding.get("subjectProfileRef").unwrap_or(&Json::Null),
            &profile_ref_json(),
        ) || binding.get("subjectDigest").and_then(Json::as_str)
            != Some(expected_subject_digest.as_str())
        {
            return Some("evidence.binding-mismatch".to_owned());
        }
        let expected_outcome = if name == "conflictResolution.decisionRef" {
            match input
                .pointer("/conflictResolution/state")
                .and_then(Json::as_str)
            {
                Some("resolved-allow") => Some("allow"),
                Some("resolved-deny") => Some("deny"),
                _ => None,
            }
        } else {
            spec.outcome.map(|outcome| outcome.as_str())
        };
        if let Some(expected) = expected_outcome {
            if evidence.get("outcome").and_then(Json::as_str) != Some(expected) {
                return Some("evidence.outcome-mismatch".to_owned());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Coherence, derivation, paths, and the decision precedence chain.
// ---------------------------------------------------------------------------

/// Whether one repository role must carry a non-null opaque ref.
fn repository_backed_role(role: &str) -> bool {
    role != "local-workspace" && role != "public-channel"
}

/// Whether the declared context is internally coherent.
fn context_coherence(input: &Json) -> Option<String> {
    let source_role = input
        .pointer("/source/repositoryRole")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let source_ref = input
        .pointer("/source/repositoryRef")
        .and_then(Json::as_str);
    let destination_role = input
        .pointer("/destination/repositoryRole")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let destination_ref = input
        .pointer("/destination/repositoryRef")
        .and_then(Json::as_str);
    let provenance_role = input
        .pointer("/provenance/repositoryRole")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let provenance_ref = input
        .pointer("/provenance/repositoryRef")
        .and_then(Json::as_str);
    let origin = input
        .pointer("/provenance/origin")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let synthetic = input
        .pointer("/provenance/synthetic")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let derived = input
        .pointer("/provenance/derived")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let boundary = input
        .pointer("/destination/trustBoundary")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let relation = input
        .pointer("/destination/repositoryRelation")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let tenant = input
        .pointer("/destination/tenantRelation")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let audience = input
        .get("audience")
        .and_then(Json::as_str)
        .unwrap_or_default();

    if provenance_role != source_role || provenance_ref != source_ref {
        return Some("provenance.source-repository-mismatch".to_owned());
    }
    if source_role == "public-channel"
        || repository_backed_role(source_role) != source_ref.is_some()
    {
        return Some("repository.source-ref-role-mismatch".to_owned());
    }
    if repository_backed_role(destination_role) != destination_ref.is_some() {
        return Some("repository.destination-ref-role-mismatch".to_owned());
    }

    let ordinary_origin = !synthetic && !derived;
    if origin == "synthetic"
        && !(synthetic && !derived && source_role == "local-workspace" && source_ref.is_none())
    {
        return Some("provenance.origin-boolean-conflict".to_owned());
    }
    if origin == "derived" && !(derived && !synthetic) {
        return Some("provenance.origin-boolean-conflict".to_owned());
    }
    if origin != "synthetic" && origin != "derived" && !ordinary_origin {
        return Some("provenance.origin-boolean-conflict".to_owned());
    }
    if origin == "consumer-repository" && source_role != "consumer-repository" {
        return Some("provenance.origin-role-conflict".to_owned());
    }
    if origin == "lekalo-repository" && source_role != "lekalo-repository" {
        return Some("provenance.origin-role-conflict".to_owned());
    }
    if origin == "external" && source_role != "external-repository" {
        return Some("provenance.origin-role-conflict".to_owned());
    }

    match boundary {
        "same-local-workspace" => {
            if destination_role != "local-workspace"
                || destination_ref.is_some()
                || relation != "not-applicable"
                || tenant != "same-tenant"
                || audience != "operator-only"
            {
                return Some("repository.local-context-contradiction".to_owned());
            }
        }
        "same-repository" => {
            if !repository_backed_role(source_role)
                || source_ref.is_none()
                || destination_role != source_role
                || destination_ref != source_ref
                || relation != "same-origin"
                || tenant != "same-tenant"
            {
                return Some("repository.same-origin-contradiction".to_owned());
            }
        }
        "same-tenant" | "cross-repository" => {
            if source_ref.is_none()
                || destination_ref.is_none()
                || source_ref == destination_ref
                || destination_role != "external-repository"
                || relation != "different-repository"
                || tenant != "same-tenant"
            {
                return Some("repository.different-origin-contradiction".to_owned());
            }
        }
        "cross-tenant" => {
            if source_ref.is_none()
                || destination_ref.is_none()
                || source_ref == destination_ref
                || destination_role != "external-repository"
                || relation != "different-repository"
                || tenant != "cross-tenant"
            {
                return Some("repository.cross-tenant-contradiction".to_owned());
            }
        }
        "public" => {
            if destination_role != "public-channel"
                || destination_ref.is_some()
                || relation != "not-applicable"
                || tenant != "not-applicable"
                || audience != "public"
            {
                return Some("repository.public-context-contradiction".to_owned());
            }
        }
        _ => return Some("repository.unknown-context".to_owned()),
    }
    None
}

/// Whether a known project-relative path violates the closed path
/// grammar (drive, UNC, rooted, home-relative, URI, `..`, backslash,
/// encoded syntax, control characters, empty/dot/non-NFC segments,
/// trailing dots/spaces, colons, DOS devices, short names).
fn invalid_path(path: &str) -> bool {
    if path.chars().nfc().collect::<String>() != path
        || path
            .chars()
            .any(|character| matches!(character, '\u{0}'..='\u{1f}') || character == '\u{7f}')
    {
        return true;
    }
    if path.starts_with('/')
        || path.starts_with("//")
        || path.starts_with('~')
        || path.contains('\\')
        || is_drive_or_uri(path)
        || contains_percent_escape(path)
    {
        return true;
    }
    path.split('/').any(|segment| {
        segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.ends_with(' ')
            || segment.ends_with('.')
            || segment.contains(':')
            || is_dos_device(segment)
            || contains_short_name_suffix(segment)
    })
}

/// `^[A-Za-z]:` (drive) or `^[A-Za-z][A-Za-z0-9+.-]*:` (URI scheme).
fn is_drive_or_uri(path: &str) -> bool {
    let mut characters = path.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let rest = &path[first.len_utf8()..];
    if rest.starts_with(':') {
        return true;
    }
    let scheme_end = rest.find(|character: char| {
        !character.is_ascii_alphanumeric()
            && character != '+'
            && character != '.'
            && character != '-'
    });
    let (scheme, after) = match scheme_end {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    !scheme.is_empty() && after.starts_with(':')
}

/// `/%[0-9A-Fa-f]{2}/` anywhere.
fn contains_percent_escape(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes
        .windows(3)
        .any(|window| window[0] == b'%' && window[1..].iter().all(|byte| byte.is_ascii_hexdigit()))
}

/// NTFS short-name-like suffix `~<digit>` anywhere in one segment.
fn contains_short_name_suffix(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes
        .windows(2)
        .any(|window| window[0] == b'~' && window[1].is_ascii_digit())
}

/// DOS device names (including compatibility aliases) with optional
/// extension, on the NFKC-normalized segment.
fn is_dos_device(segment: &str) -> bool {
    let normalized: String = segment.chars().nfkc().collect();
    let lowered = normalized.to_lowercase();
    let stem = lowered.split('.').next().unwrap_or_default();
    let has_extension = lowered.contains('.');
    let names = ["con", "prn", "aux", "nul", "conin$", "conout$", "clock$"];
    if names.contains(&stem) && (stem.len() == lowered.len() || has_extension) {
        return true;
    }
    let numbered = stem.len() == 4
        && (stem.starts_with("com") || stem.starts_with("lpt"))
        && stem.ends_with(|digit: char| digit.is_ascii_digit() && digit != '0');
    numbered && (stem.len() == lowered.len() || has_extension)
}

/// Whether the disposition rule permits the operation at all.
fn disposition_baseline(rule: &Json, operation: &str) -> bool {
    let allowed = |key: &str| {
        rule.get(key)
            .and_then(Json::as_array)
            .is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| entry.as_str() == Some(operation))
            })
    };
    allowed("allowOperations") || allowed("transformOperations")
}

/// The derived-artifact validation.
fn validate_derived(input: &Json, context: &TrustedContext) -> Option<String> {
    let derived = input.get("derivedArtifact").cloned().unwrap_or(Json::Null);
    let provenance_derived = input
        .pointer("/provenance/derived")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let disposition = input
        .get("exportDisposition")
        .and_then(Json::as_str)
        .unwrap_or_default();
    if provenance_derived != !derived.is_null() {
        return Some("derived.provenance-flag-mismatch".to_owned());
    }
    if derived.is_null() {
        return if disposition == "public-aggregate" {
            Some("derived.public-aggregate-required".to_owned())
        } else {
            None
        };
    }
    let labels = input
        .get("dataSensitivity")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    let mut transform_ids: Vec<&str> = Vec::new();
    for transform in derived
        .get("appliedTransforms")
        .and_then(Json::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let transform_id = transform
            .get("transformId")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if transform_ids.contains(&transform_id) {
            return Some("derived.transform-id-conflict".to_owned());
        }
        transform_ids.push(transform_id);
    }
    let mut source_refs: Vec<&str> = Vec::new();
    let mut ordered_sources: Vec<&Json> = Vec::new();
    for source in derived
        .get("sourceArtifacts")
        .and_then(Json::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let source_ref = source
            .get("sourceRef")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if source_refs.contains(&source_ref) {
            return Some("derived.source-ref-conflict".to_owned());
        }
        source_refs.push(source_ref);
        ordered_sources.push(source);
    }
    ordered_sources.sort_by(|left, right| {
        let left_ref = left
            .get("sourceRef")
            .and_then(Json::as_str)
            .unwrap_or_default();
        let right_ref = right
            .get("sourceRef")
            .and_then(Json::as_str)
            .unwrap_or_default();
        locale_compare(left_ref, right_ref)
    });
    let mut source_labels: Vec<&str> = Vec::new();
    for source in &ordered_sources {
        let kind = source
            .get("artifactKind")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if !context.kind_ids().iter().any(|known| known == kind) {
            return Some("derived.source-kind-unknown".to_owned());
        }
        if !same_object(
            source.get("authorityRef").unwrap_or(&Json::Null),
            &authority_ref_json(),
        ) {
            return Some("derived.source-authority-ref-mismatch".to_owned());
        }
        if !same_object(
            source.get("policyRef").unwrap_or(&Json::Null),
            &policy_ref_json(),
        ) {
            return Some("derived.source-policy-ref-mismatch".to_owned());
        }
        if source.get("exportDisposition").and_then(Json::as_str)
            != context.default_disposition(kind)
        {
            return Some("derived.source-disposition-mismatch".to_owned());
        }
        for label in source
            .get("dataSensitivity")
            .and_then(Json::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let label = label.as_str().unwrap_or_default();
            if !source_labels.contains(&label) {
                source_labels.push(label);
            }
        }
    }
    let mut removed: Vec<&str> = source_labels
        .iter()
        .copied()
        .filter(|label| {
            !labels
                .iter()
                .any(|declared| declared.as_str() == Some(*label))
        })
        .collect();
    if !removed.is_empty() {
        let decision = derived
            .get("declassificationDecision")
            .cloned()
            .unwrap_or(Json::Null);
        if decision.is_null() {
            return Some("derived.declassification-required".to_owned());
        }
        if !same_object(
            decision.get("policyRef").unwrap_or(&Json::Null),
            &policy_ref_json(),
        ) {
            return Some("derived.declassification-policy-ref-mismatch".to_owned());
        }
        if decision.get("version").and_then(Json::as_str) != Some(DECISION_VERSION) {
            return Some("derived.declassification-version-mismatch".to_owned());
        }
        if decision.get("outcome").and_then(Json::as_str) != Some("approved") {
            return Some("derived.declassification-outcome-denied".to_owned());
        }
        let mut actual: Vec<&str> = decision
            .get("removedSensitivities")
            .and_then(Json::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Json::as_str)
                    .collect::<Vec<&str>>()
            })
            .unwrap_or_default();
        sort_labels(&mut removed);
        sort_labels(&mut actual);
        if join_labels(&actual) != join_labels(&removed) {
            return Some("derived.declassification-labels-mismatch".to_owned());
        }
    } else if derived.get("declassificationDecision").map(Json::is_null) != Some(true) {
        return Some("derived.unnecessary-declassification".to_owned());
    }
    if derived.get("reevaluated") != Some(&Json::Bool(true)) {
        return Some("derived.not-reevaluated".to_owned());
    }
    if disposition == "public-aggregate" {
        if derived.get("containsSourceRows") == Some(&Json::Bool(true))
            || derived.get("containsSourceIdentities") == Some(&Json::Bool(true))
        {
            return Some("derived.aggregate-contains-source-data".to_owned());
        }
        if !transform_ids.contains(&"aggregate-no-source-rows") {
            return Some("derived.aggregate-transform-missing".to_owned());
        }
        let aggregation = derived
            .get("aggregationDecision")
            .cloned()
            .unwrap_or(Json::Null);
        if aggregation.is_null() {
            return Some("derived.aggregation-decision-required".to_owned());
        }
        if !same_object(
            aggregation.get("policyRef").unwrap_or(&Json::Null),
            &policy_ref_json(),
        ) {
            return Some("derived.aggregation-policy-ref-mismatch".to_owned());
        }
        if aggregation.get("version").and_then(Json::as_str) != Some(DECISION_VERSION) {
            return Some("derived.aggregation-version-mismatch".to_owned());
        }
        if aggregation.get("outcome").and_then(Json::as_str) != Some("approved") {
            return Some("derived.aggregation-outcome-denied".to_owned());
        }
    } else if derived.get("aggregationDecision").map(Json::is_null) != Some(true) {
        return Some("derived.unexpected-aggregation-decision".to_owned());
    }
    None
}

/// `String.prototype.localeCompare` over source refs: the reference
/// sorts sources by ref before collecting labels. For the opaque
/// ASCII ref alphabet this is exactly code-point order.
fn locale_compare(left: &str, right: &str) -> std::cmp::Ordering {
    compare_unicode_code_points(left, right)
}

/// Deterministic label sort (the labels are closed ASCII spellings).
fn sort_labels(labels: &mut [&str]) {
    labels.sort_unstable();
}

fn join_labels(labels: &[&str]) -> String {
    labels.join("\n")
}

/// Evaluate one decision input against the trusted context. The exact
/// port of the reference evaluator: same order, same reason codes,
/// same outcomes.
pub fn evaluate_decision(input: &Json, context: &TrustedContext) -> DecisionEvaluation {
    let malformed = validate_decision_input(input, context);
    if let Some(code) = malformed {
        return DecisionEvaluation {
            malformed: true,
            output: ExportDecisionOutput::deny(&code),
        };
    }
    if let Some(code) = authorizing_evidence_semantic_error(input, context) {
        return non_malformed_deny(code);
    }
    let operation = input
        .pointer("/operation/id")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned();
    let expected_disposition = context.default_disposition(
        input
            .get("artifactKind")
            .and_then(Json::as_str)
            .unwrap_or_default(),
    );
    let Some(expected_disposition) = expected_disposition else {
        return non_malformed_deny("artifact-kind.unknown");
    };
    if input.get("exportDisposition").and_then(Json::as_str) != Some(expected_disposition) {
        return non_malformed_deny("classification.disposition-default-mismatch");
    }
    if let Some(code) = context_coherence(input) {
        return non_malformed_deny(code);
    }
    let conflict_state = input
        .pointer("/conflictResolution/state")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let conflict_has_decision = !input
        .pointer("/conflictResolution/decisionRef")
        .is_some_and(Json::is_null);
    if (conflict_state == "resolved-allow" || conflict_state == "resolved-deny")
        && !conflict_has_decision
    {
        return non_malformed_deny("conflict.missing-decision");
    }
    if (conflict_state == "none" || conflict_state == "unresolved") && conflict_has_decision {
        return non_malformed_deny("conflict.unexpected-decision");
    }
    if conflict_state == "unresolved" || conflict_state == "resolved-deny" {
        return non_malformed_deny("conflict.deny");
    }
    if let Some(value_state) = input.get("valueState") {
        let state = value_state
            .get("state")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if state != "known" {
            return non_malformed_deny(format!("value-state.{state}"));
        }
    }
    let path_state = input
        .pointer("/resourcePath/state")
        .and_then(Json::as_str)
        .unwrap_or_default();
    if path_state != "known" {
        if operation != "local-use" || path_state != "withheld" {
            return non_malformed_deny(format!("path.{path_state}"));
        }
    } else {
        let path_value = input
            .pointer("/resourcePath/value")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if invalid_path(path_value) {
            return non_malformed_deny("path.not-normalized-project-relative");
        }
    }

    // The rule-row lookups below mirror the reference evaluator's
    // `find` semantics: a vocabulary member without a rule row would
    // make the JS throw (exit 1) while this port denies
    // `disposition.*.operation-denied` / `destination.profile-mismatch`
    // (exit 3). The asymmetry is unreachable under the pinned policy
    // (5/5 operation profiles, 6/6 destination profiles, 6/6
    // disposition rules, 9/9 sensitivity rules, all custody-pinned) -
    // a policy change would break custody digests first. Documented
    // in review-119-cline.md F7; no action (fix round 2, C-F7).
    let operation_entry = context
        .operation_profiles()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .find(|entry| entry.get("operation").and_then(Json::as_str) == Some(operation.as_str()))
        .cloned()
        .unwrap_or(Json::Null);
    let allowed_boundaries = operation_entry
        .get("allowedTrustBoundaries")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    let destination_boundary = input
        .pointer("/destination/trustBoundary")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned();
    if !allowed_boundaries
        .iter()
        .any(|entry| entry.as_str() == Some(destination_boundary.as_str()))
    {
        return non_malformed_deny("destination.operation-boundary-mismatch");
    }
    let destination_entry = context
        .destination_profiles()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .find(|entry| {
            entry.get("trustBoundary").and_then(Json::as_str) == Some(destination_boundary.as_str())
        })
        .cloned()
        .unwrap_or(Json::Null);
    let destination_role = input
        .pointer("/destination/repositoryRole")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let destination_audience = input
        .get("audience")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let destination_relation = input
        .pointer("/destination/repositoryRelation")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let destination_tenant = input
        .pointer("/destination/tenantRelation")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let member_list = |key: &str| -> Vec<String> {
        destination_entry
            .get(key)
            .and_then(Json::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    if destination_entry.is_null()
        || !member_list("repositoryRoles")
            .iter()
            .any(|role| role == destination_role)
        || !member_list("repositoryRelations")
            .iter()
            .any(|relation| relation == destination_relation)
        || !member_list("tenantRelations")
            .iter()
            .any(|tenant| tenant == destination_tenant)
        || !member_list("audiences")
            .iter()
            .any(|audience| audience == destination_audience)
    {
        return non_malformed_deny("destination.profile-mismatch");
    }

    let disposition = input
        .get("exportDisposition")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned();
    let disposition_rule = context
        .disposition_rules()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .find(|entry| entry.get("disposition").and_then(Json::as_str) == Some(disposition.as_str()))
        .cloned()
        .unwrap_or(Json::Null);
    if disposition == "forbidden-to-export" && operation != "local-use" {
        return non_malformed_deny("disposition.forbidden-dominates");
    }
    if !disposition_baseline(&disposition_rule, &operation) {
        return non_malformed_deny(format!("disposition.{disposition}.operation-denied"));
    }
    let source_role = input
        .pointer("/source/repositoryRole")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned();
    let source_ref = input
        .pointer("/source/repositoryRef")
        .and_then(Json::as_str)
        .map(str::to_owned);
    let provenance_origin = input
        .pointer("/provenance/origin")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned();
    let provenance_role = input
        .pointer("/provenance/repositoryRole")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned();
    let provenance_ref = input
        .pointer("/provenance/repositoryRef")
        .and_then(Json::as_str)
        .map(str::to_owned);
    if disposition == "local-private" && destination_boundary != "same-local-workspace" {
        return non_malformed_deny("disposition.local-private-boundary");
    }
    if disposition == "consumer-repository-only"
        && (source_role != "consumer-repository"
            || source_ref.is_none()
            || provenance_origin != "consumer-repository"
            || provenance_role != "consumer-repository"
            || provenance_ref != source_ref)
    {
        return non_malformed_deny("disposition.consumer-origin-acl-only");
    }
    if disposition == "consumer-repository-only" && operation == "repository-store" {
        let destination_ref = input
            .pointer("/destination/repositoryRef")
            .and_then(Json::as_str)
            .map(str::to_owned);
        let acl_ref = input.pointer("/provenance/consumerAclPermissionRef");
        let consent_ref = input.pointer("/provenance/consumerRepositoryConsentRef");
        if destination_boundary != "same-repository"
            || destination_relation != "same-origin"
            || destination_tenant != "same-tenant"
            || destination_role != "consumer-repository"
            || source_ref != destination_ref
            || acl_ref.is_some_and(Json::is_null)
            || consent_ref.is_some_and(Json::is_null)
        {
            return non_malformed_deny("disposition.consumer-origin-acl-only");
        }
    }

    let sensitivity_order = policy_vocabulary(context, "dataSensitivity");
    let mut ordered_labels: Vec<&str> = input
        .get("dataSensitivity")
        .and_then(Json::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Json::as_str)
                .collect::<Vec<&str>>()
        })
        .unwrap_or_default();
    ordered_labels.sort_by(|left, right| {
        let left_index = sensitivity_order
            .iter()
            .position(|entry| entry.as_str() == Some(*left))
            .unwrap_or(usize::MAX);
        let right_index = sensitivity_order
            .iter()
            .position(|entry| entry.as_str() == Some(*right))
            .unwrap_or(usize::MAX);
        left_index.cmp(&right_index)
    });
    let sensitivity_rules = context.sensitivity_rules().cloned().unwrap_or_default();
    for label in &ordered_labels {
        let rule = sensitivity_rules
            .iter()
            .find(|entry| entry.get("label").and_then(Json::as_str) == Some(*label))
            .cloned()
            .unwrap_or(Json::Null);
        let allowed = |key: &str, value: &str| -> bool {
            rule.get(key)
                .and_then(Json::as_array)
                .is_some_and(|entries| entries.iter().any(|entry| entry.as_str() == Some(value)))
        };
        if !allowed("allowedOperations", &operation)
            || !allowed("allowedTrustBoundaries", &destination_boundary)
            || !allowed("allowedAudiences", destination_audience)
            || !allowed("allowedRepositoryRelations", destination_relation)
            || !allowed("allowedTenantRelations", destination_tenant)
        {
            return non_malformed_deny(format!("sensitivity.{label}.denied"));
        }
    }

    let rules_for = |labels: &[&str], key: &str| -> Vec<&[Json]> {
        labels
            .iter()
            .filter_map(|label| {
                sensitivity_rules
                    .iter()
                    .find(|entry| entry.get("label").and_then(Json::as_str) == Some(*label))
            })
            .map(|rule| {
                rule.get(key)
                    .and_then(Json::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
            })
            .collect()
    };
    let every_rule_allows = |labels: &[&str], key: &str, value: &str| -> bool {
        rules_for(labels, key)
            .iter()
            .all(|entries| entries.iter().any(|entry| entry.as_str() == Some(value)))
    };
    let allow_operations = disposition_rule
        .get("allowOperations")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    let transform_operations = disposition_rule
        .get("transformOperations")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    let baseline_operations: Vec<String> = allow_operations
        .iter()
        .chain(transform_operations.iter())
        .filter_map(Json::as_str)
        .filter(|value| every_rule_allows(&ordered_labels, "allowedOperations", value))
        .map(str::to_owned)
        .collect();
    let baseline_boundaries: Vec<String> = allowed_boundaries
        .iter()
        .filter_map(Json::as_str)
        .filter(|value| every_rule_allows(&ordered_labels, "allowedTrustBoundaries", value))
        .map(str::to_owned)
        .collect();
    let destination_audiences = member_list("audiences");
    let baseline_audiences: Vec<String> = destination_audiences
        .iter()
        .filter(|value| every_rule_allows(&ordered_labels, "allowedAudiences", value))
        .cloned()
        .collect();
    for constraint in input
        .get("constraints")
        .and_then(Json::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let any_broadening = [
            ("allowedOperations", &baseline_operations),
            ("allowedTrustBoundaries", &baseline_boundaries),
            ("allowedAudiences", &baseline_audiences),
        ]
        .iter()
        .any(|(key, baseline)| {
            constraint
                .get(key)
                .and_then(Json::as_array)
                .is_some_and(|entries| {
                    entries.iter().any(|entry| {
                        entry
                            .as_str()
                            .is_some_and(|value| !baseline.iter().any(|allowed| allowed == value))
                    })
                })
        });
        if any_broadening {
            return non_malformed_deny("constraint.broadening-forbidden");
        }
        let narrowed = [
            ("allowedOperations", operation.as_str()),
            ("allowedTrustBoundaries", destination_boundary.as_str()),
            ("allowedAudiences", destination_audience),
        ]
        .iter()
        .any(|(key, value)| {
            constraint
                .get(key)
                .and_then(Json::as_array)
                .is_some_and(|entries| !entries.iter().any(|entry| entry.as_str() == Some(*value)))
        });
        if narrowed {
            return non_malformed_deny("constraint.narrowed-deny");
        }
    }

    if let Some(code) = validate_derived(input, context) {
        return non_malformed_deny(code);
    }
    let export_like =
        operation == "repository-store" || operation == "transfer" || operation == "publish";
    let public_fixture_refs = [
        input.pointer("/provenance/publicFixturePermissionRef"),
        input.pointer("/provenance/publicFixtureLicenseRef"),
        input.pointer("/provenance/publicFixtureConsentRef"),
    ];
    let consumer_refs = [
        input.pointer("/provenance/consumerAclPermissionRef"),
        input.pointer("/provenance/consumerRepositoryConsentRef"),
    ];
    if disposition != "public-fixture"
        && public_fixture_refs
            .iter()
            .any(|entry| entry.is_some_and(|value| !value.is_null()))
    {
        return non_malformed_deny("evidence.unexpected-public-fixture");
    }
    if !(disposition == "consumer-repository-only" && operation == "repository-store")
        && consumer_refs
            .iter()
            .any(|entry| entry.is_some_and(|value| !value.is_null()))
    {
        return non_malformed_deny("evidence.unexpected-consumer-repository");
    }
    let uses_general_transfer_consent = export_like
        && disposition != "public-fixture"
        && !(disposition == "consumer-repository-only" && operation == "repository-store");
    let transfer_consent = input.pointer("/provenance/exportTransferConsentRef");
    if uses_general_transfer_consent && transfer_consent.is_some_and(Json::is_null) {
        return non_malformed_deny("provenance.consent-required");
    }
    if !uses_general_transfer_consent && transfer_consent.is_some_and(|value| !value.is_null()) {
        return non_malformed_deny("evidence.unexpected-export-consent");
    }
    if disposition == "public-fixture"
        && provenance_origin == "synthetic"
        && public_fixture_refs
            .iter()
            .any(|entry| entry.is_some_and(|value| !value.is_null()))
    {
        return non_malformed_deny("evidence.unexpected-public-fixture");
    }
    if disposition == "public-fixture"
        && export_like
        && provenance_origin != "synthetic"
        && public_fixture_refs
            .iter()
            .any(|entry| entry.is_some_and(Json::is_null))
    {
        return non_malformed_deny("fixture.permission-license-consent-required");
    }

    let transforms_operations: Vec<String> = transform_operations
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect();
    if transforms_operations.contains(&operation) {
        let mut transforms = vec!["redact-content".to_owned()];
        if ordered_labels.contains(&"credential-secret") {
            transforms.push("redact-secrets".to_owned());
        }
        if ordered_labels.contains(&"personal-pii") {
            transforms.push("redact-pii".to_owned());
        }
        return DecisionEvaluation {
            malformed: false,
            output: ExportDecisionOutput::transform_required(transforms),
        };
    }
    let source_transfer = operation_entry
        .get("sourceTransfer")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let disposition_transfer = disposition_rule
        .get("sourceTransferAllowed")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let source_transfer_allowed = source_transfer && disposition_transfer;
    DecisionEvaluation {
        malformed: false,
        output: ExportDecisionOutput::new(
            super::output::ExportDecision::Allow,
            vec!["policy.allow".to_owned()],
            Vec::new(),
            Vec::new(),
            source_transfer_allowed,
        ),
    }
}

/// A non-malformed deny with one reason code.
fn non_malformed_deny(code: impl Into<String>) -> DecisionEvaluation {
    DecisionEvaluation {
        malformed: false,
        output: ExportDecisionOutput::deny(&code.into()),
    }
}

/// Construct the typed decision input from wire JSON that
/// [`validate_decision_input`] has accepted. Every conversion here is
/// a validated invariant; no path can fail on validated input.
pub fn decision_input_from_validated(
    input: &Json,
    context: &TrustedContext,
) -> ExportDecisionInput {
    if validate_decision_input(input, context).is_some() {
        panic!("decision_input_from_validated requires a validated input");
    }
    let parse = |value: Option<&Json>| -> String {
        value.and_then(Json::as_str).unwrap_or_default().to_owned()
    };
    let repository_role = |value: Option<&Json>| -> RepositoryRole {
        RepositoryRole::parse(value.and_then(Json::as_str).unwrap_or_default())
            .expect("validated repository role")
    };
    let source = input.get("source").cloned().unwrap_or(Json::Null);
    let destination = input.get("destination").cloned().unwrap_or(Json::Null);
    let provenance = input.get("provenance").cloned().unwrap_or(Json::Null);
    let conflict = input
        .get("conflictResolution")
        .cloned()
        .unwrap_or(Json::Null);

    let evidence = |value: Option<&Json>| -> Option<super::types::AuthorizingEvidence> {
        value
            .filter(|value| !value.is_null())
            .map(evidence_from_validated)
    };

    ExportDecisionInput::new(
        ref_from_validated(input.get("decisionContractRef")),
        super::types::ContractRef::new(
            parse(input.pointer("/authorityRef/contractId")),
            parse(input.pointer("/authorityRef/version")),
            parse(input.pointer("/authorityRef/digest")),
        ),
        super::types::PolicyRef::new(
            parse(input.pointer("/policyRef/policyId")),
            parse(input.pointer("/policyRef/version")),
            parse(input.pointer("/policyRef/digest")),
        ),
        super::types::ContractRef::new(
            parse(input.pointer("/authorizingEvidenceContractRef/contractId")),
            parse(input.pointer("/authorizingEvidenceContractRef/version")),
            parse(input.pointer("/authorizingEvidenceContractRef/digest")),
        ),
        super::types::SubjectProfileRef::new(
            parse(input.pointer("/authorizationSubjectProfileRef/profileId")),
            parse(input.pointer("/authorizationSubjectProfileRef/version")),
            parse(input.pointer("/authorizationSubjectProfileRef/digest")),
        ),
        parse(input.get("artifactKind")),
        parse(input.get("artifactRef")),
        super::input::Operation::new(
            OperationId::parse(
                input
                    .pointer("/operation/id")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )
            .expect("validated operation"),
            parse(input.pointer("/operation/version")),
        ),
        super::input::Endpoint::new(
            repository_role(source.get("repositoryRole")),
            source
                .get("repositoryRef")
                .and_then(Json::as_str)
                .map(str::to_owned),
        ),
        super::input::Destination::new(
            repository_role(destination.get("repositoryRole")),
            destination
                .get("repositoryRef")
                .and_then(Json::as_str)
                .map(str::to_owned),
            TrustBoundary::parse(
                destination
                    .get("trustBoundary")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )
            .expect("validated trust boundary"),
            RepositoryRelation::parse(
                destination
                    .get("repositoryRelation")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )
            .expect("validated repository relation"),
            TenantRelation::parse(
                destination
                    .get("tenantRelation")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )
            .expect("validated tenant relation"),
        ),
        Audience::parse(
            input
                .get("audience")
                .and_then(Json::as_str)
                .unwrap_or_default(),
        )
        .expect("validated audience"),
        input
            .get("dataSensitivity")
            .and_then(Json::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| {
                        DataSensitivity::parse(entry.as_str().unwrap_or_default())
                            .expect("validated sensitivity")
                    })
                    .collect()
            })
            .unwrap_or_default(),
        ExportDisposition::parse(
            input
                .get("exportDisposition")
                .and_then(Json::as_str)
                .unwrap_or_default(),
        )
        .expect("validated disposition"),
        super::input::Provenance::new(
            ProvenanceOrigin::parse(
                provenance
                    .get("origin")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )
            .expect("validated origin"),
            repository_role(provenance.get("repositoryRole")),
            provenance
                .get("repositoryRef")
                .and_then(Json::as_str)
                .map(str::to_owned),
            provenance
                .get("synthetic")
                .and_then(Json::as_bool)
                .unwrap_or(false),
            provenance
                .get("derived")
                .and_then(Json::as_bool)
                .unwrap_or(false),
            classification_ref_from_validated(provenance.get("classificationRef")),
            evidence(provenance.get("publicFixturePermissionRef")),
            evidence(provenance.get("publicFixtureLicenseRef")),
            evidence(provenance.get("publicFixtureConsentRef")),
            evidence(provenance.get("consumerAclPermissionRef")),
            evidence(provenance.get("consumerRepositoryConsentRef")),
            evidence(provenance.get("exportTransferConsentRef")),
        ),
        {
            let path = input.get("resourcePath").cloned().unwrap_or(Json::Null);
            let state = SensitivityState::parse(
                path.get("state").and_then(Json::as_str).unwrap_or_default(),
            )
            .expect("validated path state");
            if state == SensitivityState::Known {
                super::types::ResourcePath::known(parse(path.get("value")))
            } else {
                super::types::ResourcePath::state_only(state)
            }
        },
        input.get("valueState").map(|value| {
            let state = SensitivityState::parse(
                value
                    .get("state")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )
            .expect("validated value state");
            if state == SensitivityState::Known {
                ValueState::known(value.get("value").cloned().unwrap_or(Json::Null))
            } else {
                ValueState::state_only(state)
            }
        }),
        super::input::ConflictResolution::new(
            ConflictState::parse(
                conflict
                    .get("state")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )
            .expect("validated conflict state"),
            evidence(conflict.get("decisionRef")),
        ),
        input
            .get("constraints")
            .and_then(Json::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .map(|constraint| {
                        super::input::Constraint::new(
                            ConstraintScope::parse(
                                constraint
                                    .get("scope")
                                    .and_then(Json::as_str)
                                    .unwrap_or_default(),
                            )
                            .expect("validated constraint scope"),
                            super::types::AuditRef::new(
                                parse(constraint.pointer("/constraintRef/id")),
                                parse(constraint.pointer("/constraintRef/version")),
                                parse(constraint.pointer("/constraintRef/evidenceDigest")),
                            ),
                            constraint
                                .get("allowedOperations")
                                .and_then(Json::as_array)
                                .map(|operations| {
                                    operations
                                        .iter()
                                        .map(|operation| {
                                            OperationId::parse(
                                                operation.as_str().unwrap_or_default(),
                                            )
                                            .expect("validated constraint operation")
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                            constraint
                                .get("allowedTrustBoundaries")
                                .and_then(Json::as_array)
                                .map(|boundaries| {
                                    boundaries
                                        .iter()
                                        .map(|boundary| {
                                            TrustBoundary::parse(
                                                boundary.as_str().unwrap_or_default(),
                                            )
                                            .expect("validated constraint boundary")
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                            constraint
                                .get("allowedAudiences")
                                .and_then(Json::as_array)
                                .map(|audiences| {
                                    audiences
                                        .iter()
                                        .map(|audience| {
                                            Audience::parse(audience.as_str().unwrap_or_default())
                                                .expect("validated constraint audience")
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        input.get("derivedArtifact").map(|derived| {
            super::input::DerivedArtifact::new(
                derived
                    .get("sourceArtifacts")
                    .and_then(Json::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .map(|source| {
                                super::input::SourceArtifact::new(
                                    parse(source.get("sourceRef")),
                                    parse(source.get("artifactKind")),
                                    super::types::ContractRef::new(
                                        parse(source.pointer("/authorityRef/contractId")),
                                        parse(source.pointer("/authorityRef/version")),
                                        parse(source.pointer("/authorityRef/digest")),
                                    ),
                                    super::types::PolicyRef::new(
                                        parse(source.pointer("/policyRef/policyId")),
                                        parse(source.pointer("/policyRef/version")),
                                        parse(source.pointer("/policyRef/digest")),
                                    ),
                                    classification_ref_from_validated(
                                        source.get("classificationRef"),
                                    ),
                                    source
                                        .get("dataSensitivity")
                                        .and_then(Json::as_array)
                                        .map(|labels| {
                                            labels
                                                .iter()
                                                .map(|label| {
                                                    DataSensitivity::parse(
                                                        label.as_str().unwrap_or_default(),
                                                    )
                                                    .expect("validated source sensitivity")
                                                })
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                    ExportDisposition::parse(
                                        source
                                            .get("exportDisposition")
                                            .and_then(Json::as_str)
                                            .unwrap_or_default(),
                                    )
                                    .expect("validated source disposition"),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                derived
                    .get("appliedTransforms")
                    .and_then(Json::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .map(|transform| {
                                super::input::AppliedTransform::new(
                                    TransformId::parse(
                                        transform
                                            .get("transformId")
                                            .and_then(Json::as_str)
                                            .unwrap_or_default(),
                                    )
                                    .expect("validated transform"),
                                    parse(transform.get("version")),
                                    parse(transform.get("evidenceDigest")),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                derived
                    .pointer("/declassificationDecision")
                    .filter(|decision| !decision.is_null())
                    .map(|decision| {
                        super::input::DeclassificationDecision::new(
                            super::types::PolicyRef::new(
                                parse(decision.pointer("/policyRef/policyId")),
                                parse(decision.pointer("/policyRef/version")),
                                parse(decision.pointer("/policyRef/digest")),
                            ),
                            evidence_from_validated(
                                decision.get("decisionRef").unwrap_or(&Json::Null),
                            ),
                            parse(decision.get("version")),
                            super::types::EvidenceOutcome::parse(
                                decision
                                    .get("outcome")
                                    .and_then(Json::as_str)
                                    .unwrap_or_default(),
                            )
                            .expect("validated declassification outcome"),
                            decision
                                .get("removedSensitivities")
                                .and_then(Json::as_array)
                                .map(|labels| {
                                    labels
                                        .iter()
                                        .map(|label| {
                                            DataSensitivity::parse(
                                                label.as_str().unwrap_or_default(),
                                            )
                                            .expect("validated removed sensitivity")
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                        )
                    }),
                derived
                    .pointer("/aggregationDecision")
                    .filter(|decision| !decision.is_null())
                    .map(|decision| {
                        super::input::AggregationDecision::new(
                            super::types::PolicyRef::new(
                                parse(decision.pointer("/policyRef/policyId")),
                                parse(decision.pointer("/policyRef/version")),
                                parse(decision.pointer("/policyRef/digest")),
                            ),
                            evidence_from_validated(
                                decision.get("decisionRef").unwrap_or(&Json::Null),
                            ),
                            parse(decision.get("version")),
                            super::types::EvidenceOutcome::parse(
                                decision
                                    .get("outcome")
                                    .and_then(Json::as_str)
                                    .unwrap_or_default(),
                            )
                            .expect("validated aggregation outcome"),
                            decision
                                .get("removesSourceRows")
                                .and_then(Json::as_bool)
                                .unwrap_or(false),
                            decision
                                .get("removesSourceIdentities")
                                .and_then(Json::as_bool)
                                .unwrap_or(false),
                        )
                    }),
                derived
                    .get("containsSourceRows")
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
                derived
                    .get("containsSourceIdentities")
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
                derived
                    .get("reevaluated")
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
            )
        }),
    )
}

/// One validated evidence record into the typed shape.
fn evidence_from_validated(value: &Json) -> super::types::AuthorizingEvidence {
    let parse = |pointer: &str| -> String {
        value
            .pointer(pointer)
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    super::types::AuthorizingEvidence::new(
        parse("/contractId"),
        parse("/version"),
        parse("/digest"),
        parse("/evidenceKind"),
        parse("/purpose"),
        super::types::EvidenceOutcome::parse(
            value
                .get("outcome")
                .and_then(Json::as_str)
                .unwrap_or_default(),
        )
        .expect("validated evidence outcome"),
        parse("/evidenceId"),
        super::types::VerificationState::parse(
            value
                .get("verificationState")
                .and_then(Json::as_str)
                .unwrap_or_default(),
        )
        .expect("validated verification state"),
        super::types::FreshnessState::parse(
            value
                .get("freshnessState")
                .and_then(Json::as_str)
                .unwrap_or_default(),
        )
        .expect("validated freshness state"),
        super::types::EvidenceBinding::new(
            super::types::SubjectProfileRef::new(
                parse("/binding/subjectProfileRef/profileId"),
                parse("/binding/subjectProfileRef/version"),
                parse("/binding/subjectProfileRef/digest"),
            ),
            parse("/binding/subjectDigest"),
        ),
    )
}

/// One validated classification reference into the typed shape.
fn classification_ref_from_validated(
    value: Option<&Json>,
) -> super::types::ClassificationDecisionRef {
    let value = value.cloned().unwrap_or(Json::Null);
    let parse = |pointer: &str| -> String {
        value
            .pointer(pointer)
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    super::types::ClassificationDecisionRef::new(
        parse("/contractId"),
        parse("/version"),
        parse("/digest"),
        parse("/decisionId"),
        parse("/evidenceDigest"),
    )
}

/// A validated `{contractId, version}` decision reference.
fn ref_from_validated(value: Option<&Json>) -> super::types::DecisionContractRef {
    let value = value.cloned().unwrap_or(Json::Null);
    super::types::DecisionContractRef::new(
        value
            .get("contractId")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned(),
        value
            .get("version")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::output::ExportDecision;

    fn fixtures_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/privacy")
    }

    fn fixture_vectors(name: &str) -> Vec<(String, Json, String, String, u8)> {
        let path = fixtures_dir().join(format!("{name}.json"));
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("fixture {name} must exist: {error}"));
        let parsed: Json = serde_json::from_slice(&bytes).expect("fixture JSON parses");
        parsed
            .as_array()
            .expect("fixture vector array")
            .iter()
            .map(|entry| {
                let id = entry["id"].as_str().unwrap_or_default().to_owned();
                let expected = &entry["expected"];
                (
                    id,
                    entry["decision"].clone(),
                    expected["decision"].as_str().unwrap_or_default().to_owned(),
                    expected["reasonCode"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    expected["exitCode"].as_u64().unwrap_or_default() as u8,
                )
            })
            .collect()
    }

    /// The entire committed fixture corpus produces the exact expected
    /// outcomes through the Rust evaluator.
    #[test]
    fn fixture_corpus_matches_expected_outcomes() {
        let context = TrustedContext::embedded().expect("trusted context");
        let mut vector_count = 0;
        for name in [
            "allowed",
            "forbidden",
            "transform-required",
            "ambiguous",
            "malformed",
        ] {
            for (id, decision, want_decision, want_reason, want_exit) in fixture_vectors(name) {
                let evaluated = evaluate_decision(&decision, context);
                let got_exit = if evaluated.malformed {
                    1
                } else {
                    match evaluated.output.decision() {
                        ExportDecision::Allow => 0,
                        _ => 3,
                    }
                };
                assert_eq!(
                    evaluated.output.decision().as_str(),
                    want_decision,
                    "{name}/{id}"
                );
                assert_eq!(
                    evaluated.output.reason_codes().first().map(String::as_str),
                    Some(want_reason.as_str()),
                    "{name}/{id}"
                );
                assert_eq!(got_exit, want_exit, "{name}/{id}");
                assert!(!evaluated.output.reason_codes().is_empty(), "{name}/{id}");
                assert_eq!(
                    evaluated.output.effective_refs().policy_ref().digest(),
                    refs::POLICY_DIGEST,
                    "{name}/{id}"
                );
                vector_count += 1;
            }
        }
        assert_eq!(vector_count, 7);
    }

    /// A validated fixture input round-trips through the typed
    /// construction: serialize(typed) equals the original wire input.
    #[test]
    fn validated_input_constructs_the_typed_value() {
        let context = TrustedContext::embedded().expect("trusted context");
        for (id, decision, ..) in fixture_vectors("allowed") {
            assert!(
                validate_decision_input(&decision, context).is_none(),
                "{id} must validate"
            );
            let typed = decision_input_from_validated(&decision, context);
            let wire = serde_json::to_value(&typed).expect("typed input serializes");
            assert_eq!(wire, decision, "{id} round-trips exactly");
        }
    }

    /// Mutation attacks over the base fixture: the exact reference
    /// reason codes fire in the reference order.
    #[test]
    fn mutation_attacks_match_the_reference_codes() {
        let context = TrustedContext::embedded().expect("trusted context");
        let base = &fixture_vectors("allowed")[0].1;
        let expect = |mutated: &Json| -> String {
            let evaluated = evaluate_decision(mutated, context);
            evaluated
                .output
                .reason_codes()
                .first()
                .cloned()
                .unwrap_or_default()
        };

        // Unknown member — shape.
        let mut shape = base.clone();
        shape
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), Json::Bool(true));
        assert_eq!(expect(&shape), "input.shape");

        // Wrong policy digest.
        let mut policy = base.clone();
        policy.as_object_mut().unwrap()["policyRef"]["digest"] =
            Json::String(format!("sha256:{}", "0".repeat(64)));
        assert_eq!(expect(&policy), "input.policy-ref");

        // Unknown artifact kind.
        let mut kind = base.clone();
        kind.as_object_mut().unwrap()["artifactKind"] = Json::String("nonexistent.kind".to_owned());
        assert_eq!(expect(&kind), "artifact-kind.unknown");

        // Default mismatch.
        let mut mismatch = base.clone();
        mismatch.as_object_mut().unwrap()["exportDisposition"] =
            Json::String("local-private".to_owned());
        assert_eq!(
            expect(&mismatch),
            "classification.disposition-default-mismatch"
        );

        // Unverified evidence (verification fires before binding).
        let mut unverified = base.clone();
        unverified.as_object_mut().unwrap()["provenance"]["publicFixturePermissionRef"] = serde_json::json!({
            "contractId": refs::AUTHORIZING_EVIDENCE_CONTRACT_ID,
            "version": refs::DECISION_FAMILY_VERSION,
            "digest": format!("sha256:{}", refs::AUTHORIZING_EVIDENCE_RAW_SHA256),
            "evidenceKind": "public-fixture-permission",
            "purpose": "authorize-public-fixture-permission",
            "outcome": "granted",
            "evidenceId": format!("evidence-sha256:{}", "4".repeat(64)),
            "verificationState": "unverified",
            "freshnessState": "current",
            "binding": {
                "subjectProfileRef": {
                    "profileId": refs::AUTHORIZATION_SUBJECT_PROFILE_ID,
                    "version": refs::DECISION_FAMILY_VERSION,
                    "digest": format!("sha256:{}", refs::AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256),
                },
                "subjectDigest": format!("subject-sha256:{}", "b".repeat(64)),
            },
        });
        assert_eq!(expect(&unverified), "evidence.unverified");

        // Value state unknown.
        let mut value = base.clone();
        value.as_object_mut().unwrap()["valueState"] = serde_json::json!({"state": "unknown"});
        assert_eq!(expect(&value), "value-state.unknown");

        // Unsupported path state on a publish.
        let mut path = base.clone();
        path.as_object_mut().unwrap()["resourcePath"] = serde_json::json!({"state": "unsupported"});
        assert_eq!(expect(&path), "path.unsupported");

        // Absolute path.
        let mut absolute = base.clone();
        absolute.as_object_mut().unwrap()["resourcePath"] =
            serde_json::json!({"state": "known", "value": "/abs/secret.txt"});
        assert_eq!(expect(&absolute), "path.not-normalized-project-relative");

        // A trust boundary the operation profile never allows — the
        // constraint broadens the effective baseline.
        let mut broadening = base.clone();
        broadening.as_object_mut().unwrap()["constraints"] = serde_json::json!([{
            "scope": "project",
            "constraintRef": {
                "id": "audit-1",
                "version": "0.2.16",
                "evidenceDigest": format!("sha256:{}", "a".repeat(64)),
            },
            "allowedOperations": ["local-use", "repository-store"],
            "allowedTrustBoundaries": ["same-local-workspace"],
            "allowedAudiences": ["operator-only"],
        }]);
        assert_eq!(expect(&broadening), "constraint.broadening-forbidden");

        // A derived artifact that was never reevaluated.
        let mut derived = base.clone();
        let object = derived.as_object_mut().unwrap();
        object["provenance"]["synthetic"] = Json::Bool(false);
        object["provenance"]["derived"] = Json::Bool(true);
        object["provenance"]["origin"] = Json::String("derived".to_owned());
        let derived_artifact = serde_json::json!({
            "sourceArtifacts": [{
                "sourceRef": format!("source-sha256:{}", "2".repeat(64)),
                "artifactKind": "fixture",
                "authorityRef": object["authorityRef"].clone(),
                "policyRef": object["policyRef"].clone(),
                "classificationRef": object["provenance"]["classificationRef"].clone(),
                "dataSensitivity": ["public"],
                "exportDisposition": "public-fixture",
            }],
            "appliedTransforms": [{
                "transformId": "redact-content",
                "version": "0.2.16",
                "evidenceDigest": format!("sha256:{}", "3".repeat(64)),
            }],
            "declassificationDecision": null,
            "aggregationDecision": null,
            "containsSourceRows": false,
            "containsSourceIdentities": false,
            "reevaluated": false,
        });
        object.insert("derivedArtifact".to_owned(), derived_artifact);
        assert_eq!(expect(&derived), "derived.not-reevaluated");
    }

    /// The subject digest is stable under evidence-only changes and
    /// set permutations, and moves with semantic content.
    #[test]
    fn subject_digest_semantics() {
        let context = TrustedContext::embedded().expect("trusted context");
        let multi_label = &fixture_vectors("allowed")[1].1;
        let profile = context.authorization_subject_profile();

        let digest = authorization_subject_digest(multi_label, profile);
        assert!(digest.starts_with("subject-sha256:"));
        assert_eq!(digest.len(), "subject-sha256:".len() + 64);

        // Excluded members never move the digest: removing or adding
        // them leaves the projection identical.
        let mut without_conflict = multi_label.clone();
        without_conflict
            .pointer_mut("/conflictResolution")
            .and_then(Json::as_object_mut)
            .unwrap()
            .insert(
                "decisionRef".to_owned(),
                serde_json::json!({"evidenceId": "x"}),
            );
        let projected_base = authorization_subject_projection(multi_label, profile);
        let projected_with = authorization_subject_projection(&without_conflict, profile);
        assert_eq!(canonical(&projected_base), canonical(&projected_with));

        // Permuting the sensitivity set never moves the digest.
        let mut permuted = multi_label.clone();
        let sensitivities = permuted
            .get("dataSensitivity")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        let reversed: Vec<Json> = sensitivities.iter().rev().cloned().collect();
        *permuted.get_mut("dataSensitivity").unwrap() = Json::Array(reversed);
        assert_eq!(
            authorization_subject_digest(&permuted, profile),
            digest,
            "set canonicalization makes permutations stable"
        );

        // Semantic content moves the digest.
        let mut changed = multi_label.clone();
        changed.as_object_mut().unwrap()["artifactRef"] =
            Json::String(format!("artifact-sha256:{}", "9".repeat(64)));
        assert_ne!(authorization_subject_digest(&changed, profile), digest);
    }

    /// The closed path grammar: every class of unsafe representation
    /// denies, and safe relative paths pass.
    #[test]
    fn path_grammar_is_exact() {
        assert!(!invalid_path("docs/readme.md"));
        assert!(!invalid_path("a/b/c.txt"));
        assert!(invalid_path("../secret"));
        assert!(invalid_path("/absolute/path"));
        assert!(invalid_path("//unc/share"));
        assert!(invalid_path("~home/x"));
        assert!(invalid_path("a\\\\b"));
        assert!(invalid_path("C:/data/x"));
        assert!(invalid_path("a/b%20c"));
        assert!(invalid_path("a/./b"));
        assert!(invalid_path("a//b"));
        assert!(invalid_path("a/b."));
        assert!(invalid_path("a/b "));
        assert!(invalid_path("a/b:c"));
        assert!(invalid_path("a/con"));
        assert!(invalid_path("a/COM1.txt"));
        assert!(invalid_path("a/com\u{b9}"));
        assert!(invalid_path("a/LPT1"));
        assert!(invalid_path("a/CLOCK$"));
        assert!(!invalid_path("a/commit"));
        assert!(!invalid_path("a/com1x"));
        assert!(invalid_path("a/x~1"));
        assert!(invalid_path("a/cafe\u{301}")); // non-NFC e + combining acute
        assert!(!invalid_path("a/café")); // precomposed é is NFC
        assert!(invalid_path("a/b\u{7}"));
        assert!(invalid_path("a/CONIN$"));
        assert!(invalid_path("a/conout$.txt"));
        assert!(!invalid_path("a/conduit"));
        assert!(!invalid_path("a/clock"));
    }

    /// The evaluated decision output serializes to the exact closed
    /// output-schema shape for all three branches.
    #[test]
    fn evaluated_output_wire_shape_is_closed() {
        let context = TrustedContext::embedded().expect("trusted context");
        let decision = &fixture_vectors("allowed")[0].1;
        let evaluated = evaluate_decision(decision, context);
        let wire = serde_json::to_value(&evaluated.output).expect("output serializes");
        let object = wire.as_object().unwrap();
        assert_eq!(object.len(), 6);
        assert_eq!(object["decision"], "allow");
        assert_eq!(object["reasonCodes"], serde_json::json!(["policy.allow"]));
        assert_eq!(object["requiredTransforms"], serde_json::json!([]));
        assert_eq!(object["derivedArtifactRequirements"], serde_json::json!([]));
        assert_eq!(object["sourceTransferAllowed"], true);
        assert_eq!(object["effectiveRefs"].as_object().unwrap().len(), 8);

        let denied = evaluate_decision(&fixture_vectors("forbidden")[0].1, context);
        assert_eq!(denied.output.decision().as_str(), "deny");
        assert!(!denied.output.source_transfer_allowed());

        let transformed = evaluate_decision(&fixture_vectors("transform-required")[0].1, context);
        assert_eq!(transformed.output.decision().as_str(), "transform-required");
        assert!(!transformed.output.source_transfer_allowed());
        assert!(!transformed.output.required_transforms().is_empty());
        assert_eq!(
            transformed.output.derived_artifact_requirements(),
            crate::privacy::output::TRANSFORM_REQUIREMENTS
        );
    }
}
