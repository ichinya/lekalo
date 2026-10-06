//! The sealed prepared change and its bounded wire projection (issue #73).
//!
//! A [`PreparedChange`] is produced only by [`super::prepare`]: it pins the
//! request, catalog, policy, and every input byte by digest, carries the
//! owner-produced semantic diff, the exact bounded file diff, and the impact
//! and generation projections, and summarizes breaking classes. The wire
//! projection is deterministic: no wall clock, process identity, or absolute
//! path enters the bytes, and repeated preparations of the same inputs are
//! byte-identical.

use serde::Serialize;

use super::catalog::Catalog;
use super::edit::ByteEdit;
use super::policy::Policy;
use super::version;
use crate::digest::sha256_hex;

/// The canonical wire order of the compatibility classes (dominance order).
pub const CLASS_ORDER: [&str; 8] = [
    "unknown",
    "data-loss-risk",
    "storage-migration-required",
    "wire-breaking",
    "source-breaking",
    "behavioral",
    "target-specific",
    "additive",
];

/// One resolved operation row of a plan or receipt.
#[derive(Clone, Debug, Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct ResolvedOperation {
    pub operation: String,
    pub subject: String,
    #[serde(rename = "subjectFamily")]
    pub subject_family: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// One pinned input of the complete input inventory.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct InputPin {
    pub path: String,
    pub digest: String,
}

/// One planned file replacement.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct FileEdit {
    pub path: String,
    #[serde(rename = "beforeDigest")]
    pub before_digest: String,
    #[serde(rename = "afterDigest")]
    pub after_digest: String,
}

/// One line of the exact bounded file patch.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct PatchLine {
    pub kind: &'static str,
    #[serde(rename = "beforeLine")]
    pub before_line: usize,
    #[serde(rename = "afterLine")]
    pub after_line: usize,
    pub text: String,
}

/// The exact bounded diff of one edited file.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct FileDiff {
    pub path: String,
    pub before: String,
    pub after: String,
    pub patch: Vec<PatchLine>,
}

/// The semantic-diff summary of the plan.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct SemanticDiffSummary {
    pub equal: bool,
    pub classification: Vec<String>,
    pub changes: Vec<ChangeRecordWire>,
    pub complete: bool,
    #[serde(rename = "completeReason")]
    pub complete_reason: Option<String>,
}

/// One typed change record of the semantic diff.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct ChangeRecordWire {
    #[serde(rename = "changeId")]
    pub change_id: String,
    pub kind: String,
    pub subject: SubjectWire,
    pub reasons: Vec<String>,
}

/// One typed semantic subject.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct SubjectWire {
    pub family: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
}

/// One diff seed that feeds the impact projection.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct SeedEntry {
    pub family: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    pub side: &'static str,
}

/// One affected subject of the impact projection with its reference path.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct AffectedEntry {
    pub subject: String,
    #[serde(rename = "subjectFamily")]
    pub subject_family: String,
    pub direction: &'static str,
    pub via: Vec<String>,
}

/// The impact projection of the plan.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct ImpactProjection {
    pub seeds: Vec<SeedEntry>,
    pub affected: Vec<AffectedEntry>,
    pub complete: bool,
}

/// The generation projection of the plan (never executed by `change`).
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct GenerationProjection {
    pub executed: bool,
    pub targets: Vec<String>,
    pub note: String,
}

/// The sealed prepared change: every field is pinned and deterministic.
#[derive(Clone, Debug)]
pub struct PreparedChange {
    /// The request the plan was prepared from (exact bytes).
    pub(crate) request_bytes: Vec<u8>,
    /// The effective policy.
    pub(crate) policy: Policy,
    /// The plan rows in request order.
    pub(crate) operations: Vec<ResolvedOperation>,
    /// The complete input inventory (sorted by logical path).
    pub(crate) inputs: Vec<InputPin>,
    /// The planned file replacements (sorted by logical path).
    pub(crate) file_edits: Vec<FileEdit>,
    /// The exact bounded file diffs (sorted by logical path).
    pub(crate) file_diffs: Vec<FileDiff>,
    /// The semantic diff summary.
    pub(crate) semantic_diff: SemanticDiffSummary,
    /// The impact projection.
    pub(crate) impact: ImpactProjection,
    /// The generation projection.
    pub(crate) generation: GenerationProjection,
    /// The breaking classes not allowed by the effective policy.
    pub(crate) denied_breaking_classes: Vec<String>,
    /// The invocation mode recorded in the wire projection.
    pub(crate) mode: &'static str,
    /// The project semantic id.
    pub(crate) project_id: String,
    /// The sealed in-memory byte edits per changed logical path (never
    /// serialized: the wire carries only digests).
    pub(crate) edits: Vec<(String, Vec<ByteEdit>)>,
    /// The resolved project root.
    pub(crate) root: std::path::PathBuf,
    /// Whether every file diff fit inside the bounded plan export.
    pub(crate) diff_complete: bool,
}

impl PreparedChange {
    /// The exact `sha256:` digest of the request bytes.
    pub fn request_digest(&self) -> String {
        format!("sha256:{}", sha256_hex(&self.request_bytes))
    }

    /// The exact `sha256:` digest of the embedded catalog bytes.
    pub fn catalog_digest(&self) -> String {
        Catalog::embedded()
            .map(|catalog| catalog.digest().to_owned())
            .unwrap_or_else(|_| "sha256:unavailable".to_owned())
    }

    /// The exact `sha256:` digest of the policy bytes, when a policy applied.
    pub fn policy_digest(&self) -> Option<String> {
        self.policy.digest.clone()
    }

    /// The deterministic plan identity: the digest of the canonical wire
    /// bytes without the `planId` field itself.
    pub fn plan_id(&self) -> String {
        let mut wire = self.wire();
        wire.plan_id = String::new();
        let bytes = serde_json::to_string(&wire).unwrap_or_default();
        format!("sha256:{}", sha256_hex(bytes.as_bytes()))
    }

    /// Whether the candidate is semantically identical to the base.
    pub fn is_no_op(&self) -> bool {
        self.semantic_diff.equal && self.file_edits.is_empty()
    }

    /// The breaking classes the effective policy denies.
    pub fn denied_breaking_classes(&self) -> &[String] {
        &self.denied_breaking_classes
    }

    /// The resolved operation rows in request order.
    pub fn operations(&self) -> &[ResolvedOperation] {
        &self.operations
    }

    /// The exact wire bytes of the plan projection (pretty, no status).
    pub fn to_json_string(&self) -> String {
        let wire = self.wire();
        serde_json::to_string_pretty(&wire).unwrap_or_default()
    }

    fn wire(&self) -> PlanWire {
        let breaking: Vec<String> = self
            .semantic_diff
            .classification
            .iter()
            .filter(|class| {
                super::policy::BREAKING_CLASSES
                    .iter()
                    .any(|candidate| candidate == class)
            })
            .cloned()
            .collect();
        PlanWire {
            schema_version: version::PLAN_SCHEMA_VERSION.to_owned(),
            identity: version::PLAN_IDENTITY.to_owned(),
            plan_id: self.plan_id(),
            request_digest: self.request_digest(),
            catalog_digest: self.catalog_digest(),
            policy_digest: self.policy_digest(),
            mode: self.mode.to_owned(),
            equal: self.semantic_diff.equal && self.file_edits.is_empty(),
            operations: self.operations.clone(),
            inputs: self.inputs.clone(),
            file_edits: self.file_edits.clone(),
            file_diff: self.file_diffs.clone(),
            semantic_diff: self.semantic_diff.clone(),
            impact: self.impact.clone(),
            generation: self.generation.clone(),
            breaking_classes: breaking,
            policy_allowed: self.denied_breaking_classes.is_empty(),
            complete: self.diff_complete,
            complete_reason: if self.diff_complete {
                None
            } else {
                Some("change.file-diff-limit".to_owned())
            },
        }
    }
}

/// The closed wire projection of one plan.
#[derive(Serialize)]
struct PlanWire {
    schema_version: String,
    identity: String,
    plan_id: String,
    request_digest: String,
    catalog_digest: String,
    policy_digest: Option<String>,
    mode: String,
    equal: bool,
    operations: Vec<ResolvedOperation>,
    inputs: Vec<InputPin>,
    file_edits: Vec<FileEdit>,
    #[serde(rename = "fileDiff")]
    file_diff: Vec<FileDiff>,
    #[serde(rename = "semanticDiff")]
    semantic_diff: SemanticDiffSummary,
    impact: ImpactProjection,
    generation: GenerationProjection,
    #[serde(rename = "breakingClasses")]
    breaking_classes: Vec<String>,
    #[serde(rename = "policyAllowed")]
    policy_allowed: bool,
    complete: bool,
    #[serde(rename = "completeReason")]
    complete_reason: Option<String>,
}

/// Compute the exact line-level patch between two document texts. The patch
/// is bounded: over [`super::version::MAX_PATCH_LINES`] rows the caller
/// marks the plan incomplete instead of presenting a truncated diff.
pub(crate) fn compute_patch(before: &str, after: &str) -> (Vec<PatchLine>, bool) {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    // Longest common subsequence over lines; documents are bounded by the
    // loader limits, so the quadratic table stays bounded in practice.
    let n = before_lines.len();
    let m = after_lines.len();
    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if before_lines[i] == after_lines[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j + 1]
                    .max(table[i + 1][j])
                    .max(table[i][j + 1])
            };
        }
    }
    let mut patch = Vec::new();
    let mut complete = true;
    let (mut i, mut j) = (0usize, 0usize);
    while i < n || j < m {
        if patch.len() >= super::version::MAX_PATCH_LINES {
            complete = false;
            break;
        }
        if i < n && j < m && before_lines[i] == after_lines[j] {
            patch.push(PatchLine {
                kind: "equal",
                before_line: i + 1,
                after_line: j + 1,
                text: before_lines[i].to_owned(),
            });
            i += 1;
            j += 1;
        } else if j < m && (i >= n || table[i][j + 1] >= table[i + 1][j]) {
            patch.push(PatchLine {
                kind: "insert",
                before_line: i,
                after_line: j + 1,
                text: after_lines[j].to_owned(),
            });
            j += 1;
        } else {
            patch.push(PatchLine {
                kind: "delete",
                before_line: i + 1,
                after_line: j,
                text: before_lines[i].to_owned(),
            });
            i += 1;
        }
    }
    (patch, complete)
}
