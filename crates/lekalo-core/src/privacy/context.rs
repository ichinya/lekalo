//! The custody-verified trusted context of the frozen #120 family
//! (issue #119).
//!
//! [`TrustedContext::embedded`] loads the exact contract bytes
//! embedded from `contracts/`, verifies every pinned SHA-256 (raw
//! bytes and `digest  filename` sidecars), parses each document, and
//! re-checks the complete custody chain — manifest acceptance, exact
//! reference identity, the canonical policy identity projection,
//! registry closure (57 authority kinds), the closed default table,
//! the constraint/repository-identity/evidence policies, and the
//! subject-profile root inventory — exactly like the reference
//! checker. Any mismatch refuses before any decision is evaluated:
//! a caller cannot replace the policy, recalculate a digest, or
//! supply a local taxonomy.

use std::fmt;
use std::sync::LazyLock;

use serde_json::Value as Json;

use super::canonical::{canonical, compare_unicode_code_points};
use super::refs;
use crate::digest::sha256_hex;

/// The exact embedded accepted-policy manifest bytes.
pub const MANIFEST_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-policy.v0.3.2.manifest.json");
/// The exact embedded manifest sidecar bytes.
pub const MANIFEST_SIDECAR_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-policy.v0.3.2.manifest.sha256");
/// The exact embedded accepted policy bytes.
pub const POLICY_BYTES: &[u8] = include_bytes!("../../../../contracts/privacy-policy.v0.3.2.json");
/// The exact embedded policy sidecar bytes.
pub const POLICY_SIDECAR_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-policy.v0.3.2.sha256");
/// The exact embedded classification decision contract bytes.
pub const CLASSIFICATION_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-policy.v0.2.16.classification.json");
/// The exact embedded classification sidecar bytes.
pub const CLASSIFICATION_SIDECAR_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-policy.v0.2.16.classification.sha256");
/// The exact embedded authorizing-evidence contract bytes.
pub const AUTHORIZING_EVIDENCE_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-authorizing-evidence.v0.2.16.json");
/// The exact embedded authorizing-evidence sidecar bytes.
pub const AUTHORIZING_EVIDENCE_SIDECAR_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-authorizing-evidence.v0.2.16.sha256");
/// The exact embedded authorization-subject-profile bytes.
pub const SUBJECT_PROFILE_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-authorization-subject-profile.v0.2.16.json");
/// The exact embedded subject-profile sidecar bytes.
pub const SUBJECT_PROFILE_SIDECAR_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-authorization-subject-profile.v0.2.16.sha256");
/// The exact embedded strict input schema bytes.
pub const INPUT_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-export.schema.v0.3.2.json");
/// The exact embedded strict output schema bytes.
pub const OUTPUT_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-export.schema.v0.3.2.output.json");
/// The exact embedded CLI startup-error schema bytes.
pub const CLI_ERROR_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-cli-error.schema.v0.2.16.json");
/// The exact embedded classification-ref schema bytes.
pub const CLASSIFICATION_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../../contracts/privacy-export.schema.v0.2.16.classification.json");
/// The exact embedded authority matrix bytes.
pub const AUTHORITY_BYTES: &[u8] =
    include_bytes!("../../../../contracts/authority-matrix.v0.3.2.json");

/// Why the embedded contract set could not be trusted. The code is
/// the exact custody reason spelling of the reference checker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustodyError {
    code: String,
}

impl CustodyError {
    /// The exact custody reason code.
    pub fn code(&self) -> &str {
        &self.code
    }
}

impl fmt::Display for CustodyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.code)
    }
}

impl std::error::Error for CustodyError {}

/// The closed 57-kind authority registry size pinned by the accepted
/// policy.
const AUTHORITY_KIND_COUNT: usize = 57;

/// The loaded, digest-verified, immutable contract context. Pure
/// metadata: every member is a parsed frozen document or a projection
/// of one.
#[derive(Clone, Debug)]
pub struct TrustedContext {
    manifest: Json,
    policy: Json,
    classification_contract: Json,
    authorizing_evidence_contract: Json,
    authorization_subject_profile: Json,
    input_schema: Json,
    output_schema: Json,
    cli_error_schema: Json,
    classification_schema: Json,
    authority: Json,
    kind_ids: Vec<String>,
    defaults: Vec<(String, String)>,
}

/// Whether the sidecar bytes are exactly `"<digest>  <filename>"`.
fn sidecar_matches(bytes: &[u8], expected_digest: &str, filename: &str) -> bool {
    let text = std::str::from_utf8(bytes).unwrap_or_default().trim();
    text == format!("{expected_digest}  {filename}")
}

/// Whether `actual` is an object with exactly `expected`'s key set and
/// identical member values (the reference `sameObject`).
pub(crate) fn same_object(actual: &Json, expected: &Json) -> bool {
    match (actual.as_object(), expected.as_object()) {
        (Some(actual), Some(expected)) => {
            actual.len() == expected.len()
                && expected
                    .iter()
                    .all(|(key, value)| actual.get(key) == Some(value))
        }
        _ => false,
    }
}

/// The frozen reference values as wire JSON (for exact comparisons).
fn policy_ref_json() -> Json {
    serde_json::json!({
        "policyId": refs::POLICY_ID,
        "version": refs::POLICY_VERSION,
        "digest": refs::POLICY_DIGEST,
    })
}

fn authority_ref_json() -> Json {
    serde_json::json!({
        "contractId": refs::AUTHORITY_CONTRACT_ID,
        "version": refs::AUTHORITY_VERSION,
        "digest": format!("sha256:{}", refs::AUTHORITY_RAW_SHA256),
    })
}

fn classification_ref_json() -> Json {
    serde_json::json!({
        "contractId": refs::CLASSIFICATION_CONTRACT_ID,
        "version": refs::DECISION_FAMILY_VERSION,
        "digest": format!("sha256:{}", refs::CLASSIFICATION_CONTRACT_RAW_SHA256),
    })
}

fn evidence_ref_json() -> Json {
    serde_json::json!({
        "contractId": refs::AUTHORIZING_EVIDENCE_CONTRACT_ID,
        "version": refs::DECISION_FAMILY_VERSION,
        "digest": format!("sha256:{}", refs::AUTHORIZING_EVIDENCE_RAW_SHA256),
    })
}

fn profile_ref_json() -> Json {
    serde_json::json!({
        "profileId": refs::AUTHORIZATION_SUBJECT_PROFILE_ID,
        "version": refs::DECISION_FAMILY_VERSION,
        "digest": format!("sha256:{}", refs::AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256),
    })
}

fn decision_ref_json() -> Json {
    serde_json::json!({
        "contractId": refs::DECISION_CONTRACT_ID,
        "version": refs::DECISION_FAMILY_VERSION,
    })
}

/// The canonical policy identity: the SHA-256 of the canonical JSON
/// projection of the complete policy with only `policyRef.digest`
/// omitted.
fn policy_identity(policy: &Json) -> String {
    let mut projected = policy.clone();
    if let Some(policy_ref) = projected.get_mut("policyRef").and_then(Json::as_object_mut) {
        policy_ref.remove("digest");
    }
    format!("sha256:{}", sha256_hex(canonical(&projected).as_bytes()))
}

/// Parse one embedded document, or the exact custody parse failure.
fn parse_json(bytes: &[u8], name: &str) -> Result<Json, CustodyError> {
    serde_json::from_slice(bytes).map_err(|error| CustodyError {
        code: format!("custody.{name}.json: {error}"),
    })
}

/// A string member of an object.
fn str_member<'a>(value: &'a Json, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Json::as_str)
}

/// The `sha256:`-prefixed form of a pinned raw bytes digest (the
/// spelling every contract reference carries).
fn digest_ref(raw: &str) -> String {
    format!("sha256:{raw}")
}

impl TrustedContext {
    /// The embedded, once-verified trusted context of the whole
    /// process. The custody chain runs exactly once; any failure is
    /// terminal for every consumer.
    pub fn embedded() -> Result<&'static Self, CustodyError> {
        static CONTEXT: LazyLock<Result<TrustedContext, CustodyError>> =
            LazyLock::new(TrustedContext::verify_and_load);
        CONTEXT.as_ref().map_err(Clone::clone)
    }

    /// Verify the complete custody chain over the embedded bytes and
    /// load the immutable context.
    pub fn verify_and_load() -> Result<Self, CustodyError> {
        let require = |condition: bool, code: &str| {
            if condition {
                Ok(())
            } else {
                Err(CustodyError {
                    code: code.to_owned(),
                })
            }
        };

        require(
            sha256_hex(MANIFEST_BYTES) == refs::TRUSTED_MANIFEST_SHA256,
            "custody.manifest-untrusted",
        )?;
        require(
            sidecar_matches(
                MANIFEST_SIDECAR_BYTES,
                refs::TRUSTED_MANIFEST_SHA256,
                "privacy-policy.v0.3.2.manifest.json",
            ),
            "custody.manifest-sidecar-mismatch",
        )?;
        require(
            sha256_hex(POLICY_BYTES) == refs::POLICY_RAW_SHA256,
            "custody.policy-bytes-mismatch",
        )?;
        require(
            sidecar_matches(
                POLICY_SIDECAR_BYTES,
                refs::POLICY_RAW_SHA256,
                "privacy-policy.v0.3.2.json",
            ),
            "custody.policy-sidecar-mismatch",
        )?;
        require(
            sha256_hex(CLASSIFICATION_BYTES) == refs::CLASSIFICATION_CONTRACT_RAW_SHA256,
            "custody.classification-contract-bytes-mismatch",
        )?;
        require(
            sidecar_matches(
                CLASSIFICATION_SIDECAR_BYTES,
                refs::CLASSIFICATION_CONTRACT_RAW_SHA256,
                "privacy-policy.v0.2.16.classification.json",
            ),
            "custody.classification-sidecar-mismatch",
        )?;
        require(
            sha256_hex(AUTHORIZING_EVIDENCE_BYTES) == refs::AUTHORIZING_EVIDENCE_RAW_SHA256,
            "custody.authorizing-evidence-bytes-mismatch",
        )?;
        require(
            sidecar_matches(
                AUTHORIZING_EVIDENCE_SIDECAR_BYTES,
                refs::AUTHORIZING_EVIDENCE_RAW_SHA256,
                "privacy-authorizing-evidence.v0.2.16.json",
            ),
            "custody.authorizing-evidence-sidecar-mismatch",
        )?;
        require(
            sha256_hex(SUBJECT_PROFILE_BYTES) == refs::AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256,
            "custody.subject-profile-bytes-mismatch",
        )?;
        require(
            sidecar_matches(
                SUBJECT_PROFILE_SIDECAR_BYTES,
                refs::AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256,
                "privacy-authorization-subject-profile.v0.2.16.json",
            ),
            "custody.subject-profile-sidecar-mismatch",
        )?;
        require(
            sha256_hex(INPUT_SCHEMA_BYTES) == refs::INPUT_SCHEMA_RAW_SHA256,
            "custody.input-schema-bytes-mismatch",
        )?;
        require(
            sha256_hex(OUTPUT_SCHEMA_BYTES) == refs::OUTPUT_SCHEMA_RAW_SHA256,
            "custody.output-schema-bytes-mismatch",
        )?;
        require(
            sha256_hex(CLI_ERROR_SCHEMA_BYTES) == refs::CLI_ERROR_SCHEMA_RAW_SHA256,
            "custody.cli-error-schema-bytes-mismatch",
        )?;
        require(
            sha256_hex(CLASSIFICATION_SCHEMA_BYTES) == refs::CLASSIFICATION_SCHEMA_RAW_SHA256,
            "custody.classification-schema-bytes-mismatch",
        )?;
        require(
            sha256_hex(AUTHORITY_BYTES) == refs::AUTHORITY_RAW_SHA256,
            "custody.authority-bytes-mismatch",
        )?;

        let manifest = parse_json(MANIFEST_BYTES, "manifest")?;
        let policy = parse_json(POLICY_BYTES, "policy")?;
        let classification_contract = parse_json(CLASSIFICATION_BYTES, "classification-contract")?;
        let authorizing_evidence_contract =
            parse_json(AUTHORIZING_EVIDENCE_BYTES, "authorizing-evidence-contract")?;
        let authorization_subject_profile =
            parse_json(SUBJECT_PROFILE_BYTES, "authorization-subject-profile")?;
        let input_schema = parse_json(INPUT_SCHEMA_BYTES, "input-schema")?;
        let output_schema = parse_json(OUTPUT_SCHEMA_BYTES, "output-schema")?;
        let cli_error_schema = parse_json(CLI_ERROR_SCHEMA_BYTES, "cli-error-schema")?;
        let classification_schema =
            parse_json(CLASSIFICATION_SCHEMA_BYTES, "classification-schema")?;
        let authority = parse_json(AUTHORITY_BYTES, "authority")?;

        require(
            same_object(
                policy.get("policyRef").unwrap_or(&Json::Null),
                &policy_ref_json(),
            ),
            "custody.policy-ref-mismatch",
        )?;
        require(
            policy_identity(&policy) == refs::POLICY_DIGEST,
            "custody.policy-identity-mismatch",
        )?;
        require(
            str_member(policy.get("lifecycle").unwrap_or(&Json::Null), "status")
                == Some("accepted")
                && policy
                    .get("lifecycle")
                    .and_then(|lifecycle| lifecycle.get("accepted"))
                    == Some(&Json::Bool(true)),
            "custody.policy-not-accepted",
        )?;
        require(
            same_object(
                policy.get("authorityRef").unwrap_or(&Json::Null),
                &authority_ref_json(),
            ),
            "custody.policy-authority-ref-mismatch",
        )?;
        require(
            same_object(
                policy
                    .get("classificationContractRef")
                    .unwrap_or(&Json::Null),
                &classification_ref_json(),
            ),
            "custody.policy-classification-ref-mismatch",
        )?;
        require(
            same_object(
                policy
                    .get("authorizingEvidenceContractRef")
                    .unwrap_or(&Json::Null),
                &evidence_ref_json(),
            ),
            "custody.policy-authorizing-evidence-ref-mismatch",
        )?;
        require(
            same_object(
                policy
                    .get("authorizationSubjectProfileRef")
                    .unwrap_or(&Json::Null),
                &profile_ref_json(),
            ),
            "custody.policy-subject-profile-ref-mismatch",
        )?;
        require(
            classification_contract
                .get("contractId")
                .and_then(Json::as_str)
                == Some(refs::CLASSIFICATION_CONTRACT_ID)
                && classification_contract
                    .get("version")
                    .and_then(Json::as_str)
                    == Some(refs::DECISION_FAMILY_VERSION)
                && classification_contract.get("status").and_then(Json::as_str) == Some("accepted"),
            "custody.classification-contract-ref-mismatch",
        )?;
        let evidence_registry = authorizing_evidence_contract
            .get("registry")
            .and_then(Json::as_array);
        require(
            authorizing_evidence_contract
                .get("contractId")
                .and_then(Json::as_str)
                == Some(refs::AUTHORIZING_EVIDENCE_CONTRACT_ID)
                && authorizing_evidence_contract
                    .get("version")
                    .and_then(Json::as_str)
                    == Some(refs::DECISION_FAMILY_VERSION)
                && authorizing_evidence_contract
                    .get("status")
                    .and_then(Json::as_str)
                    == Some("accepted")
                && authorizing_evidence_contract.get("accepted") == Some(&Json::Bool(true))
                && evidence_registry.is_some_and(|registry| registry.len() == 9),
            "custody.authorizing-evidence-contract-ref-mismatch",
        )?;
        require(
            authorization_subject_profile
                .get("profileId")
                .and_then(Json::as_str)
                == Some(refs::AUTHORIZATION_SUBJECT_PROFILE_ID)
                && authorization_subject_profile
                    .get("version")
                    .and_then(Json::as_str)
                    == Some(refs::DECISION_FAMILY_VERSION)
                && authorization_subject_profile
                    .get("status")
                    .and_then(Json::as_str)
                    == Some("accepted")
                && authorization_subject_profile.get("accepted") == Some(&Json::Bool(true)),
            "custody.subject-profile-ref-mismatch",
        )?;
        require(
            authority.get("contractId").and_then(Json::as_str) == Some(refs::AUTHORITY_CONTRACT_ID)
                && authority.get("version").and_then(Json::as_str) == Some(refs::AUTHORITY_VERSION),
            "custody.authority-ref-mismatch",
        )?;

        let accepted = manifest
            .get("acceptedContracts")
            .and_then(Json::as_array)
            .and_then(|entries| entries.first())
            .cloned()
            .unwrap_or(Json::Null);
        let accepted_refs = accepted.clone();
        require(
            manifest.get("formatVersion").and_then(Json::as_str) == Some(refs::POLICY_VERSION)
                && manifest.get("status").and_then(Json::as_str) == Some("accepted")
                && manifest.get("accepted") == Some(&Json::Bool(true))
                && manifest
                    .get("acceptedContracts")
                    .and_then(Json::as_array)
                    .is_some_and(|entries| entries.len() == 1)
                && accepted_refs.get("status").and_then(Json::as_str) == Some("accepted")
                && accepted_refs.get("accepted") == Some(&Json::Bool(true))
                && same_object(
                    accepted_refs.get("policyRef").unwrap_or(&Json::Null),
                    &policy_ref_json(),
                )
                && accepted_refs.get("policyFileDigest").and_then(Json::as_str)
                    == Some(digest_ref(refs::POLICY_RAW_SHA256).as_str())
                && same_object(
                    manifest.get("currentAcceptedRef").unwrap_or(&Json::Null),
                    &policy_ref_json(),
                ),
            "custody.manifest-policy-mismatch",
        )?;
        require(
            same_object(
                manifest.get("authorityRef").unwrap_or(&Json::Null),
                &authority_ref_json(),
            ),
            "custody.manifest-authority-mismatch",
        )?;
        let classification_digest = digest_ref(refs::CLASSIFICATION_CONTRACT_RAW_SHA256);
        let evidence_digest = digest_ref(refs::AUTHORIZING_EVIDENCE_RAW_SHA256);
        let profile_digest = digest_ref(refs::AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256);
        let input_digest = digest_ref(refs::INPUT_SCHEMA_RAW_SHA256);
        let output_digest = digest_ref(refs::OUTPUT_SCHEMA_RAW_SHA256);
        let cli_error_digest = digest_ref(refs::CLI_ERROR_SCHEMA_RAW_SHA256);
        let classification_schema_digest = digest_ref(refs::CLASSIFICATION_SCHEMA_RAW_SHA256);
        let nested = |key: &str, member: &str| -> Option<&str> {
            accepted_refs
                .get(key)
                .and_then(|reference| reference.get(member))
                .and_then(Json::as_str)
        };
        require(
            nested("classificationContractRef", "digest") == Some(classification_digest.as_str())
                && nested("authorizingEvidenceContractRef", "digest")
                    == Some(evidence_digest.as_str())
                && nested("authorizingEvidenceContractRef", "contractId")
                    == Some(refs::AUTHORIZING_EVIDENCE_CONTRACT_ID)
                && nested("authorizingEvidenceContractRef", "version")
                    == Some(refs::DECISION_FAMILY_VERSION)
                && nested("authorizationSubjectProfileRef", "digest")
                    == Some(profile_digest.as_str())
                && nested("authorizationSubjectProfileRef", "profileId")
                    == Some(refs::AUTHORIZATION_SUBJECT_PROFILE_ID)
                && nested("authorizationSubjectProfileRef", "version")
                    == Some(refs::DECISION_FAMILY_VERSION)
                && nested("inputSchemaRef", "digest") == Some(input_digest.as_str())
                && nested("outputSchemaRef", "digest") == Some(output_digest.as_str())
                && nested("cliErrorSchemaRef", "digest") == Some(cli_error_digest.as_str())
                && nested("classificationSchemaRef", "digest")
                    == Some(classification_schema_digest.as_str())
                && nested("inputSchemaRef", "schemaId") == Some(refs::INPUT_SCHEMA_ID)
                && nested("inputSchemaRef", "version") == Some(refs::POLICY_VERSION)
                && nested("outputSchemaRef", "schemaId") == Some(refs::OUTPUT_SCHEMA_ID)
                && nested("outputSchemaRef", "version") == Some(refs::POLICY_VERSION)
                && nested("cliErrorSchemaRef", "schemaId") == Some(refs::CLI_ERROR_SCHEMA_ID)
                && nested("cliErrorSchemaRef", "version") == Some(refs::DECISION_FAMILY_VERSION)
                && nested("classificationSchemaRef", "schemaId")
                    == Some(refs::CLASSIFICATION_SCHEMA_ID)
                && nested("classificationSchemaRef", "version")
                    == Some(refs::DECISION_FAMILY_VERSION)
                && same_object(
                    accepted_refs
                        .get("decisionContractRef")
                        .unwrap_or(&Json::Null),
                    &decision_ref_json(),
                ),
            "custody.manifest-schema-mismatch",
        )?;
        require(
            input_schema.get("schemaId").and_then(Json::as_str) == Some(refs::INPUT_SCHEMA_ID)
                && input_schema.get("version").and_then(Json::as_str) == Some(refs::POLICY_VERSION),
            "custody.input-schema-ref-mismatch",
        )?;
        require(
            output_schema.get("schemaId").and_then(Json::as_str) == Some(refs::OUTPUT_SCHEMA_ID)
                && output_schema.get("version").and_then(Json::as_str)
                    == Some(refs::POLICY_VERSION),
            "custody.output-schema-ref-mismatch",
        )?;
        require(
            cli_error_schema.get("schemaId").and_then(Json::as_str)
                == Some(refs::CLI_ERROR_SCHEMA_ID)
                && cli_error_schema.get("version").and_then(Json::as_str)
                    == Some(refs::DECISION_FAMILY_VERSION),
            "custody.cli-error-schema-ref-mismatch",
        )?;
        let effective_refs = output_schema
            .pointer("/properties/effectiveRefs/properties")
            .cloned()
            .unwrap_or(Json::Null);
        require(
            effective_refs
                .pointer("/classificationContractRef/properties/version/const")
                .and_then(Json::as_str)
                == Some(refs::DECISION_FAMILY_VERSION)
                && effective_refs
                    .pointer("/classificationContractRef/properties/digest/const")
                    .and_then(Json::as_str)
                    == Some(classification_digest.as_str())
                && effective_refs
                    .pointer("/authorizingEvidenceContractRef/properties/version/const")
                    .and_then(Json::as_str)
                    == Some(refs::DECISION_FAMILY_VERSION)
                && effective_refs
                    .pointer("/authorizingEvidenceContractRef/properties/digest/const")
                    .and_then(Json::as_str)
                    == Some(evidence_digest.as_str()),
            "custody.output-schema-effective-ref-mismatch",
        )?;
        require(
            classification_schema.get("schemaId").and_then(Json::as_str)
                == Some(refs::CLASSIFICATION_SCHEMA_ID)
                && classification_schema.get("version").and_then(Json::as_str)
                    == Some(refs::DECISION_FAMILY_VERSION),
            "custody.classification-schema-ref-mismatch",
        )?;
        require(
            input_schema.pointer("/lifecycle/accepted") == Some(&Json::Bool(true))
                && output_schema.pointer("/lifecycle/accepted") == Some(&Json::Bool(true))
                && cli_error_schema.pointer("/lifecycle/accepted") == Some(&Json::Bool(true))
                && cli_error_schema.pointer("/lifecycle/exportDecisionOutput")
                    == Some(&Json::Bool(false))
                && classification_schema.pointer("/lifecycle/accepted") == Some(&Json::Bool(true)),
            "custody.schema-not-accepted",
        )?;
        require(
            same_object(
                authorizing_evidence_contract
                    .get("authorizationSubjectProfileRef")
                    .unwrap_or(&Json::Null),
                &profile_ref_json(),
            ),
            "custody.evidence-subject-profile-ref-mismatch",
        )?;

        let mut schema_root_properties: Vec<&str> = input_schema
            .get("properties")
            .and_then(Json::as_object)
            .map(|properties| properties.keys().map(String::as_str).collect())
            .unwrap_or_default();
        schema_root_properties.sort_by(|left, right| compare_unicode_code_points(left, right));
        let mut profile_root_properties: Vec<&str> = authorization_subject_profile
            .get("inputSchemaRootProperties")
            .and_then(Json::as_array)
            .map(|properties| properties.iter().filter_map(Json::as_str).collect())
            .unwrap_or_default();
        profile_root_properties.sort_by(|left, right| compare_unicode_code_points(left, right));
        let root_inventory_canonical =
            canonical(&serde_json::to_value(&schema_root_properties).unwrap_or_default())
                == canonical(&serde_json::to_value(&profile_root_properties).unwrap_or_default());
        let unique = |values: &[&str]| {
            values
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<&str>>()
                .len()
                == values.len()
        };
        require(
            root_inventory_canonical
                && unique(&schema_root_properties)
                && unique(&profile_root_properties),
            "custody.subject-profile-root-inventory-mismatch",
        )?;
        let semantic_inventory = authorization_subject_profile
            .get("decisionSemanticInventory")
            .and_then(Json::as_array);
        let inventory_unique = semantic_inventory.is_some_and(|inventory| {
            let ids: Vec<&Json> = inventory
                .iter()
                .filter_map(|entry| entry.get("id"))
                .collect();
            !inventory.is_empty()
                && ids.len() == inventory.len()
                && (1..ids.len()).all(|index| ids[..index].iter().all(|prior| prior != &ids[index]))
        });
        require(
            inventory_unique,
            "custody.subject-profile-semantic-inventory-mismatch",
        )?;

        let authority_kinds: Option<Vec<&str>> = authority
            .get("artifactKinds")
            .and_then(Json::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("id"))
                    .filter_map(Json::as_str)
                    .collect()
            });
        let authority_kinds = match authority_kinds {
            Some(kinds) if kinds.len() == AUTHORITY_KIND_COUNT => kinds,
            _ => {
                return Err(CustodyError {
                    code: "custody.authority-registry-not-closed-exact".to_owned(),
                })
            }
        };
        let mut unique_kinds = authority_kinds.clone();
        unique_kinds.sort_unstable();
        unique_kinds.dedup();
        require(
            unique_kinds.len() == AUTHORITY_KIND_COUNT,
            "custody.authority-registry-not-closed-exact",
        )?;

        let dispositions = policy
            .pointer("/vocabularies/exportDisposition")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        let defaults = policy.get("artifactDefaults").and_then(Json::as_array);
        let defaults: Vec<(String, String)> = match defaults {
            Some(entries) if entries.len() == AUTHORITY_KIND_COUNT => {
                let mut parsed = Vec::with_capacity(entries.len());
                for (index, entry) in entries.iter().enumerate() {
                    let kind = entry.get("artifactKind").and_then(Json::as_str);
                    let disposition = entry.get("exportDisposition").and_then(Json::as_str);
                    let exact_members = entry.as_object().is_some_and(|object| object.len() == 2)
                        && kind.is_some()
                        && disposition.is_some();
                    if !exact_members
                        || kind != Some(authority_kinds[index])
                        || !dispositions
                            .contains(&Json::String(disposition.unwrap_or_default().to_owned()))
                    {
                        return Err(CustodyError {
                            code: "custody.policy-default-registry-mismatch".to_owned(),
                        });
                    }
                    parsed.push((
                        kind.unwrap_or_default().to_owned(),
                        disposition.unwrap_or_default().to_owned(),
                    ));
                }
                parsed
            }
            _ => {
                return Err(CustodyError {
                    code: "custody.policy-default-count".to_owned(),
                })
            }
        };
        require(
            {
                let mut kinds: Vec<&str> = defaults.iter().map(|(kind, _)| kind.as_str()).collect();
                kinds.sort_unstable();
                kinds.dedup();
                kinds.len() == AUTHORITY_KIND_COUNT
            },
            "custody.policy-default-duplicate",
        )?;

        let constraint_policy = policy
            .get("constraintPolicy")
            .cloned()
            .unwrap_or(Json::Null);
        require(
            constraint_policy.get("localVocabularyOverridesAllowed") == Some(&Json::Bool(false))
                && constraint_policy
                    .get("reviewedBroadeningGrants")
                    .and_then(Json::as_array)
                    .is_some_and(std::vec::Vec::is_empty)
                && canonical(
                    constraint_policy
                        .get("effectiveBaseline")
                        .unwrap_or(&Json::Null),
                ) == canonical(&serde_json::json!({
                    "allowedOperations": ["disposition-rule", "all-sensitivity-rules"],
                    "allowedTrustBoundaries": ["operation-profile", "all-sensitivity-rules"],
                    "allowedAudiences": ["destination-profile", "all-sensitivity-rules"],
                })),
            "custody.local-override-or-grant-present",
        )?;
        let repository_identity_policy = policy
            .get("repositoryIdentityPolicy")
            .cloned()
            .unwrap_or(Json::Null);
        require(
            repository_identity_policy.get("declaredTokenCoherenceOnly") == Some(&Json::Bool(true))
                && repository_identity_policy.get("callerFabricatedEqualityProvesPhysicalIdentity")
                    == Some(&Json::Bool(false))
                && repository_identity_policy.get("bindingAttestationInDecisionInput")
                    == Some(&Json::Bool(false))
                && canonical(
                    repository_identity_policy
                        .get("trustedBinding")
                        .unwrap_or(&Json::Null),
                ) == canonical(&serde_json::json!({
                    "ownedByIssues": [119, 89],
                    "actors": ["runtime-envelope", "integration-adapter"],
                    "source": "physically-resolved-repository-context",
                    "requirements": ["mint", "bind", "verify-freshness"],
                    "missingStaleOrUnverified": "fail-closed-before-evaluator-invocation-or-decision-acceptance",
                    "physicalChecks": ["root-containment", "symlink", "junction", "reparse-point", "real-8.3-alias", "toctou"],
                })),
            "custody.repository-binding-seam-mismatch",
        )?;
        let evidence_policy = policy
            .get("authorizingEvidencePolicy")
            .cloned()
            .unwrap_or(Json::Null);
        require(
            evidence_policy.get("genericAuditRefAuthorizingUse").and_then(Json::as_str) == Some("forbidden")
                && evidence_policy.get("broadeningGrantInput").and_then(Json::as_str) == Some("null-only")
                && evidence_policy
                    .pointer("/trustedVerificationBoundary/callerDeclarationProvesAuthenticity")
                    == Some(&Json::Bool(false))
                && evidence_policy
                    .pointer("/trustedVerificationBoundary/physicalOrCryptographicAuthenticityCheckedByPolicy120")
                    == Some(&Json::Bool(false)),
            "custody.authorizing-evidence-policy-mismatch",
        )?;

        let kind_ids: Vec<String> = authority_kinds
            .iter()
            .map(|kind| (*kind).to_owned())
            .collect();
        Ok(Self {
            manifest,
            policy,
            classification_contract,
            authorizing_evidence_contract,
            authorization_subject_profile,
            input_schema,
            output_schema,
            cli_error_schema,
            classification_schema,
            authority,
            kind_ids,
            defaults,
        })
    }

    /// The accepted manifest document.
    pub const fn manifest(&self) -> &Json {
        &self.manifest
    }

    /// The accepted policy document.
    pub const fn policy(&self) -> &Json {
        &self.policy
    }

    /// The classification decision contract.
    pub const fn classification_contract(&self) -> &Json {
        &self.classification_contract
    }

    /// The authorizing-evidence contract.
    pub const fn authorizing_evidence_contract(&self) -> &Json {
        &self.authorizing_evidence_contract
    }

    /// The authorization-subject profile.
    pub const fn authorization_subject_profile(&self) -> &Json {
        &self.authorization_subject_profile
    }

    /// The strict input schema.
    pub const fn input_schema(&self) -> &Json {
        &self.input_schema
    }

    /// The strict output schema.
    pub const fn output_schema(&self) -> &Json {
        &self.output_schema
    }

    /// The CLI startup-error schema.
    pub const fn cli_error_schema(&self) -> &Json {
        &self.cli_error_schema
    }

    /// The classification-ref schema.
    pub const fn classification_schema(&self) -> &Json {
        &self.classification_schema
    }

    /// The authority matrix.
    pub const fn authority(&self) -> &Json {
        &self.authority
    }

    /// The closed 57-kind registry, in registry order.
    pub fn kind_ids(&self) -> &[String] {
        &self.kind_ids
    }

    /// The registered default disposition of one artifact kind, or
    /// `None` for an unknown kind (which denies).
    pub fn default_disposition(&self, artifact_kind: &str) -> Option<&str> {
        self.defaults
            .iter()
            .find(|(kind, _)| kind == artifact_kind)
            .map(|(_, disposition)| disposition.as_str())
    }

    /// One closed policy vocabulary by its wire name (the exact JSON
    /// string array; membership checks never allocate).
    pub fn vocabulary(&self, name: &str) -> Option<&Vec<Json>> {
        self.policy
            .get("vocabularies")
            .and_then(|vocabularies| vocabularies.get(name))
            .and_then(Json::as_array)
    }

    /// The policy operation profiles.
    pub fn operation_profiles(&self) -> Option<&Vec<Json>> {
        self.policy
            .get("operationProfiles")
            .and_then(Json::as_array)
    }

    /// The policy destination profiles.
    pub fn destination_profiles(&self) -> Option<&Vec<Json>> {
        self.policy
            .get("destinationProfiles")
            .and_then(Json::as_array)
    }

    /// The policy disposition rules.
    pub fn disposition_rules(&self) -> Option<&Vec<Json>> {
        self.policy.get("dispositionRules").and_then(Json::as_array)
    }

    /// The policy sensitivity rules.
    pub fn sensitivity_rules(&self) -> Option<&Vec<Json>> {
        self.policy.get("sensitivityRules").and_then(Json::as_array)
    }

    /// One closed evidence-contract vocabulary by its wire name.
    pub fn evidence_vocabulary(&self, name: &str) -> Option<&Vec<Json>> {
        self.authorizing_evidence_contract
            .get("vocabularies")
            .and_then(|vocabularies| vocabularies.get(name))
            .and_then(Json::as_array)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The embedded custody chain verifies: the whole load is the
    /// proof (every pinned digest, sidecar, manifest acceptance,
    /// registry closure, default table, and policy seam check runs).
    #[test]
    fn embedded_context_verifies_and_loads() {
        let context = TrustedContext::embedded().expect("embedded custody must verify");
        assert_eq!(context.kind_ids().len(), 57);
        assert_eq!(
            context.default_disposition("fixture"),
            Some("public-fixture")
        );
        assert_eq!(
            context.default_disposition("ai.prompt"),
            Some("forbidden-to-export")
        );
        assert_eq!(
            context.default_disposition("export.decision"),
            Some("local-private")
        );
        assert_eq!(context.default_disposition("nonexistent.kind"), None);
    }

    /// The trusted policy vocabularies equal the typed enum
    /// spellings: the typed surface and the pinned bytes cannot drift.
    #[test]
    fn policy_vocabularies_equal_the_typed_enums() {
        let context = TrustedContext::embedded().unwrap();
        let spellings = |members: &[&'static str]| {
            members
                .iter()
                .map(|member| Json::String((*member).to_owned()))
                .collect::<Vec<Json>>()
        };
        let data_sensitivity: Vec<&'static str> = crate::privacy::vocab::DataSensitivity::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("dataSensitivity"),
            Some(&spellings(&data_sensitivity))
        );
        let export_disposition: Vec<&'static str> = crate::privacy::vocab::ExportDisposition::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("exportDisposition"),
            Some(&spellings(&export_disposition))
        );
        let operation: Vec<&'static str> = crate::privacy::vocab::OperationId::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("operation"),
            Some(&spellings(&operation))
        );
        let repository_role: Vec<&'static str> = crate::privacy::vocab::RepositoryRole::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("repositoryRole"),
            Some(&spellings(&repository_role))
        );
        let trust_boundary: Vec<&'static str> = crate::privacy::vocab::TrustBoundary::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("trustBoundary"),
            Some(&spellings(&trust_boundary))
        );
        let repository_relation: Vec<&'static str> = crate::privacy::vocab::RepositoryRelation::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("repositoryRelation"),
            Some(&spellings(&repository_relation))
        );
        let tenant_relation: Vec<&'static str> = crate::privacy::vocab::TenantRelation::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("tenantRelation"),
            Some(&spellings(&tenant_relation))
        );
        let audience: Vec<&'static str> = crate::privacy::vocab::Audience::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(context.vocabulary("audience"), Some(&spellings(&audience)));
        let value_state: Vec<&'static str> = crate::privacy::vocab::SensitivityState::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("valueState"),
            Some(&spellings(&value_state))
        );
        let conflict_state: Vec<&'static str> = crate::privacy::vocab::ConflictState::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("conflictState"),
            Some(&spellings(&conflict_state))
        );
        let constraint_scope: Vec<&'static str> = crate::privacy::vocab::ConstraintScope::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("constraintScope"),
            Some(&spellings(&constraint_scope))
        );
        let transform: Vec<&'static str> = crate::privacy::vocab::TransformId::ALL
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(
            context.vocabulary("transform"),
            Some(&spellings(&transform))
        );
    }

    /// The rule tables are present and non-empty, and every evidence
    /// vocabulary is closed.
    #[test]
    fn rule_tables_and_evidence_vocabularies_are_present() {
        let context = TrustedContext::embedded().unwrap();
        assert_eq!(context.operation_profiles().map(Vec::len), Some(5));
        assert_eq!(context.destination_profiles().map(Vec::len), Some(6));
        assert_eq!(context.disposition_rules().map(Vec::len), Some(6));
        assert_eq!(context.sensitivity_rules().map(Vec::len), Some(9));
        let verification = context
            .evidence_vocabulary("verificationState")
            .expect("verificationState vocabulary");
        assert_eq!(verification.len(), 2);
        let freshness = context
            .evidence_vocabulary("freshnessState")
            .expect("freshnessState vocabulary");
        assert_eq!(freshness.len(), 3);
    }
}
