//! Scenario IR diagnostics routed through the accepted #11 contract
//! (issue #23).
//!
//! Every scenario failure is one registered rule assembled through the
//! shared registry-backed constructor and finalized into a normalized
//! [`DiagnosticSet`]. The scenario contract adds no rules of its own in
//! v1 (the registry file is unchanged by design): normalization, limit,
//! and data-flow failures reuse the graph-family infrastructure rules
//! (`graph.input-invalid`, `graph.traversal-limit`, `graph.export-limit`).
//! The `detail` field is always a fixed class token; the optional `role`
//! field is the located member, bounded and control-cleaned, and only
//! echoed after its own grammar check. Attacker-controlled text never
//! scales the diagnostic envelope.

use crate::diagnostics::normalize::build;
use crate::diagnostics::{DataObject, DataValue, Diagnostic, DiagnosticSet};

/// The registered rule reused for fatal normalization violations.
pub(crate) const INPUT_INVALID: &str = "graph.input-invalid";
/// The registered rule reused for a construction bound exhaustion.
pub(crate) const TRAVERSAL_LIMIT: &str = "graph.traversal-limit";
/// The registered rule reused for an over-bound canonical payload.
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
    DiagnosticSet::try_from_unsorted(diagnostics, crate::result::Status::Invalid)
        .unwrap_or_else(|_| crate::result::fallback_set())
}

/// One bounded, control-cleaned echo of a located member name.
pub(crate) fn token(text: &str) -> DataValue {
    crate::diagnostics::types::token_value(text)
}

/// The fatal set for one normalization violation: a fixed class token
/// plus the bounded, grammar-checked member the violation was found at.
pub(crate) fn input_invalid(detail: &str, role: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(role) = role {
        data.insert("role".to_owned(), token(role));
    }
    match one(INPUT_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a construction bound exhaustion.
pub(crate) fn limit_set(limit: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(limit));
    match one(TRAVERSAL_LIMIT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for an over-bound canonical payload.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("canonical-bytes"));
    data.insert("bytes".to_owned(), DataValue::Count(bytes as u64));
    match one(EXPORT_LIMIT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}
