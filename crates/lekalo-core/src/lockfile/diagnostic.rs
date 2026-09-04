//! The lockfile → diagnostic wire adapter (issue #11).
//!
//! Maps every closed [`LockFailure`] variant onto its registered rule with
//! typed data. The #10 status, exit class, and stream are preserved exactly;
//! the envelope carries the authoritative closed-wire diagnostics, and every
//! echoed token passes the bounded-token invariant (`types::token_value`),
//! so hostile input can neither scale the envelope nor smuggle control
//! characters into a wire item.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::token_value;
use crate::diagnostics::DataObject;
use crate::result::{DomainResult, Status};

use super::LockFailure;

/// Build one wire diagnostic for a lock rule.
fn one(id: &str, data: DataObject) -> crate::diagnostics::Diagnostic {
    build(id, None, None, data).expect("lock rules are registered and active")
}

impl From<&LockFailure> for DomainResult {
    fn from(failure: &LockFailure) -> Self {
        let mut data = DataObject::new();
        let (id, status) = match failure {
            LockFailure::Loader(outcome) => return outcome.clone(),
            LockFailure::Missing => ("lock.missing", Status::Invalid),
            LockFailure::SchemaInvalid => ("lock.schema-invalid", Status::Invalid),
            LockFailure::UnsupportedSchemaVersion { found } => {
                data.insert("found".to_owned(), token_value(found));
                (
                    "lock.unsupported-schema-version",
                    Status::UnsupportedVersion,
                )
            }
            LockFailure::Noncanonical => ("lock.noncanonical", Status::Invalid),
            LockFailure::ReferenceInvalid => ("lock.reference-invalid", Status::Invalid),
            LockFailure::ResolutionAmbiguous { identity } => {
                data.insert("identity".to_owned(), token_value(identity));
                ("lock.resolution-ambiguous", Status::Invalid)
            }
            LockFailure::ProfileUnversioned { id: profile } => {
                data.insert("id".to_owned(), token_value(profile));
                ("lock.profile-unversioned", Status::Invalid)
            }
            LockFailure::Stale => ("lock.stale", Status::Invalid),
            LockFailure::PreviewRequired => ("lock.preview-required", Status::Invalid),
            LockFailure::SourceChanged => ("lock.source-changed", Status::Invalid),
            LockFailure::UpdateInProgress => ("lock.update-in-progress", Status::Invalid),
            LockFailure::CommitFailed => ("lock.commit-failed", Status::Invalid),
            LockFailure::RecoveryRequired => ("lock.recovery-required", Status::Invalid),
            LockFailure::PathDenied => ("lock.path-denied", Status::Denied),
            // The structure passthrough keeps its own rule identity; the
            // denied flag decides the exit class, not the envelope status.
            LockFailure::Structure { code, denied } => {
                let status = if *denied {
                    Status::Denied
                } else {
                    Status::Invalid
                };
                let id = code.as_str();
                let diagnostic = one(id, data);
                let set = DomainResult::from_wire_set(status, vec![diagnostic]);
                return match status {
                    Status::Denied => DomainResult::denied(set),
                    _ => DomainResult::invalid(set),
                };
            }
            LockFailure::PrivateData { field } => {
                data.insert("field".to_owned(), token_value(field));
                ("lock.private-data-forbidden", Status::Denied)
            }
            LockFailure::DigestMismatch { domain, identity } => {
                data.insert("domain".to_owned(), token_value(domain));
                data.insert("identity".to_owned(), token_value(identity));
                ("lock.digest-mismatch", Status::Denied)
            }
            LockFailure::CatalogUnavailable => ("lock.catalog-unavailable", Status::Unavailable),
            LockFailure::ComponentUnavailable {
                kind,
                id: component,
            } => {
                data.insert("kind".to_owned(), token_value(kind));
                data.insert("id".to_owned(), token_value(component));
                ("lock.component-unavailable", Status::Unavailable)
            }
            LockFailure::PlatformUnavailable { adapter, platform } => {
                data.insert("adapter".to_owned(), token_value(adapter));
                data.insert("platform".to_owned(), token_value(platform));
                ("lock.platform-unavailable", Status::Unavailable)
            }
            LockFailure::ProviderUnavailable => {
                ("lock.resolution-provider-unavailable", Status::Unavailable)
            }
            LockFailure::ComponentVersionUnsupported { family, version } => {
                data.insert("family".to_owned(), token_value(family));
                data.insert("version".to_owned(), token_value(version));
                (
                    "lock.component-version-unsupported",
                    Status::UnsupportedVersion,
                )
            }
            LockFailure::ProtocolUnpublished => (
                "versioning.protocol-unpublished",
                Status::UnsupportedVersion,
            ),
            LockFailure::AdapterIncompatible { adapter } => {
                data.insert("adapter".to_owned(), token_value(adapter));
                (
                    "versioning.adapter-incompatible",
                    Status::UnsupportedVersion,
                )
            }
            LockFailure::ExtensionIncompatible { adapter } => {
                data.insert("adapter".to_owned(), token_value(adapter));
                (
                    "versioning.extension-incompatible",
                    Status::UnsupportedVersion,
                )
            }
        };
        let diagnostic = one(id, data);
        let set = DomainResult::from_wire_set(status, vec![diagnostic]);
        match status {
            Status::Invalid => DomainResult::invalid(set),
            Status::Denied => DomainResult::denied(set),
            Status::Unavailable => DomainResult::unavailable(set),
            Status::UnsupportedVersion => DomainResult::unsupported_version(set),
            Status::Valid | Status::Unsupported => DomainResult::invalid(set),
        }
    }
}

impl From<LockFailure> for DomainResult {
    fn from(failure: LockFailure) -> Self {
        Self::from(&failure)
    }
}
