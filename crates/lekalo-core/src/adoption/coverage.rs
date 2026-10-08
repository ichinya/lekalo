//! Hybrid coverage and the effective mode join (issue #136, increment
//! I5): untouched observed code, adapted legacy contracted code, and
//! new contracted code coexist in one project; the join is a derived
//! view with disjoint effective sets, explicit gaps, and no ownership
//! authority. No Model `mode` field, managed requirement, or generated
//! ownership claim is introduced.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value as Json;

use super::analysis::{compute_pins, AdoptionContext, AnalysisScope, Outcome};
use super::diagnostic;
use super::version;
use super::wire::{
    self, BoundaryWitness, CoverageCounts, CoverageDocument, CoverageRow, Gap, ImpactReport,
    PanelRow, Pins, StatusReport, TraceReport,
};
use crate::observed::types::{BindingStatus, ObservedIndex};
use crate::result::DomainResult;

/// The executed-verification provenance of one coverage join: the
/// sorted digests of the completed adoption apply/confirmation receipts
/// recorded under the project's receipt home. Real executions only —
/// every digest here names a persisted receipt document the rollback
/// surface can still consume.
fn executed_receipt_digests(ctx: &AdoptionContext) -> Vec<String> {
    let mut digests = Vec::new();
    let home = ctx.root.join(version::RECEIPTS_DIR);
    let Ok(heads) = std::fs::read_dir(&home) else {
        return digests;
    };
    for head in heads.flatten() {
        let Ok(tails) = std::fs::read_dir(head.path()) else {
            continue;
        };
        for tail in tails.flatten() {
            let name = tail.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(hex) = name.strip_suffix(".json") else {
                continue;
            };
            let head_hex = head.file_name().to_string_lossy().to_string();
            if head_hex.len() == 32
                && hex.len() == 32
                && head_hex
                    .bytes()
                    .chain(hex.bytes())
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                digests.push(format!("sha256:{head_hex}{hex}"));
            }
        }
    }
    digests.sort();
    digests
}

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

    let conformed_drift: BTreeSet<String> = registry
        .as_ref()
        .map(|registry| {
            registry
                .symbols
                .iter()
                .filter(|symbol| match &symbol.fingerprint {
                    Some(expected) => match std::fs::read(ctx.root.join(&symbol.source.path)) {
                        Ok(bytes) => {
                            let actual = crate::digest::sha256_hex(&bytes);
                            expected != &actual && expected != &format!("sha256:{actual}")
                        }
                        Err(_) => true,
                    },
                    None => false,
                })
                .map(|symbol| symbol.id.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut rows = Vec::new();
    let mut gaps = Vec::new();
    let receipt_digests = executed_receipt_digests(ctx);
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
        // Live conformance join (X11): the persisted registry names a
        // row conformed, but only a fresh re-fingerprint of the
        // recorded source against the real bytes makes
        // `contracted-* / current` honest. Drift downgrades the row to
        // `conflicting` with the typed gap; the registry row is never
        // presented current when the tree moved.
        let drifted = conformed_drift.contains(id);
        if drifted {
            gaps.push(Gap {
                subject: id.clone(),
                detail: "conformance-fingerprint-drift".to_owned(),
            });
        }
        // Executed provenance (X11): a conformed row carries the
        // persisted apply/confirmation receipt digests, so trace can
        // join real executions instead of a hard-wired empty list.
        let verification = if is_conformed && !receipt_digests.is_empty() {
            Some(receipt_digests.join(","))
        } else {
            None
        };
        let effective = if drifted && matches!(effective, "contracted-legacy" | "contracted-new") {
            "conflicting"
        } else {
            effective
        };
        rows.push(CoverageRow {
            symbol: id.clone(),
            effective: effective.to_owned(),
            state: state.to_owned(),
            origin: origin.map(str::to_owned),
            contract,
            binding,
            tests,
            verification,
            gaps: row_gaps,
        });
    }
    rows.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    gaps.sort_by(|left, right| (&left.subject, &left.detail).cmp(&(&right.subject, &right.detail)));
    gaps.dedup();
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
    // Conformance freshness (X11): the join binds to the exact consumed
    // input revisions it was derived from, so a later consumer can tell
    // a fresh panel from a stale one instead of presenting drift as
    // current.
    let pins = compute_pins(ctx, index, &scope_paths_of_rows(index, &rows))?;
    let revision = index
        .map(|index| index.revision.clone())
        .unwrap_or_else(|| "none".to_owned());
    Ok(CoverageDocument {
        schema_version: version::COVERAGE_SCHEMA_VERSION.to_owned(),
        identity: version::COVERAGE_IDENTITY.to_owned(),
        project,
        scope: scope.wire(),
        revision: Some(revision),
        pins: Some(pins),
        rows,
        counts,
        gaps,
    })
}

/// The consumed source set of one coverage join: every evidence path
/// the joined rows reference (locations, candidates, endpoints, test
/// bindings). Absent files contribute no pin; the freshness check
/// re-fingerprints exactly this set.
fn scope_paths_of_rows(
    index: Option<&ObservedIndex>,
    rows: &[CoverageRow],
) -> std::collections::BTreeSet<String> {
    let mut paths = std::collections::BTreeSet::new();
    for row in rows {
        if let Some(record) = index.and_then(|index| index.symbol(&row.symbol)) {
            if let Some(location) = &record.location {
                paths.insert(location.path.clone());
            }
            for candidate in &record.candidates {
                paths.insert(candidate.path.clone());
            }
        }
    }
    if let Some(index) = index {
        for endpoint in &index.endpoints {
            paths.insert(endpoint.path.clone());
        }
        for binding in &index.test_bindings {
            paths.insert(binding.path.clone());
        }
    }
    paths.retain(|path| version::is_rel_path(path));
    paths
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
    let document = wire::coverage_of_bytes(bytes).map_err(|error| error.domain_result())?;
    if document.project != project {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "coverage",
            "foreign-project",
        )));
    }
    let digest = coverage_digest(&document);
    Ok((document, digest))
}

/// The freshness verdict of one saved coverage attachment against the
/// live tree (X11): `fresh` only when the attachment's pinned
/// revisions and every pinned source digest still match the consumed
/// inputs; `stale` names the drifted members; `unknown` when the
/// attachment carries no pins (a hand-authored document).
pub fn coverage_freshness(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    coverage: &CoverageDocument,
) -> Outcome<(String, Vec<String>)> {
    let Some(pins) = &coverage.pins else {
        return Ok(("unknown".to_owned(), Vec::new()));
    };
    let scope_paths = scope_paths_of_rows(index, &coverage.rows);
    let current: Pins = compute_pins(ctx, index, &scope_paths)?;
    let changed = super::transaction::changed_pins(pins, &current);
    if changed.is_empty() {
        Ok(("fresh".to_owned(), Vec::new()))
    } else {
        Ok(("stale".to_owned(), changed))
    }
}

/// The typed status projection: the exact upstream envelope plus the
/// joined mode/freshness panels and the explicit gaps. Each panel's
/// freshness is qualified by the attachment's own recorded conformance
/// freshness: a stale or unqualified attachment never presents its
/// rows as plain `fresh` (X11).
pub fn status_report(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    upstream: Json,
    coverage: &CoverageDocument,
) -> Outcome<StatusReport> {
    let freshness = coverage_freshness(ctx, index, coverage)?;
    let attachment_fresh = freshness.0 == "fresh";
    // Live conformance join (X11): the conformance-fingerprint drift the
    // generation recorded downgrades the affected current rows even when
    // the attachment itself is byte-fresh — a drifted source is never a
    // fresh panel.
    let drifted: BTreeSet<&str> = coverage
        .gaps
        .iter()
        .filter(|gap| gap.detail == "conformance-fingerprint-drift")
        .map(|gap| gap.subject.as_str())
        .collect();
    Ok(StatusReport {
        schema_version: version::STATUS_REPORT_SCHEMA_VERSION.to_owned(),
        identity: version::STATUS_REPORT_IDENTITY.to_owned(),
        upstream,
        coverage: coverage_digest(coverage),
        panels: coverage
            .rows
            .iter()
            .map(|row| {
                let freshness = if drifted.contains(row.symbol.as_str()) {
                    "stale".to_owned()
                } else {
                    panel_freshness(row, attachment_fresh)
                };
                PanelRow {
                    symbol: row.symbol.clone(),
                    mode: row.effective.clone(),
                    state: row.state.clone(),
                    freshness,
                }
            })
            .collect(),
        gaps: coverage.gaps.clone(),
    })
}

/// The qualified per-panel freshness: a `current` row is fresh only
/// when the attachment's conformance freshness holds; any other state
/// or a stale/unqualified attachment stays qualified.
fn panel_freshness(row: &CoverageRow, attachment_fresh: bool) -> String {
    match (row.state.as_str(), attachment_fresh) {
        ("current", true) => "fresh".to_owned(),
        ("current", false) => "unqualified".to_owned(),
        ("stale", _) => "stale".to_owned(),
        _ => "unknown".to_owned(),
    }
}

/// The typed impact projection: the exact upstream envelope plus
/// mode-boundary witnesses (observed neighbors included) and the
/// explicit coverage gaps.
pub fn impact_report(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    upstream: Json,
    coverage: &CoverageDocument,
) -> Outcome<ImpactReport> {
    // The same typed freshness axis the status panels carry: a witness
    // never presents stale or unqualified evidence as plain current
    // (X11 — the computed verdict is joined, never discarded).
    let (attachment_freshness, _) = coverage_freshness(ctx, index, coverage)?;
    let drifted: BTreeSet<&str> = coverage
        .gaps
        .iter()
        .filter(|gap| gap.detail == "conformance-fingerprint-drift")
        .map(|gap| gap.subject.as_str())
        .collect();
    Ok(ImpactReport {
        schema_version: version::IMPACT_REPORT_SCHEMA_VERSION.to_owned(),
        identity: version::IMPACT_REPORT_IDENTITY.to_owned(),
        upstream,
        coverage: coverage_digest(coverage),
        boundaries: coverage
            .rows
            .iter()
            .map(|row| {
                let freshness = if drifted.contains(row.symbol.as_str()) {
                    "stale".to_owned()
                } else {
                    match (row.state.as_str(), attachment_freshness.as_str()) {
                        ("current", "fresh") => "fresh".to_owned(),
                        ("current", _) => "unqualified".to_owned(),
                        ("stale", _) => "stale".to_owned(),
                        _ => "unknown".to_owned(),
                    }
                };
                BoundaryWitness {
                    symbol: row.symbol.clone(),
                    mode: row.effective.clone(),
                    witness: match row.effective.as_str() {
                        "observed" => "source-authoritative".to_owned(),
                        "contracted-legacy" => "adapted-legacy-contracted".to_owned(),
                        "contracted-new" => "new-contracted".to_owned(),
                        "unbound" => "unresolved-binding".to_owned(),
                        _ => "ownership-conflict".to_owned(),
                    },
                    freshness,
                }
            })
            .collect(),
        gaps: coverage.gaps.clone(),
    })
}

/// The typed trace projection: the exact manifest and query result plus
/// the coverage and verification provenance joins with uncovered and
/// stale obligations visible. `verification` names the executed
/// verification provenance joined by the caller or discovered from the
/// coverage rows (persisted receipt digests, never specifications);
/// trace also revalidates the attachment freshness so a stale
/// attachment is named instead of silently joined.
pub fn trace_report(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    manifest: Json,
    query: Json,
    coverage: &CoverageDocument,
    verification: Vec<String>,
) -> Outcome<TraceReport> {
    let mut verification = verification;
    for row in &coverage.rows {
        if let Some(joined) = &row.verification {
            for digest in joined.split(',') {
                if !verification.iter().any(|known| known == digest) {
                    verification.push(digest.to_owned());
                }
            }
        }
    }
    verification.sort();
    verification.dedup();
    let uncovered: Vec<Gap> = coverage
        .rows
        .iter()
        .filter(|row| row.tests.is_empty())
        .map(|row| Gap {
            subject: row.symbol.clone(),
            detail: "no-linked-test".to_owned(),
        })
        .collect();
    let mut stale: Vec<Gap> = coverage
        .rows
        .iter()
        .filter(|row| row.state == "stale" || row.effective == "conflicting")
        .map(|row| Gap {
            subject: row.symbol.clone(),
            detail: row.state.clone(),
        })
        .collect();
    // Trace revalidates the attachment the same way status does: a
    // stale coverage join is an explicit `stale` obligation, never a
    // silently accepted join.
    match coverage_freshness(ctx, index, coverage) {
        Ok((verdict, changed)) if verdict == "stale" => {
            for input in changed {
                stale.push(Gap {
                    subject: "coverage-attachment".to_owned(),
                    detail: input,
                });
            }
        }
        Ok((verdict, _)) if verdict == "unknown" => stale.push(Gap {
            subject: "coverage-attachment".to_owned(),
            detail: "freshness-unknown".to_owned(),
        }),
        _ => {}
    }
    Ok(TraceReport {
        schema_version: version::TRACE_REPORT_SCHEMA_VERSION.to_owned(),
        identity: version::TRACE_REPORT_IDENTITY.to_owned(),
        manifest,
        query,
        coverage: coverage_digest(coverage),
        verification,
        uncovered,
        stale,
    })
}
