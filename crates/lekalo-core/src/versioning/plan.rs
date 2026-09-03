//! The pure migration planner: snapshots, byte edits, semantic diff, and
//! deterministic plan identity (issue #9).
//!
//! Planning executes every one-step transform in memory in chain order and
//! validates each step's output with that target version's decoder before
//! the next step runs. A plan never writes: dry-run reads and computes only,
//! and [`PreparedMigration`] carries the pinned inputs the transactional
//! writer consumes. `planId` is SHA-256 over the registry version, the
//! ordered edge identities, and the sorted logical paths with their
//! before/after digests — never over absolute paths or timestamps — so
//! repeated identical snapshots produce byte-identical plans.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::ir::CompiledProject;
use crate::loader::error::Span;
use crate::loader::{DocKind, ModelVersion, NormalizedModel};

use super::family::ModelContract;
use super::registry::VersionRegistry;
use super::version::ContractVersion;

/// One pinned source document: logical path, exact original bytes, the
/// literal version, and the parsed byte span of the version token.
#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    /// Logical project-relative POSIX path.
    pub path: String,
    /// Which canonical home the document came from.
    pub kind: DocKind,
    /// The literal `schema_version` value as parsed.
    pub version: String,
    pub version_span: Span,
    /// The exact original document bytes.
    pub bytes: Vec<u8>,
}

impl DocumentSnapshot {
    /// Decode these bytes exactly as the loader would.
    pub(crate) fn decode(
        &self,
    ) -> Result<crate::loader::Document, Vec<crate::loader::error::Diagnostic>> {
        crate::loader::decode_document_bytes(&self.path, self.kind, &self.bytes)
    }
}
/// The typed semantic change set between two compiled projects.
///
/// Contract envelope versions and source spans are excluded from payload
/// comparison; the Model contract change is reported separately. Every list
/// is sorted by byte order so the diff is deterministic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SemanticDiff {
    /// Semantic IDs that exist only in the target model.
    pub added: Vec<String>,
    /// Semantic IDs that exist only in the source model.
    pub removed: Vec<String>,
    /// Semantic IDs present in both whose payload changed.
    pub changed: Vec<String>,
    /// Modules affected by the change, sorted.
    #[serde(rename = "affectedModuleIds")]
    pub affected_module_ids: Vec<String>,
    /// The Model contract version change, when the versions differ.
    #[serde(rename = "contractVersionChange")]
    pub contract_version_change: Option<VersionChange>,
}

/// The Model contract version change reported beside the payload diff.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VersionChange {
    /// The source contract version.
    pub from: String,
    /// The target contract version.
    pub to: String,
}

/// Compare two compiled projects semantically.
pub fn diff_projects(before: &CompiledProject, after: &CompiledProject) -> SemanticDiff {
    let mut added: Vec<String> = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    let mut changed: Vec<String> = Vec::new();
    let mut affected: Vec<String> = Vec::new();
    let module_of = |id: &str| id.split('.').next().unwrap_or_default().to_owned();

    // Project payload.
    match (&before.project, &after.project) {
        (Some(before_project), Some(after_project)) => {
            if before_project != after_project {
                changed.push(before_project.id.as_str().to_owned());
                affected.push(before_project.id.as_str().to_owned());
            }
        }
        (Some(before_project), None) => removed.push(before_project.id.as_str().to_owned()),
        (None, Some(after_project)) => added.push(after_project.id.as_str().to_owned()),
        (None, None) => {}
    }

    // Modules by ID.
    for module in &before.modules {
        match after
            .modules
            .iter()
            .find(|candidate| candidate.id == module.id)
        {
            None => {
                removed.push(module.id.as_str().to_owned());
                affected.push(module.id.as_str().to_owned());
            }
            Some(matched) if matched != module => {
                changed.push(module.id.as_str().to_owned());
                affected.push(module.id.as_str().to_owned());
            }
            Some(_) => {}
        }
    }
    for module in &after.modules {
        if !before
            .modules
            .iter()
            .any(|candidate| candidate.id == module.id)
        {
            added.push(module.id.as_str().to_owned());
            affected.push(module.id.as_str().to_owned());
        }
    }

    // Symbol definitions by ID; the module qualifier owns the impact.
    for definition in &before.definitions {
        let id = definition.id().as_str();
        match after
            .definitions
            .iter()
            .find(|candidate| candidate.id() == definition.id())
        {
            None => {
                removed.push(id.to_owned());
                affected.push(module_of(id));
            }
            Some(matched) if matched != definition => {
                changed.push(id.to_owned());
                affected.push(module_of(id));
            }
            Some(_) => {}
        }
    }
    for definition in &after.definitions {
        let id = definition.id().as_str();
        if !before
            .definitions
            .iter()
            .any(|candidate| candidate.id() == definition.id())
        {
            added.push(id.to_owned());
            affected.push(module_of(id));
        }
    }

    added.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    removed.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    changed.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    affected.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    affected.dedup();

    let contract_version_change = if before.model_version == after.model_version {
        None
    } else {
        Some(VersionChange {
            from: before.model_version.as_str().to_owned(),
            to: after.model_version.as_str().to_owned(),
        })
    };

    SemanticDiff {
        added,
        removed,
        changed,
        affected_module_ids: affected,
        contract_version_change,
    }
}

/// One planned file change: identity and sizes only; the raw before/after
/// bytes stay private to [`PreparedMigration`] and never enter a report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, serde::Deserialize)]
pub struct PlanFile {
    /// Logical project-relative POSIX path.
    pub path: String,
    /// SHA-256 of the original bytes, lowercase hex.
    #[serde(rename = "beforeSha256")]
    pub before_sha256: String,
    /// SHA-256 of the planned bytes, lowercase hex.
    #[serde(rename = "afterSha256")]
    pub after_sha256: String,
    /// Original byte length.
    #[serde(rename = "beforeBytes")]
    pub before_bytes: usize,
    /// Planned byte length.
    #[serde(rename = "afterBytes")]
    pub after_bytes: usize,
}

/// The immutable, serializable migration plan view.
///
/// Field order is normative wire order; success envelopes begin
/// `status`/`operation`/`mode` (added by the receipt) and continue with
/// exactly these fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MigrationPlanView {
    /// The deterministic plan identity (`planId`).
    #[serde(rename = "planId")]
    pub plan_id: String,
    /// The contract family; always `model` today.
    pub family: &'static str,
    /// The source contract version.
    pub from: String,
    /// The target contract version.
    pub to: String,
    /// The ordered migration edge identities.
    pub chain: Vec<String>,
    /// Whether any file changes; already-at-target plans are `false`.
    pub changed: bool,
    /// The planned per-file changes, sorted by logical path.
    pub files: Vec<PlanFile>,
    /// The typed semantic change set.
    #[serde(rename = "semanticDiff")]
    pub semantic_diff: SemanticDiff,
    /// The aggregated declared losses of the executed chain.
    pub loss: Vec<String>,
    /// The registry artifact version the plan was computed against.
    #[serde(rename = "registryVersion")]
    pub registry_version: String,
}

/// A completed plan ready for `apply`, or already rendered for `--dry-run`.
///
/// Created only by [`super::migration::MigrationService::plan`]; fields are
/// private, the type is not `Deserialize`, and `apply` consumes it, so a
/// caller can never forge or replay a plan the planner did not produce.
pub struct PreparedMigration {
    pub(crate) source_version: ModelVersion,
    pub(crate) target_version: ModelVersion,
    pub(crate) root: std::path::PathBuf,
    /// Every source document snapshot, sorted by logical path.
    pub(crate) originals: Vec<DocumentSnapshot>,
    /// Only the documents whose planned bytes differ, sorted by path.
    pub(crate) documents: Vec<super::migration::transaction::PlannedDocument>,
    pub(crate) view: MigrationPlanView,
}

impl PreparedMigration {
    /// The immutable plan view (dry-run rendering, receipts, tests).
    pub fn view(&self) -> &MigrationPlanView {
        &self.view
    }
}

/// SHA-256 over the plan identity inputs; lowercase hex.
pub(crate) fn plan_id(registry: &VersionRegistry, chain: &[String], files: &[PlanFile]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(registry.registry_version().as_str().as_bytes());
    hasher.update([0]);
    for id in chain {
        hasher.update(id.as_bytes());
        hasher.update([0]);
    }
    hasher.update([0]);
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update([0]);
        hasher.update(file.before_sha256.as_bytes());
        hasher.update([0]);
        hasher.update(file.after_sha256.as_bytes());
        hasher.update([0]);
    }
    hex(&hasher.finalize())
}

/// Lowercase hex encoding for digests.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0F)] as char);
    }
    out
}

/// SHA-256 of arbitrary bytes as lowercase hex (manifests, receipts).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

/// Render the per-file plan rows from a snapshot/planned pair, sorted by
/// logical path.
pub(crate) fn plan_files(pairs: &[(DocumentSnapshot, Vec<u8>)]) -> Vec<PlanFile> {
    let mut files: Vec<PlanFile> = pairs
        .iter()
        .map(|(before, after)| PlanFile {
            path: before.path.clone(),
            before_sha256: sha256_hex(&before.bytes),
            after_sha256: sha256_hex(after),
            before_bytes: before.bytes.len(),
            after_bytes: after.len(),
        })
        .collect();
    files.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    files
}

/// The normalized source model, compiled for the semantic diff.
pub(crate) fn compile(
    model: &NormalizedModel,
) -> Result<CompiledProject, crate::ir::diagnostic::IrFailure> {
    crate::ir::compile(model).map(|compilation| compilation.project)
}

/// The finite Model version of a registered target version, when the
/// registry entry is one of the finite decodable versions.
pub(crate) fn finite_target(target: &ContractVersion<ModelContract>) -> Option<ModelVersion> {
    match target.as_str() {
        "0.1.0" => Some(ModelVersion::V0_1_0),
        "1.0.0" => Some(ModelVersion::V1_0_0),
        _ => None,
    }
}
