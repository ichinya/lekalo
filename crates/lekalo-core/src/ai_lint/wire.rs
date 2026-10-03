//! Closed evidence and policy documents. No target syntax is interpreted here.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "value",
    rename_all = "lowercase",
    deny_unknown_fields
)]
pub enum State<T> {
    Known(T),
    Unknown,
    Withheld,
    Unsupported,
}
impl<T> State<T> {
    pub fn known(&self) -> Option<&T> {
        if let Self::Known(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Unknown,
    Low,
    Medium,
    High,
    Exact,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Claim {
    Structural,
    PossibleBehavior,
    VerifiedBehavior,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Currency {
    Current,
    Stale,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Explicit,
    Extracted,
    Inferred,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverageState {
    Complete,
    Partial,
    Unknown,
    Unsupported,
    Withheld,
    Disabled,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Coverage {
    pub rule: String,
    pub target: String,
    pub scope: String,
    pub state: CoverageState,
    pub eligible: State<u64>,
    pub examined: State<u64>,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryRef {
    pub version: String,
    pub digest: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pins {
    pub model: State<String>,
    pub ir: State<String>,
    pub observed: State<String>,
    pub revision: State<String>,
    pub profile: State<String>,
    pub capabilities: State<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Producer {
    pub id: String,
    pub version: String,
    pub artifact_digest: String,
    pub tool: String,
    pub compiler: State<String>,
    pub framework: State<String>,
    pub recipe: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub path: String,
    pub fingerprint: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Span {
    pub id: String,
    pub source: String,
    pub start: u64,
    pub end: u64,
    pub line: u64,
    pub column: u64,
    pub end_line: u64,
    pub end_column: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Route {
    pub identity: String,
    pub confidence: Confidence,
    pub currency: Currency,
    pub origin: Origin,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Step {
    pub role: StepRole,
    pub identity: String,
    pub locations: Vec<String>,
    pub guards: Vec<String>,
    pub confidence: Confidence,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepRole {
    Binding,
    Trigger,
    Registration,
    Callback,
    Effect,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mechanism {
    Direct,
    Observer,
    Hook,
    Magic,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    BindingCandidates,
    Dispatch,
    Convention,
    Effect,
    Path,
    Reflection,
    StringReference,
    Default,
    NativeEdge,
    FieldWrite,
}
/// Each absence is a four-state fact, not an omitted member. `known:false`
/// is a fully inspected absence; unknown is never sufficient for a finding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub kind: Kind,
    pub mechanism: Mechanism,
    pub subject: String,
    pub semantic_symbol: State<String>,
    pub operation: State<String>,
    pub resource: State<String>,
    pub field: State<String>,
    pub native_id: String,
    pub confidence: Confidence,
    pub currency: Currency,
    pub origin: Origin,
    pub claim: Claim,
    pub binding: State<bool>,
    pub configuration: State<bool>,
    pub ownership: State<bool>,
    pub trace: State<bool>,
    pub value: State<String>,
    pub key: State<String>,
    pub candidates: Vec<Route>,
    pub activation: Vec<Step>,
    pub locations: Vec<String>,
    pub guards: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub schema_version: String,
    pub identity: String,
    pub target: String,
    pub scope: Vec<String>,
    pub producer: Producer,
    pub pins: Pins,
    pub input_manifest_digest: String,
    pub sources: Vec<Source>,
    pub locations: Vec<Span>,
    pub coverage: Vec<Coverage>,
    pub records: Vec<Record>,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub enabled: bool,
    pub severity: State<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Thresholds {
    pub semantic_dependency_depth: State<u64>,
    pub native_call_depth: State<u64>,
    pub uncovered_writer_groups: State<u64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Gate {
    pub minimum_confidence: Confidence,
    pub fail_on_active_warnings: bool,
    pub required_coverage: Vec<String>,
    pub require_comparable_baseline: bool,
    pub fail_on_regression: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub recipe: String,
    pub rules: Vec<Rule>,
    pub thresholds: Thresholds,
    pub gate: Gate,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelatedWriters {
    pub resource: String,
    pub field: String,
    pub operations: Vec<String>,
    pub owner: String,
    pub reason: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermittedDefault {
    pub subject: String,
    pub key: String,
    pub targets: Vec<String>,
    pub reason: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub schema_version: String,
    pub identity: String,
    pub registry_ref: RegistryRef,
    pub profiles: Vec<Profile>,
    pub related_writer_groups: Vec<RelatedWriters>,
    pub permitted_defaults: Vec<PermittedDefault>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Waiver {
    pub id: String,
    pub rule_id: String,
    pub subject: String,
    pub target: String,
    pub condition_digest: String,
    pub owner: String,
    pub reason: String,
    pub expires_on: State<String>,
    pub source_digest: State<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Waivers {
    pub schema_version: String,
    pub identity: String,
    pub config_ref: String,
    pub entries: Vec<Waiver>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaiverAudit {
    pub id: String,
    pub disposition: String,
    pub finding: State<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    pub id: String,
    pub rule_id: String,
    pub code: String,
    pub subject: String,
    pub semantic_symbol: State<String>,
    pub target: String,
    pub scope: Vec<String>,
    pub confidence: Confidence,
    pub claim: Claim,
    pub condition_digest: String,
    pub evidence: Vec<String>,
    pub locations: Vec<String>,
    pub witness: Vec<String>,
    pub guards: Vec<String>,
    pub disposition: String,
    pub waiver: State<String>,
    pub alternative: String,
    pub message: String,
    pub severity: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metric {
    pub rule: String,
    pub confidence: Confidence,
    pub raw: u64,
    pub active: u64,
    pub waived: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Depth {
    pub dimension: String,
    pub target: String,
    pub maximum: State<u64>,
    pub recursive_components: u64,
    pub witness: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Summary {
    pub raw: u64,
    pub active: u64,
    pub waived: u64,
    pub possible_effects: u64,
    pub verified_effects: u64,
    pub ambiguity_sets: u64,
    pub uncovered_writer_groups: State<u64>,
    pub diagnostic_projection_truncated: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Delta {
    pub rule: String,
    pub confidence: Confidence,
    pub raw: i64,
    pub active: i64,
    pub waived: i64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DepthDelta {
    pub dimension: String,
    pub target: String,
    pub before: u64,
    pub after: u64,
    pub change: i64,
    pub recursive_components: i64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Comparison {
    pub schema_version: String,
    pub identity: String,
    pub baseline_ref: String,
    pub candidate_ref: String,
    pub comparable: bool,
    pub reasons: Vec<String>,
    pub deltas: Vec<Delta>,
    pub depth_deltas: Vec<DepthDelta>,
    pub uncovered_writer_groups_delta: State<i64>,
    pub new_findings: Vec<String>,
    pub resolved_findings: Vec<String>,
    pub retained_findings: Vec<String>,
    pub regression: State<bool>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub schema_version: String,
    pub identity: String,
    pub registry_ref: RegistryRef,
    pub model_ref: String,
    pub ir_ref: String,
    pub config_ref: String,
    pub profile_ref: String,
    pub waiver_ref: State<String>,
    pub evidence_refs: Vec<String>,
    pub attachment_refs: Vec<String>,
    pub scope: Vec<String>,
    pub profile: String,
    pub mode: String,
    pub as_of: State<String>,
    pub recipes: Vec<String>,
    pub coverage: Vec<Coverage>,
    pub findings: Vec<Finding>,
    pub summary: Summary,
    pub metrics: Vec<Metric>,
    pub depths: Vec<Depth>,
    pub waiver_audit: Vec<WaiverAudit>,
    pub comparison: State<Comparison>,
}
