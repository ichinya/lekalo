//! Every warning/refusal goes through the active diagnostic registry.

use crate::diagnostics::{normalize::build, types::token_value, Diagnostic, DiagnosticSet};
use crate::result::Status;

pub(super) const MISSING: &str = "trace.bridge-mapping-missing";
pub(super) const UNRESOLVED: &str = "trace.bridge-reference-unresolved";
pub(super) const CONFLICT: &str = "trace.bridge-conflict";
pub(super) const STALE: &str = "trace.bridge-evidence-stale";
pub(super) const PROVIDER: &str = "trace.bridge-provider-unsupported";
pub(super) const EXECUTION: &str = "trace.bridge-execution-unverified";
pub(super) const UNCOVERED: &str = "trace.bridge-chain-uncovered";
pub(super) const INVALID: &str = "trace.bridge-input-invalid";
pub(super) const DENIED: &str = "trace.bridge-policy-denied";

pub(super) fn make(rule: &str, detail: &str, subject: &str) -> Diagnostic {
    let data = [
        ("detail".to_owned(), token_value(detail)),
        ("subject".to_owned(), token_value(subject)),
    ]
    .into_iter()
    .collect();
    build(rule, None, None, data).expect("registered trace assessment rule")
}

pub(super) fn invalid(detail: &str) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(vec![make(INVALID, detail, "evidence")], Status::Invalid)
        .expect("registered invalid trace assessment diagnostic")
}
