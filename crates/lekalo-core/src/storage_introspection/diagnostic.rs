//! Storage-introspection diagnostics routed through the accepted #11
//! contract (issue #117).
//!
//! Every evidence failure is one registered `storage.introspection-*`
//! rule assembled through the shared registry-backed constructor and
//! finalized into a normalized [`DiagnosticSet`]. Every echoed token is
//! bounded before construction: rule data carries only fixed detail
//! tags and declared limits — never raw input, paths, tokens, digests,
//! or attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal evidence input violations.
pub(crate) const INPUT_INVALID: &str = "storage.introspection-invalid";
/// The registered rule for an impossible evidence comparison.
#[allow(dead_code)]
pub(crate) const DIFF_INVALID: &str = "storage.introspection-diff-invalid";

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

/// The fatal set for one evidence comparison refusal.
#[allow(dead_code)]
pub(crate) fn diff_invalid(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(DIFF_INVALID, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// Bound an echoed identifier to the diagnostic token bound.
#[allow(dead_code)]
pub(crate) fn bounded(text: &str) -> String {
    bound_token(text)
}
