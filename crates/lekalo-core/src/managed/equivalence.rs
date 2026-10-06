//! Neutral behavioral-equivalence adjudication (issue #41).
//!
//! The evidence document is producer-owned and independently checked here:
//! any qualified mismatch is a failed gate, any unsupported or missing
//! coverage is an incomplete gate, and only a fully supported matching
//! corpus passes. Timing metrics and raw sources are never part of it.

use super::diagnostic;
use super::wire::{Equivalence, EquivalenceVerdict, ScenarioOutcome};
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

/// Adjudicate one equivalence evidence document.
pub fn adjudicate(bytes: &[u8]) -> DomainResult {
    let evidence: Equivalence = match super::parse_equivalence(bytes) {
        Ok(evidence) => evidence,
        Err(error) => return error,
    };
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
