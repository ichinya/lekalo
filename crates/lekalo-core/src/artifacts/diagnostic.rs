//! The artifacts → diagnostic wire adapter (issue #21).
//!
//! Maps every closed [`ArtifactFailure`] onto its registered rule with
//! bounded tokens. The registry is closed at v1.2.0 and this issue adds no
//! rules: drift verdicts ride the closest registered lock/structure rules,
//! one diagnostic per finding, and every echoed token passes the bounded
//! token invariant so hostile input can neither scale the envelope nor
//! smuggle control characters into a wire item.

use super::{ArtifactFailure, DriftFinding};
use crate::diagnostics::normalize::build;
use crate::diagnostics::types::token_value;
use crate::diagnostics::{DataObject, Diagnostic};
use crate::result::{DomainResult, Status};

/// Build one wire diagnostic for a registered rule.
fn one(id: &str, symbol: Option<String>, data: DataObject) -> Diagnostic {
    build(id, symbol, None, data).expect("artifacts rules are registered and active")
}

fn token(text: impl Into<String>) -> DataObject {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(&text.into()));
    data
}

fn finding_data(finding: &DriftFinding) -> DataObject {
    let mut data = DataObject::new();
    if let Some(owner) = &finding.owner {
        data.insert("owner".to_owned(), token_value(owner));
    }
    if let Some(kind) = finding.kind {
        data.insert("kind".to_owned(), token_value(kind));
    }
    if let Some(lifecycle) = finding.lifecycle {
        data.insert("lifecycle".to_owned(), token_value(lifecycle));
    }
    data
}

impl From<&ArtifactFailure> for DomainResult {
    fn from(failure: &ArtifactFailure) -> Self {
        match failure {
            ArtifactFailure::Loader(result) => result.clone(),
            ArtifactFailure::Lock(lock_failure) => DomainResult::from(lock_failure),
            ArtifactFailure::ManifestNoncanonical => {
                DomainResult::invalid(DomainResult::from_wire_set(
                    Status::Invalid,
                    vec![one("lock.noncanonical", None, DataObject::new())],
                ))
            }
            ArtifactFailure::ManifestInvalid => DomainResult::invalid(DomainResult::from_wire_set(
                Status::Invalid,
                vec![one("lock.schema-invalid", None, DataObject::new())],
            )),
            ArtifactFailure::UnsupportedSchemaVersion { found } => {
                DomainResult::unsupported_version(DomainResult::from_wire_set(
                    Status::UnsupportedVersion,
                    vec![one(
                        "lock.unsupported-schema-version",
                        None,
                        token(found.clone()),
                    )],
                ))
            }
            ArtifactFailure::ManifestDigestMismatch => {
                DomainResult::denied(DomainResult::from_wire_set(
                    Status::Denied,
                    vec![one("lock.digest-mismatch", None, {
                        let mut data = DataObject::new();
                        data.insert("domain".to_owned(), token_value("artifact-manifest"));
                        data.insert("identity".to_owned(), token_value("manifest-digest"));
                        data
                    })],
                ))
            }
            ArtifactFailure::ReferenceInvalid => {
                DomainResult::invalid(DomainResult::from_wire_set(
                    Status::Invalid,
                    vec![one("lock.reference-invalid", None, DataObject::new())],
                ))
            }
            ArtifactFailure::SourceMapInvalid => {
                DomainResult::invalid(DomainResult::from_wire_set(
                    Status::Invalid,
                    vec![one("lock.reference-invalid", None, token("source-map"))],
                ))
            }
            ArtifactFailure::StaleManifest => DomainResult::invalid(DomainResult::from_wire_set(
                Status::Invalid,
                vec![one("lock.stale", None, token("artifact-manifest"))],
            )),
            ArtifactFailure::Structure { code, denied } => {
                // The structure passthrough keeps its own rule identity;
                // the denied flag decides the exit class, not the envelope.
                let status = if *denied {
                    Status::Denied
                } else {
                    Status::Invalid
                };
                let result =
                    DomainResult::from_wire_set(status, vec![one(code, None, DataObject::new())]);
                match status {
                    Status::Denied => DomainResult::denied(result),
                    _ => DomainResult::invalid(result),
                }
            }
            ArtifactFailure::Io(detail) => DomainResult::invalid(DomainResult::from_wire_set(
                Status::Invalid,
                vec![one("loader.io", None, token((*detail).to_owned()))],
            )),
            ArtifactFailure::Drift(findings) => {
                let diagnostics: Vec<Diagnostic> = findings
                    .iter()
                    .map(|finding| {
                        let (rule, _) = drift_rule_by_name(finding.verdict);
                        one(rule, Some(finding.path.clone()), finding_data(finding))
                    })
                    .collect();
                DomainResult::invalid(DomainResult::from_wire_set(Status::Invalid, diagnostics))
            }
            ArtifactFailure::PreviewRequired => DomainResult::invalid(DomainResult::from_wire_set(
                Status::Invalid,
                vec![one("lock.preview-required", None, DataObject::new())],
            )),
            ArtifactFailure::PlanChanged => DomainResult::invalid(DomainResult::from_wire_set(
                Status::Invalid,
                vec![one("lock.source-changed", None, token("plan"))],
            )),
        }
    }
}

impl From<ArtifactFailure> for DomainResult {
    fn from(failure: ArtifactFailure) -> Self {
        Self::from(&failure)
    }
}

fn drift_rule_by_name(verdict: &str) -> (&'static str, Status) {
    match verdict {
        "stale" => ("lock.stale", Status::Invalid),
        "manual-drift" => ("lock.source-changed", Status::Invalid),
        "missing" => ("structure.document-missing", Status::Invalid),
        _ => ("structure.runtime-unexpected-entry", Status::Invalid),
    }
}
