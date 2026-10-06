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

use std::path::{Path, PathBuf};

use super::diagnostic::ChangeError;
use super::edit::ByteEdit;

/// The journal file name inside one transaction directory.
const JOURNAL_NAME: &str = "journal.json";

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
}

impl WriterLock {
    /// Acquire the change writer lock or report the contention.
    pub(crate) fn acquire(root: &Path) -> Result<Self, ChangeError> {
        let cache = root.join(".lekalo/cache/change");
        std::fs::create_dir_all(&cache).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let path = cache.join("writer.lock");
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Ok(Self { path: Some(path) }),
            Err(_) => Err(ChangeError::PreconditionFailed {
                subject: "writer-lock".to_owned(),
                detail: "held",
            }),
        }
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
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
    /// durable journal before any source byte changes.
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
        let files: Vec<String> = replacements
            .iter()
            .map(|replacement| replacement.logical.clone())
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
        let target = self
            .root
            .join(plan.logical.replace('/', std::path::MAIN_SEPARATOR_STR));
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
        let mut after_bytes = current.clone();
        for edit in &plan.edits {
            after_bytes = edit.apply(&after_bytes);
        }
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
            if let Err(_) = write_same_dir(&staged.target, &staged.after_bytes) {
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
        let history_target = self
            .root
            .join(super::version::HISTORY_LOGICAL_PATH.replace('/', std::path::MAIN_SEPARATOR_STR));
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

/// Recover one interrupted transaction: restore every journaled file from
/// its verified backup, then remove the journal. History files created by
/// the transaction are removed again.
pub(crate) fn recover(root: &Path, transaction: &str) -> Result<(), ChangeError> {
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
    let journal: serde_json::Value =
        serde_json::from_slice(&journal_bytes).map_err(|_| ChangeError::CommitFailed {
            transaction: transaction.to_owned(),
            detail: "journal-shape",
        })?;
    let files = journal
        .get("files")
        .and_then(serde_json::Value::as_array)
        .ok_or(ChangeError::CommitFailed {
            transaction: transaction.to_owned(),
            detail: "journal-files",
        })?;
    let mut failures = 0usize;
    for (index, file) in files.iter().enumerate() {
        let Some(logical) = file.as_str() else {
            continue;
        };
        let target = root.join(logical.replace('/', std::path::MAIN_SEPARATOR_STR));
        let backup = journal_dir.join("backups").join(index.to_string());
        if backup.is_file() {
            match std::fs::read(&backup).and_then(|bytes| write_same_dir(&target, &bytes)) {
                Ok(()) => {}
                Err(_) => failures += 1,
            }
        } else {
            // A creation without backup is removed again.
            let _ = std::fs::remove_file(&target);
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
    Ok(())
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
