//! Issue #119: the privacy/export enforcement runtime.
//!
//! This module is the typed, in-core projection of the frozen #120
//! contract family `dev.lekalo.privacy-export-policy@0.3.2` (see
//! `docs/privacy.md`). It owns three things and nothing more:
//!
//! 1. The frozen contract references and their pinned SHA-256 digests
//!    ([`refs`]), embedded from `contracts/` at compile time and
//!    custody-verified at load ([`context::TrustedContext`]) exactly
//!    like the reference checker does.
//! 2. The typed decision input ([`input::ExportDecisionInput`]) with
//!    the exact closed member sets of
//!    `contracts/privacy-export.schema.v0.3.2.json` and the typed
//!    decision output ([`output::ExportDecisionOutput`]) with the
//!    exact closed member set of the frozen output schema. Unknown
//!    vocabulary values are unrepresentable; construction is
//!    deny-unknown by type.
//! 3. The closed vocabulary enums ([`vocab`]) with the exact wire
//!    spellings of the pinned policy.
//!
//! The module is metadata-only by construction: no value, secret,
//! source text, raw prompt/response, or absolute host path ever
//! crosses this surface. Everything is deterministic; nothing here
//! performs filesystem, network, or process access at decision time.

pub mod canonical;
pub mod context;
pub mod evaluate;
pub mod export;
pub mod input;
pub mod output;
pub mod redact;
pub mod refs;
pub mod types;
pub mod vocab;

pub use canonical::{canonical, compare_unicode_code_points};
pub use context::{CustodyError, TrustedContext};
pub use evaluate::{evaluate_decision, DecisionEvaluation};
pub use input::{
    AggregationDecision, AppliedTransform, ConflictResolution, Constraint,
    DeclassificationDecision, DerivedArtifact, Destination, Endpoint, ExportDecisionInput,
    Operation, Provenance, SourceArtifact,
};
pub use output::{
    EffectiveRefs, ExportDecision, ExportDecisionOutput, DECISION_VERSION, TRANSFORM_REQUIREMENTS,
};
pub use refs::FrozenRefs;
pub use types::{
    AuditRef, AuthorityRef, AuthorizingEvidence, ClassificationContractRef,
    ClassificationDecisionRef, ContractRef, DecisionContractRef, EvidenceBinding,
    EvidenceContractRef, EvidenceOutcome, EvidenceSpec, FreshnessState, PolicyRef, ResourcePath,
    SchemaRef, SubjectProfileRef, ValueState, VerificationState,
};
pub use vocab::{
    Audience, ConflictState, ConstraintScope, DataSensitivity, ExportDisposition, OperationId,
    ProvenanceOrigin, RepositoryRelation, RepositoryRole, SensitivityState, TenantRelation,
    TransformId, TrustBoundary,
};
