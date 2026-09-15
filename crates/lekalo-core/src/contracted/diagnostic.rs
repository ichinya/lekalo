//! Contracted-mode diagnostics routed through the accepted #11 contract
//! (issue #40).
//!
//! Every contracted failure or finding is one registered `contracted.*`
//! rule assembled through the shared registry-backed constructor and
//! finalized into a normalized [`DiagnosticSet`] (the reserved 1.19.0
//! registry carries the family as a wire-shape-preserving additive minor
//! increment over 1.16.0). Echoed tokens are bounded after grammar
//! validation; raw input never scales or shapes the envelope.

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

/// The fatal set for a refused declaration document or malformed
/// registry. `detail` is a fixed classification tag owned by this
/// module; the optional echo is a bounded grammar-validated token.
pub fn declaration_invalid_set(detail: &'static str, target: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    if let Some(echo) = target {
        data.insert("target".to_owned(), token_value(echo));
    }
    one(
        "contracted.declaration-invalid",
        None,
        Status::Invalid,
        data,
    )
}

/// The fatal set for an over-bound declaration or registry document.
pub fn declaration_limit_set(limit: &str, found: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("limit".to_owned(), token_value(limit));
    data.insert("found".to_owned(), DataValue::Count(found as u64));
    one("contracted.declaration-limit", None, Status::Invalid, data)
}

/// The fatal set for a declaration naming a module the canonical model
/// does not declare.
pub(crate) fn unknown_module_set(module: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("module".to_owned(), token_value(module));
    one("contracted.unknown-module", None, Status::Invalid, data)
}

/// The fatal set for a declaration naming a symbol the canonical model
/// does not declare.
pub(crate) fn unknown_symbol_set(symbol: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(symbol));
    one("contracted.unknown-symbol", None, Status::Invalid, data)
}

/// The fatal set for registry read/write filesystem failures or a
/// noncanonical persisted document.
pub(crate) fn registry_io_set(detail: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    one("contracted.registry-io", None, Status::Invalid, data)
}

/// One conformance-drift gate diagnostic item; `None` means the registry
/// refused (a developer fault the caller folds into the
/// registry-invariant set).
pub(crate) fn try_drift_item(
    symbol: &str,
    detail: &'static str,
) -> Option<crate::diagnostics::Diagnostic> {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    build(
        "contracted.binding-drift",
        Some(bounded(symbol)),
        None,
        data,
    )
    .ok()
}

/// One stale support-artifact gate diagnostic item.
pub(crate) fn try_stale_artifact_item(path: &str) -> Option<crate::diagnostics::Diagnostic> {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("content-stale"));
    build("contracted.stale-artifact", Some(bounded(path)), None, data).ok()
}

/// One coverage-gap gate diagnostic item.
pub(crate) fn try_coverage_item(symbol: &str) -> Option<crate::diagnostics::Diagnostic> {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("no-native-test"));
    build(
        "contracted.coverage-missing",
        Some(bounded(symbol)),
        None,
        data,
    )
    .ok()
}

/// The fatal set for a selector naming no recorded conformed symbol.
pub(crate) fn selector_unknown_set(symbol: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(symbol));
    one(
        "contracted.unknown-symbol",
        Some(bounded(symbol)),
        Status::Invalid,
        data,
    )
}

/// The bounded token echo: the registry data-field bound is 256 bytes.
pub(crate) fn bounded(value: &str) -> String {
    value.chars().take(256).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_items_carry_the_registered_rule() {
        let item = try_drift_item("planner.focus_task", "fingerprint-mismatch").expect("item");
        assert_eq!(item.id.as_str(), "contracted.binding-drift");
    }

    #[test]
    fn stale_artifact_items_carry_the_registered_rule() {
        let item = try_stale_artifact_item(".lekalo/generated/openapi/planner.json").expect("item");
        assert_eq!(item.id.as_str(), "contracted.stale-artifact");
    }

    #[test]
    fn unknown_modules_and_symbols_fail_closed() {
        let set = unknown_module_set("ghost");
        assert_eq!(set.reason_ids()[0], "contracted.unknown-module");
        let set = unknown_symbol_set("planner.ghost");
        assert_eq!(set.reason_ids()[0], "contracted.unknown-symbol");
    }

    #[test]
    fn bounded_echoes_respect_the_wire_bound() {
        let long = "x".repeat(1000);
        assert_eq!(bounded(&long).len(), 256);
    }
}
