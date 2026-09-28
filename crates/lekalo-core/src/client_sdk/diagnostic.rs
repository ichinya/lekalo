//! Client-SDK diagnostics routed through the accepted #11 contract
//! (issue #72).
//!
//! Every SDK failure is one registered `client.*` rule assembled through
//! the shared registry-backed constructor and finalized into a normalized
//! [`DiagnosticSet`]. Every echoed token is bounded before construction:
//! rule data carries only fixed detail tags and bounded ids — never raw
//! input, paths, tokens, digests, or attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal wire input violations.
pub(crate) const INPUT_INVALID: &str = "client.input-invalid";
/// The registered rule for a projection that violates its own closed
/// contract.
pub(crate) const CONTRACT_INVALID: &str = "client.contract-invalid";
/// The registered rule for a required semantic join that does not
/// resolve (endpoint, operation, type, or error).
pub(crate) const SYMBOL_UNRESOLVED: &str = "client.symbol-unresolved";
/// The registered rule for an unsupported language mapping or
/// serialization feature the contract cannot express.
pub(crate) const MAPPING_UNSUPPORTED: &str = "client.mapping-unsupported";
/// The registered rule for a retry configuration that the declared
/// contract never authorizes.
pub(crate) const RETRY_UNSAFE: &str = "client.retry-unsafe";
/// The registered rule for a maintained-client check failure. Wired
/// to the `client check` gate in a follow-up of this issue's staged
/// implementation; the rule stays registered from the first
/// generation so the wire grammar is stable.
#[allow(dead_code)]
pub(crate) const DRIFT: &str = "client.drift";
/// The registered rule for a consumer inventory violation (unbound,
/// duplicate, or unregistered consumer).
pub(crate) const CONSUMER_INVALID: &str = "client.consumer-invalid";
/// The registered rule for one crossed bound.
pub(crate) const LIMIT_EXCEEDED: &str = "client.limit-exceeded";

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

/// The bounded subject data record: a fixed detail tag plus an
/// optional bounded subject echo.
fn subject_data(detail: &str, subject: Option<&str>) -> DataObject {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(subject) = subject {
        data.insert("subject".to_owned(), token(&bounded(subject)));
    }
    data
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

/// The fatal set for one declaration violation carrying a rule, a
/// fixed detail tag, and an optional bounded subject echo over the
/// family rules.
pub(crate) fn rule_invalid(rule: &str, detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let data = subject_data(detail, subject);
    match one(rule, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// One registered refusal set for CLI-level custody and lookup
/// failures.
pub fn rule_set(rule: &str, detail: &str, subject: Option<&str>) -> DiagnosticSet {
    rule_invalid(rule, detail, subject)
}

/// One non-blocking finding assembled outside a set (the impact
/// warning path).
#[allow(dead_code)]
pub(crate) fn finding(id: &str, detail: &str, subject: Option<&str>) -> Option<Diagnostic> {
    one(id, subject_data(detail, subject)).ok()
}

/// The fatal set for the over-bound canonical payload.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("canonical-bytes"));
    data.insert("limit".to_owned(), token(&format!("bytes={bytes}")));
    match one(LIMIT_EXCEEDED, data) {
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

    #[test]
    fn refusals_carry_bounded_tokens_only() {
        let long = "x".repeat(500);
        let set = rule_invalid(CONTRACT_INVALID, "projection", Some(&long));
        let rendered = serde_json::to_string(set.as_slice()).expect("wire");
        assert!(
            !rendered.contains(&"x".repeat(257)),
            "echo within the token bound"
        );
    }

    #[test]
    fn every_rule_is_registered() {
        for rule in [
            INPUT_INVALID,
            CONTRACT_INVALID,
            SYMBOL_UNRESOLVED,
            MAPPING_UNSUPPORTED,
            RETRY_UNSAFE,
            DRIFT,
            CONSUMER_INVALID,
            LIMIT_EXCEEDED,
        ] {
            let set = rule_invalid(rule, "probe", None);
            assert_eq!(set.as_slice()[0].id(), rule, "{rule} registered");
        }
    }
}
