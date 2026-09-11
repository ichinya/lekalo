//! Query-model diagnostics routed through the accepted #11 contract
//! (issue #64).
//!
//! Every attachment failure is one registered `query.*` rule assembled
//! through the shared registry-backed constructor and finalized into a
//! normalized [`DiagnosticSet`]. Every echoed token is bounded before
//! construction: rule data carries only fixed detail tags and declared
//! limits — never raw input, paths, tokens, digests, or
//! attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal wire input violations.
pub(crate) const INPUT_INVALID: &str = "query.input-invalid";
/// The registered rule for a query declaration that violates its own
/// closed contract.
pub(crate) const CONTRACT_INVALID: &str = "query.contract-invalid";
/// The registered rule for an unreadable, non-entity, or write-surface
/// source.
pub(crate) const SOURCE_INVALID: &str = "query.source-invalid";
/// The registered rule for a filter outside the closed grammar or the
/// bound field types.
pub(crate) const FILTER_INVALID: &str = "query.filter-invalid";
/// The registered rule for a sort without a deterministic total order.
pub(crate) const SORT_INVALID: &str = "query.sort-invalid";
/// The registered rule for a pagination block in conflict with the
/// result contract.
pub(crate) const PAGINATION_INVALID: &str = "query.pagination-invalid";
/// The registered rule for a strict-profile tenant filter omission.
pub(crate) const TENANT_FILTER_MISSING: &str = "query.tenant-filter-missing";
/// The registered rule for a selection that crosses the visibility
/// boundary of the query.
pub(crate) const VISIBILITY_BOUNDARY: &str = "query.visibility-boundary";
/// The registered rule for a reference that does not resolve to the
/// required symbol kind in the bound model.
pub(crate) const REFERENCE_INVALID: &str = "query.reference-invalid";
/// The registered rule for the canonical payload bound.
pub(crate) const EXPORT_LIMIT: &str = "query.export-limit";

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

/// The fatal set for one wire normalization violation. The detail tag
/// is a fixed classification token with no subject echo.
pub(crate) fn input_invalid(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(INPUT_INVALID, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one semantic record violation with a fixed detail
/// tag and an optional bounded subject echo. `rule` selects the
/// registered contract, source, filter, sort, pagination, tenant,
/// visibility, or reference rule.
pub(crate) fn rule_invalid(rule: &str, detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(subject) = subject {
        data.insert("subject".to_owned(), token(&bounded(subject)));
    }
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

/// The fatal set for one attachment document read failure. The
/// detail token is a fixed classification, never an OS message.
pub fn io_failure(detail: &str) -> DiagnosticSet {
    input_invalid(detail)
}

/// Bound an echoed identifier to the diagnostic token bound.
pub(crate) fn bounded(text: &str) -> String {
    bound_token(text)
}
