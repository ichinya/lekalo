//! Extended-effects diagnostics routed through the accepted #11 contract
//! (issue #26).
//!
//! Every attachment failure is one registered `extended.*`, `event.*`,
//! `job.*`, `call.*`, `cache.*`, `publication.*`, `contract.*`, or
//! `case.*` rule assembled through the shared registry-backed
//! constructor and finalized into a normalized [`DiagnosticSet`]. Every
//! echoed token is bounded before construction: rule data carries only
//! fixed detail tags and declared limits — never raw input, paths,
//! tokens, digests, or attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal construction input violations.
pub(crate) const INPUT_INVALID: &str = "extended.input-invalid";
/// The registered rule for an event contract semantic violation.
pub(crate) const EVENT_INVALID: &str = "event.contract-invalid";
/// The registered rule for a job contract semantic violation.
pub(crate) const JOB_INVALID: &str = "job.contract-invalid";
/// The registered rule for an external-call contract semantic violation.
pub(crate) const CALL_INVALID: &str = "call.contract-invalid";
/// The registered rule for a cache contract semantic violation.
pub(crate) const CACHE_INVALID: &str = "cache.contract-invalid";
/// The registered rule for a publication without its explicit opt-in or
/// approval contract.
pub(crate) const CONSENT_MISSING: &str = "publication.consent-missing";
/// The registered rule for missing or unresolved capability requirements.
pub(crate) const CAPABILITY_MISSING: &str = "contract.capability-missing";
/// The registered rule for contradictory retry and idempotency
/// declarations.
pub(crate) const RETRY_CONFLICT: &str = "contract.retry-conflict";
/// The registered rule for invalid partial-failure cases.
pub(crate) const CASE_INVALID: &str = "case.case-invalid";
/// The registered rule for a sensitive effect without its security
/// review gate hint.
pub(crate) const GATE_REQUIRED: &str = "contract.gate-required";
/// The registered rule for the canonical payload bound.
pub(crate) const EXPORT_LIMIT: &str = "extended.export-limit";

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
