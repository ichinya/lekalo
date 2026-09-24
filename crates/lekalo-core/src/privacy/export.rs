//! The export enforcement pipeline (issue #119, plan S5).
//!
//! [`run_export`] is the runtime seam between a real artifact, the
//! project's #87 classification attachments, and the frozen #120
//! evaluator: it reads the artifact envelope, resolves the class
//! (fail-closed: a missing or unknown class refuses before
//! evaluation), synthesizes the exact closed decision input, evaluates
//! against the custody-verified context, and — only for
//! `transform-required` — applies the closed redaction transforms and
//! the leak-scanner verification pass. A residual leak fails the
//! export, never silently ships.
//!
//! Metadata-only by construction: the decision input carries the
//! artifact's opaque `artifact-sha256:` identity and declared
//! envelope members; the payload text itself never enters the
//! decision input, the decision record, the findings, or any
//! diagnostic. The pipeline is deterministic: identical inputs and
//! project state produce byte-identical outputs and writes.

use std::path::Path;

use serde::Serialize;
use serde_json::Value as Json;

use super::context::TrustedContext;
use super::evaluate::{authorization_subject_digest, evaluate_decision};
use super::output::ExportDecisionOutput;
use super::redact::{redact, RedactionRequest, RedactionSubject};
use super::redact::{scan, LeakFinding};
use super::vocab::{Audience, DataSensitivity, RepositoryRole, TransformId, TrustBoundary};
use super::{input, refs};
use crate::digest::sha256_hex;

/// The closed destination vocabulary of the export command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationSpec {
    /// Local use in the same workspace (`workspace`).
    Workspace,
    /// Store into the consumer repository (`repository-store`).
    RepositoryStore,
    /// Transfer inside the same tenant (`transfer-tenant`).
    TransferTenant,
    /// Transfer to one named external repository
    /// (`transfer-external`).
    TransferExternal,
    /// Transfer across tenants (`transfer-cross-tenant`).
    TransferCrossTenant,
    /// Publish to the public channel (`publish`).
    Publish,
}

impl DestinationSpec {
    /// Every destination in closed order.
    pub const ALL: [DestinationSpec; 6] = [
        DestinationSpec::Workspace,
        DestinationSpec::RepositoryStore,
        DestinationSpec::TransferTenant,
        DestinationSpec::TransferExternal,
        DestinationSpec::TransferCrossTenant,
        DestinationSpec::Publish,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "workspace" => Self::Workspace,
            "repository-store" => Self::RepositoryStore,
            "transfer-tenant" => Self::TransferTenant,
            "transfer-external" => Self::TransferExternal,
            "transfer-cross-tenant" => Self::TransferCrossTenant,
            "publish" => Self::Publish,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::RepositoryStore => "repository-store",
            Self::TransferTenant => "transfer-tenant",
            Self::TransferExternal => "transfer-external",
            Self::TransferCrossTenant => "transfer-cross-tenant",
            Self::Publish => "publish",
        }
    }
}

impl serde::Serialize for DestinationSpec {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Why an export refused before any write. Shape refusals are
/// malformed-input protocol (exit 1); policy refusals deny (exit 3).
#[derive(Clone, Debug, PartialEq)]
pub enum ExportFailure {
    /// Malformed input: unreadable artifact, invalid envelope, or an
    /// invalid classification attachment. Carries the exact refusal
    /// code for the closed CLI error object.
    Malformed(&'static str),
    /// The evaluator denied: the closed output carries the reason.
    Denied(ExportDecisionOutput),
    /// The decision allowed or required a transform, but the leak
    /// scanner found residual leaks over the final payload: the
    /// export refuses instead of silently shipping.
    ResidualLeaks {
        /// The closed decision output.
        output: ExportDecisionOutput,
        /// The residual leaks, sorted, aliases only.
        leaks: Vec<LeakFinding>,
    },
}

/// The completed export plan: everything the CLI renders or writes.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutcome {
    decision: ExportDecisionOutput,
    artifact_kind: String,
    artifact_ref: String,
    destination: DestinationSpec,
    payload: String,
    payload_digest: String,
    applied_transforms: Vec<TransformId>,
    findings: Vec<LeakFinding>,
    residuals: Vec<LeakFinding>,
    export_path: String,
    decision_path: String,
    written: bool,
}

impl ExportOutcome {
    /// The closed decision output.
    pub const fn decision(&self) -> &ExportDecisionOutput {
        &self.decision
    }

    /// The artifact kind.
    pub fn artifact_kind(&self) -> &str {
        &self.artifact_kind
    }

    /// The opaque `artifact-sha256:` identity of the source artifact.
    pub fn artifact_ref(&self) -> &str {
        &self.artifact_ref
    }

    /// The destination spec.
    pub const fn destination(&self) -> DestinationSpec {
        self.destination
    }

    /// The exact candidate payload (redacted where required).
    pub fn payload(&self) -> &str {
        &self.payload
    }

    /// The payload digest of the candidate bytes.
    pub fn payload_digest(&self) -> &str {
        &self.payload_digest
    }

    /// The transforms the engine applied.
    pub fn applied_transforms(&self) -> &[TransformId] {
        &self.applied_transforms
    }

    /// The pre-transform leak findings (the redaction diff).
    pub fn findings(&self) -> &[LeakFinding] {
        &self.findings
    }

    /// The residual leaks over the candidate payload (empty when the
    /// candidate may ship).
    pub fn residuals(&self) -> &[LeakFinding] {
        &self.residuals
    }

    /// The project-relative export path (`.lekalo/privacy/exports/…`).
    pub fn export_path(&self) -> &str {
        &self.export_path
    }

    /// The project-relative decision-record path.
    pub fn decision_path(&self) -> &str {
        &self.decision_path
    }

    /// Whether the writes happened (never under `--dry-run`).
    pub const fn written(&self) -> bool {
        self.written
    }

    /// The deterministic decision-record document for this export.
    pub fn decision_record(&self) -> Json {
        serde_json::json!({
            "decision": serde_json::to_value(self.decision()).unwrap_or(Json::Null),
            "artifact": {
                "artifactKind": self.artifact_kind,
                "artifactRef": self.artifact_ref,
                "payloadDigest": self.payload_digest,
            },
            "destination": self.destination.as_str(),
            "redaction": {
                "appliedTransforms": self.applied_transforms
                    .iter()
                    .map(|transform| transform.as_str())
                    .collect::<Vec<&str>>(),
                "findings": serde_json::to_value(self.findings()).unwrap_or(Json::Null),
                "residuals": serde_json::to_value(self.residuals()).unwrap_or(Json::Null),
            },
            "exportPath": self.export_path,
        })
    }
}

/// One resolved destination: the operation, the exact destination
/// members, and the coherent source/provenance custody (transfers and
/// repository stores are repository-backed; local use and publication
/// run from the workspace).
struct ResolvedDestination {
    operation: input::Operation,
    source: input::Endpoint,
    provenance_ref: Option<String>,
    destination: input::Destination,
    audience: Audience,
}

impl DestinationSpec {
    fn resolve(self, repository_token: &str, consumer_token: &str) -> ResolvedDestination {
        let operation = |id| input::Operation::new(id, refs::DECISION_FAMILY_VERSION.to_owned());
        let destination = |role, reference, boundary, relation, tenant| {
            input::Destination::new(role, reference, boundary, relation, tenant)
        };
        let workspace_source = || input::Endpoint::new(RepositoryRole::LocalWorkspace, None);
        let repository_source = || {
            input::Endpoint::new(
                RepositoryRole::ConsumerRepository,
                Some(consumer_token.to_owned()),
            )
        };
        match self {
            Self::Workspace => ResolvedDestination {
                operation: operation(super::vocab::OperationId::LocalUse),
                source: workspace_source(),
                provenance_ref: None,
                destination: destination(
                    RepositoryRole::LocalWorkspace,
                    None,
                    TrustBoundary::SameLocalWorkspace,
                    super::vocab::RepositoryRelation::NotApplicable,
                    super::vocab::TenantRelation::SameTenant,
                ),
                audience: Audience::OperatorOnly,
            },
            Self::RepositoryStore => ResolvedDestination {
                operation: operation(super::vocab::OperationId::RepositoryStore),
                source: repository_source(),
                provenance_ref: Some(consumer_token.to_owned()),
                destination: destination(
                    RepositoryRole::ConsumerRepository,
                    Some(repository_token.to_owned()),
                    TrustBoundary::SameRepository,
                    super::vocab::RepositoryRelation::SameOrigin,
                    super::vocab::TenantRelation::SameTenant,
                ),
                audience: Audience::RepositoryCollaborators,
            },
            Self::TransferTenant => ResolvedDestination {
                operation: operation(super::vocab::OperationId::Transfer),
                source: repository_source(),
                provenance_ref: Some(consumer_token.to_owned()),
                destination: destination(
                    RepositoryRole::ExternalRepository,
                    Some(repository_token.to_owned()),
                    TrustBoundary::SameTenant,
                    super::vocab::RepositoryRelation::DifferentRepository,
                    super::vocab::TenantRelation::SameTenant,
                ),
                audience: Audience::TenantMembers,
            },
            Self::TransferExternal => ResolvedDestination {
                operation: operation(super::vocab::OperationId::Transfer),
                source: repository_source(),
                provenance_ref: Some(consumer_token.to_owned()),
                destination: destination(
                    RepositoryRole::ExternalRepository,
                    Some(repository_token.to_owned()),
                    TrustBoundary::CrossRepository,
                    super::vocab::RepositoryRelation::DifferentRepository,
                    super::vocab::TenantRelation::SameTenant,
                ),
                audience: Audience::NamedExternal,
            },
            Self::TransferCrossTenant => ResolvedDestination {
                operation: operation(super::vocab::OperationId::Transfer),
                source: repository_source(),
                provenance_ref: Some(consumer_token.to_owned()),
                destination: destination(
                    RepositoryRole::ExternalRepository,
                    Some(repository_token.to_owned()),
                    TrustBoundary::CrossTenant,
                    super::vocab::RepositoryRelation::DifferentRepository,
                    super::vocab::TenantRelation::CrossTenant,
                ),
                audience: Audience::NamedExternal,
            },
            Self::Publish => ResolvedDestination {
                operation: operation(super::vocab::OperationId::Publish),
                source: workspace_source(),
                provenance_ref: None,
                destination: destination(
                    RepositoryRole::PublicChannel,
                    None,
                    TrustBoundary::Public,
                    super::vocab::RepositoryRelation::NotApplicable,
                    super::vocab::TenantRelation::NotApplicable,
                ),
                audience: Audience::Public,
            },
        }
    }
}

/// Map the #87 data-kind lattice onto the #120 sensitivity labels.
/// The mapping only ever narrows to the restrictive side.
pub fn sensitivity_of_kind(kind: crate::classification::types::DataKind) -> DataSensitivity {
    use crate::classification::types::DataKind;
    match kind {
        DataKind::Public => DataSensitivity::Public,
        DataKind::Internal | DataKind::Derived => DataSensitivity::Internal,
        DataKind::RetentionLimited => DataSensitivity::RetentionLimited,
        DataKind::TenantScoped => DataSensitivity::TenantScoped,
        DataKind::Confidential => DataSensitivity::Confidential,
        DataKind::Financial => DataSensitivity::Financial,
        DataKind::Personal => DataSensitivity::PersonalPii,
        DataKind::Health => DataSensitivity::HealthSpecialCategory,
        DataKind::Credential => DataSensitivity::CredentialSecret,
    }
}

/// Run the export pipeline over one artifact document. `dry_run`
/// computes the full plan and writes nothing. Writes go only under
/// `.lekalo/privacy/`: the candidate payload under `exports/` and the
/// decision record under `decisions/export/`.
#[allow(clippy::result_large_err)]
pub fn run_export(
    project: &Path,
    artifact_path: &Path,
    destination: DestinationSpec,
    consent: Option<&Json>,
    dry_run: bool,
) -> Result<ExportOutcome, ExportFailure> {
    let context = TrustedContext::embedded()
        .map_err(|_| ExportFailure::Malformed("privacy.custody-failure"))?;
    let artifact_bytes = std::fs::read(artifact_path)
        .map_err(|_| ExportFailure::Malformed("privacy.artifact-unreadable"))?;
    let envelope: Json = serde_json::from_slice(&artifact_bytes)
        .map_err(|_| ExportFailure::Malformed("privacy.envelope-invalid"))?;
    let envelope_object = envelope
        .as_object()
        .ok_or(ExportFailure::Malformed("privacy.envelope-invalid"))?;
    let artifact_kind = envelope_object
        .get("artifactKind")
        .and_then(Json::as_str)
        .filter(|kind| !kind.is_empty())
        .ok_or(ExportFailure::Malformed("privacy.artifact-kind-missing"))?;
    let payload_value = envelope_object
        .get("payload")
        .ok_or(ExportFailure::Malformed("privacy.payload-missing"))?;
    let payload_text = payload_text(payload_value);

    // The class: envelope declaration first, then the classification
    // attachment defaults; neither means refuse (fail closed), and an
    // unknown label refuses.
    let mut labels: Vec<DataSensitivity> = Vec::new();
    if let Some(declared) = envelope_object.get("class") {
        let declared = declared
            .as_array()
            .filter(|entries| !entries.is_empty())
            .ok_or(ExportFailure::Malformed("privacy.class-unknown"))?;
        for entry in declared {
            let label = entry
                .as_str()
                .and_then(DataSensitivity::parse)
                .ok_or(ExportFailure::Malformed("privacy.class-unknown"))?;
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
    } else if let Some(default_kind) = project_payload_default(project)? {
        let label = sensitivity_of_kind(default_kind);
        labels.push(label);
    } else {
        return Err(ExportFailure::Denied(ExportDecisionOutput::deny(
            "privacy.class-missing",
        )));
    }

    // The artifact identity: opaque digests only, never paths.
    let artifact_digest = sha256_hex(&artifact_bytes);
    let artifact_ref = format!("artifact-sha256:{artifact_digest}");
    let classification_digest = classification_custody_digest(project, &artifact_digest)?;

    // The destination repository token: a declared-coherence opaque
    // digest over the resolved project root; the physical binding
    // remains the #89 adapter obligation.
    let repository_token = format!(
        "repo-sha256:{}",
        sha256_hex(
            std::fs::canonicalize(project)
                .unwrap_or_else(|_| project.to_path_buf())
                .to_string_lossy()
                .as_bytes()
        )
    );
    let consumer_token = format!(
        "repo-sha256:{}",
        sha256_hex(
            format!(
                "{}:consumer",
                std::fs::canonicalize(project)
                    .unwrap_or_else(|_| project.to_path_buf())
                    .to_string_lossy()
            )
            .as_bytes()
        )
    );
    let resolved = destination.resolve(&repository_token, &consumer_token);

    // Provenance: repository-backed destinations are coherent only
    // from the consumer repository; workspace destinations carry the
    // envelope's declared origin booleans.
    let synthetic = envelope_object
        .get("synthetic")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let derived = envelope_object
        .get("derived")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let repository_backed = !matches!(
        destination,
        DestinationSpec::Workspace | DestinationSpec::Publish
    );
    let origin = if repository_backed {
        super::vocab::ProvenanceOrigin::ConsumerRepository
    } else if synthetic {
        super::vocab::ProvenanceOrigin::Synthetic
    } else if derived {
        super::vocab::ProvenanceOrigin::Derived
    } else {
        super::vocab::ProvenanceOrigin::ToolRuntime
    };
    let path_value = artifact_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let provenance = input::Provenance::new(
        origin,
        resolved.source.repository_role(),
        resolved.provenance_ref.clone(),
        synthetic,
        derived,
        super::types::ClassificationDecisionRef::new(
            refs::CLASSIFICATION_CONTRACT_ID.to_owned(),
            refs::DECISION_FAMILY_VERSION.to_owned(),
            format!("sha256:{}", refs::CLASSIFICATION_CONTRACT_RAW_SHA256),
            format!("classification-sha256:{classification_digest}"),
            format!("sha256:{artifact_digest}"),
        ),
        None,
        None,
        None,
        None,
        None,
        None,
    );

    // The optional caller-declared consent evidence is bound to the
    // exact subject digest after the input exists (below); every
    // other member must already be declared correctly or the
    // evaluator denies.

    let decision_input = input::ExportDecisionInput::with_frozen_refs(
        artifact_kind.to_owned(),
        artifact_ref.clone(),
        resolved.operation,
        resolved.source.clone(),
        resolved.destination,
        resolved.audience,
        labels.clone(),
        default_disposition(context, artifact_kind),
        provenance,
        super::types::ResourcePath::known(path_value.to_string()),
        None,
        input::ConflictResolution::new(super::vocab::ConflictState::None, None),
        Vec::new(),
        None,
    );
    let mut wire = serde_json::to_value(&decision_input)
        .map_err(|_| ExportFailure::Malformed("privacy.envelope-invalid"))?;

    // Bind the consent evidence to the exact subject digest after the
    // input exists, then splice it into the wire input.
    if let Some(record) = consent {
        let subject_digest =
            authorization_subject_digest(&wire, context.authorization_subject_profile());
        let mut bound = record.clone();
        if bound.get("binding").is_none() {
            bound["binding"] = serde_json::json!({});
        }
        bound["binding"]["subjectProfileRef"] = profile_ref_value();
        bound["binding"]["subjectDigest"] = Json::String(subject_digest);
        wire["provenance"]["exportTransferConsentRef"] = bound;
    }

    let evaluated = evaluate_decision(&wire, context);
    if evaluated.output.decision() == super::output::ExportDecision::Deny {
        return match evaluated.malformed {
            true => Err(ExportFailure::Malformed("privacy.input-malformed")),
            false => Err(ExportFailure::Denied(evaluated.output)),
        };
    }

    // The redaction pipeline: the required transforms, the declared
    // subject, and the verification pass. A payload that still leaks
    // refuses the export.
    let redacted = redact(&RedactionRequest {
        payload: &payload_text,
        transforms: &required_transforms(&evaluated.output),
        labels: &labels,
        subject: RedactionSubject::default(),
    });
    let residuals = redacted.residuals().to_vec();
    if !residuals.is_empty() {
        return Err(ExportFailure::ResidualLeaks {
            output: evaluated.output,
            leaks: residuals,
        });
    }
    // An allowed transfer ships only a payload the scanner passes.
    if evaluated.output.decision() == super::output::ExportDecision::Allow {
        let leaks = scan(&payload_text);
        if !leaks.is_empty() {
            return Err(ExportFailure::ResidualLeaks {
                output: evaluated.output,
                leaks,
            });
        }
    }

    let payload_digest = format!("sha256:{}", sha256_hex(redacted.payload().as_bytes()));
    let name = path_value;
    let stem = std::path::Path::new(&name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.clone());
    let export_path = format!(".lekalo/privacy/exports/{name}");
    let decision_path = format!(".lekalo/privacy/decisions/export/{stem}.json");
    let mut outcome = ExportOutcome {
        decision: evaluated.output,
        artifact_kind: artifact_kind.to_owned(),
        artifact_ref,
        destination,
        payload: redacted.payload().to_owned(),
        payload_digest,
        applied_transforms: redacted.applied().to_vec(),
        findings: redacted.findings().to_vec(),
        residuals,
        export_path,
        decision_path,
        written: false,
    };
    if !dry_run {
        write_under(project, &outcome.export_path, outcome.payload.as_bytes())?;
        let record = outcome.decision_record().to_string();
        write_under(project, &outcome.decision_path, record.as_bytes())?;
        outcome.written = true;
    }
    Ok(outcome)
}

/// The redaction diff contract of `lekalo privacy redact`: scan and
/// apply the closed transforms over one payload document, returning
/// the redacted text, the diff (findings), and the residual scan.
/// Read-only by definition.
pub fn redact_preview(
    payload: &str,
    transforms: &[TransformId],
    labels: &[DataSensitivity],
) -> (String, Vec<LeakFinding>, Vec<LeakFinding>, Vec<TransformId>) {
    let redacted = redact(&RedactionRequest {
        payload,
        transforms,
        labels,
        subject: RedactionSubject::default(),
    });
    (
        redacted.payload().to_owned(),
        redacted.findings().to_vec(),
        redacted.residuals().to_vec(),
        redacted.applied().to_vec(),
    )
}

fn required_transforms(output: &ExportDecisionOutput) -> Vec<TransformId> {
    output
        .required_transforms()
        .iter()
        .filter_map(|transform| TransformId::parse(transform))
        .collect()
}

fn default_disposition(
    context: &TrustedContext,
    artifact_kind: &str,
) -> super::vocab::ExportDisposition {
    context
        .default_disposition(artifact_kind)
        .and_then(super::vocab::ExportDisposition::parse)
        .unwrap_or(super::vocab::ExportDisposition::ForbiddenToExport)
}

fn profile_ref_value() -> Json {
    serde_json::json!({
        "profileId": refs::AUTHORIZATION_SUBJECT_PROFILE_ID,
        "version": refs::DECISION_FAMILY_VERSION,
        "digest": format!("sha256:{}", refs::AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256),
    })
}

/// The candidate payload text: strings stay strings; every other
/// payload value serializes to its compact canonical JSON text.
fn payload_text(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// The declared payload default of the project's classification
/// attachment, when one is present. A present but invalid attachment
/// refuses; an absent one is not an error.
#[allow(clippy::result_large_err)]
fn project_payload_default(
    project: &Path,
) -> Result<Option<crate::classification::types::DataKind>, ExportFailure> {
    let attachment_path = project.join("classification.json");
    if !attachment_path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&attachment_path)
        .map_err(|_| ExportFailure::Malformed("privacy.classification-invalid"))?;
    let attachment = crate::classification::wire::Attachment::parse(&bytes)
        .map_err(|_| ExportFailure::Malformed("privacy.classification-invalid"))?;
    if project.join("classification-policy.json").exists() {
        let policy_bytes = std::fs::read(project.join("classification-policy.json"))
            .map_err(|_| ExportFailure::Malformed("privacy.classification-invalid"))?;
        crate::classification::policy::PolicyAttachment::parse(&policy_bytes)
            .map_err(|_| ExportFailure::Malformed("privacy.classification-invalid"))?;
    }
    Ok(Some(attachment.defaults().unclassified_payloads()))
}

/// The opaque classification custody digest: over the classification
/// attachment bytes when present, else over the artifact digest.
#[allow(clippy::result_large_err)]
fn classification_custody_digest(
    project: &Path,
    artifact_digest: &str,
) -> Result<String, ExportFailure> {
    let attachment_path = project.join("classification.json");
    let basis = if attachment_path.exists() {
        std::fs::read(&attachment_path)
            .map_err(|_| ExportFailure::Malformed("privacy.classification-invalid"))?
    } else {
        artifact_digest.as_bytes().to_vec()
    };
    Ok(sha256_hex(&basis))
}

/// Write one file under the project, creating the reserved privacy
/// homes. The path must be a relative `.lekalo/privacy/...` spelling.
#[allow(clippy::result_large_err)]
fn write_under(project: &Path, relative: &str, bytes: &[u8]) -> Result<(), ExportFailure> {
    let full = project.join(relative);
    let parent = full
        .parent()
        .ok_or(ExportFailure::Malformed("privacy.write-refused"))?;
    std::fs::create_dir_all(parent)
        .map_err(|_| ExportFailure::Malformed("privacy.write-refused"))?;
    std::fs::write(full, bytes).map_err(|_| ExportFailure::Malformed("privacy.write-refused"))
}

#[cfg(test)]
mod tests {
    use super::super::redact::LeakKind;
    use super::*;
    use std::path::PathBuf;

    /// Write one envelope under a temporary project and return its path.
    fn consent_record() -> Json {
        serde_json::json!({
            "contractId": "dev.lekalo.privacy-authorizing-evidence",
            "version": "0.2.16",
            "digest": format!("sha256:{}", crate::privacy::refs::AUTHORIZING_EVIDENCE_RAW_SHA256),
            "evidenceKind": "export-transfer-consent",
            "purpose": "authorize-export-transfer-or-storage",
            "outcome": "granted",
            "evidenceId": format!("evidence-sha256:{}", "9".repeat(64)),
            "verificationState": "verified",
            "freshnessState": "current",
        })
    }

    fn write_artifact(project: &Path, name: &str, envelope: &Json) -> PathBuf {
        let path = project.join(name);
        std::fs::write(&path, envelope.to_string()).expect("artifact writes");
        path
    }

    /// Publishing a raw prompt is forbidden: the disposition dominates,
    /// nothing is written, and the closed reason fires.
    #[test]
    fn deny_publishing_raw_prompt_fails_closed() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_artifact(
            project.path(),
            "prompt.json",
            &serde_json::json!({
                "artifactKind": "ai.prompt",
                "payload": "the raw user prompt text",
                "class": ["confidential"],
            }),
        );
        let outcome = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            None,
            true,
        );
        match outcome {
            Err(ExportFailure::Denied(output)) => {
                assert_eq!(output.decision().as_str(), "deny");
                assert_eq!(output.reason_codes(), ["disposition.forbidden-dominates"]);
            }
            other => panic!("expected a deny, got {other:?}"),
        }
        assert!(!project.path().join(".lekalo/privacy/exports").exists());
    }

    /// A missing class refuses before evaluation: fail closed.
    #[test]
    fn missing_class_refuses_before_evaluation() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_artifact(
            project.path(),
            "summary.json",
            &serde_json::json!({
                "artifactKind": "generated.summary",
                "payload": "a harmless summary",
            }),
        );
        let outcome = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Workspace,
            None,
            true,
        );
        match outcome {
            Err(ExportFailure::Denied(output)) => {
                assert_eq!(output.reason_codes(), ["privacy.class-missing"]);
            }
            other => panic!("expected the class refusal, got {other:?}"),
        }
    }

    /// An unknown class label refuses: fail closed.
    #[test]
    fn unknown_class_refuses() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_artifact(
            project.path(),
            "summary.json",
            &serde_json::json!({
                "artifactKind": "generated.summary",
                "payload": "a harmless summary",
                "class": ["top-secret-bogo"],
            }),
        );
        let outcome = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Workspace,
            None,
            true,
        );
        assert!(matches!(
            outcome,
            Err(ExportFailure::Malformed("privacy.class-unknown"))
        ));
    }

    /// A missing artifact kind or payload refuses: fail closed.
    #[test]
    fn incomplete_envelopes_refuse() {
        let project = tempfile::tempdir().expect("temp project");
        let kindless = write_artifact(
            project.path(),
            "kindless.json",
            &serde_json::json!({ "payload": "x", "class": ["public"] }),
        );
        let outcome = run_export(
            project.path(),
            &kindless,
            DestinationSpec::Workspace,
            None,
            true,
        );
        assert!(matches!(
            outcome,
            Err(ExportFailure::Malformed("privacy.artifact-kind-missing"))
        ));
        let payloadless = write_artifact(
            project.path(),
            "payloadless.json",
            &serde_json::json!({ "artifactKind": "fixture", "class": ["public"] }),
        );
        let outcome = run_export(
            project.path(),
            &payloadless,
            DestinationSpec::Workspace,
            None,
            true,
        );
        assert!(matches!(
            outcome,
            Err(ExportFailure::Malformed("privacy.payload-missing"))
        ));
    }

    /// Transform-required publication redacts the body to the stub and
    /// writes the candidate plus the decision record under the
    /// reserved homes; the secret never reaches any written byte.
    #[test]
    fn transform_required_redacts_writes_and_records() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_artifact(
            project.path(),
            "summary.json",
            &serde_json::json!({
                "artifactKind": "generated.summary",
                "payload": "summary containing AKIAABCDEFGHIJKLMNOP",
                "class": ["public"],
            }),
        );
        let outcome = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            Some(&consent_record()),
            true,
        )
        .expect("transform-required publish plans");
        assert_eq!(outcome.decision().decision().as_str(), "transform-required");
        assert_eq!(outcome.applied_transforms(), [TransformId::RedactContent]);
        assert!(outcome.payload().contains("redacted-content"));
        assert!(!outcome.payload().contains("AKIA"));
        assert!(!outcome.written());
        assert!(!project
            .path()
            .join(".lekalo/privacy/exports/summary.json")
            .exists());

        let applied = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            Some(&consent_record()),
            false,
        )
        .expect("transform-required publish applies");
        assert!(applied.written());
        let written =
            std::fs::read_to_string(project.path().join(".lekalo/privacy/exports/summary.json"))
                .expect("export written");
        assert!(!written.contains("AKIA"));
        assert!(written.contains("redacted-content"));
        let record = std::fs::read_to_string(
            project
                .path()
                .join(".lekalo/privacy/decisions/export/summary.json"),
        )
        .expect("decision record written");
        assert!(!record.contains("AKIA"));
        assert!(record.contains("transform-required"));
    }

    /// A synthetic public fixture publishes as-is and records the
    /// allow decision.
    #[test]
    fn allow_synthetic_fixture_publishes() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_artifact(
            project.path(),
            "fixture.json",
            &serde_json::json!({
                "artifactKind": "fixture",
                "payload": "synthetic fixture text",
                "class": ["public"],
                "synthetic": true,
            }),
        );
        let outcome = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            None,
            false,
        )
        .expect("synthetic fixture publishes");
        assert_eq!(outcome.decision().decision().as_str(), "allow");
        assert_eq!(outcome.payload(), "synthetic fixture text");
        assert!(outcome.written());
        let written =
            std::fs::read_to_string(project.path().join(".lekalo/privacy/exports/fixture.json"))
                .expect("export written");
        assert_eq!(written, "synthetic fixture text");
    }

    /// An allowed class with a leaking payload refuses instead of
    /// silently shipping: the scanner verification pass.
    #[test]
    fn allow_with_leak_refuses() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_artifact(
            project.path(),
            "fixture.json",
            &serde_json::json!({
                "artifactKind": "fixture",
                "payload": "clean text referencing https://private.example/acme and /home/dev/secrets",
                "class": ["public"],
                "synthetic": true,
            }),
        );
        let outcome = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            None,
            true,
        );
        match outcome {
            Err(ExportFailure::ResidualLeaks { leaks, .. }) => {
                let kinds: Vec<LeakKind> = leaks.iter().map(|leak| leak.kind()).collect();
                assert!(kinds.contains(&LeakKind::Url));
                assert!(kinds.contains(&LeakKind::PathFragment));
            }
            other => panic!("expected the leak refusal, got {other:?}"),
        }
    }

    /// A shareable summary transferred without consent evidence
    /// denies with the exact consent reason.
    #[test]
    fn transfer_without_consent_denies() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_artifact(
            project.path(),
            "summary.json",
            &serde_json::json!({
                "artifactKind": "generated.summary",
                "payload": "summary text",
                "class": ["internal"],
            }),
        );
        let outcome = run_export(
            project.path(),
            &artifact,
            DestinationSpec::TransferTenant,
            None,
            true,
        );
        match outcome {
            Err(ExportFailure::Denied(output)) => {
                assert_eq!(output.reason_codes(), ["provenance.consent-required"]);
            }
            other => panic!("expected the consent deny, got {other:?}"),
        }
    }

    /// Deterministic outcomes: identical runs produce identical
    /// payloads and decision records.
    #[test]
    fn outcomes_are_deterministic() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_artifact(
            project.path(),
            "summary.json",
            &serde_json::json!({
                "artifactKind": "generated.summary",
                "payload": "summary containing AKIAABCDEFGHIJKLMNOP",
                "class": ["public"],
            }),
        );
        let one = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            Some(&consent_record()),
            true,
        )
        .expect("first run");
        let two = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            Some(&consent_record()),
            true,
        )
        .expect("second run");
        assert_eq!(one.payload(), two.payload());
        assert_eq!(one.payload_digest(), two.payload_digest());
        assert_eq!(
            serde_json::to_string(&one.decision_record()).unwrap(),
            serde_json::to_string(&two.decision_record()).unwrap()
        );
    }
}
