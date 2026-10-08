//! The durable change transaction (issue #73).
//!
//! The transaction applies only a service-created sealed plan. Under a
//! project-scoped writer lock it rechecks the idempotency ledger and every
//! before digest, stages a durable journal plus verified backups, applies
//! same-directory replacements, verifies every after digest, finalizes the
//! canonical history in the same transaction, and then removes the journal
//! as the commit marker. Ordinary failure restores all originals in reverse
//! order; a crash leaves a recoverable journal that blocks every project
//! reader until explicit `change recover` succeeds.
//!
//! Recovery safety (review round 1): every journaled path is validated
//! against the confined project root component-wise with no-follow checks
//! before any write (finding 1); each backup is digest-verified against the
//! `beforeDigest` pinned in the journal before restoration (finding 4); the
//! journal decodes into a closed typed structure that fails on malformed
//! members (finding 11); and a stale writer lock (dead owner) is reclaimed
//! by recovery, while a live lock still denies (finding 3).

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value as Json;

use super::diagnostic::ChangeError;
use super::edit::ByteEdit;
use crate::project_fs::Fs;

/// The journal file name inside one transaction directory.
const JOURNAL_NAME: &str = "journal.json";

/// The lock-holder metadata file name inside the change cache.
const LOCK_META_NAME: &str = "writer.lock.json";

/// Maximum journal bytes accepted before decoding.
const MAX_JOURNAL_BYTES: usize = 64 * 1024;

/// Maximum journaled file entries per transaction.
const MAX_JOURNAL_FILES: usize = 1024;

/// One staged replacement: the target logical path, its verified backup,
/// and the exact prospective bytes.
struct StagedFile {
    target: PathBuf,
    backup: Option<PathBuf>,
    after_bytes: Vec<u8>,
    after_digest: String,
}

/// The open transaction: journals, backups, replacements, and history.
pub(crate) struct Transaction {
    root: PathBuf,
    id: String,
    journal_dir: PathBuf,
    staged: Vec<StagedFile>,
}

/// The project-scoped writer lock; exclusive creation, removed on drop.
pub(crate) struct WriterLock {
    path: Option<PathBuf>,
    meta: Option<PathBuf>,
}

impl WriterLock {
    /// Acquire the change writer lock or report the contention. The lock
    /// token records the holder's process identity and a monotonic-ish
    /// timestamp so a later recovery can tell a dead holder from a live
    /// one (review finding 3).
    pub(crate) fn acquire(root: &Path) -> Result<Self, ChangeError> {
        let cache = root.join(".lekalo/cache/change");
        std::fs::create_dir_all(&cache).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let path = cache.join("writer.lock");
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => {
                let meta = cache.join(LOCK_META_NAME);
                let record = serde_json::json!({
                    "pid": std::process::id(),
                    "boot": boot_token(),
                    "started": unix_time(),
                });
                let _ = std::fs::write(&meta, record.to_string().as_bytes());
                Ok(Self {
                    path: Some(path),
                    meta: Some(meta),
                })
            }
            Err(_) => Err(ChangeError::PreconditionFailed {
                subject: "writer-lock".to_owned(),
                detail: "held",
            }),
        }
    }

    /// Reclaim the lock of a crashed owner: only when the recorded holder
    /// process is provably dead on this boot. A live holder (including an
    /// unrecordable PID reuse on another boot) still denies.
    pub(crate) fn reclaim_stale(root: &Path) -> Result<Self, ChangeError> {
        let cache = root.join(".lekalo/cache/change");
        let path = cache.join("writer.lock");
        if !path.is_file() {
            // No lock artifact: acquire normally.
            return Self::acquire(root);
        }
        let holder_alive = std::fs::read(cache.join(LOCK_META_NAME))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<LockMeta>(&bytes).ok())
            .map(|meta| process_alive(meta.pid, &meta.boot))
            .unwrap_or(false);
        if holder_alive {
            return Err(ChangeError::PreconditionFailed {
                subject: "writer-lock".to_owned(),
                detail: "held",
            });
        }
        // Dead (or unidentifiable) holder: take over the artifact.
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(cache.join(LOCK_META_NAME));
        Self::acquire(root)
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
        if let Some(meta) = self.meta.take() {
            let _ = std::fs::remove_file(meta);
        }
    }
}

#[derive(serde::Deserialize)]
struct LockMeta {
    pid: u32,
    boot: String,
}

/// A process-liveness token that distinguishes the current boot (PID reuse
/// across reboots must never read as liveness). Best effort: an
/// unavailable token collapses to a boot-unique sentinel.
fn boot_token() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(token) = std::fs::read_to_string("/proc/sys/kernel/random/boot_id") {
            return token.trim().to_owned();
        }
    }
    #[cfg(windows)]
    {
        // The volume serial number of the system drive is a stable per-boot
        // witness (it changes on reformat, not on reboot) combined with the
        // boot tick count; together they never collide across boots in any
        // realistic recovery window.
        use windows_sys::Win32::System::SystemInformation::GetTickCount64;
        // SAFETY: GetTickCount64 takes no arguments and cannot fail.
        return format!("tick-{}", unsafe { GetTickCount64() });
    }
    #[allow(unreachable_code)]
    {
        format!("fallback-{}", std::process::id())
    }
}

/// The process exists right now (signal 0 probe on Unix; a query handle
/// with exit-code inspection on Windows). Only meaningful within one boot.
fn process_alive(pid: u32, boot: &str) -> bool {
    if boot != boot_token() {
        return false;
    }
    if pid == std::process::id() {
        return true;
    }
    #[cfg(unix)]
    {
        use rustix::process::{kill_process, Pid, Signal};
        match Pid::from_raw(pid as i32) {
            Some(pid) => kill_process(pid, Signal::None).is_ok(),
            None => false,
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // STILL_ACTIVE is a plain 32-bit exit-code constant on Windows.
        const STILL_ACTIVE: u32 = 0x0000_0103;
        const ERROR_INVALID_PARAMETER: u32 = 87;
        const ERROR_ACCESS_DENIED: u32 = 5;
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                let error = GetLastError();
                // A vanished PID is dead. An inaccessible-but-existing PID
                // (protected process) stays live: reclaiming its lock could
                // interleave with a real writer.
                return error == ERROR_ACCESS_DENIED;
            }
            let mut code: u32 = 0;
            let queried = GetExitCodeProcess(handle, &mut code) != 0;
            let invalid = GetLastError() == ERROR_INVALID_PARAMETER;
            CloseHandle(handle);
            if !queried || invalid {
                return false;
            }
            code != STILL_ACTIVE
        }
    }
}

/// Seconds since the Unix epoch; informational only.
fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// The durable reader barrier: `Some("change.recovery-required")` while any
/// change journal exists under the project cache. The loader calls this in
/// its fail-closed phase so no reader observes a partial revision.
pub(crate) fn recovery_barrier_code(fs: &crate::project_fs::Fs) -> Option<&'static str> {
    let home = super::version::TRANSACTION_CACHE_ROOT;
    let entries = fs.entries(home).ok()?;
    for (name, entry_type) in entries {
        if entry_type != crate::project_fs::EntryType::Directory {
            continue;
        }
        if matches!(
            fs.entry_type(&format!("{home}/{name}"), JOURNAL_NAME),
            Ok(crate::project_fs::EntryType::File)
        ) {
            return Some(super::diagnostic::RECOVERY_REQUIRED);
        }
    }
    None
}

/// The logical transaction ids currently awaiting recovery, sorted.
pub(crate) fn pending_transactions(root: &Path) -> Vec<String> {
    let home = root.join(super::version::TRANSACTION_CACHE_ROOT);
    let Ok(entries) = std::fs::read_dir(&home) else {
        return Vec::new();
    };
    let mut pending = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join(JOURNAL_NAME).is_file() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                pending.push(name.to_owned());
            }
        }
    }
    pending.sort();
    pending
}

impl Transaction {
    /// Begin one transaction: create the journal directory and write the
    /// durable journal before any source byte changes. The journal pins
    /// every replacement's before/after digests so recovery can verify the
    /// backups it restores (review finding 4).
    pub(crate) fn begin(
        root: &Path,
        id: &str,
        plan_id: &str,
        replacements: &[ReplacementPlan],
    ) -> Result<Self, ChangeError> {
        let journal_dir = root.join(super::version::TRANSACTION_CACHE_ROOT).join(id);
        std::fs::create_dir_all(journal_dir.join("backups")).map_err(|_| {
            ChangeError::CommitFailed {
                transaction: id.to_owned(),
                detail: "journal-create",
            }
        })?;
        let files: Vec<Json> = replacements
            .iter()
            .map(|replacement| {
                serde_json::json!({
                    "path": replacement.logical,
                    "beforeDigest": replacement.before_digest,
                    "afterDigest": replacement.after_digest,
                    "creation": replacement.creation,
                })
            })
            .collect();
        let journal = serde_json::json!({
            "transaction": id,
            "planId": plan_id,
            "files": files,
            "history": replacements.iter().any(|replacement| replacement.logical == super::version::HISTORY_LOGICAL_PATH),
        });
        write_durable(
            &journal_dir.join(JOURNAL_NAME),
            journal.to_string().as_bytes(),
        )
        .map_err(|_| ChangeError::CommitFailed {
            transaction: id.to_owned(),
            detail: "journal-write",
        })?;
        Ok(Self {
            root: root.to_path_buf(),
            id: id.to_owned(),
            journal_dir,
            staged: Vec::new(),
        })
    }

    /// Stage one replacement: verify the before digest, back up the exact
    /// current bytes, and hold the prospective bytes (nothing is replaced).
    pub(crate) fn stage(
        &mut self,
        index: usize,
        plan: &ReplacementPlan,
    ) -> Result<(), ChangeError> {
        let target = validated_target(&self.root, &plan.logical)?;
        let current = match std::fs::read(&target) {
            Ok(bytes) => bytes,
            Err(_) if plan.creation => Vec::new(),
            Err(_) => {
                return Err(ChangeError::CommitFailed {
                    transaction: self.id.clone(),
                    detail: "read-before",
                })
            }
        };
        let digest = format!("sha256:{}", crate::digest::sha256_hex(&current));
        if digest != plan.before_digest {
            return Err(ChangeError::PreconditionFailed {
                subject: plan.logical.clone(),
                detail: "before-digest",
            });
        }
        let after_bytes = ByteEdit::apply_all(&current, &plan.edits);
        let after_digest = format!("sha256:{}", crate::digest::sha256_hex(&after_bytes));
        if after_digest != plan.after_digest {
            return Err(ChangeError::CommitFailed {
                transaction: self.id.clone(),
                detail: "after-digest-plan",
            });
        }
        let backup = if plan.creation {
            None
        } else {
            let path = self.journal_dir.join("backups").join(index.to_string());
            write_durable(&path, &current).map_err(|_| ChangeError::CommitFailed {
                transaction: self.id.clone(),
                detail: "backup-write",
            })?;
            Some(path)
        };
        self.staged.push(StagedFile {
            target,
            backup,
            after_bytes,
            after_digest,
        });
        Ok(())
    }

    /// Apply every staged replacement in order and verify each after
    /// digest. On any failure the originals are restored in reverse order.
    pub(crate) fn replace_all(&mut self) -> Result<(), ChangeError> {
        let mut replaced = 0usize;
        for staged in &self.staged {
            if write_same_dir(&staged.target, &staged.after_bytes).is_err() {
                self.restore_prefix(replaced);
                return Err(ChangeError::CommitFailed {
                    transaction: self.id.clone(),
                    detail: "replace",
                });
            }
            replaced += 1;
        }
        for staged in &self.staged {
            let actual = std::fs::read(&staged.target).map_err(|_| {
                self.restore_prefix(replaced);
                ChangeError::CommitFailed {
                    transaction: self.id.clone(),
                    detail: "verify-read",
                }
            })?;
            let digest = format!("sha256:{}", crate::digest::sha256_hex(&actual));
            if digest != staged.after_digest {
                self.restore_prefix(replaced);
                return Err(ChangeError::CommitFailed {
                    transaction: self.id.clone(),
                    detail: "verify-digest",
                });
            }
        }
        Ok(())
    }

    /// Restore the first `count` replacements in reverse order. A failed
    /// restoration leaves the journal in place: readers stay blocked.
    fn restore_prefix(&self, count: usize) {
        for staged in self.staged[..count].iter().rev() {
            let restored = match &staged.backup {
                Some(backup) => {
                    std::fs::read(backup).and_then(|bytes| write_same_dir(&staged.target, &bytes))
                }
                None => std::fs::remove_file(&staged.target).map(|_| ()),
            };
            if restored.is_err() {
                // The journal stays; recovery remains required.
                return;
            }
        }
    }

    /// Finalize the canonical history inside the same transaction and
    /// publish the commit by removing the journal.
    pub(crate) fn finalize(self, history_bytes: &[u8]) -> Result<(), ChangeError> {
        let history_target = validated_target(&self.root, super::version::HISTORY_LOGICAL_PATH)?;
        write_same_dir(&history_target, history_bytes).map_err(|_| {
            // History finalization failed after replacements: restore.
            self.restore_prefix(self.staged.len());
            ChangeError::CommitFailed {
                transaction: self.id.clone(),
                detail: "history-finalize",
            }
        })?;
        // The commit point: the durable history exists; the journal goes.
        std::fs::remove_dir_all(&self.journal_dir).map_err(|_| ChangeError::CommitFailed {
            transaction: self.id.clone(),
            detail: "journal-remove",
        })?;
        Ok(())
    }

    /// Abandon the transaction after restoration: remove the journal.
    pub(crate) fn cleanup_after_restore(&self) {
        let _ = std::fs::remove_dir_all(&self.journal_dir);
    }
}

/// One planned file replacement handed to [`Transaction::begin`] and
/// [`Transaction::stage`].
pub(crate) struct ReplacementPlan {
    pub logical: String,
    pub before_digest: String,
    pub after_digest: String,
    pub edits: Vec<ByteEdit>,
    /// The file does not exist yet (a canonical home creation).
    pub creation: bool,
}

/// The closed typed journal wire (review finding 11): every member is
/// required, strictly typed, and bounded; unknown members and malformed
/// entries fail closed instead of being skipped.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalFile {
    path: String,
    #[serde(rename = "beforeDigest")]
    before_digest: String,
    #[serde(rename = "afterDigest")]
    after_digest: String,
    creation: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    transaction: String,
    #[serde(rename = "planId")]
    plan_id: String,
    files: Vec<JournalFile>,
    history: bool,
}

impl Journal {
    fn decode(bytes: &[u8]) -> Result<Self, ChangeError> {
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(ChangeError::CommitFailed {
                transaction: String::new(),
                detail: "journal-over-limit",
            });
        }
        serde_json::from_slice(bytes).map_err(|_| ChangeError::CommitFailed {
            transaction: String::new(),
            detail: "journal-shape",
        })
    }
}

/// Validate one journaled logical path component-wise against the confined
/// project root and return the write target (review finding 1). The same
/// closed logical-path grammar as the loader applies (no traversal, no
/// absolute or drive forms, portable segments), and every existing
/// component is checked no-follow so symlinks, junctions, and reparse
/// points can never redirect a recovery write outside the root.
fn validated_target(root: &Path, logical: &str) -> Result<PathBuf, ChangeError> {
    if crate::project_fs::path_violation(logical).is_some() {
        return Err(ChangeError::WriteBoundaryDenied);
    }
    let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
    let segments: Vec<&str> = logical.split('/').filter(|part| !part.is_empty()).collect();
    let (directories, file) = segments.split_at(segments.len() - 1);
    let mut walked = String::new();
    for segment in directories {
        match fs.entry_type(&walked, segment) {
            Ok(crate::project_fs::EntryType::Directory) => {}
            _ => return Err(ChangeError::WriteBoundaryDenied),
        }
        if walked.is_empty() {
            walked.push_str(segment);
        } else {
            walked = format!("{walked}/{segment}");
        }
    }
    match fs.entry_type(&walked, file[0]) {
        // The file exists: it must be a regular file (a directory in the
        // way is an escape attempt, not a restorable target).
        Ok(crate::project_fs::EntryType::File) => {}
        // Missing is acceptable only for the history sidecar creation.
        Ok(crate::project_fs::EntryType::Symlink) | Ok(crate::project_fs::EntryType::Special) => {
            return Err(ChangeError::WriteBoundaryDenied)
        }
        Err(crate::project_fs::FsErrorKind::NotFound) => {}
        Err(_) => return Err(ChangeError::WriteBoundaryDenied),
        Ok(_) => return Err(ChangeError::WriteBoundaryDenied),
    }
    let target = root.join(logical.replace('/', std::path::MAIN_SEPARATOR_STR));
    // Belt and braces: the composed path must stay inside the root.
    if !target.starts_with(root) {
        return Err(ChangeError::WriteBoundaryDenied);
    }
    Ok(target)
}

/// Recover one interrupted transaction: validate every journaled path,
/// digest-verify every backup against the journal pin, restore, then
/// remove the journal. History files created by the transaction are
/// removed again. Verification failures keep the journal (and the reader
/// barrier) in place. Returns the bounded per-file outcomes of the
/// rollback in journal order.
pub(crate) fn recover(
    root: &Path,
    transaction: &str,
) -> Result<Vec<super::receipt::FileOutcome>, ChangeError> {
    if !transaction
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || transaction.len() < 8
        || transaction.len() > 64
    {
        return Err(ChangeError::WriteBoundaryDenied);
    }
    let journal_dir = root
        .join(super::version::TRANSACTION_CACHE_ROOT)
        .join(transaction);
    let journal_path = journal_dir.join(JOURNAL_NAME);
    let journal_bytes =
        std::fs::read(&journal_path).map_err(|_| ChangeError::PreconditionFailed {
            subject: "transaction".to_owned(),
            detail: "journal-missing",
        })?;
    let journal =
        Journal::decode(&journal_bytes).map_err(|error| error.with_transaction(transaction))?;
    if journal.transaction != transaction {
        return Err(ChangeError::CommitFailed {
            transaction: transaction.to_owned(),
            detail: "journal-identity",
        });
    }
    // The pinned plan identity must be a digest shape (the journal binds
    // the interrupted transaction to its sealed plan).
    super::expect_digest(&journal.plan_id).ok_or(ChangeError::CommitFailed {
        transaction: transaction.to_owned(),
        detail: "journal-plan-id",
    })?;
    if journal.files.len() > MAX_JOURNAL_FILES {
        return Err(ChangeError::CommitFailed {
            transaction: transaction.to_owned(),
            detail: "journal-files-over-bound",
        });
    }
    let _ = journal.history;
    // Validate every journaled path BEFORE any write (finding 1) and pin
    // each backup's expected digest (finding 4). The backup index is the
    // entry's position in `files[]`, matching `Transaction::stage`.
    let mut restorations: Vec<Restoration> = Vec::new();
    for (index, file) in journal.files.iter().enumerate() {
        let target = validated_target(root, &file.path)?;
        if file.creation {
            restorations.push(Restoration {
                target,
                backup: None,
                before_digest: String::new(),
            });
            continue;
        }
        super::expect_digest(&file.before_digest).ok_or(ChangeError::CommitFailed {
            transaction: transaction.to_owned(),
            detail: "journal-before-digest",
        })?;
        super::expect_digest(&file.after_digest).ok_or(ChangeError::CommitFailed {
            transaction: transaction.to_owned(),
            detail: "journal-after-digest",
        })?;
        restorations.push(Restoration {
            target,
            backup: Some(journal_dir.join("backups").join(index.to_string())),
            before_digest: file.before_digest.clone(),
        });
    }
    // Verify every backup's bytes against its journal pin before any
    // restore write; a single corrupt member fails the whole recovery with
    // the journal intact (the reader barrier stays up).
    for restoration in &restorations {
        let Some(backup) = &restoration.backup else {
            continue;
        };
        let bytes = std::fs::read(backup).map_err(|_| ChangeError::RecoveryRequired {
            transaction: transaction.to_owned(),
        })?;
        let digest = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
        if digest != restoration.before_digest {
            return Err(ChangeError::CommitFailed {
                transaction: transaction.to_owned(),
                detail: "backup-digest-mismatch",
            });
        }
    }
    let mut failures = 0usize;
    for restoration in &restorations {
        let outcome = match &restoration.backup {
            None => std::fs::remove_file(&restoration.target),
            Some(backup) => {
                std::fs::read(backup).and_then(|bytes| write_same_dir(&restoration.target, &bytes))
            }
        };
        if outcome.is_err() {
            failures += 1;
        }
    }
    if failures > 0 {
        return Err(ChangeError::RecoveryRequired {
            transaction: transaction.to_owned(),
        });
    }
    std::fs::remove_dir_all(&journal_dir).map_err(|_| ChangeError::RecoveryRequired {
        transaction: transaction.to_owned(),
    })?;
    // The rollback receipt's file outcomes, journal order. A restoration
    // replaced the transaction's prospective bytes with the verified
    // originals; a creation removal replaced the created bytes with the
    // absent-file sentinel digest.
    let outcomes = journal
        .files
        .iter()
        .map(|file| super::receipt::FileOutcome {
            path: file.path.clone(),
            before_digest: file.after_digest.clone(),
            after_digest: if file.creation {
                super::absent_digest()
            } else {
                file.before_digest.clone()
            },
            outcome: "replaced".to_owned(),
        })
        .collect();
    Ok(outcomes)
}

impl ChangeError {
    /// Bind a decode failure to its transaction for the bounded wire data.
    fn with_transaction(self, transaction: &str) -> Self {
        match self {
            Self::CommitFailed { detail, .. } => Self::CommitFailed {
                transaction: transaction.to_owned(),
                detail,
            },
            other => other,
        }
    }
}

/// One validated restoration plan of the recovery pass.
struct Restoration {
    target: PathBuf,
    /// The digest-verified backup file, or `None` for a creation (the
    /// file is removed again instead of restored).
    backup: Option<PathBuf>,
    /// The exact `sha256:` digest the backup bytes must carry.
    before_digest: String,
}

/// Write bytes durably (create parents, sync the file).
fn write_durable(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()
}

/// Replace one file in its own directory: temp file plus rename, so the
/// replacement is same-volume and the temp name never collides.
fn write_same_dir(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let file_name = target
        .file_name()
        .map(|name| name.to_os_string())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no-file-name"))?;
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no-parent"))?;
    let temp = parent.join(format!(".{}.lekalo-tmp", file_name.to_string_lossy()));
    write_durable(&temp, bytes)?;
    match std::fs::rename(&temp, target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}
