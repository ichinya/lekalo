//! `openapi.*` diagnostics routed through the accepted #11 contract
//! (issue #46).
//!
//! Every projection refusal is one registered `openapi.*` rule
//! assembled through the shared registry-backed constructor and
//! finalized into a normalized [`DiagnosticSet`]. Echoed tokens are
//! bounded before construction: data carries only fixed detail tags,
//! bounded ids, and pointer tokens — never raw input, paths, or
//! host data.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::Diagnostic;
pub use crate::diagnostics::DiagnosticSet;
use crate::result::{singleton_set, Status};

/// The registered rule for fatal document/config input violations.
pub(crate) const INPUT_INVALID: &str = "openapi.input-invalid";
/// The registered rule for a member not expressible in the projection.
pub(crate) const PROJECTION_PARTIAL: &str = "openapi.projection-partial";
/// The registered rule for a construct impossible at the declared version.
pub(crate) const VERSION_UNSUPPORTED: &str = "openapi.version-unsupported";
/// The registered rule for a document that fails the OpenAPI meta-schema.
pub(crate) const SCHEMA_INVALID: &str = "openapi.schema-invalid";
/// The registered rule for a pointer collision with manual ownership.
pub(crate) const MERGE_CONFLICT: &str = "openapi.merge-conflict";
/// The registered rule for an unresolved operation binding.
pub(crate) const BINDING_UNRESOLVED: &str = "openapi.binding-unresolved";
/// The registered rule for a bound operation differing from its recomputed
/// fragment.
pub(crate) const DRIFT: &str = "openapi.drift";
/// The registered rule for the canonical payload bound.
pub(crate) const EXPORT_LIMIT: &str = "openapi.export-limit";

/// Finalize one registered diagnostic; a registry failure collapses
/// to the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(id: &str, data: DataObject) -> Result<Diagnostic, BuildError> {
    build(id, None, None, data)
}

/// Build a validated set or collapse to the invariant set.
fn set_of(diagnostics: Vec<Diagnostic>, status: Status) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, status)
        .unwrap_or_else(|_| singleton_set("diagnostics.registry-invalid"))
}

/// One bounded token data value.
fn token(text: &str) -> DataValue {
    token_value(text)
}

/// The fixed data prefix every rule shares: a detail tag plus an
/// optional bounded subject echo.
fn subject_data(detail: &str, subject: Option<&str>) -> DataObject {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(subject) = subject {
        data.insert("subject".to_owned(), token(&bounded(subject)));
    }
    data
}

/// The fatal set for one document/config input violation. The detail
/// tag is a fixed classification token with no subject echo.
pub(crate) fn input_invalid(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(INPUT_INVALID, data) {
        Ok(diagnostic) => set_of(vec![diagnostic], Status::Invalid),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The warning set for one not-expressible member: the symbol and the
/// fixed detail tag travel as bounded tokens. The set stays `valid` —
/// the projection is partial, not wrong.
pub(crate) fn projection_partial(findings: &[crate::openapi::types::Finding]) -> DiagnosticSet {
    let mut diagnostics = Vec::new();
    for finding in findings.iter().take(64) {
        let mut data = DataObject::new();
        data.insert("detail".to_owned(), token(&finding.detail));
        data.insert("symbol".to_owned(), token(&bounded(&finding.symbol)));
        match one(PROJECTION_PARTIAL, data) {
            Ok(diagnostic) => diagnostics.push(diagnostic),
            Err(_) => return singleton_set("diagnostics.registry-invalid"),
        }
    }
    set_of(diagnostics, Status::Valid)
}

/// The fatal set for one construct impossible at the declared version.
pub(crate) fn version_unsupported(detail: &str, version: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("version".to_owned(), token(version));
    match one(VERSION_UNSUPPORTED, data) {
        Ok(diagnostic) => set_of(vec![diagnostic], Status::Invalid),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one pointer collision with manual ownership.
pub(crate) fn merge_conflict(pointer: &str, detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("pointer".to_owned(), token(&bounded(pointer)));
    match one(MERGE_CONFLICT, data) {
        Ok(diagnostic) => set_of(vec![diagnostic], Status::Invalid),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one unresolved operation binding.
pub(crate) fn binding_unresolved(subject: &str, detail: &str) -> DiagnosticSet {
    let data = subject_data(detail, Some(subject));
    match one(BINDING_UNRESOLVED, data) {
        Ok(diagnostic) => set_of(vec![diagnostic], Status::Invalid),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one drift report against a bound operation.
pub(crate) fn drift(pointer: &str, detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("pointer".to_owned(), token(&bounded(pointer)));
    match one(DRIFT, data) {
        Ok(diagnostic) => set_of(vec![diagnostic], Status::Invalid),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for the over-bound canonical payload.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("canonical-bytes"));
    data.insert("limit".to_owned(), token(&format!("bytes={bytes}")));
    match one(EXPORT_LIMIT, data) {
        Ok(diagnostic) => set_of(vec![diagnostic], Status::Invalid),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// One registered refusal set for CLI-level custody and lookup
/// failures over the family rules.
pub fn rule_set(rule: &str, detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let data = subject_data(detail, subject);
    match one(rule, data) {
        Ok(diagnostic) => set_of(vec![diagnostic], Status::Invalid),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one document read failure. The detail token is a
/// fixed classification, never an OS message.
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
    /// registered exactly once with the LEK-OAPI code family (ADR-0043).
    #[test]
    fn every_family_rule_is_registered() {
        let registry = DiagnosticRegistry::embedded().expect("embedded registry is valid");
        let rules = [
            INPUT_INVALID,
            PROJECTION_PARTIAL,
            VERSION_UNSUPPORTED,
            SCHEMA_INVALID,
            MERGE_CONFLICT,
            BINDING_UNRESOLVED,
            DRIFT,
            EXPORT_LIMIT,
        ];
        for rule in rules {
            let entry = registry.entry(rule).unwrap_or_else(|| panic!("{rule}"));
            assert!(
                entry.code().starts_with("LEK-OAPI-"),
                "{}: {}",
                rule,
                entry.code()
            );
        }
        assert!(!registry.entry("openapi.not-a-rule").is_some());
    }

    #[test]
    fn the_partial_projection_is_a_valid_warning_set() {
        let set = projection_partial(&[crate::openapi::types::Finding {
            detail: "command-output-undeclared".to_owned(),
            symbol: "planner.focus_task".to_owned(),
        }]);
        assert_eq!(set.as_slice().len(), 1);
        assert_eq!(set.as_slice()[0].id.as_str(), "openapi.projection-partial");
        assert_eq!(set.reason_ids(), vec!["openapi.projection-partial"]);
    }

    #[test]
    fn the_version_refusal_carries_the_declared_version() {
        let set = version_unsupported("scheme-kind", "3.0");
        assert!(!set.is_empty());
        assert!(format!("{:?}", set.as_slice()).contains("3.0"));
    }
}
