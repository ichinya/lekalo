//! The versioning → diagnostic wire adapter (issue #11).
//!
//! Maps every closed [`VersioningFailure`] variant onto its registered rule
//! with typed data. The adapter never widens the failure vocabulary: ids,
//! statuses, and exit classes are exactly the accepted #9 ones, and every
//! echoed token passes the bounded-token invariant (`types::token_value`),
//! so hostile input can neither scale the envelope nor smuggle control
//! characters into a wire item.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataValue};
use crate::diagnostics::DataObject;
use crate::result::{DomainResult, Status};

use super::migration::VersioningFailure;
use super::support::TargetMalformation;

/// The closed detail token of a refused selector or version spelling.
fn malformation_detail(detail: TargetMalformation) -> &'static str {
    match detail {
        TargetMalformation::Malformed => "malformed",
        TargetMalformation::BuildMetadata => "build-metadata",
        TargetMalformation::NotAscii => "not-ascii",
    }
}

/// Build one wire diagnostic for a versioning rule.
fn one(id: &str, data: DataObject) -> crate::diagnostics::Diagnostic {
    build(id, None, None, data).expect("versioning rules are registered and active")
}

impl From<&VersioningFailure> for DomainResult {
    fn from(failure: &VersioningFailure) -> Self {
        let mut data = DataObject::new();
        let (id, status) = match failure {
            VersioningFailure::RegistryInvalid | VersioningFailure::AmbiguousPath => {
                ("versioning.registry-invalid", Status::Invalid)
            }
            VersioningFailure::InvalidVersion(detail) => {
                data.insert(
                    "detail".to_owned(),
                    token_value(malformation_detail(*detail)),
                );
                ("versioning.invalid-version", Status::Invalid)
            }
            VersioningFailure::Unsupported {
                version,
                state,
                replacement,
            } => {
                data.insert("version".to_owned(), token_value(version));
                data.insert("state".to_owned(), token_value(state));
                if let Some(replacement) = replacement {
                    data.insert("replacement".to_owned(), token_value(replacement));
                }
                ("versioning.unsupported-version", Status::UnsupportedVersion)
            }
            VersioningFailure::NoMigrationPath { from, to } => {
                data.insert("from".to_owned(), token_value(from));
                data.insert("to".to_owned(), token_value(to));
                ("versioning.no-migration-path", Status::UnsupportedVersion)
            }
            VersioningFailure::MigrationPrecondition { violations } => {
                let records = violations
                    .iter()
                    .map(|violation| {
                        let mut record = std::collections::BTreeMap::new();
                        record.insert(
                            "id".to_owned(),
                            crate::diagnostics::NestedValue::Scalar(
                                crate::diagnostics::Scalar::Token(bound_token(&violation.id)),
                            ),
                        );
                        record.insert(
                            "rule".to_owned(),
                            crate::diagnostics::NestedValue::Scalar(
                                crate::diagnostics::Scalar::Token(bound_token(
                                    violation.rule.as_str(),
                                )),
                            ),
                        );
                        record
                    })
                    .collect();
                data.insert("violations".to_owned(), DataValue::Records(records));
                ("versioning.migration-precondition", Status::Invalid)
            }
            VersioningFailure::NotFileMigratable { path, detail } => {
                data.insert("path".to_owned(), token_value(path));
                data.insert("detail".to_owned(), token_value(detail));
                ("versioning.not-file-migratable", Status::Invalid)
            }
            VersioningFailure::SourceChanged => ("versioning.source-changed", Status::Invalid),
            VersioningFailure::MigrationInProgress => {
                ("versioning.migration-in-progress", Status::Invalid)
            }
            VersioningFailure::BackupFailed => ("versioning.backup-failed", Status::Invalid),
            VersioningFailure::CommitFailed => ("versioning.commit-failed", Status::Invalid),
            VersioningFailure::RollbackConflict => {
                ("versioning.rollback-conflict", Status::Invalid)
            }
            VersioningFailure::RollbackFailed => ("versioning.rollback-failed", Status::Invalid),
            VersioningFailure::RecoveryRequired => {
                ("versioning.recovery-required", Status::Invalid)
            }
            // The accepted #9 mapping keeps transaction-phase refusals under
            // the backup-failed rule; the logical path is not echoed.
            VersioningFailure::DeniedPath(_) | VersioningFailure::Io { .. } => {
                ("versioning.backup-failed", Status::Invalid)
            }
        };
        let diagnostic = one(id, data);
        let set = DomainResult::from_wire_set(status, vec![diagnostic]);
        match status {
            Status::UnsupportedVersion => DomainResult::unsupported_version(set),
            _ => DomainResult::invalid(set),
        }
    }
}

impl From<VersioningFailure> for DomainResult {
    fn from(failure: VersioningFailure) -> Self {
        Self::from(&failure)
    }
}

impl From<&super::migration::PlanFailure> for DomainResult {
    fn from(failure: &super::migration::PlanFailure) -> Self {
        match failure {
            super::migration::PlanFailure::Loader(outcome) => outcome.clone(),
            super::migration::PlanFailure::Versioning(failure) => Self::from(failure),
        }
    }
}
