//! Closed provider-neutral evidence input and assessment output (issue #35).

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::trace::{ContractRef, ExternalSystem};

pub const EVIDENCE_SCHEMA: &str = "lekalo/trace-validation-evidence/v0.6.4";
pub const EVIDENCE_IDENTITY: &str = "dev.lekalo.trace-validation-evidence@0.6.4";
pub const REPORT_SCHEMA: &str = "lekalo/trace-assessment/v0.6.4";
pub const REPORT_IDENTITY: &str = "dev.lekalo.trace-assessment@0.6.4";
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ROWS: usize = 10_000;
pub const MAX_EVIDENCE: usize = 2_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Pin {
    pub id: String,
    pub version: String,
    pub digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderPin {
    pub provider: ExternalSystem,
    pub tool: Pin,
    pub protocol: Pin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Policy {
    Optional,
    Required,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Scope {
    pub requirements: Vec<String>,
    pub artifacts: Vec<String>,
    pub scenarios: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Chain {
    pub id: String,
    pub occurrence: String,
    pub requirement: String,
    pub symbol: String,
    pub artifact: String,
    pub scenario: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    pub relation_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Pass,
    Warn,
    Fail,
    Unavailable,
    Unsupported,
    Infrastructure,
    Configuration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    Check,
    Execution,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OriginalSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OriginalDiagnostic {
    pub code: String,
    pub severity: OriginalSeverity,
    pub subject: String,
}

/// A receipt supplied by the orchestration owner. No provider-native wire,
/// paths, command text, source bodies, timestamps or raw streams are accepted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Evidence {
    pub id: String,
    pub provider: ExternalSystem,
    pub kind: EvidenceKind,
    pub outcome: Outcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Pin>,
    pub protocol: Pin,
    pub source_revision: String,
    pub model_ref: ContractRef,
    pub working_set_digest: String,
    pub result_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_digest: Option<String>,
    pub diagnostics: Vec<OriginalDiagnostic>,
}

/// Current input pins and mapping are selected explicitly by the external
/// owner. The service compares them to the supplied trace and every receipt;
/// it does not assert that it inspected Git or a live project itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvidenceDocument {
    pub schema_version: String,
    pub identity: String,
    pub project_ref: String,
    pub source_revision: String,
    pub model_ref: ContractRef,
    pub working_set_digest: String,
    pub trace_digest: String,
    pub mapping_digest: String,
    pub policy: Policy,
    pub required_providers: Vec<ExternalSystem>,
    pub provider_pins: Vec<ProviderPin>,
    pub require_execution: bool,
    pub scope: Scope,
    pub chains: Vec<Chain>,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Coverage {
    Complete,
    Partial,
    Conflicting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Execution {
    Passed,
    Failed,
    Unverified,
    NotRequested,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Ready,
    Degraded,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub rule: String,
    pub subject: String,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainReport {
    pub chain: Chain,
    pub coverage: Coverage,
    pub execution: Execution,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentReport {
    pub schema_version: &'static str,
    pub identity: &'static str,
    pub project_ref: String,
    pub source_revision: String,
    pub model_ref: ContractRef,
    pub working_set_digest: String,
    pub trace_digest: String,
    pub mapping_digest: String,
    pub evidence_digest: String,
    pub expected_trace_digest: String,
    pub policy: Policy,
    pub required_providers: Vec<ExternalSystem>,
    pub provider_pins: Vec<ProviderPin>,
    pub require_execution: bool,
    pub scope: Scope,
    pub coverage: Coverage,
    pub verdict: Verdict,
    pub chains: Vec<ChainReport>,
    pub evidence: Vec<Evidence>,
    pub findings: Vec<Finding>,
    pub diagnostics: Vec<Diagnostic>,
}
