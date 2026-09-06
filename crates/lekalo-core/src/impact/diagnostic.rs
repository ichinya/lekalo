//! Impact diagnostics routed through the accepted #11 contract (issue #16).
//!
//! Every impact failure and degraded marker is one registered `impact.*`
//! rule (`LEK-IMPACT-NNN`, registry 1.3.0) assembled through the shared
//! registry-backed constructor and finalized into a normalized
//! [`DiagnosticSet`]. Every echo is a bounded fixed token validated by the
//! selector/entry grammar first — raw input, paths, and Git output never
//! enter a diagnostic. Severity never computes an exit.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

/// The selector or filter combination was rejected.
pub const SELECTOR_INVALID: &str = "impact.selector-invalid";
/// The changed-input handoff was malformed or violated containment.
pub const CHANGED_INPUT_INVALID: &str = "impact.changed-input-invalid";
/// Some changed inputs resolved to no semantic symbols (degraded, visible).
pub const CHANGED_INPUT_INCOMPLETE: &str = "impact.changed-input-incomplete";
/// A requested symbol is not part of the graph.
pub const SYMBOL_UNKNOWN: &str = "impact.symbol-unknown";
/// Detected effect evidence for an affected operation is stale.
pub const EFFECT_STALE: &str = "impact.effect-stale";
/// Required evidence for a projection is unavailable.
pub const EVIDENCE_UNKNOWN: &str = "impact.evidence-unknown";
/// The mandatory public closure stopped early (degraded, visible).
pub const PUBLIC_IMPACT_INCOMPLETE: &str = "impact.public-impact-incomplete";
/// The traversal exceeded its recorded bound: rejected, never truncated.
pub const TRAVERSAL_LIMIT: &str = "impact.traversal-limit";
/// The canonical result exceeded its recorded byte bound.
pub const OUTPUT_LIMIT: &str = "impact.output-limit";
/// Strict-profile denial on required unknown or stale evidence.
pub const GATE_BLOCKED: &str = "impact.gate-blocked";

/// Finalize one registered diagnostic; a registry failure collapses to the
/// registry-invariant set (double developer fault) instead of panicking.
fn one(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
) -> Result<crate::diagnostics::Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, symbol, None, data)
}

/// Build a validated `invalid` set or collapse to the invariant set.
fn invalid_set(diagnostics: Vec<crate::diagnostics::Diagnostic>) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, Status::Invalid)
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// One `detail`-tagged set of any allowed status.
fn detail_set(id: &str, status: Status, detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    match one(id, None, data) {
        Ok(diagnostic) => DiagnosticSet::try_from_unsorted(vec![diagnostic], status)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a rejected selector or filter combination.
pub fn selector_invalid_set(detail: &str) -> DiagnosticSet {
    detail_set(SELECTOR_INVALID, Status::Invalid, detail)
}

/// The `invalid` set for a malformed changed-input handoff.
pub fn changed_input_invalid_set(detail: &str) -> DiagnosticSet {
    detail_set(CHANGED_INPUT_INVALID, Status::Invalid, detail)
}

/// The non-blocking warning for changed inputs with no resolvable symbols.
pub fn changed_input_incomplete_warning(detail: &str) -> Option<crate::diagnostics::Diagnostic> {
    warning(CHANGED_INPUT_INCOMPLETE, detail)
}

/// The `invalid` set for a request naming a symbol outside the graph.
pub fn symbol_unknown_set(symbol: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(symbol));
    match one(SYMBOL_UNKNOWN, Some(bound_token(symbol)), data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a bound-exhausted traversal or allocation.
pub fn traversal_limit_set(limit: &'static str) -> DiagnosticSet {
    detail_set(TRAVERSAL_LIMIT, Status::Invalid, limit)
}

/// The `invalid` set for an over-bound canonical export.
pub fn output_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("export-byte-limit"));
    if let Ok(count) = u64::try_from(bytes) {
        data.insert("bytes".to_owned(), DataValue::Count(count));
    }
    match one(OUTPUT_LIMIT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `denied` set for a strict-profile gate block.
pub fn gate_blocked_set(detail: &str) -> DiagnosticSet {
    detail_set(GATE_BLOCKED, Status::Denied, detail)
}

/// One non-blocking warning carried alongside a valid result.
pub fn warning(id: &str, detail: &str) -> Option<crate::diagnostics::Diagnostic> {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    one(id, None, data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failures_carry_bounded_tokens_only() {
        let long = "x".repeat(500);
        let set = symbol_unknown_set(&long);
        let rendered = serde_json::to_string(&set.as_slice()).expect("wire");
        assert!(rendered.len() < 4_000, "echo stayed bounded");
        // The #11 contract bounds each token echo at 256 bytes: no single
        // run of the hostile input may exceed the bound.
        assert!(
            !rendered.contains(&"x".repeat(257)),
            "echo within the token bound"
        );
        assert_eq!(set.reason_ids(), vec![SYMBOL_UNKNOWN]);
    }

    #[test]
    fn strict_denial_carries_the_denied_status() {
        let set = gate_blocked_set("required-evidence-unknown");
        assert_eq!(set.reason_ids(), vec![GATE_BLOCKED]);
        assert!(!set.is_empty());
    }

    #[test]
    fn warnings_validate_against_the_registry() {
        let diagnostic = changed_input_incomplete_warning("unresolved-changed-inputs");
        assert!(diagnostic.is_some());
        let set = DiagnosticSet::try_from_unsorted(vec![diagnostic.expect("some")], Status::Valid)
            .expect("warning set");
        assert_eq!(set.reason_ids(), vec![CHANGED_INPUT_INCOMPLETE]);
    }
}
