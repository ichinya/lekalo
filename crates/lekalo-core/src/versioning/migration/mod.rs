//! The migration service: plan, apply, rollback, recovery (issue #9).
//!
//! [`MigrationService::plan`] runs the accepted loader over the selected
//! project (structure validation, capability-safe reads, the shared
//! version gate), resolves the unique declared chain, executes every step
//! in memory, validates each step's output, compiles the final project
//! through the accepted #8 IR, and computes the typed semantic diff. The
//! resulting [`PreparedMigration`] is the only input [`MigrationService::apply`]
//! accepts.
//!
//! An IR transition is never a file rewrite: the target IR is produced by
//! rebuilding from the migrated Model source, and the post-apply check
//! proves the semantic payload is unchanged apart from the contract
//! version.

pub mod transaction;

use serde::Serialize;
use std::fmt;

use crate::ir::CompiledProject;
use crate::loader::{LoadSelection, ModelVersion};

use super::graph::{self, ChainError, MigrationStep, PreconditionViolation, StepFailure};
pub use super::plan::PreparedMigration;
use super::plan::{
    compile, diff_projects, finite_target, plan_files, plan_id as compute_plan_id, sha256_hex,
    DocumentSnapshot, MigrationPlanView, SemanticDiff,
};
use super::reasons;
use super::registry::VersionRegistry;
use super::support::{TargetError, TargetMalformation};
use super::ModelTarget;

/// Why planning failed: either the accepted loader pipeline refused the
/// project (its rich diagnostic envelope is preserved verbatim) or a
/// versioning-specific refusal fired.
pub enum PlanFailure {
    /// The loader's terminal envelope (structure, encoding, versions).
    Loader(crate::loader::LoadOutput),
    /// A versioning refusal.
    Versioning(VersioningFailure),
}

impl From<VersioningFailure> for PlanFailure {
    fn from(failure: VersioningFailure) -> Self {
        Self::Versioning(failure)
    }
}

impl fmt::Debug for PlanFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loader(outcome) => write!(formatter, "Loader({})", outcome.status.as_str()),
            Self::Versioning(failure) => write!(formatter, "Versioning({})", failure.reason_code()),
        }
    }
}

impl PlanFailure {
    /// Render onto the protocol streams deterministically.
    pub fn outcome(&self) -> MigrationOutcome {
        match self {
            Self::Loader(outcome) => MigrationOutcome {
                status: outcome.status.as_str().to_owned(),
                exit_code: outcome.status.exit_code(),
                writes_stderr: outcome.status.writes_stderr(),
                json: format!("{}\n", outcome.json.trim_end_matches('\n')),
                human: format!("{}\n", outcome.human),
            },
            Self::Versioning(failure) => MigrationOutcome::failure(failure),
        }
    }
}

/// Every deterministic refusal the versioning surface can produce.
///
/// The reason code, status, exit class, and stream are fixed per variant;
/// nothing here echoes raw operating-system messages, absolute paths,
/// timestamps, or environment data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VersioningFailure {
    /// The embedded registry violates its own invariants (developer fault).
    RegistryInvalid,
    /// A selector or version spelling is not canonical (`exit 1`).
    InvalidVersion(TargetMalformation),
    /// A well-formed version or target is not registered (`exit 5`).
    Unsupported {
        /// The refused version spelling.
        version: String,
        /// `unregistered` or `retired`.
        state: String,
        /// The declared replacement for a retired version.
        replacement: Option<String>,
    },
    /// The registry declares no migration path (`exit 5`).
    NoMigrationPath {
        /// The source version spelling.
        from: String,
        /// The target version spelling.
        to: String,
    },
    /// The chain is ambiguous — a registry fault (`exit 1`).
    AmbiguousPath,
    /// Representability preconditions failed; owner-authored semantic
    /// edits are required (`exit 1`, zero writes).
    MigrationPrecondition {
        /// Sorted, closed-rule violations.
        violations: Vec<PreconditionViolation>,
    },
    /// A document cannot be rewritten byte-safely (`exit 1`).
    NotFileMigratable {
        /// The logical project-relative path.
        path: String,
        /// The closed detail token.
        detail: &'static str,
    },
    /// Source bytes changed between planning and apply (`exit 1`).
    SourceChanged,
    /// Another migration holds the runtime lock (`exit 1`).
    MigrationInProgress,
    /// Backup or staging creation failed before any source write (`exit 1`).
    BackupFailed,
    /// A source replacement or postcheck failed (`exit 1`).
    CommitFailed,
    /// Rollback refused: the migrated state was edited by someone else
    /// (`exit 1`, zero writes).
    RollbackConflict,
    /// Rollback itself failed after writes began (`exit 1`).
    RollbackFailed,
    /// A crashed transaction demands recovery before new work (`exit 1`).
    RecoveryRequired,
    /// A physical path policy denial inside the transaction home (`exit 1`).
    DeniedPath(String),
    /// An unclassified I/O failure inside a transaction phase (`exit 1`).
    Io {
        /// The logical path the failure occurred on.
        logical: String,
    },
}

impl VersioningFailure {
    /// The stable reason code.
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::RegistryInvalid | Self::AmbiguousPath => reasons::REGISTRY_INVALID,
            Self::InvalidVersion(_) => reasons::INVALID_VERSION,
            Self::Unsupported { .. } => reasons::UNSUPPORTED_VERSION,
            Self::NoMigrationPath { .. } => reasons::NO_MIGRATION_PATH,
            Self::MigrationPrecondition { .. } => reasons::MIGRATION_PRECONDITION,
            Self::NotFileMigratable { .. } => reasons::NOT_FILE_MIGRATABLE,
            Self::SourceChanged => reasons::SOURCE_CHANGED,
            Self::MigrationInProgress => reasons::MIGRATION_IN_PROGRESS,
            Self::BackupFailed => reasons::BACKUP_FAILED,
            Self::CommitFailed => reasons::COMMIT_FAILED,
            Self::RollbackConflict => reasons::ROLLBACK_CONFLICT,
            Self::RollbackFailed => reasons::ROLLBACK_FAILED,
            Self::RecoveryRequired => reasons::RECOVERY_REQUIRED,
            Self::DeniedPath(_) | Self::Io { .. } => reasons::BACKUP_FAILED,
        }
    }

    /// The exit class: versioning refusals are invalid (1) or
    /// unsupported-version (5).
    pub fn exit_code(&self) -> u8 {
        match self.reason_code() {
            reasons::UNSUPPORTED_VERSION | reasons::NO_MIGRATION_PATH => 5,
            _ => 1,
        }
    }

    /// The stable status spelling for the envelope.
    pub fn status(&self) -> &'static str {
        match self.reason_code() {
            reasons::UNSUPPORTED_VERSION | reasons::NO_MIGRATION_PATH => "unsupported-version",
            _ => "invalid",
        }
    }

    /// Map a registry-target refusal.
    pub fn from_target_error(error: TargetError) -> Self {
        match error {
            TargetError::Malformed => Self::InvalidVersion(TargetMalformation::Malformed),
            TargetError::MalformedDetail(detail) => Self::InvalidVersion(detail),
            TargetError::Unsupported(version) => Self::Unsupported {
                version,
                state: "unregistered".to_owned(),
                replacement: None,
            },
        }
    }

    /// Map a graph chain refusal.
    pub(crate) fn from_chain_error(error: ChainError, from: &str, to: &str) -> Self {
        match error {
            ChainError::NoPath => Self::NoMigrationPath {
                from: from.to_owned(),
                to: to.to_owned(),
            },
            ChainError::Ambiguous => Self::AmbiguousPath,
        }
    }

    /// Map a step failure.
    pub(crate) fn from_step_failure(error: StepFailure) -> Self {
        match error {
            StepFailure::Precondition { violations } => Self::MigrationPrecondition { violations },
            StepFailure::NotFileMigratable { path, detail } => Self::NotFileMigratable {
                path,
                detail: detail.as_str(),
            },
        }
    }
}

/// The terminal result of one migrate operation: exact JSON envelope
/// bytes, the stable human line, and the exit class.
pub struct MigrationOutcome {
    /// The stable status spelling (`valid`, `invalid`,
    /// `unsupported-version`, or a loader status).
    pub status: String,
    /// The exit code (0, 1, 3, or 5).
    pub exit_code: u8,
    /// Whether the payload belongs on stderr.
    pub writes_stderr: bool,
    /// The exact JSON envelope (pretty, two-space, one trailing LF).
    pub json: String,
    /// The single stable human line (with trailing LF).
    pub human: String,
}

impl MigrationOutcome {
    /// Render a failure deterministically on stderr.
    pub fn failure(failure: &VersioningFailure) -> Self {
        let json = format!(
            "{{\n  \"status\": \"{}\",\n  \"reasonCodes\": [\n    \"{}\"\n  ]\n}}\n",
            failure.status(),
            failure.reason_code()
        );
        let human = format!("{}: {}\n", failure.status(), failure.reason_code());
        Self {
            status: failure.status().to_owned(),
            exit_code: failure.exit_code(),
            writes_stderr: true,
            json,
            human,
        }
    }

    /// Render any closed success value (a receipt or the compatibility
    /// report) on stdout as pretty two-space JSON plus one LF.
    pub fn success<T: Serialize>(value: &T, human: String) -> Self {
        let json = format!(
            "{}\n",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
        Self {
            status: "valid".to_owned(),
            exit_code: 0,
            writes_stderr: false,
            json,
            human,
        }
    }
}

/// The transaction state of a receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationTransactionState {
    /// Dry-run: computed, nothing written.
    Planned,
    /// Applied and post-verified.
    Committed,
    /// Restored from backups to the recorded before state.
    RolledBack,
    /// The project already sat at the target; nothing to do.
    Unchanged,
}

impl MigrationTransactionState {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Committed => "committed",
            Self::RolledBack => "rolled-back",
            Self::Unchanged => "unchanged",
        }
    }
}

/// The backup summary of a committed or rolled-back receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BackupSummary {
    /// The plan home identity (the plan id, never a path).
    pub id: String,
    /// SHA-256 of the immutable manifest bytes.
    #[serde(rename = "manifestSha256")]
    pub manifest_sha256: String,
}

/// The closed success wire object of every migrate operation. Field order
/// is normative: `status`, `operation`, `mode`, then the plan view fields,
/// then `transaction` and `backup`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MigrationReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `migrate`.
    pub operation: &'static str,
    /// `dry-run`, `apply`, or `rollback`.
    pub mode: &'static str,
    /// The deterministic plan identity.
    #[serde(rename = "planId")]
    pub plan_id: String,
    /// The contract family.
    pub family: &'static str,
    /// The source contract version.
    pub from: String,
    /// The target contract version.
    pub to: String,
    /// The ordered edge identities.
    pub chain: Vec<String>,
    /// Whether any file changed.
    pub changed: bool,
    /// The per-file identities, sorted by logical path.
    pub files: Vec<super::plan::PlanFile>,
    /// The typed semantic change set.
    #[serde(rename = "semanticDiff")]
    pub semantic_diff: SemanticDiff,
    /// The declared losses of the executed chain.
    pub loss: Vec<String>,
    /// The transaction state.
    pub transaction: MigrationTransactionState,
    /// The backup summary, when durable recovery material exists.
    pub backup: Option<BackupSummary>,
}

/// The migration entry point.
pub struct MigrationService;

impl MigrationService {
    /// Plan one migration of the selected project to the resolved target.
    ///
    /// Pure: reads the project through the accepted loader, executes the
    /// chain in memory, and returns the prepared plan. Never writes.
    pub fn plan(
        selection: &LoadSelection,
        target: ModelTarget,
    ) -> Result<PreparedMigration, PlanFailure> {
        let registry = embedded_registry().map_err(PlanFailure::Versioning)?;
        let resolved = target
            .resolve(registry)
            .map_err(VersioningFailure::from_target_error)
            .map_err(PlanFailure::Versioning)?;
        let target_version = finite_target(&resolved).ok_or_else(|| {
            PlanFailure::Versioning(VersioningFailure::NoMigrationPath {
                from: String::new(),
                to: resolved.to_string(),
            })
        })?;

        // Recovery comes first: a crashed earlier run fail-closes the
        // loader gate, so this explicit migrate operation recovers before
        // any new work.
        let root_for_recovery =
            crate::loader::root_for_selection(selection).map_err(PlanFailure::Loader)?;
        recover_pending(&root_for_recovery).map_err(PlanFailure::Versioning)?;

        // The accepted loader pipeline (structure, reads, version gate,
        // support gate) plus the pinned snapshot.
        let loaded = crate::loader::load_with_snapshot(selection).map_err(PlanFailure::Loader)?;
        let source_version = loaded.model.model_version;
        let from = source_version.as_str().to_owned();
        let to = target_version.as_str().to_owned();

        // Pinned source snapshots, sorted by logical path.
        let mut originals: Vec<DocumentSnapshot> = loaded
            .documents
            .iter()
            .map(|loaded| DocumentSnapshot {
                path: loaded.document.path.clone(),
                version: loaded.document.version.clone(),
                version_span: loaded.document.version_span,
                kind: loaded.document.kind,
                bytes: loaded.bytes.clone(),
            })
            .collect();
        originals.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));

        // Already at the target: deterministic changed:false success.
        if source_version == target_version {
            let view = MigrationPlanView {
                plan_id: compute_plan_id(registry, &[], &[]),
                family: "model",
                from: from.clone(),
                to: to.clone(),
                chain: Vec::new(),
                changed: false,
                files: Vec::new(),
                semantic_diff: SemanticDiff {
                    added: Vec::new(),
                    removed: Vec::new(),
                    changed: Vec::new(),
                    affected_module_ids: Vec::new(),
                    contract_version_change: None,
                },
                loss: Vec::new(),
                registry_version: registry.registry_version().to_string(),
            };
            return Ok(PreparedMigration {
                source_version,
                target_version,
                root: loaded.root,
                originals,
                documents: Vec::new(),
                view,
            });
        }

        // The unique declared chain; every edge must have a compiled step.
        let chain =
            graph::model_chain(registry, source_version, target_version).map_err(|error| {
                PlanFailure::Versioning(VersioningFailure::from_chain_error(error, &from, &to))
            })?;
        let chain_ids: Vec<String> = chain.iter().map(|edge| edge.id.clone()).collect();
        let mut steps: Vec<&'static dyn MigrationStep> = Vec::with_capacity(chain.len());
        for edge in &chain {
            let step = graph::catalog()
                .iter()
                .copied()
                .find(|step| step.id() == edge.id)
                .ok_or(PlanFailure::Versioning(VersioningFailure::RegistryInvalid))?;
            steps.push(step);
        }

        // Preflight the first step over the normalized source model, then
        // execute every step in order. Each later step preflights over the
        // rebuilt aggregate of the previous step's output; each step's
        // output documents are decoded with the step's target version.
        if let Some(first) = steps.first() {
            first.preflight(&loaded.model).map_err(|error| {
                PlanFailure::Versioning(VersioningFailure::from_step_failure(error))
            })?;
        }
        let mut current: Vec<(DocumentSnapshot, Vec<u8>)> = originals
            .iter()
            .map(|snapshot| (snapshot.clone(), snapshot.bytes.clone()))
            .collect();
        for (position, step) in steps.iter().enumerate() {
            if position > 0 {
                let model =
                    build_normalized(step.from(), &current).map_err(PlanFailure::Versioning)?;
                step.preflight(&model).map_err(|error| {
                    PlanFailure::Versioning(VersioningFailure::from_step_failure(error))
                })?;
            }
            let mut next: Vec<(DocumentSnapshot, Vec<u8>)> = Vec::with_capacity(current.len());
            for (snapshot, _) in &current {
                let after = step.transform(snapshot).map_err(|error| {
                    PlanFailure::Versioning(VersioningFailure::from_step_failure(error))
                })?;
                let transformed = DocumentSnapshot {
                    path: snapshot.path.clone(),
                    version: step.to().as_str().to_owned(),
                    version_span: snapshot.version_span,
                    kind: snapshot.kind,
                    bytes: after.clone(),
                };
                // Validate the intermediate output with the step target's
                // decoder; a chain never skips its steps.
                transformed.decode().map_err(|_| {
                    PlanFailure::Versioning(VersioningFailure::NotFileMigratable {
                        path: snapshot.path.clone(),
                        detail: "reparse-failed",
                    })
                })?;
                next.push((transformed, after));
            }
            current = next;
        }

        // Compile before and after through the accepted #8 IR and diff.
        let before_compiled = compile(&loaded.model)
            .map_err(|_| PlanFailure::Versioning(VersioningFailure::CommitFailed))?;
        let after_model =
            build_normalized(target_version, &current).map_err(PlanFailure::Versioning)?;
        let after_compiled = compile(&after_model)
            .map_err(|_| PlanFailure::Versioning(VersioningFailure::CommitFailed))?;
        let semantic_diff = diff_projects(&before_compiled, &after_compiled);

        // Only genuinely changed documents enter the transaction; the
        // CAS/backup base is always the pinned ORIGINAL snapshot.
        let mut documents: Vec<transaction::PlannedDocument> = current
            .into_iter()
            .zip(originals.iter())
            .filter(|((transformed, _), original)| transformed.bytes != original.bytes)
            .map(|((_, after), original)| transaction::PlannedDocument {
                snapshot: original.clone(),
                after,
            })
            .collect();
        documents.sort_by(|left, right| {
            left.snapshot
                .path
                .as_bytes()
                .cmp(right.snapshot.path.as_bytes())
        });
        let cas_pairs: Vec<(DocumentSnapshot, Vec<u8>)> = documents
            .iter()
            .map(|document| (document.snapshot.clone(), document.after.clone()))
            .collect();
        let files = plan_files(&cas_pairs);
        let loss: Vec<String> = steps
            .iter()
            .flat_map(|step| step.loss().iter().map(|token| (*token).to_owned()))
            .collect();

        let view = MigrationPlanView {
            plan_id: compute_plan_id(registry, &chain_ids, &files),
            family: "model",
            from,
            to,
            chain: chain_ids,
            changed: !files.is_empty(),
            files,
            semantic_diff,
            loss,
            registry_version: registry.registry_version().to_string(),
        };
        Ok(PreparedMigration {
            source_version,
            target_version,
            root: loaded.root,
            originals,
            documents,
            view,
        })
    }

    /// Render the dry-run receipt of a prepared plan (no transaction).
    pub fn dry_run_receipt(prepared: &PreparedMigration) -> MigrationReceipt {
        receipt_from_view(
            &prepared.view,
            "dry-run",
            MigrationTransactionState::Planned,
            None,
            None,
        )
    }

    /// Apply a prepared plan transactionally; consumes the plan.
    pub fn apply(prepared: PreparedMigration) -> Result<MigrationReceipt, VersioningFailure> {
        embedded_registry()?;
        let write = transaction::MigrationWrite::new(prepared.root.clone());

        // A crashed earlier run recovers before new work; a live lock held
        // by another migration is an in-progress refusal.
        recover_pending(prepared.root.as_path())?;

        // Revalidate the source snapshot before the first replacement.
        for document in &prepared.documents {
            let current = write
                .read_file(&document.snapshot.path)
                .map_err(|_| VersioningFailure::SourceChanged)?;
            if sha256_hex(&current) != sha256_hex(&document.snapshot.bytes) {
                return Err(VersioningFailure::SourceChanged);
            }
        }

        if !prepared.view.changed {
            debug_assert_eq!(prepared.source_version, prepared.target_version);
            return Ok(receipt_from_view(
                &prepared.view,
                "apply",
                MigrationTransactionState::Unchanged,
                None,
                None,
            ));
        }

        let manifest = transaction::BackupManifest {
            registry_version: prepared.view.registry_version.clone(),
            chain: prepared.view.chain.clone(),
            files: prepared.view.files.clone(),
            loss: prepared.view.loss.clone(),
        };
        let plan_id = prepared.view.plan_id.clone();

        transaction::run_apply(&write, &plan_id, &manifest, &prepared.documents)?;

        // Post-apply: reload through the accepted read path, verify the
        // target versions, and prove the planned semantic diff.
        let reloaded = crate::loader::load_validated_root(prepared.root.clone())
            .map_err(|_| VersioningFailure::CommitFailed)?;
        if reloaded.model.model_version.as_str() != prepared.view.to {
            return Err(VersioningFailure::CommitFailed);
        }
        let after_compiled =
            compile(&reloaded.model).map_err(|_| VersioningFailure::CommitFailed)?;
        let before_compiled = compiled_before(&prepared)?;
        let actual_diff = diff_projects(&before_compiled, &after_compiled);
        if actual_diff != prepared.view.semantic_diff {
            return Err(VersioningFailure::CommitFailed);
        }

        let mut receipt = receipt_from_view(
            &prepared.view,
            "apply",
            MigrationTransactionState::Committed,
            None,
            None,
        );
        receipt.backup = Some(BackupSummary {
            id: plan_id,
            manifest_sha256: manifest.digest(),
        });
        Ok(receipt)
    }

    /// Roll back one recorded plan to its verified before state.
    pub fn rollback(
        selection: &LoadSelection,
        plan_id: &str,
    ) -> Result<MigrationReceipt, PlanFailure> {
        let registry = embedded_registry()?;
        if !valid_plan_id(plan_id) {
            return Err(PlanFailure::Versioning(VersioningFailure::InvalidVersion(
                TargetMalformation::Malformed,
            )));
        }
        // Resolve the project root exactly like a load; the shared
        // fail-closed journal gate applies here too.
        let loaded = crate::loader::load_with_snapshot(selection).map_err(PlanFailure::Loader)?;
        let write = transaction::MigrationWrite::new(loaded.root.clone());
        let before_compiled = compile(&loaded.model)
            .map_err(|_| PlanFailure::Versioning(VersioningFailure::CommitFailed))?;

        let manifest =
            transaction::run_rollback(&write, plan_id).map_err(PlanFailure::Versioning)?;

        let reloaded =
            crate::loader::load_validated_root(loaded.root.clone()).map_err(PlanFailure::Loader)?;
        let after_compiled = compile(&reloaded.model)
            .map_err(|_| PlanFailure::Versioning(VersioningFailure::CommitFailed))?;
        let semantic_diff = diff_projects(&before_compiled, &after_compiled);

        let view = MigrationPlanView {
            plan_id: plan_id.to_owned(),
            family: "model",
            from: semantic_diff
                .contract_version_change
                .as_ref()
                .map(|change| change.from.clone())
                .unwrap_or_else(|| reloaded.model.model_version.as_str().to_owned()),
            to: reloaded.model.model_version.as_str().to_owned(),
            chain: manifest.chain.clone(),
            changed: false,
            files: manifest.files.clone(),
            semantic_diff,
            loss: manifest.loss.clone(),
            registry_version: registry.registry_version().to_string(),
        };
        let mut receipt = receipt_from_view(
            &view,
            "rollback",
            MigrationTransactionState::RolledBack,
            None,
            None,
        );
        receipt.backup = Some(BackupSummary {
            id: plan_id.to_owned(),
            manifest_sha256: manifest.digest(),
        });
        Ok(receipt)
    }
}

/// The validated embedded registry or the stable registry fault.
fn embedded_registry() -> Result<&'static VersionRegistry, VersioningFailure> {
    VersionRegistry::embedded().map_err(|_| VersioningFailure::RegistryInvalid)
}

/// Recover any crashed transaction under the project's runtime home before
/// new migration work. A live lock without a journal means another
/// migration is in progress right now and is refused, not recovered.
fn recover_pending(root: &std::path::Path) -> Result<(), VersioningFailure> {
    let write = transaction::MigrationWrite::new(root.to_path_buf());
    if let Some(held) = transaction::read_lock(&write) {
        let journal = format!(
            "{}/{held}/{}",
            transaction::MIGRATIONS_HOME,
            transaction::JOURNAL
        );
        if write.file_exists(&journal) {
            return transaction::run_recovery(&write, &held);
        }
        return Err(VersioningFailure::MigrationInProgress);
    }
    if let Some(plan) = crashed_journal(&write) {
        return transaction::run_recovery(&write, &plan);
    }
    Ok(())
}

/// Build a normalized model from planned in-memory documents.
fn build_normalized(
    version: ModelVersion,
    planned: &[(DocumentSnapshot, Vec<u8>)],
) -> Result<crate::loader::NormalizedModel, VersioningFailure> {
    let documents: Vec<crate::loader::project_docs::Document> = planned
        .iter()
        .map(|(snapshot, bytes)| {
            crate::loader::decode_document_bytes(&snapshot.path, snapshot.kind, bytes)
        })
        .collect::<Result<Vec<_>, Vec<_>>>()
        .map_err(|_| VersioningFailure::CommitFailed)?;
    crate::loader::build_normalized_model(version, documents)
        .map_err(|_| VersioningFailure::CommitFailed)
}

/// The compiled before-model of a prepared plan, recomputed from the
/// pinned original snapshot bytes without touching the disk.
fn compiled_before(prepared: &PreparedMigration) -> Result<CompiledProject, VersioningFailure> {
    let documents: Vec<crate::loader::Document> = prepared
        .originals
        .iter()
        .map(|snapshot| snapshot.decode())
        .collect::<Result<Vec<_>, Vec<_>>>()
        .map_err(|_| VersioningFailure::SourceChanged)?;
    let model = crate::loader::build_normalized_model(prepared.source_version, documents)
        .map_err(|_| VersioningFailure::CommitFailed)?;
    compile(&model).map_err(|_| VersioningFailure::CommitFailed)
}

/// A plan home with a journal but no live lock holder (crash between the
/// lock loss window and journal removal is impossible, but the lock file
/// removal is the last write, so read it defensively).
fn crashed_journal(write: &transaction::MigrationWrite) -> Option<String> {
    for plan in write.plan_directories() {
        let journal = format!(
            "{}/{plan}/{}",
            transaction::MIGRATIONS_HOME,
            transaction::JOURNAL
        );
        if write.file_exists(&journal) {
            return Some(plan);
        }
    }
    None
}

/// Build the success receipt from a plan view.
fn receipt_from_view(
    view: &MigrationPlanView,
    mode: &'static str,
    transaction_state: MigrationTransactionState,
    from_override: Option<String>,
    semantic_override: Option<SemanticDiff>,
) -> MigrationReceipt {
    MigrationReceipt {
        status: "valid",
        operation: "migrate",
        mode,
        plan_id: view.plan_id.clone(),
        family: view.family,
        from: from_override.unwrap_or_else(|| view.from.clone()),
        to: view.to.clone(),
        chain: view.chain.clone(),
        changed: view.changed,
        files: view.files.clone(),
        semantic_diff: semantic_override.unwrap_or_else(|| view.semantic_diff.clone()),
        loss: view.loss.clone(),
        transaction: transaction_state,
        backup: None,
    }
}

/// Whether a rollback selector is a well-formed plan identity (64 hex).
fn valid_plan_id(plan_id: &str) -> bool {
    plan_id.len() == 64
        && plan_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
