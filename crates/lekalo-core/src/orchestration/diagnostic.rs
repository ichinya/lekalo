//! The orchestration → diagnostic wire adapter (issue #91).
//!
//! The registry is closed at the accepted integrated version and this
//! surface adds no rules: every failure maps onto one registered rule of
//! an accepted family with bounded tokens. Status — and therefore the
//! exit class — is owned by the mapping, never by severity.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::token_value;
use crate::diagnostics::{DataObject, Diagnostic};
use crate::result::{DomainResult, Status};
use crate::target_protocol::TargetFailure;

/// The closed failure taxonomy of the orchestration surface.
///
/// Every variant maps onto exactly one registered diagnostic rule (or an
/// accepted passthrough result) in [`From<&Failure> for DomainResult`].
#[derive(Clone, Debug)]
pub enum Failure {
    /// A terminal loader/IR/versioning result: pass through untouched.
    Loader(DomainResult),
    /// A physical/policy structure refusal observed on this surface.
    Structure { code: &'static str, denied: bool },
    /// A lock-surface refusal (missing, stale, integrity, unavailable).
    Lock(crate::lockfile::LockFailure),
    /// The ownership-manifest surface refused.
    Artifact(crate::artifacts::ArtifactFailure),
    /// The adapter protocol client refused.
    Target(TargetFailure),
    /// The requested target names no locked adapter.
    AdapterNotLocked { adapter: String },
    /// The mode requires an adapter program and none was supplied.
    AdapterSupplyRequired,
    /// The local adapter bytes disagree with the locked pins.
    AdapterDigestMismatch { adapter: String },
    /// The discovered adapter does not declare the requested target.
    TargetNotDeclared { target: String, adapter: String },
    /// The generation scope cannot be bound: no project definition.
    ProjectRefUnresolved,
    /// No target remains after the explicit selection.
    NoTargetSelected,
}

/// Build one wire diagnostic for a registered rule.
fn one(id: &str, symbol: Option<String>, data: DataObject) -> Diagnostic {
    build(id, symbol, None, data).expect("orchestration rules are registered and active")
}

fn token(text: impl Into<String>) -> DataObject {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(&text.into()));
    data
}

fn named(field: &str, value: &str) -> DataObject {
    let mut data = DataObject::new();
    data.insert(field.to_owned(), token_value(value));
    data
}

/// Assemble one classified result from a single mapped diagnostic.
fn classified(status: Status, diagnostic: Diagnostic) -> DomainResult {
    let set = DomainResult::from_wire_set(status, vec![diagnostic]);
    match status {
        Status::Invalid => DomainResult::Invalid { diagnostics: set },
        Status::Denied => DomainResult::Denied { diagnostics: set },
        Status::Unavailable => DomainResult::Unavailable { diagnostics: set },
        Status::Unsupported => DomainResult::UnsupportedOperation { diagnostics: set },
        Status::UnsupportedVersion => DomainResult::UnsupportedVersion { diagnostics: set },
        Status::Valid => DomainResult::Unavailable { diagnostics: set },
    }
}

impl From<&Failure> for DomainResult {
    fn from(failure: &Failure) -> Self {
        match failure {
            Failure::Loader(result) => result.clone(),
            Failure::Structure { code, denied } => {
                let status = if *denied {
                    Status::Denied
                } else {
                    Status::Invalid
                };
                classified(status, one(code, None, DataObject::new()))
            }
            Failure::Lock(lock_failure) => DomainResult::from(lock_failure),
            Failure::Artifact(artifact_failure) => DomainResult::from(artifact_failure),
            Failure::Target(target_failure) => DomainResult::from(target_failure),
            Failure::AdapterNotLocked { adapter } => classified(
                Status::Unavailable,
                one(
                    "lock.component-unavailable",
                    None,
                    named("component", adapter),
                ),
            ),
            Failure::AdapterSupplyRequired => classified(
                Status::Unavailable,
                one(
                    "lock.component-unavailable",
                    None,
                    token("adapter-inventory"),
                ),
            ),
            Failure::AdapterDigestMismatch { adapter } => classified(
                Status::Denied,
                one("lock.digest-mismatch", None, named("component", adapter)),
            ),
            Failure::TargetNotDeclared { target, adapter } => classified(
                Status::Unsupported,
                one(
                    "target.capability-unsupported",
                    Some(adapter.clone()),
                    named("target", target),
                ),
            ),
            Failure::ProjectRefUnresolved => classified(
                Status::Invalid,
                one("lock.reference-invalid", None, token("project-ref")),
            ),
            Failure::NoTargetSelected => classified(
                Status::Unsupported,
                one(
                    "target.capability-unsupported",
                    None,
                    token("no-declared-target"),
                ),
            ),
        }
    }
}

impl From<Failure> for DomainResult {
    fn from(failure: Failure) -> Self {
        Self::from(&failure)
    }
}
