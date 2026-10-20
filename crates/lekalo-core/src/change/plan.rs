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

/// One pinned generation target of the generation plan (round 2,
/// finding 13): the decoded target identity, its exact document digest,
/// and the intended (never executed) action.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct TargetPlan {
    pub target: String,
    pub file: String,
    pub digest: String,
    pub status: &'static str,
    pub action: String,
}

/// The pinned project lock of the generation plan.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct LockPin {
    pub file: String,
    pub digest: String,
}

/// The generation projection of the plan (never executed by `change`):
/// every declared target pinned by its exact document bytes, the project
/// lock pinned when present, and explicit unknown/incomplete states.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct GenerationProjection {
    pub executed: bool,
    pub targets: Vec<TargetPlan>,
    pub lock: Option<LockPin>,
    pub complete: bool,
    pub note: String,
}

/// The sealed prepared change: every field is pinned and deterministic.
#[derive(Clone, Debug)]
pub struct PreparedChange {
    pub(crate) ownership: Option<super::ownership::SealedOwnership>,
    pub(crate) symbol_move: Option<super::symbol_move::Guards>,
    pub(crate) binding: Option<super::binding::SealedConfirmation>,
    pub(crate) module_move: Option<super::module_move::ModuleMove>,
    /// The canonical request the plan was prepared from: the decoded
    /// request re-serialized with sorted object keys and compact
    /// separators, so one request presented as pretty file JSON and as
    /// compact CLI-sugar bytes carries the same identity (the research
    /// contract binds replay to the same normalized request intent).
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
    /// The sealed in-memory prospective bytes per changed logical path —
    /// the composed result of every sequential operation (never
    /// serialized: the wire carries only digests).
    pub(crate) edits: Vec<(String, Vec<u8>)>,
    /// The logical paths whose replacement creates a new canonical kind
    /// home (round 3, finding 11: a module's first definition of a kind).
    /// Never serialized; the apply-time transaction builds `creation`
    /// replacement plans from it.
    pub(crate) creations: Vec<String>,
    /// The sealed generation evidence pins (round 3, finding 7): every
    /// declared target document and the project lock, each pinned by
    /// exact digest (or by the absence sentinel for a missing lock).
    /// Never serialized — the apply-time revalidation under the writer
    /// lock re-reads them; the displayed digests live in the generation
    /// projection.
    pub(crate) generation_pins: Vec<InputPin>,
    /// The resolved project root.
    pub(crate) root: std::path::PathBuf,
    /// Whether every file diff fit inside the bounded plan export.
    pub(crate) diff_complete: bool,
    /// Whether the preparation run carried the explicit breaking-class
    /// acknowledgement (part of the durable execution intent; round 2,
    /// finding 10). Evidence classes are never authorized by it.
    pub(crate) allow_breaking: bool,
}

impl PreparedChange {
    /// The exact `sha256:` digest of the canonical request bytes.
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
        let bytes = serde_json::to_string(&self.wire_content()).unwrap_or_default();
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

    /// The exact bounded file diffs (sorted by logical path).
    pub fn file_diffs(&self) -> &[FileDiff] {
        &self.file_diffs
    }

    /// The planned file replacements (sorted by logical path).
    pub fn file_edits(&self) -> &[FileEdit] {
        &self.file_edits
    }

    /// The exact wire bytes of the plan projection (pretty; the document
    /// carries its own envelope status). The serialized export obeys the
    /// contract's bounded-export ceiling: an over-cap projection refuses
    /// (the typed `PlanExportLimit` error) instead of emitting a plan its
    /// own 32 MiB bound rejects (r6, finding omp F1).
    pub fn to_json_string(&self) -> Result<String, super::diagnostic::ChangeError> {
        let wire = self.wire();
        let json = serde_json::to_string_pretty(&wire)
            .map_err(|_| super::diagnostic::ChangeError::HistoryInvalid { detail: "export" })?;
        if json.len() > version::MAX_PLAN_EXPORT_BYTES {
            return Err(super::diagnostic::ChangeError::PlanExportLimit {
                subject: "plan-bytes",
                measured: json.len() as u64,
                allowed: version::MAX_PLAN_EXPORT_BYTES as u64,
            });
        }
        Ok(json)
    }

    fn wire(&self) -> PlanWire {
        let mut wire = self.wire_content();
        wire.plan_id = self.plan_id();
        wire
    }

    /// The canonical wire content with the plan identity left empty (the
    /// digest input of [`Self::plan_id`]).
    fn wire_content(&self) -> PlanWire {
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
            status: "valid".to_owned(),
            schema_version: version::PLAN_SCHEMA_VERSION.to_owned(),
            identity: version::PLAN_IDENTITY.to_owned(),
            plan_id: String::new(),
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
    status: String,
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    identity: String,
    #[serde(rename = "planId")]
    plan_id: String,
    #[serde(rename = "requestDigest")]
    request_digest: String,
    #[serde(rename = "catalogDigest")]
    catalog_digest: String,
    #[serde(rename = "policyDigest")]
    policy_digest: Option<String>,
    mode: String,
    equal: bool,
    operations: Vec<ResolvedOperation>,
    inputs: Vec<InputPin>,
    #[serde(rename = "fileEdits")]
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
    // Compact JSON can fit the file-byte budget while its single line
    // exceeds the patch-row schema. Keep the exact before/after evidence
    // and digests in the outer plan; never emit an invalid or cut row.
    if before_lines
        .iter()
        .chain(&after_lines)
        .any(|line| line.chars().count() > super::version::MAX_PATCH_LINE_CHARS)
    {
        return (vec![], false);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // r7 review, finding omp F6 (LOW): the fileDiff degradation contract —
    // an over-`MAX_PATCH_LINES` diff marks the plan envelope incomplete
    // with `change.file-diff-limit` instead of presenting a truncated
    // patch as complete — was enumerated in the plan schema but pinned by
    // no test. These unit tests pin the patch-row bound; the integration
    // suite pins the byte bound end to end.
    #[test]
    fn a_bounded_patch_is_complete() {
        let before = "one\ntwo\nthree\n";
        let after = "one\ntwo changed\nthree\nfour\n";
        let (patch, complete) = compute_patch(before, after);
        assert!(complete);
        assert!(patch.len() < super::super::version::MAX_PATCH_LINES);
        assert!(patch.iter().any(|line| line.kind == "insert"));
        assert!(patch.iter().any(|line| line.kind == "delete"));
    }

    #[test]
    fn compact_and_unicode_patch_lines_obey_the_character_bound_without_truncation() {
        for character in ['a', 'я'] {
            let allowed = character
                .to_string()
                .repeat(super::super::version::MAX_PATCH_LINE_CHARS);
            let (patch, complete) = compute_patch("", &allowed);
            assert!(complete);
            assert_eq!(patch[0].text, allowed);
            let (patch, complete) = compute_patch("", &(allowed + &character.to_string()));
            assert!(!complete);
            assert!(patch.is_empty());
        }
    }

    #[test]
    fn an_over_limit_patch_degrades_to_incomplete_instead_of_truncating() {
        // MAX_PATCH_LINES + 1 distinct inserted lines: the patch cannot
        // fit, so the projection reports itself incomplete.
        let before = "";
        let after_lines: Vec<String> = (0..=super::super::version::MAX_PATCH_LINES)
            .map(|index| format!("line-{index}"))
            .collect();
        let after = after_lines.join("\n");
        let (patch, complete) = compute_patch(before, &after);
        assert!(!complete, "an over-limit patch must self-report");
        assert_eq!(patch.len(), super::super::version::MAX_PATCH_LINES);
    }
}
