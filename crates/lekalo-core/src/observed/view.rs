//! Observed projections: the per-symbol card, the observed impact, and
//! the view the canonical impact analyzer consumes (issue #39).
//!
//! Every projection states its own completeness with the closed
//! `complete`/`incomplete`/`unknown` states: an inferred or stale fact
//! degrades the card, missing evidence means `unknown`, and an observed
//! impact always reports that the recorded graph may be incomplete.

use serde::Serialize;

use super::diagnostic;
use super::types::{
    BindingState, BindingStatus, Confidence, EndpointRecord, Evidence, ObservedIndex, Provenance,
    SchemaRecord, SourceLocation, SymbolKind, SymbolRecord,
};
use super::version;
use crate::diagnostics::DiagnosticSet;

/// The closed completeness state of one projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Completeness {
    Complete,
    Incomplete,
    Unknown,
}

impl Completeness {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incomplete => "incomplete",
            Self::Unknown => "unknown",
        }
    }
}

/// The completeness decision for one record: only a user-owned, current
/// binding with fingerprint evidence is complete; anything inferred,
/// stale, or without evidence degrades.
pub fn record_completeness(record: &SymbolRecord) -> Completeness {
    if record.state == BindingState::Unknown || record.fingerprint.is_none() {
        return Completeness::Unknown;
    }
    if record.state == BindingState::Stale || record.status == BindingStatus::Inferred {
        return Completeness::Incomplete;
    }
    Completeness::Complete
}

/// The observed symbol card: the deterministic answer to the
/// single-symbol question over recorded existing code.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObservedCard {
    pub mode: &'static str,
    pub symbol: String,
    pub kind: SymbolKind,
    pub module: String,
    pub binding: BindingStatus,
    pub state: BindingState,
    pub promoted: bool,
    pub promotion: Option<super::types::PromotionReceipt>,
    pub stable_key: Option<String>,
    pub location: Option<SourceLocation>,
    pub fingerprint: Option<String>,
    pub evidence: Evidence,
    pub provenance: Provenance,
    pub native_tests: Vec<String>,
    pub gates: Vec<String>,
    pub endpoints: Vec<EndpointRecord>,
    pub schemas: Vec<SchemaRecord>,
    pub completeness: Completeness,
}

/// Project the card for one recorded symbol.
pub fn inspect_card(index: &ObservedIndex, symbol_id: &str) -> Result<ObservedCard, DiagnosticSet> {
    let record = index
        .symbol(symbol_id)
        .ok_or_else(|| diagnostic::unknown_symbol_set(symbol_id))?;
    let module = ObservedIndex::module_of(symbol_id)
        .unwrap_or_default()
        .to_owned();
    Ok(ObservedCard {
        mode: version::MODE,
        symbol: record.id.clone(),
        kind: record.kind,
        module,
        binding: record.status,
        state: record.state,
        promoted: record.promoted,
        promotion: record.promotion.clone(),
        stable_key: record.stable_key.clone(),
        location: record.location.clone(),
        fingerprint: record.fingerprint.clone(),
        evidence: record.evidence.clone(),
        provenance: record.provenance.clone(),
        native_tests: record.native_tests.clone(),
        gates: record.gates.clone(),
        endpoints: index
            .endpoints
            .iter()
            .filter(|endpoint| endpoint.symbol == symbol_id)
            .cloned()
            .collect(),
        schemas: index
            .schemas
            .iter()
            .filter(|schema| schema.symbol == symbol_id)
            .cloned()
            .collect(),
        completeness: record_completeness(record),
    })
}

impl ObservedCard {
    /// The canonical JSON payload bytes (no envelope, no trailing LF).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// The deterministic human projection.
    pub fn to_human(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "observed {} ({} {})",
            self.symbol,
            self.kind.key(),
            self.completeness.key()
        ));
        lines.push(format!(
            "  binding {} {}",
            self.binding.key(),
            self.state.key()
        ));
        if let Some(location) = &self.location {
            match location.line {
                Some(line) => lines.push(format!("  at {}:{line}", location.path)),
                None => lines.push(format!("  at {}", location.path)),
            }
        }
        if let Some(key) = &self.stable_key {
            lines.push(format!("  key {key}"));
        }
        if self.promoted {
            lines.push("  promoted to the canonical model".to_owned());
        }
        for reference in &self.evidence.references {
            lines.push(format!(
                "  {} {} ({})",
                reference.role.key(),
                reference.target,
                reference.confidence.key()
            ));
        }
        for endpoint in &self.endpoints {
            lines.push(format!(
                "  endpoint {} {} {}",
                endpoint.method, endpoint.path, endpoint.id
            ));
        }
        for test in &self.native_tests {
            lines.push(format!("  native-test {test}"));
        }
        for gate in &self.gates {
            lines.push(format!("  gate {gate}"));
        }
        lines.join("\n")
    }
}

/// The observed impact of one symbol: the recorded dependents and the
/// explicit incompleteness of the recorded graph.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObservedImpact {
    pub mode: &'static str,
    pub symbol: String,
    pub roots: Vec<String>,
    pub dependents: Vec<ObservedDependent>,
    pub recorded_symbols: usize,
    pub stale: usize,
    pub unknown: usize,
    pub completeness: Completeness,
}

/// One recorded dependent edge.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize)]
pub struct ObservedDependent {
    pub from: String,
    pub role: super::types::ReferenceRole,
    pub confidence: super::types::Confidence,
    pub binding: BindingStatus,
}

/// Project the observed impact for one recorded symbol.
pub fn impact_card(
    index: &ObservedIndex,
    symbol_id: &str,
) -> Result<ObservedImpact, DiagnosticSet> {
    if index.symbol(symbol_id).is_none() {
        return Err(diagnostic::unknown_symbol_set(symbol_id));
    }
    let mut dependents: Vec<ObservedDependent> = Vec::new();
    for record in &index.symbols {
        for reference in &record.evidence.references {
            if reference.target == symbol_id {
                dependents.push(ObservedDependent {
                    from: record.id.clone(),
                    role: reference.role,
                    confidence: reference.confidence,
                    binding: record.status,
                });
            }
        }
    }
    dependents.sort_by(|left, right| (&left.from, &left.role).cmp(&(&right.from, &right.role)));
    dependents.dedup();
    let stale = index
        .symbols
        .iter()
        .filter(|record| record.state == BindingState::Stale)
        .count();
    let unknown = index
        .symbols
        .iter()
        .filter(|record| record.state == BindingState::Unknown)
        .count();
    // The recorded graph is never provably complete: unscanned code can
    // always hide further dependents, so the observed impact always
    // reports the incompleteness explicitly.
    let completeness = Completeness::Incomplete;
    Ok(ObservedImpact {
        mode: version::MODE,
        symbol: symbol_id.to_owned(),
        roots: vec![symbol_id.to_owned()],
        dependents,
        recorded_symbols: index.symbols.len(),
        stale,
        unknown,
        completeness,
    })
}

impl ObservedImpact {
    /// The canonical JSON payload bytes (no envelope, no trailing LF).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// The deterministic human projection.
    pub fn to_human(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "observed impact {} (graph {})",
            self.symbol,
            self.completeness.key()
        ));
        for dependent in &self.dependents {
            lines.push(format!(
                "  {} {} ({}, {})",
                dependent.from,
                dependent.role.key(),
                dependent.binding.key(),
                dependent.confidence.key()
            ));
        }
        lines.push(format!(
            "  {} recorded symbols, {} stale, {} unknown",
            self.recorded_symbols, self.stale, self.unknown
        ));
        lines.push("  note the recorded graph is incomplete by construction".to_owned());
        lines.join("\n")
    }
}

/// One recorded reference from observed code to a non-recorded target:
/// the edge the canonical impact analyzer must classify (a canonical
/// dependent, or a dangling reference in the incompleteness report).
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ObservedEdge {
    /// The recorded symbol holding the reference.
    pub observer: String,
    /// The referenced target id (not recorded in the index).
    pub target: String,
    pub confidence: Confidence,
}

/// The view the canonical impact analyzer consumes: every recorded
/// reference to a non-recorded target, plus the incompleteness counters
/// the result must surface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObservedView {
    /// Sorted `(target, observer)` edges.
    pub edges: Vec<ObservedEdge>,
    pub symbols: usize,
    pub stale: usize,
    pub unknown: usize,
}

impl ObservedView {
    /// Build the analyzer view of one index: every recorded reference,
    /// because only the analyzer can decide which targets are canonical.
    pub fn of(index: &ObservedIndex) -> Self {
        let mut edges: Vec<ObservedEdge> = Vec::new();
        for record in &index.symbols {
            if record.promoted {
                // Promoted records are canonical-owned: their references
                // are already part of the compiled canonical graph.
                continue;
            }
            for reference in &record.evidence.references {
                edges.push(ObservedEdge {
                    observer: record.id.clone(),
                    target: reference.target.clone(),
                    confidence: reference.confidence,
                });
            }
        }
        edges.sort_by(|left, right| {
            (&left.target, &left.observer).cmp(&(&right.target, &right.observer))
        });
        edges.dedup();
        Self {
            edges,
            symbols: index.symbols.len(),
            stale: index
                .symbols
                .iter()
                .filter(|record| record.state == BindingState::Stale)
                .count(),
            unknown: index
                .symbols
                .iter()
                .filter(|record| record.state == BindingState::Unknown)
                .count(),
        }
    }

    /// Whether any recorded fact can change an impact verdict.
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty() && self.stale == 0 && self.unknown == 0
    }
}
