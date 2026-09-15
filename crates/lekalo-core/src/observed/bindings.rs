//! The binding registry surface (issue #42).
//!
//! The observed index is the binding registry: every symbol record binds
//! one semantic id to native evidence, every endpoint record is the
//! `exposes` relation, and every native test binding is the `verifies`
//! relation. This module owns the registry workflow on top of that store:
//!
//! - [`propose`] derives one deterministic proposal per inferred binding
//!   with the full ranked candidate set. An ambiguous adapter mapping
//!   (two or more equally confident candidates) is never resolved by
//!   picking the first match: the proposal lists every candidate and
//!   refuses confirmation until the user names exactly one.
//! - [`confirm`] and [`confirm_batch`] turn inferred bindings into
//!   user-confirmed facts. Provenance is preserved (origin, adapter, and
//!   the exact scan revision stay on the record); only the status and
//!   the resolved native identity change. An inferred binding never
//!   becomes confirmed by anything except an explicit confirmation.
//! - [`audit`] re-fingerprints every binding — symbol and native test —
//!   after source changes: a changed signature or path is `stale`
//!   (gate failure) or correctly re-resolved by the adapter's stable
//!   keys, never silent.
//! - [`list`] projects the whole registry with the issue's binding data:
//!   semantic id, target/profile, native symbol identity, relative
//!   path/line, signature evidence, relation, source, confidence,
//!   adapter/version/fingerprint, and freshness state.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::diagnostics::DiagnosticSet;
use crate::project_fs::Fs;

use super::diagnostic;
use super::index;
use super::types::{BindingState, BindingStatus, CandidateRecord, ObservedIndex, SymbolRecord};
use super::version;

/// The closed relation of every registry row: symbol bindings implement,
/// endpoint bindings expose, native test bindings verify.
pub const RELATION_IMPLEMENTS: &str = "implements";
pub const RELATION_EXPOSES: &str = "exposes";
pub const RELATION_VERIFIES: &str = "verifies";

/// The issue #42 `source` projection of a binding: `explicit` is
/// user-declared, `user-confirmed` is a confirmed adapter mapping,
/// `detected` is an adapter mapping at a recorded location, and
/// `inferred` is an adapter mapping without one.
fn source_of(record: &SymbolRecord) -> &'static str {
    match record.status {
        BindingStatus::Explicit => "explicit",
        BindingStatus::Confirmed => "user-confirmed",
        BindingStatus::Inferred => {
            if record.location.is_some() {
                "detected"
            } else {
                "inferred"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Proposals
// ---------------------------------------------------------------------------

/// One ranked native candidate of a proposal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProposalCandidate {
    pub native: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub confidence: String,
}

/// One binding proposal: the deterministic, re-derivable offer to
/// confirm one inferred binding at one native identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Proposal {
    pub proposal: String,
    pub symbol: String,
    pub kind: &'static str,
    pub relation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub source: &'static str,
    /// The confidence of the top-ranked candidate.
    pub confidence: String,
    /// Whether the top two candidates tie: confirmation then requires
    /// naming exactly one candidate, and nothing is picked silently.
    pub ambiguous: bool,
    /// The full ranked candidate set (best first, name-ascending inside
    /// one confidence tier).
    pub candidates: Vec<ProposalCandidate>,
}

/// The receipt of `bindings propose`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProposeReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub action: &'static str,
    pub mode: &'static str,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub adapter: String,
    pub revision: String,
    pub proposals: Vec<Proposal>,
    pub ambiguous: usize,
}

/// The receipt of one `bindings confirm`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConfirmReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub action: &'static str,
    pub mode: &'static str,
    pub proposal: String,
    pub symbol: String,
    pub binding: &'static str,
    pub native: String,
    pub path: String,
    pub state: String,
}

/// One planned batch confirmation row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BatchEntry {
    pub proposal: String,
    pub symbol: String,
    pub native: String,
    pub path: String,
    pub confidence: String,
}

/// The receipt of the batch preview or apply.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BatchReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub action: &'static str,
    pub phase: &'static str,
    pub mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub entries: Vec<BatchEntry>,
    pub confirmed: Vec<String>,
}

/// The candidate set of one inferred record: the recorded mapping (when
/// it has a native identity) plus every scanned candidate, deduplicated
/// by native identity and ranked by (confidence descending, native
/// ascending). The top of the ranking is the only candidate an
/// unambiguous confirmation may resolve to.
fn ranked_candidates(record: &SymbolRecord) -> Vec<CandidateRecord> {
    let mut candidates: Vec<CandidateRecord> = Vec::new();
    if let Some(location) = &record.location {
        let native = record
            .stable_key
            .clone()
            .unwrap_or_else(|| location.path.clone());
        if !candidates.iter().any(|c| c.native == native) {
            candidates.push(CandidateRecord {
                native,
                path: location.path.clone(),
                line: location.line,
                fingerprint: record.fingerprint.clone(),
                confidence: record.provenance.confidence,
            });
        }
    }
    for candidate in &record.candidates {
        if !candidates.iter().any(|c| c.native == candidate.native) {
            candidates.push(candidate.clone());
        }
    }
    // `Confidence` orders exact < ... < unknown, so ascending order is
    // descending confidence; the native name breaks ties deterministically.
    candidates.sort_by(|left, right| {
        (&left.confidence, &left.native).cmp(&(&right.confidence, &right.native))
    });
    candidates
}

/// Whether the ranked candidate set ties at the top: two or more
/// candidates share the best confidence, so nothing may be picked
/// without the user naming one.
fn ambiguous(candidates: &[CandidateRecord]) -> bool {
    match (candidates.first(), candidates.get(1)) {
        (Some(first), Some(second)) => first.confidence == second.confidence,
        _ => false,
    }
}

/// The deterministic proposal identifier: `prop-` plus the SHA-256 over
/// the canonical payload of everything the proposal claims (project,
/// symbol, kind, relation, target, profile, adapter, revision, and the
/// full ranked candidate set). Any index change re-derives different
/// identifiers, so a stale id is refused instead of misapplied.
fn proposal_id(index: &ObservedIndex, symbol: &str, candidates: &[CandidateRecord]) -> String {
    let wire_candidates: Vec<serde_json::Value> = candidates
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "native": candidate.native,
                "path": candidate.path,
                "line": candidate.line,
                "fingerprint": candidate.fingerprint,
                "confidence": candidate.confidence.key(),
            })
        })
        .collect();
    let payload = serde_json::json!({
        "project": index.project,
        "symbol": symbol,
        "adapter": index.adapter.id,
        "revision": index.revision,
        "target": index.target,
        "profile": index.profile,
        "candidates": wire_candidates,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{}{:x}", version::PROPOSAL_ID_PREFIX, hasher.finalize())
}

/// Project one proposal from one inferred record; `None` for records
/// with no native identity at all (a placeholder no confirmation could
/// resolve).
fn proposal_of(index: &ObservedIndex, record: &SymbolRecord) -> Option<Proposal> {
    let candidates = ranked_candidates(record);
    let first = candidates.first()?;
    let id = proposal_id(index, &record.id, &candidates);
    Some(Proposal {
        proposal: id,
        symbol: record.id.clone(),
        kind: record.kind.key(),
        relation: RELATION_IMPLEMENTS,
        target: index.target.clone(),
        profile: index.profile.clone(),
        source: "inferred",
        confidence: first.confidence.key().to_owned(),
        ambiguous: ambiguous(&candidates),
        candidates: candidates
            .iter()
            .map(|candidate| ProposalCandidate {
                native: candidate.native.clone(),
                path: candidate.path.clone(),
                line: candidate.line,
                fingerprint: candidate.fingerprint.clone(),
                confidence: candidate.confidence.key().to_owned(),
            })
            .collect(),
    })
}

/// Derive every current proposal from the recorded index. Pure: the same
/// index always produces the same proposals with the same identifiers.
fn proposals_of(index: &ObservedIndex) -> Vec<Proposal> {
    let mut proposals: Vec<Proposal> = Vec::new();
    for record in &index.symbols {
        if record.status != BindingStatus::Inferred || record.promoted {
            // User-owned facts never gather proposals: explicit bindings
            // have priority, and a confirmed binding is already owned.
            continue;
        }
        if let Some(proposal) = proposal_of(index, record) {
            proposals.push(proposal);
        }
    }
    proposals.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    proposals
}

/// `bindings propose`: the full proposal set over the recorded registry.
pub fn propose(ctx: &super::ObservedContext) -> Result<ProposeReceipt, DiagnosticSet> {
    let index = index::load_index(ctx)?.ok_or_else(diagnostic::missing_index_set)?;
    let proposals = proposals_of(&index);
    let ambiguous = proposals.iter().filter(|p| p.ambiguous).count();
    Ok(ProposeReceipt {
        status: "valid",
        operation: "bindings",
        action: "propose",
        mode: version::MODE,
        project: index.project.clone(),
        target: index.target.clone(),
        profile: index.profile.clone(),
        adapter: index.adapter.id.clone(),
        revision: index.revision.clone(),
        ambiguous,
        proposals,
    })
}

/// Resolve the candidate a confirmation names: `None` picks the top
/// candidate of an unambiguous set; `Some(native)` must name one member
/// of the set.
fn resolve_candidate<'a>(
    proposal: &'a Proposal,
    candidates: &'a [CandidateRecord],
    native: Option<&str>,
) -> Result<&'a CandidateRecord, DiagnosticSet> {
    match native {
        None => {
            if proposal.ambiguous {
                return Err(diagnostic::ambiguous_set(
                    &proposal.symbol,
                    "ambiguous-candidates",
                ));
            }
            Ok(&candidates[0])
        }
        Some(named) => candidates
            .iter()
            .find(|candidate| candidate.native == named)
            .ok_or_else(|| diagnostic::ambiguous_set(&proposal.symbol, "unknown-candidate")),
    }
}

/// Apply one confirmation to the recorded index: the binding becomes
/// user-confirmed at the chosen native identity, provenance preserved.
fn apply_confirmation(
    index: &mut ObservedIndex,
    proposal: &Proposal,
    chosen: &CandidateRecord,
) -> Option<String> {
    let record = index.symbols.iter_mut().find(|r| r.id == proposal.symbol)?;
    let previous_path = record.location.as_ref().map(|l| l.path.clone());
    record.status = BindingStatus::Confirmed;
    record.state = BindingState::Current;
    record.stable_key = Some(chosen.native.clone());
    record.location = Some(super::types::SourceLocation {
        path: chosen.path.clone(),
        line: chosen.line,
    });
    record.fingerprint = chosen.fingerprint.clone();
    // Provenance is preserved: origin, adapter, and the exact scan
    // revision stay exactly as the scan recorded them.
    record.history.push(super::types::HistoryEntry {
        event: super::types::HistoryEvent::Confirmed,
        revision: index.revision.clone(),
        from: previous_path,
        to: Some(chosen.path.clone()),
    });
    index::trim_history(record);
    Some(record.id.clone())
}

/// `bindings confirm PROPOSAL [--candidate NATIVE]`: confirm one
/// inferred binding. An ambiguous proposal refuses until exactly one
/// candidate is named; the named candidate must be a member of the
/// proposal's set (never a free-form location).
pub fn confirm(
    ctx: &super::ObservedContext,
    proposal_id: &str,
    candidate: Option<&str>,
) -> Result<ConfirmReceipt, DiagnosticSet> {
    let index = index::load_index(ctx)?.ok_or_else(diagnostic::missing_index_set)?;
    let proposals = proposals_of(&index);
    let proposal = proposals
        .iter()
        .find(|p| p.proposal == proposal_id)
        .ok_or_else(|| diagnostic::proposal_unknown_set(proposal_id))?
        .clone();
    let candidates = ranked_candidates(
        index
            .symbols
            .iter()
            .find(|r| r.id == proposal.symbol)
            .expect("proposal source exists"),
    );
    let chosen = resolve_candidate(&proposal, &candidates, candidate)?.clone();
    let mut owned = index;
    let confirmed = apply_confirmation(&mut owned, &proposal, &chosen)
        .ok_or_else(|| diagnostic::proposal_unknown_set(proposal_id))?;
    index::save_index(ctx, &owned)?;
    Ok(ConfirmReceipt {
        status: "valid",
        operation: "bindings",
        action: "confirm",
        mode: version::MODE,
        proposal: proposal.proposal,
        symbol: confirmed,
        binding: "confirmed",
        native: chosen.native,
        path: chosen.path,
        state: BindingState::Current.key().to_owned(),
    })
}

/// The confirmable subset of the proposals: unambiguous sets only. A
/// batch never silently resolves an ambiguous mapping.
fn confirmable(proposals: &[Proposal], index: &ObservedIndex) -> Vec<BatchEntry> {
    let mut entries: Vec<BatchEntry> = Vec::new();
    for proposal in proposals {
        if proposal.ambiguous {
            continue;
        }
        let Some(record) = index.symbols.iter().find(|r| r.id == proposal.symbol) else {
            continue;
        };
        let candidates = ranked_candidates(record);
        let Some(chosen) = candidates.first() else {
            continue;
        };
        entries.push(BatchEntry {
            proposal: proposal.proposal.clone(),
            symbol: proposal.symbol.clone(),
            native: chosen.native.clone(),
            path: chosen.path.clone(),
            confidence: chosen.confidence.key().to_owned(),
        });
    }
    entries.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    entries
}

/// The deterministic batch plan identifier: the SHA-256 over the index
/// digest and the sorted proposal identifiers, in the accepted
/// `sha256:<64 lowercase hex>` plan form.
fn batch_plan_id(index: &ObservedIndex, entries: &[BatchEntry]) -> String {
    let mut payload = index::index_digest(index).unwrap_or_default();
    payload.push('\n');
    for entry in entries {
        payload.push_str(&entry.proposal);
        payload.push('\n');
    }
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// `bindings confirm --batch`: plan the batch (preview) with `None`, or
/// apply exactly a previously previewed plan with `Some(plan_id)`. The
/// apply recomputes the plan and refuses on any drift, so a batch can
/// never confirm anything its preview did not name.
pub fn confirm_batch(
    ctx: &super::ObservedContext,
    plan_id: Option<&str>,
) -> Result<BatchReceipt, DiagnosticSet> {
    let mut index = index::load_index(ctx)?.ok_or_else(diagnostic::missing_index_set)?;
    let proposals = proposals_of(&index);
    let entries = confirmable(&proposals, &index);
    let Some(expected) = plan_id else {
        return Ok(BatchReceipt {
            status: "valid",
            operation: "bindings",
            action: "confirm",
            phase: "plan",
            mode: version::MODE,
            plan: Some(batch_plan_id(&index, &entries)),
            confirmed: Vec::new(),
            entries,
        });
    };
    if batch_plan_id(&index, &entries) != expected {
        return Err(diagnostic::plan_mismatch_set("plan-id"));
    }
    let mut confirmed: Vec<String> = Vec::new();
    for entry in &entries {
        let proposal = proposals
            .iter()
            .find(|p| p.proposal == entry.proposal)
            .expect("planned proposal exists");
        let candidates = ranked_candidates(
            index
                .symbols
                .iter()
                .find(|r| r.id == proposal.symbol)
                .expect("proposal source exists"),
        );
        let chosen = resolve_candidate(proposal, &candidates, None)?.clone();
        if let Some(symbol) = apply_confirmation(&mut index, proposal, &chosen) {
            confirmed.push(symbol);
        }
    }
    confirmed.sort();
    index::save_index(ctx, &index)?;
    Ok(BatchReceipt {
        status: "valid",
        operation: "bindings",
        action: "confirm",
        phase: "apply",
        mode: version::MODE,
        plan: Some(expected.to_owned()),
        confirmed,
        entries,
    })
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

/// The receipt of a clean `bindings audit`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AuditReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub action: &'static str,
    pub mode: &'static str,
    pub symbols: usize,
    pub current: usize,
    pub unknown: usize,
    pub test_bindings: usize,
    pub tests_current: usize,
    pub tests_unknown: usize,
}

/// `bindings audit`: the staleness gate over the whole registry after
/// source changes. Every symbol binding (except promoted, canonical-owned
/// ones) and every native test binding is re-fingerprinted; a mismatch or
/// a missing source fails the gate with the registered
/// `observed.stale-binding` diagnostics, and evidence-free bindings are
/// `unknown`, never stale.
pub fn audit(ctx: &super::ObservedContext) -> Result<AuditReceipt, DiagnosticSet> {
    let index = index::load_index(ctx)?.ok_or_else(diagnostic::missing_index_set)?;
    let fs = Fs::open(&ctx.root).map_err(|_| diagnostic::index_io_set("root-unreadable"))?;
    let mut current = 0usize;
    let mut unknown = 0usize;
    let mut tests_current = 0usize;
    let mut tests_unknown = 0usize;
    let mut failures: Vec<(String, &'static str)> = Vec::new();
    for record in &index.symbols {
        if record.promoted {
            continue;
        }
        match (&record.fingerprint, &record.location) {
            (Some(digest), Some(location)) => match index::read_fingerprint(&fs, &location.path) {
                Ok(Some(actual)) if &actual == digest => current += 1,
                Ok(Some(_)) => failures.push((record.id.clone(), "fingerprint-mismatch")),
                Ok(None) | Err(_) => failures.push((record.id.clone(), "source-missing")),
            },
            _ => unknown += 1,
        }
    }
    for binding in &index.test_bindings {
        match &binding.fingerprint {
            Some(digest) => match index::read_fingerprint(&fs, &binding.path) {
                Ok(Some(actual)) if &actual == digest => tests_current += 1,
                Ok(Some(_)) => failures.push((binding.id.clone(), "fingerprint-mismatch")),
                Ok(None) | Err(_) => failures.push((binding.id.clone(), "source-missing")),
            },
            None => tests_unknown += 1,
        }
    }
    if !failures.is_empty() {
        let mut items = Vec::new();
        for (symbol, detail) in failures
            .iter()
            .take(crate::diagnostics::types::limits::DIAGNOSTICS_PER_RESULT)
        {
            match diagnostic::try_stale_binding_item(symbol, detail) {
                Some(item) => items.push(item),
                None => return Err(crate::result::singleton_set("diagnostics.registry-invalid")),
            }
        }
        return Err(crate::diagnostics::DiagnosticSet::try_from_unsorted(
            items,
            crate::result::Status::Invalid,
        )
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")));
    }
    Ok(AuditReceipt {
        status: "valid",
        operation: "bindings",
        action: "audit",
        mode: version::MODE,
        symbols: index.symbols.len(),
        current,
        unknown,
        test_bindings: index.test_bindings.len(),
        tests_current,
        tests_unknown,
    })
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

/// One projected binding row of `bindings list`: the full issue #42 data
/// model for one relation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BindingRow {
    pub semantic: String,
    pub kind: String,
    pub relation: &'static str,
    pub native: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub source: &'static str,
    pub confidence: String,
    pub state: String,
    pub adapter: String,
    pub revision: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<CandidateRecord>,
}

/// The registry counts of one list projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ListCounts {
    pub bindings: usize,
    pub endpoints: usize,
    pub tests: usize,
    pub explicit: usize,
    pub confirmed: usize,
    pub inferred: usize,
    pub current: usize,
    pub stale: usize,
    pub unknown: usize,
}

/// The receipt of `bindings list`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub action: &'static str,
    pub mode: &'static str,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub adapter: String,
    pub revision: String,
    pub counts: ListCounts,
    pub bindings: Vec<BindingRow>,
    pub endpoints: Vec<BindingRow>,
    pub tests: Vec<BindingRow>,
}

fn symbol_row(record: &SymbolRecord) -> BindingRow {
    BindingRow {
        semantic: record.id.clone(),
        kind: record.kind.key().to_owned(),
        relation: RELATION_IMPLEMENTS,
        native: record.stable_key.clone(),
        path: record.location.as_ref().map(|l| l.path.clone()),
        line: record.location.as_ref().and_then(|l| l.line),
        fingerprint: record.fingerprint.clone(),
        source: source_of(record),
        confidence: record.provenance.confidence.key().to_owned(),
        state: record.state.key().to_owned(),
        adapter: record.provenance.adapter.clone(),
        revision: record.provenance.revision.clone(),
        candidates: record.candidates.clone(),
    }
}

/// `bindings list`: the whole registry, projected. Symbol bindings
/// implement, endpoint bindings expose, and native test bindings verify;
/// every row carries the freshness state and provenance the registry
/// recorded.
pub fn list(ctx: &super::ObservedContext) -> Result<ListReceipt, DiagnosticSet> {
    let index = index::load_index(ctx)?.ok_or_else(diagnostic::missing_index_set)?;
    let bindings: Vec<BindingRow> = index.symbols.iter().map(symbol_row).collect();
    let endpoints: Vec<BindingRow> = index
        .endpoints
        .iter()
        .map(|endpoint| BindingRow {
            semantic: endpoint.symbol.clone(),
            kind: "endpoint".to_owned(),
            relation: RELATION_EXPOSES,
            native: Some(format!("{} {}", endpoint.method, endpoint.path)),
            path: None,
            line: None,
            fingerprint: None,
            source: "detected",
            confidence: "high".to_owned(),
            state: BindingState::Current.key().to_owned(),
            adapter: index.adapter.id.clone(),
            revision: index.revision.clone(),
            candidates: Vec::new(),
        })
        .collect();
    let tests: Vec<BindingRow> = index
        .test_bindings
        .iter()
        .map(|binding| BindingRow {
            semantic: binding.symbol.clone(),
            kind: "native-test".to_owned(),
            relation: RELATION_VERIFIES,
            native: Some(binding.id.clone()),
            path: Some(binding.path.clone()),
            line: None,
            fingerprint: binding.fingerprint.clone(),
            source: "detected",
            confidence: "high".to_owned(),
            state: binding.state.key().to_owned(),
            adapter: index.adapter.id.clone(),
            revision: index.revision.clone(),
            candidates: Vec::new(),
        })
        .collect();
    let counts = ListCounts {
        bindings: bindings.len(),
        endpoints: endpoints.len(),
        tests: tests.len(),
        explicit: index
            .symbols
            .iter()
            .filter(|r| r.status == BindingStatus::Explicit)
            .count(),
        confirmed: index
            .symbols
            .iter()
            .filter(|r| r.status == BindingStatus::Confirmed)
            .count(),
        inferred: index
            .symbols
            .iter()
            .filter(|r| r.status == BindingStatus::Inferred)
            .count(),
        current: index
            .symbols
            .iter()
            .filter(|r| r.state == BindingState::Current)
            .count(),
        stale: index
            .symbols
            .iter()
            .filter(|r| r.state == BindingState::Stale)
            .count(),
        unknown: index
            .symbols
            .iter()
            .filter(|r| r.state == BindingState::Unknown)
            .count(),
    };
    Ok(ListReceipt {
        status: "valid",
        operation: "bindings",
        action: "list",
        mode: version::MODE,
        project: index.project.clone(),
        target: index.target.clone(),
        profile: index.profile.clone(),
        adapter: index.adapter.id.clone(),
        revision: index.revision.clone(),
        counts,
        bindings,
        endpoints,
        tests,
    })
}
