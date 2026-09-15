//! Expression-family diagnostics routed through the accepted #11
//! contract (issue #66).
//!
//! Every attachment or evaluation failure is one registered
//! `expression.*` rule assembled through the shared registry-backed
//! constructor and finalized into a normalized [`DiagnosticSet`].
//! Record-level rejections carry the expression's declared source
//! span (file, byte range, line/column) whenever the attachment
//! declares one, so every refusal is explainable at its authoring
//! position. Every echoed token is bounded before construction: rule
//! data carries only fixed detail tags, bounded subjects, and
//! declared limits — never raw input, paths, tokens, digests, or
//! attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet, SourceLocation};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal wire input violations.
pub(crate) const INPUT_INVALID: &str = "expression.input-invalid";
/// The registered rule for a record that violates its own closed
/// contract (identity grammar, kind/target coherence, params).
pub(crate) const CONTRACT_INVALID: &str = "expression.contract-invalid";
/// The registered rule for an invalid static type or operator
/// combination.
pub(crate) const TYPE_INVALID: &str = "expression.type-invalid";
/// The registered rule for an expression that exceeds the closed
/// grammar and belongs in a foreign implementation.
pub(crate) const COMPLEXITY_LIMIT: &str = "expression.complexity-limit";
/// The registered rule for a built-in absent from a target's
/// declared capability snapshot (the managed-mode block).
pub(crate) const BUILTIN_UNSUPPORTED: &str = "expression.builtin-unsupported";
/// The registered rule for a deterministic evaluation domain error.
pub(crate) const EVAL_INVALID: &str = "expression.eval-invalid";
/// The registered rule for evaluation bindings that do not satisfy
/// the declared references.
pub(crate) const BINDING_INVALID: &str = "expression.binding-invalid";
/// The registered rule for a semantically impossible comparison.
pub(crate) const DIFF_INVALID: &str = "expression.diff-invalid";
/// The registered rule for the canonical payload bound.
pub(crate) const EXPORT_LIMIT: &str = "expression.export-limit";

/// Finalize one registered diagnostic; a registry failure collapses
/// to the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(id: &str, data: DataObject) -> Result<Diagnostic, BuildError> {
    build(id, None, None, data)
}

/// Finalize one registered record diagnostic with its declared span.
fn one_spanned(
    id: &str,
    data: DataObject,
    span: Option<&SourceLocation>,
) -> Result<Diagnostic, BuildError> {
    let mut result = build(id, None, None, data)?;
    result.source = span.cloned();
    Ok(result)
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

/// Bound an echoed identifier to the diagnostic token bound.
pub(crate) fn bounded(text: &str) -> String {
    bound_token(text)
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

/// The fatal set for one record violation with a fixed detail tag, an
/// optional bounded subject echo, and the record's declared span.
pub(crate) fn rule_invalid(
    rule: &str,
    detail: &str,
    subject: Option<&str>,
    span: Option<&SourceLocation>,
) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(subject) = subject {
        data.insert("subject".to_owned(), token(&bounded(subject)));
    }
    match one_spanned(rule, data, span) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one evaluation domain error with its closed
/// error token.
pub(crate) fn eval_invalid(
    error: super::builtin::EvalError,
    subject: Option<&str>,
) -> DiagnosticSet {
    rule_invalid(EVAL_INVALID, error.key(), subject, None)
}

/// The fatal set for one binding violation.
pub(crate) fn binding_invalid(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    rule_invalid(BINDING_INVALID, detail, subject, None)
}

/// The fatal set for the over-bound canonical payload.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("canonical-bytes"));
    data.insert(
        "limit".to_owned(),
        token(&super::version::MAX_CANONICAL_BYTES.to_string()),
    );
    let _ = bytes;
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

/// The fatal set for one impossible comparison (foreign projects or
/// mixed revisions).
pub(crate) fn diff_invalid(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(DIFF_INVALID, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}
