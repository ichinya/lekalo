//! Issue #14: the deterministic effect graph for reads, writes, events,
//! and external calls.
//!
//! A read-only, typed projection beside the accepted #13 dependency
//! graph: the declared effects of the compiled IR (query reads, command
//! CRUD, effect emissions) plus typed detected effects supplied only
//! through evidence envelopes. Every edge carries a closed kind with its
//! action, a typed subject (resource plus optional exact field), a closed
//! provenance with non-numeric confidence, and optional descriptive
//! transaction-group and sensitivity references.
//!
//! Boundaries: event/job/external/cache/publish/audit/transaction kinds
//! are DECLARED data or typed evidence records, never detected from
//! source — detection belongs to adapters. Transaction groups are
//! descriptive boundaries; atomicity and isolation semantics are #24.
//! Sensitivity markers are opaque references; policy and gates are #25.
//! Comparison, conflicts, and summaries are derived, rebuildable data —
//! this graph never writes anywhere.
//!
//! Determinism: edges sort by operation, kind, action, resource kind,
//! resource, field, origin, occurrence, then provenance bytes. Exact
//! duplicates collapse only when every machine field matches. Reverse
//! lookups answer from the precomputed index — never a rescan.

pub mod build;
pub mod canonical;
pub mod compare;
pub mod conflict;
pub mod diagnostic;
pub mod edge;
pub mod identity;
pub mod index;
pub mod kind;
pub mod provenance;
pub mod summary;
pub mod version;

use crate::diagnostics::DiagnosticSet;

use crate::loader::ModelVersion;

pub use compare::{Comparison, ComparisonItem, ComparisonSpec, ComparisonState};
pub use conflict::{ChangeSet, ConflictItem, ConflictKind, ConflictReport};
pub use edge::{EffectEdge, EffectKey};
pub use identity::{
    EffectOrigin, FieldName, OperationId, ResourceId, ResourceKind, Sensitivity, Subject,
    TransactionGroupId,
};
pub use kind::{EffectKind, WriteAction};
pub use provenance::{DeclaredRole, EffectProvenance, EvidenceEntry, EvidenceEnvelope, TrustState};
pub use summary::{EffectSummary, OperationSummary, SummarySpec};
pub use version::{
    IDENTITY, MAX_CHANGED_OPERATIONS, MAX_COMPARISON_ITEMS, MAX_CONFLICT_ITEMS, MAX_EFFECTS,
    MAX_ENVELOPES, MAX_ENVELOPE_ENTRIES, MAX_EXPORT_BYTES, MAX_FILTER_TERMS, MAX_RESULT_EDGES,
    MAX_RESULT_ROWS, SCHEMA_VERSION, VERSION,
};

pub use build::build;

/// The confidence vocabulary reused from the accepted #13 graph.
pub use crate::graph::model::Confidence;

/// One finished effect graph: immutable, deterministically ordered, and
/// safe to share across threads. Every query is side-effect free.
pub struct EffectGraph {
    project: Option<String>,
    model_version: ModelVersion,
    ir_digest: String,
    declared: Vec<EffectEdge>,
    detected: Vec<EffectEdge>,
    envelopes: usize,
    index: index::EffectIndex,
}

impl EffectGraph {
    /// Assemble from finished construction parts (crate internal); the
    /// index is built once here.
    pub(crate) fn assemble(
        project: Option<String>,
        model_version: ModelVersion,
        ir_digest: String,
        declared: Vec<EffectEdge>,
        detected: Vec<EffectEdge>,
        envelopes: usize,
    ) -> Self {
        let index = index::EffectIndex::build(&declared, &detected);
        Self {
            project,
            model_version,
            ir_digest,
            declared,
            detected,
            envelopes,
            index,
        }
    }

    /// The effect contract identity (`dev.lekalo.effects@1.0.0`).
    pub const fn identity(&self) -> &'static str {
        IDENTITY
    }

    /// The effect contract discriminator (`lekalo/effects/v1.0.0`).
    pub const fn schema_version(&self) -> &'static str {
        SCHEMA_VERSION
    }

    /// The exact source Model version the IR was compiled from.
    pub const fn model_version(&self) -> ModelVersion {
        self.model_version
    }

    /// The project semantic id, when the source declared one.
    pub fn project_id(&self) -> Option<&str> {
        self.project.as_deref()
    }

    /// The `sha256` digest of the canonical IR the declared projection
    /// was built from.
    pub fn ir_digest(&self) -> &str {
        &self.ir_digest
    }

    /// Every declared edge in canonical order.
    pub fn declared(&self) -> &[EffectEdge] {
        &self.declared
    }

    /// Every detected edge in canonical order.
    pub fn detected(&self) -> &[EffectEdge] {
        &self.detected
    }

    /// How many evidence envelopes contributed detected edges.
    pub const fn envelope_count(&self) -> usize {
        self.envelopes
    }

    /// Whether the graph knows one operation (its declared or detected
    /// sets carry at least one edge).
    pub fn knows_operation(&self, operation: &OperationId) -> bool {
        !self
            .index
            .operation_edges(operation, &self.declared, &self.detected)
            .is_empty()
    }

    /// Every edge of one operation in canonical order.
    pub fn operation_edges(&self, operation: &OperationId) -> Vec<&EffectEdge> {
        self.index
            .operation_edges(operation, &self.declared, &self.detected)
    }

    /// The number of distinct operations in the graph.
    pub fn operation_count(&self) -> usize {
        self.operations().len()
    }

    /// Every operation the graph carries, in canonical order.
    pub fn operations(&self) -> Vec<&OperationId> {
        let mut operations: Vec<&OperationId> = self
            .declared
            .iter()
            .chain(self.detected.iter())
            .map(|edge| edge.key().operation())
            .collect();
        operations.sort();
        operations.dedup();
        operations
    }

    /// Readers of one subject: operations with a direct `read` edge
    /// matching the selector's scope.
    pub fn readers(&self, selector: &SubjectSelector) -> Result<Vec<&EffectEdge>, DiagnosticSet> {
        self.side(selector, false)
    }

    /// Writers of one subject: operations with a direct mutation edge
    /// matching the selector's scope.
    pub fn writers(&self, selector: &SubjectSelector) -> Result<Vec<&EffectEdge>, DiagnosticSet> {
        self.side(selector, true)
    }

    fn side(
        &self,
        selector: &SubjectSelector,
        writes: bool,
    ) -> Result<Vec<&EffectEdge>, DiagnosticSet> {
        let mut edges: Vec<&EffectEdge> = self
            .index
            .resource_edges(
                selector.subject().resource(),
                &self.declared,
                &self.detected,
            )
            .into_iter()
            .filter(|edge| {
                let kind = edge.key().kind();
                if writes {
                    index::is_write(kind)
                } else {
                    index::is_read(kind)
                }
            })
            .filter(|edge| selector.admits(edge.key().subject()))
            .collect();
        if edges.len() > MAX_RESULT_EDGES {
            return Err(diagnostic::traversal_limit_set("result-edge-limit"));
        }
        edges.sort_by_key(|edge| edge.sort_key());
        Ok(edges)
    }

    /// The declared-versus-detected comparison.
    pub fn compare(&self, spec: &ComparisonSpec) -> Result<Comparison, DiagnosticSet> {
        compare::compare(self, spec)
    }

    /// The parallel-change conflict classification over one explicit
    /// change set.
    pub fn conflicts(&self, change_set: &ChangeSet) -> Result<ConflictReport, DiagnosticSet> {
        conflict::conflicts(self, &self.index, change_set)
    }

    /// The compact whole-graph summary.
    pub fn summary(&self, spec: &SummarySpec) -> Result<EffectSummary, DiagnosticSet> {
        summary::summarize(self, spec)
    }

    /// The canonical bytes of the whole graph; rejects beyond the export
    /// bound instead of truncating.
    pub fn to_canonical_json(&self) -> Result<String, DiagnosticSet> {
        canonical::effects_bytes(self)
    }
}

/// The subject selector of the reverse queries: an exact resource, an
/// exact field, or a field with explicitly accepted entity-wide scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubjectSelector {
    subject: Subject,
    accept_entity_wide: bool,
}

impl SubjectSelector {
    /// An entity-level selector: matches only entity-wide effects.
    pub fn entity(resource: ResourceId) -> Self {
        Self {
            subject: Subject::new(resource),
            accept_entity_wide: true,
        }
    }

    /// An exact-field selector: matches only that exact field.
    pub fn exact_field(resource: ResourceId, field: FieldName) -> Self {
        Self {
            subject: Subject::with_field(resource, field),
            accept_entity_wide: false,
        }
    }

    /// An exact-field selector that also accepts entity-wide effects.
    pub fn field_with_entity_scope(resource: ResourceId, field: FieldName) -> Self {
        Self {
            subject: Subject::with_field(resource, field),
            accept_entity_wide: true,
        }
    }

    /// The selected subject.
    pub const fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The wire spelling of the selector scope.
    pub fn scope_key(&self) -> String {
        match (self.subject.field(), self.accept_entity_wide) {
            (Some(_), true) => "field-with-entity-scope".to_owned(),
            (Some(_), false) => "field".to_owned(),
            (None, _) => "entity".to_owned(),
        }
    }

    fn admits(&self, subject: &Subject) -> bool {
        match (self.subject.field(), subject.field()) {
            (Some(wanted), Some(actual)) => wanted == actual,
            (Some(_), None) => self.accept_entity_wide,
            (None, Some(_)) => false,
            (None, None) => true,
        }
    }
}

/// The registered `graph.unknown-node` failure for a query naming an
/// unknown operation or resource (public for the CLI handoff).
pub fn unknown_subject_set(subject: &str) -> DiagnosticSet {
    diagnostic::unknown_subject_set(subject)
}

/// Attach one detected-evidence envelope: the returned graph shares the
/// declared projection and appends the validated records (canonical
/// model untouched).
pub fn attach_detected(
    graph: &EffectGraph,
    envelope: &EvidenceEnvelope,
) -> Result<EffectGraph, DiagnosticSet> {
    build::attach(graph, envelope)
}
