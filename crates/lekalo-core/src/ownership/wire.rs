//! Closed ownership-transition documents. Old managed contracts are unchanged.
use crate::managed::Mode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: &str = "1.0.0";
pub const STATE: &str = "lekalo/ownership-state.json";
pub const HOME: &str = ".lekalo/generated/manifests/ownership-transitions";
pub const MAX_FILES: usize = 512;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Runner {
    pub program: String,
    pub digest: String,
    /// Reviewed argv, executed identically on the baseline and candidate.
    /// `ownership-bindings.json` in each disposable copy selects its bindings.
    pub args: Vec<String>,
    /// A declared, deterministic oracle. No implicit timestamp/UUID stripping.
    pub expected: Vec<Observation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub scenario: String,
    pub output: Value,
    pub error: Value,
    pub state: Value,
    pub effects: Vec<Value>,
    pub authorization: Value,
    pub transaction: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: String,
    pub module: String,
    pub symbol: Option<String>,
    pub to: Mode,
    /// Explicit additional consumed source/test/config files. Model documents,
    /// lock, declarations, bound sources and adapter evidence are derived too.
    pub inputs: Vec<String>,
    pub coverage: Option<String>,
    pub declaration: Option<String>,
    pub unknown_behavior: Vec<String>,
    pub runner: Runner,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub symbol: String,
    pub path: String,
    pub native_symbol: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Claim {
    pub binding: Binding,
    pub digest: String,
    pub lifecycle: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assignment {
    pub module: String,
    pub symbol: Option<String>,
    pub mode: Mode,
    pub transition: String,
    pub artifacts: Vec<Claim>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnershipState {
    pub schema_version: String,
    pub pending: Option<PendingGrant>,
    pub assignments: Vec<Assignment>,
}

/// Source-side grant for an interrupted handoff. Cache alone grants no writes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingGrant {
    pub plan_digest: String,
    pub journal_digest: String,
    pub phase: String,
    pub base_digest: Option<String>,
}

impl Default for OwnershipState {
    fn default() -> Self {
        Self {
            schema_version: format!("lekalo/ownership-state/v{VERSION}"),
            pending: None,
            assignments: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pin {
    pub path: String,
    pub digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edit {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub before_content: Option<String>,
    /// Complete preview bytes; snapshots are retained privately at commit.
    pub content: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionPlan {
    pub schema_version: String,
    pub plan_digest: String,
    pub project: String,
    pub request: Request,
    pub from: Mode,
    pub inputs: Vec<Pin>,
    pub baseline: Vec<Binding>,
    pub candidate: Vec<Binding>,
    pub edits: Vec<Edit>,
    pub claims: Vec<Claim>,
    pub semantic_diff: Vec<SemanticChange>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticChange {
    pub kind: String,
    pub subject: String,
    pub from: Mode,
    pub to: Mode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Verification {
    pub schema_version: String,
    pub plan_digest: String,
    pub runner_digest: String,
    pub verdict: String,
    pub observations: Vec<ObservationEvidence>,
    pub counterexample: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservationEvidence {
    pub scenario: String,
    pub baseline_digest: Option<String>,
    pub candidate_digest: Option<String>,
    pub expected_digest: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub schema_version: String,
    pub plan_digest: String,
    pub project: String,
    pub state: String,
    pub module: String,
    pub symbol: Option<String>,
    pub from: Mode,
    pub to: Mode,
    pub verification: Verification,
    pub files: Vec<Pin>,
    pub rollback: String,
    pub previous_receipt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModeStatus {
    pub module: String,
    pub mode: Mode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnershipStatus {
    pub schema_version: String,
    pub project: String,
    pub evidence_basis: String,
    pub legacy: Vec<ModeStatus>,
    pub assignments: Vec<Assignment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnershipTrace {
    pub schema_version: String,
    pub authority: String,
    pub project: String,
    pub plan_digest: String,
    pub request_digest: String,
    pub subject: String,
    pub from: Mode,
    pub to: Mode,
    pub state: String,
    pub current: bool,
    pub semantic_diff: Vec<SemanticChange>,
    pub inputs: Vec<Pin>,
    pub verification: Verification,
    pub files: Vec<Pin>,
    pub rollback: String,
    pub receipt_digest: String,
    pub previous_receipt: Option<String>,
}
