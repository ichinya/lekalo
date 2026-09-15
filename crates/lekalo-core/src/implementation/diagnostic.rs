//! Foreign/custom implementation diagnostics routed through the accepted
//! #11 contract (issue #30).
//!
//! Every attachment failure is one registered `implementation.*` rule
//! assembled through the shared registry-backed constructor and finalized
//! into a normalized [`DiagnosticSet`]. Every echoed token is bounded
//! before construction: rule data carries only fixed reason tags and
//! validated document identifiers — never raw input, host paths, target
//! commands, digests, or attacker-controlled text.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for wire and self-check violations.
pub(crate) const DOCUMENT_INVALID: &str = "implementation.document-invalid";
/// The registered rule for an acknowledged effect outside the declared
/// operation surface.
pub(crate) const EFFECT_MISMATCH: &str = "implementation.effect-mismatch";
/// The registered rule for an operation symbol that does not resolve as
/// a command or query in the bound IR.
pub(crate) const REFERENCE_UNRESOLVED: &str = "implementation.reference-unresolved";
/// The registered rule for competing target implementations without
/// exactly one explicit selection.
pub(crate) const SELECTION_AMBIGUOUS: &str = "implementation.selection-ambiguous";
/// The registered rule for an explicit selection on a target with no
/// competing implementation.
pub(crate) const SELECTION_UNKNOWN: &str = "implementation.selection-unknown";
/// The registered rule for a refused target symbol spelling.
pub(crate) const SYMBOL_INVALID: &str = "implementation.symbol-invalid";
/// The registered portability warning for a project target without an
/// implementation entry.
pub(crate) const TARGET_MISSING: &str = "implementation.target-missing";

/// Finalize one registered diagnostic; a registry failure yields `None`
/// and the caller's set fails closed during normalization.
fn one(id: &str, symbol: Option<&str>, data: DataObject) -> Option<Diagnostic> {
    build(id, symbol.map(bound_token), None, data).ok()
}

/// Build a validated set or collapse to the invariant set.
pub(crate) fn finish(diagnostics: Vec<Diagnostic>, status: Status) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, status)
        .unwrap_or_else(|_| singleton_set("diagnostics.registry-invalid"))
}

/// One bounded token data value.
fn token(text: &str) -> DataValue {
    token_value(text)
}

/// The `{reason[, detail]}` data object of `document-invalid`.
fn document_data(reason: &str, subject: Option<&str>) -> DataObject {
    let mut data = DataObject::new();
    data.insert("reason".to_owned(), token(reason));
    if let Some(subject) = subject {
        data.insert("detail".to_owned(), token(&bound_token(subject)));
    }
    data
}

/// One fatal wire or self-check violation as a finalized set (wire
/// decode stops at the first violation).
pub(crate) fn document_invalid(reason: &str, subject: Option<&str>) -> DiagnosticSet {
    match one(DOCUMENT_INVALID, None, document_data(reason, subject)) {
        Some(diagnostic) => finish(vec![diagnostic], Status::Invalid),
        None => singleton_set("diagnostics.registry-invalid"),
    }
}

/// Collect one document-invalid error; collection continues.
pub(crate) fn push_document_invalid(errors: &mut Vec<Diagnostic>, reason: &str) {
    if let Some(diagnostic) = one(DOCUMENT_INVALID, None, document_data(reason, None)) {
        errors.push(diagnostic);
    }
}

/// The fatal set for a refused target symbol spelling: the target name
/// and one fixed reason tag, never the refused text.
pub(crate) fn symbol_invalid(target: &str, reason: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("target".to_owned(), token(target));
    data.insert("reason".to_owned(), token(reason));
    match one(SYMBOL_INVALID, None, data) {
        Some(diagnostic) => finish(vec![diagnostic], Status::Invalid),
        None => singleton_set("diagnostics.registry-invalid"),
    }
}

/// Collect one unresolved operation reference; collection continues.
pub(crate) fn push_reference_unresolved(
    errors: &mut Vec<Diagnostic>,
    symbol: &str,
    contract: &str,
) {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token(symbol));
    data.insert("contract".to_owned(), token(contract));
    if let Some(diagnostic) = one(REFERENCE_UNRESOLVED, Some(symbol), data) {
        errors.push(diagnostic);
    }
}

/// Collect one acknowledged-effect violation: the undeclared effect
/// reference is echoed bounded.
pub(crate) fn push_effect_mismatch(errors: &mut Vec<Diagnostic>, symbol: &str, reference: &str) {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token(symbol));
    data.insert("reference".to_owned(), token(reference));
    if let Some(diagnostic) = one(EFFECT_MISMATCH, Some(symbol), data) {
        errors.push(diagnostic);
    }
}

/// Collect one ambiguous-selection violation.
pub(crate) fn push_selection_ambiguous(errors: &mut Vec<Diagnostic>, symbol: &str, target: &str) {
    push_pair(errors, SELECTION_AMBIGUOUS, symbol, target);
}

/// Collect one unknown-selection violation.
pub(crate) fn push_selection_unknown(errors: &mut Vec<Diagnostic>, symbol: &str, target: &str) {
    push_pair(errors, SELECTION_UNKNOWN, symbol, target);
}

/// Collect one portability warning: a project target binding has no
/// implementation entry for the escape-hatched operation.
pub(crate) fn push_target_missing(warnings: &mut Vec<Diagnostic>, symbol: &str, target: &str) {
    push_pair(warnings, TARGET_MISSING, symbol, target);
}

/// Emit one `{symbol, target}` rule diagnostic, ignoring a registry
/// failure (the caller's set then fails closed during normalization).
fn push_pair(errors: &mut Vec<Diagnostic>, rule: &str, symbol: &str, target: &str) {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token(symbol));
    data.insert("target".to_owned(), token(target));
    if let Some(diagnostic) = one(rule, Some(symbol), data) {
        errors.push(diagnostic);
    }
}
