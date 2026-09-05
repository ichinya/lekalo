//! The generated-artifact ownership manifest and drift detection
//! (issue #21).
//!
//! The manifest binds every generated, scaffolded, checked, external, and
//! custom artifact to its semantic owner, its exact locked adapter and
//! generator identity, the exact lockfile revision, the exact generation
//! inputs, and the SHA-256 of the file's observed bytes. The checker is
//! hermetic and read-only: it verifies bindings, inventories content,
//! validates source maps, and scans the declared managed root for orphans,
//! then reports deterministic verdicts. Cleaning is a separately planned,
//! explicitly confirmed, fully revalidated file operation — the plan
//! phase computes only, and only the confirm phase deletes.
//!
//! Wire contract: [`contracts/artifact-manifest.schema.v1.0.0.json`].
//! Diagnostics reuse the accepted closed registry (no new rules); drift
//! verdicts are typed result data mapped onto registered rules with
//! bounded tokens only.

mod canonical;
mod check;
mod clean;
mod diagnostic;
mod source_map;
mod types;

pub use clean::{CleanFile, CleanPlanReceipt, CleanReceipt, CleanService};
pub use types::{
    AdapterRef, ArtifactEntry, ArtifactKey, ArtifactKind, ArtifactManifest, ArtifactPath,
    DriftVerdict, GeneratedPath, Lifecycle, LockRevision, ProjectRef, RegenerationPolicy,
    SemanticOwnerId, SourceMapBinding, SourceRange, GENERATED_ROOT, MANIFEST_DIR, MANIFEST_NAME,
    MAX_ARTIFACT_BYTES, MAX_MANIFEST_BYTES, SCHEMA_VERSION,
};

use serde::Serialize;

use crate::loader::LoadSelection;

/// `lekalo generate --check`: read-only drift detection.
///
/// Absent manifest and an empty managed root are a clean vacuous check;
/// anything else is verified against the exact lock, inputs, adapters, and
/// bytes. Blocking verdicts (generated staleness, drift, or absence, and
/// any orphan) fail the check with per-artifact diagnostics; findings on
/// never-overwritten lifecycles (scaffolded, checked, external, custom)
/// are reported without blocking and never repaired.
pub struct GenerateService;

impl GenerateService {
    /// The current generation inputs of a project: the exact canonical
    /// Model and typed-IR payload pins, their revision digest, and the
    /// exact lock digest. This is the same binding `check` verifies a
    /// manifest against, exposed for manifest authors and for the later
    /// generation pipeline (#91).
    pub fn inputs(selection: &LoadSelection) -> Result<InputsReceipt, ArtifactFailure> {
        let prepared = check::Prepared::prepare(selection)?;
        let inputs = prepared.inputs();
        Ok(InputsReceipt {
            status: "valid",
            operation: "generate",
            mode: "inputs",
            lock_digest: prepared.lock().digest().as_str().to_owned(),
            model_version: inputs.model().version().as_str().to_owned(),
            model_digest: inputs.model().digest().as_str().to_owned(),
            ir_version: inputs.ir().version().as_str().to_owned(),
            ir_digest: inputs.ir().digest().as_str().to_owned(),
            revision: check::inputs_revision(inputs.model(), inputs.ir())
                .as_str()
                .to_owned(),
        })
    }

    pub fn check(selection: &LoadSelection) -> Result<CheckReceipt, ArtifactFailure> {
        let prepared = check::Prepared::prepare(selection)?;
        check::run_check(&prepared)
    }

    /// `lekalo generate --clean --dry-run`: compute the deterministic
    /// clean plan without touching a single byte.
    pub fn clean_plan(selection: &LoadSelection) -> Result<CleanPlanReceipt, ArtifactFailure> {
        clean::CleanService::plan(selection)
    }

    /// `lekalo generate --clean --confirm sha256:<planId>`: apply exactly
    /// that plan after full revalidation, or delete nothing.
    pub fn clean_apply(
        selection: &LoadSelection,
        plan_id: &str,
    ) -> Result<CleanReceipt, ArtifactFailure> {
        clean::CleanService::apply(selection, plan_id)
    }
}

/// The receipt of the current generation inputs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InputsReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `generate`.
    pub operation: &'static str,
    /// Always `inputs` for this receipt.
    pub mode: &'static str,
    /// The exact lock digest the inputs are bound to.
    #[serde(rename = "lockDigest")]
    pub lock_digest: String,
    /// The exact canonical Model contract version.
    #[serde(rename = "modelVersion")]
    pub model_version: String,
    /// The SHA-256 over the canonical Model payload bytes.
    #[serde(rename = "modelDigest")]
    pub model_digest: String,
    /// The exact IR contract version.
    #[serde(rename = "irVersion")]
    pub ir_version: String,
    /// The SHA-256 over the canonical typed-IR payload bytes.
    #[serde(rename = "irDigest")]
    pub ir_digest: String,
    /// The revision digest binding both inputs (the source-map domain).
    pub revision: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct VerdictCounts {
    pub artifacts: usize,
    pub clean: usize,
    pub stale: usize,
    #[serde(rename = "manualDrift")]
    pub manual_drift: usize,
    pub missing: usize,
    pub orphan: usize,
    pub reported: usize,
}

/// One reported or blocking finding of a check.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize)]
pub struct DriftFinding {
    /// The project-relative logical path of the artifact or orphan.
    pub path: String,
    /// The semantic owner, when the finding came from a manifest entry.
    pub owner: Option<String>,
    /// The artifact kind, when known.
    pub kind: Option<&'static str>,
    /// The lifecycle, when known.
    pub lifecycle: Option<&'static str>,
    /// The closed verdict.
    pub verdict: &'static str,
}

impl DriftFinding {
    pub(crate) fn entry(entry: &ArtifactEntry, verdict: DriftVerdict) -> Self {
        Self {
            path: entry.key().path().as_str().to_owned(),
            owner: Some(entry.key().semantic_owner().as_str().to_owned()),
            kind: Some(entry.key().kind().as_str()),
            lifecycle: Some(entry.lifecycle().as_str()),
            verdict: verdict.as_str(),
        }
    }

    pub(crate) fn orphan(path: GeneratedPath) -> Self {
        Self {
            path: path.as_str().to_owned(),
            owner: None,
            kind: None,
            lifecycle: None,
            verdict: DriftVerdict::Orphan.as_str(),
        }
    }
}

/// The receipt of a successful `generate --check`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CheckReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `generate`.
    pub operation: &'static str,
    /// Always `check` for this receipt.
    pub mode: &'static str,
    /// The verified manifest digest; `None` for the vacuous no-manifest check.
    #[serde(rename = "manifestDigest")]
    pub manifest_digest: Option<String>,
    /// The exact lock digest the check bound to.
    #[serde(rename = "lockDigest")]
    pub lock_digest: String,
    /// `clean` when nothing was reported; `reported` when only
    /// non-blocking findings exist.
    pub verdict: &'static str,
    /// Verdict counts.
    pub counts: VerdictCounts,
    /// The sorted non-blocking findings.
    pub findings: Vec<DriftFinding>,
}

/// Why a generate operation failed; every variant maps onto exactly one
/// registered diagnostic rule (or an accepted loader result passthrough).
#[derive(Clone, Debug)]
pub enum ArtifactFailure {
    /// The accepted loader/IR pipeline refused the project; its exact
    /// domain result is terminal for the command.
    Loader(crate::result::DomainResult),
    /// A lock-domain failure (missing, stale, denied, ...) from the
    /// accepted #10 service.
    Lock(crate::lockfile::LockFailure),
    /// The manifest bytes are valid JSON but not the canonical spelling.
    ManifestNoncanonical,
    /// The manifest is not a valid v1 wire document.
    ManifestInvalid,
    /// A future `lekalo/artifact-manifest/vX` discriminator.
    UnsupportedSchemaVersion {
        /// The bounded future discriminator spelling.
        found: String,
    },
    /// The manifest fails its own integrity digest.
    ManifestDigestMismatch,
    /// A sorted, duplicate, or cross-reference invariant failed.
    ReferenceInvalid,
    /// A source-map range invariant failed.
    SourceMapInvalid,
    /// The manifest was generated under another lock revision or other
    /// generation inputs; it is stale as a whole.
    StaleManifest,
    /// A path policy refusal with its stable `structure.*` code.
    Structure {
        /// The registered structure code.
        code: &'static str,
        /// Whether the refusal is the denied exit class.
        denied: bool,
    },
    /// A bounded infrastructure read failure.
    Io(&'static str),
    /// Blocking drift findings: generated staleness/drift/absence and
    /// orphans, one diagnostic per finding.
    Drift(Vec<DriftFinding>),
    /// A mutating clean without a bound preview or exact confirmation.
    PreviewRequired,
    /// The project, manifest, or plan changed between preview and apply.
    PlanChanged,
}

impl From<crate::lockfile::LockFailure> for ArtifactFailure {
    fn from(failure: crate::lockfile::LockFailure) -> Self {
        Self::Lock(failure)
    }
}
