//! Inspect diagnostics routed through the accepted #11 contract
//! (issue #15).
//!
//! The four inspect rules were added to the embedded registry as a
//! wire-shape-preserving minor increment (`1.2.0` -> `1.3.0`, ADR-0014):
//! `inspect.symbol-unknown`, `inspect.short-name-unknown`,
//! `inspect.short-name-ambiguous`, and `inspect.output-limit`. Unknown
//! and ambiguous resolution are distinct stable diagnostics and are
//! never first-match collapses. Malformed selectors are CLI syntax
//! failures and reuse the registered `cli.usage` rule with no echo at
//! all. Every echoed token passes the bounded-token invariant, and an
//! echoed selector exists only after the selector grammar accepted it.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue, Scalar};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::Status;

/// The registered rule for a selector whose full id names no
/// inspectable symbol.
pub(crate) const SYMBOL_UNKNOWN: &str = "inspect.symbol-unknown";
/// The registered rule for a short name matching no symbol.
pub(crate) const SHORT_NAME_UNKNOWN: &str = "inspect.short-name-unknown";
/// The registered rule for a short name matching several symbols.
pub(crate) const SHORT_NAME_AMBIGUOUS: &str = "inspect.short-name-ambiguous";
/// The registered rule for a payload beyond the output bound.
pub(crate) const OUTPUT_LIMIT: &str = "inspect.output-limit";

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
) -> Result<Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, symbol, None, data)
}

/// Build a validated `invalid` set or collapse to the invariant set.
fn invalid_set(diagnostics: Vec<Diagnostic>) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, Status::Invalid)
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// The `invalid` set for a full-id or alias selector that names no
/// inspectable symbol. `detail` is a fixed vocabulary token, never the
/// raw input.
pub(crate) fn symbol_unknown_set(symbol: &str, detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(symbol));
    data.insert("detail".to_owned(), token_value(detail));
    match one(SYMBOL_UNKNOWN, Some(bound_token(symbol)), data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a short name matching no symbol.
pub(crate) fn short_name_unknown_set(name: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(name));
    match one(SHORT_NAME_UNKNOWN, Some(bound_token(name)), data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a short name matching several symbols. The
/// candidate list is the bounded, unsigned-UTF-8-sorted prefix of the
/// fully qualified matches; `matched` carries the exact total so the
/// omitted tail stays countable.
pub(crate) fn short_name_ambiguous_set(
    name: &str,
    candidates: &[String],
    matched: usize,
) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token_value(name));
    data.insert(
        "candidates".to_owned(),
        DataValue::List(
            candidates
                .iter()
                .map(|candidate| Scalar::Token(bound_token(candidate)))
                .collect(),
        ),
    );
    data.insert(
        "matched".to_owned(),
        DataValue::Count(matched.try_into().unwrap_or(u64::MAX)),
    );
    match one(SHORT_NAME_AMBIGUOUS, Some(bound_token(name)), data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a canonical payload beyond the output bound:
/// no partial inspect payload is emitted.
pub(crate) fn output_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("inspect-payload-limit"));
    data.insert(
        "limit".to_owned(),
        DataValue::Count(super::version::MAX_OUTPUT_BYTES as u64),
    );
    let _ = bytes;
    match one(OUTPUT_LIMIT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The registered `cli.usage` failure for a selector that violates the
/// invocation grammar; the wire carries no echo of the rejected input.
pub(crate) fn selector_invalid_set() -> DiagnosticSet {
    crate::result::singleton_set(crate::result::CLI_USAGE)
}
