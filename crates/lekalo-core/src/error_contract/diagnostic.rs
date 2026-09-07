//! Error-contract diagnostics routed through the accepted #11 contract
//! (issue #62).
//!
//! Every error-contract failure is one registered `error.*` rule assembled
//! through the shared registry-backed constructor and finalized into a
//! normalized [`DiagnosticSet`]. Every echoed token is bounded before
//! construction: rule data carries only fixed detail tags and bounded
//! ids — never raw input, paths, provider text, or attacker-controlled
//! echoes.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::Status;

/// The registered rule for a malformed contract or source invariant.
pub(crate) const CONTRACT_INVALID: &str = "error.contract-invalid";
/// The registered rule for an operation binding that mismatches the IR.
pub(crate) const BINDING_INVALID: &str = "error.binding-invalid";
/// The registered rule for payload schema or value violations.
pub(crate) const PAYLOAD_INVALID: &str = "error.payload-invalid";
/// The registered rule for an error unreachable from every binding.
pub(crate) const UNREACHABLE: &str = "error.unreachable";
/// The registered rule for a code collision with a live entry or tombstone.
pub(crate) const CODE_REUSED: &str = "error.code-reused";
/// The registered rule for contradictory retry/idempotency/effect metadata.
pub(crate) const RETRY_IDEMPOTENCY_CONFLICT: &str = "error.retry-idempotency-conflict";
/// The registered rule for missing or contradictory coverage.
pub(crate) const COVERAGE_INVALID: &str = "error.coverage-invalid";
/// The registered rule for a strict mapping that misses a union member.
pub(crate) const MAPPING_MISSING: &str = "error.mapping-missing";
/// The registered rule for a mapping that violates projection invariants.
pub(crate) const MAPPING_INVALID: &str = "error.mapping-invalid";
/// The registered rule for an ambiguous revision comparison.
pub(crate) const DIFF_INVALID: &str = "error.diff-invalid";
/// The registered rule for one crossed bound.
pub(crate) const LIMIT_EXCEEDED: &str = "error.limit-exceeded";
/// The registered rule for registry invariant violations.
pub(crate) const REGISTRY_INVALID: &str = "error.registry-invalid";

/// Build one registered diagnostic without finalizing a set, for
/// validators that collect several findings.
pub(crate) fn finding(id: &str, data: DataObject) -> Option<Diagnostic> {
    one(id, None, data).ok()
}

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

/// The fatal set for a malformed contract or crossed source invariant.
pub(crate) fn contract_invalid_set(detail: &str, subject: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("subject".to_owned(), token(&bounded(subject)));
    match one(CONTRACT_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a binding that mismatches the accepted IR.
pub(crate) fn binding_invalid_set(detail: &str, subject: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("subject".to_owned(), token(&bounded(subject)));
    match one(BINDING_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one payload violation.
pub(crate) fn payload_invalid_set(detail: &str, field: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("field".to_owned(), token(&bounded(field)));
    match one(PAYLOAD_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one code collision.
pub(crate) fn code_reused_set(detail: &str, code: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("code".to_owned(), token(&bounded(code)));
    match one(CODE_REUSED, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for contradictory retry/idempotency/effect metadata.
pub(crate) fn retry_conflict_set(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(RETRY_IDEMPOTENCY_CONFLICT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for missing or contradictory coverage.
pub(crate) fn coverage_invalid_set(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(COVERAGE_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a strict mapping that misses a union member.
pub(crate) fn mapping_missing_set(target: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("union-member-unmapped"));
    data.insert("target".to_owned(), token(&bounded(target)));
    match one(MAPPING_MISSING, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a mapping that violates the projection invariants.
pub(crate) fn mapping_invalid_set(detail: &str, target: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("target".to_owned(), token(&bounded(target)));
    match one(MAPPING_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for an ambiguous revision comparison.
pub(crate) fn diff_invalid_set(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(DIFF_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one crossed bound.
pub(crate) fn limit_exceeded_set(detail: &'static str, limit: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("limit".to_owned(), token(limit));
    match one(LIMIT_EXCEEDED, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one registry invariant violation.
pub(crate) fn registry_invalid_set(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(REGISTRY_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// Bound an echoed token before it enters diagnostic data.
fn bounded(text: &str) -> String {
    crate::diagnostics::types::bound_token(text)
}
