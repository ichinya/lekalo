//! The target protocol → diagnostic wire adapter (issue #27).
//!
//! Maps every closed [`TargetFailure`] variant onto its registered rule
//! with typed bounded data. The status — and therefore the exit class —
//! lives here and nowhere else; every echoed token passes the bounded-token
//! invariant, and no raw operating-system text, host data, or child output
//! enters a diagnostic item.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::token_value;
use crate::diagnostics::DataObject;
use crate::result::{DomainResult, Status};

use super::TargetFailure;

/// Build one wire diagnostic for a target rule.
fn one(id: &str, data: DataObject) -> crate::diagnostics::Diagnostic {
    build(id, None, None, data).expect("target rules are registered and active")
}

/// The registered rule id and status of one failure.
pub fn rule_for(failure: &TargetFailure) -> (&'static str, Status) {
    match failure {
        TargetFailure::ProtocolUnpublished => (
            "versioning.protocol-unpublished",
            Status::UnsupportedVersion,
        ),
        TargetFailure::RegistryInvalid => ("versioning.registry-invalid", Status::Invalid),
        TargetFailure::RequestInvalid { .. } => ("target.request-invalid", Status::Invalid),
        TargetFailure::HandshakeRequired { .. } => ("target.handshake-required", Status::Invalid),
        TargetFailure::ProtocolMismatch { .. } => {
            ("target.protocol-mismatch", Status::UnsupportedVersion)
        }
        TargetFailure::CapabilityUnsupported { .. } => {
            ("target.capability-unsupported", Status::Unsupported)
        }
        TargetFailure::ScopeViolation { .. } => ("target.scope-violation", Status::Denied),
        TargetFailure::ProtectedPath { .. } => ("target.protected-path", Status::Denied),
        TargetFailure::DryRunMutation { .. } => ("target.dry-run-mutation", Status::Denied),
        TargetFailure::PlanMismatch { .. } => ("target.plan-mismatch", Status::Invalid),
        TargetFailure::OperationFailed { .. } => ("target.operation-failed", Status::Invalid),
        TargetFailure::ResponseInvalid { .. } => ("target.response-invalid", Status::Unavailable),
        TargetFailure::TransportFailed { .. } => ("target.transport-failed", Status::Unavailable),
        TargetFailure::Timeout => ("target.timeout", Status::Unavailable),
        TargetFailure::Crash { .. } => ("target.crash", Status::Unavailable),
        TargetFailure::OutputLimit { .. } => ("target.output-limit", Status::Unavailable),
        TargetFailure::Cancelled => ("target.cancelled", Status::Unavailable),
    }
}

impl From<&TargetFailure> for DomainResult {
    fn from(failure: &TargetFailure) -> Self {
        let (id, status) = rule_for(failure);
        let mut data = DataObject::new();
        match failure {
            TargetFailure::ProtocolUnpublished | TargetFailure::RegistryInvalid => {}
            TargetFailure::RequestInvalid { detail } => {
                data.insert("detail".to_owned(), token_value(detail));
            }
            TargetFailure::HandshakeRequired { operation } => {
                data.insert("operation".to_owned(), token_value(operation.as_str()));
            }
            TargetFailure::ProtocolMismatch { detail } => {
                data.insert("detail".to_owned(), token_value(detail.detail()));
            }
            TargetFailure::CapabilityUnsupported { detail } => {
                data.insert("detail".to_owned(), token_value(detail));
            }
            TargetFailure::ScopeViolation { path, detail } => {
                if let Some(path) = path {
                    data.insert("path".to_owned(), token_value(path));
                }
                data.insert("detail".to_owned(), token_value(detail));
            }
            TargetFailure::ProtectedPath { path, home } => {
                data.insert("path".to_owned(), token_value(path));
                data.insert("detail".to_owned(), token_value(home));
            }
            TargetFailure::DryRunMutation { path } => {
                if let Some(path) = path {
                    data.insert("path".to_owned(), token_value(path));
                }
            }
            TargetFailure::PlanMismatch { path, detail } => {
                if let Some(path) = path {
                    data.insert("path".to_owned(), token_value(path));
                }
                data.insert("detail".to_owned(), token_value(detail));
            }
            TargetFailure::OperationFailed { class, code, .. } => {
                data.insert("class".to_owned(), token_value(class.as_str()));
                data.insert("code".to_owned(), token_value(code));
            }
            TargetFailure::ResponseInvalid { detail } => {
                data.insert("detail".to_owned(), token_value(detail.detail()));
            }
            TargetFailure::TransportFailed { detail } => {
                data.insert("detail".to_owned(), token_value(detail));
            }
            TargetFailure::Timeout => {
                data.insert("detail".to_owned(), token_value("deadline"));
            }
            TargetFailure::Crash { detail } => {
                data.insert("detail".to_owned(), token_value(detail));
            }
            TargetFailure::OutputLimit { stream } => {
                data.insert("stream".to_owned(), token_value(stream.as_str()));
            }
            TargetFailure::Cancelled => {
                data.insert("detail".to_owned(), token_value("caller"));
            }
        }
        let diagnostic = one(id, data);
        let set = DomainResult::from_wire_set(status, vec![diagnostic]);
        match status {
            Status::Invalid => DomainResult::Invalid { diagnostics: set },
            Status::Denied => DomainResult::Denied { diagnostics: set },
            Status::Unavailable => DomainResult::Unavailable { diagnostics: set },
            Status::UnsupportedVersion => DomainResult::UnsupportedVersion { diagnostics: set },
            _ => DomainResult::Unavailable { diagnostics: set },
        }
    }
}

impl From<TargetFailure> for DomainResult {
    fn from(failure: TargetFailure) -> Self {
        Self::from(&failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_failure_projects_a_registered_rule_and_status() {
        let cases: Vec<(TargetFailure, &'static str, Status)> = vec![
            (
                TargetFailure::ProtocolMismatch {
                    detail: crate::target_protocol::wire::ProtocolMismatch::Token,
                },
                "target.protocol-mismatch",
                Status::UnsupportedVersion,
            ),
            (
                TargetFailure::Timeout,
                "target.timeout",
                Status::Unavailable,
            ),
            (
                TargetFailure::ProtectedPath {
                    path: "lekalo/project.yaml".to_owned(),
                    home: "lekalo-model",
                },
                "target.protected-path",
                Status::Denied,
            ),
            (
                TargetFailure::RequestInvalid { detail: "dry-run" },
                "target.request-invalid",
                Status::Invalid,
            ),
        ];
        for (failure, id, status) in cases {
            assert_eq!(failure.rule(), (id, status));
            let result = DomainResult::from(failure);
            assert_eq!(result.status(), status);
        }
    }
}
