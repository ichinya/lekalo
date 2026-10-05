//! Closed governance DTOs. The store is the successor of AI-lint waivers.
pub use crate::ai_lint::wire::{RegistryRef, State};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectorKind {
    Rule,
    Capability,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScopeKind {
    Project,
    Module,
    Symbol,
    Path,
    Target,
    Profile,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Risk {
    Correctness,
    Compatibility,
    Security,
    DataLoss,
    CapabilityGap,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    Active,
    Revoked,
    Superseded,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selector {
    pub kind: SelectorKind,
    pub id: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub kind: ScopeKind,
    pub id: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileRef {
    pub id: String,
    pub version: String,
    pub digest: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fingerprint {
    pub model: State<String>,
    pub ir: State<String>,
    pub adapter: State<String>,
    pub revision: State<String>,
    pub capabilities: State<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Approval {
    pub id: String,
    pub subject_digest: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRef {
    pub kind: String,
    pub id: String,
    pub digest: State<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub finding_id: String,
    pub fact_digest: String,
    pub selector: Selector,
    pub scope: Scope,
    pub subject: String,
    pub target: String,
    pub profile_ref: ProfileRef,
    pub condition_digest: String,
    pub owner: String,
    pub approver: String,
    pub approval_ref: Approval,
    pub reason: String,
    pub created_at: String,
    pub expires_at: State<String>,
    pub review_after: State<String>,
    pub source_ref: SourceRef,
    pub accepted_risk: Risk,
    pub fingerprint: Fingerprint,
    pub lifecycle: Lifecycle,
    pub supersedes: State<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Store {
    pub schema_version: String,
    pub identity: String,
    pub project_id: String,
    pub registry_ref: RegistryRef,
    pub entries: Vec<Entry>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fact {
    pub id: String,
    pub selector: Selector,
    pub subject: String,
    pub target: String,
    pub symbol: State<String>,
    pub module: State<String>,
    pub path: State<String>,
    pub condition_digest: String,
    pub fingerprint: Fingerprint,
    pub source_outcome: String,
    pub source_severity: State<String>,
    pub source_confidence: State<String>,
    pub evidence_refs: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub schema_version: String,
    pub identity: String,
    pub project_id: String,
    pub profile_ref: ProfileRef,
    pub lock_ref: State<String>,
    pub complete: bool,
    pub facts: Vec<Fact>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditEntry {
    pub id: String,
    pub status: String,
    pub effective: bool,
    pub deadline: String,
    pub reason_codes: Vec<String>,
    pub mismatched_pins: Vec<String>,
    pub finding: State<String>,
    pub entry_digest: String,
    pub approval_ref: Approval,
    pub supersedes: State<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Disposition {
    pub fact: Fact,
    pub fact_digest: String,
    pub severity: State<String>,
    pub original_gate: String,
    pub effective_gate: String,
    pub waiver: State<String>,
    pub waiver_entry_digest: State<String>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Summary {
    pub raw: u64,
    pub active: u64,
    pub waived: u64,
    pub active_waivers: u64,
    pub expiring: u64,
    pub expired: u64,
    pub stale: u64,
    pub non_waivable: u64,
    pub unverifiable: u64,
    pub orphan: u64,
    pub unexamined: u64,
    pub revoked: u64,
    pub superseded: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub id: String,
    pub kind: String,
    pub before: State<String>,
    pub after: State<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Audit {
    pub schema_version: String,
    pub identity: String,
    pub project_id: String,
    pub registry_ref: RegistryRef,
    pub profile_ref: ProfileRef,
    pub store_digest: String,
    pub input_digest: String,
    pub lock_ref: State<String>,
    pub as_of: String,
    pub expiring_window_seconds: u64,
    pub entries: Vec<AuditEntry>,
    pub findings: Vec<Disposition>,
    pub summary: Summary,
    pub base_store_digest: State<String>,
    pub changes: Vec<Change>,
    pub denied: bool,
    pub done_digest: String,
}
