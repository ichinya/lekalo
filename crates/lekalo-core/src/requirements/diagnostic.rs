//! Requirements diagnostics routed through the accepted #11 contract
//! (issue #36).
//!
//! Every integration failure is one registered `requirements.*` rule
//! assembled through the shared registry-backed constructor and finalized
//! into a normalized [`DiagnosticSet`]. Every echoed token is bounded
//! before construction: rule data carries only fixed detail tags and
//! bounded identifier echoes — never raw input, paths, requirement text,
//! digests, or attacker-controlled content. A registry construction
//! failure collapses the whole set to the registry-invariant set (double
//! developer fault) instead of panicking.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for a wire or semantic attachment violation.
pub(crate) const DOCUMENT_INVALID: &str = "requirements.document-invalid";
/// The registered rule for an invalid provider declaration or tree.
pub(crate) const PROVIDER_INVALID: &str = "requirements.provider-invalid";
/// The registered rule for a reference naming an unknown source or symbol.
pub(crate) const REF_UNKNOWN: &str = "requirements.ref-unknown";
/// The registered rule for a project or Model pin custody mismatch.
pub(crate) const MODEL_REF_MISMATCH: &str = "requirements.model-ref-mismatch";
/// The registered rule for a referenced requirement absent from the
/// resolved set.
pub(crate) const REQUIREMENT_MISSING: &str = "requirements.requirement-missing";
/// The registered rule for a reference whose pin no longer matches.
pub(crate) const REQUIREMENT_STALE: &str = "requirements.requirement-stale";
/// The registered rule for conflicting active changes or duplicate
/// identities.
pub(crate) const REQUIREMENT_CONFLICT: &str = "requirements.requirement-conflict";
/// The registered rule for an absent or unreadable provider tree.
pub(crate) const PROVIDER_UNAVAILABLE: &str = "requirements.provider-unavailable";
/// The registered rule for the bound or canonical payload limit.
pub(crate) const EXPORT_LIMIT: &str = "requirements.export-limit";
/// The registered rule for an empty requested projection.
pub(crate) const PROJECTION_EMPTY: &str = "requirements.projection-empty";

/// Why one diagnostic could not be finalized (collapsed to the invariant
/// set by the caller).
type Built = Result<Diagnostic, BuildError>;

/// Finalize one registered diagnostic; a registry failure surfaces as
/// `Err` and collapses the whole set to the invariant set.
fn one(id: &str, data: DataObject) -> Built {
    build(id, None, None, data)
}

/// Build one validated diagnostic with a fixed detail tag and optional
/// bounded subject echo, or an invariant-collapse marker.
fn diagnostic(id: &str, detail: &str, subject: Option<&str>) -> Result<Diagnostic, ()> {
    let tag = match subject {
        Some(subject) => format!("{detail}:{}", bounded(subject)),
        None => detail.to_owned(),
    };
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(&tag));
    one(id, data).map_err(|_| ())
}

/// Build a validated set of one status or collapse to the invariant set.
fn set(status: Status, diagnostics: Vec<Result<Diagnostic, ()>>) -> DiagnosticSet {
    match diagnostics.into_iter().collect::<Result<Vec<_>, _>>() {
        Ok(built) => DiagnosticSet::try_from_unsorted(built, status)
            .unwrap_or_else(|_| singleton_set("diagnostics.registry-invalid")),
        Err(()) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// One bounded token data value.
fn token(text: &str) -> DataValue {
    token_value(text)
}

pub(crate) fn document_invalid(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(DOCUMENT_INVALID, detail, subject)],
    )
}

/// The fatal `invalid` set for one invalid provider declaration or tree.
pub(crate) fn provider_invalid(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(PROVIDER_INVALID, detail, subject)],
    )
}

/// The fatal `invalid` set for a reference naming an unknown source or
/// symbol.
pub(crate) fn ref_unknown(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(REF_UNKNOWN, detail, subject)],
    )
}

/// The `denied` set for a project or Model pin custody mismatch.
pub(crate) fn model_ref_mismatch(detail: &str) -> DiagnosticSet {
    set(
        Status::Denied,
        vec![diagnostic(MODEL_REF_MISMATCH, detail, None)],
    )
}

/// The aggregated `denied` set of one failed gate: stale references,
/// missing references, and explicit conflicts in canonical order.
/// Returns `None` when every group is empty (the gate passes).
pub(crate) fn gate(
    stale: Vec<String>,
    missing: Vec<String>,
    conflicts: Vec<String>,
) -> Option<DiagnosticSet> {
    if stale.is_empty() && missing.is_empty() && conflicts.is_empty() {
        return None;
    }
    let diagnostics: Vec<Result<Diagnostic, ()>> =
        conflicts
            .iter()
            .map(|subject| diagnostic(REQUIREMENT_CONFLICT, "conflicting-identity", Some(subject)))
            .chain(missing.iter().map(|subject| {
                diagnostic(REQUIREMENT_MISSING, "requirement-absent", Some(subject))
            }))
            .chain(
                stale.iter().map(|subject| {
                    diagnostic(REQUIREMENT_STALE, "revision-mismatch", Some(subject))
                }),
            )
            .collect();
    Some(set(Status::Denied, diagnostics))
}

/// The `unavailable` set for an absent or unreadable provider tree.
pub(crate) fn provider_unavailable(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    set(
        Status::Unavailable,
        vec![diagnostic(PROVIDER_UNAVAILABLE, detail, subject)],
    )
}

/// The fatal `invalid` set for the over-bound document or canonical
/// payload. The limit echo is a fixed `name=value` token.
pub(crate) fn export_limit(detail: &str, limit: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("limit".to_owned(), token(limit));
    let built = one(EXPORT_LIMIT, data).map_err(|_| ());
    set(Status::Invalid, vec![built])
}

/// The fatal `invalid` set for an empty requested projection.
pub(crate) fn projection_empty() -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(PROJECTION_EMPTY, "no-requirement-rows", None)],
    )
}

/// Bound an echoed identifier to the diagnostic token bound.
pub(crate) fn bounded(text: &str) -> String {
    bound_token(text)
}

/// Map an attachment document read failure to the fatal `invalid` set;
/// the detail is a fixed classification token, never an OS error string.
pub fn io_failure(detail: &str) -> DiagnosticSet {
    document_invalid(detail, None)
}
