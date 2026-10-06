//! Hybrid coverage and the effective mode join (issue #136, increment
//! I5): untouched observed code, adapted legacy contracted code, and
//! new contracted code coexist in one project; the join is a derived
//! view with disjoint effective sets, explicit gaps, and no ownership
//! authority. No Model `mode` field, managed requirement, or generated
//! ownership claim is introduced.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value as Json;

use super::analysis::{AdoptionContext, AnalysisScope, Outcome};
use super::diagnostic;
use super::version;
use super::wire::{
    self, BoundaryWitness, CoverageCounts, CoverageDocument, CoverageRow, Gap, ImpactReport,
    PanelRow, StatusReport, TraceReport,
};
use crate::observed::types::{BindingStatus, ObservedIndex};
use crate::result::DomainResult;

/// Produce the coverage join for exactly one selected scope.
pub fn coverage(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    scope: &AnalysisScope,
) -> Outcome<CoverageDocument> {
    let (_, ids) = super::analysis::resolve_scope(ctx, index, scope)?;
    let canonical: BTreeSet<&str> = ctx
        .model
        .definitions
        .iter()
        .map(|definition| definition.id.as_str())
        .collect();
    let registry = super::refactor::conformed_registry(ctx);
    let conformed: BTreeSet<String> = registry
        .as_ref()
        .map(|registry| {
            registry
                .symbols
                .iter()
                .map(|symbol| symbol.id.clone())
                .collect()
        })
        .unwrap_or_default();

    // The active (unconfirmed) binding proposals, for the pending
    // binding rows; confirmed bindings live in the record itself.
    let mut proposals: BTreeMap<String, (String, String)> = BTreeMap::new();
    if index.is_some() {
        if let Ok(receipt) = crate::observed::bindings::propose(&crate::observed::ObservedContext {
            model: ctx.model.clone(),
            root: ctx.root.clone(),
        }) {
            for proposal in receipt.proposals {
                if let Some(candidate) = proposal.candidates.first() {
                    proposals.insert(
                        proposal.symbol.clone(),
                        (proposal.proposal, candidate.native.clone()),
                    );
                }
            }
        }
    }

    let mut rows = Vec::new();
    let mut gaps = Vec::new();
    for id in &ids {
        let record = index.and_then(|index| index.symbol(id));
        let is_canonical = canonical.contains(id.as_str());
        let is_conformed = conformed.contains(id);
        let (effective, origin, state, row_gaps): (&str, Option<&str>, &str, Vec<String>) =
            match (is_canonical, is_conformed, record) {
                (true, true, Some(record))
                    if record.state == crate::observed::types::BindingState::Stale =>
                {
                    (
                        "conflicting",
                        Some("legacy"),
                        "stale",
                        vec!["observed-evidence-stale".to_owned()],
                    )
                }
                (true, true, Some(_)) => {
                    ("contracted-legacy", Some("legacy"), "current", Vec::new())
                }
                (true, true, None) => ("contracted-new", Some("new"), "current", Vec::new()),
                (true, false, Some(record)) => {
                    let mut gaps = vec!["canonical-without-conformance".to_owned()];
                    if record.state == crate::observed::types::BindingState::Stale {
                        gaps.push("observed-evidence-stale".to_owned());
                    }
                    ("observed", Some("legacy"), state_of_record(record), gaps)
                }
                (true, false, None) => (
                    "unbound",
                    None,
                    "unknown",
                    vec!["definition-without-evidence".to_owned()],
                ),
                (false, false, Some(record)) => {
                    if record.status == BindingStatus::Inferred {
                        (
                            "unbound",
                            Some("legacy"),
                            state_of_record(record),
                            vec!["binding-inferred".to_owned()],
                        )
                    } else {
                        (
                            "observed",
                            Some("legacy"),
                            state_of_record(record),
                            Vec::new(),
                        )
                    }
                }
                (false, false, None) => continue,
                (false, true, _) => (
                    "conflicting",
                    Some("new"),
                    "unknown",
                    vec!["conformed-without-canonical".to_owned()],
                ),
            };
        let (tests, contract) = match (record, is_conformed) {
            (Some(record), conformed) => (
                record.native_tests.clone(),
                if conformed { Some(id.clone()) } else { None },
            ),
            (None, true) => (
                registry
                    .as_ref()
                    .and_then(|registry| registry.symbols.iter().find(|symbol| &symbol.id == id))
                    .map(|symbol| symbol.native_tests.clone())
                    .unwrap_or_default(),
                Some(id.clone()),
            ),
            (None, false) => (Vec::new(), None),
        };
        let binding = match (record, proposals.get(id)) {
            (Some(record), Some((proposal, candidate)))
                if record.status == BindingStatus::Explicit =>
            {
                Some(super::wire::BindingSelection {
                    proposal: proposal.clone(),
                    candidate: candidate.clone(),
                    symbol: id.clone(),
                })
            }
            (Some(record), _) if record.status == BindingStatus::Confirmed => None,
            _ => None,
        };
        for gap in &row_gaps {
            gaps.push(Gap {
                subject: id.clone(),
                detail: gap.clone(),
            });
        }
        rows.push(CoverageRow {
            symbol: id.clone(),
            effective: effective.to_owned(),
            state: state.to_owned(),
            origin: origin.map(str::to_owned),
            contract,
            binding,
            tests,
            verification: None,
            gaps: row_gaps,
        });
    }
    rows.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    gaps.sort_by(|left, right| (&left.subject, &left.detail).cmp(&(&right.subject, &right.detail)));
    let counts = CoverageCounts {
        observed: rows
            .iter()
            .filter(|row| row.effective == "observed")
            .count() as u64,
        contracted_legacy: rows
            .iter()
            .filter(|row| row.effective == "contracted-legacy")
            .count() as u64,
        contracted_new: rows
            .iter()
            .filter(|row| row.effective == "contracted-new")
            .count() as u64,
        unbound: rows.iter().filter(|row| row.effective == "unbound").count() as u64,
        conflicting: rows
            .iter()
            .filter(|row| row.effective == "conflicting")
            .count() as u64,
    };
    let project = ctx
        .model
        .project
        .as_ref()
        .map(|project| project.id.clone())
        .unwrap_or_default();
    Ok(CoverageDocument {
        schema_version: version::COVERAGE_SCHEMA_VERSION.to_owned(),
        identity: version::COVERAGE_IDENTITY.to_owned(),
        project,
        scope: scope.wire(),
        rows,
        counts,
        gaps,
    })
}

fn state_of_record(record: &crate::observed::types::SymbolRecord) -> &'static str {
    match record.state {
        crate::observed::types::BindingState::Current => "current",
        crate::observed::types::BindingState::Stale => "stale",
        crate::observed::types::BindingState::Unknown => "unknown",
    }
}

/// The content-address digest of one coverage document.
pub fn coverage_digest(document: &CoverageDocument) -> String {
    let value = serde_json::to_value(document).expect("coverage serializes");
    wire::digest("adoption-coverage", &value)
}

/// Load a saved coverage attachment and recompute its digest; a stale
/// or foreign attachment refuses rather than silently degrading.
pub fn load_coverage(bytes: &[u8], project: &str) -> Outcome<(CoverageDocument, String)> {
    let document =
        wire::coverage_of_bytes(bytes).map_err(|error| DomainResult::invalid(error.set()))?;
    if document.project != project {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "coverage",
            "foreign-project",
        )));
    }
    let digest = coverage_digest(&document);
    Ok((document, digest))
}

/// The typed status projection: the exact upstream envelope plus the
/// joined mode/freshness panels and the explicit gaps.
pub fn status_report(upstream: Json, coverage: &CoverageDocument) -> StatusReport {
    StatusReport {
        schema_version: version::STATUS_REPORT_SCHEMA_VERSION.to_owned(),
        identity: version::STATUS_REPORT_IDENTITY.to_owned(),
        upstream,
        coverage: coverage_digest(coverage),
        panels: coverage
            .rows
            .iter()
            .map(|row| PanelRow {
                symbol: row.symbol.clone(),
                mode: row.effective.clone(),
                state: row.state.clone(),
                freshness: match row.state.as_str() {
                    "current" => "fresh".to_owned(),
                    "stale" => "stale".to_owned(),
                    _ => "unknown".to_owned(),
                },
            })
            .collect(),
        gaps: coverage.gaps.clone(),
    }
}

/// The typed impact projection: the exact upstream envelope plus
/// mode-boundary witnesses (observed neighbors included) and the
/// explicit coverage gaps.
pub fn impact_report(upstream: Json, coverage: &CoverageDocument) -> ImpactReport {
    ImpactReport {
        schema_version: version::IMPACT_REPORT_SCHEMA_VERSION.to_owned(),
        identity: version::IMPACT_REPORT_IDENTITY.to_owned(),
        upstream,
        coverage: coverage_digest(coverage),
        boundaries: coverage
            .rows
            .iter()
            .map(|row| BoundaryWitness {
                symbol: row.symbol.clone(),
                mode: row.effective.clone(),
                witness: match row.effective.as_str() {
                    "observed" => "source-authoritative".to_owned(),
                    "contracted-legacy" => "adapted-legacy-contracted".to_owned(),
                    "contracted-new" => "new-contracted".to_owned(),
                    "unbound" => "unresolved-binding".to_owned(),
                    _ => "ownership-conflict".to_owned(),
                },
            })
            .collect(),
        gaps: coverage.gaps.clone(),
    }
}

/// The typed trace projection: the exact manifest and query result plus
/// the coverage and verification provenance joins with uncovered and
/// stale obligations visible.
pub fn trace_report(
    manifest: Json,
    query: Json,
    coverage: &CoverageDocument,
    verification: Vec<String>,
) -> TraceReport {
    let uncovered: Vec<Gap> = coverage
        .rows
        .iter()
        .filter(|row| row.tests.is_empty())
        .map(|row| Gap {
            subject: row.symbol.clone(),
            detail: "no-linked-test".to_owned(),
        })
        .collect();
    let stale: Vec<Gap> = coverage
        .rows
        .iter()
        .filter(|row| row.state == "stale" || row.effective == "conflicting")
        .map(|row| Gap {
            subject: row.symbol.clone(),
            detail: row.state.clone(),
        })
        .collect();
    TraceReport {
        schema_version: version::TRACE_REPORT_SCHEMA_VERSION.to_owned(),
        identity: version::TRACE_REPORT_IDENTITY.to_owned(),
        manifest,
        query,
        coverage: coverage_digest(coverage),
        verification,
        uncovered,
        stale,
    }
}
