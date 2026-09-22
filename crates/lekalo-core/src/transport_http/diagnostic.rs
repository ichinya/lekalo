//! Transport-http diagnostics routed through the accepted #11
//! contract (issue #70).
//!
//! Every attachment failure is one registered `transport.*` rule
//! assembled through the shared registry-backed constructor and
//! finalized into a normalized [`DiagnosticSet`]. Every echoed token
//! is bounded before construction: rule data carries only fixed
//! detail tags, declared limits, and bounded ids — never raw input,
//! paths, tokens, digests, or attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal wire input violations.
pub(crate) const INPUT_INVALID: &str = "transport.input-invalid";
/// The registered rule for a declaration that violates its own
/// closed contract.
pub(crate) const CONTRACT_INVALID: &str = "transport.contract-invalid";
/// The registered rule for an endpoint reference that does not
/// resolve to a Model endpoint symbol.
pub(crate) const ENDPOINT_UNRESOLVED: &str = "transport.endpoint-unresolved";
/// The registered rule for a parameter binding violation.
pub(crate) const PARAM_INVALID: &str = "transport.param-invalid";
/// The registered rule for an unmapped union member.
pub(crate) const MAPPING_MISSING: &str = "transport.mapping-missing";
/// The registered rule for an actor/scheme combination violation.
pub(crate) const SECURITY_INVALID: &str = "transport.security-invalid";
/// The registered rule for a capability the resolved profile does
/// not support.
pub(crate) const CAPABILITY_UNSATISFIED: &str = "transport.capability-unsatisfied";
/// The registered rule for a pagination conflict.
pub(crate) const PAGINATION_INVALID: &str = "transport.pagination-invalid";
/// The registered rule for the canonical payload bound.
pub(crate) const EXPORT_LIMIT: &str = "transport.export-limit";

/// Finalize one registered diagnostic; a registry failure collapses
/// to the registry-invariant set (double developer fault) instead of
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

/// The fatal set for one wire normalization violation. The detail
/// tag is a fixed classification token with no subject echo.
pub(crate) fn input_invalid(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(INPUT_INVALID, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// Assemble the fixed data prefix every rule shares: a detail tag
/// plus an optional bounded subject echo.
fn subject_data(detail: &str, subject: Option<&str>) -> DataObject {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(subject) = subject {
        data.insert("subject".to_owned(), token(&bounded(subject)));
    }
    data
}

/// The fatal set for one declaration violation carrying a rule, a
/// fixed detail tag, and an optional bounded subject echo.
pub(crate) fn rule_invalid(rule: &str, detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let data = subject_data(detail, subject);
    match one(rule, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a capability the resolved profile does not
/// support: the capability id, the required minimum, and the actual
/// support travel as bounded tokens.
pub(crate) fn capability_unsatisfied(
    subject: &str,
    capability: &str,
    required: &str,
    actual: &str,
) -> DiagnosticSet {
    let mut data = subject_data("capability-unsupported", Some(subject));
    data.insert("capability".to_owned(), token(capability));
    data.insert("required".to_owned(), token(required));
    data.insert("actual".to_owned(), token(actual));
    match one(CAPABILITY_UNSATISFIED, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// One registered refusal set for CLI-level custody and lookup
/// failures: a fixed detail tag plus an optional bounded subject
/// echo over the family rules.
pub fn rule_set(rule: &str, detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let data = subject_data(detail, subject);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::registry::DiagnosticRegistry;

    /// Registry parity: every rule the family routes through is
    /// registered exactly once with the LEK-TRN code family and the
    /// invalid status (ADR-0042).
    #[test]
    fn every_family_rule_is_registered() {
        let registry = DiagnosticRegistry::embedded().expect("embedded registry is valid");
        let rules = [
            INPUT_INVALID,
            CONTRACT_INVALID,
            ENDPOINT_UNRESOLVED,
            PARAM_INVALID,
            MAPPING_MISSING,
            SECURITY_INVALID,
            CAPABILITY_UNSATISFIED,
            PAGINATION_INVALID,
            EXPORT_LIMIT,
        ];
        for rule in rules {
            let entry = registry.entry(rule).unwrap_or_else(|| panic!("{rule}"));
            assert!(
                entry.code().starts_with("LEK-TRN-"),
                "{}: {}",
                rule,
                entry.code()
            );
            assert!(entry.allows_status(crate::result::Status::Invalid));
        }
        assert!(!registry.entry("transport.not-a-rule").is_some());
    }
}
