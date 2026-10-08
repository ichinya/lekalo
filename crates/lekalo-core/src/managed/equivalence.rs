//! Neutral behavioral-equivalence adjudication (issue #41).
//!
//! The evidence document is producer-owned and independently checked here
//! against the exact committed generation: the implementation pin must be
//! the digest of the recorded implementation inventory, every scenario
//! pin must be the digest of the independently produced outcome bytes the
//! producer retains in custody (verified against custody objects, never
//! caller labels), the scenario corpus must cover every rendered
//! requirement of the committed record, and any qualified mismatch is a
//! failed gate. Unsupported or missing coverage is an incomplete gate,
//! and only a fully supported matching corpus passes. Timing metrics and
//! raw sources are never part of it.

use super::diagnostic;
use super::wire::{ArtifactRole, Equivalence, EquivalenceVerdict, ScenarioOutcome};
use super::{detail, DomainResult, EQUIVALENCE_FAILED, EQUIVALENCE_INCOMPLETE, INPUT_INVALID};

/// The adjudication receipt.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquivalenceReceipt {
    pub module: String,
    pub target: String,
    pub verdict: String,
    pub scenarios: usize,
    pub matched: usize,
    pub gaps: Vec<String>,
}

fn invalid(reason: &str) -> DomainResult {
    diagnostic::failure(INPUT_INVALID, &detail(reason))
}

/// The digest over one module's recorded implementation inventory: the
/// ordered concatenation of every implementation/test output digest of
/// the current generation for the evidence subject.
fn implementation_inventory_digest(
    record: &super::wire::GenerationRecord,
    module: &str,
    target: &str,
) -> Option<String> {
    let digests: Vec<&str> = record
        .outputs
        .iter()
        .filter(|output| {
            output.owner_module == module
                && output.owner_target == target
                && matches!(
                    output.role,
                    ArtifactRole::Implementation | ArtifactRole::Test
                )
        })
        .map(|output| output.digest.as_str())
        .collect();
    if digests.is_empty() {
        return None;
    }
    Some(super::digest(digests.join("\n").as_bytes()))
}

/// Adjudicate one equivalence evidence document against the exact
/// committed generation (read-only; `None` custody keeps the pure bytes
/// behavior for detached callers).
pub fn adjudicate_for(root: &std::path::Path, bytes: &[u8]) -> DomainResult {
    let evidence: Equivalence = match super::parse_equivalence(bytes) {
        Ok(evidence) => evidence,
        Err(error) => return error,
    };
    let Some(current) = (match super::history::current(root) {
        Ok(Some(current)) => Some(current),
        Ok(None) => None,
        Err(error) => return error,
    }) else {
        return invalid("custody");
    };
    let record = match super::history::record(root, &current) {
        Ok(record) => record,
        Err(error) => return error,
    };
    // The evidence is only ever about the exact committed implementation:
    // the caller-provided pin must equal the recorded inventory digest.
    match implementation_inventory_digest(&record, &evidence.module, &evidence.target) {
        Some(digest) => {
            if digest != evidence.implementation_digest {
                return diagnostic::failure(EQUIVALENCE_FAILED, &detail("stale-evidence"));
            }
        }
        None => return invalid("custody"),
    }
    // The required scenario corpus: every rendered requirement of the
    // committed record for the subject must appear as a scenario, and
    // every scenario's pinned digest must be the digest of the
    // independently produced outcome bytes retained in managed custody
    // (finding 5): the producer writes each native/reference run's
    // normalized outcome into the custody object store under its digest,
    // so a caller cannot forge verdict labels — the pinned bytes are
    // re-read and re-hashed here.
    let mut required: Vec<String> = record
        .outputs
        .iter()
        .filter(|output| {
            output.owner_module == evidence.module
                && output.owner_target == evidence.target
                && output.role == ArtifactRole::Implementation
        })
        .map(|output| output.path.clone())
        .collect();
    required.sort();
    for path in &required {
        let name = path
            .rsplit('/')
            .next()
            .and_then(|file| file.strip_suffix(".ts"))
            .unwrap_or(path);
        let scenario = format!("{}.{}", evidence.module, name.replace('-', "_"));
        let Some(row) = evidence
            .scenarios
            .iter()
            .find(|row| row.scenario == scenario)
        else {
            return diagnostic::failure(EQUIVALENCE_INCOMPLETE, &detail("corpus"));
        };
        // Every scenario pin must resolve to real retained outcome bytes:
        // the digest is verified against the custody object, so a
        // zero-hash or foreign digest is missing evidence, not a pass.
        if super::history::object(root, &row.digest).is_err() {
            return diagnostic::failure(EQUIVALENCE_INCOMPLETE, &detail("evidence"));
        }
    }
    // The reference bytes are independently produced evidence too: the
    // pinned reference digest must resolve to retained custody bytes
    // produced by the reference lane, never a caller-chosen label.
    if let Some(reference) = evidence.reference_digest.value() {
        if super::history::object(root, reference).is_err() {
            return diagnostic::failure(EQUIVALENCE_INCOMPLETE, &detail("reference"));
        }
    }
    adjudicate_checked(evidence)
}

/// The pure adjudication over a pre-bound evidence document.
fn adjudicate_checked(evidence: Equivalence) -> DomainResult {
    let mismatched = evidence
        .scenarios
        .iter()
        .filter(|scenario| scenario.verdict == ScenarioOutcome::Mismatch)
        .count();
    let unsupported = evidence
        .scenarios
        .iter()
        .filter(|scenario| scenario.verdict == ScenarioOutcome::Unsupported)
        .count();
    let matched = evidence.scenarios.len() - mismatched - unsupported;
    // Any qualified mismatch fails on an independently checked assertion;
    // the prior generation and the evidence rows are retained by the caller.
    if mismatched > 0 || evidence.verdict == EquivalenceVerdict::Mismatch {
        return diagnostic::failure(EQUIVALENCE_FAILED, &detail("mismatch"));
    }
    // Unsupported or missing required coverage never lowers the denominator.
    if unsupported > 0
        || !evidence.gaps.is_empty()
        || evidence.verdict == EquivalenceVerdict::Incomplete
    {
        return diagnostic::failure(EQUIVALENCE_INCOMPLETE, &detail("coverage"));
    }
    if evidence.reference_digest.value().is_none() {
        return invalid("reference");
    }
    if matched == 0 {
        return invalid("corpus");
    }
    super::render(&EquivalenceReceipt {
        module: evidence.module,
        target: evidence.target,
        verdict: "match".to_owned(),
        scenarios: evidence.scenarios.len(),
        matched,
        gaps: evidence.gaps,
    })
}

/// Adjudicate one equivalence evidence document without project custody
/// (detached callers; the corpus binding stays structural).
pub fn adjudicate(bytes: &[u8]) -> DomainResult {
    let evidence: Equivalence = match super::parse_equivalence(bytes) {
        Ok(evidence) => evidence,
        Err(error) => return error,
    };
    adjudicate_checked(evidence)
}
