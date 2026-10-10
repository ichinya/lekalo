//! Neutral behavioral-equivalence adjudication (issue #41).
//!
//! The evidence document is producer-owned and independently checked here
//! against the exact live generation: the implementation pin must be the
//! digest of the **live** generated implementation bytes of the committed
//! record (recomputed at adjudication, so any manual drift fails the gate —
//! finding 5), every scenario pin must be the exact retained executable
//! scenario of the requirement it names — the generated behavioral test
//! that invokes the pinned handler on the pinned sample and asserts the
//! pinned outcome, retained as a custody object — and the scenario corpus
//! must cover every rendered requirement of the committed record. Any
//! qualified mismatch is a failed gate. Unsupported or missing coverage is
//! an incomplete gate, and only a fully supported matching corpus passes.
//! Timing metrics and raw sources are never part of it. The gate itself
//! never executes anything: it certifies that the retained executable
//! scenario corpus is complete, current, and bound to the live bytes — the
//! execution happens in the producer's own suite over exactly these
//! scenarios.

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

/// The digest over the **live** implementation inventory of one module:
/// the ordered concatenation of the exact bytes of every implementation/
/// test output of the current generation for the evidence subject, read
/// from the live project at adjudication time. Drifted, missing, or
/// unreadable live bytes fail the gate instead of matching a stale pin
/// (finding 5): retained-record existence is not behavioral evidence.
fn live_implementation_digest(
    root: &std::path::Path,
    record: &super::wire::GenerationRecord,
    module: &str,
    target: &str,
) -> Option<String> {
    let mut live = Vec::new();
    for output in &record.outputs {
        if output.owner_module != module
            || output.owner_target != target
            || !matches!(
                output.role,
                ArtifactRole::Implementation | ArtifactRole::Test
            )
        {
            continue;
        }
        let bytes = super::fs::read_file(root, &output.path).ok()??;
        live.push(output.path.clone());
        live.push(super::digest(&bytes));
    }
    if live.is_empty() {
        return None;
    }
    Some(super::digest(live.join("\n").as_bytes()))
}

/// Test-visible re-export of the live inventory digest computation.
#[cfg(test)]
pub(crate) fn live_implementation_digest_for_test(
    root: &std::path::Path,
    record: &super::wire::GenerationRecord,
    module: &str,
    target: &str,
) -> Option<String> {
    live_implementation_digest(root, record, module, target)
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
    // The evidence is only ever about the exact live implementation: the
    // caller-provided pin must equal the digest of the live generated
    // bytes (recomputed here, finding 5) — a retained-record pin that no
    // longer describes the tree fails instead of matching.
    match live_implementation_digest(root, &record, &evidence.module, &evidence.target) {
        Some(digest) => {
            if digest != evidence.implementation_digest {
                return diagnostic::failure(EQUIVALENCE_FAILED, &detail("stale-evidence"));
            }
        }
        None => return invalid("custody"),
    }
    // The required scenario corpus: every rendered requirement of the
    // committed record for the subject must appear as a scenario, and
    // every scenario's pinned digest must be the exact retained executable
    // scenario of that requirement (finding 5): the generated behavioral
    // test that invokes the pinned handler on the pinned sample. Any other
    // retained object — a source file, a foreign snapshot, a zero hash —
    // is not scenario evidence.
    let mut required: Vec<(String, String)> = record
        .outputs
        .iter()
        .filter(|output| {
            output.owner_module == evidence.module
                && output.owner_target == evidence.target
                && output.role == ArtifactRole::Test
        })
        .filter_map(|output| {
            let name = output
                .path
                .rsplit('/')
                .next()
                .and_then(|file| file.strip_suffix(".test.ts"))?
                .to_owned();
            if name.is_empty() {
                return None;
            }
            Some((
                format!("{}.{}", evidence.module, name.replace('-', "_")),
                output.digest.clone(),
            ))
        })
        .collect();
    required.sort();
    if required.is_empty() {
        return invalid("custody");
    }
    for (scenario, scenario_digest) in &required {
        let Some(row) = evidence
            .scenarios
            .iter()
            .find(|row| &row.scenario == scenario)
        else {
            return diagnostic::failure(EQUIVALENCE_INCOMPLETE, &detail("corpus"));
        };
        // The pin must be the scenario's own retained evidence: the exact
        // digest of the generated test bytes (whose custody object the
        // record verification already re-read and re-hashed).
        if row.digest != *scenario_digest {
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
