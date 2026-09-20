//! Data-flow report diagnostics routed through the accepted #11
//! contract (issue #87).
//!
//! The derived report itself is always constructible (findings carry
//! the rule violations); this module covers the wire-level refusals of
//! the report surface: bound violations, malformed re-read documents,
//! and the canonical export limit. Every refusal is one registered
//! `classification.*` wire rule assembled through the shared
//! registry-backed constructor; a registry failure collapses to the
//! invariant set instead of panicking.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::DiagnosticSet;

/// The registered wire rule for a report document violation (the
/// classification family owns the shared wire vocabulary of the #87
/// attachments).
const DOCUMENT_INVALID: &str = "classification.unknown-kind";

/// Finalize one registered diagnostic; a registry failure surfaces as
/// `Err` and collapses the whole set to the invariant set.
fn one(data: DataObject) -> Result<crate::diagnostics::Diagnostic, BuildError> {
    build(DOCUMENT_INVALID, None, None, data)
}

/// One `detail`-tagged refusal set.
pub fn document_invalid(detail: &str, _subject: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(&format!("report-{detail}")));
    match one(data) {
        Ok(built) => DiagnosticSet::try_from_unsorted(vec![built], crate::result::Status::Invalid)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The closed token value spelling.
fn token(tag: &str) -> DataValue {
    token_value(tag)
}
