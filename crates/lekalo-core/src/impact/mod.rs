//! Issue #16: deterministic impact and change-radius analysis.
//!
//! A read-only, typed projection over the accepted #13 dependency graph
//! and #14 effect graph: the direct and transitive radius of a semantic
//! symbol or a typed changed-input set, the mandatory-public closure that
//! depth limits can never hide, the closed risk vector, the affected
//! target/scenario/test/gate projections, bounded explanation paths, and
//! explicit evidence, completeness, and confidence. Everything derives
//! from canonical data; nothing scans source, parses Git inside the core,
//! infers effects, invokes adapters, or writes anywhere.
//!
//! Determinism: results are canonical compact UTF-8 JSON whose object keys
//! follow the schema order, set-like arrays sort by unsigned UTF-8 of
//! their typed keys, and the digest is the SHA-256 of the canonical bytes
//! with the digest field empty. The same inputs always produce
//! byte-identical output on every platform.

pub mod analyzer;
pub mod cache;
pub mod canonical;
pub mod diagnostic;
pub mod evidence;
pub mod gate;
pub mod input;
pub mod path;
pub mod request;
pub mod risk;
pub mod version;

use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::graph::model::Confidence;
use crate::graph::NodeId;
use crate::loader::ModelVersion;
pub use analyzer::{analyze, ImpactFailure};

pub use input::{
    ChangedInput, ChangedInputSet, ChangedMode, EntryEvidence, FileChange, MemberChange, MemberSeed,
};
pub use request::{ImpactProfile, ImpactRequest};
pub use version::{
    ALGORITHM, DEFAULT_DEPTH, FAMILY, IDENTITY, MAX_DEPTH, MAX_ENTRIES, MAX_EXPORT_BYTES,
    MAX_FILTER_TERMS, MAX_ITEMS, MAX_PATH_EDGES, MAX_PROVENANCE_RECORDS, MAX_ROOTS, SCHEMA_VERSION,
    VERSION,
};

/// The closed evidence state of one result element.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum EvidenceState {
    Canonical,
    Verified,
    Extracted,
    Inferred,
    Stale,
    Unknown,
}

impl EvidenceState {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Verified => "verified",
            Self::Extracted => "extracted",
            Self::Inferred => "inferred",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }
}

/// The closed section state vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum SectionState {
    Complete,
    Incomplete,
    Stale,
    Unknown,
    Unsupported,
    Conflicting,
}

impl SectionState {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incomplete => "incomplete",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
            Self::Unsupported => "unsupported",
            Self::Conflicting => "conflicting",
        }
    }
}

/// The closed impact scope of one affected item.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Scope {
    Direct,
    Transitive,
    MandatoryPublic,
}

impl Scope {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Transitive => "transitive",
            Self::MandatoryPublic => "mandatory-public",
        }
    }
}

/// The closed risk dimensions; independent, never one lossy severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum RiskDimension {
    PublicContract,
    MigrationData,
    Transaction,
    Authorization,
    Portability,
    Effects,
    BindingsArtifacts,
    ScenariosTests,
}

impl RiskDimension {
    /// Every dimension in canonical wire order.
    pub const ALL: [Self; 8] = [
        Self::PublicContract,
        Self::MigrationData,
        Self::Transaction,
        Self::Authorization,
        Self::Portability,
        Self::Effects,
        Self::BindingsArtifacts,
        Self::ScenariosTests,
    ];

    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::PublicContract => "public_contract",
            Self::MigrationData => "migration_data",
            Self::Transaction => "transaction",
            Self::Authorization => "authorization",
            Self::Portability => "portability",
            Self::Effects => "effects",
            Self::BindingsArtifacts => "bindings_artifacts",
            Self::ScenariosTests => "scenarios_tests",
        }
    }
}

/// The closed gate state vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum GateState {
    Selected,
    NotRequired,
    Unknown,
    Blocked,
}

impl GateState {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::NotRequired => "not-required",
            Self::Unknown => "unknown",
            Self::Blocked => "blocked",
        }
    }
}

/// The closed provenance-kind vocabulary of sections and paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ProvenanceKind {
    CanonicalIr,
    Evidence,
    Derived,
}

impl ProvenanceKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::CanonicalIr => "canonical-ir",
            Self::Evidence => "evidence",
            Self::Derived => "derived",
        }
    }
}

/// The closed evidence-surface vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum SurfaceKind {
    Graph,
    Effects,
    ChangedInputs,
    DetectedEvidence,
    Manifests,
    Scenarios,
    Tests,
}

impl SurfaceKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Graph => "graph",
            Self::Effects => "effects",
            Self::ChangedInputs => "changed-inputs",
            Self::DetectedEvidence => "detected-evidence",
            Self::Manifests => "manifests",
            Self::Scenarios => "scenarios",
            Self::Tests => "tests",
        }
    }
}

/// The closed input-mode vocabulary of one result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum InputMode {
    Symbol,
    Committed,
    Index,
    Worktree,
    Mixed,
}

impl InputMode {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::Committed => "committed",
            Self::Index => "index",
            Self::Worktree => "worktree",
            Self::Mixed => "mixed",
        }
    }
}

/// The shared bounded summary every result section carries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SectionSummary {
    pub state: SectionState,
    pub complete: bool,
    pub returned: usize,
    pub omitted: usize,
    pub frontier: usize,
    pub reason_refs: Vec<String>,
    pub provenance: Vec<ProvenanceKind>,
    pub confidence: Confidence,
}

impl SectionSummary {
    /// The summary of an empty, fully observed section.
    pub fn empty_complete() -> Self {
        Self {
            state: SectionState::Complete,
            complete: true,
            returned: 0,
            omitted: 0,
            frontier: 0,
            reason_refs: Vec::new(),
            provenance: vec![ProvenanceKind::CanonicalIr],
            confidence: Confidence::Canonical,
        }
    }
}

/// One affected item: subject, scope, distance, and its explanations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactItem {
    pub subject: NodeId,
    pub scope: Scope,
    pub distance: usize,
    pub reason_refs: Vec<String>,
    pub path_refs: Vec<String>,
    pub evidence_state: EvidenceState,
    pub confidence: Confidence,
    pub risk_refs: Vec<RiskDimension>,
    pub required_gate_refs: Vec<String>,
}

/// The direct, transitive, and mandatory-public radius sections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactSection {
    pub summary: SectionSummary,
    pub items: Vec<ImpactItem>,
}

/// One closed-dimension risk fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskItem {
    pub dimension: RiskDimension,
    pub state: SectionState,
    pub subject_refs: Vec<NodeId>,
    pub reason_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub required: bool,
    pub confidence: Confidence,
}

/// The risk vector section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskSection {
    pub summary: SectionSummary,
    pub items: Vec<RiskItem>,
}

/// One affected target binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetItem {
    pub binding: NodeId,
    pub target: String,
    pub reason_refs: Vec<String>,
    pub confidence: Confidence,
}

/// The affected-target section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetSection {
    pub summary: SectionSummary,
    pub items: Vec<TargetItem>,
}

/// One affected named surface element (artifact, scenario, test).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedItem {
    pub id: NodeId,
    pub state: EvidenceState,
    pub reason_refs: Vec<String>,
    pub confidence: Confidence,
}

/// A named-surface section (artifacts, scenarios, tests).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedSection {
    pub summary: SectionSummary,
    pub items: Vec<NamedItem>,
}

/// One neutral targeted-gate selection fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateItem {
    pub gate_id: String,
    pub owner: &'static str,
    pub reason_refs: Vec<String>,
    pub required: bool,
    pub state: GateState,
    pub evidence_state: EvidenceState,
    pub confidence: Confidence,
}

/// The gate-selection section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateSection {
    pub summary: SectionSummary,
    pub items: Vec<GateItem>,
}

/// One bounded explanation path from a root to an affected subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplanationPath {
    pub path_id: String,
    pub root: NodeId,
    pub subject: NodeId,
    pub ordered_edges: Vec<String>,
    pub relation_kinds: Vec<crate::graph::RelationKindId>,
    pub confidence: Confidence,
    pub provenance_refs: Vec<ProvenanceKind>,
}

/// One evidence-surface fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceSurface {
    pub surface: SurfaceKind,
    pub state: SectionState,
    pub reason_refs: Vec<String>,
    pub confidence: Confidence,
}

/// The evidence section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceSection {
    pub summary: SectionSummary,
    pub items: Vec<EvidenceSurface>,
}

/// One finished impact result: immutable, canonical, digest-carrying.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactResult {
    pub(crate) project: Option<String>,
    pub(crate) model_version: ModelVersion,
    pub(crate) input_mode: InputMode,
    pub(crate) base_revision_ref: Option<String>,
    pub(crate) candidate_revision_ref: Option<String>,
    pub(crate) changed_input_digest: String,
    pub(crate) request: ImpactRequest,
    pub(crate) roots: Vec<NodeId>,
    pub(crate) direct: ImpactSection,
    pub(crate) transitive: ImpactSection,
    pub(crate) mandatory_public: ImpactSection,
    pub(crate) risks: RiskSection,
    pub(crate) targets: TargetSection,
    pub(crate) artifacts: NamedSection,
    pub(crate) scenarios: NamedSection,
    pub(crate) tests: NamedSection,
    pub(crate) gates: GateSection,
    pub(crate) explanations: Vec<ExplanationPath>,
    pub(crate) evidence: EvidenceSection,
    pub(crate) completeness: SectionSummary,
    pub(crate) diagnostic_refs: Vec<String>,
    pub(crate) warnings: Vec<Diagnostic>,
    pub(crate) digest: String,
}

impl ImpactResult {
    /// The project semantic id, when the source declared one.
    pub fn project(&self) -> Option<&str> {
        self.project.as_deref()
    }

    /// The exact source Model version the analysis ran against.
    pub const fn model_version(&self) -> ModelVersion {
        self.model_version
    }

    /// The canonical digest of the semantic envelope.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The request the result answers.
    pub fn request(&self) -> &ImpactRequest {
        &self.request
    }

    /// The resolved roots (kind-qualified), in canonical order.
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// The direct radius.
    pub fn direct(&self) -> &ImpactSection {
        &self.direct
    }

    /// The transitive radius.
    pub fn transitive(&self) -> &ImpactSection {
        &self.transitive
    }

    /// The mandatory-public closure.
    pub fn mandatory_public(&self) -> &ImpactSection {
        &self.mandatory_public
    }

    /// The risk vector.
    pub fn risks(&self) -> &RiskSection {
        &self.risks
    }

    /// The affected target bindings.
    pub fn targets(&self) -> &TargetSection {
        &self.targets
    }

    /// The artifacts projection.
    pub fn artifacts(&self) -> &NamedSection {
        &self.artifacts
    }

    /// The scenarios projection.
    pub fn scenarios(&self) -> &NamedSection {
        &self.scenarios
    }

    /// The tests projection.
    pub fn tests(&self) -> &NamedSection {
        &self.tests
    }

    /// The targeted-gate selection.
    pub fn gates(&self) -> &GateSection {
        &self.gates
    }

    /// The bounded explanation paths.
    pub fn explanations(&self) -> &[ExplanationPath] {
        &self.explanations
    }

    /// The evidence section.
    pub fn evidence(&self) -> &EvidenceSection {
        &self.evidence
    }

    /// The top-level completeness.
    pub fn completeness(&self) -> &SectionSummary {
        &self.completeness
    }

    /// The closed input mode of the analysis.
    pub const fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    /// The opaque changed-input digest (the empty-set digest in symbol
    /// mode).
    pub fn changed_input_digest(&self) -> &str {
        &self.changed_input_digest
    }

    /// The opaque base revision reference, when the mode carries one.
    pub fn base_revision_ref(&self) -> Option<&str> {
        self.base_revision_ref.as_deref()
    }

    /// The opaque candidate revision reference, when the mode carries one.
    pub fn candidate_revision_ref(&self) -> Option<&str> {
        self.candidate_revision_ref.as_deref()
    }

    /// The sorted rule ids of the non-blocking diagnostics.
    pub fn diagnostic_refs(&self) -> &[String] {
        &self.diagnostic_refs
    }

    /// The non-blocking diagnostics the analysis attached (also rendered in
    /// the CLI envelope); the payload carries only their ids.
    pub fn warnings(&self) -> &[Diagnostic] {
        &self.warnings
    }

    /// The canonical compact JSON bytes of the semantic envelope; the
    /// trailing newline stays with the CLI. Rejects beyond the export
    /// bound instead of truncating.
    pub fn to_canonical_json(&self) -> Result<String, DiagnosticSet> {
        canonical::impact_bytes(self)
    }

    /// The stable cache fingerprint: the exact typed inputs a cache owner
    /// may key on. Purely derived; nothing is persisted here.
    pub fn fingerprint(&self) -> String {
        cache::fingerprint(self)
    }
}
