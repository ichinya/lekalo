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

use super::failure_semantic;

/// The closed runtime home of formatter state under `.lekalo/cache`.
pub(crate) const FORMAT_HOME: &str = ".lekalo/cache/format";
/// The exclusive runtime formatter lock (distinct from `lekalo.lock`).
const ACTIVE_LOCK: &str = "active.lock";
/// The durable journal inside one plan home.
const JOURNAL: &str = "journal.json";
/// The immutable backup manifest inside one plan home.
const MANIFEST: &str = "manifest.json";
/// The internal, versioned runtime-record identity (never a public
/// contract; the reader owns this version).
const RUNTIME_SCHEMA: &str = "lekalo/format-transaction/v0.8.0";

/// Why an interrupted transaction could not be recovered.
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

/// One write failure of the apply pipeline.
pub(crate) enum ApplyFailure {
    /// Another mutating formatter holds the runtime lock.
    Locked,
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
            Self::Locked => failure_semantic("lock", "locked"),
            Self::SourceChanged {
                path,
                expected,
                current,
            } => {
                let mut data = crate::diagnostics::DataObject::new();
                data.insert(
                    "expectedDigest".to_owned(),
                    crate::diagnostics::types::DataValue::Token(expected),
                );
                data.insert(
                    "currentDigest".to_owned(),
                    crate::diagnostics::types::DataValue::Token(current),
                );
                let source =
                    crate::diagnostics::normalize::source_location(Some(path.clone()), None);
                let diagnostics = crate::diagnostics::normalize::build(
                    "format.source-changed",
                    None,
                    source,
                    data,
                )
                .map(|diagnostic| vec![diagnostic])
                .unwrap_or_default();
                let set = DomainResult::from_wire_set(Status::Denied, diagnostics);
                DomainResult::denied(set)
            }
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
                let diagnostics =
                    crate::diagnostics::normalize::build("format.write-failed", None, None, data)
                        .map(|diagnostic| vec![diagnostic])
                        .unwrap_or_default();
                let set = DomainResult::from_wire_set(Status::Invalid, diagnostics);
                DomainResult::invalid(set)
            }
            Self::RecoveryError(error) => super::failure_recovery(&error.plan_id, error.reason),
        }
    }
}

/// The fail-closed loader gate: any active formatter lock or journal
/// blocks reads exactly like a migration journal does.
pub(crate) fn recovery_code(fs: &crate::project_fs::Fs) -> Option<&'static str> {
    if let Ok(crate::project_fs::EntryType::File) = fs.entry_type(FORMAT_HOME, ACTIVE_LOCK) {
        return Some("format.recovery-required");
    }
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

/// The plan identity of an active interrupted transaction, if any.
pub(crate) fn pending_plan(selection: &LoadSelection) -> Option<String> {
    let root = crate::loader::root_for_selection(selection).ok()?;
    let fs = crate::project_fs::Fs::open(&root).ok()?;
    recovery_code(&fs)?;
    if matches!(
        fs.entry_type(FORMAT_HOME, ACTIVE_LOCK),
        Ok(crate::project_fs::EntryType::File)
    ) {
        if let Ok(Some(bytes)) = fs.read_file_opt(FORMAT_HOME, ACTIVE_LOCK, 256) {
            if let Ok(text) = String::from_utf8(bytes) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_owned());
                }
            }
        }
    }
    plan_directories(&root)
        .first()
        .cloned()
        .or_else(|| Some("unknown".to_owned()))
}

/// Recover every interrupted plan (those still carrying a journal) under
/// the selection's root before new mutating work. Completed plan homes —
/// journal removed at commit — are rollback material, never recovery
/// targets. No-op when no recovery state exists.
pub(crate) fn recover_pending(selection: &LoadSelection) -> Result<(), RecoveryError> {
    let root = crate::loader::root_for_selection(selection).map_err(|_| RecoveryError {
        plan_id: "unknown".to_owned(),
        reason: "root-unresolved",
    })?;
    let fs = crate::project_fs::Fs::open(&root).map_err(|_| RecoveryError {
        plan_id: "unknown".to_owned(),
        reason: "root-unreadable",
    })?;
    if recovery_code(&fs).is_none() {
        return Ok(());
    }
    let write = FormatWrite::new(root);
    let plans = plan_directories_from(&write);
    for plan_id in plans {
        // Only interrupted plans (durable journal present) recover;
        // completed homes hold immutable backups for explicit rollback.
        if !write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{JOURNAL}")) {
            continue;
        }
        recover_plan(&write, &plan_id)?;
    }
    // The lock outlives plan homes on a crash between lock and journal.
    let _ = write.remove_file(&format!("{FORMAT_HOME}/{ACTIVE_LOCK}"));
    Ok(())
}

/// The plan directories beneath the formatter home (sorted).
fn plan_directories(root: &Path) -> Vec<String> {
    let write = FormatWrite::new(root.to_path_buf());
    plan_directories_from(&write)
}

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

/// Restore one plan's manifest files to their verified before digests and
/// clear the journal. Fails closed on any disagreement, keeping state.
fn recover_plan(write: &FormatWrite, plan_id: &str) -> Result<(), RecoveryError> {
    let home = format!("{FORMAT_HOME}/{plan_id}");
    let manifest_path = format!("{home}/{MANIFEST}");
    let manifest_bytes = write.read_file(&manifest_path).map_err(|_| RecoveryError {
        plan_id: plan_id.to_owned(),
        reason: "manifest-unreadable",
    })?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| RecoveryError {
            plan_id: plan_id.to_owned(),
            reason: "manifest-malformed",
        })?;
    if manifest.plan_id != plan_id || manifest.schema_version != RUNTIME_SCHEMA {
        return Err(RecoveryError {
            plan_id: plan_id.to_owned(),
            reason: "manifest-identity",
        });
    }
    let journal_path = format!("{home}/{JOURNAL}");
    let journal_bytes = write.read_file(&journal_path).map_err(|_| RecoveryError {
        plan_id: plan_id.to_owned(),
        reason: "journal-unreadable",
    })?;
    let journal: Journal = serde_json::from_slice(&journal_bytes).map_err(|_| RecoveryError {
        plan_id: plan_id.to_owned(),
        reason: "journal-malformed",
    })?;
    if journal.manifest_sha256 != manifest.digest() || journal.plan_id != plan_id {
        return Err(RecoveryError {
            plan_id: plan_id.to_owned(),
            reason: "journal-mismatch",
        });
    }
    // Full conservative restore: every manifest file back to its before
    // digest, reverse order, backups verified on read.
    for file in manifest.files.iter().rev() {
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
    for file in &manifest.files {
        let current = write.read_file(&file.path).map_err(|_| RecoveryError {
            plan_id: plan_id.to_owned(),
            reason: "verify-failed",
        })?;
        if crate::digest::sha256_hex(&current) != file.before_sha256 {
            return Err(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "verify-mismatch",
            });
        }
    }
    write
        .remove_file(&journal_path)
        .map_err(|_| RecoveryError {
            plan_id: plan_id.to_owned(),
            reason: "journal-cleanup",
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
    let plan_id = plan_identity(policy_digest, files);

    // Phase 1: the exclusive runtime lock.
    write
        .ensure_dir(FORMAT_HOME)
        .map_err(|_| ApplyFailure::Locked)?;
    let lock_path = format!("{FORMAT_HOME}/{ACTIVE_LOCK}");
    match write.write_new(&lock_path, format!("{plan_id}\n").as_bytes()) {
        Ok(()) => {}
        Err(_) => return Err(ApplyFailure::Locked),
    }
    let outcome = apply_locked(&write, &plan_id, policy_digest, files, after_bytes);
    let journal_present = write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{JOURNAL}"));
    if !journal_present {
        let _ = write.remove_file(&lock_path);
    }
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

/// The deterministic plan identity of one verified change set.
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

/// The immutable backup manifest persisted before the first replacement.
#[derive(serde::Serialize, serde::Deserialize)]
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
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PlanFile {
    path: String,
    before_sha256: String,
    after_sha256: String,
}

/// The durable journal: written before the first replacement, removed at
/// commit. Its presence is the fail-closed reader signal.
#[derive(serde::Serialize, serde::Deserialize)]
struct Journal {
    schema_version: String,
    plan_id: String,
    manifest_sha256: String,
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
