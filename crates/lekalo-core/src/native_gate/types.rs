//! The closed typed values of the native gate contracts (issue #48).
//!
//! Every struct mirrors one JSON Schema document exactly
//! (`contracts/native-gate-{plan,policy,run,view}.schema.v0.3.2.json`);
//! decoding is strict (unknown members, duplicate keys, and explicit
//! nulls are refused) and every bound is enforced here as well.

use serde::{Deserialize, Serialize};

/// One bounded workspace package record of a native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePackage {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub root: String,
    pub manifest_digest: String,
}

/// One typed workspace edge: consumer -> local dependency.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEdge {
    pub from: String,
    pub to: String,
    pub kind: NativeEdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specifier: Option<String>,
    pub provenance: String,
}

/// The closed edge-kind set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeEdgeKind {
    Dependency,
    DevDependency,
    OptionalDependency,
    PeerDependency,
    TsReference,
    ScannerReference,
    TargetBinding,
}

/// One workspace uncertainty: recorded, never silently dropped.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeUncertainty {
    pub kind: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
}

/// The workspace section of one native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWorkspace {
    pub manager: String,
    pub declared_version: String,
    pub compatibility_path: String,
    pub root: String,
    pub manifest_digest: String,
    pub lock_digest_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_digest: Option<String>,
    pub packages: Vec<NativePackage>,
    pub edges: Vec<NativeEdge>,
    pub completeness: String,
    pub uncertainties: Vec<NativeUncertainty>,
}

/// One changed-file row of a native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFileChange {
    pub path: String,
    pub change: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_digest: Option<String>,
}

/// The changed inputs of a native plan.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeChanges {
    pub files: Vec<NativeFileChange>,
    pub symbols: Vec<String>,
}

/// One affected-package entry with its bounded reason list.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAffected {
    pub package_id: String,
    pub reasons: Vec<NativeReason>,
}

/// One selection reason with its bounded explanation path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeReason {
    pub kind: String,
    pub source_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_path: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_rule_ref: Option<String>,
}

/// One excluded package and the stable reason.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExcluded {
    pub package_id: String,
    pub reason: String,
}

/// The whole-run limits of a native plan or command.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeLimits {
    pub timeout_ms_per_command: u64,
    pub timeout_ms_per_run: u64,
    pub max_stdout_bytes: u64,
    pub max_stderr_bytes: u64,
    pub max_output_bytes_per_run: u64,
}

/// The write policy: stage-only, or bounded named output scopes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWritePolicy {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<NativeWriteScope>>,
}

/// One bounded output scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWriteScope {
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_files: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

/// One proposed gate command of a native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCommand {
    pub id: String,
    pub package_id: String,
    pub gate: String,
    pub script_name: String,
    pub script_digest: String,
    pub confirmation_ref: String,
    pub cwd: String,
    pub tool_ref: String,
    pub argv: Vec<String>,
    pub env: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tsconfig_ref: Option<String>,
    pub depends_on: Vec<String>,
    pub affected_reason_refs: Vec<String>,
    pub read_manifest_ref: String,
    pub allowed_writes: NativeWritePolicy,
    pub limits: NativeLimits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// One env binding: a nonsecret literal or a typed relocation token.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEnvBinding {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// The env section of a native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEnv {
    pub allowed_names: Vec<String>,
    pub bindings: Vec<NativeEnvBinding>,
}

/// One tool entry of a native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTool {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub artifact_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_digest: Option<String>,
    pub platform: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// The run eligibility of a native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRunEligibility {
    pub state: String,
    pub reason_codes: Vec<String>,
}

/// The proposed immutable native gate plan (issue #48).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlan {
    pub schema_version: String,
    pub kind: String,
    pub plan_digest: String,
    pub adapter: NativeAdapterIdentity,
    pub planner_version: String,
    pub canonicalization_version: String,
    pub repository_role: String,
    pub trust: NativeTrust,
    pub authority_ref: NativeRef,
    pub policy_ref: NativeRef,
    pub classification_ref: NativeRef,
    pub execution_policy_ref: NativeRef,
    pub profile_ref: NativeProfileRef,
    pub input_manifest_digest: String,
    pub scan_ref: NativeRef,
    pub observed_ref: NativeRef,
    pub tool_catalog_digest: String,
    pub capability_snapshot_digest: String,
    pub workspace: NativeWorkspace,
    pub changes: NativeChanges,
    pub affected: Vec<NativeAffected>,
    pub excluded: Vec<NativeExcluded>,
    pub selection_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_rule_ref: Option<String>,
    pub commands: Vec<NativeCommand>,
    pub env: NativeEnv,
    pub tools: Vec<NativeTool>,
    pub required_capabilities: Vec<String>,
    pub capabilities: Vec<NativeCapabilityState>,
    pub run_eligibility: NativeRunEligibility,
    pub limits: NativeLimits,
    pub write_policy: NativeWritePolicy,
}

/// The adapter identity block of a native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAdapterIdentity {
    pub id: String,
    pub version: String,
    pub digest: String,
}

/// The trust block: mode plus optional fixture/provenance custody.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTrust {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// One exact three-part reference (id, version, digest). The id member
/// key varies by contract family (contractId/policyId/id); the raw
/// member name is preserved so the canonical bytes round-trip exactly.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(from = "serde_json::Value", into = "serde_json::Value")]
pub struct NativeRef {
    /// The key the reference used for its identity member.
    pub id_key: String,
    pub id: String,
    pub version: String,
    pub digest: String,
}

impl From<serde_json::Value> for NativeRef {
    fn from(value: serde_json::Value) -> Self {
        let object = match value {
            serde_json::Value::Object(object) => object,
            _ => panic!("a native reference must be an object"),
        };
        let id_key = ["contractId", "policyId", "profileId", "id"]
            .iter()
            .find(|key| object.get(**key).is_some())
            .expect("the reference carries an identity member")
            .to_string();
        Self {
            id_key: id_key.clone(),
            id: object
                .get(id_key.as_str())
                .and_then(|value| value.as_str())
                .expect("the identity member is a string")
                .to_owned(),
            version: object
                .get("version")
                .and_then(|value| value.as_str())
                .expect("the reference carries version")
                .to_owned(),
            digest: object
                .get("digest")
                .and_then(|value| value.as_str())
                .expect("the reference carries digest")
                .to_owned(),
        }
    }
}

impl From<NativeRef> for serde_json::Value {
    fn from(reference: NativeRef) -> Self {
        serde_json::json!({
            reference.id_key: reference.id,
            "version": reference.version,
            "digest": reference.digest,
        })
    }
}

/// The profile reference of a native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProfileRef {
    pub id: String,
    pub digest: String,
}

/// One capability state carried by a native plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCapabilityState {
    pub id: String,
    pub definition_version: String,
    pub state: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_ref: Option<String>,
}

/// One authorizing approval of a run request: outside the hashed plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeApproval {
    pub mode: String,
    pub subject_ref: String,
    pub policy_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_ref: Option<String>,
}

/// The run request: names one exact approved plan digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRunRequest {
    pub schema_version: String,
    pub kind: String,
    pub plan_ref: String,
    pub approved_plan_digest: String,
    pub approval: NativeApproval,
    pub fixture_catalog_ref: String,
}

/// One measured value with the #120 valueState vocabulary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeValueState {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<u64>,
}

/// The output reference of one command result: a redacted artifact
/// digest, or an explicit withheld/unknown state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NativeOutputRef {
    /// The digest of the redacted output artifact.
    Digest(String),
    /// An explicit withheld/unknown state.
    State { state: String },
}

/// One per-command result of a run receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCommandResult {
    pub command_id: String,
    pub package_id: String,
    pub cwd: String,
    pub tool_ref: String,
    pub argv: Vec<String>,
    pub env_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_recipe_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<NativeValueState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<NativeValueState>,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_ref: Option<NativeOutputRef>,
}

/// One created/modified path of a mutation summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMutation {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// The mutation summary of a run receipt.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMutationSummary {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created: Vec<NativeMutation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified: Vec<NativeMutation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unexpected: Vec<String>,
}

/// The original-workspace verification result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeOriginalVerification {
    pub state: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutated_paths: Vec<String>,
}

/// The cleanup outcome.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCleanup {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The capability evidence of a run receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCapabilityEvidence {
    pub network_denial: String,
    pub process_containment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_digest: Option<String>,
}

/// The terminal run receipt: exactly one outcome, per-command results,
/// mutation/original/cleanup sections, and capability evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRunResult {
    pub schema_version: String,
    pub kind: String,
    pub plan_digest: String,
    pub execution_policy_ref: NativeRef,
    pub authority_ref: NativeRef,
    pub policy_ref: NativeRef,
    pub outcome: String,
    pub reason_codes: Vec<String>,
    pub commands: Vec<NativeCommandResult>,
    pub mutation_summary: NativeMutationSummary,
    pub original_verification: NativeOriginalVerification,
    pub cleanup: NativeCleanup,
    pub capability_evidence: NativeCapabilityEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}
