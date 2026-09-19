//! NFR diagnostics routed through the accepted #11 contract (issue
//! #85).
//!
//! Every integration failure is one registered `nfr.*` rule assembled
//! through the shared registry-backed constructor and finalized into a
//! normalized [`DiagnosticSet`]. Subjects are hashed before
//! construction: rule data carries only fixed detail tags and opaque
//! SHA-256 tokens — never raw input, paths, constraint ids, evidence
//! text, digests, or attacker-controlled content. A registry
//! construction failure collapses the whole set to the
//! registry-invariant set (double developer fault) instead of
//! panicking.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for a wire or semantic attachment violation.
pub const DOCUMENT_INVALID: &str = "nfr.document-invalid";
/// The registered rule for a closed-vocabulary or field-coherence
/// violation inside one constraint.
pub const CONSTRAINT_INVALID: &str = "nfr.constraint-invalid";
/// The registered rule for a scope that does not resolve in the bound
/// IR or names the wrong symbol kind.
pub const SCOPE_UNKNOWN: &str = "nfr.scope-unknown";
/// The registered rule for a project/Model/IR custody mismatch.
pub const MODEL_REF_MISMATCH: &str = "nfr.model-ref-mismatch";
/// The registered rule for an evidence record violating shape or
/// coherence.
pub const EVIDENCE_INVALID: &str = "nfr.evidence-invalid";
/// The registered rule for a required capability absent or below its
/// minimum in the resolved profile.
pub const CAPABILITY_UNSATISFIED: &str = "nfr.capability-unsatisfied";
/// The registered rule for evidence under an environment outside the
/// constraint's accepted set.
pub const ENVIRONMENT_INCOMPATIBLE: &str = "nfr.environment-incompatible";
/// The registered rule for evidence pinned to an older constraint
/// revision or expired against the as-of date.
pub const EVIDENCE_STALE: &str = "nfr.evidence-stale";
/// The registered rule for the aggregated gate denial.
pub const GATE_DENIED: &str = "nfr.gate-denied";
/// The registered rule for the bound or canonical payload limit.
pub const EXPORT_LIMIT: &str = "nfr.export-limit";
/// The registered rule for foreign or mixed-revision diff inputs.
pub const DIFF_INVALID: &str = "nfr.diff-invalid";
/// The registered rule for an empty requested projection.
pub const PROJECTION_EMPTY: &str = "nfr.projection-empty";

/// Why one diagnostic could not be finalized (collapsed to the invariant
/// set by the caller).
type Built = Result<Diagnostic, BuildError>;

/// Finalize one registered diagnostic; a registry failure surfaces as
/// `Err` and collapses the whole set to the invariant set.
fn one(id: &str, data: DataObject) -> Built {
    build(id, None, None, data)
}

/// Build one validated diagnostic with a fixed detail tag and optional
/// opaque subject digest, or an invariant-collapse marker.
fn diagnostic(id: &str, detail: &str, subject: Option<&str>) -> Result<Diagnostic, ()> {
    let tag = match subject {
        Some(subject) => format!(
            "{detail}:subject-{}",
            super::canonical::sha256_hex(subject.as_bytes())
        ),
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

/// The fatal `invalid` set for one wire or semantic attachment
/// violation on either document.
pub fn document_invalid(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(DOCUMENT_INVALID, detail, subject)],
    )
}

/// The fatal `invalid` set for one invalid constraint.
pub fn constraint_invalid(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(CONSTRAINT_INVALID, detail, subject)],
    )
}

/// The fatal `invalid` set for an unresolvable or wrong-kind scope.
pub fn scope_unknown(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(SCOPE_UNKNOWN, detail, subject)],
    )
}

/// The `denied` set for a project/Model/IR custody mismatch.
pub fn model_ref_mismatch(detail: &str) -> DiagnosticSet {
    set(
        Status::Denied,
        vec![diagnostic(MODEL_REF_MISMATCH, detail, None)],
    )
}

/// The fatal `invalid` set for one invalid evidence record.
pub fn evidence_invalid(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(EVIDENCE_INVALID, detail, subject)],
    )
}

/// One non-blocking warning about an unsatisfied capability
/// requirement. `None` is the registry-invariant collapse marker.
pub fn capability_unsatisfied(detail: &str, subject: &str) -> Option<Diagnostic> {
    diagnostic(CAPABILITY_UNSATISFIED, detail, Some(subject)).ok()
}

/// One non-blocking warning about foreign-environment evidence.
/// `None` is the registry-invariant collapse marker.
pub fn environment_incompatible(detail: &str, subject: &str) -> Option<Diagnostic> {
    diagnostic(ENVIRONMENT_INCOMPATIBLE, detail, Some(subject)).ok()
}

/// One non-blocking warning about stale or expired evidence. `None`
/// is the registry-invariant collapse marker.
pub fn evidence_stale(detail: &str, subject: &str) -> Option<Diagnostic> {
    diagnostic(EVIDENCE_STALE, detail, Some(subject)).ok()
}

/// The aggregated `denied` set of the failed NFR gate: one diagnostic
/// per denied constraint row in canonical order. Returns `None` when
/// the gate passes.
pub fn gate(denied: Vec<(String, &'static str)>) -> Option<DiagnosticSet> {
    if denied.is_empty() {
        return None;
    }
    let diagnostics: Vec<Result<Diagnostic, ()>> = denied
        .into_iter()
        .map(|(subject, status)| diagnostic(GATE_DENIED, status, Some(&subject)))
        .collect();
    Some(set(Status::Denied, diagnostics))
}

/// The fatal `invalid` set for the over-bound document or canonical
/// payload. The limit echo is a fixed `name=value` token.
pub fn export_limit(detail: &str, limit: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("limit".to_owned(), token(limit));
    let built = one(EXPORT_LIMIT, data).map_err(|_| ());
    set(Status::Invalid, vec![built])
}

/// The fatal `invalid` set for foreign or mixed-revision diff inputs.
pub fn diff_invalid(detail: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(DIFF_INVALID, detail, None)],
    )
}

/// The fatal `invalid` set for an empty requested projection.
pub fn projection_empty() -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(PROJECTION_EMPTY, "no-constraint-rows", None)],
    )
}

/// Map a document read failure to the fatal `invalid` set; the detail
/// is a fixed classification token, never an OS error string.
pub fn io_failure(detail: &str) -> DiagnosticSet {
    document_invalid(detail, None)
}
