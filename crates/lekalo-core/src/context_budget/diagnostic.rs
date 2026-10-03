//! Context-budget diagnostics routed through the accepted #11 contract
//! (issue #75).
//!
//! Every finding is one registered `context.*` rule (`LEK-CONTEXT-NNN`)
//! assembled through the shared registry-backed constructor with bounded
//! tokens on every echo. The advisory rules (`closure-incomplete`,
//! `artifact-evidence-incomplete`, `baseline-incomparable`,
//! `baseline-regression`, `budget-exceeded`) ride the `valid` envelope;
//! `input-invalid` is fatal `invalid`, `profile-unsupported` is
//! `unsupported-version` with no fallback, and `policy-denied` is the
//! mandatory-policy `denied` that never suppresses its report.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

/// A malformed request, selector, profile, policy, baseline, or evidence input.
pub const INPUT_INVALID: &str = "context.input-invalid";
/// An unsupported profile/estimator/contract version; never a fallback.
pub const PROFILE_UNSUPPORTED: &str = "context.profile-unsupported";
/// A bound, unresolved required edge, stale evidence, or unrepresentable
/// attachment stopped the closure.
pub const CLOSURE_INCOMPLETE: &str = "context.closure-incomplete";
/// Requested source/target evidence is missing, stale, or unmeasurable.
pub const ARTIFACT_EVIDENCE_INCOMPLETE: &str = "context.artifact-evidence-incomplete";
/// The baseline cannot support a regression verdict.
pub const BASELINE_INCOMPARABLE: &str = "context.baseline-incomparable";
/// The complete required estimate exceeds the effective content budget.
pub const BUDGET_EXCEEDED: &str = "context.budget-exceeded";
/// A comparable metric exceeds a configured regression allowance.
pub const BASELINE_REGRESSION: &str = "context.baseline-regression";
/// The selected mandatory policy failed on budget, regression, or completeness.
pub const POLICY_DENIED: &str = "context.policy-denied";

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of panicking.
fn one(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
) -> Result<crate::diagnostics::Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, symbol, None, data)
}

/// Build a validated set or collapse to the invariant set.
fn finalize(diagnostics: Vec<crate::diagnostics::Diagnostic>, status: Status) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, status)
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// One bounded detail token.
pub(crate) fn token(text: &str) -> DataValue {
    token_value(text)
}

/// The fatal `invalid` set for one input violation.
pub fn input_invalid(detail: &str) -> DiagnosticSet {
    input_invalid_detail(detail, None)
}

/// The fatal `invalid` set with a bounded subject echo.
pub fn input_invalid_detail(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(subject) = subject {
        data.insert("subject".to_owned(), token(subject));
    }
    match one(INPUT_INVALID, None, data) {
        Ok(diagnostic) => finalize(vec![diagnostic], Status::Invalid),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `unsupported-version` set for an unsupported profile/estimator pin.
pub fn profile_unsupported(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(subject) = subject {
        data.insert("subject".to_owned(), token(subject));
    }
    match one(PROFILE_UNSUPPORTED, None, data) {
        Ok(diagnostic) => finalize(vec![diagnostic], Status::UnsupportedVersion),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// One advisory warning row for the `valid` envelope.
fn warning(id: &str, subject: &str, detail: &str) -> Option<crate::diagnostics::Diagnostic> {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("subject".to_owned(), token(subject));
    one(id, None, data).ok()
}

/// The advisory closure-incomplete rows (one per bounded subject).
pub fn closure_incomplete(
    subjects: &[String],
    detail: &str,
) -> Vec<crate::diagnostics::Diagnostic> {
    subjects
        .iter()
        .filter_map(|subject| warning(CLOSURE_INCOMPLETE, subject, detail))
        .collect()
}

/// The advisory artifact-evidence rows (one per incomplete subject).
pub fn artifact_evidence_incomplete(
    subjects: &[String],
    detail: &str,
) -> Vec<crate::diagnostics::Diagnostic> {
    subjects
        .iter()
        .filter_map(|subject| warning(ARTIFACT_EVIDENCE_INCOMPLETE, subject, detail))
        .collect()
}

/// One advisory baseline-incomparable row.
pub fn baseline_incomparable(subject: &str, detail: &str) -> Vec<crate::diagnostics::Diagnostic> {
    warning(BASELINE_INCOMPARABLE, subject, detail)
        .into_iter()
        .collect()
}

/// The advisory over-budget rows (one per over-budget subject).
pub fn budget_exceeded(subjects: &[String]) -> Vec<crate::diagnostics::Diagnostic> {
    subjects
        .iter()
        .filter_map(|subject| warning(BUDGET_EXCEEDED, subject, "over-budget"))
        .collect()
}

/// The advisory baseline-regression rows (one per regressed subject).
pub fn baseline_regression(subjects: &[String]) -> Vec<crate::diagnostics::Diagnostic> {
    subjects
        .iter()
        .filter_map(|subject| warning(BASELINE_REGRESSION, subject, "regression"))
        .collect()
}

/// The `denied` set for a failed mandatory policy; the report rides
/// alongside it, never suppressed.
pub fn policy_denied(subject: &str, detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    data.insert("subject".to_owned(), token(subject));
    match one(POLICY_DENIED, None, data) {
        Ok(diagnostic) => finalize(vec![diagnostic], Status::Denied),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_invalid_carries_a_bounded_detail_token() {
        let set = input_invalid_detail("budget-out-of-range", Some(&"x".repeat(4_096)));
        let rendered = serde_json::to_string(&set).expect("set serializes");
        assert!(rendered.contains("context.input-invalid"));
        assert!(rendered.contains("LEK-CONTEXT-001"));
        // The bounded token never echoes the whole hostile input.
        assert!(rendered.len() < 4_096);
    }

    #[test]
    fn unsupported_version_carries_the_profile_rule() {
        let set = profile_unsupported("estimator-identity", None);
        let rendered = serde_json::to_string(&set).expect("set serializes");
        assert!(rendered.contains("context.profile-unsupported"));
        assert!(rendered.contains("LEK-CONTEXT-002"));
        let status = crate::result::Status::UnsupportedVersion;
        // The set is allowed under its own envelope status.
        assert!(crate::diagnostics::DiagnosticSet::try_from_unsorted(
            set.as_slice().to_vec(),
            status
        )
        .is_ok());
    }

    #[test]
    fn advisory_rules_are_warning_rows() {
        let rows = budget_exceeded(&["operation:planner.sync".to_owned()]);
        assert_eq!(rows.len(), 1);
        let rendered = serde_json::to_string(&rows).expect("rows serialize");
        assert!(rendered.contains("context.budget-exceeded"));
        assert!(rendered.contains("LEK-CONTEXT-006"));
    }

    #[test]
    fn policy_denied_is_a_denied_set() {
        let set = policy_denied("operation:planner.sync", "over-budget");
        let rendered = serde_json::to_string(&set).expect("set serializes");
        assert!(rendered.contains("context.policy-denied"));
        assert!(rendered.contains("LEK-CONTEXT-008"));
    }

    #[test]
    fn registry_entry_is_active_with_both_vectors() {
        let registry = crate::diagnostics::registry::DiagnosticRegistry::embedded()
            .expect("embedded registry");
        let entry = registry.entry(BUDGET_EXCEEDED).expect("registered");
        assert!(entry.allows_status(crate::result::Status::Valid));
        assert!(!entry.allows_status(crate::result::Status::Invalid));
        let denied = registry.entry(POLICY_DENIED).expect("registered");
        assert!(denied.allows_status(crate::result::Status::Denied));
        assert!(!denied.allows_status(crate::result::Status::Valid));
    }
}
