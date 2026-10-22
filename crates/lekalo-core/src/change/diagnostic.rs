//! Change diagnostics routed through the accepted #11 contract (issue #73).
//!
//! Every change failure is one registered `change.*` rule
//! (`LEK-CHANGE-001..012`, registry 0.8.0) assembled through the shared
//! registry-backed constructor and finalized into a normalized
//! [`DiagnosticSet`]. Every echo is a bounded fixed token validated by the
//! selector grammar first — raw input, paths, and secrets never enter a
//! diagnostic. Severity never computes an exit: the failure status does.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject};
use crate::diagnostics::DiagnosticSet;
use crate::result::{DomainResult, Status};

/// The request or one operation payload is malformed or out of bounds.
pub const REQUEST_INVALID: &str = "change.request-invalid";
/// A cataloged operation the running implementation does not provide.
pub const OPERATION_UNSUPPORTED: &str = "change.operation-unsupported";
/// The target is missing, ambiguous, wrong-kind, duplicate, or colliding.
pub const SUBJECT_CONFLICT: &str = "change.subject-conflict";
/// The candidate has breaking classes the effective policy does not allow.
pub const COMPATIBILITY_DENIED: &str = "change.compatibility-denied";
/// An expected precondition changed between preparation and application.
pub const PRECONDITION_FAILED: &str = "change.precondition-failed";
/// The idempotency key is already bound to a different change intent.
pub const IDEMPOTENCY_CONFLICT: &str = "change.idempotency-conflict";
/// No proven non-overlapping source edit exists for the accepted change.
pub const SOURCE_PRESERVATION_UNAVAILABLE: &str = "change.source-preservation-unavailable";
/// A destination violates the write boundary or path safety rules.
pub const WRITE_BOUNDARY_DENIED: &str = "change.write-boundary-denied";
/// The transaction failed and its originals were restored.
pub const COMMIT_FAILED: &str = "change.commit-failed";
/// An interrupted transaction blocks readers until explicit recovery.
pub const RECOVERY_REQUIRED: &str = "change.recovery-required";
/// Required impact, generation, or ownership evidence is missing or stale.
pub const REQUIRED_EVIDENCE_UNAVAILABLE: &str = "change.required-evidence-unavailable";
/// The canonical change history is malformed, unordered, or fails integrity.
pub const HISTORY_INVALID: &str = "change.history-invalid";

/// One terminal change failure with its registered outcome.
#[derive(Clone, Debug, PartialEq)]
pub enum ChangeError {
    /// Malformed request or operation payload (`invalid`, exit 1).
    RequestInvalid { detail: &'static str },
    /// Wrong discriminator of the request document.
    UnsupportedVersion { version: String },
    /// Known catalog operation without a sealed executor (`unsupported`, exit 4).
    OperationUnsupported { operation: String },
    /// Missing, duplicate, wrong-kind, or colliding subject (`invalid`, exit 1).
    SubjectConflict {
        subject: String,
        detail: &'static str,
    },
    /// Declared typed default or wire value outside the accepted Model.
    UnsupportedMember {
        operation: String,
        member: &'static str,
    },
    /// Breaking candidate under a policy that does not allow it (`denied`, exit 3).
    CompatibilityDenied { classes: Vec<String> },
    /// Changed precondition between prepare and apply (`denied`, exit 3).
    PreconditionFailed {
        subject: String,
        detail: &'static str,
    },
    /// Key already bound to a different intent (`denied`, exit 3).
    IdempotencyConflict,
    /// Unproven source edit (`denied`, exit 3).
    SourcePreservation {
        subject: String,
        detail: &'static str,
    },
    /// Path safety or write-boundary refusal (`denied`, exit 3).
    WriteBoundaryDenied,
    /// Transaction failure with restoration (`invalid`, exit 1).
    CommitFailed {
        transaction: String,
        detail: &'static str,
    },
    /// Interrupted transaction; readers block (`denied`, exit 3).
    RecoveryRequired { transaction: String },
    /// Malformed canonical history (`invalid`, exit 1).
    HistoryInvalid { detail: &'static str },
    /// The sealed plan's export exceeds its own published bounded-schema
    /// cap (r6, finding omp F1): input pins, changed files, or serialized
    /// bytes beyond the contract ceiling refuse the preparation before
    /// any schema-invalid plan can exist (`invalid`, exit 1).
    PlanExportLimit {
        subject: &'static str,
        measured: u64,
        allowed: u64,
    },
    /// Policy document refused.
    PolicyInvalid { detail: &'static str },
    /// Required evidence missing, partial, or stale (`denied`, exit 3).
    EvidenceUnavailable { capability: &'static str },
    /// The selected project root is structurally invalid or unreadable.
    LoadFailed(DomainResult),
}

impl ChangeError {
    /// The envelope status this failure maps to (result.rs).
    pub fn status(&self) -> Status {
        match self {
            Self::RequestInvalid { .. }
            | Self::SubjectConflict { .. }
            | Self::CommitFailed { .. }
            | Self::HistoryInvalid { .. }
            | Self::PlanExportLimit { .. }
            | Self::PolicyInvalid { .. }
            | Self::LoadFailed(_) => Status::Invalid,
            Self::UnsupportedVersion { .. } => Status::UnsupportedVersion,
            Self::OperationUnsupported { .. } | Self::UnsupportedMember { .. } => {
                Status::Unsupported
            }
            Self::CompatibilityDenied { .. }
            | Self::PreconditionFailed { .. }
            | Self::IdempotencyConflict
            | Self::SourcePreservation { .. }
            | Self::WriteBoundaryDenied
            | Self::RecoveryRequired { .. }
            | Self::EvidenceUnavailable { .. } => Status::Denied,
        }
    }
}

/// Build one registered diagnostic; a registry failure collapses to the
/// invariant set (double developer fault) instead of panicking.
fn one(id: &str, data: DataObject) -> Option<crate::diagnostics::Diagnostic> {
    build(id, None, None, data).ok()
}

/// Build a validated set of any allowed status or collapse to the invariant.
fn set(status: Status, diagnostics: Vec<crate::diagnostics::Diagnostic>) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, status)
        .unwrap_or_else(|_| crate::result::fallback_set())
}

fn token_set(id: &str, status: Status, data: DataObject) -> DiagnosticSet {
    match one(id, data) {
        Some(diagnostic) => set(status, vec![diagnostic]),
        None => crate::result::fallback_set(),
    }
}

impl ChangeError {
    /// The normalized diagnostic set of this failure.
    pub fn diagnostic_set(&self) -> DiagnosticSet {
        let status = self.status();
        match self {
            Self::LoadFailed(result) => {
                // The loader/validator diagnostics pass through unchanged;
                // the original producer diagnostics keep their identities.
                let diagnostics = result.diagnostics().to_vec();
                DiagnosticSet::try_from_unsorted(diagnostics, status)
                    .unwrap_or_else(|_| crate::result::fallback_set())
            }
            Self::RequestInvalid { detail } => {
                let mut data = DataObject::new();
                data.insert("detail".to_owned(), token_value(detail));
                token_set(REQUEST_INVALID, status, data)
            }
            Self::UnsupportedVersion { version } => {
                let mut data = DataObject::new();
                data.insert("version".to_owned(), token_value(version));
                data.insert("state".to_owned(), token_value("unregistered"));
                token_set("versioning.unsupported-version", status, data)
            }
            Self::OperationUnsupported { operation } => {
                let mut data = DataObject::new();
                data.insert("operation".to_owned(), token_value(operation));
                data.insert("detail".to_owned(), token_value("not-provided"));
                token_set(OPERATION_UNSUPPORTED, status, data)
            }
            Self::UnsupportedMember { operation, member } => {
                let mut data = DataObject::new();
                data.insert("operation".to_owned(), token_value(operation));
                data.insert("detail".to_owned(), token_value(member));
                token_set(OPERATION_UNSUPPORTED, status, data)
            }
            Self::SubjectConflict { subject, detail } => {
                let mut data = DataObject::new();
                data.insert("subject".to_owned(), token_value(subject));
                data.insert("detail".to_owned(), token_value(detail));
                token_set(SUBJECT_CONFLICT, status, data)
            }
            Self::CompatibilityDenied { classes } => {
                let mut data = DataObject::new();
                data.insert("classes".to_owned(), token_value(&classes.join(",")));
                data.insert("detail".to_owned(), token_value("policy-restricted"));
                token_set(COMPATIBILITY_DENIED, status, data)
            }
            Self::PreconditionFailed { subject, detail } => {
                let mut data = DataObject::new();
                data.insert("subject".to_owned(), token_value(subject));
                data.insert("detail".to_owned(), token_value(detail));
                token_set(PRECONDITION_FAILED, status, data)
            }
            Self::IdempotencyConflict => {
                let mut data = DataObject::new();
                data.insert("key".to_owned(), token_value("pinned"));
                data.insert("detail".to_owned(), token_value("intent-mismatch"));
                token_set(IDEMPOTENCY_CONFLICT, status, data)
            }
            Self::SourcePreservation { subject, detail } => {
                let mut data = DataObject::new();
                data.insert("subject".to_owned(), token_value(subject));
                data.insert("detail".to_owned(), token_value(detail));
                token_set(SOURCE_PRESERVATION_UNAVAILABLE, status, data)
            }
            Self::WriteBoundaryDenied => {
                let mut data = DataObject::new();
                data.insert("subject".to_owned(), token_value("write-target"));
                data.insert("detail".to_owned(), token_value("boundary"));
                token_set(WRITE_BOUNDARY_DENIED, status, data)
            }
            Self::CommitFailed {
                transaction,
                detail,
            } => {
                let mut data = DataObject::new();
                data.insert("transaction".to_owned(), token_value(transaction));
                data.insert("detail".to_owned(), token_value(detail));
                token_set(COMMIT_FAILED, status, data)
            }
            Self::RecoveryRequired { transaction } => {
                let mut data = DataObject::new();
                data.insert("transaction".to_owned(), token_value(transaction));
                data.insert("detail".to_owned(), token_value("journal-present"));
                token_set(RECOVERY_REQUIRED, status, data)
            }
            Self::HistoryInvalid { detail } => {
                let mut data = DataObject::new();
                data.insert("detail".to_owned(), token_value(detail));
                token_set(HISTORY_INVALID, status, data)
            }
            Self::PlanExportLimit {
                subject,
                measured,
                allowed,
            } => {
                let mut data = DataObject::new();
                data.insert("detail".to_owned(), token_value(subject));
                data.insert(
                    "operation".to_owned(),
                    token_value(&format!("plan-export {measured}/{allowed}")),
                );
                token_set(REQUEST_INVALID, status, data)
            }
            Self::PolicyInvalid { detail } => {
                let mut data = DataObject::new();
                data.insert("detail".to_owned(), token_value(detail));
                token_set(REQUEST_INVALID, status, data)
            }
            Self::EvidenceUnavailable { capability } => {
                let mut data = DataObject::new();
                data.insert("capability".to_owned(), token_value(capability));
                data.insert("detail".to_owned(), token_value("unavailable"));
                token_set(REQUIRED_EVIDENCE_UNAVAILABLE, status, data)
            }
        }
    }

    /// The terminal [`DomainResult`] of this failure.
    pub fn into_result(self) -> DomainResult {
        if let Self::LoadFailed(result) = &self {
            return result.clone();
        }
        let set = self.diagnostic_set();
        match self.status() {
            Status::Valid => DomainResult::invalid(set),
            Status::Invalid => DomainResult::invalid(set),
            Status::Denied => DomainResult::denied(set),
            Status::Unavailable => DomainResult::unavailable(set),
            Status::Unsupported => DomainResult::unsupported_operation(set),
            Status::UnsupportedVersion => DomainResult::unsupported_version(set),
        }
    }
}

impl From<&ChangeError> for DomainResult {
    fn from(error: &ChangeError) -> Self {
        error.clone().into_result()
    }
}

impl From<ChangeError> for DomainResult {
    fn from(error: ChangeError) -> Self {
        error.into_result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_follows_result_rs() {
        assert_eq!(
            ChangeError::RequestInvalid { detail: "x" }.status(),
            Status::Invalid
        );
        assert_eq!(
            ChangeError::OperationUnsupported {
                operation: "definition:add".to_owned()
            }
            .status(),
            Status::Unsupported
        );
        assert_eq!(
            ChangeError::CompatibilityDenied { classes: vec![] }.status(),
            Status::Denied
        );
    }

    #[test]
    fn every_failure_carries_its_registered_rule() {
        let cases: Vec<ChangeError> = vec![
            ChangeError::RequestInvalid { detail: "x" },
            ChangeError::OperationUnsupported {
                operation: "definition:add".to_owned(),
            },
            ChangeError::SubjectConflict {
                subject: "tasks.task".to_owned(),
                detail: "duplicate",
            },
            ChangeError::CompatibilityDenied {
                classes: vec!["wire-breaking".to_owned()],
            },
            ChangeError::PreconditionFailed {
                subject: "inputs".to_owned(),
                detail: "digest",
            },
            ChangeError::PlanExportLimit {
                subject: "inputs",
                measured: 1001,
                allowed: 1000,
            },
            ChangeError::IdempotencyConflict,
            ChangeError::SourcePreservation {
                subject: "tasks.task".to_owned(),
                detail: "comment-adjacent",
            },
            ChangeError::WriteBoundaryDenied,
            ChangeError::CommitFailed {
                transaction: "change-ab12cd34-1".to_owned(),
                detail: "replace",
            },
            ChangeError::RecoveryRequired {
                transaction: "change-ab12cd34-1".to_owned(),
            },
            ChangeError::HistoryInvalid { detail: "order" },
            ChangeError::PolicyInvalid { detail: "identity" },
        ];
        for case in cases {
            let result = case.clone().into_result();
            assert_eq!(result.status(), case.status(), "{case:?}");
            let ids: Vec<&str> = result.diagnostics().iter().map(|d| d.id()).collect();
            assert!(!ids.is_empty(), "{case:?} carries no diagnostic");
            for id in ids {
                assert!(
                    id.starts_with("change.") || id.starts_with("versioning.") || id == "cli.usage",
                    "{case:?} carries foreign diagnostic {id}"
                );
            }
        }
    }
}
