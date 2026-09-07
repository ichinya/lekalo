//! Invariant-transition diagnostics routed through the accepted #11
//! contract (issue #63).
//!
//! Every attachment failure is one registered `invariant.*` rule
//! assembled through the shared registry-backed constructor and
//! finalized into a normalized [`DiagnosticSet`]. Every echoed token
//! is bounded before construction: rule data carries only fixed detail
//! tags and declared limits — never raw input, paths, tokens, digests,
//! or attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal construction input violations.
pub(crate) const INPUT_INVALID: &str = "invariant.input-invalid";
/// The registered rule for a malformed or incomplete state space.
pub(crate) const STATE_INVALID: &str = "invariant.state-invalid";
/// The registered rule for a transition semantic violation.
pub(crate) const TRANSITION_INVALID: &str = "invariant.transition-invalid";
/// The registered rule for a contradictory verification mapping.
pub(crate) const MAPPING_INVALID: &str = "invariant.mapping-invalid";
/// The registered rule for a transition-graph rule violation.
pub(crate) const GRAPH_INVALID: &str = "invariant.graph-invalid";
/// The registered rule for an invariant whose members do not match
/// its kind contract.
pub(crate) const CONTRACT_INVALID: &str = "invariant.contract-invalid";
/// The registered rule for the canonical payload bound.
pub(crate) const EXPORT_LIMIT: &str = "invariant.export-limit";
/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(id: &str, data: DataObject) -> Result<Diagnostic, BuildError> {
    build(id, None, None, data)
}

/// Build a validated `invalid` set or collapse to the invariant set.
pub(crate) fn invalid_set(diagnostics: Vec<Diagnostic>) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, Status::Invalid)
        .unwrap_or_else(|_| singleton_set("diagnostics.registry-invalid"))
}

/// One bounded token data value.
fn token(text: &str) -> DataValue {
    token_value(text)
}

/// The fatal set for one wire normalization violation. The detail tag
/// is a fixed classification token with no subject echo.
pub(crate) fn input_invalid(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(INPUT_INVALID, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one semantic record violation with a fixed detail
/// tag and an optional bounded subject echo. `rule` selects the
/// registered state, transition, or mapping rule.
pub(crate) fn rule_invalid(rule: &str, detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(subject) = subject {
        data.insert("subject".to_owned(), token(&bounded(subject)));
    }
    match one(rule, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for the over-bound canonical payload.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("canonical-bytes"));
    data.insert("limit".to_owned(), token(&format!("bytes={bytes}")));
    match one(EXPORT_LIMIT, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// Bound an echoed identifier to the diagnostic token bound.
pub(crate) fn bounded(text: &str) -> String {
    bound_token(text)
}
