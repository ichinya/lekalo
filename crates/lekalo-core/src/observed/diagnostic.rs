//! Observed-mode diagnostics routed through the accepted #11 contract
//! (issue #39).
//!
//! Every observed failure or finding is one registered `observed.*` rule
//! assembled through the shared registry-backed constructor and finalized
//! into a normalized [`DiagnosticSet`] (the integrated registry 1.16.0 carries the family
//! as a wire-shape-preserving additive minor increment). Echoed tokens
//! are bounded after grammar validation; raw input never scales or
//! shapes the envelope.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(id: &str, symbol: Option<String>, status: Status, data: DataObject) -> DiagnosticSet {
    match build(id, symbol, None, data) {
        Ok(diagnostic) => match DiagnosticSet::try_from_unsorted(vec![diagnostic], status) {
            Ok(set) => set,
            Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
        },
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a refused scan document. `detail` is a fixed
/// classification tag owned by this module; the optional echo is a
/// bounded grammar-validated token.
pub(crate) fn scan_invalid_set(detail: &'static str, target: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    if let Some(echo) = target {
        data.insert("target".to_owned(), token_value(echo));
    }
    one("observed.scan-invalid", None, Status::Invalid, data)
}

/// The fatal set for an over-bound scan or index.
pub fn scan_limit_set(limit: &str, found: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("limit".to_owned(), token_value(limit));
    data.insert("found".to_owned(), DataValue::Count(found as u64));
    one("observed.scan-limit", None, Status::Invalid, data)
}

/// The fatal set for a scan naming a module the canonical model does not
/// declare.
pub(crate) fn unknown_module_set(module: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("module".to_owned(), token_value(module));
    one("observed.unknown-module", None, Status::Invalid, data)
}

/// The fatal set for an operation that needs an index with none recorded.
pub(crate) fn index_missing_set(operation: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("operation".to_owned(), token_value(operation));
    one("observed.index-missing", None, Status::Invalid, data)
}

/// The fatal set for index read/write filesystem failures.
pub(crate) fn index_io_set(detail: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    one("observed.index-io", None, Status::Invalid, data)
}

/// The fatal set for a selector naming no recorded observed symbol.
pub(crate) fn unknown_symbol_set(symbol: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(symbol));
    one(
        "observed.unknown-symbol",
        Some(bounded(symbol)),
        Status::Invalid,
        data,
    )
}

/// The fatal set for confirming a binding that is not inferred.
pub(crate) fn confirm_refused_set(symbol: &str, status: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(symbol));
    data.insert("status".to_owned(), token_value(status));
    one(
        "observed.confirm-refused",
        Some(bounded(symbol)),
        Status::Invalid,
        data,
    )
}

/// One gate diagnostic item; `None` means the registry refused (a
/// developer fault the caller folds into the registry-invariant set).
pub(crate) fn try_stale_binding_item(
    symbol: &str,
    detail: &'static str,
) -> Option<crate::diagnostics::Diagnostic> {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(symbol));
    data.insert("detail".to_owned(), token_value(detail));
    build("observed.stale-binding", Some(bounded(symbol)), None, data).ok()
}
/// The fatal set for a refused promotion.
pub(crate) fn promotion_refused_set(symbol: &str, detail: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(symbol));
    data.insert("detail".to_owned(), token_value(detail));
    one(
        "observed.promotion-refused",
        Some(bounded(symbol)),
        Status::Invalid,
        data,
    )
}

/// The fatal set for a confirmed plan that no longer matches the
/// recorded state.
pub(crate) fn promotion_plan_mismatch_set(detail: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    one(
        "observed.promotion-plan-mismatch",
        None,
        Status::Invalid,
        data,
    )
}

/// A bounded echo of a grammar-validated symbol id.
fn bounded(symbol: &str) -> String {
    crate::diagnostics::types::bound_token(symbol)
}

/// The fatal set for an unreadable scan document file.
pub fn scan_io_failure(detail: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    one("observed.index-io", None, Status::Invalid, data)
}

/// The fatal set for an operation that needs an index with none recorded.
pub fn missing_index_set() -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("operation".to_owned(), token_value("observed"));
    one("observed.index-missing", None, Status::Invalid, data)
}
