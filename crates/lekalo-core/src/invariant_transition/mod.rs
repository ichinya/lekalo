//! Issue #63: the closed, versioned invariant-transition attachment.
//!
//! One independent, immutable contract family declaring the first-
//! class domain invariants and state transitions of a project:
//! bounded state spaces with explicit initial/terminal states and
//! explicit cycle and dead-state policies, eleven closed invariant
//! kinds over a minimal closed predicate/value AST, typed transitions
//! with ordered assignments, preconditions, authorization policy
//! references, and branch error references, verification mappings with
//! declared enforcement layers and typed evidence, and bounded
//! property hints.
//!
//! Boundaries: this module is pure declaration and validation — no
//! runtime enforcement, no adapter generation, no SQL or method
//! enforcement, no transaction semantics (#24), no authorization
//! execution (#25), no scenario execution (#23), no reference
//! evaluation (#107), and no report, trace-manifest, or gate surface.
//! Error identity stays with #62 (only opaque typed refs are carried);
//! capability registries and profile resolution stay with #27/#28/#29;
//! semantic-diff integration stays with #18 (this module contributes
//! the pure comparison only).
//!
//! Determinism and denial: canonical bytes are compact UTF-8 JSON with
//! byte-sorted keys; set-like collections normalize to sorted form
//! while assignment sets keep their declared behavioral order. Every
//! bound and every semantic contradiction rejects with an explicit
//! registered diagnostic and no partial result.

pub mod canonical;
pub mod capability;
mod diagnostic;
pub mod diff;
pub mod expr;
pub mod graph;
pub mod id;
pub mod invariant;
pub mod state;
pub mod trace;
pub mod transition;
mod validate;
pub(crate) mod version;
pub(crate) mod wire;

pub use capability::{CapabilityRequirement, RequirementLevel};
pub use diff::{compare, DiffClass, DiffPath, DiffResult};
pub use expr::PredicateNode;
pub use id::{ExpressionRef, StateRef};
pub use invariant::{FieldRef, Invariant, InvariantKind};
pub use state::{
    CyclePolicy, DeadPolicy, Duration, DurationUnit, StateSpace, StateState, ValueNode,
};
pub use trace::{
    EnforcementLayer, Evidence, EvidenceStatus, MappingConfidence, MappingRelation, MappingStatus,
    PropertyHint, PropertyKind, ScenarioRef, Subject, SubjectKind, TargetKind, TargetRef,
    VerificationMapping,
};
pub use transition::{Assignment, AssignmentValue, Transition};
pub use version::{FAMILY, IDENTITY, SCHEMA_VERSION, VERSION};

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::SemanticId;

/// The bound source Model contract: exact accepted version plus digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelPin {
    /// The accepted Model contract version.
    pub(crate) version: crate::scenario::ModelPin,
    /// The digest of the exact source Model document.
    pub(crate) digest: Sha256Digest,
}

impl ModelPin {
    /// The accepted Model contract version.
    pub const fn version(&self) -> &crate::scenario::ModelPin {
        &self.version
    }

    /// The digest of the exact source Model document.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// One finished invariant-transition attachment: immutable,
/// deterministically ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvariantTransitionAttachment {
    attachment_revision: SemVer,
    project_id: SemanticId,
    model_ref: ModelPin,
    ir_digest: Sha256Digest,
    source_map_ref: Option<crate::scenario::id::NamespacedId>,
    state_spaces: Vec<StateSpace>,
    invariants: Vec<Invariant>,
    transitions: Vec<Transition>,
    verification_mappings: Vec<VerificationMapping>,
    property_hints: Vec<PropertyHint>,
}

impl InvariantTransitionAttachment {
    /// Assemble from validated parts (crate internal); collections are
    /// stored in the caller's normalized order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        attachment_revision: SemVer,
        project_id: SemanticId,
        model_ref: ModelPin,
        ir_digest: Sha256Digest,
        source_map_ref: Option<crate::scenario::id::NamespacedId>,
        state_spaces: Vec<StateSpace>,
        invariants: Vec<Invariant>,
        transitions: Vec<Transition>,
        verification_mappings: Vec<VerificationMapping>,
        property_hints: Vec<PropertyHint>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_digest,
            source_map_ref,
            state_spaces,
            invariants,
            transitions,
            verification_mappings,
            property_hints,
        }
    }

    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set.
    pub fn from_value(json: &serde_json::Value) -> Result<Self, DiagnosticSet> {
        wire::from_value(json)
    }

    /// The canonical payload bytes (compact JSON, byte-sorted keys,
    /// sorted set-like collections, no trailing LF), or a typed refusal
    /// beyond the payload bound.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        canonical::attachment_bytes(self)
    }

    /// The optional bound source-map reference.
    pub fn source_map_ref(&self) -> Option<&crate::scenario::id::NamespacedId> {
        self.source_map_ref.as_ref()
    }

    /// The internal semantic check (wire normalization and typed
    /// revalidation share one entry).
    pub(crate) fn semantic_self_check(&self) -> Result<(), DiagnosticSet> {
        validate::semantic_self_check(self)
    }

    /// The semantic self-check over this attachment; pure and
    /// read-only.
    pub fn validate_invariants(&self) -> Result<(), DiagnosticSet> {
        self.semantic_self_check()
    }

    /// The exact attachment revision.
    pub fn attachment_revision(&self) -> &SemVer {
        &self.attachment_revision
    }

    /// The stable project identity.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The bound source Model contract.
    pub const fn model_ref(&self) -> &ModelPin {
        &self.model_ref
    }

    /// The digest of the exact canonical IR payload.
    pub fn ir_digest(&self) -> &Sha256Digest {
        &self.ir_digest
    }

    /// Every declared state space.
    pub fn state_spaces(&self) -> &[StateSpace] {
        &self.state_spaces
    }

    /// Every declared invariant.
    pub fn invariants(&self) -> &[Invariant] {
        &self.invariants
    }

    /// Every declared transition.
    pub fn transitions(&self) -> &[Transition] {
        &self.transitions
    }

    /// Every declared verification mapping.
    pub fn verification_mappings(&self) -> &[VerificationMapping] {
        &self.verification_mappings
    }

    /// Every declared property hint.
    pub fn property_hints(&self) -> &[PropertyHint] {
        &self.property_hints
    }
}
