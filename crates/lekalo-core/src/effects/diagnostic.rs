//! Effect-graph diagnostics routed through the accepted #11 contract
//! (issue #14).
//!
//! Every effect-graph failure is one registered rule assembled through
//! the shared registry-backed constructor and finalized into a
//! normalized [`DiagnosticSet`]. The effect contract adds no rules of its
//! own in v1 (the registry file is unchanged by design): construction and
//! limit failures reuse the graph-family infrastructure rules
//! (`graph.input-invalid`, `graph.traversal-limit`, `graph.export-limit`)
//! and unknown-subject queries reuse `graph.unknown-node`. Declared versus
//! detected mismatches are not diagnostics at all — they are closed
//! comparison states in the result data with stable explanation strings.
//! Every echoed token is bounded (the #11 review invariant).

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::Diagnostic;
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

use super::provenance::EnvelopeViolation;

/// The registered rule reused for fatal construction input violations.
pub(crate) const INPUT_INVALID: &str = "graph.input-invalid";
/// The registered rule reused for a query naming an unknown subject.
pub(crate) const UNKNOWN_NODE: &str = "graph.unknown-node";
/// The registered rule reused for a bound-exhausted query.
pub(crate) const TRAVERSAL_LIMIT: &str = "graph.traversal-limit";
/// The registered rule reused for an over-bound canonical export.
pub(crate) const EXPORT_LIMIT: &str = "graph.export-limit";

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
pub(crate) fn one(
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

/// The bounded echo the #11 wire carries for located rules.
pub(crate) fn bounded(text: &str) -> String {
    bound_token(text)
}

/// One bounded token data value.
pub(crate) fn token(text: &str) -> DataValue {
    token_value(text)
}

/// The fatal set for a single detail-tagged construction violation.
pub(crate) fn input_invalid_detail(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(INPUT_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one evidence-envelope violation.
pub(crate) fn envelope_violation_set(violation: &EnvelopeViolation) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(&violation.detail()));
    let name = match violation {
        EnvelopeViolation::MalformedField { .. } => "malformed-field",
        EnvelopeViolation::MalformedDigest { .. } => "malformed-digest",
        EnvelopeViolation::OverLimit => "over-limit",
        EnvelopeViolation::IllegalEntry { .. } => "illegal-entry",
    };
    data.insert("violation".to_owned(), token(name));
    match one(INPUT_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a construction cap violation.
pub(crate) fn cap_exceeded_set(cap: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(cap));
    match one(INPUT_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a query naming an unknown operation or resource.
pub(crate) fn unknown_subject_set(subject: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("unknown-subject"));
    match one(UNKNOWN_NODE, Some(bounded(subject)), data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a query that crossed a recorded bound.
pub(crate) fn traversal_limit_set(limit: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("limit".to_owned(), token(limit));
    match one(TRAVERSAL_LIMIT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for an over-bound canonical export.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("bytes".to_owned(), DataValue::Count(bytes as u64));
    match one(EXPORT_LIMIT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}
