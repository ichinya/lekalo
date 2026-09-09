//! The conformance suite → diagnostic wire adapter (issue #31).
//!
//! Maps one failed check onto its registered `adapter.*` rule with the
//! check id, class, and bounded detail token. The verdict — and with it
//! the exit class — is owned by the report; a diagnostic never computes
//! an exit. Every echoed token passes the bounded-token invariant; no
//! host path, child output, or adapter text enters a diagnostic item.

use super::check::{CheckClass, CheckOutcome, CheckState, Verdict};
use crate::diagnostics::normalize::build;
use crate::diagnostics::types::token_value;
use crate::diagnostics::{DataObject, Diagnostic, DiagnosticSet};

/// The registered rule id of one failed check, by class.
fn rule_for(class: CheckClass) -> &'static str {
    match class {
        CheckClass::Security => "adapter.security-failure",
        CheckClass::Protocol => "adapter.protocol-failure",
        CheckClass::Process => "adapter.run-failure",
        CheckClass::Feature | CheckClass::Determinism => "adapter.check-failed",
    }
}

/// Build one wire diagnostic for a failed check.
fn one(outcome: &CheckOutcome) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    data.insert("check".to_owned(), token_value(outcome.id.as_str()));
    data.insert("class".to_owned(), token_value(outcome.class.as_str()));
    if let Some(detail) = outcome.detail {
        data.insert("detail".to_owned(), token_value(detail));
    }
    build(rule_for(outcome.class), None, None, data).ok()
}

/// Project the run onto the diagnostic set: one diagnostic per failed
/// check, normalized order.
pub fn diagnostic_set(verdict: Verdict, outcomes: &[CheckOutcome]) -> DiagnosticSet {
    let diagnostics: Vec<Diagnostic> = outcomes
        .iter()
        .filter(|outcome| outcome.state == CheckState::Fail)
        .filter_map(one)
        .collect();
    DiagnosticSet::try_from_unsorted(diagnostics, verdict.status())
        .unwrap_or_else(|_| DiagnosticSet::empty())
}
