//! The typed composition attachment value (issue #51).
//!
//! Every collection is canonically ordered at construction; the value
//! carries no source spans, no runtime values, and no free-form text
//! beyond bounded descriptions.

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;

/// A validated bounded identifier of the composition surface:
/// lowercase segments separated by dots or slashes.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct CompositionId(String);

impl CompositionId {
    /// Validate and keep the exact text.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || text.len() > 192 {
            return None;
        }
        for segment in text.split(['.', '/']) {
            let bytes = segment.as_bytes();
            if bytes.is_empty()
                || bytes.len() > 63
                || !bytes[0].is_ascii_lowercase()
                || !bytes[1..].iter().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || *byte == b'_'
                        || *byte == b'-'
                })
            {
                return None;
            }
        }
        Some(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// One exact consumed-attachment pin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumedRef {
    pub(crate) family: String,
    pub(crate) identity: String,
    pub(crate) version: String,
    pub(crate) digest: Sha256Digest,
}

impl ConsumedRef {
    pub fn family(&self) -> &str {
        &self.family
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// One declared effect of one operation role.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct EffectDeclaration {
    pub(crate) kind: String,
    pub(crate) effect_id: CompositionId,
}

impl EffectDeclaration {
    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn effect_id(&self) -> &str {
        self.effect_id.as_str()
    }
}

/// One declared operation role with its explicit effect set and commit
/// boundary. `default_selection` is always `false`: no action enables
/// another implicitly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationRole {
    pub(crate) operation: String,
    pub(crate) effects: Vec<EffectDeclaration>,
    pub(crate) commit_boundary: String,
    pub(crate) default_selection: bool,
}

impl OperationRole {
    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn effects(&self) -> &[EffectDeclaration] {
        &self.effects
    }

    pub fn commit_boundary(&self) -> &str {
        &self.commit_boundary
    }

    pub const fn default_selection(&self) -> bool {
        self.default_selection
    }
}

/// One independence rule: `operation` must never enable any of
/// `must_not_enable`.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct IndependenceRule {
    pub(crate) operation: String,
    pub(crate) must_not_enable: Vec<String>,
}

impl IndependenceRule {
    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn must_not_enable(&self) -> &[String] {
        &self.must_not_enable
    }
}

/// One correlation group over a shared observation key.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Correlation {
    pub(crate) correlation_id: CompositionId,
    pub(crate) key_ref: String,
    pub(crate) member_operations: Vec<String>,
}

impl Correlation {
    pub fn correlation_id(&self) -> &str {
        self.correlation_id.as_str()
    }

    pub fn key_ref(&self) -> &str {
        &self.key_ref
    }

    pub fn member_operations(&self) -> &[String] {
        &self.member_operations
    }
}

/// One approval binding: the exact fields the publication precondition
/// re-checks at commit time.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ApprovalBinding {
    pub(crate) approval_id: CompositionId,
    pub(crate) operation: String,
    pub(crate) preview_revision_field: String,
    pub(crate) preview_digest_field: String,
    pub(crate) tenant_field: String,
    pub(crate) destination_field: String,
    pub(crate) authority_field: String,
    pub(crate) policy_revision_field: String,
}

impl ApprovalBinding {
    pub fn approval_id(&self) -> &str {
        self.approval_id.as_str()
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn preview_revision_field(&self) -> &str {
        &self.preview_revision_field
    }

    pub fn preview_digest_field(&self) -> &str {
        &self.preview_digest_field
    }

    pub fn tenant_field(&self) -> &str {
        &self.tenant_field
    }

    pub fn destination_field(&self) -> &str {
        &self.destination_field
    }

    pub fn authority_field(&self) -> &str {
        &self.authority_field
    }

    pub fn policy_revision_field(&self) -> &str {
        &self.policy_revision_field
    }
}

/// One append-only snapshot declaration.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct SnapshotDecl {
    pub(crate) snapshot_id: CompositionId,
    pub(crate) entity: CompositionId,
    pub(crate) revision_field: String,
    pub(crate) digest_field: String,
    pub(crate) schema_version_field: String,
    pub(crate) immutability: String,
}

impl SnapshotDecl {
    pub fn snapshot_id(&self) -> &str {
        self.snapshot_id.as_str()
    }

    pub fn entity(&self) -> &str {
        self.entity.as_str()
    }

    pub fn revision_field(&self) -> &str {
        &self.revision_field
    }

    pub fn digest_field(&self) -> &str {
        &self.digest_field
    }

    pub fn schema_version_field(&self) -> &str {
        &self.schema_version_field
    }

    pub fn immutability(&self) -> &str {
        &self.immutability
    }
}

/// One public allowlist field.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct PublicField {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) source: String,
}

impl PublicField {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

/// The shared rendering and safe-URL policy of preview and final
/// public output.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct RenderingPolicy {
    pub(crate) default_mode: String,
    pub(crate) url_schemes: Vec<String>,
    pub(crate) reject_encoded: bool,
    pub(crate) reject_control_chars: bool,
}

impl RenderingPolicy {
    pub fn default_mode(&self) -> &str {
        &self.default_mode
    }

    pub fn url_schemes(&self) -> &[String] {
        &self.url_schemes
    }

    pub const fn reject_encoded(&self) -> bool {
        self.reject_encoded
    }

    pub const fn reject_control_chars(&self) -> bool {
        self.reject_control_chars
    }
}

/// The public DTO allowlist: the only fields a public projection may
/// serve, and the closed excluded set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicAllowlist {
    pub(crate) dto: CompositionId,
    pub(crate) fields: Vec<PublicField>,
    pub(crate) excluded: Vec<String>,
    pub(crate) rendering: RenderingPolicy,
}

impl PublicAllowlist {
    pub fn dto(&self) -> &str {
        self.dto.as_str()
    }

    pub fn fields(&self) -> &[PublicField] {
        &self.fields
    }

    pub fn excluded(&self) -> &[String] {
        &self.excluded
    }

    pub fn rendering(&self) -> &RenderingPolicy {
        &self.rendering
    }
}

/// One payload-bound replay scope.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ReplayScope {
    pub(crate) operation: String,
    pub(crate) scope: CompositionId,
    pub(crate) key_ref: String,
    pub(crate) duplicate: String,
    pub(crate) same_key_different_request: String,
    pub(crate) payload_binding: String,
}

impl ReplayScope {
    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn scope(&self) -> &str {
        self.scope.as_str()
    }

    pub fn key_ref(&self) -> &str {
        &self.key_ref
    }

    pub fn duplicate(&self) -> &str {
        &self.duplicate
    }

    pub fn same_key_different_request(&self) -> &str {
        &self.same_key_different_request
    }

    pub fn payload_binding(&self) -> &str {
        &self.payload_binding
    }
}

/// One capability requirement over explicit operation refs.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct CapabilityRequirement {
    pub(crate) requirement_id: CompositionId,
    pub(crate) capability: String,
    pub(crate) minimum: String,
    pub(crate) reason: String,
    pub(crate) operation_refs: Vec<String>,
}

impl CapabilityRequirement {
    pub fn requirement_id(&self) -> &str {
        self.requirement_id.as_str()
    }

    pub fn capability(&self) -> &str {
        &self.capability
    }

    pub fn minimum(&self) -> &str {
        &self.minimum
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn operation_refs(&self) -> &[String] {
        &self.operation_refs
    }
}

/// The validated composition attachment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Composition {
    pub(crate) project_id: String,
    pub(crate) model_digest: String,
    pub(crate) ir_digest: String,
    pub(crate) consumed: Vec<ConsumedRef>,
    pub(crate) operations: Vec<OperationRole>,
    pub(crate) independent_effects: Vec<IndependenceRule>,
    pub(crate) correlations: Vec<Correlation>,
    pub(crate) approvals: Vec<ApprovalBinding>,
    pub(crate) snapshots: Vec<SnapshotDecl>,
    pub(crate) public_allowlist: PublicAllowlist,
    pub(crate) replay_scopes: Vec<ReplayScope>,
    pub(crate) capability_requirements: Vec<CapabilityRequirement>,
}

impl Composition {
    /// Assemble one composition from canonically ordered parts,
    /// enforcing the closed bounds (the wire layer guarantees closed
    /// vocabularies). Duplicate roles, unsorted collections, or a
    /// bound overflow refuse with the typed diagnostic set.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        project_id: &str,
        model_digest: String,
        ir_digest: String,
        consumed: Vec<ConsumedRef>,
        operations: Vec<OperationRole>,
        independent_effects: Vec<IndependenceRule>,
        correlations: Vec<Correlation>,
        approvals: Vec<ApprovalBinding>,
        snapshots: Vec<SnapshotDecl>,
        public_allowlist: PublicAllowlist,
        replay_scopes: Vec<ReplayScope>,
        capability_requirements: Vec<CapabilityRequirement>,
    ) -> Result<Self, DiagnosticSet> {
        if project_id.is_empty() || project_id.len() > 64 {
            return Err(super::diagnostic::input_invalid("project-id"));
        }
        if project_id
            .bytes()
            .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'))
        {
            return Err(super::diagnostic::input_invalid("project-id"));
        }
        if operations.len() < super::version::OPERATION_ROLES.len()
            || operations.len() > super::version::MAX_OPERATIONS
        {
            return Err(super::diagnostic::input_invalid("operations-bound"));
        }
        for pair in operations.windows(2) {
            if pair[0].operation >= pair[1].operation {
                return Err(super::diagnostic::input_invalid("duplicate-role"));
            }
        }
        let mut effects_seen = std::collections::BTreeSet::new();
        for role in &operations {
            for effect in &role.effects {
                if !effects_seen.insert((role.operation.as_str(), effect.effect_id())) {
                    return Err(super::diagnostic::input_invalid("duplicate-effect"));
                }
            }
        }
        if independent_effects.is_empty()
            || independent_effects.len() > super::version::MAX_INDEPENDENCE_RULES
        {
            return Err(super::diagnostic::input_invalid("independence-bound"));
        }
        for pair in independent_effects.windows(2) {
            if pair[0].operation >= pair[1].operation {
                return Err(super::diagnostic::input_invalid(
                    "duplicate-independence-rule",
                ));
            }
        }
        for pair in replay_scopes.windows(2) {
            if pair[0].operation >= pair[1].operation {
                return Err(super::diagnostic::input_invalid("duplicate-replay-scope"));
            }
        }
        // Replay scopes are operation-unique and their scopes
        // operation-scoped: two operations may never share one scope
        // identity, and one operation may never appear twice.
        let mut scopes_seen = std::collections::BTreeSet::new();
        for scope in &replay_scopes {
            if !scopes_seen.insert(scope.scope().to_owned()) {
                return Err(super::diagnostic::input_invalid("replay-scope-collision"));
            }
        }
        Ok(Self {
            project_id: project_id.to_owned(),
            model_digest,
            ir_digest,
            consumed,
            operations,
            independent_effects,
            correlations,
            approvals,
            snapshots,
            public_allowlist,
            replay_scopes,
            capability_requirements,
        })
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn model_digest(&self) -> &str {
        &self.model_digest
    }

    pub fn ir_digest(&self) -> &str {
        &self.ir_digest
    }

    pub fn consumed(&self) -> &[ConsumedRef] {
        &self.consumed
    }

    pub fn operations(&self) -> &[OperationRole] {
        &self.operations
    }

    pub fn independent_effects(&self) -> &[IndependenceRule] {
        &self.independent_effects
    }

    pub fn correlations(&self) -> &[Correlation] {
        &self.correlations
    }

    pub fn approvals(&self) -> &[ApprovalBinding] {
        &self.approvals
    }

    pub fn snapshots(&self) -> &[SnapshotDecl] {
        &self.snapshots
    }

    pub fn public_allowlist(&self) -> &PublicAllowlist {
        &self.public_allowlist
    }

    pub fn replay_scopes(&self) -> &[ReplayScope] {
        &self.replay_scopes
    }

    pub fn capability_requirements(&self) -> &[CapabilityRequirement] {
        &self.capability_requirements
    }

    /// The operation role of one id, if declared.
    pub fn role(&self, operation: &str) -> Option<&OperationRole> {
        self.operations
            .iter()
            .find(|role| role.operation == operation)
    }
}
