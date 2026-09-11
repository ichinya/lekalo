//! The closed receipt wire of the generate and verify orchestration
//! (issue #91).
//!
//! Both renderers project these values; the JSON shape is published as
//! `contracts/orchestration-report.schema.v1.0.0.json`. Every array is
//! emitted in canonical order, no field carries an absolute path,
//! timestamp, host, or environment value, and partial success is never
//! spelled as full success: the verdict plus the per-target and
//! per-component states distinguish ready, degraded, and blocked.

use serde::Serialize;

/// The closed v1 wire discriminator of a generate or verify receipt.
pub const SCHEMA_VERSION: &str = "lekalo/orchestration/v1.0.0";

/// The independent contract identity of the published receipt schema.
pub const IDENTITY: &str = "dev.lekalo.orchestration-report@1.0.0";

/// The closed verdict of one orchestration invocation.
///
/// `ready` — every executed component passed and nothing degraded;
/// `degraded` — nothing required failed, but at least one executed
/// component reported findings or an explicit absence; `blocked` — at
/// least one executed component failed. The CLI maps the verdict onto
/// the accepted exit classes; the receipt always carries the detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Ready,
    Degraded,
    Blocked,
}

impl Verdict {
    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Blocked => "blocked",
        }
    }
}

/// The closed per-target state of a generate run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetState {
    /// The plan was computed and listed; nothing was written.
    Planned,
    /// The plan was applied, verified, and the manifest updated.
    Applied,
    /// The target failed; the failure stayed isolated to this row.
    Failed,
}

impl TargetState {
    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Applied => "applied",
            Self::Failed => "failed",
        }
    }
}

/// The closed component state of a verify run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ComponentState {
    /// The component executed and passed.
    Pass,
    /// The component executed and reported findings that do not block it.
    Degraded,
    /// The component failed.
    Fail,
    /// The component cannot run in this project or build (declared, never
    /// silently skipped).
    Unsupported,
    /// The component has no input in this project (legal absence).
    Absent,
}

impl ComponentState {
    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Degraded => "degraded",
            Self::Fail => "fail",
            Self::Unsupported => "unsupported",
            Self::Absent => "absent",
        }
    }
}

/// The generation inputs one run binds: the exact canonical Model and
/// typed-IR payload pins and the revision digest over both.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InputsReceipt {
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
    /// The revision digest binding both inputs.
    pub revision: String,
}

/// The generation or verification scope of one run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ScopeReceipt {
    /// Every module id the scope covers, sorted; empty with
    /// `allModules` set.
    pub modules: Vec<String>,
    /// Whether the scope is the whole project.
    #[serde(rename = "allModules")]
    pub all_modules: bool,
}

/// The canonical IR evidence one run binds or consumes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IrEvidenceReceipt {
    /// The project-relative logical path of the evidence file.
    pub path: String,
    /// The SHA-256 over the exact canonical IR bytes.
    pub digest: String,
}

/// One planned or applied file action of one target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WriteReceipt {
    /// The project-relative logical path.
    pub path: String,
    /// The closed action: `create`, `replace`, or `delete`.
    pub action: String,
    /// The SHA-256 over the exact intended bytes; deletions carry none.
    #[serde(rename = "sha256")]
    pub digest: Option<String>,
}

/// The adapter identity a run bound, deep-equal to the locked pins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdapterReceipt {
    pub id: String,
    pub version: String,
    /// The exact locked package digest.
    pub digest: String,
}

/// One target result of a generate run; failures stay isolated here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetReceipt {
    /// The requested target id.
    pub target: String,
    /// The selected profile token, when the adapter declared one.
    pub profile: Option<String>,
    /// The bound adapter identity.
    pub adapter: AdapterReceipt,
    /// What the run did for this target.
    pub state: TargetState,
    /// The registered reason token when the state is `failed`.
    #[serde(rename = "reasonCode")]
    pub reason_code: Option<String>,
    /// The protocol plan identity the exchange bound (dry-run and
    /// apply); absent for a failed target.
    #[serde(rename = "planId")]
    pub plan_id: Option<String>,
    /// The planned (dry-run) or applied (apply) file actions, in
    /// canonical `(path, action)` order; empty for a failed target.
    pub writes: Vec<WriteReceipt>,
    /// The ownership-manifest digest after the atomic update (apply).
    #[serde(rename = "manifestDigest")]
    pub manifest_digest: Option<String>,
}

/// The aggregate counts over the per-target results.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetCounts {
    pub targets: usize,
    pub planned: usize,
    pub applied: usize,
    pub failed: usize,
}

/// The success receipt of one generate invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GenerateReceipt {
    #[serde(rename = "schema_version")]
    pub schema_version: &'static str,
    /// Always `generate`.
    pub operation: &'static str,
    /// `dry-run` or `apply`.
    pub mode: &'static str,
    /// The contract identity of the published receipt schema.
    pub identity: &'static str,
    /// The immutable project id.
    pub project: String,
    /// The exact committed lock digest the run is bound to.
    #[serde(rename = "lockDigest")]
    pub lock_digest: String,
    /// Whether the run demanded the full locked inventory (`--locked`).
    pub locked: bool,
    /// The generation inputs the run bound.
    pub inputs: InputsReceipt,
    /// The canonical IR evidence the run wrote or verified.
    #[serde(rename = "irEvidence")]
    pub ir_evidence: IrEvidenceReceipt,
    /// The generation scope.
    pub scope: ScopeReceipt,
    /// One isolated result per target, sorted by target id.
    pub targets: Vec<TargetReceipt>,
    /// The aggregate counts.
    pub counts: TargetCounts,
    /// The closed verdict.
    pub verdict: Verdict,
}

/// One executed verify component.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ComponentReceipt {
    /// The stable component id (`artifact.drift`, `adapter.<target>`,
    /// `bindings.registry`, `model.validation`, `native.gates`,
    /// `scenarios.execution`, `scenarios.portable`, `trace.summary`).
    pub id: String,
    /// Whether a non-pass state fails the invocation.
    pub required: bool,
    pub state: ComponentState,
    /// The registered reason token when the state is not `pass`.
    #[serde(rename = "reasonCode")]
    pub reason_code: Option<String>,
    /// The bounded severity counts of the core validation component.
    #[serde(rename = "severityCounts")]
    pub severity_counts: Option<SeverityCounts>,
    /// The drift verdict counts of the drift component.
    #[serde(rename = "verdictCounts")]
    pub verdict_counts: Option<VerdictCountsReceipt>,
    /// The number of reported adapter findings (validate/verify results).
    pub findings: Option<usize>,
    /// The scenario coverage summary of the portable-scenario component.
    pub scenarios: Option<ScenarioCoverage>,
    /// The trace summary of the trace component.
    pub trace: Option<TraceSummary>,
}

/// The severity counts of one semantic validation run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeverityCounts {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

/// The bounded drift verdict counts of one check.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VerdictCountsReceipt {
    pub artifacts: usize,
    pub clean: usize,
    pub stale: usize,
    #[serde(rename = "manualDrift")]
    pub manual_drift: usize,
    pub missing: usize,
    pub orphan: usize,
    pub blocking: usize,
}

/// The portable scenario coverage summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ScenarioCoverage {
    /// The number of scenario definitions in scope.
    pub scenarios: usize,
    /// The number of distinct operations covered by at least one
    /// scenario.
    #[serde(rename = "coveredOperations")]
    pub covered_operations: usize,
    /// The number of scenario definitions without any `covers` reference.
    #[serde(rename = "uncoveredScenarios")]
    pub uncovered_scenarios: usize,
}

/// The neutral trace summary of one manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TraceSummary {
    /// The number of relations in the manifest.
    pub relations: usize,
    /// The number of explicit gaps.
    pub gaps: usize,
    /// The number of uncovered sinks.
    #[serde(rename = "uncoveredSinks")]
    pub uncovered_sinks: usize,
}

/// The success receipt of one verify invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VerifyReceipt {
    #[serde(rename = "schema_version")]
    pub schema_version: &'static str,
    /// Always `verify`.
    pub operation: &'static str,
    /// `full` or `changed`.
    pub mode: &'static str,
    /// The contract identity of the published receipt schema.
    pub identity: &'static str,
    /// The immutable project id.
    pub project: String,
    /// The exact committed lock digest the run is bound to.
    #[serde(rename = "lockDigest")]
    pub lock_digest: String,
    /// Whether the run demanded the full locked inventory (`--locked`).
    pub locked: bool,
    /// The generation inputs the run verified against.
    pub inputs: InputsReceipt,
    /// The verified scope.
    pub scope: ScopeReceipt,
    /// One row per executed or declared-absent component, in fixed id
    /// order.
    pub components: Vec<ComponentReceipt>,
    /// The closed verdict.
    pub verdict: Verdict,
}
