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
use super::redact::LeakFinding;
use super::redact::{redact, RedactionRequest, RedactionSubject};
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
    /// The digest of the #89 confinement evidence carried by the
    /// envelope, when present.
    confinement_digest: Option<String>,
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
            "confinementDigest": self.confinement_digest,
            "exportPath": self.export_path,
        })
    }

    /// The digest of the #89 confinement evidence carried by the
    /// envelope, when present.
    pub const fn confinement_digest(&self) -> Option<&String> {
        self.confinement_digest.as_ref()
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
/// The synthesized decision input plus the payload-side context the
/// export pipeline continues from.
struct Synthesis {
    wire: Json,
    payload_text: String,
    labels: Vec<DataSensitivity>,
    artifact_kind: String,
    artifact_ref: String,
    file_name: String,
    repository_name: Option<String>,
    protected_terms: Vec<String>,
    confinement_digest: Option<String>,
}

/// Synthesize the exact wire decision input for one artifact and
/// destination: everything from the envelope read up to evaluation.
#[allow(clippy::result_large_err)]
fn synthesize(
    context: &TrustedContext,
    project: &Path,
    artifact_path: &Path,
    destination: DestinationSpec,
) -> Result<Synthesis, ExportFailure> {
    let artifact_bytes = std::fs::read(artifact_path)
        .map_err(|_| ExportFailure::Malformed("privacy.artifact-unreadable"))?;
    let envelope: Json = serde_json::from_slice(&artifact_bytes)
        .map_err(|_| ExportFailure::Malformed("privacy.envelope-invalid"))?;
    let envelope_object = envelope
        .as_object()
        .ok_or(ExportFailure::Malformed("privacy.envelope-invalid"))?;
    // The #89 integration seam: an adapter-produced artifact may carry
    // its confinement evidence. Where present it must be structurally
    // coherent (the closed member set) or the export refuses; its
    // digest is recorded in the decision record. This is metadata
    // custody only - physical containment stays the adapter
    // obligation, and the adapter gains no new filesystem or network
    // scope from this read.
    let confinement_digest: Option<String> = match envelope_object.get("confinement") {
        None => None,
        Some(confinement) => {
            let coherent = confinement.is_object()
                && confinement.get("budget").is_some_and(Json::is_object)
                && confinement.get("described").is_some_and(Json::is_object)
                && confinement.get("effective").is_some_and(Json::is_object)
                && confinement
                    .get("platform")
                    .and_then(Json::as_str)
                    .is_some_and(|platform| !platform.is_empty());
            if !coherent {
                return Err(ExportFailure::Malformed("privacy.confinement-invalid"));
            }
            let bytes = serde_json::to_vec(confinement)
                .map_err(|_| ExportFailure::Malformed("privacy.confinement-invalid"))?;
            Some(format!("sha256:{}", sha256_hex(&bytes)))
        }
    };
    let artifact_kind = envelope_object
        .get("artifactKind")
        .and_then(Json::as_str)
        .filter(|kind| !kind.is_empty())
        .ok_or(ExportFailure::Malformed("privacy.artifact-kind-missing"))?;
    let payload_value = envelope_object
        .get("payload")
        .ok_or(ExportFailure::Malformed("privacy.payload-missing"))?;
    let payload_text = payload_text(payload_value);

    // The class: the envelope claim plus the declared floor (fix
    // round 2, C-F2). When the project classification attachment
    // parses, its unclassified-payload default maps to a #120 label
    // that is unioned into the effective set - a claim below the
    // declared floor widens (never silently lowers, the #87
    // propagation doctrine). An unknown label refuses; no claim and
    // no attachment refuses (`privacy.class-missing`).
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
    }
    if let Some(default_kind) = project_payload_default(project)? {
        let floor = sensitivity_of_kind(default_kind);
        if !labels.contains(&floor) {
            labels.push(floor);
        }
    }
    if labels.is_empty() {
        return Err(ExportFailure::Denied(ExportDecisionOutput::deny(
            "privacy.class-missing",
        )));
    }

    // The artifact identity: opaque digests only, never paths.
    let artifact_digest = sha256_hex(&artifact_bytes);
    let artifact_ref = format!("artifact-sha256:{artifact_digest}");
    let classification_digest = classification_custody_digest(project, &artifact_digest)?;

    // The destination repository tokens (fix round 2, C-F3): a
    // declared-coherence opaque digest over
    // "lekalo.repository-identity\n" plus the custody basis - the
    // classification attachment bytes when present, else the artifact
    // envelope bytes - scoped per endpoint role. Never a host path:
    // the token is deterministic across clones and proves custody
    // coherence, not host identity; the physical binding and
    // freshness checks remain the #89 adapter obligation.
    let classification_path = project.join("classification.json");
    let identity_basis: Vec<u8> = if classification_path.exists() {
        std::fs::read(&classification_path)
            .map_err(|_| ExportFailure::Malformed("privacy.classification-invalid"))?
    } else {
        artifact_bytes.clone()
    };
    let destination_token = repository_identity_token("", &identity_basis);
    let consumer_token = repository_identity_token("consumer", &identity_basis);
    let resolved = destination.resolve(&destination_token, &consumer_token);

    // Provenance: repository-backed destinations are coherent only
    // from the consumer repository; workspace destinations carry the
    // envelope's declared origin booleans. A `synthetic: true` claim
    // is honored only when corroborated (fix round 2, C-F2): the
    // artifact must sit under a project-local `tests/fixtures/<family>/`
    // whose fixture-provenance manifest declares the family
    // `origin: "synthetic"`. Uncorroborated claims drop to the
    // non-synthetic origin so the evaluator's public-fixture evidence
    // requirements apply. `derived` stays claimed (claiming derived
    // adds requirements - self-limiting).
    let synthetic_claimed = envelope_object
        .get("synthetic")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let synthetic = synthetic_claimed && synthetic_corroborated(project, artifact_path);
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
    let wire = serde_json::to_value(&decision_input)
        .map_err(|_| ExportFailure::Malformed("privacy.envelope-invalid"))?;
    let repository_name = envelope_object
        .get("repository")
        .and_then(Json::as_str)
        .filter(|name| !name.is_empty())
        .map(|name| name.to_owned());
    let protected_terms: Vec<String> = envelope_object
        .get("protectedTerms")
        .and_then(Json::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Json::as_str)
                .filter(|term| !term.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Ok(Synthesis {
        wire,
        payload_text,
        labels,
        artifact_kind: artifact_kind.to_owned(),
        artifact_ref,
        file_name: path_value.clone(),
        confinement_digest,
        repository_name,
        protected_terms,
    })
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
    let synthesis = synthesize(context, project, artifact_path, destination)?;
    // The caller-supplied consent record goes into the wire verbatim
    // (fix round 2, C-F1): the runtime never mints, adds, or corrects
    // any evidence member, and never computes a binding on the
    // caller's behalf. `binding` is a required evidence member: an
    // absent binding fails input-shape validation (exit 1); a declared
    // binding that mismatches the computed subject digest denies
    // `evidence.binding-mismatch` (exit 3). The evaluator owns the
    // check. Declared evidence proves shape, coherence, and subject
    // binding only; issuance and authenticity custody is the
    // evidence-store obligation (#121). Author evidence with
    // `lekalo privacy subject`.
    let mut wire = synthesis.wire;
    if let Some(record) = consent {
        wire["provenance"]["exportTransferConsentRef"] = record.clone();
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
    // refuses the export. The subject inputs (declared repository
    // identity and protected terms) come from the envelope; they are
    // matching inputs only and never enter any output.
    let subject = RedactionSubject {
        repository: synthesis
            .repository_name
            .as_deref()
            .map(|name| (name, RepositoryRole::ConsumerRepository)),
        protected_terms: &synthesis.protected_terms,
    };
    let redacted = redact(&RedactionRequest {
        payload: &synthesis.payload_text,
        transforms: &required_transforms(&evaluated.output),
        labels: &synthesis.labels,
        subject,
    });
    let residuals = redacted.residuals().to_vec();
    if !residuals.is_empty() {
        // Covers every branch: the always-on hygiene pseudonymized
        // what it could (paths, repository identity, tenants, URLs);
        // whatever still leaks - secrets and PII on an allow, or any
        // missed class - refuses the export, never silently ships.
        return Err(ExportFailure::ResidualLeaks {
            output: evaluated.output,
            leaks: residuals,
        });
    }
    let payload_digest = format!("sha256:{}", sha256_hex(redacted.payload().as_bytes()));
    let name = synthesis.file_name;
    let stem = std::path::Path::new(&name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.clone());
    let export_path = format!(".lekalo/privacy/exports/{name}");
    let decision_path = format!(".lekalo/privacy/decisions/export/{stem}.json");
    let mut outcome = ExportOutcome {
        decision: evaluated.output,
        artifact_kind: synthesis.artifact_kind,
        artifact_ref: synthesis.artifact_ref,
        destination,
        payload: redacted.payload().to_owned(),
        payload_digest,
        applied_transforms: redacted.applied().to_vec(),
        findings: redacted.findings().to_vec(),
        residuals,
        export_path,
        decision_path,
        confinement_digest: synthesis.confinement_digest,
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

/// The canonical subject projection identity of the synthesized
/// decision input (fix round 2, C-F1): the `{subjectDigest,
/// subjectProfileRef}` pair an operator needs to author authorizing
/// evidence for the same artifact+destination. Metadata-only; the
/// evidence positions are excluded from the projection, so authoring
/// never needs a fixpoint. The synthesis refusals apply verbatim
/// (malformed envelope, class missing/unknown, invalid attachment).
#[allow(clippy::result_large_err)]
pub fn subject_of(
    project: &Path,
    artifact_path: &Path,
    destination: DestinationSpec,
) -> Result<(String, Json), ExportFailure> {
    let context = TrustedContext::embedded()
        .map_err(|_| ExportFailure::Malformed("privacy.custody-failure"))?;
    let synthesis = synthesize(context, project, artifact_path, destination)?;
    let digest =
        authorization_subject_digest(&synthesis.wire, context.authorization_subject_profile());
    Ok((digest, profile_ref_value()))
}

/// Show the redaction diff contract of one payload document (issue
/// #119): the applied transforms, the leak findings, and the redacted
/// payload. Read-only by definition.
pub fn redact_preview(
    payload: &str,
    transforms: &[TransformId],
    labels: &[DataSensitivity],
    subject: RedactionSubject<'_>,
) -> (String, Vec<LeakFinding>, Vec<LeakFinding>, Vec<TransformId>) {
    let redacted = redact(&RedactionRequest {
        payload,
        transforms,
        labels,
        subject,
    });
    (
        redacted.payload().to_owned(),
        redacted.findings().to_vec(),
        redacted.residuals().to_vec(),
        redacted.applied().to_vec(),
    )
}

/// The propagated envelope class of one project's artifacts (issue
/// #119, plan S6): the sensitivity label of the classification
/// attachment's declared payload default, plus the exact policy
/// identity. Exportable receipt surfaces carry these as additive
/// optional `class`/`policyRef` members; `None` means the project
/// declares no classification and any export attempt refuses
/// fail-closed at class resolution.
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

/// The declared-coherence repository token (fix round 2, C-F3): the
/// opaque `repo-sha256:` digest of `"lekalo.repository-identity\n"`
/// plus the custody basis, scoped per endpoint role so distinct
/// endpoints keep distinct tokens. Never derived from a host path.
fn repository_identity_token(role_scope: &str, basis: &[u8]) -> String {
    let mut material = b"lekalo.repository-identity\n".to_vec();
    material.extend_from_slice(basis);
    if !role_scope.is_empty() {
        material.extend_from_slice(format!("\n{role_scope}").as_bytes());
    }
    format!("repo-sha256:{}", sha256_hex(&material))
}

/// Whether a `synthetic: true` envelope claim is corroborated (fix
/// round 2, C-F2): the artifact must sit inside a project-local
/// `tests/fixtures/<family>/` directory whose entry in the project's
/// `tests/fixtures/fixture-provenance.json` declares
/// `origin: "synthetic"`. Any doubt is uncorroborated.
fn synthetic_corroborated(project: &Path, artifact_path: &Path) -> bool {
    let (Ok(project_root), Ok(artifact)) = (
        std::fs::canonicalize(project),
        std::fs::canonicalize(artifact_path),
    ) else {
        return false;
    };
    let Ok(relative) = artifact.strip_prefix(&project_root) else {
        return false;
    };
    let mut segments = relative.iter();
    let is_expected = |segment: Option<&std::ffi::OsStr>, expected: &str| {
        segment.is_some_and(|segment| segment == expected)
    };
    if !is_expected(segments.next(), "tests") || !is_expected(segments.next(), "fixtures") {
        return false;
    }
    let Some(family) = segments.next() else {
        return false;
    };
    if segments.next().is_none() {
        // The artifact must sit inside the family directory.
        return false;
    }
    let family = family.to_string_lossy().into_owned();
    let manifest_path = project_root
        .join("tests")
        .join("fixtures")
        .join("fixture-provenance.json");
    let Ok(bytes) = std::fs::read(&manifest_path) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_slice::<Json>(&bytes) else {
        return false;
    };
    manifest
        .get("families")
        .and_then(Json::as_array)
        .is_some_and(|families| {
            families.iter().any(|entry| {
                entry.get("family").and_then(Json::as_str) == Some(family.as_str())
                    && entry.get("origin").and_then(Json::as_str) == Some("synthetic")
            })
        })
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

pub fn propagated_class(project: &Path) -> Option<(Vec<String>, String)> {
    let default_kind = project_payload_default(project).ok()??;
    Some((
        vec![sensitivity_of_kind(default_kind).as_str().to_owned()],
        refs::POLICY_IDENTITY.to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::redact::LeakKind;
    use super::*;
    use std::path::PathBuf;

    /// Write one envelope under a temporary project and return its path.
    fn write_artifact(project: &Path, name: &str, envelope: &Json) -> PathBuf {
        let path = project.join(name);
        std::fs::write(&path, envelope.to_string()).expect("artifact writes");
        path
    }

    /// Declare one synthetic fixture family in the temp project (the
    /// project-local provenance manifest) and return its directory
    /// (fix round 2, C-F2: `synthetic: true` is honored only when
    /// corroborated by this manifest + placement).
    fn synthetic_family(project: &Path, family: &str) -> PathBuf {
        let dir = project.join("tests").join("fixtures").join(family);
        std::fs::create_dir_all(&dir).expect("family dir");
        let manifest = serde_json::json!({
            "manifestId": "dev.lekalo.fixture-provenance",
            "version": "0.1.0",
            "families": [{ "family": family, "origin": "synthetic" }],
        });
        std::fs::write(
            project
                .join("tests")
                .join("fixtures")
                .join("fixture-provenance.json"),
            manifest.to_string(),
        )
        .expect("manifest writes");
        dir
    }

    /// Write one envelope into a corroborated synthetic family.
    fn write_synthetic_artifact(
        project: &Path,
        family: &str,
        name: &str,
        envelope: &Json,
    ) -> PathBuf {
        let dir = synthetic_family(project, family);
        let path = dir.join(name);
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

    /// A consent record for the export-transfer position, authored
    /// against the synthesized subject exactly like the CLI verb
    /// prescribes (fix round 2, C-F1): the binding is declared by the
    /// author, never minted by the runtime.
    fn authored_consent(project: &Path, artifact: &Path, destination: DestinationSpec) -> Json {
        let (subject_digest, subject_profile_ref) =
            subject_of(project, artifact, destination).expect("subject of the synthesized input");
        serde_json::json!({
            "contractId": "dev.lekalo.privacy-authorizing-evidence",
            "version": "0.2.16",
            "digest": format!("sha256:{}", refs::AUTHORIZING_EVIDENCE_RAW_SHA256),
            "evidenceKind": "export-transfer-consent",
            "purpose": "authorize-export-transfer-or-storage",
            "outcome": "granted",
            "evidenceId": format!("evidence-sha256:{}", "9".repeat(64)),
            "verificationState": "verified",
            "freshnessState": "current",
            "binding": {
                "subjectProfileRef": subject_profile_ref,
                "subjectDigest": subject_digest,
            },
        })
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
            Some(&authored_consent(
                project.path(),
                &artifact,
                DestinationSpec::Publish,
            )),
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
            Some(&authored_consent(
                project.path(),
                &artifact,
                DestinationSpec::Publish,
            )),
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
        let artifact = write_synthetic_artifact(
            project.path(),
            "fixtures",
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

    /// An allowed payload with a hygiene-class leak ships
    /// pseudonymized; a secret-class leak refuses: never silently
    /// ships.
    /// The class floor (fix round 2, C-F2): a claim below the
    /// classification attachment's unclassified-payload default
    /// widens to the floor - `public` plus a `confidential` floor
    /// publishes nothing (confidential denies publish).
    #[test]
    fn class_floor_unions_the_attachment_default() {
        let project = tempfile::tempdir().expect("temp project");
        let attachment = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/classification/valid/planner/classification.json");
        std::fs::copy(&attachment, project.path().join("classification.json"))
            .expect("attachment copies");
        let artifact = write_artifact(
            project.path(),
            "summary.json",
            &serde_json::json!({
                "artifactKind": "generated.summary",
                "payload": "summary text",
                "class": ["public"],
            }),
        );
        let outcome = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            Some(&authored_consent(
                project.path(),
                &artifact,
                DestinationSpec::Publish,
            )),
            true,
        );
        match outcome {
            Err(ExportFailure::Denied(output)) => {
                assert_eq!(output.reason_codes(), ["sensitivity.confidential.denied"]);
            }
            other => panic!("expected the floor to widen the claim, got {other:?}"),
        }
    }

    /// An uncorroborated `synthetic: true` claim drops to the
    /// non-synthetic origin, so the evaluator's public-fixture
    /// evidence requirements apply (fail closed).
    /// Repository tokens are machine-independent (fix round 2, C-F3):
    /// identical custody bytes at different host paths synthesize the
    /// identical subject - the token is never a host-path fingerprint.
    #[test]
    fn repo_tokens_are_machine_independent() {
        let make_project = |dir: &std::path::Path| {
            std::fs::create_dir_all(dir.join(".lekalo")).expect("project dir");
            let attachment = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/classification/valid/planner/classification.json");
            std::fs::copy(&attachment, dir.join("classification.json")).expect("attachment");
            let artifact = dir.join("summary.json");
            std::fs::write(
                &artifact,
                serde_json::json!({
                    "artifactKind": "generated.summary",
                    "payload": "summary text",
                    "class": ["public"],
                })
                .to_string(),
            )
            .expect("artifact");
            artifact
        };
        let one = tempfile::tempdir().expect("temp project one");
        let two = tempfile::tempdir().expect("temp project two");
        let artifact_one = make_project(one.path());
        let artifact_two = make_project(two.path());
        let (digest_one, _) =
            subject_of(one.path(), &artifact_one, DestinationSpec::TransferTenant)
                .expect("subject one");
        let (digest_two, _) =
            subject_of(two.path(), &artifact_two, DestinationSpec::TransferTenant)
                .expect("subject two");
        assert_eq!(digest_one, digest_two);
    }

    #[test]
    fn uncorroborated_synthetic_drops_to_non_synthetic() {
        let project = tempfile::tempdir().expect("temp project");
        // Placed at the project root: no tests/fixtures/<family>/ home
        // and no project-local provenance manifest.
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
            true,
        );
        match outcome {
            Err(ExportFailure::Denied(output)) => {
                assert_eq!(
                    output.reason_codes(),
                    ["fixture.permission-license-consent-required"]
                );
            }
            other => panic!("expected the corroboration deny, got {other:?}"),
        }
    }

    #[test]
    fn allow_payload_leaks_are_pseudonymized_or_refused() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_synthetic_artifact(
            project.path(),
            "fixtures",
            "fixture.json",
            &serde_json::json!({
                "artifactKind": "fixture",
                "payload": "clean text referencing https://private.example/acme",
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
        )
        .expect("the hygiene-class leak ships pseudonymized");
        assert_eq!(outcome.decision().decision().as_str(), "allow");
        assert!(outcome.payload().contains("<redacted:url>"));
        assert!(!outcome.payload().contains("https://"));
        assert_eq!(outcome.residuals(), []);

        let secret = write_synthetic_artifact(
            project.path(),
            "fixtures",
            "secret.json",
            &serde_json::json!({
                "artifactKind": "fixture",
                "payload": "token ghp_abcdefghijklmnopqrstuvwxyz0123456789abcd",
                "class": ["public"],
                "synthetic": true,
            }),
        );
        let outcome = run_export(
            project.path(),
            &secret,
            DestinationSpec::Publish,
            None,
            true,
        );
        match outcome {
            Err(ExportFailure::ResidualLeaks { leaks, .. }) => {
                assert!(leaks
                    .iter()
                    .any(|leak| leak.kind() == LeakKind::SecretToken));
            }
            other => panic!("expected the secret refusal, got {other:?}"),
        }
    }

    /// The #89 integration seam: an adapter-produced artifact's
    /// confinement evidence is read where present, recorded in the
    /// decision record, and refused when malformed. No adapter gains
    /// filesystem or network scope from this read.
    #[test]
    fn confinement_evidence_is_read_where_present() {
        let project = tempfile::tempdir().expect("temp project");
        let confinement = serde_json::json!({
            "budget": {"readScopes": [], "network": "denied"},
            "described": {},
            "effective": {},
            "platform": "linux-x86_64",
        });
        let artifact = write_synthetic_artifact(
            project.path(),
            "fixtures",
            "fixture.json",
            &serde_json::json!({
                "artifactKind": "fixture",
                "payload": "synthetic text",
                "class": ["public"],
                "synthetic": true,
                "confinement": confinement,
            }),
        );
        let outcome = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            None,
            true,
        )
        .expect("confinement-bearing export plans");
        let digest = outcome.confinement_digest().expect("confinement digest");
        assert!(digest.starts_with("sha256:"));
        let record = outcome.decision_record();
        assert_eq!(record["confinementDigest"], serde_json::json!(digest));

        // A malformed confinement member refuses fail-closed.
        let malformed = write_synthetic_artifact(
            project.path(),
            "fixtures",
            "broken.json",
            &serde_json::json!({
                "artifactKind": "fixture",
                "payload": "synthetic text",
                "class": ["public"],
                "synthetic": true,
                "confinement": {"budget": "nonsense"},
            }),
        );
        let outcome = run_export(
            project.path(),
            &malformed,
            DestinationSpec::Publish,
            None,
            true,
        );
        assert!(matches!(
            outcome,
            Err(ExportFailure::Malformed("privacy.confinement-invalid"))
        ));
    }

    /// Consent evidence is verified, never minted (fix round 2,
    /// C-F1): a record without a binding fails input-shape validation;
    /// a declared binding that mismatches the subject denies with the
    /// exact evaluator code. The runtime never repairs either.
    #[test]
    fn consent_is_verified_never_minted() {
        let project = tempfile::tempdir().expect("temp project");
        let artifact = write_artifact(
            project.path(),
            "summary.json",
            &serde_json::json!({
                "artifactKind": "generated.summary",
                "payload": "summary text",
                "class": ["public"],
            }),
        );
        let destination = DestinationSpec::TransferTenant;

        // Binding-free record: input-shape validation, not a mint.
        let mut binding_free = authored_consent(project.path(), &artifact, destination);
        binding_free.as_object_mut().unwrap().remove("binding");
        let outcome = run_export(
            project.path(),
            &artifact,
            destination,
            Some(&binding_free),
            true,
        );
        assert!(matches!(
            outcome,
            Err(ExportFailure::Malformed("privacy.input-malformed"))
        ));

        // Mismatched binding: the evaluator's binding check denies.
        let mut mismatched = authored_consent(project.path(), &artifact, destination);
        mismatched["binding"]["subjectDigest"] =
            serde_json::json!(format!("subject-sha256:{}", "f".repeat(64)));
        let outcome = run_export(
            project.path(),
            &artifact,
            destination,
            Some(&mismatched),
            true,
        );
        match outcome {
            Err(ExportFailure::Denied(output)) => {
                assert_eq!(output.reason_codes(), ["evidence.binding-mismatch"]);
            }
            other => panic!("expected the binding deny, got {other:?}"),
        }

        // The authored record passes.
        let outcome = run_export(
            project.path(),
            &artifact,
            destination,
            Some(&authored_consent(project.path(), &artifact, destination)),
            true,
        );
        assert!(outcome.is_ok());
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

    /// The propagated class of a project: a valid classification
    /// attachment yields the payload-default label plus the exact
    /// policy identity; a missing or invalid attachment yields `None`
    /// so an export attempt refuses fail-closed.
    #[test]
    fn propagated_class_follows_the_classification_attachment() {
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/classification/valid/planner");
        let propagated =
            propagated_class(&fixtures).expect("planner project declares classification");
        assert_eq!(propagated.1, refs::POLICY_IDENTITY);
        assert!(!propagated.0.is_empty());
        assert!(propagated
            .0
            .iter()
            .all(|label| DataSensitivity::parse(label).is_some()));

        let empty = tempfile::tempdir().expect("temp project");
        assert_eq!(propagated_class(empty.path()), None);

        let invalid = tempfile::tempdir().expect("temp project");
        std::fs::write(invalid.path().join("classification.json"), b"{ not json").unwrap();
        assert_eq!(propagated_class(invalid.path()), None);
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
            Some(&authored_consent(
                project.path(),
                &artifact,
                DestinationSpec::Publish,
            )),
            true,
        )
        .expect("first run");
        let two = run_export(
            project.path(),
            &artifact,
            DestinationSpec::Publish,
            Some(&authored_consent(
                project.path(),
                &artifact,
                DestinationSpec::Publish,
            )),
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
