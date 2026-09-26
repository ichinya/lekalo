//! The typed export-decision input (issue #119).
//!
//! [`ExportDecisionInput`] is the exact closed member set of
//! `contracts/privacy-export.schema.v0.3.2.json`: eighteen required
//! members plus the two optional members (`valueState`,
//! `derivedArtifact`). Member sets are closed by construction — the
//! struct shape cannot gain a member, vocabulary members are closed
//! enums, and the nullable positions serialize as explicit JSON nulls
//! exactly where the schema demands them. The only wire-acceptance
//! path is the exact-reason-code validator
//! ([`super::evaluate`]); this type is what that validator
//! constructs and what the runtime synthesizes and serializes.

use serde::Serialize;

use super::types::{
    AuditRef, AuthorizingEvidence, ClassificationDecisionRef, DecisionContractRef, ResourcePath,
    ValueState,
};
use super::types::{
    AuthorityRef as AuthorityRefWire, EvidenceContractRef, PolicyRef, SubjectProfileRef,
};
use super::vocab::{
    Audience, ConflictState, ConstraintScope, DataSensitivity, ExportDisposition, OperationId,
    ProvenanceOrigin, RepositoryRelation, RepositoryRole, TenantRelation, TransformId,
    TrustBoundary,
};

/// The wire shape `{id, version}` of the evaluated operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    id: OperationId,
    version: String,
}

impl Operation {
    /// Assemble from a closed operation and its exact vocabulary
    /// version.
    pub fn new(id: OperationId, version: String) -> Self {
        Self { id, version }
    }

    /// The closed operation.
    pub const fn id(&self) -> OperationId {
        self.id
    }

    /// The operation vocabulary version.
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// A repository endpoint `{repositoryRole, repositoryRef}`. The ref
/// is an opaque `repo-sha256:` identity or explicit null.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    repository_role: RepositoryRole,
    repository_ref: Option<String>,
}

impl Endpoint {
    /// Assemble from validated members.
    pub fn new(repository_role: RepositoryRole, repository_ref: Option<String>) -> Self {
        Self {
            repository_role,
            repository_ref,
        }
    }

    /// The declared repository role.
    pub const fn repository_role(&self) -> RepositoryRole {
        self.repository_role
    }

    /// The opaque repository ref, when repository-backed.
    pub fn repository_ref(&self) -> Option<&str> {
        self.repository_ref.as_deref()
    }
}

/// The destination `{repositoryRole, repositoryRef, trustBoundary,
/// repositoryRelation, tenantRelation}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    repository_role: RepositoryRole,
    repository_ref: Option<String>,
    trust_boundary: TrustBoundary,
    repository_relation: RepositoryRelation,
    tenant_relation: TenantRelation,
}

impl Destination {
    /// Assemble from validated members.
    pub fn new(
        repository_role: RepositoryRole,
        repository_ref: Option<String>,
        trust_boundary: TrustBoundary,
        repository_relation: RepositoryRelation,
        tenant_relation: TenantRelation,
    ) -> Self {
        Self {
            repository_role,
            repository_ref,
            trust_boundary,
            repository_relation,
            tenant_relation,
        }
    }

    /// The declared destination repository role.
    pub const fn repository_role(&self) -> RepositoryRole {
        self.repository_role
    }

    /// The opaque destination repository ref.
    pub fn repository_ref(&self) -> Option<&str> {
        self.repository_ref.as_deref()
    }

    /// The declared trust boundary.
    pub const fn trust_boundary(&self) -> TrustBoundary {
        self.trust_boundary
    }

    /// The declared repository relation.
    pub const fn repository_relation(&self) -> RepositoryRelation {
        self.repository_relation
    }

    /// The declared tenant relation.
    pub const fn tenant_relation(&self) -> TenantRelation {
        self.tenant_relation
    }
}

/// The provenance block: origin, custody role/ref, the two origin
/// booleans, the exact classification-custody reference, and the six
/// nullable authorizing-evidence positions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    origin: ProvenanceOrigin,
    repository_role: RepositoryRole,
    repository_ref: Option<String>,
    synthetic: bool,
    derived: bool,
    classification_ref: ClassificationDecisionRef,
    public_fixture_permission_ref: Option<AuthorizingEvidence>,
    public_fixture_license_ref: Option<AuthorizingEvidence>,
    public_fixture_consent_ref: Option<AuthorizingEvidence>,
    consumer_acl_permission_ref: Option<AuthorizingEvidence>,
    consumer_repository_consent_ref: Option<AuthorizingEvidence>,
    export_transfer_consent_ref: Option<AuthorizingEvidence>,
}

impl Provenance {
    /// Assemble from validated members (the closed twelve-member set).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        origin: ProvenanceOrigin,
        repository_role: RepositoryRole,
        repository_ref: Option<String>,
        synthetic: bool,
        derived: bool,
        classification_ref: ClassificationDecisionRef,
        public_fixture_permission_ref: Option<AuthorizingEvidence>,
        public_fixture_license_ref: Option<AuthorizingEvidence>,
        public_fixture_consent_ref: Option<AuthorizingEvidence>,
        consumer_acl_permission_ref: Option<AuthorizingEvidence>,
        consumer_repository_consent_ref: Option<AuthorizingEvidence>,
        export_transfer_consent_ref: Option<AuthorizingEvidence>,
    ) -> Self {
        Self {
            origin,
            repository_role,
            repository_ref,
            synthetic,
            derived,
            classification_ref,
            public_fixture_permission_ref,
            public_fixture_license_ref,
            public_fixture_consent_ref,
            consumer_acl_permission_ref,
            consumer_repository_consent_ref,
            export_transfer_consent_ref,
        }
    }

    /// The declared origin.
    pub const fn origin(&self) -> ProvenanceOrigin {
        self.origin
    }

    /// The provenance repository role.
    pub const fn repository_role(&self) -> RepositoryRole {
        self.repository_role
    }

    /// The provenance repository ref.
    pub fn repository_ref(&self) -> Option<&str> {
        self.repository_ref.as_deref()
    }

    /// The synthetic flag.
    pub const fn synthetic(&self) -> bool {
        self.synthetic
    }

    /// The derived flag.
    pub const fn derived(&self) -> bool {
        self.derived
    }

    /// The exact classification-custody reference.
    pub const fn classification_ref(&self) -> &ClassificationDecisionRef {
        &self.classification_ref
    }

    /// `publicFixturePermissionRef`.
    pub const fn public_fixture_permission_ref(&self) -> Option<&AuthorizingEvidence> {
        self.public_fixture_permission_ref.as_ref()
    }

    /// `publicFixtureLicenseRef`.
    pub const fn public_fixture_license_ref(&self) -> Option<&AuthorizingEvidence> {
        self.public_fixture_license_ref.as_ref()
    }

    /// `publicFixtureConsentRef`.
    pub const fn public_fixture_consent_ref(&self) -> Option<&AuthorizingEvidence> {
        self.public_fixture_consent_ref.as_ref()
    }

    /// `consumerAclPermissionRef`.
    pub const fn consumer_acl_permission_ref(&self) -> Option<&AuthorizingEvidence> {
        self.consumer_acl_permission_ref.as_ref()
    }

    /// `consumerRepositoryConsentRef`.
    pub const fn consumer_repository_consent_ref(&self) -> Option<&AuthorizingEvidence> {
        self.consumer_repository_consent_ref.as_ref()
    }

    /// `exportTransferConsentRef`.
    pub const fn export_transfer_consent_ref(&self) -> Option<&AuthorizingEvidence> {
        self.export_transfer_consent_ref.as_ref()
    }
}

/// The conflict state `{state, decisionRef}`. The decision ref is an
/// authorizing evidence record or explicit null.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictResolution {
    state: ConflictState,
    decision_ref: Option<AuthorizingEvidence>,
}

impl ConflictResolution {
    /// Assemble from validated members.
    pub fn new(state: ConflictState, decision_ref: Option<AuthorizingEvidence>) -> Self {
        Self {
            state,
            decision_ref,
        }
    }

    /// The declared conflict state.
    pub const fn state(&self) -> ConflictState {
        self.state
    }

    /// The resolved decision evidence, when resolved.
    pub const fn decision_ref(&self) -> Option<&AuthorizingEvidence> {
        self.decision_ref.as_ref()
    }
}

/// One local constraint `{scope, constraintRef, allowedOperations,
/// allowedTrustBoundaries, allowedAudiences}`. The constraint ref is
/// non-authorizing audit metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Constraint {
    scope: ConstraintScope,
    constraint_ref: AuditRef,
    allowed_operations: Vec<OperationId>,
    allowed_trust_boundaries: Vec<TrustBoundary>,
    allowed_audiences: Vec<Audience>,
}

impl Constraint {
    /// Assemble from validated members.
    pub fn new(
        scope: ConstraintScope,
        constraint_ref: AuditRef,
        allowed_operations: Vec<OperationId>,
        allowed_trust_boundaries: Vec<TrustBoundary>,
        allowed_audiences: Vec<Audience>,
    ) -> Self {
        Self {
            scope,
            constraint_ref,
            allowed_operations,
            allowed_trust_boundaries,
            allowed_audiences,
        }
    }

    /// The constraint scope.
    pub const fn scope(&self) -> ConstraintScope {
        self.scope
    }

    /// The non-authorizing audit ref.
    pub const fn constraint_ref(&self) -> &AuditRef {
        &self.constraint_ref
    }

    /// The allowed operations.
    pub fn allowed_operations(&self) -> &[OperationId] {
        &self.allowed_operations
    }

    /// The allowed trust boundaries.
    pub fn allowed_trust_boundaries(&self) -> &[TrustBoundary] {
        &self.allowed_trust_boundaries
    }

    /// The allowed audiences.
    pub fn allowed_audiences(&self) -> &[Audience] {
        &self.allowed_audiences
    }
}

/// One derived source `{sourceRef, artifactKind, authorityRef,
/// policyRef, classificationRef, dataSensitivity, exportDisposition}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceArtifact {
    source_ref: String,
    artifact_kind: String,
    authority_ref: AuthorityRefWire,
    policy_ref: PolicyRef,
    classification_ref: ClassificationDecisionRef,
    data_sensitivity: Vec<DataSensitivity>,
    export_disposition: ExportDisposition,
}

impl SourceArtifact {
    /// Assemble from validated members.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_ref: String,
        artifact_kind: String,
        authority_ref: AuthorityRefWire,
        policy_ref: PolicyRef,
        classification_ref: ClassificationDecisionRef,
        data_sensitivity: Vec<DataSensitivity>,
        export_disposition: ExportDisposition,
    ) -> Self {
        Self {
            source_ref,
            artifact_kind,
            authority_ref,
            policy_ref,
            classification_ref,
            data_sensitivity,
            export_disposition,
        }
    }

    /// The opaque `source-sha256:` identity.
    pub fn source_ref(&self) -> &str {
        &self.source_ref
    }

    /// The source artifact kind.
    pub fn artifact_kind(&self) -> &str {
        &self.artifact_kind
    }

    /// The source authority reference.
    pub const fn authority_ref(&self) -> &AuthorityRefWire {
        &self.authority_ref
    }

    /// The source policy reference.
    pub const fn policy_ref(&self) -> &PolicyRef {
        &self.policy_ref
    }

    /// The source classification reference.
    pub const fn classification_ref(&self) -> &ClassificationDecisionRef {
        &self.classification_ref
    }

    /// The source sensitivity labels.
    pub fn data_sensitivity(&self) -> &[DataSensitivity] {
        &self.data_sensitivity
    }

    /// The source default disposition.
    pub const fn export_disposition(&self) -> ExportDisposition {
        self.export_disposition
    }
}

/// One applied transform `{transformId, version, evidenceDigest}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedTransform {
    transform_id: TransformId,
    version: String,
    evidence_digest: String,
}

impl AppliedTransform {
    /// Assemble from validated members.
    pub fn new(transform_id: TransformId, version: String, evidence_digest: String) -> Self {
        Self {
            transform_id,
            version,
            evidence_digest,
        }
    }

    /// The closed transform id.
    pub const fn transform_id(&self) -> TransformId {
        self.transform_id
    }

    /// The exact transform vocabulary version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The opaque evidence digest.
    pub fn evidence_digest(&self) -> &str {
        &self.evidence_digest
    }
}

/// The declassification decision `{policyRef, decisionRef, version,
/// outcome, removedSensitivities}` naming exactly the labels removed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclassificationDecision {
    policy_ref: PolicyRef,
    decision_ref: AuthorizingEvidence,
    version: String,
    outcome: super::types::EvidenceOutcome,
    removed_sensitivities: Vec<DataSensitivity>,
}

impl DeclassificationDecision {
    /// Assemble from validated members.
    pub fn new(
        policy_ref: PolicyRef,
        decision_ref: AuthorizingEvidence,
        version: String,
        outcome: super::types::EvidenceOutcome,
        removed_sensitivities: Vec<DataSensitivity>,
    ) -> Self {
        Self {
            policy_ref,
            decision_ref,
            version,
            outcome,
            removed_sensitivities,
        }
    }

    /// The policy reference.
    pub const fn policy_ref(&self) -> &PolicyRef {
        &self.policy_ref
    }

    /// The authorizing decision evidence.
    pub const fn decision_ref(&self) -> &AuthorizingEvidence {
        &self.decision_ref
    }

    /// The exact decision version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The declared outcome.
    pub const fn outcome(&self) -> super::types::EvidenceOutcome {
        self.outcome
    }

    /// The exactly-named removed labels.
    pub fn removed_sensitivities(&self) -> &[DataSensitivity] {
        &self.removed_sensitivities
    }
}

/// The aggregation decision `{policyRef, decisionRef, version,
/// outcome, removesSourceRows, removesSourceIdentities}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregationDecision {
    policy_ref: PolicyRef,
    decision_ref: AuthorizingEvidence,
    version: String,
    outcome: super::types::EvidenceOutcome,
    removes_source_rows: bool,
    removes_source_identities: bool,
}

impl AggregationDecision {
    /// Assemble from validated members.
    pub fn new(
        policy_ref: PolicyRef,
        decision_ref: AuthorizingEvidence,
        version: String,
        outcome: super::types::EvidenceOutcome,
        removes_source_rows: bool,
        removes_source_identities: bool,
    ) -> Self {
        Self {
            policy_ref,
            decision_ref,
            version,
            outcome,
            removes_source_rows,
            removes_source_identities,
        }
    }

    /// The policy reference.
    pub const fn policy_ref(&self) -> &PolicyRef {
        &self.policy_ref
    }

    /// The authorizing decision evidence.
    pub const fn decision_ref(&self) -> &AuthorizingEvidence {
        &self.decision_ref
    }

    /// The exact decision version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The declared outcome.
    pub const fn outcome(&self) -> super::types::EvidenceOutcome {
        self.outcome
    }

    /// Whether source rows are removed.
    pub const fn removes_source_rows(&self) -> bool {
        self.removes_source_rows
    }

    /// Whether source identities are removed.
    pub const fn removes_source_identities(&self) -> bool {
        self.removes_source_identities
    }
}

/// The derived-artifact block: every retained source, the applied
/// transforms, the two nullable decision records, and the three
/// content booleans.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedArtifact {
    source_artifacts: Vec<SourceArtifact>,
    applied_transforms: Vec<AppliedTransform>,
    declassification_decision: Option<DeclassificationDecision>,
    aggregation_decision: Option<AggregationDecision>,
    contains_source_rows: bool,
    contains_source_identities: bool,
    reevaluated: bool,
}

impl DerivedArtifact {
    /// Assemble from validated members.
    pub fn new(
        source_artifacts: Vec<SourceArtifact>,
        applied_transforms: Vec<AppliedTransform>,
        declassification_decision: Option<DeclassificationDecision>,
        aggregation_decision: Option<AggregationDecision>,
        contains_source_rows: bool,
        contains_source_identities: bool,
        reevaluated: bool,
    ) -> Self {
        Self {
            source_artifacts,
            applied_transforms,
            declassification_decision,
            aggregation_decision,
            contains_source_rows,
            contains_source_identities,
            reevaluated,
        }
    }

    /// Every retained source artifact.
    pub fn source_artifacts(&self) -> &[SourceArtifact] {
        &self.source_artifacts
    }

    /// Every applied transform.
    pub fn applied_transforms(&self) -> &[AppliedTransform] {
        &self.applied_transforms
    }

    /// The declassification decision, when present.
    pub const fn declassification_decision(&self) -> Option<&DeclassificationDecision> {
        self.declassification_decision.as_ref()
    }

    /// The aggregation decision, when present.
    pub const fn aggregation_decision(&self) -> Option<&AggregationDecision> {
        self.aggregation_decision.as_ref()
    }

    /// Whether source rows remain.
    pub const fn contains_source_rows(&self) -> bool {
        self.contains_source_rows
    }

    /// Whether source identities remain.
    pub const fn contains_source_identities(&self) -> bool {
        self.contains_source_identities
    }

    /// Whether the derived output was reevaluated.
    pub const fn reevaluated(&self) -> bool {
        self.reevaluated
    }
}

/// The closed export-decision input: the exact member set of the
/// frozen strict input schema.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDecisionInput {
    decision_contract_ref: DecisionContractRef,
    authority_ref: AuthorityRefWire,
    policy_ref: PolicyRef,
    authorizing_evidence_contract_ref: EvidenceContractRef,
    authorization_subject_profile_ref: SubjectProfileRef,
    artifact_kind: String,
    artifact_ref: String,
    operation: Operation,
    source: Endpoint,
    destination: Destination,
    audience: Audience,
    data_sensitivity: Vec<DataSensitivity>,
    export_disposition: ExportDisposition,
    provenance: Provenance,
    resource_path: ResourcePath,
    #[serde(skip_serializing_if = "Option::is_none")]
    value_state: Option<ValueState>,
    conflict_resolution: ConflictResolution,
    constraints: Vec<Constraint>,
    /// Always null: the accepted policy contains no broadening grant,
    /// and a caller-supplied grant can never legalize broadening.
    broadening_grant: (),
    #[serde(skip_serializing_if = "Option::is_none")]
    derived_artifact: Option<DerivedArtifact>,
}

impl ExportDecisionInput {
    /// Assemble from validated members (the exact closed member set).
    /// Construction is deny-unknown by type: the member set is the
    /// struct, every vocabulary member is a closed enum, and both
    /// nullable-only positions (`broadeningGrant`) are unrepresentable
    /// as anything but null.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        decision_contract_ref: DecisionContractRef,
        authority_ref: AuthorityRefWire,
        policy_ref: PolicyRef,
        authorizing_evidence_contract_ref: EvidenceContractRef,
        authorization_subject_profile_ref: SubjectProfileRef,
        artifact_kind: String,
        artifact_ref: String,
        operation: Operation,
        source: Endpoint,
        destination: Destination,
        audience: Audience,
        data_sensitivity: Vec<DataSensitivity>,
        export_disposition: ExportDisposition,
        provenance: Provenance,
        resource_path: ResourcePath,
        value_state: Option<ValueState>,
        conflict_resolution: ConflictResolution,
        constraints: Vec<Constraint>,
        derived_artifact: Option<DerivedArtifact>,
    ) -> Self {
        Self {
            decision_contract_ref,
            authority_ref,
            policy_ref,
            authorizing_evidence_contract_ref,
            authorization_subject_profile_ref,
            artifact_kind,
            artifact_ref,
            operation,
            source,
            destination,
            audience,
            data_sensitivity,
            export_disposition,
            provenance,
            resource_path,
            value_state,
            conflict_resolution,
            constraints,
            broadening_grant: (),
            derived_artifact,
        }
    }

    /// Assemble with the exact frozen contract references in every
    /// reference position — the only custody-honest shortcut.
    #[allow(clippy::too_many_arguments)]
    pub fn with_frozen_refs(
        artifact_kind: String,
        artifact_ref: String,
        operation: Operation,
        source: Endpoint,
        destination: Destination,
        audience: Audience,
        data_sensitivity: Vec<DataSensitivity>,
        export_disposition: ExportDisposition,
        provenance: Provenance,
        resource_path: ResourcePath,
        value_state: Option<ValueState>,
        conflict_resolution: ConflictResolution,
        constraints: Vec<Constraint>,
        derived_artifact: Option<DerivedArtifact>,
    ) -> Self {
        Self::new(
            DecisionContractRef::frozen(),
            AuthorityRefWire::frozen_authority(),
            PolicyRef::frozen(),
            EvidenceContractRef::frozen_evidence(),
            SubjectProfileRef::frozen(),
            artifact_kind,
            artifact_ref,
            operation,
            source,
            destination,
            audience,
            data_sensitivity,
            export_disposition,
            provenance,
            resource_path,
            value_state,
            conflict_resolution,
            constraints,
            derived_artifact,
        )
    }

    /// The decision contract reference.
    pub const fn decision_contract_ref(&self) -> &DecisionContractRef {
        &self.decision_contract_ref
    }

    /// The authority reference.
    pub const fn authority_ref(&self) -> &AuthorityRefWire {
        &self.authority_ref
    }

    /// The policy reference.
    pub const fn policy_ref(&self) -> &PolicyRef {
        &self.policy_ref
    }

    /// The authorizing-evidence contract reference.
    pub const fn authorizing_evidence_contract_ref(&self) -> &EvidenceContractRef {
        &self.authorizing_evidence_contract_ref
    }

    /// The authorization-subject-profile reference.
    pub const fn authorization_subject_profile_ref(&self) -> &SubjectProfileRef {
        &self.authorization_subject_profile_ref
    }

    /// The artifact kind (validated against the closed registry at
    /// evaluation time; unknown kinds deny).
    pub fn artifact_kind(&self) -> &str {
        &self.artifact_kind
    }

    /// The opaque `artifact-sha256:` identity.
    pub fn artifact_ref(&self) -> &str {
        &self.artifact_ref
    }

    /// The evaluated operation.
    pub const fn operation(&self) -> &Operation {
        &self.operation
    }

    /// The source endpoint.
    pub const fn source(&self) -> &Endpoint {
        &self.source
    }

    /// The destination.
    pub const fn destination(&self) -> &Destination {
        &self.destination
    }

    /// The declared audience.
    pub const fn audience(&self) -> Audience {
        self.audience
    }

    /// The non-empty unique sensitivity labels.
    pub fn data_sensitivity(&self) -> &[DataSensitivity] {
        &self.data_sensitivity
    }

    /// The declared disposition.
    pub const fn export_disposition(&self) -> ExportDisposition {
        self.export_disposition
    }

    /// The provenance block.
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// The resource-path representation.
    pub const fn resource_path(&self) -> &ResourcePath {
        &self.resource_path
    }

    /// The optional measured-value state.
    pub const fn value_state(&self) -> Option<&ValueState> {
        self.value_state.as_ref()
    }

    /// The conflict resolution.
    pub const fn conflict_resolution(&self) -> &ConflictResolution {
        &self.conflict_resolution
    }

    /// The local constraints.
    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    /// The optional derived-artifact block.
    pub const fn derived_artifact(&self) -> Option<&DerivedArtifact> {
        self.derived_artifact.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::privacy::refs;

    /// A minimal closed decision input for the synthetic public
    /// fixture publish vector.
    fn synthetic_publish_input() -> ExportDecisionInput {
        ExportDecisionInput::with_frozen_refs(
            "fixture".to_owned(),
            format!("artifact-sha256:{}", "1".repeat(64)),
            Operation::new(OperationId::Publish, "0.2.16".to_owned()),
            Endpoint::new(RepositoryRole::LocalWorkspace, None),
            Destination::new(
                RepositoryRole::PublicChannel,
                None,
                TrustBoundary::Public,
                RepositoryRelation::NotApplicable,
                TenantRelation::NotApplicable,
            ),
            Audience::Public,
            vec![DataSensitivity::Public],
            ExportDisposition::PublicFixture,
            Provenance::new(
                ProvenanceOrigin::Synthetic,
                RepositoryRole::LocalWorkspace,
                None,
                true,
                false,
                ClassificationDecisionRef::new(
                    refs::CLASSIFICATION_CONTRACT_ID.to_owned(),
                    refs::DECISION_FAMILY_VERSION.to_owned(),
                    format!("sha256:{}", refs::CLASSIFICATION_CONTRACT_RAW_SHA256),
                    format!("classification-sha256:{}", "1".repeat(64)),
                    format!("sha256:{}", "1".repeat(64)),
                ),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            ResourcePath::known("tests/fixtures/synthetic.json".to_owned()),
            None,
            ConflictResolution::new(ConflictState::None, None),
            Vec::new(),
            None,
        )
    }

    /// The typed input serializes to the exact closed wire shape: the
    /// optional members stay absent, nullable positions are explicit
    /// nulls, and `broadeningGrant` is always null.
    #[test]
    fn wire_shape_is_the_exact_closed_member_set() {
        let wire = serde_json::to_value(synthetic_publish_input()).unwrap();
        let object = wire.as_object().unwrap();
        assert_eq!(object.len(), 18);
        for member in [
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
        ] {
            assert!(object.contains_key(member), "missing {member}");
        }
        assert!(!object.contains_key("valueState"));
        assert!(!object.contains_key("derivedArtifact"));
        assert_eq!(object["broadeningGrant"], json!(null));
        assert_eq!(
            object["source"],
            json!({"repositoryRole": "local-workspace", "repositoryRef": null})
        );
        let provenance = object["provenance"].as_object().unwrap();
        assert_eq!(provenance.len(), 12);
        for field in [
            "publicFixturePermissionRef",
            "publicFixtureLicenseRef",
            "publicFixtureConsentRef",
            "consumerAclPermissionRef",
            "consumerRepositoryConsentRef",
            "exportTransferConsentRef",
        ] {
            assert_eq!(
                provenance[field],
                json!(null),
                "{field} must serialize as null"
            );
        }
    }

    /// Adding the optional members serializes them in place, and the
    /// derived block carries explicit nulls for absent decisions.
    #[test]
    fn optional_members_serialize_in_place() {
        let mut input = synthetic_publish_input();
        input.value_state = Some(ValueState::known(json!(0)));
        input.derived_artifact = Some(DerivedArtifact::new(
            vec![SourceArtifact::new(
                format!("source-sha256:{}", "2".repeat(64)),
                "fixture".to_owned(),
                AuthorityRefWire::frozen_authority(),
                PolicyRef::frozen(),
                ClassificationDecisionRef::new(
                    refs::CLASSIFICATION_CONTRACT_ID.to_owned(),
                    refs::DECISION_FAMILY_VERSION.to_owned(),
                    format!("sha256:{}", refs::CLASSIFICATION_CONTRACT_RAW_SHA256),
                    format!("classification-sha256:{}", "2".repeat(64)),
                    format!("sha256:{}", "2".repeat(64)),
                ),
                vec![DataSensitivity::Public],
                ExportDisposition::PublicFixture,
            )],
            vec![AppliedTransform::new(
                TransformId::RedactContent,
                "0.2.16".to_owned(),
                format!("sha256:{}", "3".repeat(64)),
            )],
            None,
            None,
            false,
            false,
            true,
        ));
        let wire = serde_json::to_value(&input).unwrap();
        let object = wire.as_object().unwrap();
        assert_eq!(object.len(), 20);
        assert_eq!(object["valueState"], json!({"state": "known", "value": 0}));
        let derived = object["derivedArtifact"].as_object().unwrap();
        assert_eq!(derived.len(), 7);
        assert_eq!(derived["declassificationDecision"], json!(null));
        assert_eq!(derived["aggregationDecision"], json!(null));
        assert_eq!(derived["reevaluated"], json!(true));
        assert_eq!(
            derived["sourceArtifacts"][0]["exportDisposition"],
            json!("public-fixture")
        );
        assert_eq!(
            derived["appliedTransforms"][0]["transformId"],
            json!("redact-content")
        );
    }

    /// Deterministic serialization: two structurally equal inputs
    /// produce identical wire bytes.
    #[test]
    fn serialization_is_deterministic() {
        let wire = serde_json::to_string(&synthetic_publish_input()).unwrap();
        let again = serde_json::to_string(&synthetic_publish_input()).unwrap();
        assert_eq!(wire, again);
        assert!(wire.contains(r#""artifactKind":"fixture""#));
    }
}
