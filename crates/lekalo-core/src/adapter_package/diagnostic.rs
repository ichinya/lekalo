//! The adapter package surface → diagnostic wire adapter (issue #32).
//!
//! Maps every closed [`PackageFailure`] variant onto its registered
//! `adapter.*` rule with typed data. The status and exit class are owned
//! by the failure; a diagnostic never computes an exit. Every echoed
//! token passes the bounded-token invariant, so hostile input can
//! neither scale the envelope nor smuggle control characters into a
//! wire item.

use crate::adapter_package::reasons;
use crate::adapter_package::types::PackageFailure;
use crate::diagnostics::normalize::build;
use crate::diagnostics::types::token_value;
use crate::diagnostics::{DataObject, Diagnostic};
use crate::result::{DomainResult, Status};

/// The registered rule id of one package failure.
pub fn reason_of(failure: &PackageFailure) -> &'static str {
    match failure {
        PackageFailure::ManifestInvalid { .. } => reasons::MANIFEST_INVALID,
        PackageFailure::ManifestMismatch { .. } => reasons::MANIFEST_MISMATCH,
        PackageFailure::Incompatible { .. } => reasons::INCOMPATIBLE,
        PackageFailure::ChecksumMismatch { .. } => reasons::CHECKSUM_MISMATCH,
        PackageFailure::SignatureUnverified { .. } => reasons::SIGNATURE_UNVERIFIED,
        PackageFailure::Revoked { .. } => reasons::REVOKED,
        PackageFailure::Quarantined { .. } => reasons::QUARANTINED,
        PackageFailure::TrustInsufficient { .. } => reasons::TRUST_INSUFFICIENT,
        PackageFailure::SourceUnavailable { .. } => reasons::SOURCE_UNAVAILABLE,
        PackageFailure::InstallPlanRequired => reasons::INSTALL_PLAN_REQUIRED,
        PackageFailure::SourceChanged => reasons::SOURCE_CHANGED,
        PackageFailure::InstallConflict { .. } => reasons::INSTALL_CONFLICT,
        PackageFailure::RecoveryRequired { .. } => reasons::RECOVERY_REQUIRED,
        PackageFailure::HooksDeclared { .. } => reasons::HOOKS_DECLARED,
        PackageFailure::PermissionEscalated { .. } => reasons::PERMISSION_ESCALATED,
    }
}

/// The envelope status of one package failure.
pub(crate) fn status_of(failure: &PackageFailure) -> Status {
    match failure {
        PackageFailure::ManifestInvalid { .. }
        | PackageFailure::InstallPlanRequired
        | PackageFailure::SourceChanged
        | PackageFailure::RecoveryRequired { .. } => Status::Invalid,
        PackageFailure::ManifestMismatch { .. }
        | PackageFailure::ChecksumMismatch { .. }
        | PackageFailure::Revoked { .. }
        | PackageFailure::Quarantined { .. }
        | PackageFailure::TrustInsufficient { .. }
        | PackageFailure::InstallConflict { .. }
        | PackageFailure::PermissionEscalated { .. } => Status::Denied,
        PackageFailure::Incompatible { .. } => Status::UnsupportedVersion,
        PackageFailure::SignatureUnverified { .. }
        | PackageFailure::SourceUnavailable { .. }
        | PackageFailure::HooksDeclared { .. } => Status::Unavailable,
    }
}

/// Build one wire diagnostic for a package failure.
fn one(failure: &PackageFailure) -> Diagnostic {
    let mut data = DataObject::new();
    match failure {
        PackageFailure::ManifestInvalid { reason } => {
            data.insert("reason".to_owned(), token_value(reason));
        }
        PackageFailure::ManifestMismatch { field } => {
            data.insert("field".to_owned(), token_value(field));
        }
        PackageFailure::Incompatible { adapter } => {
            data.insert("adapter".to_owned(), token_value(adapter));
        }
        PackageFailure::ChecksumMismatch { domain, identity } => {
            data.insert("domain".to_owned(), token_value(domain));
            data.insert("identity".to_owned(), token_value(identity));
        }
        PackageFailure::SignatureUnverified { scheme } => {
            data.insert("scheme".to_owned(), token_value(scheme));
        }
        PackageFailure::Revoked { id, version } => {
            data.insert("id".to_owned(), token_value(id));
            data.insert("version".to_owned(), token_value(version));
        }
        PackageFailure::Quarantined { id, version } => {
            data.insert("id".to_owned(), token_value(id));
            data.insert("version".to_owned(), token_value(version));
        }
        PackageFailure::TrustInsufficient { id, level } => {
            data.insert("id".to_owned(), token_value(id));
            data.insert("level".to_owned(), token_value(level));
        }
        PackageFailure::SourceUnavailable { source } => {
            data.insert("source".to_owned(), token_value(source));
        }
        PackageFailure::InstallPlanRequired | PackageFailure::SourceChanged => {}
        PackageFailure::InstallConflict { path } => {
            data.insert("path".to_owned(), token_value(path));
        }
        PackageFailure::RecoveryRequired { stage } => {
            data.insert("stage".to_owned(), token_value(stage));
        }
        PackageFailure::HooksDeclared { count } => {
            data.insert(
                "count".to_owned(),
                crate::diagnostics::types::DataValue::Count(*count),
            );
        }
        PackageFailure::PermissionEscalated { adapter, member } => {
            data.insert("adapter".to_owned(), token_value(adapter));
            data.insert("member".to_owned(), token_value(member));
        }
    }
    build(reason_of(failure), None, None, data)
        .expect("adapter package rules are registered and active")
}

/// Project one failure onto the full domain envelope: the registered
/// rule, the fixed status, and the accepted exit class.
pub fn domain_result(failure: &PackageFailure) -> DomainResult {
    let diagnostic = one(failure);
    let status = status_of(failure);
    let set = DomainResult::from_wire_set(status, vec![diagnostic]);
    match status {
        Status::Invalid => DomainResult::invalid(set),
        Status::Denied => DomainResult::denied(set),
        Status::Unavailable => DomainResult::unavailable(set),
        Status::UnsupportedVersion => DomainResult::unsupported_version(set),
        _ => DomainResult::invalid(set),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_failure_maps_onto_its_registered_rule_and_exit() {
        let cases: Vec<(PackageFailure, &str, u8)> = vec![
            (
                PackageFailure::ManifestInvalid {
                    reason: "grammar".to_owned(),
                },
                "adapter.manifest-invalid",
                1,
            ),
            (
                PackageFailure::ManifestMismatch {
                    field: "capabilities".to_owned(),
                },
                "adapter.manifest-mismatch",
                3,
            ),
            (
                PackageFailure::Incompatible {
                    adapter: "some-adapter".to_owned(),
                },
                "adapter.incompatible",
                5,
            ),
            (
                PackageFailure::ChecksumMismatch {
                    domain: "package".to_owned(),
                    identity: "some-adapter".to_owned(),
                },
                "adapter.checksum-mismatch",
                3,
            ),
            (
                PackageFailure::SignatureUnverified {
                    scheme: "minisign".to_owned(),
                },
                "adapter.signature-unverified",
                4,
            ),
            (
                PackageFailure::Revoked {
                    id: "some-adapter".to_owned(),
                    version: "*".to_owned(),
                },
                "adapter.revoked",
                3,
            ),
            (
                PackageFailure::Quarantined {
                    id: "some-adapter".to_owned(),
                    version: "0.1.0".to_owned(),
                },
                "adapter.quarantined",
                3,
            ),
            (
                PackageFailure::TrustInsufficient {
                    id: "some-adapter".to_owned(),
                    level: "community".to_owned(),
                },
                "adapter.trust-insufficient",
                3,
            ),
            (
                PackageFailure::SourceUnavailable {
                    source: "release:channel/id".to_owned(),
                },
                "adapter.source-unavailable",
                4,
            ),
            (
                PackageFailure::InstallPlanRequired,
                "adapter.install-plan-required",
                1,
            ),
            (PackageFailure::SourceChanged, "adapter.source-changed", 1),
            (
                PackageFailure::InstallConflict {
                    path: "packages/x/1.0.0-abcd1234".to_owned(),
                },
                "adapter.install-conflict",
                3,
            ),
            (
                PackageFailure::RecoveryRequired {
                    stage: "promote".to_owned(),
                },
                "adapter.recovery-required",
                1,
            ),
            (
                PackageFailure::HooksDeclared { count: 2 },
                "adapter.hooks-declared",
                4,
            ),
            (
                PackageFailure::PermissionEscalated {
                    adapter: "some-adapter".to_owned(),
                    member: "network".to_owned(),
                },
                "adapter.permission-escalated",
                3,
            ),
        ];
        for (failure, rule, exit) in cases {
            assert_eq!(reason_of(&failure), rule, "{failure:?}");
            let envelope = domain_result(&failure);
            assert_eq!(envelope.exit_code(), exit, "{failure:?}");
        }
    }
}
