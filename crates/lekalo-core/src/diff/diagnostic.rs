//! Semantic-diff diagnostics routed through the accepted #11 contract
//! (issue #18).
//!
//! Every diff failure is one registered `diff.*` rule assembled through
//! the shared registry-backed constructor and finalized into a
//! normalized [`DiagnosticSet`].
//! Every echoed token is bounded before construction: rule data carries
//! only fixed detail tags, declared limits, and byte counts — never raw
//! input, paths, or attacker-controlled text.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::Diagnostic;
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

/// The registered rule for fatal construction input violations.
pub(crate) const INPUT_INVALID: &str = "diff.input-invalid";
/// The registered rule for a malformed profile reference.
pub(crate) const PROFILE_INVALID: &str = "diff.profile-invalid";
/// The registered rule for a refused adapter contribution envelope.
pub(crate) const ADAPTER_INVALID: &str = "diff.adapter-invalid";
/// The registered rule for the compared-subject bound.
pub(crate) const SUBJECT_LIMIT: &str = "diff.subject-limit";
/// The registered rule for the affected-seed bound.
pub(crate) const SEED_LIMIT: &str = "diff.seed-limit";
/// The registered rule for the canonical-result byte bound.
pub(crate) const EXPORT_LIMIT: &str = "diff.export-limit";

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
) -> Result<Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, symbol, None, data)
}

/// Build a validated `invalid` set or collapse to the invariant set.
pub(crate) fn invalid_set(diagnostics: Vec<Diagnostic>) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, Status::Invalid)
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// One bounded token data value.
fn token(text: &str) -> DataValue {
    token_value(text)
}

/// The fatal set for a mismatched base/candidate contract family.
pub(crate) fn family_mismatch_set(detail: &str, echoed: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("subject".to_owned(), token(&bounded(echoed)));
    match one(INPUT_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a refused adapter envelope.
pub(crate) fn adapter_invalid_set(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("subject".to_owned(), token(""));
    match one(ADAPTER_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a malformed profile term.
pub(crate) fn profile_invalid_set(echoed: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("unknown-profile"));
    data.insert("subject".to_owned(), token(&bounded(echoed)));
    match one(PROFILE_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one crossed bound.
pub(crate) fn limit_set(rule: &str, limit: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("bound-exceeded"));
    data.insert("limit".to_owned(), token(limit));
    match one(rule, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for an over-bound canonical export.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("bytes".to_owned(), DataValue::Count(bytes as u64));
    data.insert("detail".to_owned(), token("export-bound"));
    match one(EXPORT_LIMIT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The bounded echo the #11 wire carries for located rules.
pub(crate) fn bounded(text: &str) -> String {
    bound_token(text)
}
