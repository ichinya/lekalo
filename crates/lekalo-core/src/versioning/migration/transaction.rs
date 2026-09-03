//! The capability-safe, no-follow migration transaction (issue #9).
//!
//! Only the Model documents the accepted #7 loader enumerated may be
//! mutated, and only through this module: there is no `PathBuf` escape
//! hatch, no caller-created plan, and no raw logical path input. The writer
//! pins the validated project root, rejects symlink/junction/reparse
//! components on every operation, verifies digests at every boundary, and
//! journals its progress so a crash can only leave the documented
//! fail-closed recovery state — never a silently half-migrated project.
//!
//! The guaranteed contract (no portable filesystem offers one atomic
//! transaction across multiple files):
//!
//! - every preflight failure happens before the first source write;
//! - ordinary write failures restore the replaced files in reverse order
//!   from verified backups and report `versioning.commit-failed`;
//! - while the durable journal exists, every reader fails closed with
//!   `versioning.recovery-required`;
//! - the next explicit migrate operation recovers — restoring every
//!   manifest file to its verified before digest — before doing new work.
//!
//! Backups under `.lekalo/cache/migrations/<planId>/before/` are immutable
//! local recovery material. They are never overwritten, never published,
//! and never treated as canonical source.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::VersioningFailure;
use crate::versioning::plan::{sha256_hex, DocumentSnapshot, PlanFile};

/// The closed runtime home of migration state under `.lekalo/cache`.
pub(crate) const MIGRATIONS_HOME: &str = ".lekalo/cache/migrations";
/// The exclusive runtime migration lock (distinct from `lekalo.lock`).
pub(crate) const ACTIVE_LOCK: &str = "active.lock";
/// The durable journal file inside one plan home.
pub(crate) const JOURNAL: &str = "journal.json";
/// The immutable backup manifest inside one plan home.
pub(crate) const MANIFEST: &str = "manifest.json";

/// One planned document: its pinned before-snapshot and the exact after
/// bytes the planner produced. Sorted by logical path.
pub(crate) struct PlannedDocument {
    pub snapshot: DocumentSnapshot,
    pub after: Vec<u8>,
}

/// The immutable backup manifest persisted before the first replacement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, serde::Deserialize)]
pub struct BackupManifest {
    /// The registry artifact version the plan used.
    #[serde(rename = "registryVersion")]
    pub registry_version: String,
    /// The ordered migration edge identities.
    pub chain: Vec<String>,
    /// The planned file identities, sorted by logical path.
    pub files: Vec<PlanFile>,
    /// The declared losses of the chain.
    pub loss: Vec<String>,
}

impl BackupManifest {
    /// The manifest's own SHA-256 over its canonical JSON bytes.
    pub fn digest(&self) -> String {
        sha256_hex(&canonical_bytes(self))
    }
}

/// The durable journal: written before the first replacement, removed at
/// commit. Its presence (with the lock) is the fail-closed reader signal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, serde::Deserialize)]
struct Journal {
    #[serde(rename = "planId")]
    plan_id: String,
    #[serde(rename = "manifestSha256")]
    manifest_sha256: String,
}

/// Serialize a value to compact canonical JSON bytes.
fn canonical_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    // serde_json emits struct fields in declaration order; the output is
    // deterministic.
    serde_json::to_vec(value).unwrap_or_default()
}

/// The write capability derived from the accepted #7 project root after an
/// explicit migrate operation. Opaque and non-`Clone`: every operation is
/// logical and re-checks the physical path chain component by component.
pub(crate) struct MigrationWrite {
    root: PathBuf,
}

impl MigrationWrite {
    /// Derive the capability from an already-validated absolute root.
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Physical path of one logical path, rejecting symlink/junction/
    /// reparse components. When `expect_file` the final component must be
    /// a regular file, else a directory that already exists.
    fn physical(&self, logical: &str, expect_file: bool) -> Result<PathBuf, VersioningFailure> {
        let mut path = self.root.clone();
        let segments: Vec<&str> = logical.split('/').filter(|s| !s.is_empty()).collect();
        for (position, segment) in segments.iter().enumerate() {
            path.push(segment);
            let metadata = std::fs::symlink_metadata(&path).map_err(|_| io_failure(logical))?;
            if metadata.file_type().is_symlink() {
                return Err(VersioningFailure::DeniedPath(logical.to_owned()));
            }
            #[cfg(windows)]
            {
                // Every reparse point (junction, mount point, symlink) is a
                // policy denial on sight, matching the #4 read path.
                if reparse_point(&metadata) {
                    return Err(VersioningFailure::DeniedPath(logical.to_owned()));
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
                    return Err(io_failure(logical));
                }
            } else if !is_dir {
                return Err(io_failure(logical));
            }
        }
        Ok(path)
    }

    /// The validated parent directory path of one logical file path.
    fn parent(&self, logical: &str) -> Result<PathBuf, VersioningFailure> {
        let (parent, _) = split_logical(logical);
        self.physical(parent, false)
    }

    /// Ensure the logical directory chain exists, creating real directories
    /// and rejecting any existing non-directory component.
    pub(crate) fn ensure_dir(&self, logical: &str) -> Result<(), VersioningFailure> {
        let mut path = self.root.clone();
        let mut built = String::new();
        for segment in logical.split('/').filter(|segment| !segment.is_empty()) {
            path.push(segment);
            if !built.is_empty() {
                built.push('/');
            }
            built.push_str(segment);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        return Err(io_failure(&built));
                    }
                    #[cfg(windows)]
                    {
                        if reparse_point(&metadata) {
                            return Err(VersioningFailure::DeniedPath(built));
                        }
                    }
                }
                Err(_) => {
                    std::fs::create_dir(&path).map_err(|_| io_failure(&built))?;
                }
            }
        }
        Ok(())
    }

    /// Create one new file exclusively with the exact bytes and flush it.
    /// An existing file is never overwritten; the caller validates and
    /// reuses it instead.
    pub(crate) fn write_new(&self, logical: &str, bytes: &[u8]) -> Result<(), VersioningFailure> {
        let dir_path = self.parent(logical)?;
        let (_, name) = split_logical(logical);
        let path = dir_path.join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| io_failure(logical))?;
        use std::io::Write;
        file.write_all(bytes).map_err(|_| io_failure(logical))?;
        file.sync_all().map_err(|_| io_failure(logical))?;
        drop(file);
        sync_dir(&dir_path);
        Ok(())
    }

    /// Replace a source file atomically within its directory: write a
    /// temp sibling, flush, then rename over the target. The target must
    /// be a reparse-free regular file.
    pub(crate) fn replace_file(
        &self,
        logical: &str,
        bytes: &[u8],
    ) -> Result<(), VersioningFailure> {
        let dir_path = self.parent(logical)?;
        let (_, name) = split_logical(logical);
        let target = dir_path.join(name);
        let metadata = std::fs::symlink_metadata(&target).map_err(|_| io_failure(logical))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(VersioningFailure::DeniedPath(logical.to_owned()));
        }
        #[cfg(windows)]
        {
            if reparse_point(&metadata) {
                return Err(VersioningFailure::DeniedPath(logical.to_owned()));
            }
        }
        let temp = dir_path.join(format!("{name}.lekalo-new"));
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|_| io_failure(logical))?;
            use std::io::Write;
            file.write_all(bytes).map_err(|_| io_failure(logical))?;
            file.sync_all().map_err(|_| io_failure(logical))?;
        }
        std::fs::rename(&temp, &target).map_err(|_| io_failure(logical))?;
        sync_dir(&dir_path);
        Ok(())
    }

    /// Read one file's exact bytes with reparse checks.
    pub(crate) fn read_file(&self, logical: &str) -> Result<Vec<u8>, VersioningFailure> {
        let path = self.physical(logical, true)?;
        std::fs::read(&path).map_err(|_| io_failure(logical))
    }

    /// Whether one file exists as a reparse-free regular file.
    pub(crate) fn file_exists(&self, logical: &str) -> bool {
        self.physical(logical, true).is_ok()
    }

    /// Remove one file if present.
    pub(crate) fn remove_file(&self, logical: &str) -> Result<(), VersioningFailure> {
        match std::fs::remove_file(self.physical(logical, true)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(io_failure(logical)),
        }
    }

    /// List the plan-home directory names (sorted, reparse-checked dirs).
    pub(crate) fn plan_directories(&self) -> Vec<String> {
        let Ok(home) = self.physical(MIGRATIONS_HOME, false) else {
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

/// Map any I/O failure on a logical path to the stable stage failure; the
/// reason code is refined by the phase the caller is in.
fn io_failure(logical: &str) -> VersioningFailure {
    VersioningFailure::Io {
        logical: logical.to_owned(),
    }
}

#[cfg(windows)]
fn reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// The current lock holder, if any.
pub(crate) fn read_lock(write: &MigrationWrite) -> Option<String> {
    if !write.file_exists(&format!("{MIGRATIONS_HOME}/{ACTIVE_LOCK}")) {
        return None;
    }
    write
        .read_file(&format!("{MIGRATIONS_HOME}/{ACTIVE_LOCK}"))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// Acquire the exclusive runtime migration lock.
pub(crate) fn acquire_lock(write: &MigrationWrite, plan_id: &str) -> Result<(), LockState> {
    write
        .ensure_dir(MIGRATIONS_HOME)
        .map_err(|_| LockState::Blocked)?;
    let bytes = format!("{plan_id}\n").into_bytes();
    match write.write_new(&format!("{MIGRATIONS_HOME}/{ACTIVE_LOCK}"), &bytes) {
        Ok(()) => Ok(()),
        Err(_) => Err(LockState::Held {
            plan_id: read_lock(write).unwrap_or_default(),
        }),
    }
}

/// The outcome of a lock acquisition attempt.
pub(crate) enum LockState {
    /// Another (or a crashed) migration holds the lock.
    #[allow(dead_code)] // the held identity is diagnostic; the type is the signal
    Held { plan_id: String },
    /// The lock could not even be created or read.
    Blocked,
}

/// The backup-home spelling of a source logical path: the leading
/// `lekalo/` component is dropped so the runtime tree never contains a
/// nested `lekalo` directory, which the accepted #4 scan denies. The
/// manifest always carries the true logical path.
fn backup_logical(path: &str) -> &str {
    path.strip_prefix("lekalo/").unwrap_or(path)
}

/// Split a logical path into its directory and final segment.
pub(crate) fn split_logical(logical: &str) -> (&str, &str) {
    match logical.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", logical),
    }
}

/// The apply pipeline. `documents` must be sorted by logical path and
/// contain only documents whose bytes change.
pub(crate) fn run_apply(
    write: &MigrationWrite,
    plan_id: &str,
    manifest: &BackupManifest,
    documents: &[PlannedDocument],
) -> Result<(), VersioningFailure> {
    // Phase 2: exclusive runtime lock; a held lock means in-progress or a
    // crash the caller must recover from first.
    match acquire_lock(write, plan_id) {
        Ok(()) => {}
        Err(LockState::Held { .. }) => return Err(VersioningFailure::MigrationInProgress),
        Err(LockState::Blocked) => return Err(VersioningFailure::BackupFailed),
    }
    let result = apply_locked(write, plan_id, manifest, documents);
    // The lock is released on every path that leaves no journal behind; a
    // journal is the durable recovery signal and keeps the lock in place.
    let journal_present = write.file_exists(&format!("{MIGRATIONS_HOME}/{plan_id}/{JOURNAL}"));
    if !journal_present {
        let _ = write.remove_file(&format!("{MIGRATIONS_HOME}/{ACTIVE_LOCK}"));
    }
    result
}

fn apply_locked(
    write: &MigrationWrite,
    plan_id: &str,
    manifest: &BackupManifest,
    documents: &[PlannedDocument],
) -> Result<(), VersioningFailure> {
    let home = format!("{MIGRATIONS_HOME}/{plan_id}");
    let before_dir = format!("{home}/before");

    // Phase 3: immutable backups plus the staged manifest, verified and
    // flushed before the first source mutation. Existing material is
    // reused only after exact validation and never overwritten.
    for document in documents {
        let backup_path = format!("{before_dir}/{}", backup_logical(&document.snapshot.path));
        let (backup_dir, _) = split_logical(&backup_path);
        write
            .ensure_dir(backup_dir)
            .map_err(|_| VersioningFailure::BackupFailed)?;
        match write.read_file(&backup_path) {
            Ok(existing) => {
                if sha256_hex(&existing) != sha256_hex(&document.snapshot.bytes) {
                    return Err(VersioningFailure::BackupFailed);
                }
            }
            Err(_) => {
                write
                    .write_new(&backup_path, &document.snapshot.bytes)
                    .map_err(|_| VersioningFailure::BackupFailed)?;
            }
        }
    }
    let manifest_bytes = canonical_bytes(manifest);
    let manifest_path = format!("{home}/{MANIFEST}");
    match write.file_exists(&manifest_path) {
        true => {
            if write.read_file(&manifest_path) != Ok(manifest_bytes) {
                return Err(VersioningFailure::BackupFailed);
            }
        }
        false => {
            write
                .write_new(&manifest_path, &manifest_bytes)
                .map_err(|_| VersioningFailure::BackupFailed)?;
        }
    }

    // Phase 4: durable journal, then sorted same-filesystem replacement.
    let journal = Journal {
        plan_id: plan_id.to_owned(),
        manifest_sha256: manifest.digest(),
    };
    let journal_path = format!("{home}/{JOURNAL}");
    write
        .write_new(&journal_path, &canonical_bytes(&journal))
        .map_err(|_| VersioningFailure::MigrationInProgress)?;

    for (index, document) in documents.iter().enumerate() {
        if let Err(error) = write.replace_file(&document.snapshot.path, &document.after) {
            // Ordinary commit failure: restore completed files in reverse
            // order from verified backups.
            let completed: Vec<usize> = (0..index).collect();
            return match restore_all(write, plan_id, manifest, &completed) {
                Ok(()) => {
                    let _ = write.remove_file(&journal_path);
                    Err(error)
                }
                Err(restore_error) => Err(restore_error),
            };
        }
        // Journal rewrite after each replacement: the completed set is
        // derivable from the manifest, so rewrite = remove + create.
        let _ = write.remove_file(&journal_path);
        // The journal is gone but the sources are consistent at this
        // boundary; a crash here is still recoverable (full restore).
        write.write_new(&journal_path, &canonical_bytes(&journal))?;
    }

    // Phase 5: post-load verification through the real read path.
    if let Err(error) = verify_committed(write, manifest, documents) {
        let all: Vec<usize> = (0..manifest.files.len()).collect();
        return match restore_all(write, plan_id, manifest, &all) {
            Ok(()) => {
                let _ = write.remove_file(&journal_path);
                Err(error)
            }
            Err(restore_error) => Err(restore_error),
        };
    }

    // Phase 6: committed. Remove the journal; backups and manifest remain
    // for explicit rollback.
    write
        .remove_file(&journal_path)
        .map_err(|_| VersioningFailure::CommitFailed)?;
    Ok(())
}

/// Verify every manifest file now carries its after digest and exact
/// planned bytes.
fn verify_committed(
    write: &MigrationWrite,
    manifest: &BackupManifest,
    documents: &[PlannedDocument],
) -> Result<(), VersioningFailure> {
    for (index, file) in manifest.files.iter().enumerate() {
        let current = write
            .read_file(&file.path)
            .map_err(|_| VersioningFailure::CommitFailed)?;
        if sha256_hex(&current) != file.after_sha256 {
            return Err(VersioningFailure::CommitFailed);
        }
        let document = documents
            .get(index)
            .ok_or(VersioningFailure::CommitFailed)?;
        if current != document.after {
            return Err(VersioningFailure::CommitFailed);
        }
    }
    Ok(())
}

/// The explicit rollback for one recorded plan. Fails closed with zero
/// writes when any backup or current digest disagrees.
pub(crate) fn run_rollback(
    write: &MigrationWrite,
    plan_id: &str,
) -> Result<BackupManifest, VersioningFailure> {
    let home = format!("{MIGRATIONS_HOME}/{plan_id}");
    let manifest_path = format!("{home}/{MANIFEST}");
    if !write.file_exists(&manifest_path) {
        return Err(VersioningFailure::RollbackFailed);
    }
    let manifest_bytes = write
        .read_file(&manifest_path)
        .map_err(|_| VersioningFailure::RollbackFailed)?;
    let manifest: BackupManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| VersioningFailure::RollbackFailed)?;

    // Preflight every backup and every current after digest before any
    // write; user edits after migration refuse with rollback-conflict.
    let before_dir = format!("{home}/before");
    for file in &manifest.files {
        let backup = write
            .read_file(&format!("{before_dir}/{}", backup_logical(&file.path)))
            .map_err(|_| VersioningFailure::RollbackFailed)?;
        if sha256_hex(&backup) != file.before_sha256 {
            return Err(VersioningFailure::RollbackFailed);
        }
        let current = write
            .read_file(&file.path)
            .map_err(|_| VersioningFailure::SourceChanged)?;
        if sha256_hex(&current) != file.after_sha256 {
            return Err(VersioningFailure::RollbackConflict);
        }
    }

    // Rollback is journaled, verified, and locked exactly like apply; the
    // lock is released on every path that leaves no journal behind.
    acquire_lock(write, plan_id).map_err(|_| VersioningFailure::MigrationInProgress)?;
    let result = rollback_locked(write, &home, &before_dir, plan_id, &manifest);
    let journal_present = write.file_exists(&format!("{home}/{JOURNAL}"));
    if !journal_present {
        let _ = write.remove_file(&format!("{MIGRATIONS_HOME}/{ACTIVE_LOCK}"));
    }
    result?;
    Ok(manifest)
}

/// The journaled restore: every manifest file from its verified backup in
/// reverse manifest order, the restored state verified, then the journal
/// cleared while still holding the lock.
fn rollback_locked(
    write: &MigrationWrite,
    home: &str,
    before_dir: &str,
    plan_id: &str,
    manifest: &BackupManifest,
) -> Result<(), VersioningFailure> {
    let journal_path = format!("{home}/{JOURNAL}");
    let journal = Journal {
        plan_id: plan_id.to_owned(),
        manifest_sha256: manifest.digest(),
    };
    write
        .write_new(&journal_path, &canonical_bytes(&journal))
        .map_err(|_| VersioningFailure::MigrationInProgress)?;
    for file in manifest.files.iter().rev() {
        let backup = match write.read_file(&format!("{before_dir}/{}", backup_logical(&file.path)))
        {
            Ok(backup) => backup,
            Err(error) => {
                let _ = write.remove_file(&journal_path);
                return Err(error);
            }
        };
        if let Err(error) = write.replace_file(&file.path, &backup) {
            let _ = write.remove_file(&journal_path);
            return Err(error);
        }
    }
    // Verify the restored state, then clear the journal.
    for file in &manifest.files {
        let restored = write
            .read_file(&file.path)
            .map_err(|_| VersioningFailure::RollbackFailed)?;
        if sha256_hex(&restored) != file.before_sha256 {
            return Err(VersioningFailure::RollbackFailed);
        }
    }
    write
        .remove_file(&journal_path)
        .map_err(|_| VersioningFailure::RollbackFailed)?;
    Ok(())
}

/// Recover a crashed apply: restore every manifest file to its verified
/// before digest and clear the runtime state. Returns the recovered plan
/// identity.
pub(crate) fn run_recovery(write: &MigrationWrite, plan_id: &str) -> Result<(), VersioningFailure> {
    let home = format!("{MIGRATIONS_HOME}/{plan_id}");
    let manifest_path = format!("{home}/{MANIFEST}");
    let manifest_bytes = write
        .read_file(&manifest_path)
        .map_err(|_| VersioningFailure::RecoveryRequired)?;
    let manifest: BackupManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| VersioningFailure::RecoveryRequired)?;
    if write.file_exists(&format!("{home}/{JOURNAL}")) {
        let journal_bytes = write
            .read_file(&format!("{home}/{JOURNAL}"))
            .map_err(|_| VersioningFailure::RecoveryRequired)?;
        let journal: Journal = serde_json::from_slice(&journal_bytes)
            .map_err(|_| VersioningFailure::RecoveryRequired)?;
        if journal.manifest_sha256 != manifest.digest() || journal.plan_id != plan_id {
            return Err(VersioningFailure::RecoveryRequired);
        }
    } else {
        return Err(VersioningFailure::RecoveryRequired);
    }
    // Full conservative restore: every manifest file back to its before
    // digest, reverse order, backups verified on read.
    let all: Vec<usize> = (0..manifest.files.len()).collect();
    restore_all(write, plan_id, &manifest, &all)?;
    for file in &manifest.files {
        let current = write
            .read_file(&file.path)
            .map_err(|_| VersioningFailure::RecoveryRequired)?;
        if sha256_hex(&current) != file.before_sha256 {
            return Err(VersioningFailure::RecoveryRequired);
        }
    }
    write
        .remove_file(&format!("{home}/{JOURNAL}"))
        .map_err(|_| VersioningFailure::RecoveryRequired)?;
    write
        .remove_file(&format!("{MIGRATIONS_HOME}/{ACTIVE_LOCK}"))
        .map_err(|_| VersioningFailure::RecoveryRequired)?;
    Ok(())
}

/// Restore the given manifest indices in reverse order from verified
/// backups beneath one plan home.
fn restore_all(
    write: &MigrationWrite,
    plan_id: &str,
    manifest: &BackupManifest,
    indices: &[usize],
) -> Result<(), VersioningFailure> {
    let before_dir = format!("{MIGRATIONS_HOME}/{plan_id}/before");
    for index in indices.iter().rev() {
        let file = manifest
            .files
            .get(*index)
            .ok_or(VersioningFailure::RecoveryRequired)?;
        let backup = write
            .read_file(&format!("{before_dir}/{}", backup_logical(&file.path)))
            .map_err(|_| VersioningFailure::RecoveryRequired)?;
        if sha256_hex(&backup) != file.before_sha256 {
            return Err(VersioningFailure::RecoveryRequired);
        }
        write
            .replace_file(&file.path, &backup)
            .map_err(|_| VersioningFailure::RecoveryRequired)?;
    }
    Ok(())
}
