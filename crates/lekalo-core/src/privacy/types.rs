//! The typed wire shapes shared by the decision input and output
//! (issue #119): frozen contract references, authorizing evidence,
//! and the state-only path/value representations.
//!
//! Every struct is a closed member set with the exact wire spellings
//! of the pinned #120 schemas. Construction is deny-unknown by type:
//! there is no way to add a member, and vocabulary members are closed
//! enums. Only [`ExportDecisionInput`](super::input::ExportDecisionInput)
//! parsing (the exact-reason-code validator) accepts wire bytes; these
//! types are the representation it constructs and the one the runtime
//! serializes.

use serde::Serialize;
use serde_json::Value as Json;

use super::refs;
use super::vocab::SensitivityState;

/// The wire shape `{policyId, version, digest}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRef {
    policy_id: String,
    version: String,
    digest: String,
}

impl PolicyRef {
    /// Assemble from validated members.
    pub fn new(policy_id: String, version: String, digest: String) -> Self {
        Self {
            policy_id,
            version,
            digest,
        }
    }

    /// The exact frozen policy reference.
    pub fn frozen() -> Self {
        Self {
            policy_id: refs::POLICY_ID.to_owned(),
            version: refs::POLICY_VERSION.to_owned(),
            digest: refs::POLICY_DIGEST.to_owned(),
        }
    }

    /// The policy identity.
    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }

    /// The policy version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The pinned semantic digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// The wire shape `{contractId, version, digest}` of a satellite
/// contract reference (authority, classification, evidence).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractRef {
    contract_id: String,
    version: String,
    digest: String,
}

impl ContractRef {
    /// Assemble from validated members.
    pub fn new(contract_id: String, version: String, digest: String) -> Self {
        Self {
            contract_id,
            version,
            digest,
        }
    }

    /// The exact frozen authority reference.
    pub fn frozen_authority() -> Self {
        Self::new(
            refs::AUTHORITY_CONTRACT_ID.to_owned(),
            refs::AUTHORITY_VERSION.to_owned(),
            format!("sha256:{}", refs::AUTHORITY_RAW_SHA256),
        )
    }

    /// The exact frozen classification-decision contract reference.
    pub fn frozen_classification() -> Self {
        Self::new(
            refs::CLASSIFICATION_CONTRACT_ID.to_owned(),
            refs::DECISION_FAMILY_VERSION.to_owned(),
            format!("sha256:{}", refs::CLASSIFICATION_CONTRACT_RAW_SHA256),
        )
    }

    /// The exact frozen authorizing-evidence contract reference.
    pub fn frozen_evidence() -> Self {
        Self::new(
            refs::AUTHORIZING_EVIDENCE_CONTRACT_ID.to_owned(),
            refs::DECISION_FAMILY_VERSION.to_owned(),
            format!("sha256:{}", refs::AUTHORIZING_EVIDENCE_RAW_SHA256),
        )
    }

    /// The contract identity.
    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }

    /// The contract version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The pinned bytes digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// The authority wire alias (`{contractId, version, digest}`).
pub type AuthorityRef = ContractRef;
/// The classification-contract wire alias (`{contractId, version, digest}`).
pub type ClassificationContractRef = ContractRef;
/// The authorizing-evidence contract wire alias (`{contractId, version, digest}`).
pub type EvidenceContractRef = ContractRef;

/// The wire shape `{profileId, version, digest}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectProfileRef {
    profile_id: String,
    version: String,
    digest: String,
}

impl SubjectProfileRef {
    /// Assemble from validated members.
    pub fn new(profile_id: String, version: String, digest: String) -> Self {
        Self {
            profile_id,
            version,
            digest,
        }
    }

    /// The exact frozen subject-profile reference.
    pub fn frozen() -> Self {
        Self {
            profile_id: refs::AUTHORIZATION_SUBJECT_PROFILE_ID.to_owned(),
            version: refs::DECISION_FAMILY_VERSION.to_owned(),
            digest: format!("sha256:{}", refs::AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256),
        }
    }

    /// The profile identity.
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    /// The profile version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The pinned bytes digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// The wire shape `{contractId, version}` of the decision contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionContractRef {
    contract_id: String,
    version: String,
}

impl DecisionContractRef {
    /// Assemble from validated members.
    pub fn new(contract_id: String, version: String) -> Self {
        Self {
            contract_id,
            version,
        }
    }

    /// The exact frozen decision contract reference.
    pub fn frozen() -> Self {
        Self {
            contract_id: refs::DECISION_CONTRACT_ID.to_owned(),
            version: refs::DECISION_FAMILY_VERSION.to_owned(),
        }
    }

    /// The contract identity.
    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }

    /// The contract version.
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// The wire shape `{schemaId, version}` of a schema reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaRef {
    schema_id: String,
    version: String,
}

impl SchemaRef {
    /// The exact frozen input schema reference.
    pub fn frozen_input() -> Self {
        Self::schema(refs::INPUT_SCHEMA_ID, refs::POLICY_VERSION)
    }

    /// The exact frozen output schema reference.
    pub fn frozen_output() -> Self {
        Self::schema(refs::OUTPUT_SCHEMA_ID, refs::POLICY_VERSION)
    }

    /// The exact frozen CLI startup-error schema reference.
    pub fn frozen_cli_error() -> Self {
        Self::schema(refs::CLI_ERROR_SCHEMA_ID, refs::DECISION_FAMILY_VERSION)
    }

    /// The exact frozen classification-ref schema reference.
    pub fn frozen_classification_schema() -> Self {
        Self::schema(
            refs::CLASSIFICATION_SCHEMA_ID,
            refs::DECISION_FAMILY_VERSION,
        )
    }

    fn schema(schema_id: &str, version: &str) -> Self {
        Self {
            schema_id: schema_id.to_owned(),
            version: version.to_owned(),
        }
    }

    /// The schema identity.
    pub fn schema_id(&self) -> &str {
        &self.schema_id
    }

    /// The schema version.
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// The exact classification-custody reference carried by provenance
/// and every derived source: `{contractId, version, digest, decisionId,
/// evidenceDigest}`. The decision id is an opaque
/// `classification-sha256:` identity; no classification content
/// travels with it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationDecisionRef {
    contract_id: String,
    version: String,
    digest: String,
    decision_id: String,
    evidence_digest: String,
}

impl ClassificationDecisionRef {
    /// Assemble from validated members.
    pub fn new(
        contract_id: String,
        version: String,
        digest: String,
        decision_id: String,
        evidence_digest: String,
    ) -> Self {
        Self {
            contract_id,
            version,
            digest,
            decision_id,
            evidence_digest,
        }
    }

    /// The contract identity.
    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }

    /// The contract version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The pinned contract digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The opaque `classification-sha256:` decision identity.
    pub fn decision_id(&self) -> &str {
        &self.decision_id
    }

    /// The opaque evidence digest.
    pub fn evidence_digest(&self) -> &str {
        &self.evidence_digest
    }
}

/// The non-authorizing audit reference `{id, version, evidenceDigest}`.
/// A generic audit ref is metadata only and can never affect an
/// `allow` or `transform-required` outcome.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRef {
    id: String,
    version: String,
    evidence_digest: String,
}

impl AuditRef {
    /// Assemble from validated members.
    pub fn new(id: String, version: String, evidence_digest: String) -> Self {
        Self {
            id,
            version,
            evidence_digest,
        }
    }

    /// The audit identity.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The audit version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The opaque evidence digest.
    pub fn evidence_digest(&self) -> &str {
        &self.evidence_digest
    }
}

/// The closed evidence-outcome vocabulary of the pinned
/// authorizing-evidence contract (the union of every registry row).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceOutcome {
    /// `granted`.
    Granted,
    /// `denied`.
    Denied,
    /// `licensed`.
    Licensed,
    /// `unlicensed`.
    Unlicensed,
    /// `allow`.
    Allow,
    /// `deny`.
    Deny,
    /// `approved`.
    Approved,
    /// `rejected`.
    Rejected,
}

impl EvidenceOutcome {
    /// Every outcome in contract vocabulary order.
    pub const ALL: [EvidenceOutcome; 8] = [
        EvidenceOutcome::Granted,
        EvidenceOutcome::Denied,
        EvidenceOutcome::Licensed,
        EvidenceOutcome::Unlicensed,
        EvidenceOutcome::Allow,
        EvidenceOutcome::Deny,
        EvidenceOutcome::Approved,
        EvidenceOutcome::Rejected,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "granted" => Self::Granted,
            "denied" => Self::Denied,
            "licensed" => Self::Licensed,
            "unlicensed" => Self::Unlicensed,
            "allow" => Self::Allow,
            "deny" => Self::Deny,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::Licensed => "licensed",
            Self::Unlicensed => "unlicensed",
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }
}

/// The closed verification-state vocabulary of the pinned
/// authorizing-evidence contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationState {
    /// `verified`.
    Verified,
    /// `unverified`.
    Unverified,
}

impl VerificationState {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "verified" => Self::Verified,
            "unverified" => Self::Unverified,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
        }
    }
}

/// The closed freshness-state vocabulary of the pinned
/// authorizing-evidence contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreshnessState {
    /// `current`.
    Current,
    /// `stale`.
    Stale,
    /// `expired`.
    Expired,
}

impl FreshnessState {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "current" => Self::Current,
            "stale" => Self::Stale,
            "expired" => Self::Expired,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Expired => "expired",
        }
    }
}

/// The subject binding `{subjectProfileRef, subjectDigest}` of every
/// authorizing evidence record. The digest is an opaque
/// `subject-sha256:` identity over the canonical subject projection;
/// no subject content travels with it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceBinding {
    subject_profile_ref: SubjectProfileRef,
    subject_digest: String,
}

impl EvidenceBinding {
    /// Assemble from validated members.
    pub fn new(subject_profile_ref: SubjectProfileRef, subject_digest: String) -> Self {
        Self {
            subject_profile_ref,
            subject_digest,
        }
    }

    /// The bound subject-profile reference.
    pub fn subject_profile_ref(&self) -> &SubjectProfileRef {
        &self.subject_profile_ref
    }

    /// The opaque `subject-sha256:` binding digest.
    pub fn subject_digest(&self) -> &str {
        &self.subject_digest
    }
}

/// One closed authorizing-evidence record: the exact contract triple,
/// the kind/purpose pair, the outcome, the opaque `evidence-sha256:`
/// identity, verification/freshness state, and the subject binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizingEvidence {
    contract_id: String,
    version: String,
    digest: String,
    evidence_kind: String,
    purpose: String,
    outcome: EvidenceOutcome,
    evidence_id: String,
    verification_state: VerificationState,
    freshness_state: FreshnessState,
    binding: EvidenceBinding,
}

impl AuthorizingEvidence {
    /// Assemble from validated members (the closed ten-member set).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        contract_id: String,
        version: String,
        digest: String,
        evidence_kind: String,
        purpose: String,
        outcome: EvidenceOutcome,
        evidence_id: String,
        verification_state: VerificationState,
        freshness_state: FreshnessState,
        binding: EvidenceBinding,
    ) -> Self {
        Self {
            contract_id,
            version,
            digest,
            evidence_kind,
            purpose,
            outcome,
            evidence_id,
            verification_state,
            freshness_state,
            binding,
        }
    }

    /// The contract identity.
    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }

    /// The contract version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The pinned contract digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The exact evidence kind.
    pub fn evidence_kind(&self) -> &str {
        &self.evidence_kind
    }

    /// The exact purpose.
    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    /// The declared outcome.
    pub const fn outcome(&self) -> EvidenceOutcome {
        self.outcome
    }

    /// The opaque `evidence-sha256:` identity.
    pub fn evidence_id(&self) -> &str {
        &self.evidence_id
    }

    /// The verification state.
    pub const fn verification_state(&self) -> VerificationState {
        self.verification_state
    }

    /// The freshness state.
    pub const fn freshness_state(&self) -> FreshnessState {
        self.freshness_state
    }

    /// The subject binding.
    pub const fn binding(&self) -> &EvidenceBinding {
        &self.binding
    }
}

/// One closed evidence position: the exact `evidenceKind`/`purpose`
/// pair, the single authorizing outcome (when the position has one),
/// and the closed outcome set. These are the exact nine positions of
/// the pinned registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceSpec {
    /// The exact evidence kind.
    pub evidence_kind: &'static str,
    /// The exact purpose.
    pub purpose: &'static str,
    /// The single authorizing outcome, when the position has one.
    pub outcome: Option<EvidenceOutcome>,
    /// The closed outcome set of the position.
    pub outcomes: &'static [EvidenceOutcome],
}

impl EvidenceSpec {
    /// `provenance.publicFixturePermissionRef`.
    pub const PUBLIC_FIXTURE_PERMISSION: Self = Self {
        evidence_kind: "public-fixture-permission",
        purpose: "authorize-public-fixture-permission",
        outcome: Some(EvidenceOutcome::Granted),
        outcomes: &[EvidenceOutcome::Granted, EvidenceOutcome::Denied],
    };
    /// `provenance.publicFixtureLicenseRef`.
    pub const PUBLIC_FIXTURE_LICENSE: Self = Self {
        evidence_kind: "public-fixture-license",
        purpose: "authorize-public-fixture-license",
        outcome: Some(EvidenceOutcome::Licensed),
        outcomes: &[EvidenceOutcome::Licensed, EvidenceOutcome::Unlicensed],
    };
    /// `provenance.publicFixtureConsentRef`.
    pub const PUBLIC_FIXTURE_CONSENT: Self = Self {
        evidence_kind: "public-fixture-consent",
        purpose: "authorize-public-fixture-consent",
        outcome: Some(EvidenceOutcome::Granted),
        outcomes: &[EvidenceOutcome::Granted, EvidenceOutcome::Denied],
    };
    /// `provenance.consumerAclPermissionRef`.
    pub const CONSUMER_ACL_PERMISSION: Self = Self {
        evidence_kind: "consumer-acl-permission",
        purpose: "authorize-consumer-repository-acl",
        outcome: Some(EvidenceOutcome::Granted),
        outcomes: &[EvidenceOutcome::Granted, EvidenceOutcome::Denied],
    };
    /// `provenance.consumerRepositoryConsentRef`.
    pub const CONSUMER_REPOSITORY_CONSENT: Self = Self {
        evidence_kind: "consumer-repository-consent",
        purpose: "authorize-consumer-repository-consent",
        outcome: Some(EvidenceOutcome::Granted),
        outcomes: &[EvidenceOutcome::Granted, EvidenceOutcome::Denied],
    };
    /// `provenance.exportTransferConsentRef`.
    pub const EXPORT_TRANSFER_CONSENT: Self = Self {
        evidence_kind: "export-transfer-consent",
        purpose: "authorize-export-transfer-or-storage",
        outcome: Some(EvidenceOutcome::Granted),
        outcomes: &[EvidenceOutcome::Granted, EvidenceOutcome::Denied],
    };
    /// `conflictResolution.decisionRef` (outcome follows the declared
    /// conflict state, not a fixed spec outcome).
    pub const CONFLICT_RESOLUTION: Self = Self {
        evidence_kind: "conflict-resolution-decision",
        purpose: "resolve-export-conflict",
        outcome: None,
        outcomes: &[EvidenceOutcome::Allow, EvidenceOutcome::Deny],
    };
    /// `derivedArtifact.declassificationDecision.decisionRef`.
    pub const DECLASSIFICATION: Self = Self {
        evidence_kind: "declassification-decision",
        purpose: "authorize-declassification",
        outcome: Some(EvidenceOutcome::Approved),
        outcomes: &[EvidenceOutcome::Approved, EvidenceOutcome::Rejected],
    };
    /// `derivedArtifact.aggregationDecision.decisionRef`.
    pub const AGGREGATION: Self = Self {
        evidence_kind: "aggregation-decision",
        purpose: "authorize-public-aggregation",
        outcome: Some(EvidenceOutcome::Approved),
        outcomes: &[EvidenceOutcome::Approved, EvidenceOutcome::Rejected],
    };
}

/// The state-only measured-value representation: `known` is the only
/// state that carries a value; every other state carries exactly the
/// state member.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueState {
    state: SensitivityState,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Json>,
}

impl ValueState {
    /// A known value (the carried value stays an opaque JSON scalar:
    /// number, string, boolean, or explicit null).
    pub fn known(value: Json) -> Self {
        Self {
            state: SensitivityState::Known,
            value: Some(value),
        }
    }

    /// One of the three state-only alternatives.
    pub const fn state_only(state: SensitivityState) -> Self {
        Self { state, value: None }
    }

    /// The declared state.
    pub const fn state(&self) -> SensitivityState {
        self.state
    }

    /// The carried value, when known.
    pub fn value(&self) -> Option<&Json> {
        self.value.as_ref()
    }
}

/// The resource-path representation: either a normalized NFC
/// project-relative POSIX path or a state-only spelling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePath {
    state: SensitivityState,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
}

impl ResourcePath {
    /// A known project-relative path.
    pub fn known(value: String) -> Self {
        Self {
            state: SensitivityState::Known,
            value: Some(value),
        }
    }

    /// One of the three state-only alternatives.
    pub const fn state_only(state: SensitivityState) -> Self {
        Self { state, value: None }
    }

    /// The declared state.
    pub const fn state(&self) -> SensitivityState {
        self.state
    }

    /// The carried path, when known.
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

impl serde::Serialize for EvidenceOutcome {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl serde::Serialize for VerificationState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl serde::Serialize for FreshnessState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_evidence(outcome: EvidenceOutcome) -> AuthorizingEvidence {
        AuthorizingEvidence::new(
            refs::AUTHORIZING_EVIDENCE_CONTRACT_ID.to_owned(),
            refs::DECISION_FAMILY_VERSION.to_owned(),
            refs::AUTHORIZING_EVIDENCE_RAW_SHA256.to_owned(),
            "declassification-decision".to_owned(),
            "authorize-declassification".to_owned(),
            outcome,
            format!("evidence-sha256:{}", "a".repeat(64)),
            VerificationState::Verified,
            FreshnessState::Current,
            EvidenceBinding::new(
                SubjectProfileRef::frozen(),
                format!("subject-sha256:{}", "b".repeat(64)),
            ),
        )
    }

    /// Every evidence outcome round-trips its wire spelling and refuses
    /// unknown spellings.
    #[test]
    fn evidence_outcomes_round_trip() {
        for outcome in EvidenceOutcome::ALL {
            assert_eq!(EvidenceOutcome::parse(outcome.as_str()), Some(outcome));
        }
        assert_eq!(EvidenceOutcome::parse("approved-"), None);
        assert_eq!(
            VerificationState::parse("verified"),
            Some(VerificationState::Verified)
        );
        assert_eq!(VerificationState::parse("reverified"), None);
        assert_eq!(FreshnessState::parse("stale"), Some(FreshnessState::Stale));
        assert_eq!(FreshnessState::parse("fresh"), None);
    }

    /// The evidence specs are the exact nine registry rows.
    #[test]
    fn evidence_specs_are_the_exact_registry_rows() {
        assert_eq!(
            EvidenceSpec::PUBLIC_FIXTURE_PERMISSION.evidence_kind,
            "public-fixture-permission"
        );
        assert_eq!(EvidenceSpec::CONFLICT_RESOLUTION.outcome, None);
        assert_eq!(
            EvidenceSpec::AGGREGATION.outcome,
            Some(EvidenceOutcome::Approved)
        );
        let evidence = sample_evidence(EvidenceOutcome::Approved);
        assert_eq!(evidence.evidence_kind(), "declassification-decision");
        assert_eq!(evidence.verification_state(), VerificationState::Verified);
    }

    /// Wire spellings serialize exactly: camelCase members, nullable
    /// state-only shapes, and value-carrying known states.
    #[test]
    fn wire_serialization_is_exact() {
        let policy = PolicyRef::frozen();
        assert_eq!(
            serde_json::to_value(&policy).unwrap(),
            json!({
                "policyId": refs::POLICY_ID,
                "version": "0.3.2",
                "digest": refs::POLICY_DIGEST,
            })
        );
        let path = ResourcePath::known("docs/readme.md".to_owned());
        assert_eq!(
            serde_json::to_value(&path).unwrap(),
            json!({"state": "known", "value": "docs/readme.md"})
        );
        let withheld = ResourcePath::state_only(SensitivityState::Withheld);
        assert_eq!(
            serde_json::to_value(&withheld).unwrap(),
            json!({"state": "withheld"})
        );
        let zero = ValueState::known(json!(0));
        assert_eq!(
            serde_json::to_value(&zero).unwrap(),
            json!({"state": "known", "value": 0})
        );
        let unknown = ValueState::state_only(SensitivityState::Unknown);
        assert_eq!(
            serde_json::to_value(&unknown).unwrap(),
            json!({"state": "unknown"})
        );
        let evidence = sample_evidence(EvidenceOutcome::Approved);
        let wire = serde_json::to_value(&evidence).unwrap();
        let object = wire.as_object().unwrap();
        assert_eq!(object.len(), 10);
        assert_eq!(object["evidenceKind"], "declassification-decision");
        assert_eq!(object["outcome"], "approved");
        assert_eq!(object["verificationState"], "verified");
        assert_eq!(object["freshnessState"], "current");
    }
}
