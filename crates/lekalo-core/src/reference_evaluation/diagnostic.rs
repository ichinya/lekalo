//! Reference-evaluation diagnostics routed through the accepted #11
//! contract (issue #107).
//!
//! The reference evaluator adds no rules of its own in v1 (the
//! diagnostic registry file is unchanged by design, exactly like the
//! Scenario IR before it): the only fatal conditions — an over-bound
//! canonical trace and an over-bound effect log — reuse the
//! graph-family infrastructure rules (`graph.export-limit`,
//! `graph.traversal-limit`) with fixed detail tokens. Every
//! evaluation outcome inside a trace is closed typed data (`ok`,
//! `error`, `unsupported` with a fixed reason token), never a
//! diagnostic: a scenario whose expectations do not hold is a `fail`
//! verdict, not an invalid input.

use crate::diagnostics::normalize::build;
use crate::diagnostics::{DataObject, DataValue, Diagnostic, DiagnosticSet};

/// The registered rule reused for an over-bound canonical trace.
pub(crate) const EXPORT_LIMIT: &str = "graph.export-limit";

/// The registered rule reused for a construction bound exhaustion.
pub(crate) const TRAVERSAL_LIMIT: &str = "graph.traversal-limit";

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
pub(crate) fn one(
    id: &str,
    data: DataObject,
) -> Result<Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, None, None, data)
}

/// Build a validated `invalid` set or collapse to the invariant set.
pub(crate) fn invalid_set(diagnostics: Vec<Diagnostic>) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, crate::result::Status::Invalid)
        .unwrap_or_else(|_| crate::result::fallback_set())
}

/// One bounded, control-cleaned echo of a located member name.
pub(crate) fn token(text: &str) -> DataValue {
    crate::diagnostics::types::token_value(text)
}

/// The fatal set for a construction bound exhaustion.
pub(crate) fn limit_set(limit: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(limit));
    match one(TRAVERSAL_LIMIT, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for an over-bound canonical payload.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("canonical-bytes"));
    data.insert("bytes".to_owned(), DataValue::Count(bytes as u64));
    match one(EXPORT_LIMIT, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}
