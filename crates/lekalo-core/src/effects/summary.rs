//! Compact deterministic effect summaries (issue #14).
//!
//! The summary is bounded, sorted, and rebuildable: per-operation kind
//! and action counts, declared-versus-detected totals, evidence trust,
//! and conflict/sensitivity flags. No raw values, payloads, or adapter
//! prose ever enter it, and a bounded summary reports incompleteness
//! instead of pretending to be whole.

use super::edge::EffectEdge;
use super::identity::OperationId;
use super::provenance::{EffectProvenance, TrustState};
use super::version::MAX_RESULT_ROWS;
use super::EffectGraph;

/// One per-operation summary row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationSummary {
    operation: OperationId,
    reads: usize,
    writes: usize,
    emissions: usize,
    infrastructure: usize,
    declared: usize,
    detected: usize,
    degraded: bool,
    sensitive: bool,
}

impl OperationSummary {
    /// The summarized operation.
    pub const fn operation(&self) -> &OperationId {
        &self.operation
    }

    /// Read-effect count (declared plus detected).
    pub const fn reads(&self) -> usize {
        self.reads
    }

    /// Write-effect count (CRUD, field writes, cache mutations).
    pub const fn writes(&self) -> usize {
        self.writes
    }

    /// Event and job emission count.
    pub const fn emissions(&self) -> usize {
        self.emissions
    }

    /// External, cache-infrastructure, output, audit, and boundary count.
    pub const fn infrastructure(&self) -> usize {
        self.infrastructure
    }

    /// How many of the operation's edges are declared.
    pub const fn declared(&self) -> usize {
        self.declared
    }

    /// How many of the operation's edges are detected.
    pub const fn detected(&self) -> usize {
        self.detected
    }

    /// Whether any edge carries degraded (stale/unknown) evidence.
    pub const fn degraded(&self) -> bool {
        self.degraded
    }

    /// Whether any edge carries a classified sensitivity marker.
    pub const fn sensitive(&self) -> bool {
        self.sensitive
    }
}

/// One finished summary over the whole graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectSummary {
    rows: Vec<OperationSummary>,
    total: usize,
    complete: bool,
}

impl EffectSummary {
    /// The per-operation rows in canonical order.
    pub fn rows(&self) -> &[OperationSummary] {
        &self.rows
    }

    /// The total effect-edge count.
    pub const fn total(&self) -> usize {
        self.total
    }

    /// Whether the summary covered every operation within the row bound.
    pub const fn complete(&self) -> bool {
        self.complete
    }
}

/// Whether one edge's evidence is degraded.
fn degraded(edge: &EffectEdge) -> bool {
    match edge.provenance() {
        EffectProvenance::Evidence { trust, .. } => {
            matches!(trust, TrustState::Stale | TrustState::Unknown)
        }
        _ => false,
    }
}

/// Build the whole-graph summary; the spec is v1 parameter-free and
/// exists so successors can narrow without a breaking shape change.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SummarySpec {}

impl SummarySpec {
    /// The default whole-graph spec.
    pub fn new() -> Self {
        Self {}
    }
}

/// Build one bounded summary of one graph.
pub(crate) fn summarize(
    graph: &EffectGraph,
    _spec: &SummarySpec,
) -> Result<EffectSummary, crate::diagnostics::DiagnosticSet> {
    let mut operations: Vec<OperationId> = graph
        .declared()
        .iter()
        .chain(graph.detected().iter())
        .map(|edge| edge.key().operation().clone())
        .collect();
    operations.sort();
    operations.dedup();

    let mut complete = true;
    let mut rows: Vec<OperationSummary> = Vec::new();
    for operation in &operations {
        if rows.len() >= MAX_RESULT_ROWS {
            complete = false;
            break;
        }
        let mut row = OperationSummary {
            operation: operation.clone(),
            reads: 0,
            writes: 0,
            emissions: 0,
            infrastructure: 0,
            declared: 0,
            detected: 0,
            degraded: false,
            sensitive: false,
        };
        for edge in graph.operation_edges(operation) {
            let kind = edge.key().kind();
            if kind.is_mutation() {
                row.writes += 1;
            } else {
                match kind {
                    super::kind::EffectKind::Read => row.reads += 1,
                    super::kind::EffectKind::EmitEvent | super::kind::EffectKind::EnqueueJob => {
                        row.emissions += 1
                    }
                    _ => row.infrastructure += 1,
                }
            }
            match edge.provenance().sort_rank() {
                0 => row.declared += 1,
                _ => row.detected += 1,
            }
            row.degraded |= degraded(edge);
            row.sensitive |= edge.is_sensitivity_classified();
        }
        rows.push(row);
    }
    Ok(EffectSummary {
        total: graph.declared().len() + graph.detected().len(),
        rows,
        complete,
    })
}
