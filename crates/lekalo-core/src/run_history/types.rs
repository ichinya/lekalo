//! The typed wire models of the run history (issue #121).
//!
//! Input models (harness observations) deserialize with closed-field
//! denials; output models (run records, assertion sets, the store
//! document) serialize to the canonical bytes validated by the same
//! versioned schemas under `contracts/`. The wire spelling is camelCase
//! except the established `schema_version` discriminator.

use serde::Deserialize;
use serde::Serialize;

use super::value::Vs;

/// One closed measured u64 metric leaf of an observation input. An
/// absent leaf normalizes to [`Vs::Unknown`].
pub(crate) type CountIn = Option<Vs<u64>>;

/// The closed pilot shape shared by observations and records.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Pilot {
    pub(crate) mode: PilotMode,
    pub(crate) scope_state: ScopeState,
}

/// The closed pilot mode vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PilotMode {
    Greenfield,
    Brownfield,
}

/// The closed measured-scope state vocabulary. Untouched legacy stays
/// `observed`; it is never promoted to `contracted` by recording.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ScopeState {
    Observed,
    Contracted,
    Hybrid,
}

/// The closed operation shape.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Operation {
    pub(crate) kind: OperationKind,
    pub(crate) affected_semantic_ids: Vec<String>,
}

/// The closed supported-operation vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OperationKind {
    Scan,
    Verify,
    Context,
    Generate,
    Nfr,
    Scenario,
    Gate,
    Evaluation,
}

/// The closed terminal operation verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Outcome {
    Pass,
    Warn,
    Fail,
    Unsupported,
    Infrastructure,
}

impl Outcome {
    /// The exact wire spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Unsupported => "unsupported",
            Self::Infrastructure => "infrastructure",
        }
    }
}

/// The closed coverage vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CoverageState {
    Complete,
    Incomplete,
    Unknown,
}

/// The closed status shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct StatusShape {
    pub(crate) outcome: Outcome,
    pub(crate) coverage_state: CoverageState,
}

// ---------------------------------------------------------------------------
// Provenance
// ---------------------------------------------------------------------------

/// The exact provenance pins of one run (record spelling: every leaf is
/// a closed value state).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Provenance {
    pub(crate) git: GitProvenance,
    pub(crate) model: ModelProvenance,
    pub(crate) lock: LockProvenance,
    pub(crate) core: CoreProvenance,
    pub(crate) adapters: Vec<AdapterProvenance>,
    pub(crate) profile: ProfileProvenance,
    pub(crate) harness: HarnessProvenance,
}

/// The exact local Git pins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitProvenance {
    pub(crate) commit: Vs<String>,
    pub(crate) dirty: Vs<bool>,
    pub(crate) working_set_digest: Vs<String>,
}

/// The exact governed model pins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelProvenance {
    pub(crate) revision: Vs<String>,
    pub(crate) digest: Vs<String>,
    pub(crate) ir_digest: Vs<String>,
}

/// The exact project lock pins. A missing lock stays unknown; it never
/// hashes an empty file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LockProvenance {
    pub(crate) version: Vs<String>,
    pub(crate) digest: Vs<String>,
}

/// The executing core build pins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CoreProvenance {
    pub(crate) version: Vs<String>,
    pub(crate) build_revision: Vs<String>,
    pub(crate) build_digest: Vs<String>,
}

/// One resolved adapter with exact installed pins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct AdapterProvenance {
    pub(crate) id: String,
    pub(crate) version: Vs<String>,
    pub(crate) manifest_digest: Vs<String>,
    pub(crate) bundle_digest: Vs<String>,
}

/// The effective resolved profile pins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileProvenance {
    pub(crate) id: Vs<String>,
    pub(crate) version: Vs<String>,
    pub(crate) digest: Vs<String>,
}

/// The harness identity pins supplied by the harness itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HarnessProvenance {
    pub(crate) id: Vs<String>,
    pub(crate) version: Vs<String>,
    pub(crate) model_id: Vs<String>,
    pub(crate) model_revision: Vs<String>,
}

/// The observation input provenance: every leaf optional; an absent
/// leaf normalizes to unknown.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ProvenanceIn {
    pub(crate) git: Option<GitProvenanceIn>,
    pub(crate) model: Option<ModelProvenanceIn>,
    pub(crate) lock: Option<LockProvenanceIn>,
    pub(crate) core: Option<CoreProvenanceIn>,
    #[serde(default)]
    pub(crate) adapters: Vec<AdapterProvenance>,
    pub(crate) profile: Option<ProfileProvenanceIn>,
    pub(crate) harness: Option<HarnessProvenanceIn>,
}

/// Observation-input Git pins.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct GitProvenanceIn {
    pub(crate) commit: Option<Vs<String>>,
    pub(crate) dirty: Option<Vs<bool>>,
    pub(crate) working_set_digest: Option<Vs<String>>,
}

/// Observation-input model pins.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ModelProvenanceIn {
    pub(crate) revision: Option<Vs<String>>,
    pub(crate) digest: Option<Vs<String>>,
    pub(crate) ir_digest: Option<Vs<String>>,
}

/// Observation-input lock pins.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct LockProvenanceIn {
    pub(crate) version: Option<Vs<String>>,
    pub(crate) digest: Option<Vs<String>>,
}

/// Observation-input core pins.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct CoreProvenanceIn {
    pub(crate) version: Option<Vs<String>>,
    pub(crate) build_revision: Option<Vs<String>>,
    pub(crate) build_digest: Option<Vs<String>>,
}

/// Observation-input profile pins.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ProfileProvenanceIn {
    pub(crate) id: Option<Vs<String>>,
    pub(crate) version: Option<Vs<String>>,
    pub(crate) digest: Option<Vs<String>>,
}

/// Observation-input harness pins.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct HarnessProvenanceIn {
    pub(crate) id: Option<Vs<String>>,
    pub(crate) version: Option<Vs<String>>,
    pub(crate) model_id: Option<Vs<String>>,
    pub(crate) model_revision: Option<Vs<String>>,
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// The closed metrics block of a record (record spelling).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metrics {
    pub(crate) duration_ms: Vs<u64>,
    pub(crate) files_read: Vs<u64>,
    pub(crate) files_changed: Vs<u64>,
    pub(crate) tool_calls: Vs<u64>,
    pub(crate) retry_count: Vs<u64>,
    pub(crate) replan_count: Vs<u64>,
    pub(crate) tokens: Tokens,
    pub(crate) cost: Cost,
    pub(crate) context: Context,
}

/// The closed token metrics. `total` is never inferred from operands.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Tokens {
    pub(crate) input: Vs<u64>,
    pub(crate) output: Vs<u64>,
    pub(crate) reasoning: Vs<u64>,
    pub(crate) total: Vs<u64>,
}

/// The closed cost metrics. The recorder never looks up pricing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Cost {
    pub(crate) amount: Vs<String>,
    pub(crate) currency: Vs<String>,
    pub(crate) basis: Vs<CostBasis>,
}

/// The closed cost basis vocabulary. An estimated cost is never
/// silently presented as billed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CostBasis {
    Reported,
    Estimated,
}

/// The closed context-capsule metrics. Only capsule metadata is
/// representable; capsule bodies are never recorded.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Context {
    pub(crate) bytes: Vs<u64>,
    pub(crate) estimated_tokens: Vs<u64>,
    pub(crate) included_facts: Vs<u64>,
    pub(crate) candidate_facts: Vs<u64>,
    pub(crate) coverage_ratio: Vs<String>,
    pub(crate) representation: Vs<Representation>,
    pub(crate) estimator_version: Vs<String>,
}

/// The closed measured-representation vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Representation {
    Json,
    Markdown,
}

/// The observation-input metrics block: every leaf optional.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct MetricsIn {
    pub(crate) duration_ms: CountIn,
    pub(crate) files_read: CountIn,
    pub(crate) files_changed: CountIn,
    pub(crate) tool_calls: CountIn,
    pub(crate) retry_count: CountIn,
    pub(crate) replan_count: CountIn,
    pub(crate) tokens: Option<TokensIn>,
    pub(crate) cost: Option<CostIn>,
    pub(crate) context: Option<ContextIn>,
}

/// Observation-input token metrics.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct TokensIn {
    pub(crate) input: CountIn,
    pub(crate) output: CountIn,
    pub(crate) reasoning: CountIn,
    pub(crate) total: CountIn,
}

/// Observation-input cost metrics.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct CostIn {
    pub(crate) amount: Option<Vs<String>>,
    pub(crate) currency: Option<Vs<String>>,
    pub(crate) basis: Option<Vs<CostBasis>>,
}

/// Observation-input context metrics.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ContextIn {
    pub(crate) bytes: CountIn,
    pub(crate) estimated_tokens: CountIn,
    pub(crate) included_facts: CountIn,
    pub(crate) candidate_facts: CountIn,
    pub(crate) coverage_ratio: Option<Vs<String>>,
    pub(crate) representation: Option<Vs<Representation>>,
    pub(crate) estimator_version: Option<Vs<String>>,
}

// ---------------------------------------------------------------------------
// Summaries, sources, diagnostics
// ---------------------------------------------------------------------------

/// One measurement source binding a known metric leaf to its origin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct MeasurementSource {
    pub(crate) field: MeasurementField,
    pub(crate) source_kind: SourceKind,
    pub(crate) source_id: String,
    pub(crate) source_version: Vs<String>,
}

/// The closed metric field-path vocabulary of measurement sources.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MeasurementField {
    #[serde(rename = "metrics.durationMs")]
    DurationMs,
    #[serde(rename = "metrics.filesRead")]
    FilesRead,
    #[serde(rename = "metrics.filesChanged")]
    FilesChanged,
    #[serde(rename = "metrics.toolCalls")]
    ToolCalls,
    #[serde(rename = "metrics.retryCount")]
    RetryCount,
    #[serde(rename = "metrics.replanCount")]
    ReplanCount,
    #[serde(rename = "metrics.tokens.input")]
    TokensInput,
    #[serde(rename = "metrics.tokens.output")]
    TokensOutput,
    #[serde(rename = "metrics.tokens.reasoning")]
    TokensReasoning,
    #[serde(rename = "metrics.tokens.total")]
    TokensTotal,
    #[serde(rename = "metrics.cost.amount")]
    CostAmount,
    #[serde(rename = "metrics.context.bytes")]
    ContextBytes,
    #[serde(rename = "metrics.context.estimatedTokens")]
    ContextEstimatedTokens,
    #[serde(rename = "metrics.context.includedFacts")]
    ContextIncludedFacts,
    #[serde(rename = "metrics.context.candidateFacts")]
    ContextCandidateFacts,
    #[serde(rename = "metrics.context.coverageRatio")]
    ContextCoverageRatio,
}

/// The closed measurement-origin vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceKind {
    Core,
    Adapter,
    Harness,
    Derived,
}

/// One test/gate summary with the original source outcome preserved.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct GateSummary {
    pub(crate) id: String,
    pub(crate) kind: SummaryKind,
    pub(crate) source_outcome: SourceOutcome,
    pub(crate) passed: CountIn,
    pub(crate) failed: CountIn,
    pub(crate) unsupported: CountIn,
    pub(crate) infrastructure: CountIn,
    pub(crate) coverage_state: CoverageState,
    #[serde(default)]
    pub(crate) evidence_ref: Option<String>,
}

/// The closed summary subject vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SummaryKind {
    Test,
    Gate,
    Nfr,
}

/// The closed source-outcome vocabulary the recorder maps from. The
/// original spelling is preserved in the record; the mapped outcome is
/// derived by the exhaustive [`super::validate::map_source_outcome`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceOutcome {
    Passed,
    Failed,
    Degraded,
    Blocked,
    Security,
    Missing,
    Unsupported,
    Infrastructure,
}

/// One stored diagnostic summary by stable registry code.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct DiagnosticSummary {
    pub(crate) code: String,
    pub(crate) severity: DiagnosticSeverity,
    pub(crate) count: u64,
}

/// The closed diagnostic severity vocabulary (the registry labels).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/// One assertion row: identity, subject, kind, and outcome only. No
/// expected/actual values, expressions, failure prose, or excerpts are
/// representable.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct AssertionRow {
    pub(crate) assertion_id: String,
    pub(crate) subject_semantic_id: Option<String>,
    pub(crate) kind: AssertionKind,
    pub(crate) outcome: AssertionOutcome,
    pub(crate) evidence_ref: Option<String>,
}

/// The closed assertion-kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AssertionKind {
    Behavior,
    Contract,
    Invariant,
    Scenario,
    Gate,
    Custom,
}

/// The closed assertion-outcome vocabulary. A degraded row stays
/// degraded in the set; the run-status projection maps it to warn with
/// incomplete coverage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AssertionOutcome {
    Pass,
    Fail,
    Unsupported,
    Infrastructure,
    Degraded,
}

/// The inline observation assertions: rows only; the set id and the
/// digest binding are recorder-owned.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct AssertionsIn {
    pub(crate) rows: Vec<AssertionRow>,
}

/// The durable assertion-set document stored separately from metrics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssertionSet {
    #[serde(rename = "schema_version")]
    pub(crate) schema_version: &'static str,
    pub(crate) identity: &'static str,
    pub(crate) artifact_kind: &'static str,
    pub(crate) set_id: String,
    pub(crate) run_id: String,
    pub(crate) scope: SetScope,
    pub(crate) policy_ref: PolicyRef,
    pub(crate) authority_ref: AuthorityRef,
    pub(crate) data_sensitivity: String,
    pub(crate) export_disposition: &'static str,
    pub(crate) rows: Vec<AssertionRow>,
}

/// The scope binding of an assertion set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetScope {
    pub(crate) repository_id: String,
    pub(crate) tenant_scope_id: String,
}

// ---------------------------------------------------------------------------
// Run record
// ---------------------------------------------------------------------------

/// The record scope: locally generated opaque ids and the #120 role
/// alias. No repository name, remote URL, tenant name, or path hash is
/// representable.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordScope {
    pub(crate) repository_id: String,
    pub(crate) tenant_scope_id: String,
    pub(crate) repository_role: String,
}

/// The digest-bound reference to the separately stored assertion set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssertionsRef {
    pub(crate) set_id: String,
    pub(crate) digest: String,
    pub(crate) count: u64,
}

/// The repeat link to a parent run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepeatLink {
    pub(crate) parent_run_id: String,
    pub(crate) input_fingerprint: String,
    pub(crate) comparability: Comparability,
}

/// The closed repeat-comparability vocabulary. Exact requires a live
/// same-scope parent and all required pins known and equal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Comparability {
    Exact,
    Changed,
    Incomplete,
}

/// The exact #120 accepted policy reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicyRef {
    pub(crate) policy_id: &'static str,
    pub(crate) version: &'static str,
    pub(crate) digest: &'static str,
}

/// The exact #120 accepted authority reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuthorityRef {
    pub(crate) contract_id: &'static str,
    pub(crate) version: &'static str,
    pub(crate) digest: &'static str,
}

/// The exact #120 classification-decision contract reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClassificationRef {
    pub(crate) contract_id: &'static str,
    pub(crate) version: &'static str,
}

/// The privacy block of a record: closed labels, the exact #120
/// references, and the constant local-private custody.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrivacyBlock {
    pub(crate) data_sensitivity: String,
    pub(crate) export_disposition: &'static str,
    pub(crate) policy_ref: PolicyRef,
    pub(crate) authority_ref: AuthorityRef,
    pub(crate) classification_contract_ref: ClassificationRef,
    pub(crate) provenance: PrivacyProvenance,
    pub(crate) export_eligibility: &'static str,
}

/// The privacy provenance child set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrivacyProvenance {
    pub(crate) origin: PrivacyOrigin,
    pub(crate) repository_role: String,
    pub(crate) derived: bool,
    pub(crate) source_refs: Vec<String>,
}

/// The closed privacy-origin vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PrivacyOrigin {
    Local,
    // The imported-origin label of the closed vocabulary; the M6
    // recorder only produces local-origin records, the label stays
    // representable for the #102 consumer seam.
    #[allow(dead_code)]
    Imported,
}

/// The durable run-record document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunRecord {
    #[serde(rename = "schema_version")]
    pub(crate) schema_version: &'static str,
    pub(crate) identity: &'static str,
    pub(crate) artifact_kind: &'static str,
    pub(crate) run_id: String,
    pub(crate) timestamp: String,
    pub(crate) recorded_at: String,
    pub(crate) scope: RecordScope,
    pub(crate) pilot: Pilot,
    pub(crate) provenance: Provenance,
    pub(crate) operation: Operation,
    pub(crate) status: StatusShape,
    pub(crate) metrics: Metrics,
    pub(crate) measurement_sources: Vec<MeasurementSource>,
    pub(crate) test_gate_summaries: Vec<GateSummary>,
    pub(crate) diagnostics: Vec<DiagnosticSummary>,
    pub(crate) assertions_ref: Option<AssertionsRef>,
    pub(crate) repeat: Option<RepeatLink>,
    pub(crate) privacy: PrivacyBlock,
}

// ---------------------------------------------------------------------------
// Observation and store document
// ---------------------------------------------------------------------------

/// The closed harness observation (`history record --input -`).
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Observation {
    #[serde(rename = "schema_version")]
    pub(crate) schema_version: String,
    pub(crate) identity: String,
    #[serde(default)]
    pub(crate) run_id: Option<String>,
    pub(crate) pilot: Pilot,
    pub(crate) operation: Operation,
    pub(crate) timestamp: String,
    pub(crate) provenance: ProvenanceIn,
    pub(crate) metrics: MetricsIn,
    pub(crate) measurement_sources: Vec<MeasurementSource>,
    pub(crate) test_gate_summaries: Vec<GateSummary>,
    pub(crate) diagnostics: Vec<DiagnosticSummary>,
    pub(crate) assertions: Option<AssertionsIn>,
    pub(crate) repeat_parent_run_id: Option<String>,
    pub(crate) status: StatusShape,
    pub(crate) data_sensitivity: String,
}

/// One bound source of a dependent reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DependentSource {
    pub(crate) run_id: String,
    pub(crate) record_digest: String,
    pub(crate) assertion_digest: Option<String>,
}

/// One dependent reference of the store document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DependentEntry {
    pub(crate) id: String,
    pub(crate) kind: DependentKind,
    pub(crate) state: DependentState,
    pub(crate) generation: u64,
    pub(crate) source_runs: Vec<DependentSource>,
}

/// The closed dependent-kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DependentKind {
    Index,
    Claim,
    AggregateInput,
}

impl DependentKind {
    /// The exact wire spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Claim => "claim",
            Self::AggregateInput => "aggregate-input",
        }
    }

    /// Parse the exact wire spelling.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "index" => Some(Self::Index),
            "claim" => Some(Self::Claim),
            "aggregate-input" => Some(Self::AggregateInput),
            _ => None,
        }
    }
}

/// The closed dependent-state vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DependentState {
    Valid,
    Invalidated,
}

/// The configurable retention bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Retention {
    pub(crate) max_age_days: u32,
    pub(crate) max_records: u32,
    pub(crate) max_bytes: u64,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            max_age_days: super::limits::DEFAULT_MAX_AGE_DAYS,
            max_records: super::limits::DEFAULT_MAX_RECORDS,
            max_bytes: super::limits::DEFAULT_MAX_BYTES,
        }
    }
}

/// One tenant scope of the store document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TenantScope {
    pub(crate) tenant_scope_id: String,
    pub(crate) created_at: String,
}

/// The logical store state document (the recovery/retention projection).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoreDocument {
    #[serde(rename = "schema_version")]
    pub(crate) schema_version: &'static str,
    pub(crate) identity: &'static str,
    pub(crate) repository_id: String,
    pub(crate) tenant_scopes: Vec<TenantScope>,
    pub(crate) generation: u64,
    pub(crate) retention: Retention,
    pub(crate) dependents: Vec<DependentEntry>,
}
