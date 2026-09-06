//! Transaction-concurrency diagnostics routed through the accepted #11
//! contract (issue #24).
//!
//! Every attachment failure is one registered `transaction.*` or
//! `concurrency.*` rule assembled through the shared registry-backed
//! constructor and finalized into a normalized [`DiagnosticSet`].
//! Every echoed token is bounded before construction: rule data carries
//! only fixed detail tags and declared limits — never raw input, paths,
//! tokens, ETags, or attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal construction input violations.
pub(crate) const INPUT_INVALID: &str = "transaction.input-invalid";
/// The registered rule for a required operation without an atomic group.
pub(crate) const GROUP_MISSING: &str = "transaction.group-missing";
/// The registered rule for duplicate or overlapping group membership.
pub(crate) const GROUP_OVERLAP: &str = "transaction.group-overlap";
/// The registered rule for an external effect inside a local atomic group.
pub(crate) const EXTERNAL_ATOMIC: &str = "transaction.external-atomic";
/// The registered rule for unacknowledged partial failure exposure.
pub(crate) const PARTIAL_UNACKNOWLEDGED: &str = "transaction.partial-unacknowledged";
/// The registered rule for invalid preconditions.
pub(crate) const PRECONDITION_INVALID: &str = "concurrency.precondition-invalid";
/// The registered rule for invalid unique invariants.
pub(crate) const INVARIANT_INVALID: &str = "concurrency.invariant-invalid";
/// The registered rule for invalid concurrency cases.
pub(crate) const CASE_INVALID: &str = "concurrency.case-invalid";
/// The registered rule for missing or unresolved capability requirements.
pub(crate) const CAPABILITY_MISSING: &str = "concurrency.capability-missing";
/// The registered rule for contradictory idempotency/retry declarations.
pub(crate) const RETRY_CONFLICT: &str = "concurrency.retry-conflict";
/// The registered rule for the canonical payload bound.
pub(crate) const EXPORT_LIMIT: &str = "transaction.export-limit";

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

/// The fatal set for one wire or semantic normalization violation. The
/// detail tag is a fixed classification token; the optional subject echo
/// is bounded to the diagnostic token bound before construction.
pub(crate) fn input_invalid(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    rule_invalid(INPUT_INVALID, detail, subject)
}

/// The fatal set for a semantic contract violation with a fixed detail
/// tag and an optional bounded subject echo.
pub(crate) fn rule_invalid(rule: &str, detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let tag = match subject {
        Some(subject) => format!("{detail}:{}", bounded(subject)),
        None => detail.to_owned(),
    };
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(&tag));
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
