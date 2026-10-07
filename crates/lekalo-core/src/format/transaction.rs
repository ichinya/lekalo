//! The capability-safe formatter transaction (issue #94).
//!
//! Mirrors the accepted #9 migration guarantees with a formatter-owned
//! runtime home: only admitted Model documents the loader enumerated may
//! be replaced, only through this module. Every physical path component
//! is re-validated and reparse points are refused on sight, preimage
//! digests are verified at every boundary, and a durable journal makes a
//! crash fail closed instead of half-formatted.
//!
//! The guaranteed contract (no portable filesystem offers one atomic
//! transaction across multiple files):
//!
//! - every preflight failure happens before the first source write;
//! - ordinary write failures restore the replaced files in reverse order
//!   from verified backups and report `format.write-failed`;
//! - while the durable journal or the active lock exists, every loader
//!   entry path fails closed with `format.recovery-required`;
//! - the next explicit mutating `format` operation recovers — restoring
//!   every manifest file to its verified before digest — before doing new
//!   work. Check and editor modes never recover and never create state.
//!
//! Backups under `.lekalo/cache/format/<planId>/before/` are immutable
//! local recovery material, never canonical source, never published.

use std::path::{Path, PathBuf};

use crate::loader::LoadSelection;
use crate::result::{DomainResult, Status};

/// The closed runtime home of formatter state under `.lekalo/cache`.
pub(crate) const FORMAT_HOME: &str = ".lekalo/cache/format";
/// The exclusive runtime formatter lock (distinct from `lekalo.lock`).
const ACTIVE_LOCK: &str = "active.lock";
/// The durable journal inside one plan home.
const JOURNAL: &str = "journal.json";
/// The maximum journal/manifest record bytes admitted for parsing.
const RECORD_BOUND: u64 = 256 * 1024;
/// The maximum admitted file count of one journal/manifest record.
const RECORD_MAX_FILES: usize = 4096;
/// The maximum attempt suffix of one plan identity.
const ATTEMPT_BOUND: usize = 8;
/// The immutable backup manifest inside one plan home.
const MANIFEST: &str = "manifest.json";
/// The internal, versioned runtime-record identity (never a public
/// contract; the reader owns this version).
const RUNTIME_SCHEMA: &str = "lekalo/format-transaction/v0.8.0";

/// Why an interrupted transaction could not be recovered.
#[derive(Debug)]
pub(crate) struct RecoveryError {
    pub plan_id: String,
    pub reason: &'static str,
}

impl RecoveryError {
    #[allow(dead_code)] // constructed by the recovery paths below
    fn new(plan_id: String, reason: &'static str) -> Self {
        Self { plan_id, reason }
    }
}

/// The two failure families recovery can end a run with: the material
/// disagrees with itself (or a live writer holds the home), or a source
/// moved somewhere the pinned plan never authorized. The second is the
/// explicit conflict decision and is never reported as the general
/// recovery refusal.
#[derive(Debug)]
pub(crate) enum RecoveryFailure {
    /// A live formatter owns the runtime home right now.
    LiveWriter,
    /// The recovery material refused recovery; state stays.
    Recovery(RecoveryError),
    /// A source's current bytes diverge from the pinned plan before any
    /// restore; the journal stays and the conflict names the divergence.
    Conflict {
        path: String,
        expected: String,
        current: String,
    },
}

/// The process-death-released formatter ownership lock: an exclusive
/// advisory OS lock on one runtime file. Ownership lives in the open
/// descriptor (POSIX `flock`) or the share-mode-zero handle (Windows);
/// the kernel releases it when the process dies. The file is never
/// removed — deleting a held lock file would let a second owner lock a
/// fresh inode — so its presence alone is not a reader signal; the
/// durable journal is.
pub(crate) struct FormatGuard {
    // Held for RAII: dropping the file releases the OS lock.
    #[allow(dead_code)]
    file: std::fs::File,
}

impl FormatGuard {
    /// Best-effort advisory stamp of the owned plan identity. The stamp
    /// never gates anything; diagnostics read it only as a hint.
    fn stamp(&self, plan_id: &str) {
        use std::io::{Seek, Write};
        let mut file = &self.file;
        if file.set_len(0).is_ok() {
            let _ = file.seek(std::io::SeekFrom::Start(0));
            let _ = file.write_all(format!("{plan_id}\n").as_bytes());
        }
    }
}

/// One write failure of the apply pipeline.
pub(crate) enum ApplyFailure {
    /// Another mutating formatter holds the runtime lock.
    Locked,
    /// A recovered plan needs the explicit conflict decision: a source's
    /// current bytes diverge from the pinned plan before any restore.
    RecoverConflict {
        path: String,
        expected: String,
        current: String,
    },
    /// A pinned preimage changed before its replacement.
    SourceChanged {
        path: String,
        expected: String,
        current: String,
    },
    /// A replacement-stage failure; verified rollback or fail-closed
    /// recovery applies.
    WriteFailed {
        stage: &'static str,
        rollback: &'static str,
    },
    /// Recovery material disagrees with itself.
    /// The recovery material disagreed with itself mid-apply.
    RecoveryError(RecoveryError),
}

impl ApplyFailure {
    /// Project onto the terminal domain result.
    pub fn into_result(self) -> DomainResult {
        match self {
            // The registered infrastructure denial of a busy formatter
            // transaction — never the semantic family.
            Self::Locked => failure_locked(),
            Self::RecoverConflict {
                path,
                expected,
                current,
            } => failure_source_changed(&path, &expected, &current),
            Self::SourceChanged {
                path,
                expected,
                current,
            } => failure_source_changed(&path, &expected, &current),
            Self::WriteFailed { stage, rollback } => {
                let mut data = crate::diagnostics::DataObject::new();
                data.insert(
                    "stage".to_owned(),
                    crate::diagnostics::types::DataValue::Token(stage.to_owned()),
                );
                data.insert(
                    "rollback".to_owned(),
                    crate::diagnostics::types::DataValue::Token(rollback.to_owned()),
                );
                // The registered path guarantee of a replacement-stage
                // failure: the transaction root document.
                let source = crate::diagnostics::normalize::source_location(
                    Some("lekalo/project.yaml".to_owned()),
                    None,
                );
                let diagnostics =
                    crate::diagnostics::normalize::build("format.write-failed", None, source, data)
                        .map(|diagnostic| vec![diagnostic])
                        .unwrap_or_default();
                let set = DomainResult::from_wire_set(Status::Invalid, diagnostics);
                DomainResult::invalid(set)
            }
            Self::RecoveryError(error) => super::failure_recovery(&error.plan_id, error.reason),
        }
    }
}

/// The registered `format.locked` denial: another mutating formatter
/// holds the runtime ownership lock (deny, exit 3, stdout).
fn failure_locked() -> DomainResult {
    let data = crate::diagnostics::DataObject::new();
    let diagnostics = crate::diagnostics::normalize::build("format.locked", None, None, data)
        .map(|diagnostic| vec![diagnostic])
        .unwrap_or_default();
    let set = DomainResult::from_wire_set(Status::Denied, diagnostics);
    DomainResult::denied(set)
}

/// The registered `format.source-changed` denial with its guaranteed
/// path location: the exact pin that diverged.
fn failure_source_changed(path: &str, expected: &str, current: &str) -> DomainResult {
    let mut data = crate::diagnostics::DataObject::new();
    data.insert(
        "expectedDigest".to_owned(),
        crate::diagnostics::types::DataValue::Token(expected.to_owned()),
    );
    data.insert(
        "currentDigest".to_owned(),
        crate::diagnostics::types::DataValue::Token(current.to_owned()),
    );
    let source = crate::diagnostics::normalize::source_location(Some(path.to_owned()), None);
    let diagnostics =
        crate::diagnostics::normalize::build("format.source-changed", None, source, data)
            .map(|diagnostic| vec![diagnostic])
            .unwrap_or_default();
    let set = DomainResult::from_wire_set(Status::Denied, diagnostics);
    DomainResult::denied(set)
}

/// The fail-closed loader gate: a durable journal blocks reads exactly
/// like a migration journal does. The ownership lock file is not a
/// reader signal: between acquiring ownership and writing the journal
/// nothing has been replaced, so readers see consistent originals, and
/// a crashed run in that window leaves no partially written source.
pub(crate) fn recovery_code(fs: &crate::project_fs::Fs) -> Option<&'static str> {
    let entries = fs.entries(FORMAT_HOME).ok()?;
    for (name, entry_type) in entries {
        if entry_type != crate::project_fs::EntryType::Directory {
            continue;
        }
        if matches!(
            fs.entry_type(&format!("{FORMAT_HOME}/{name}"), JOURNAL),
            Ok(crate::project_fs::EntryType::File)
        ) {
            return Some("format.recovery-required");
        }
    }
    None
}

/// The plan identity of an active interrupted transaction, if any: the
/// first journaled plan home's pinned id, bounded before use.
pub(crate) fn pending_plan(selection: &LoadSelection) -> Option<String> {
    let root = crate::loader::root_for_selection(selection).ok()?;
    let fs = crate::project_fs::Fs::open(&root).ok()?;
    recovery_code(&fs)?;
    let write = FormatWrite::new(root);
    for plan_id in plan_directories_from(&write) {
        if !write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{JOURNAL}")) {
            continue;
        }
        if let Ok(Some(bytes)) =
            fs.read_file_opt(&format!("{FORMAT_HOME}/{plan_id}"), JOURNAL, 4096)
        {
            if let Ok(text) = String::from_utf8(bytes) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(id) = value
                        .get("plan_id")
                        .or_else(|| value.get("planId"))
                        .and_then(|found| found.as_str())
                    {
                        return Some(id.chars().take(64).collect());
                    }
                }
            }
        }
        return Some("unknown".to_owned());
    }
    Some("unknown".to_owned())
}

/// Acquire the exclusive formatter ownership of one selection's root
/// and recover every interrupted plan (those still carrying a journal)
/// before new mutating work. The same real ownership lock serializes
/// recovery and the apply that follows, so a live writer is refused as
/// busy instead of being rolled back as if crashed (finding C).
/// Completed plan homes — journal removed at commit — are rollback
/// material, never recovery targets; homes with neither journal nor
/// manifest are crashed partial attempts and are swept. The guard is
/// held by the caller across the load/plan/apply window.
pub(crate) fn recover_pending(
    selection: &LoadSelection,
) -> Result<Option<FormatGuard>, RecoveryFailure> {
    let root = crate::loader::root_for_selection(selection).map_err(|_| {
        RecoveryFailure::Recovery(RecoveryError {
            plan_id: "unknown".to_owned(),
            reason: "root-unresolved",
        })
    })?;
    // Read the recovery signal first: a project with no formatter state
    // gets no runtime-home writes at all from planning (a no-op apply on
    // a canonical tree stays byte-identical, finding D's gate premise).
    let fs = crate::project_fs::Fs::open(&root).map_err(|_| {
        RecoveryFailure::Recovery(RecoveryError {
            plan_id: "unknown".to_owned(),
            reason: "root-unreadable",
        })
    })?;
    if recovery_code(&fs).is_none() {
        return Ok(None);
    }
    let write = FormatWrite::new(root);
    let guard = write
        .acquire_ownership()
        .map_err(|_| RecoveryFailure::LiveWriter)?;
    let plans = plan_directories_from(&write);
    for plan_id in &plans {
        // Only interrupted plans (durable journal present) recover;
        // completed homes hold immutable backups for explicit rollback.
        if !write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{JOURNAL}")) {
            continue;
        }
        recover_plan(&write, plan_id)?;
    }
    for plan_id in &plans {
        // A home with neither journal nor manifest is a crashed attempt
        // between ownership and the manifest write; its unpinned partial
        // backups are swept so a later identical retry cannot collide.
        if !write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{JOURNAL}"))
            && !write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{MANIFEST}"))
        {
            let _ = write.remove_dir(&format!("{FORMAT_HOME}/{plan_id}"));
        }
    }
    Ok(Some(guard))
}

/// The plan directories beneath the formatter home (sorted).
fn plan_directories_from(write: &FormatWrite) -> Vec<String> {
    let Ok(home) = write.physical(FORMAT_HOME, false) else {
        return Vec::new();
    };
    let Ok(read) = std::fs::read_dir(&home) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .metadata()
                .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
                .unwrap_or(false)
        })
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    names
}

/// Whether one text is the runtime plan-home spelling: 47 hex digits,
/// `-`, and a decimal attempt counter (`[a-z0-9][a-z0-9._-]{0,63}` fits
/// the governed path grammar with room for the attempt suffix).
fn valid_plan_id(text: &str) -> bool {
    let Some((base, attempt)) = text.rsplit_once('-') else {
        return false;
    };
    let digits = base.len() == 47 && base.bytes().all(|byte| byte.is_ascii_hexdigit());
    let count = !attempt.is_empty()
        && attempt.len() <= 3
        && attempt.bytes().all(|byte| byte.is_ascii_digit())
        && attempt != "0";
    digits && count
}

/// The attempt suffix of a plan home spelling.
fn plan_attempt(plan_id: &str) -> usize {
    plan_id
        .rsplit_once('-')
        .and_then(|(_, attempt)| attempt.parse().ok())
        .unwrap_or(1)
}

/// The runtime plan-home spelling of one base identity and attempt.
fn plan_home(base: &str, attempt: usize) -> String {
    format!("{}-{attempt}", &base[..47.min(base.len())])
}

/// The admitted-Model-path grammar of one recovery pin (finding A): the
/// loader's exact enumeration — the project document, module manifests,
/// and the closed definition-kind stems — under the portable path
/// grammar. No other path can ever be restored.
fn valid_plan_file(path: &str) -> bool {
    if crate::project_fs::path_violation(path).is_some() {
        return false;
    }
    if path == "lekalo/project.yaml" {
        return true;
    }
    let Some(rest) = path.strip_prefix("lekalo/modules/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let directory = parts.next().unwrap_or_default();
    let file = parts.next().unwrap_or_default();
    if parts.next().is_some() || directory.is_empty() || file.is_empty() {
        return false;
    }
    file == "module.yaml"
        || crate::project_fs::KIND_STEMS
            .iter()
            .any(|stem| file == format!("{stem}.yaml"))
}

/// Whether one text is a plain 64-hex-digit SHA-256 spelling.
fn valid_digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Parse and fully validate one bounded recovery record (finding A).
macro_rules! validated_record {
    ($write:expr, $path:expr, $plan_id:expr, $reason:literal, $type:ty) => {{
        let bytes = $write.read_file_bounded($path, RECORD_BOUND).map_err(|_| {
            RecoveryFailure::Recovery(RecoveryError {
                plan_id: $plan_id.to_owned(),
                reason: concat!($reason, "-unreadable"),
            })
        })?;
        let record: $type = serde_json::from_slice(&bytes).map_err(|_| {
            RecoveryFailure::Recovery(RecoveryError {
                plan_id: $plan_id.to_owned(),
                reason: concat!($reason, "-malformed"),
            })
        })?;
        record.validate($plan_id).map_err(|reason| {
            RecoveryFailure::Recovery(RecoveryError {
                plan_id: $plan_id.to_owned(),
                reason,
            })
        })?;
        record
    }};
}

/// Restore one plan's manifest files to their verified before digests and
/// clear the journal. Every record is fully validated (finding A) and
/// every current source digest is checked before the first restore
/// (finding B): a post-crash user edit leaves the journal and all
/// sources untouched and surfaces as the explicit conflict decision.
fn recover_plan(write: &FormatWrite, plan_id: &str) -> Result<(), RecoveryFailure> {
    let home = format!("{FORMAT_HOME}/{plan_id}");
    let manifest_path = format!("{home}/{MANIFEST}");
    let manifest: Manifest =
        validated_record!(write, manifest_path.as_str(), plan_id, "manifest", Manifest);
    let journal_path = format!("{home}/{JOURNAL}");
    let journal: Journal =
        validated_record!(write, journal_path.as_str(), plan_id, "journal", Journal);
    if journal.manifest_sha256 != manifest.digest() {
        return Err(RecoveryFailure::Recovery(RecoveryError {
            plan_id: plan_id.to_owned(),
            reason: "journal-mismatch",
        }));
    }
    // Preflight EVERY source+backup pair before the first restore
    // (finding B). A source whose current digest is neither the before
    // nor the after pin is a post-crash edit; recovery must never erase
    // it, so the run ends in the explicit conflict decision with the
    // journal and every source untouched.
    for file in &manifest.files {
        let current = write.read_file(&file.path).map_err(|_| {
            RecoveryFailure::Recovery(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "verify-failed",
            })
        })?;
        let digest = crate::digest::sha256_hex(&current);
        if digest != file.before_sha256 && digest != file.after_sha256 {
            return Err(RecoveryFailure::Conflict {
                path: file.path.clone(),
                expected: file.before_sha256.clone(),
                current: digest,
            });
        }
        let backup_path = format!("{home}/before/{}", backup_logical(&file.path));
        let backup = write.read_file(&backup_path).map_err(|_| {
            RecoveryFailure::Recovery(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "backup-unreadable",
            })
        })?;
        if crate::digest::sha256_hex(&backup) != file.before_sha256 {
            return Err(RecoveryFailure::Recovery(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "backup-mismatch",
            }));
        }
    }
    // Verified restore: every manifest file back to its before digest,
    // reverse order, from the preflight-verified backups.
    for file in manifest.files.iter().rev() {
        let backup_path = format!("{home}/before/{}", backup_logical(&file.path));
        let backup = write.read_file(&backup_path).map_err(|_| {
            RecoveryFailure::Recovery(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "backup-unreadable",
            })
        })?;
        if crate::digest::sha256_hex(&backup) != file.before_sha256 {
            return Err(RecoveryFailure::Recovery(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "backup-mismatch",
            }));
        }
        write.replace_file(&file.path, &backup).map_err(|_| {
            RecoveryFailure::Recovery(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "restore-failed",
            })
        })?;
    }
    for file in &manifest.files {
        let current = write.read_file(&file.path).map_err(|_| {
            RecoveryFailure::Recovery(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "verify-failed",
            })
        })?;
        if crate::digest::sha256_hex(&current) != file.before_sha256 {
            return Err(RecoveryFailure::Recovery(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "verify-mismatch",
            }));
        }
    }
    write.remove_file(&journal_path).map_err(|_| {
        RecoveryFailure::Recovery(RecoveryError {
            plan_id: plan_id.to_owned(),
            reason: "journal-cleanup",
        })
    })?;
    Ok(())
}

/// Apply one verified plan through the confined transaction. `files` are
/// the changed rows (path, before digest, after digest) sorted by path;
/// `after_bytes` holds the verified candidate bytes in the same order.
pub(crate) fn apply(
    project: &Option<String>,
    policy_digest: &str,
    files: &[(String, String, String)],
    after_bytes: &[Vec<u8>],
    guard: Option<FormatGuard>,
) -> Result<(), ApplyFailure> {
    let selection = LoadSelection {
        project: project.clone(),
    };
    let root =
        crate::loader::root_for_selection(&selection).map_err(|_| ApplyFailure::WriteFailed {
            stage: "root",
            rollback: "not-started",
        })?;
    let write = FormatWrite::new(root);

    // Phase 1: the exclusive OS ownership lock over the whole mutating
    // window. The kernel releases it on process death, so a second
    // formatter sees the busy denial instead of mistaking a live writer
    // for a crashed one (finding C). Recovery and apply share one
    // unbroken window: the guard the caller already holds (or a fresh
    // acquisition when entering through the transaction directly).
    write
        .ensure_dir(FORMAT_HOME)
        .map_err(|_| ApplyFailure::Locked)?;
    let guard = match guard {
        Some(held) => held,
        None => write
            .acquire_ownership()
            .map_err(|_| ApplyFailure::Locked)?,
    };

    // The stable content identity plus the attempt suffix resolved under
    // the lock: a retry of an already-completed plan (external revert to
    // identical bytes, interrupted-then-recovered material) gets a fresh
    // home instead of colliding with the completed one on exclusive
    // writes (finding D). Bound the chain; a full chain is a bug, not a
    // condition to spin on.
    let base = plan_identity(policy_digest, files);
    let mut attempt = 1usize;
    while write.file_exists(&format!(
        "{FORMAT_HOME}/{}/{}",
        plan_home(&base, attempt),
        MANIFEST
    )) || write.file_exists(&format!(
        "{FORMAT_HOME}/{}/{}",
        plan_home(&base, attempt),
        JOURNAL
    )) {
        attempt += 1;
        if attempt > ATTEMPT_BOUND {
            drop(guard);
            return Err(ApplyFailure::WriteFailed {
                stage: "plan",
                rollback: "not-started",
            });
        }
    }
    let plan_id = plan_home(&base, attempt);

    // The durable marker with the owned identity (reader-gate state for
    // diagnostics; the OS lock above is the real exclusion).
    guard.stamp(&plan_id);
    let outcome = apply_locked(&write, &plan_id, policy_digest, files, after_bytes);
    let journal_present = write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{JOURNAL}"));
    if !journal_present {
        let _ = write.remove_file(&format!("{FORMAT_HOME}/{ACTIVE_LOCK}"));
    }
    drop(guard);
    outcome
}

fn apply_locked(
    write: &FormatWrite,
    plan_id: &str,
    policy_digest: &str,
    files: &[(String, String, String)],
    after_bytes: &[Vec<u8>],
) -> Result<(), ApplyFailure> {
    let home = format!("{FORMAT_HOME}/{plan_id}");
    let before_dir = format!("{home}/before");

    // Phase 2: immutable backups verified against the pinned preimages.
    for (index, (path, before, _)) in files.iter().enumerate() {
        let backup_path = format!("{before_dir}/{}", backup_logical(path));
        let (backup_dir, _) = split_logical(&backup_path);
        write
            .ensure_dir(backup_dir)
            .map_err(|_| ApplyFailure::WriteFailed {
                stage: "backup",
                rollback: "not-started",
            })?;
        let current = write
            .read_file(path)
            .map_err(|_| ApplyFailure::WriteFailed {
                stage: "backup",
                rollback: "not-started",
            })?;
        let current_digest = crate::digest::sha256_hex(&current);
        if &current_digest != before {
            return Err(ApplyFailure::SourceChanged {
                path: path.clone(),
                expected: before.clone(),
                current: current_digest,
            });
        }
        write
            .write_new(&backup_path, &current)
            .map_err(|_| ApplyFailure::WriteFailed {
                stage: "backup",
                rollback: "not-started",
            })?;
        if after_bytes.get(index).is_none() {
            return Err(ApplyFailure::WriteFailed {
                stage: "plan",
                rollback: "not-started",
            });
        }
    }

    // Phase 3: the backup manifest, then the durable journal.
    let manifest = Manifest {
        schema_version: RUNTIME_SCHEMA.to_owned(),
        plan_id: plan_id.to_owned(),
        policy_digest: policy_digest.to_owned(),
        files: files
            .iter()
            .map(|(path, before, after)| PlanFile {
                path: path.clone(),
                before_sha256: before.clone(),
                after_sha256: after.clone(),
            })
            .collect(),
    };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|_| ApplyFailure::WriteFailed {
        stage: "manifest",
        rollback: "not-started",
    })?;
    write
        .write_new(&format!("{home}/{MANIFEST}"), &manifest_bytes)
        .map_err(|_| ApplyFailure::WriteFailed {
            stage: "manifest",
            rollback: "not-started",
        })?;
    let journal = Journal {
        schema_version: RUNTIME_SCHEMA.to_owned(),
        plan_id: plan_id.to_owned(),
        manifest_sha256: manifest.digest(),
    };
    let journal_path = format!("{home}/{JOURNAL}");
    write
        .write_new(
            &journal_path,
            &serde_json::to_vec(&journal).unwrap_or_default(),
        )
        .map_err(|_| ApplyFailure::WriteFailed {
            stage: "journal",
            rollback: "not-started",
        })?;

    // Phase 4: preimage-verified replacement in sorted order.
    for (index, (path, before, _)) in files.iter().enumerate() {
        let current = write.read_file(path);
        let replaced = match current {
            Ok(bytes) => {
                let digest = crate::digest::sha256_hex(&bytes);
                if &digest != before {
                    return Err(ApplyFailure::SourceChanged {
                        path: path.clone(),
                        expected: before.clone(),
                        current: digest,
                    });
                }
                write.replace_file(path, &after_bytes[index])
            }
            Err(_) => write.replace_file(path, &after_bytes[index]),
        };
        if replaced.is_err() {
            return match restore_prefix(write, plan_id, &manifest, index) {
                Ok(()) => {
                    let _ = write.remove_file(&journal_path);
                    Err(ApplyFailure::WriteFailed {
                        stage: "replace",
                        rollback: "restored",
                    })
                }
                Err(error) => Err(ApplyFailure::RecoveryError(error)),
            };
        }
    }

    // Phase 5: post-apply verification through the same read path.
    for (index, (path, _, after)) in files.iter().enumerate() {
        match write.read_file(path) {
            Ok(bytes) => {
                if crate::digest::sha256_hex(&bytes) != *after || bytes != after_bytes[index] {
                    return match restore_prefix(write, plan_id, &manifest, files.len()) {
                        Ok(()) => {
                            let _ = write.remove_file(&journal_path);
                            Err(ApplyFailure::WriteFailed {
                                stage: "verify",
                                rollback: "restored",
                            })
                        }
                        Err(error) => Err(ApplyFailure::RecoveryError(error)),
                    };
                }
            }
            Err(_) => {
                return match restore_prefix(write, plan_id, &manifest, files.len()) {
                    Ok(()) => {
                        let _ = write.remove_file(&journal_path);
                        Err(ApplyFailure::WriteFailed {
                            stage: "verify",
                            rollback: "restored",
                        })
                    }
                    Err(error) => Err(ApplyFailure::RecoveryError(error)),
                };
            }
        }
    }

    // Phase 6: committed. Backups and manifest remain for explicit
    // rollback by recovery; the journal is the durable signal only while
    // work is in flight.
    write
        .remove_file(&journal_path)
        .map_err(|_| ApplyFailure::WriteFailed {
            stage: "commit",
            rollback: "restored",
        })?;
    Ok(())
}

/// Restore the manifest's first `prefix` files (the completed set) in
/// reverse order from verified backups.
fn restore_prefix(
    write: &FormatWrite,
    plan_id: &str,
    manifest: &Manifest,
    prefix: usize,
) -> Result<(), RecoveryError> {
    let home = format!("{FORMAT_HOME}/{plan_id}");
    for file in manifest.files[..prefix.min(manifest.files.len())]
        .iter()
        .rev()
    {
        let backup_path = format!("{home}/before/{}", backup_logical(&file.path));
        let backup = write.read_file(&backup_path).map_err(|_| RecoveryError {
            plan_id: plan_id.to_owned(),
            reason: "backup-unreadable",
        })?;
        if crate::digest::sha256_hex(&backup) != file.before_sha256 {
            return Err(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "backup-mismatch",
            });
        }
        write
            .replace_file(&file.path, &backup)
            .map_err(|_| RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "restore-failed",
            })?;
    }
    Ok(())
}

/// The stable plan identity of one verified change set: policy digest
/// and pinned pins only. Transaction-attempt storage is decided
/// separately (the attempt suffix resolved under the ownership lock),
/// so two attempts of one change set never share a home (finding D).
/// The runtime home spelling is the first 47 hex digits of this digest
/// plus the attempt suffix, so every plan home satisfies the governed
/// segment grammar (at most 64 portable characters).
fn plan_identity(policy_digest: &str, files: &[(String, String, String)]) -> String {
    let mut material = String::with_capacity(128 + files.len() * 80);
    material.push_str(policy_digest);
    for (path, before, after) in files {
        material.push('\u{1F}');
        material.push_str(path);
        material.push('\u{1F}');
        material.push_str(before);
        material.push('\u{1F}');
        material.push_str(after);
    }
    crate::digest::sha256_hex(material.as_bytes())
}

/// The backup-home spelling of a source logical path: the leading
/// `lekalo/` component is dropped so the runtime tree never contains a
/// nested `lekalo` directory, which the accepted #4 scan denies.
fn backup_logical(path: &str) -> &str {
    path.strip_prefix("lekalo/").unwrap_or(path)
}

/// Split a logical path into its directory and final segment.
fn split_logical(logical: &str) -> (&str, &str) {
    match logical.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", logical),
    }
}

/// One validated plan file pin. Exact field set, validated digest
/// spellings, and an admitted Model path (finding A).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanFile {
    path: String,
    before_sha256: String,
    after_sha256: String,
}

impl PlanFile {
    fn validate(&self) -> Result<(), &'static str> {
        if !valid_plan_file(&self.path) {
            return Err("manifest-path");
        }
        if !valid_digest(&self.before_sha256) || !valid_digest(&self.after_sha256) {
            return Err("manifest-digest");
        }
        Ok(())
    }
}

/// The immutable backup manifest persisted before the first replacement.
/// Exact closed field set; fully validated on read (finding A).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: String,
    plan_id: String,
    policy_digest: String,
    files: Vec<PlanFile>,
}

impl Manifest {
    fn digest(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        crate::digest::sha256_hex(&bytes)
    }

    /// The exact closed-schema validation (finding A): identity pins,
    /// recomputed plan identity, unique admitted Model paths, bounded
    /// non-empty file set, and pinned digest spellings.
    fn validate(&self, plan_id: &str) -> Result<(), &'static str> {
        if self.schema_version != RUNTIME_SCHEMA || self.plan_id != plan_id {
            return Err("manifest-identity");
        }
        if !valid_plan_id(plan_id) {
            return Err("manifest-identity");
        }
        if !valid_digest(
            self.policy_digest
                .strip_prefix("sha256:")
                .unwrap_or_default(),
        ) {
            return Err("manifest-policy");
        }
        if self.files.is_empty() || self.files.len() > RECORD_MAX_FILES {
            return Err("manifest-limit");
        }
        let mut paths: Vec<&str> = self.files.iter().map(|file| file.path.as_str()).collect();
        paths.sort_unstable();
        if paths.windows(2).any(|window| window[0] == window[1]) {
            return Err("manifest-duplicate");
        }
        for file in &self.files {
            file.validate()?;
        }
        // The record only ever passes when its pins recompute the exact
        // plan base identity it claims (the home name carries the attempt
        // suffix; the record re-derives the full 64-hex digest and checks
        // the home spelling against it), so a drifted path/pin pair
        // cannot be re-signed into acceptance.
        let base = plan_identity(&self.policy_digest, &self.plan_order());
        if plan_home(&base, plan_attempt(plan_id)) != plan_id {
            return Err("manifest-identity");
        }
        Ok(())
    }

    /// The pins in plan (sorted-by-path) order.
    fn plan_order(&self) -> Vec<(String, String, String)> {
        let mut files: Vec<(String, String, String)> = self
            .files
            .iter()
            .map(|file| {
                (
                    file.path.clone(),
                    file.before_sha256.clone(),
                    file.after_sha256.clone(),
                )
            })
            .collect();
        files.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        files
    }
}

/// The durable journal: written before the first replacement, removed at
/// commit. Its presence is the fail-closed reader signal. Exact closed
/// field set; fully validated on read (finding A).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: String,
    plan_id: String,
    manifest_sha256: String,
}

impl Journal {
    /// The exact closed-schema validation (finding A).
    fn validate(&self, plan_id: &str) -> Result<(), &'static str> {
        if self.schema_version != RUNTIME_SCHEMA || self.plan_id != plan_id {
            return Err("journal-identity");
        }
        if !valid_plan_id(plan_id) || !valid_digest(&self.manifest_sha256) {
            return Err("journal-identity");
        }
        Ok(())
    }
}

/// The write capability derived from the validated project root. Opaque
/// and non-`Clone`: every operation is logical and re-checks the physical
/// path chain component by component.
struct FormatWrite {
    root: PathBuf,
}

impl FormatWrite {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Acquire the process-death-released ownership lock: an exclusive
    /// advisory OS lock on the runtime lock file (POSIX `flock`, Windows
    /// share-mode zero). The file is created once and never removed, so
    /// every owner locks the same inode; a held lock refuses a second
    /// owner immediately (non-blocking).
    fn acquire_ownership(&self) -> Result<FormatGuard, ()> {
        self.ensure_dir(FORMAT_HOME)?;
        let path = self.root.join(format!("{FORMAT_HOME}/{ACTIVE_LOCK}"));
        #[cfg(unix)]
        {
            use rustix::fs::{flock, FlockOperation};
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .map_err(|_| ())?;
            flock(&file, FlockOperation::NonBlockingLockExclusive).map_err(|_| ())?;
            Ok(FormatGuard { file })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(0)
                .open(&path)
                .map_err(|_| ())?;
            Ok(FormatGuard { file })
        }
    }

    /// Physical path of one logical path, rejecting symlink/junction/
    /// reparse components. When `expect_file` the final component must be
    /// a regular file, else a directory that already exists.
    fn physical(&self, logical: &str, expect_file: bool) -> Result<PathBuf, ()> {
        let mut path = self.root.clone();
        let segments: Vec<&str> = logical.split('/').filter(|s| !s.is_empty()).collect();
        for (position, segment) in segments.iter().enumerate() {
            path.push(segment);
            let metadata = std::fs::symlink_metadata(&path).map_err(|_| ())?;
            if metadata.file_type().is_symlink() {
                return Err(());
            }
            #[cfg(windows)]
            {
                if reparse_point(&metadata) {
                    return Err(());
                }
            }
            let last = position + 1 == segments.len();
            let is_dir = metadata.is_dir();
            if last {
                let ok = if expect_file {
                    metadata.is_file()
                } else {
                    is_dir
                };
                if !ok {
                    return Err(());
                }
            } else if !is_dir {
                return Err(());
            }
        }
        Ok(path)
    }

    /// Ensure the logical directory chain exists, creating real
    /// directories and rejecting any existing non-directory component.
    fn ensure_dir(&self, logical: &str) -> Result<(), ()> {
        let mut path = self.root.clone();
        for segment in logical.split('/').filter(|segment| !segment.is_empty()) {
            path.push(segment);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        return Err(());
                    }
                    #[cfg(windows)]
                    {
                        if reparse_point(&metadata) {
                            return Err(());
                        }
                    }
                }
                Err(_) => {
                    std::fs::create_dir(&path).map_err(|_| ())?;
                }
            }
        }
        Ok(())
    }

    /// Create one new file exclusively with the exact bytes and flush it.
    fn write_new(&self, logical: &str, bytes: &[u8]) -> Result<(), ()> {
        let dir_path = self.parent(logical)?;
        let (_, name) = split_logical(logical);
        let path = dir_path.join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| ())?;
        use std::io::Write;
        file.write_all(bytes).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        drop(file);
        sync_dir(&dir_path);
        Ok(())
    }

    /// Replace a source file atomically within its directory: write a
    /// temp sibling carrying the original's permissions, flush, then
    /// rename over the target. The target must be a reparse-free regular
    /// file; it is never truncated in place.
    fn replace_file(&self, logical: &str, bytes: &[u8]) -> Result<(), ()> {
        let dir_path = self.parent(logical)?;
        let (_, name) = split_logical(logical);
        let target = dir_path.join(name);
        let metadata = std::fs::symlink_metadata(&target).map_err(|_| ())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(());
        }
        #[cfg(windows)]
        {
            if reparse_point(&metadata) {
                return Err(());
            }
        }
        let temp = dir_path.join(format!(".{name}.lekalo-format-new"));
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|_| ())?;
            preserve_permissions(&metadata, &file)?;
            use std::io::Write;
            file.write_all(bytes).map_err(|_| {
                let _ = std::fs::remove_file(&temp);
            })?;
            file.sync_all().map_err(|_| {
                let _ = std::fs::remove_file(&temp);
            })?;
        }
        std::fs::rename(&temp, &target).map_err(|_| {
            let _ = std::fs::remove_file(&temp);
        })?;
        sync_dir(&dir_path);
        Ok(())
    }

    /// Read one file's exact bytes with reparse checks.
    fn read_file(&self, logical: &str) -> Result<Vec<u8>, ()> {
        let path = self.physical(logical, true)?;
        std::fs::read(&path).map_err(|_| ())
    }

    /// Read one file's exact bytes bounded by `max` bytes: recovery
    /// records are parsed only within a closed bound.
    fn read_file_bounded(&self, logical: &str, max: u64) -> Result<Vec<u8>, ()> {
        let path = self.physical(logical, true)?;
        let metadata = std::fs::metadata(&path).map_err(|_| ())?;
        if !metadata.is_file() || metadata.len() > max {
            return Err(());
        }
        std::fs::read(&path).map_err(|_| ())
    }

    /// Whether one file exists as a reparse-free regular file.
    fn file_exists(&self, logical: &str) -> bool {
        self.physical(logical, true).is_ok()
    }

    /// Remove one file if present.
    fn remove_file(&self, logical: &str) -> Result<(), ()> {
        match std::fs::remove_file(self.physical(logical, true)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(()),
        }
    }

    /// Remove one directory tree if present; the logical path must
    /// resolve to a real reparse-free directory. Used only for crashed
    /// partial attempt homes inside the runtime home (no journal, no
    /// manifest — never recovery material).
    fn remove_dir(&self, logical: &str) -> Result<(), ()> {
        std::fs::remove_dir_all(self.physical(logical, false)?).map_err(|_| ())
    }

    /// The validated parent directory path of one logical file path.
    fn parent(&self, logical: &str) -> Result<PathBuf, ()> {
        let (parent, _) = split_logical(logical);
        self.physical(parent, false)
    }
}

/// Carry the original file's permissions onto the replacement (POSIX).
fn preserve_permissions(metadata: &std::fs::Metadata, file: &std::fs::File) -> Result<(), ()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(metadata.permissions().mode() & 0o7777);
        file.set_permissions(permissions).map_err(|_| ())
    }
    #[cfg(not(unix))]
    {
        let _ = (metadata, file);
        Ok(())
    }
}

/// Best-effort parent-directory flush (POSIX only; Windows has no
/// portable directory fsync).
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        if let Ok(handle) = std::fs::File::open(dir) {
            let _ = handle.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

#[cfg(windows)]
fn reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
