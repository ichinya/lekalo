//! Managed diagnostic identity and typed failure construction (issue #41).
//!
//! Every managed rule is registered in the `0.8.0` union successor; the
//! status class of each failure comes from the registry, never from the
//! caller.

use crate::diagnostics::types::{token_value, DataObject};
use crate::diagnostics::DiagnosticSet;
use crate::result::{DomainResult, Status};

pub const INPUT_INVALID: &str = "managed.input-invalid";
pub const VERSION_UNSUPPORTED: &str = "managed.version-unsupported";
pub const CAPABILITY_GAP: &str = "managed.capability-gap";
pub const GENERATED_DRIFT: &str = "managed.generated-drift";
pub const EXTENSION_UNRESOLVED: &str = "managed.extension-unresolved";
pub const PLAN_STALE: &str = "managed.plan-stale";
pub const ARTIFACT_COVERAGE_INCOMPLETE: &str = "managed.artifact-coverage-incomplete";
pub const EQUIVALENCE_FAILED: &str = "managed.equivalence-failed";
pub const EQUIVALENCE_INCOMPLETE: &str = "managed.equivalence-incomplete";
pub const ROLLBACK_REFUSED: &str = "managed.rollback-refused";
pub const RECOVERY_REQUIRED: &str = "managed.recovery-required";
pub const OWNERSHIP_DENIED: &str = "managed.ownership-denied";

/// The stable registry-backed status of one managed rule.
pub fn status_of(id: &str) -> Status {
    match id {
        VERSION_UNSUPPORTED => Status::UnsupportedVersion,
        CAPABILITY_GAP | EQUIVALENCE_INCOMPLETE => Status::Unsupported,
        OWNERSHIP_DENIED => Status::Denied,
        // Every other managed rule invalidates the run (exit 1, stderr).
        _ => Status::Invalid,
    }
}

/// Build one registry-backed managed diagnostic set with bounded token
/// data (`module`, `target`, `requirement`, `path`, `pin`, `reason`).
pub fn diagnostic(id: &str, data: &[(&str, &str)]) -> DiagnosticSet {
    let object: DataObject = data
        .iter()
        .map(|(key, value)| ((*key).to_owned(), token_value(value)))
        .collect();
    let built = crate::diagnostics::normalize::build(id, None, None, object).unwrap_or_else(|_| {
        crate::diagnostics::normalize::build(id, None, None, DataObject::new())
            .expect("registered managed rule")
    });
    DiagnosticSet::try_from_unsorted(vec![built], status_of(id))
        .expect("managed registry status admits its rule")
}

/// The typed failure envelope for one managed rule.
pub fn failure(id: &str, data: &[(&str, &str)]) -> DomainResult {
    let set = diagnostic(id, data);
    match status_of(id) {
        Status::UnsupportedVersion => DomainResult::unsupported_version(set),
        Status::Denied => DomainResult::denied(set),
        Status::Unsupported => DomainResult::UnsupportedOperation { diagnostics: set },
        _ => DomainResult::invalid(set),
    }
}

/// Attach the closed data fields shared by every managed failure.
pub fn detail(reason: &str) -> [(&str, &str); 1] {
    [("reason", reason)]
}
