//! Traceability records of the invariant-transition attachment (issue
//! #63): verification mappings, declared enforcement layers, typed
//! evidence, scenario references, and property hints.
//!
//! Mapping status and evidence status stay distinct: `full` and
//! `verified` require current bound evidence; `gap` means none;
//! `dangling`, `stale`, `conflict`, `unsupported`, and `unknown` are
//! explicit degraded states. Runtime values, lock tokens, ETags, user
//! data, source snippets, logs, stacks, transcripts, timestamps, and
//! host data never enter these records — evidence is carried as opaque
//! digests over owner-held artifacts only.

use crate::scenario::id::{NamespacedId, SemanticId};

use crate::lockfile::types::Sha256Digest;

/// One pinned scenario reference in the accepted #23 grammar.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ScenarioRef {
    pub(crate) scenario_id: String,
    pub(crate) scenario_version: String,
    pub(crate) ir_digest: Sha256Digest,
}

impl ScenarioRef {
    /// The scenario identifier.
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    /// The pinned scenario version.
    pub fn scenario_version(&self) -> &str {
        &self.scenario_version
    }

    /// The pinned Scenario IR digest.
    pub fn ir_digest(&self) -> &Sha256Digest {
        &self.ir_digest
    }
}

/// The closed subject kind of a mapping or hint.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SubjectKind {
    /// The subject is an invariant.
    Invariant,
    /// The subject is a transition.
    Transition,
    /// The subject is a state space (hints only).
    StateSpace,
}

impl SubjectKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Invariant => "invariant",
            Self::Transition => "transition",
            Self::StateSpace => "state_space",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "invariant" => Some(Self::Invariant),
            "transition" => Some(Self::Transition),
            "state_space" => Some(Self::StateSpace),
            _ => None,
        }
    }
}

/// One typed subject pointer.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Subject {
    pub(crate) kind: SubjectKind,
    pub(crate) id: SemanticId,
}

impl Subject {
    /// The closed subject kind.
    pub const fn kind(&self) -> SubjectKind {
        self.kind
    }

    /// The subject identifier.
    pub fn id(&self) -> &SemanticId {
        &self.id
    }
}

/// The closed mapping relation vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum MappingRelation {
    /// The subject is verified by the target.
    VerifiedBy,
    /// The subject is tested by the target.
    TestedBy,
    /// The subject is enforced by the target.
    EnforcedBy,
    /// The invariant constrains the target.
    Constrains,
    /// The transition changes the state of the target.
    ChangesState,
    /// The transition requires the command.
    RequiresCommand,
    /// The subject requires the capability.
    RequiresCapability,
}

impl MappingRelation {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::VerifiedBy => "verified_by",
            Self::TestedBy => "tested_by",
            Self::EnforcedBy => "enforced_by",
            Self::Constrains => "constrains",
            Self::ChangesState => "changes_state",
            Self::RequiresCommand => "requires_command",
            Self::RequiresCapability => "requires_capability",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "verified_by" => Some(Self::VerifiedBy),
            "tested_by" => Some(Self::TestedBy),
            "enforced_by" => Some(Self::EnforcedBy),
            "constrains" => Some(Self::Constrains),
            "changes_state" => Some(Self::ChangesState),
            "requires_command" => Some(Self::RequiresCommand),
            "requires_capability" => Some(Self::RequiresCapability),
            _ => None,
        }
    }
}

/// The closed declared enforcement layers. The attachment records the
/// declared layer only; it never claims enforcement from a
/// declaration, method name, SQL constraint name, adapter capability
/// name, test existence, or equal product version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EnforcementLayer {
    /// A validator layer.
    Validator,
    /// A runtime method layer.
    RuntimeMethod,
    /// A database constraint layer.
    DatabaseConstraint,
    /// A storage layer.
    Storage,
    /// A native test layer.
    NativeTest,
    /// An adapter layer.
    Adapter,
    /// The layer is unknown.
    Unknown,
}

impl EnforcementLayer {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Validator => "validator",
            Self::RuntimeMethod => "runtime_method",
            Self::DatabaseConstraint => "database_constraint",
            Self::Storage => "storage",
            Self::NativeTest => "native_test",
            Self::Adapter => "adapter",
            Self::Unknown => "unknown",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "validator" => Some(Self::Validator),
            "runtime_method" => Some(Self::RuntimeMethod),
            "database_constraint" => Some(Self::DatabaseConstraint),
            "storage" => Some(Self::Storage),
            "native_test" => Some(Self::NativeTest),
            "adapter" => Some(Self::Adapter),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// The closed target kinds of one mapping target.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TargetKind {
    /// A validator target.
    Validator,
    /// A runtime method target.
    RuntimeMethod,
    /// A database constraint target.
    DatabaseConstraint,
    /// A storage target.
    Storage,
    /// A native test target.
    NativeTest,
    /// An adapter target.
    Adapter,
    /// A scenario target.
    Scenario,
    /// A requirement target.
    Requirement,
    /// The target kind is unknown.
    Unknown,
}

impl TargetKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Validator => "validator",
            Self::RuntimeMethod => "runtime_method",
            Self::DatabaseConstraint => "database_constraint",
            Self::Storage => "storage",
            Self::NativeTest => "native_test",
            Self::Adapter => "adapter",
            Self::Scenario => "scenario",
            Self::Requirement => "requirement",
            Self::Unknown => "unknown",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "validator" => Some(Self::Validator),
            "runtime_method" => Some(Self::RuntimeMethod),
            "database_constraint" => Some(Self::DatabaseConstraint),
            "storage" => Some(Self::Storage),
            "native_test" => Some(Self::NativeTest),
            "adapter" => Some(Self::Adapter),
            "scenario" => Some(Self::Scenario),
            "requirement" => Some(Self::Requirement),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// One typed target pointer: closed kind plus a bounded opaque
/// identifier in the owner's namespace. Never a file path, URL,
/// method signature, or SQL text.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TargetRef {
    pub(crate) target_kind: TargetKind,
    pub(crate) target_id: String,
}

impl TargetRef {
    /// The closed target kind.
    pub const fn target_kind(&self) -> TargetKind {
        self.target_kind
    }

    /// The bounded opaque target identifier.
    pub fn target_id(&self) -> &str {
        &self.target_id
    }
}

/// The sanitized execution result of one evidence record. A pass here
/// is evidence, never an enforcement claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EvidenceStatus {
    /// The evidence run passed.
    Pass,
    /// The evidence run failed.
    Fail,
    /// The evidence run was skipped.
    Skipped,
    /// The evidence result is unknown.
    Unknown,
}

impl EvidenceStatus {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Skipped => "skipped",
            Self::Unknown => "unknown",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "pass" => Some(Self::Pass),
            "fail" => Some(Self::Fail),
            "skipped" => Some(Self::Skipped),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// One typed evidence record: target/profile/adapter/protocol
/// identity, the exact executable digest, the bound source revision, a
/// scenario/test/gate reference, the sanitized result status, and the
/// opaque evidence digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Evidence {
    pub(crate) profile_ref: Option<NamespacedId>,
    pub(crate) adapter_ref: Option<NamespacedId>,
    pub(crate) protocol_ref: Option<NamespacedId>,
    pub(crate) executable_digest: Sha256Digest,
    pub(crate) source_revision: String,
    pub(crate) scenario_ref: Option<ScenarioRef>,
    pub(crate) test_ref: Option<NamespacedId>,
    pub(crate) gate_ref: Option<NamespacedId>,
    pub(crate) result_status: EvidenceStatus,
    pub(crate) evidence_digest: Sha256Digest,
}

impl Evidence {
    /// The bound target profile, when declared.
    pub fn profile_ref(&self) -> Option<&NamespacedId> {
        self.profile_ref.as_ref()
    }

    /// The bound adapter, when declared.
    pub fn adapter_ref(&self) -> Option<&NamespacedId> {
        self.adapter_ref.as_ref()
    }

    /// The bound protocol, when declared.
    pub fn protocol_ref(&self) -> Option<&NamespacedId> {
        self.protocol_ref.as_ref()
    }

    /// The exact executable digest.
    pub fn executable_digest(&self) -> &Sha256Digest {
        &self.executable_digest
    }

    /// The bound source revision.
    pub fn source_revision(&self) -> &str {
        &self.source_revision
    }

    /// The pinned scenario, when declared.
    pub fn scenario_ref(&self) -> Option<&ScenarioRef> {
        self.scenario_ref.as_ref()
    }

    /// The pinned test, when declared.
    pub fn test_ref(&self) -> Option<&NamespacedId> {
        self.test_ref.as_ref()
    }

    /// The pinned gate, when declared.
    pub fn gate_ref(&self) -> Option<&NamespacedId> {
        self.gate_ref.as_ref()
    }

    /// The sanitized result status.
    pub const fn result_status(&self) -> EvidenceStatus {
        self.result_status
    }

    /// The opaque evidence digest.
    pub fn evidence_digest(&self) -> &Sha256Digest {
        &self.evidence_digest
    }
}

/// The closed mapping status vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum MappingStatus {
    /// Full coverage with current bound evidence.
    Full,
    /// Verified with current bound evidence.
    Verified,
    /// Limited coverage.
    Partial,
    /// No evidence.
    Gap,
    /// The target reference is absent.
    Dangling,
    /// The bound revision mismatches.
    Stale,
    /// Contradictory evidence.
    Conflict,
    /// The capability is unavailable.
    Unsupported,
    /// Not evaluated.
    Unknown,
}

impl MappingStatus {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Verified => "verified",
            Self::Partial => "partial",
            Self::Gap => "gap",
            Self::Dangling => "dangling",
            Self::Stale => "stale",
            Self::Conflict => "conflict",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "full" => Some(Self::Full),
            "verified" => Some(Self::Verified),
            "partial" => Some(Self::Partial),
            "gap" => Some(Self::Gap),
            "dangling" => Some(Self::Dangling),
            "stale" => Some(Self::Stale),
            "conflict" => Some(Self::Conflict),
            "unsupported" => Some(Self::Unsupported),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// The closed mapping confidence vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum MappingConfidence {
    /// Low confidence.
    Low,
    /// Medium confidence.
    Medium,
    /// High confidence.
    High,
}

impl MappingConfidence {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// One verification mapping record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationMapping {
    pub(crate) mapping_id: SemanticId,
    pub(crate) subject: Subject,
    pub(crate) relation: MappingRelation,
    pub(crate) target_ref: TargetRef,
    pub(crate) enforcement_layer: EnforcementLayer,
    pub(crate) evidence: Option<Evidence>,
    pub(crate) status: MappingStatus,
    pub(crate) confidence: MappingConfidence,
}

impl VerificationMapping {
    /// The stable mapping identifier.
    pub fn mapping_id(&self) -> &SemanticId {
        &self.mapping_id
    }

    /// The mapped subject.
    pub const fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The closed relation.
    pub const fn relation(&self) -> MappingRelation {
        self.relation
    }

    /// The typed target reference.
    pub const fn target_ref(&self) -> &TargetRef {
        &self.target_ref
    }

    /// The declared enforcement layer.
    pub const fn enforcement_layer(&self) -> EnforcementLayer {
        self.enforcement_layer
    }

    /// The typed evidence, when bound.
    pub fn evidence(&self) -> Option<&Evidence> {
        self.evidence.as_ref()
    }

    /// The closed mapping status.
    pub const fn status(&self) -> MappingStatus {
        self.status
    }

    /// The closed confidence.
    pub const fn confidence(&self) -> MappingConfidence {
        self.confidence
    }
}

/// The closed property hint vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PropertyKind {
    /// Uniqueness under arbitrary member order.
    Uniqueness,
    /// At most one active member per partition.
    OneActive,
    /// Transitions never leave the declared graph.
    TransitionConsistency,
    /// Temporal fields stay ordered.
    TemporalOrdering,
    /// Collection sizes stay in bounds.
    CardinalityBounds,
    /// Fields stay immutable after the trigger state.
    ImmutableAfterState,
}

impl PropertyKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Uniqueness => "uniqueness",
            Self::OneActive => "one_active",
            Self::TransitionConsistency => "transition_consistency",
            Self::TemporalOrdering => "temporal_ordering",
            Self::CardinalityBounds => "cardinality_bounds",
            Self::ImmutableAfterState => "immutable_after_state",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "uniqueness" => Some(Self::Uniqueness),
            "one_active" => Some(Self::OneActive),
            "transition_consistency" => Some(Self::TransitionConsistency),
            "temporal_ordering" => Some(Self::TemporalOrdering),
            "cardinality_bounds" => Some(Self::CardinalityBounds),
            "immutable_after_state" => Some(Self::ImmutableAfterState),
            _ => None,
        }
    }
}

/// One bounded property-based test hint: a pure pointer for the
/// scenario and reference-evaluation owners; never executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyHint {
    pub(crate) hint_id: SemanticId,
    pub(crate) subject: Subject,
    pub(crate) property: PropertyKind,
    pub(crate) description: Option<String>,
}

impl PropertyHint {
    /// The stable hint identifier.
    pub fn hint_id(&self) -> &SemanticId {
        &self.hint_id
    }

    /// The hinted subject.
    pub const fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The closed property kind.
    pub const fn property(&self) -> PropertyKind {
        self.property
    }

    /// The bounded description, when declared.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}
