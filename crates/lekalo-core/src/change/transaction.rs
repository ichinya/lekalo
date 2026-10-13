//! The durable change transaction (issue #73).
//!
//! The transaction applies only a service-created sealed plan. Under a
//! project-scoped writer lock it rechecks the idempotency ledger and every
//! before digest, stages a durable journal plus verified backups, applies
//! same-directory replacements, verifies every after digest, finalizes the
//! canonical history in the same transaction, and then publishes the commit
//! marker before removing the journal. Ordinary failure restores all
//! originals in reverse order; a crash leaves a recoverable journal that
//! blocks every project reader until explicit `change recover` succeeds.
//!
//! Recovery safety (review rounds 1 and 2): every journaled path is
//! validated against the confined project root component-wise with no-follow
//! checks before any write; each backup is digest-verified against the
//! `beforeDigest` pinned in the journal before restoration; the journal
//! decodes into a closed typed structure that fails on malformed members;
//! the cache home itself is validated no-follow so a link planted after
//! preparation cannot redirect transaction or recovery writes; temp files
//! carry unpredictable names and are created exclusively; and the whole
//! journal models three durable states (`staging`, `replacing`, `committed`)
//! so every real crash window recovers idempotently (round 2, findings 2-5).
//!
//! Writer serialization (round 2, finding 1) is an OS-held lock: the holder
//! keeps an open exclusive handle for the whole critical section, so a live
//! writer is always detectable without inferring liveness from process
//! metadata — a dead owner's handle is closed by the OS and the lock is
//! provably free. Unknown ownership always denies; recovery never steals a
//! lock it cannot prove free.
//!
//! Round 3 hardening: the journal's ENTIRE target set is validated against
//! the canonical homes the structure owner admits before any recovery
//! write, so a forged journal can no longer claim an unrelated project
//! file (finding 1); every target is preflighted against the states the
//! transaction can explain before any restoration, so user edits made
//! after an interrupted transaction survive and the barrier is retained
//! on drift (finding 2); a durably committed recovery completes its
//! cleanup before reporting success, so the reader barrier can never
//! outlive a committed receipt (finding 3); and the read-only pending
//! verdict reuses the same closed decode + admitted-target gate for the
//! receipt surface (finding 6).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;
use serde_json::Value as Json;

use super::diagnostic::ChangeError;
use crate::project_fs::{EntryType, Fs, FsErrorKind};

/// The journal file name inside one transaction directory.
const JOURNAL_NAME: &str = "journal.json";

/// The durable pending-registry file name under the `.lekalo` home (its
/// integrity is anchored OUTSIDE the cache home, round 5, finding 1: a
/// journal planted into `.lekalo/cache` alone can no longer mint
/// recovery authority).
const PENDING_NAME: &str = "pending-transactions.json";

/// The closed wire identity of the journal and of the pending registry.
const JOURNAL_SCHEMA_VERSION: &str = "lekalo/change-journal/1";
const PENDING_SCHEMA_VERSION: &str = "lekalo/change-pending/1";

/// Maximum pending-registry bytes accepted before decoding.
const MAX_PENDING_BYTES: usize = 64 * 1024;

/// Maximum records accepted in one pending registry.
const MAX_PENDING_RECORDS: usize = 64;

/// The writer lock file name inside the change cache.
const LOCK_NAME: &str = "writer.lock";

/// The backups directory name inside one transaction directory.
const BACKUPS_NAME: &str = "backups";

/// Maximum journal bytes accepted before decoding.
const MAX_JOURNAL_BYTES: usize = 64 * 1024;

/// Maximum journaled file entries per transaction.
const MAX_JOURNAL_FILES: usize = 1024;

/// The journal state before any backup has been staged: no source byte has
/// been touched, so recovery aborts the transaction without restoring.
pub(crate) const STATE_STAGING: &str = "staging";

/// The journal state once every backup exists: replacements may have
/// started, so recovery restores from the verified backups.
pub(crate) const STATE_REPLACING: &str = "replacing";

/// The journal state after the canonical history was durably written: the
/// transaction is committed; recovery only verifies and cleans up.
pub(crate) const STATE_COMMITTED: &str = "committed";

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
    journal: Json,
    staged: Vec<StagedFile>,
}

/// The project-scoped writer lock: exclusive creation plus an OS-held
/// handle, removed on drop.
pub(crate) struct WriterLock {
    path: PathBuf,
    file: Option<std::fs::File>,
}

impl WriterLock {
    /// Acquire the change writer lock, taking over a provably dead owner's
    /// artifact when the operating system has released it (review round 2,
    /// finding 1). The lock is an exclusive-create file PLUS an OS-held
    /// handle (`share_mode(0)` on Windows, `flock` on Unix) kept open for
    /// the whole critical section:
    ///
    /// - no lock file: created and held;
    /// - lock file with a live holder: the shared-open or flock fails and
    ///   the caller is denied — a live writer is never misclassified dead;
    /// - lock file of a dead owner: the OS closed the handle, so the open
    ///   or flock succeeds and recovery takes the artifact over — a crash
    ///   can never brick recovery behind its own lock artifact;
    /// - any other error (including a planted link at the lock path): fail
    ///   closed, never reclaim on unknown ownership.
    pub(crate) fn acquire(root: &Path) -> Result<Self, ChangeError> {
        let cache = validated_cache_dir(root)?;
        std::fs::create_dir_all(&cache).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let path = cache.join(LOCK_NAME);
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if path_is_link(&metadata) {
                // A link at the lock path is an escape attempt, never a
                // lock: refuse instead of following it.
                return Err(ChangeError::PreconditionFailed {
                    subject: "writer-lock".to_owned(),
                    detail: "held",
                });
            }
        }
        let file = open_held(&path).map_err(|_| ChangeError::PreconditionFailed {
            subject: "writer-lock".to_owned(),
            detail: "held",
        })?;
        // Holder identity is informational only; the OS handle is the
        // ownership oracle.
        let _ = record_holder(&file);
        Ok(Self {
            path,
            file: Some(file),
        })
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        // Close the OS handle first so Windows allows the removal.
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Windows: open with no sharing so any second open fails while a live
/// holder exists; Unix: open then take a non-blocking exclusive `flock`.
fn open_held(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(path)?;
        Ok(file)
    }
    #[cfg(unix)]
    {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
        Ok(file)
    }
}

/// Best-effort holder record inside the lock file (informational only).
fn record_holder(file: &std::fs::File) -> Option<()> {
    let mut file = file;
    use std::io::Write;
    file.set_len(0).ok()?;
    let record = serde_json::json!({
        "pid": std::process::id(),
        "started": unix_time(),
    });
    file.write_all(record.to_string().as_bytes()).ok()?;
    file.sync_all().ok()
}

fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Whether one metadata describes a symlink/junction/reparse point (the
/// authoritative link signal on both platforms).
fn path_is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(unix)]
    {
        metadata.file_type().is_symlink()
    }
}

/// Validate the change cache home component-wise with no-follow checks and
/// return its on-disk path (round 2, finding 2). Every existing component
/// of `.lekalo/cache/change` must be a real directory; a link or special
/// entry planted after preparation refuses every transaction write that
/// would pass through it. Missing components are created by the caller.
fn validated_cache_dir(root: &Path) -> Result<PathBuf, ChangeError> {
    let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
    let mut walked = String::new();
    for segment in [".lekalo", "cache", "change"] {
        match fs.entry_type(&walked, segment) {
            Ok(EntryType::Directory) => {}
            Ok(_) => return Err(ChangeError::WriteBoundaryDenied),
            Err(FsErrorKind::NotFound) => {}
            Err(_) => return Err(ChangeError::WriteBoundaryDenied),
        }
        walked = if walked.is_empty() {
            segment.to_owned()
        } else {
            format!("{walked}/{segment}")
        };
    }
    Ok(root.join(".lekalo").join("cache").join("change"))
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
/// Round 5, finding 4: the listing walks the cache home through the
/// no-follow `Fs` entry types — the same reader the barrier uses — so a
/// link-backed directory can never be listed as pending.
pub(crate) fn pending_transactions(root: &Path) -> Vec<String> {
    let home = super::version::TRANSACTION_CACHE_ROOT;
    let Ok(fs) = Fs::open(root) else {
        return Vec::new();
    };
    let Ok(entries) = fs.entries(home) else {
        return Vec::new();
    };
    let mut pending = Vec::new();
    for (name, entry_type) in entries {
        if entry_type != crate::project_fs::EntryType::Directory {
            continue;
        }
        if matches!(
            fs.entry_type(&format!("{home}/{name}"), JOURNAL_NAME),
            Ok(crate::project_fs::EntryType::File)
        ) {
            pending.push(name);
        }
    }
    pending.sort();
    pending
}

impl Transaction {
    /// Begin one transaction: durably record the pending provenance pin
    /// before publishing a journal (round 5, finding 1), in a FRESH
    /// directory (a pre-existing directory is planted material, never a
    /// transaction resume point), then write the durable journal —
    /// state `staging`, before any source byte changes. The journal pins
    /// every replacement's before/after digests so recovery can verify the
    /// backups it restores, and every member is mirrored by the pending
    /// record recovery verifies it against.
    pub(crate) fn begin(
        root: &Path,
        id: &str,
        plan_id: &str,
        replacements: &[ReplacementPlan],
    ) -> Result<Self, ChangeError> {
        let cache = validated_cache_dir(root)?;
        std::fs::create_dir_all(&cache).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let journal_dir = cache.join(id);
        match std::fs::symlink_metadata(&journal_dir) {
            // A pre-existing entry at the transaction home — real
            // directory, link, or file — is planted or leftover
            // material: a fresh transaction only ever begins into a
            // directory it creates itself (this also closes the
            // pre-created `backups` sibling note: nothing pre-existing
            // survives to be followed).
            Ok(_) => return Err(ChangeError::WriteBoundaryDenied),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ChangeError::WriteBoundaryDenied),
        }
        let files: Vec<JournalFile> = replacements
            .iter()
            .map(|replacement| JournalFile {
                path: replacement.logical.clone(),
                before_digest: replacement.before_digest.clone(),
                after_digest: replacement.after_digest.clone(),
                creation: replacement.creation,
            })
            .collect();
        let history = replacements
            .iter()
            .any(|replacement| replacement.logical == super::version::HISTORY_LOGICAL_PATH);
        // The durable provenance pin precedes the journal: a crash in
        // between leaves at most a dangling record (not a reader barrier),
        // never a journal without provenance. A
        // record for this id can only be stale (its journal directory
        // does not exist), so it is replaced.
        let mut records = load_pending_registry(root)?;
        records.retain(|record| record.transaction != id);
        records.push(PendingRecord {
            transaction: id.to_owned(),
            plan_id: plan_id.to_owned(),
            state: STATE_STAGING.to_owned(),
            files: files.clone(),
            history,
        });
        std::fs::create_dir(&journal_dir).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        if std::fs::create_dir(journal_dir.join(BACKUPS_NAME)).is_err() {
            let _ = std::fs::remove_dir(&journal_dir);
            return Err(ChangeError::CommitFailed {
                transaction: id.to_owned(),
                detail: "journal-create",
            });
        }
        if let Err(error) = write_pending_registry(root, &records).map_err(bind_pending(id)) {
            let _ = std::fs::remove_dir_all(&journal_dir);
            return Err(error);
        }
        let files_wire: Vec<Json> = files.iter().map(journal_file_json).collect();
        let journal = serde_json::json!({
            "schemaVersion": JOURNAL_SCHEMA_VERSION,
            "transaction": id,
            "planId": plan_id,
            "state": STATE_STAGING,
            "files": files_wire,
            "history": history,
        });
        if write_same_dir(
            &journal_dir.join(JOURNAL_NAME),
            journal.to_string().as_bytes(),
        )
        .is_err()
        {
            if std::fs::remove_dir_all(&journal_dir).is_ok() {
                remove_pending_record(root, id);
            }
            return Err(ChangeError::CommitFailed {
                transaction: id.to_owned(),
                detail: "journal-write",
            });
        }
        Ok(Self {
            root: root.to_path_buf(),
            id: id.to_owned(),
            journal_dir,
            journal,
            staged: Vec::new(),
        })
    }

    /// Publish a durable journal state transition (`staging` ->
    /// `replacing` -> `committed`). Recovery keys every crash window off
    /// these states (round 2, finding 4). The journal is republished
    /// before the pending record, so a crash in between leaves the
    /// journal at most one step ahead — the only state divergence
    /// provenance accepts (round 5, finding 1).
    pub(crate) fn mark_state(&mut self, state: &str) -> Result<(), ChangeError> {
        if let Some(map) = self.journal.as_object_mut() {
            map.insert("state".to_owned(), Json::String(state.to_owned()));
        }
        write_same_dir(
            &self.journal_dir.join(JOURNAL_NAME),
            self.journal.to_string().as_bytes(),
        )
        .map_err(|_| ChangeError::CommitFailed {
            transaction: self.id.clone(),
            detail: "journal-state",
        })?;
        self.set_pending_state(state)?;
        Ok(())
    }

    /// Mirror one state transition into the durable pending record.
    fn set_pending_state(&self, state: &str) -> Result<(), ChangeError> {
        let mut records = load_pending_registry(&self.root)?;
        let Some(record) = records
            .iter_mut()
            .find(|record| record.transaction == self.id)
        else {
            return Err(ChangeError::CommitFailed {
                transaction: self.id.clone(),
                detail: "journal-provenance",
            });
        };
        record.state = state.to_owned();
        write_pending_registry(&self.root, &records).map_err(bind_pending(&self.id))
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
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && plan.creation => {
                Vec::new()
            }
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
        let after_digest = format!("sha256:{}", crate::digest::sha256_hex(&plan.after_bytes));
        if after_digest != plan.after_digest {
            return Err(ChangeError::CommitFailed {
                transaction: self.id.clone(),
                detail: "after-digest-plan",
            });
        }
        let backup = if plan.creation {
            None
        } else {
            validate_backup(&self.root, &self.id, index, false)?;
            let path = self.journal_dir.join(BACKUPS_NAME).join(index.to_string());
            write_same_dir(&path, &current).map_err(|_| ChangeError::CommitFailed {
                transaction: self.id.clone(),
                detail: "backup-write",
            })?;
            Some(path)
        };
        self.staged.push(StagedFile {
            target,
            backup,
            after_bytes: plan.after_bytes.clone(),
            after_digest,
        });
        Ok(())
    }

    /// Apply every staged replacement in order and verify each after
    /// digest. On any failure the originals are restored in reverse order;
    /// a restoration that itself fails reports `RecoveryRequired` so the
    /// caller retains the journal and the reader barrier (round 2,
    /// finding 5) instead of deleting the recovery material.
    pub(crate) fn replace_all(&mut self) -> Result<(), ChangeError> {
        let mut replaced = 0usize;
        for staged in &self.staged {
            if write_same_dir(&staged.target, &staged.after_bytes).is_err() {
                return if self.restore_prefix(replaced) {
                    Err(ChangeError::CommitFailed {
                        transaction: self.id.clone(),
                        detail: "replace",
                    })
                } else {
                    Err(ChangeError::RecoveryRequired {
                        transaction: self.id.clone(),
                    })
                };
            }
            replaced += 1;
        }
        for staged in &self.staged {
            let verified = std::fs::read(&staged.target).map(|bytes| {
                format!("sha256:{}", crate::digest::sha256_hex(&bytes)) == staged.after_digest
            });
            match verified {
                Ok(true) => {}
                _ => {
                    return if self.restore_prefix(replaced) {
                        Err(ChangeError::CommitFailed {
                            transaction: self.id.clone(),
                            detail: if verified.is_err() {
                                "verify-read"
                            } else {
                                "verify-digest"
                            },
                        })
                    } else {
                        Err(ChangeError::RecoveryRequired {
                            transaction: self.id.clone(),
                        })
                    };
                }
            }
        }
        Ok(())
    }

    /// Restore the first `count` replacements in reverse order. Returns
    /// whether every restoration completed; a failed restoration leaves
    /// the journal in place so readers stay blocked and recovery remains
    /// possible (round 2, finding 5).
    fn restore_prefix(&self, count: usize) -> bool {
        for staged in self.staged[..count].iter().rev() {
            let restored = match &staged.backup {
                Some(backup) => {
                    std::fs::read(backup).and_then(|bytes| write_same_dir(&staged.target, &bytes))
                }
                None => std::fs::remove_file(&staged.target).map(|_| ()),
            };
            if restored.is_err() {
                return false;
            }
        }
        true
    }

    /// Finalize the canonical history inside the same transaction, publish
    /// the durable `committed` marker, and only then remove the recovery
    /// material (round 2, finding 4: the commit is durably established
    /// before cleanup, so a crash in between recovers as committed).
    pub(crate) fn finalize(&mut self, history_bytes: &[u8]) -> Result<(), ChangeError> {
        let history_target = validated_target(&self.root, super::version::HISTORY_LOGICAL_PATH)?;
        if write_same_dir(&history_target, history_bytes).is_err() {
            // History finalization failed after replacements: restore.
            return if self.restore_prefix(self.staged.len()) {
                self.cleanup_after_restore();
                Err(ChangeError::CommitFailed {
                    transaction: self.id.clone(),
                    detail: "history-finalize",
                })
            } else {
                Err(ChangeError::RecoveryRequired {
                    transaction: self.id.clone(),
                })
            };
        }
        self.mark_state(STATE_COMMITTED)?;
        if std::fs::remove_dir_all(&self.journal_dir).is_err() {
            // Committed is durable; only the cleanup is outstanding, and
            // recovery finishes it (and reports committed).
            return Err(ChangeError::RecoveryRequired {
                transaction: self.id.clone(),
            });
        }
        // The journal is gone: its provenance pin has nothing left to
        // guard (a leftover record is inert — no journal can match it).
        remove_pending_record(&self.root, &self.id);
        Ok(())
    }

    /// Abandon the transaction after full restoration or a staging abort:
    /// remove the journal, every backup, and the provenance record. Only
    /// ever called when no restoration is outstanding.
    pub(crate) fn cleanup_after_restore(&self) {
        if std::fs::remove_dir_all(&self.journal_dir).is_ok() {
            remove_pending_record(&self.root, &self.id);
        }
    }
}

/// One planned file replacement handed to [`Transaction::begin`] and
/// [`Transaction::stage`].
pub(crate) struct ReplacementPlan {
    pub logical: String,
    pub before_digest: String,
    pub after_digest: String,
    /// The exact prospective bytes pinned at preparation time (re-verified
    /// against `after_digest` at staging). Sequential batches compose their
    /// per-operation edits into these bytes, so the transaction applies
    /// exactly what the sealed plan validated (round 2, finding 16).
    pub after_bytes: Vec<u8>,
    /// The file does not exist yet (a canonical home creation).
    pub creation: bool,
}

/// The closed typed journal wire: every member is required, strictly typed,
/// and bounded; unknown members and malformed entries fail closed instead
/// of being skipped. The durable `state` member models the three crash
/// windows recovery must distinguish (round 2, finding 4).
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
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    transaction: String,
    #[serde(rename = "planId")]
    plan_id: String,
    state: String,
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
        // Round 5, finding 4: the journal is forgeable material, so it
        // decodes through the shared duplicate-key-refusing parser (a
        // `{"state":"staging","state":"replacing"}` document must
        // never silently collapse to its last member) and pins its wire
        // identity with a `schemaVersion` const.
        let value = super::request::unique_json(bytes).map_err(|_| ChangeError::CommitFailed {
            transaction: String::new(),
            detail: "journal-shape",
        })?;
        let journal: Journal =
            serde_json::from_value(value).map_err(|_| ChangeError::CommitFailed {
                transaction: String::new(),
                detail: "journal-shape",
            })?;
        if journal.schema_version != JOURNAL_SCHEMA_VERSION {
            return Err(ChangeError::CommitFailed {
                transaction: String::new(),
                detail: "journal-schema-version",
            });
        }
        if !matches!(
            journal.state.as_str(),
            STATE_STAGING | STATE_REPLACING | STATE_COMMITTED
        ) {
            return Err(ChangeError::CommitFailed {
                transaction: String::new(),
                detail: "journal-state",
            });
        }
        Ok(journal)
    }
}

/// One durable pending-transaction record: the provenance pin a journal
/// must exactly reproduce before recovery trusts any of its self-declared
/// members (round 5, finding 1). The registry lives under `.lekalo` but
/// outside the cache home, so material planted into `.lekalo/cache` alone
/// — via clone, checkout, or any archive that preserves the tree — can
/// never mint overwrite or deletion authority.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingRecord {
    transaction: String,
    #[serde(rename = "planId")]
    plan_id: String,
    state: String,
    files: Vec<JournalFile>,
    history: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingRegistry {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    records: Vec<PendingRecord>,
}

/// The rank of one durable state in the fixed `staging` -> `replacing`
/// -> `committed` progression.
fn state_rank(state: &str) -> Option<u8> {
    match state {
        STATE_STAGING => Some(0),
        STATE_REPLACING => Some(1),
        STATE_COMMITTED => Some(2),
        _ => None,
    }
}

/// Load the durable pending registry; an absent file is an empty
/// registry, and anything malformed fails closed.
fn load_pending_registry(root: &Path) -> Result<Vec<PendingRecord>, ChangeError> {
    let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
    match fs.entry_type("", ".lekalo") {
        Ok(EntryType::Directory) => {}
        Err(FsErrorKind::NotFound) => return Ok(Vec::new()),
        _ => return Err(ChangeError::WriteBoundaryDenied),
    }
    match fs.entry_type(".lekalo", PENDING_NAME) {
        Ok(EntryType::File) => {}
        Err(FsErrorKind::NotFound) => return Ok(Vec::new()),
        _ => return Err(ChangeError::WriteBoundaryDenied),
    }
    let bytes = match fs.read_file_opt(".lekalo", PENDING_NAME, MAX_PENDING_BYTES) {
        Ok(Some(bytes)) => bytes,
        Ok(None) | Err(FsErrorKind::NotFound) => return Ok(Vec::new()),
        Err(_) => {
            return Err(ChangeError::CommitFailed {
                transaction: String::new(),
                detail: "pending-registry-read",
            })
        }
    };
    if bytes.len() > MAX_PENDING_BYTES {
        return Err(ChangeError::CommitFailed {
            transaction: String::new(),
            detail: "pending-registry-over-limit",
        });
    }
    let value = super::request::unique_json(&bytes).map_err(|_| ChangeError::CommitFailed {
        transaction: String::new(),
        detail: "pending-registry-shape",
    })?;
    let registry: PendingRegistry =
        serde_json::from_value(value).map_err(|_| ChangeError::CommitFailed {
            transaction: String::new(),
            detail: "pending-registry-shape",
        })?;
    if registry.schema_version != PENDING_SCHEMA_VERSION {
        return Err(ChangeError::CommitFailed {
            transaction: String::new(),
            detail: "pending-registry-schema-version",
        });
    }
    if registry.records.len() > MAX_PENDING_RECORDS {
        return Err(ChangeError::CommitFailed {
            transaction: String::new(),
            detail: "pending-registry-over-limit",
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    for record in &registry.records {
        if !seen.insert(&record.transaction) {
            return Err(ChangeError::CommitFailed {
                transaction: record.transaction.clone(),
                detail: "pending-registry-duplicate",
            });
        }
    }
    Ok(registry.records)
}

/// Durably write the pending registry (validated no-follow `.lekalo`
/// home, same-directory atomic replace). Every caller holds the writer
/// lock, so read-modify-write stays serialized.
fn write_pending_registry(root: &Path, records: &[PendingRecord]) -> Result<(), ChangeError> {
    let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
    match fs.entry_type("", ".lekalo") {
        Ok(EntryType::Directory) => {}
        Ok(_) => return Err(ChangeError::WriteBoundaryDenied),
        Err(FsErrorKind::NotFound) => {
            std::fs::create_dir(root.join(".lekalo"))
                .map_err(|_| ChangeError::WriteBoundaryDenied)?;
        }
        Err(_) => return Err(ChangeError::WriteBoundaryDenied),
    }
    match fs.entry_type(".lekalo", PENDING_NAME) {
        Ok(EntryType::File) | Err(FsErrorKind::NotFound) => {}
        _ => return Err(ChangeError::WriteBoundaryDenied),
    }
    let wire: Vec<Json> = records.iter().map(pending_record_json).collect();
    let document = serde_json::json!({
        "schemaVersion": PENDING_SCHEMA_VERSION,
        "records": wire,
    })
    .to_string();
    if records.len() > MAX_PENDING_RECORDS || document.len() > MAX_PENDING_BYTES {
        return Err(ChangeError::CommitFailed {
            transaction: String::new(),
            detail: "pending-registry-over-limit",
        });
    }
    write_same_dir(
        &root.join(".lekalo").join(PENDING_NAME),
        document.as_bytes(),
    )
    .map_err(|_| ChangeError::CommitFailed {
        transaction: String::new(),
        detail: "pending-registry-write",
    })
}

/// The canonical JSON projection of one pending record.
fn pending_record_json(record: &PendingRecord) -> Json {
    serde_json::json!({
        "transaction": record.transaction,
        "planId": record.plan_id,
        "state": record.state,
        "files": record.files.iter().map(journal_file_json).collect::<Vec<Json>>(),
        "history": record.history,
    })
}

/// The canonical JSON projection of one journaled file pin.
fn journal_file_json(file: &JournalFile) -> Json {
    serde_json::json!({
        "path": file.path,
        "beforeDigest": file.before_digest,
        "afterDigest": file.after_digest,
        "creation": file.creation,
    })
}

/// Best-effort removal of one transaction's pending record (the journal
/// is gone: no reader is gated by a leftover record. Its immutable pins
/// still cannot authorize bytes outside the original transaction).
fn remove_pending_record(root: &Path, transaction: &str) {
    let Ok(mut records) = load_pending_registry(root) else {
        return;
    };
    let before = records.len();
    records.retain(|record| record.transaction != transaction);
    if records.len() != before {
        let _ = write_pending_registry(root, &records);
    }
}

/// Verify one journal against its durable pending record (round 5,
/// finding 1): the transaction id, plan id, history flag, and the ENTIRE
/// file pin set (path, before/after digests, creation flags, in order)
/// must match exactly, and the journal's state may be at most one step
/// ahead of the record's (the journal is republished before the record on
/// every state transition; a journal claiming `committed` over a
/// `staging` record — or over no record at all — is forged provenance and
/// fails closed with the barrier retained).
fn verify_journal_provenance(root: &Path, journal: &Journal) -> Result<(), ChangeError> {
    let records = load_pending_registry(root)?;
    let record = records
        .iter()
        .find(|record| record.transaction == journal.transaction)
        .ok_or(ChangeError::CommitFailed {
            transaction: journal.transaction.clone(),
            detail: "journal-provenance",
        })?;
    let proven = record.plan_id == journal.plan_id
        && record.history == journal.history
        && record.files.len() == journal.files.len()
        && record
            .files
            .iter()
            .zip(journal.files.iter())
            .all(|(pinned, claimed)| {
                pinned.path == claimed.path
                    && pinned.before_digest == claimed.before_digest
                    && pinned.after_digest == claimed.after_digest
                    && pinned.creation == claimed.creation
            });
    let states_explained = match (state_rank(&record.state), state_rank(&journal.state)) {
        (Some(record_rank), Some(journal_rank)) => {
            journal_rank == record_rank || journal_rank == record_rank + 1
        }
        _ => false,
    };
    if !proven || !states_explained {
        return Err(ChangeError::CommitFailed {
            transaction: journal.transaction.clone(),
            detail: "journal-provenance",
        });
    }
    Ok(())
}

/// Verify that the canonical history durably names one transaction as
/// committed (round 5, finding 1): the receipt projection of some record
/// carries the transaction's id with committed state. An absent,
/// unreadable, or unverified ledger fails closed.
fn verify_history_custody(root: &Path, transaction: &str) -> Result<(), ChangeError> {
    let bytes = std::fs::read(
        root.join(super::version::HISTORY_LOGICAL_PATH.replace('/', std::path::MAIN_SEPARATOR_STR)),
    )
    .map_err(|_| ChangeError::RecoveryRequired {
        transaction: transaction.to_owned(),
    })?;
    let history =
        super::receipt::History::decode(&bytes).map_err(|_| ChangeError::RecoveryRequired {
            transaction: transaction.to_owned(),
        })?;
    super::verify_history_project(root, &history)?;
    let named = history.records.iter().any(|record| {
        record
            .receipt
            .get("transaction")
            .and_then(|wire| wire.get("id"))
            .and_then(Json::as_str)
            == Some(transaction)
            && record
                .receipt
                .get("transaction")
                .and_then(|wire| wire.get("state"))
                .and_then(Json::as_str)
                == Some(STATE_COMMITTED)
    });
    if !named {
        return Err(ChangeError::RecoveryRequired {
            transaction: transaction.to_owned(),
        });
    }
    Ok(())
}

/// Validate one journaled logical path component-wise against the confined
/// project root and return the write target. The same closed logical-path
/// grammar as the loader applies (no traversal, no absolute or drive
/// forms, portable segments), and every existing component is checked
/// no-follow so symlinks, junctions, and reparse points can never redirect
/// a transaction or recovery write outside the root.
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
            Ok(EntryType::Directory) => {}
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
        Ok(EntryType::File) => {}
        // Missing is acceptable only for the history sidecar creation.
        Ok(EntryType::Symlink) | Ok(EntryType::Special) => {
            return Err(ChangeError::WriteBoundaryDenied)
        }
        Err(FsErrorKind::NotFound) => {}
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

/// The outcome of one successful recovery pass: the durable journal state
/// the transaction was found in, and the bounded per-file outcomes of the
/// rollback (or of the committed verification).
pub(crate) struct RecoveryResult {
    /// Whether the transaction was found (and verified) durably committed.
    pub committed: bool,
    /// The per-file outcomes in journal order.
    pub files: Vec<super::receipt::FileOutcome>,
}

/// Recover one interrupted transaction by its durable journal state
/// (round 2, findings 3-5):
///
/// - `staging`: no replacement started; the transaction aborts without
///   touching a single source byte (a real interruption during staging
///   recovers cleanly instead of bricking the project);
/// - `replacing`: every backup is digest-verified, every target is
///   preflighted against the states this transaction can explain, and
///   replaced files are restored (idempotently — already-restored files
///   are skipped); creations are removed again only when the current
///   bytes provably are the bytes this transaction created and the
///   destination is an admitted canonical home (round 2, finding 3;
///   round 3, finding 2 adds the whole-set preflight so a later drift
///   conflict is discovered before any earlier restoration writes);
/// - `committed`: the commit is durable; recovery verifies every target
///   against its pinned after digest and finishes the cleanup — the
///   barrier is only cleared after the cleanup actually completed
///   (round 3, finding 3).
///
/// Verification failures, drifted files, and forged entries keep the
/// journal (and the reader barrier) in place and fail closed.
pub(crate) fn recover(root: &Path, transaction: &str) -> Result<RecoveryResult, ChangeError> {
    let (journal_dir, journal) = load_validated_journal(root, transaction)?;
    let restorations = restorations_of(root, &journal_dir, &journal)?;
    match journal.state.as_str() {
        STATE_STAGING => recover_staging(root, transaction, &restorations),
        STATE_REPLACING => recover_replacing(root, transaction, &journal_dir, &restorations),
        STATE_COMMITTED => recover_committed(root, transaction, &journal_dir, &restorations),
        _ => Err(ChangeError::CommitFailed {
            transaction: transaction.to_owned(),
            detail: "journal-state",
        }),
    }
}

/// Load and fully validate one pending transaction's journal without
/// writing: the closed decoder, the transaction/plan identity pins, the
/// file bound, the admitted-target-set validation (round 3, finding 1),
/// and the history-flag consistency. Every other recovery-side consumer
/// (the read-only receipt verdict included) reuses exactly this gate.
fn load_validated_journal(
    root: &Path,
    transaction: &str,
) -> Result<(PathBuf, Journal), ChangeError> {
    if !transaction
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || transaction.len() < 8
        || transaction.len() > 64
    {
        return Err(ChangeError::WriteBoundaryDenied);
    }
    // The cache home is re-validated no-follow: a link planted at any
    // component after preparation refuses every recovery write.
    validated_cache_dir(root)?;
    let journal_dir = root
        .join(super::version::TRANSACTION_CACHE_ROOT)
        .join(transaction);
    // Round 5, finding 1: the journal directory and the journal file
    // itself are validated no-follow through the same `Fs` reader the
    // barrier uses — a link-backed transaction home or journal never
    // becomes readable recovery material.
    {
        let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        let home = super::version::TRANSACTION_CACHE_ROOT;
        let missing = || ChangeError::PreconditionFailed {
            subject: "transaction".to_owned(),
            detail: "journal-missing",
        };
        match fs.entry_type(home, transaction) {
            Ok(EntryType::Directory) => {}
            Err(FsErrorKind::NotFound) => return Err(missing()),
            _ => return Err(ChangeError::WriteBoundaryDenied),
        }
        match fs.entry_type(&format!("{home}/{transaction}"), JOURNAL_NAME) {
            Ok(EntryType::File) => {}
            Err(FsErrorKind::NotFound) => return Err(missing()),
            _ => return Err(ChangeError::WriteBoundaryDenied),
        }
    }
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
    // Round 3, finding 1: the ENTIRE target set is validated against the
    // admitted canonical homes before any recovery write. Root
    // containment alone is not custody: a journal naming an unrelated
    // project file (existing or to-be-created) is a forged overwrite/
    // deletion authority and fails closed with the barrier retained.
    validate_journal_targets(root, &journal)?;
    // The journal's durable history flag must agree with the file set: a
    // forged combination fails closed before any restoration.
    if journal.history
        != journal
            .files
            .iter()
            .any(|file| file.path == super::version::HISTORY_LOGICAL_PATH)
    {
        return Err(ChangeError::CommitFailed {
            transaction: transaction.to_owned(),
            detail: "journal-history-flag",
        });
    }
    // Round 5, finding 1: every self-declared journal member is now
    // verified against the durable pending record anchored outside the
    // cache home. A journal without provenance — planted via clone,
    // checkout, or an archive that preserves the tree — has zero
    // recovery authority, no matter how self-consistent its digests and
    // backups are; the barrier stays up with the journal intact.
    verify_journal_provenance(root, &journal)?;
    Ok((journal_dir, journal))
}

/// The module directory and kind stem of one canonical kind-home path
/// (`lekalo/modules/<dir>/<stem>.yaml`), or `None` for any other shape.
fn module_kind_home(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("lekalo/modules/")?;
    let (dir, file) = rest.split_once('/')?;
    if dir.is_empty() || file.contains('/') {
        return None;
    }
    let stem = file.strip_suffix(".yaml")?;
    crate::project_fs::KIND_STEMS
        .iter()
        .any(|candidate| candidate == &stem)
        .then_some((dir, stem))
}

/// Whether one path is a module document home of an admitted module.
fn admitted_module_home(path: &str, modules: &[String]) -> bool {
    let Some(rest) = path.strip_prefix("lekalo/modules/") else {
        return false;
    };
    match rest.split_once('/') {
        Some((dir, "module.yaml")) => modules.iter().any(|module| module == dir),
        _ => false,
    }
}

/// Validate the journal's whole target set against the canonical homes
/// the structure owner admits right now (round 3, finding 1): every
/// existing-file entry must be an admitted Model document home or the
/// history sidecar; every creation entry must be the history sidecar or
/// an admitted module's kind home (the `definition:add` first-document
/// creation surface, round 3, finding 11); duplicate or aliased entries
/// refuse. An unvalidatable structure refuses everything.
fn validate_journal_targets(root: &Path, journal: &Journal) -> Result<(), ChangeError> {
    let modules = match crate::project_fs::Fs::validate_project(root) {
        crate::project_fs::StructureOutcome::Valid(report) => report.modules,
        _ => return Err(ChangeError::WriteBoundaryDenied),
    };
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for file in &journal.files {
        if !seen.insert(file.path.as_str()) {
            return Err(ChangeError::CommitFailed {
                transaction: journal.transaction.clone(),
                detail: "journal-target-duplicate",
            });
        }
        let admitted = if file.path == super::version::HISTORY_LOGICAL_PATH
            || file.path == "lekalo/project.yaml"
            || admitted_module_home(&file.path, &modules)
        {
            true
        } else {
            match module_kind_home(&file.path) {
                Some((dir, _)) => modules.iter().any(|module| module == dir),
                None => false,
            }
        };
        if !admitted {
            return Err(ChangeError::WriteBoundaryDenied);
        }
    }
    Ok(())
}

/// Validate backup components independently of the project-wide scan.
/// Staging permits absence; replacing requires an existing regular file.
fn validate_backup(
    root: &Path,
    transaction: &str,
    index: usize,
    required: bool,
) -> Result<(), ChangeError> {
    let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
    let home = format!("{}/{transaction}", super::version::TRANSACTION_CACHE_ROOT);
    match fs.entry_type(&home, BACKUPS_NAME) {
        Ok(EntryType::Directory) => {}
        _ => return Err(ChangeError::WriteBoundaryDenied),
    }
    match fs.entry_type(&format!("{home}/{BACKUPS_NAME}"), &index.to_string()) {
        Ok(EntryType::File) => Ok(()),
        Err(FsErrorKind::NotFound) if !required => Ok(()),
        Err(FsErrorKind::NotFound) => Err(ChangeError::RecoveryRequired {
            transaction: transaction.to_owned(),
        }),
        _ => Err(ChangeError::WriteBoundaryDenied),
    }
}

/// Build the validated restoration set of one journal: every target path
/// is component-wise validated against the confined root (no-follow),
/// creation destinations are restricted to the admitted history home or
/// an admitted kind home, and every digest pin is shape-checked.
fn restorations_of(
    root: &Path,
    journal_dir: &Path,
    journal: &Journal,
) -> Result<Vec<Restoration>, ChangeError> {
    let mut restorations: Vec<Restoration> = Vec::new();
    for (index, file) in journal.files.iter().enumerate() {
        let target = validated_target(root, &file.path)?;
        if file.creation {
            // A creation destination is restricted to the admitted history
            // home or an admitted module kind home (round 2, finding 3,
            // extended for the first-definition creation surface in round
            // 3, finding 11 — the whole-set admission already ran): any
            // other journaled creation is a forged deletion authority and
            // fails closed.
            let admitted_creation = file.path == super::version::HISTORY_LOGICAL_PATH
                || module_kind_home(&file.path).is_some();
            if !admitted_creation {
                return Err(ChangeError::WriteBoundaryDenied);
            }
            super::expect_digest(&file.after_digest).ok_or(ChangeError::CommitFailed {
                transaction: journal.transaction.clone(),
                detail: "journal-after-digest",
            })?;
            restorations.push(Restoration {
                path: file.path.clone(),
                target,
                backup: None,
                before_digest: String::new(),
                after_digest: file.after_digest.clone(),
            });
            continue;
        }
        super::expect_digest(&file.before_digest).ok_or(ChangeError::CommitFailed {
            transaction: journal.transaction.clone(),
            detail: "journal-before-digest",
        })?;
        super::expect_digest(&file.after_digest).ok_or(ChangeError::CommitFailed {
            transaction: journal.transaction.clone(),
            detail: "journal-after-digest",
        })?;
        if journal.state == STATE_REPLACING {
            validate_backup(root, &journal.transaction, index, true)?;
        }
        restorations.push(Restoration {
            path: file.path.clone(),
            target,
            backup: Some(journal_dir.join(BACKUPS_NAME).join(index.to_string())),
            before_digest: file.before_digest.clone(),
            after_digest: file.after_digest.clone(),
        });
    }
    Ok(restorations)
}

/// The strict read-only verdict over one pending transaction (round 3,
/// finding 6): `Ok(())` only when the journal closed-decodes, its whole
/// target set is admitted, its state is durably `committed`, and every
/// target still carries its pinned after bytes. Any unresolved,
/// malformed, or unverifiable state is `RecoveryRequired` — a lexical
/// winner never masks a later unresolved journal. Never writes.
pub(crate) fn require_durably_committed(root: &Path, transaction: &str) -> Result<(), ChangeError> {
    let (journal_dir, journal) = load_validated_journal(root, transaction)?;
    if journal.state != STATE_COMMITTED {
        return Err(ChangeError::RecoveryRequired {
            transaction: transaction.to_owned(),
        });
    }
    let restorations = restorations_of(root, &journal_dir, &journal)?;
    for restoration in &restorations {
        let Some(digest) = target_digest(restoration) else {
            return Err(ChangeError::RecoveryRequired {
                transaction: transaction.to_owned(),
            });
        };
        if digest != restoration.after_digest {
            return Err(ChangeError::RecoveryRequired {
                transaction: transaction.to_owned(),
            });
        }
    }
    Ok(())
}

/// `staging`: nothing was replaced; verify no source byte carries a
/// transaction after-digest (a contradicting journal fails closed), then
/// remove the recovery material. No restoration is needed.
fn recover_staging(
    root: &Path,
    transaction: &str,
    restorations: &[Restoration],
) -> Result<RecoveryResult, ChangeError> {
    for restoration in restorations {
        let Some(digest) = target_digest(restoration) else {
            continue; // absent target: never replaced
        };
        if digest == restoration.after_digest {
            return Err(ChangeError::CommitFailed {
                transaction: transaction.to_owned(),
                detail: "staging-state-contradiction",
            });
        }
    }
    let journal_dir = root
        .join(super::version::TRANSACTION_CACHE_ROOT)
        .join(transaction);
    std::fs::remove_dir_all(&journal_dir).map_err(|_| ChangeError::RecoveryRequired {
        transaction: transaction.to_owned(),
    })?;
    remove_pending_record(root, transaction);
    Ok(RecoveryResult {
        committed: false,
        files: Vec::new(),
    })
}

/// `replacing`: restoration material exists and replacements may have
/// started. Every backup is digest-verified before any write; each target
/// is restored unless it already carries its verified original bytes
/// (idempotent re-recovery); creations are removed only while the current
/// bytes match exactly what this transaction created.
fn recover_replacing(
    root: &Path,
    transaction: &str,
    journal_dir: &Path,
    restorations: &[Restoration],
) -> Result<RecoveryResult, ChangeError> {
    // Verify every backup against its journal pin before any restore
    // write; a single corrupt member fails the whole recovery with the
    // journal intact (the reader barrier stays up).
    for restoration in restorations {
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
    // Preflight EVERY target before any restoration write (round 3,
    // finding 2): a target may only be in a state this transaction can
    // explain — its pinned before bytes (not yet replaced, or already
    // restored by an earlier recovery pass), its pinned after bytes (the
    // transaction's own prospective output), or, for an existing target,
    // absence pending restoration from the verified backup. Any other
    // bytes are edits made after the interruption that the transaction
    // does not own: recovery refuses with the journal and the reader
    // barrier retained, and no earlier restoration has been performed.
    for restoration in restorations {
        let digest = target_digest(restoration);
        let explained = match (&restoration.backup, digest) {
            // A creation already removed (or never created).
            (None, None) => true,
            // The exact bytes this transaction created: removed again.
            (None, Some(digest)) => digest == restoration.after_digest,
            // An existing target absent pending restoration.
            (Some(_), None) => true,
            // Already restored to the verified original.
            (Some(_), Some(digest)) if digest == restoration.before_digest => true,
            // Still carrying (or indistinguishable from) the transaction's
            // own prospective bytes: restored from the verified backup.
            (Some(_), Some(digest)) => digest == restoration.after_digest,
        };
        if !explained {
            return Err(ChangeError::PreconditionFailed {
                subject: restoration.path.clone(),
                detail: "recovery-drift",
            });
        }
    }
    let mut failures = 0usize;
    for restoration in restorations {
        let outcome: std::io::Result<()> = match &restoration.backup {
            None => match std::fs::read(&restoration.target) {
                // Already removed by an earlier recovery attempt: nothing
                // to do (idempotent, round 2 finding 4).
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
                Ok(bytes) => {
                    // Delete only the exact bytes this transaction created
                    // (round 2, finding 3): a drifted or unrelated file
                    // stays untouched with the barrier retained.
                    let digest = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
                    if digest == restoration.after_digest {
                        std::fs::remove_file(&restoration.target)
                    } else {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "creation-drifted",
                        ))
                    }
                }
            },
            Some(backup) => match std::fs::read(&restoration.target) {
                // Already restored by an earlier recovery attempt: the
                // verified original is in place, nothing to do.
                Ok(bytes) => {
                    let digest = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
                    if digest == restoration.before_digest {
                        Ok(())
                    } else {
                        std::fs::read(backup)
                            .and_then(|original| write_same_dir(&restoration.target, &original))
                    }
                }
                Err(_) => std::fs::read(backup)
                    .and_then(|original| write_same_dir(&restoration.target, &original)),
            },
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
    std::fs::remove_dir_all(journal_dir).map_err(|_| ChangeError::RecoveryRequired {
        transaction: transaction.to_owned(),
    })?;
    remove_pending_record(root, transaction);
    // The rollback receipt's file outcomes, journal order: a restoration
    // replaced the transaction's prospective bytes with the verified
    // originals; a creation removal replaced the created bytes with the
    // absent-file sentinel digest.
    let files = restorations
        .iter()
        .map(|restoration| super::receipt::FileOutcome {
            path: restoration.path.clone(),
            before_digest: restoration.after_digest.clone(),
            after_digest: if restoration.backup.is_none() {
                super::absent_digest()
            } else {
                restoration.before_digest.clone()
            },
            outcome: "replaced".to_owned(),
        })
        .collect();
    Ok(RecoveryResult {
        committed: false,
        files,
    })
}

/// `committed`: the canonical history was durably written before the
/// journal was removed. Recovery verifies every target still carries its
/// pinned after bytes, then finishes the outstanding cleanup and ONLY
/// THEN reports the transaction committed — never rolled back, and never
/// a success while the journal still gates readers (round 3, finding 3).
fn recover_committed(
    root: &Path,
    transaction: &str,
    journal_dir: &Path,
    restorations: &[Restoration],
) -> Result<RecoveryResult, ChangeError> {
    for restoration in restorations {
        // A committed transaction's every target must still carry the
        // exact committed bytes; a missing or drifted target is an
        // unknown state and fails closed with the barrier retained.
        let Some(digest) = target_digest(restoration) else {
            return Err(ChangeError::RecoveryRequired {
                transaction: transaction.to_owned(),
            });
        };
        if digest != restoration.after_digest {
            return Err(ChangeError::RecoveryRequired {
                transaction: transaction.to_owned(),
            });
        }
    }
    // Round 5, finding 1: a durably committed transaction is one the
    // canonical history actually names — the ledger record with this
    // transaction's identity and committed state. A journal claiming
    // `committed` for a transaction that never ran (a forged cosmetic
    // commit) has no record and fails closed with the barrier retained.
    verify_history_custody(root, transaction)?;
    // The commit is durably verified: complete the cleanup BEFORE any
    // success is reported, so a subsequent ordinary read can never still
    // be gated by this journal and a repeat recovery cannot re-report
    // success over retained material (round 3, finding 3).
    std::fs::remove_dir_all(journal_dir).map_err(|_| ChangeError::RecoveryRequired {
        transaction: transaction.to_owned(),
    })?;
    remove_pending_record(root, transaction);
    let files = restorations
        .iter()
        .map(|restoration| super::receipt::FileOutcome {
            path: restoration.path.clone(),
            before_digest: restoration.before_digest.clone(),
            after_digest: restoration.after_digest.clone(),
            outcome: "replaced".to_owned(),
        })
        .collect();
    Ok(RecoveryResult {
        committed: true,
        files,
    })
}

/// The digest of one restoration target's current bytes, or `None` when
/// absent.
fn target_digest(restoration: &Restoration) -> Option<String> {
    let bytes = std::fs::read(&restoration.target).ok()?;
    Some(format!("sha256:{}", crate::digest::sha256_hex(&bytes)))
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

/// Bind a pending-registry failure to the transaction being begun.
fn bind_pending(transaction: &str) -> impl Fn(ChangeError) -> ChangeError + '_ {
    move |error| match error {
        ChangeError::CommitFailed { detail, .. } => ChangeError::CommitFailed {
            transaction: transaction.to_owned(),
            detail,
        },
        other => other,
    }
}

/// One validated restoration plan of the recovery pass.
struct Restoration {
    /// The journaled logical path (receipt evidence only).
    path: String,
    target: PathBuf,
    /// The digest-verified backup file, or `None` for a creation (the
    /// file is removed again instead of restored).
    backup: Option<PathBuf>,
    /// The exact `sha256:` digest the backup bytes must carry.
    before_digest: String,
    /// The exact `sha256:` digest the transaction's prospective bytes had.
    after_digest: String,
}

/// A process-unique, unpredictable temp suffix so no concurrent or
/// hostile observer can pre-link a predictable temp path (round 2,
/// finding 2).
fn temp_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mixed = format!(
        "{}-{nanos}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    crate::digest::sha256_hex(mixed.as_bytes())[..24].to_owned()
}

/// Replace one file in its own directory: an exclusively created temp file
/// with an unpredictable name plus rename, so the replacement is
/// same-volume, never truncates a pre-existing linked entry, and cannot be
/// predicted and hard-linked before creation (round 2, finding 2).
fn write_same_dir(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let file_name = target
        .file_name()
        .map(|name| name.to_os_string())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no-file-name"))?;
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no-parent"))?
        .to_path_buf();
    let mut last_error = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "temp-exists");
    for attempt in 0..4 {
        let temp = parent.join(format!(
            ".{}.{}-{}.lekalo-tmp",
            file_name.to_string_lossy(),
            temp_suffix(),
            attempt
        ));
        let file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                last_error = error;
                continue;
            }
            Err(error) => return Err(error),
        };
        let mut file = file;
        if let Err(error) = std::io::Write::write_all(&mut file, bytes) {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
        if let Err(error) = file.sync_all() {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
        drop(file);
        return match std::fs::rename(&temp, target) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = std::fs::remove_file(&temp);
                Err(error)
            }
        };
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The writer lock denies a second holder while the first is alive.
    #[test]
    fn live_writer_lock_denies_a_second_holder() {
        let root = std::env::temp_dir().join(format!("lekalo-tx-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".lekalo/cache/change")).expect("cache");
        let first = WriterLock::acquire(&root).expect("first holder");
        let second = WriterLock::acquire(&root);
        assert!(
            matches!(
                second,
                Err(ChangeError::PreconditionFailed { detail: "held", .. })
            ),
            "a live holder must deny"
        );
        drop(first);
        let third = WriterLock::acquire(&root);
        assert!(third.is_ok(), "a released lock must be acquirable");
        std::fs::remove_dir_all(&root).ok();
    }

    /// A stale lock artifact of a dead owner is taken over — recovery is
    /// never bricked behind its own crash artifact.
    #[test]
    fn stale_lock_artifact_is_reclaimed() {
        let root = std::env::temp_dir().join(format!("lekalo-tx-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join(".lekalo/cache/change");
        std::fs::create_dir_all(&cache).expect("cache");
        std::fs::write(cache.join(LOCK_NAME), b"held-by-a-dead-process\n").expect("plant");
        let lock = WriterLock::acquire(&root);
        assert!(lock.is_ok(), "a dead owner's artifact must be reclaimable");
        std::fs::remove_dir_all(&root).ok();
    }

    /// A link planted at the lock path fails closed instead of being
    /// followed or replaced.
    #[cfg(unix)]
    #[test]
    fn lock_path_link_fails_closed() {
        let root = std::env::temp_dir().join(format!("lekalo-tx-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join(".lekalo/cache/change");
        std::fs::create_dir_all(&cache).expect("cache");
        std::os::unix::fs::symlink("/lekalo-elsewhere", cache.join(LOCK_NAME)).expect("link");
        let lock = WriterLock::acquire(&root);
        assert!(lock.is_err(), "a linked lock path must refuse");
        std::fs::remove_dir_all(&root).ok();
    }

    /// The cache-home validation refuses a link planted at `.lekalo`.
    #[cfg(unix)]
    #[test]
    fn cache_home_link_refuses() {
        let root = std::env::temp_dir().join(format!("lekalo-tx-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        std::os::unix::fs::symlink("/lekalo-elsewhere", root.join(".lekalo")).expect("link");
        assert!(validated_cache_dir(&root).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    /// The journal decoder refuses unknown states and malformed members.
    #[test]
    fn journal_decode_fails_closed() {
        let valid = br#"{"schemaVersion":"lekalo/change-journal/1","transaction":"change-abcdef01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"staging","files":[],"history":false}"#;
        assert!(Journal::decode(valid).is_ok());
        let bad_state = br#"{"schemaVersion":"lekalo/change-journal/1","transaction":"change-abcdef01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":" vanished","files":[],"history":false}"#;
        assert!(Journal::decode(bad_state).is_err());
        let unknown_member = br#"{"schemaVersion":"lekalo/change-journal/1","transaction":"change-abcdef01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"staging","files":[],"history":false,"extra":1}"#;
        assert!(Journal::decode(unknown_member).is_err());
        let missing_member = br#"{"schemaVersion":"lekalo/change-journal/1","transaction":"change-abcdef01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"staging","files":[]}"#;
        assert!(Journal::decode(missing_member).is_err());
        // Round 5, finding 4: duplicate keys never silently collapse, and
        // the wire identity is pinned by a schemaVersion const.
        let duplicate_state = br#"{"schemaVersion":"lekalo/change-journal/1","transaction":"change-abcdef01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"staging","state":"replacing","files":[],"history":false}"#;
        assert!(Journal::decode(duplicate_state).is_err());
        let wrong_version = br#"{"schemaVersion":"lekalo/change-journal/2","transaction":"change-abcdef01","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","state":"staging","files":[],"history":false}"#;
        assert!(Journal::decode(wrong_version).is_err());
    }

    /// Round 5, finding 1: a fully self-consistent forged journal — real
    /// backup bytes, pins matching the live project — is refused unless a
    /// durable pending record outside the cache home pins the exact same
    /// facts. The overwrite and the silent-delete exploits both die here.
    #[test]
    fn forged_journal_without_provenance_never_recovers() {
        let root = std::env::temp_dir().join(format!("lekalo-tx-forge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // A structurally valid project (the admitted-target gate runs
        // before provenance and needs the real home layout).
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/change/field-vertical");
        fn copy_dir(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).expect("target dir");
            for entry in std::fs::read_dir(from).expect("read source") {
                let entry = entry.expect("entry");
                let target = to.join(entry.file_name());
                if entry.file_type().expect("type").is_dir() {
                    copy_dir(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), target).expect("copy");
                }
            }
        }
        copy_dir(&fixture, &root);
        let entities = root.join("lekalo/modules/tasks/entities.yaml");
        let original = std::fs::read(&entities).expect("entities");
        // change-forge002: `creation:false` + self-consistent backup pins
        // would restore the attacker payload over the admitted file.
        let attack = {
            let mut attack = original.clone();
            attack.extend_from_slice(b"# ATTACKER-CONTROLLED LINE\n");
            attack
        };
        let digest = |bytes: &[u8]| format!("sha256:{}", crate::digest::sha256_hex(bytes));
        let forge_replacing = serde_json::json!({
            "schemaVersion": JOURNAL_SCHEMA_VERSION,
            "transaction": "change-forge002",
            "planId": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "state": "replacing",
            "files": [{
                "path": "lekalo/modules/tasks/entities.yaml",
                "beforeDigest": digest(&attack),
                "afterDigest": digest(&original),
                "creation": false,
            }],
            "history": false,
        });
        let dir = root.join(".lekalo/cache/change/change-forge002");
        std::fs::create_dir_all(dir.join("backups")).expect("forge home");
        std::fs::write(dir.join("backups/0"), attack).expect("planted backup");
        std::fs::write(dir.join(JOURNAL_NAME), forge_replacing.to_string()).expect("journal");
        let refused = match recover(&root, "change-forge002") {
            Err(error) => error,
            Ok(_) => panic!("forged journal recovered"),
        };
        assert!(
            matches!(
                refused,
                ChangeError::CommitFailed {
                    detail: "journal-provenance",
                    ..
                }
            ),
            "got {refused:?}"
        );
        assert_eq!(std::fs::read(&entities).unwrap(), original.to_vec());
        assert!(dir.join(JOURNAL_NAME).is_file(), "journal kept");
        // change-forge003: `creation:true` + afterDigest of the real bytes
        // would silently delete the admitted file — same refusal.
        let forge_creation = serde_json::json!({
            "schemaVersion": JOURNAL_SCHEMA_VERSION,
            "transaction": "change-forge003",
            "planId": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "state": "replacing",
            "files": [{
                "path": "lekalo/modules/tasks/entities.yaml",
                "beforeDigest": digest(&[]),
                "afterDigest": digest(&original),
                "creation": true,
            }],
            "history": false,
        });
        let dir = root.join(".lekalo/cache/change/change-forge003");
        std::fs::create_dir_all(&dir).expect("forge home");
        std::fs::write(dir.join(JOURNAL_NAME), forge_creation.to_string()).expect("journal");
        let refused = match recover(&root, "change-forge003") {
            Err(error) => error,
            Ok(_) => panic!("forged journal recovered"),
        };
        assert!(
            matches!(
                refused,
                ChangeError::CommitFailed {
                    detail: "journal-provenance",
                    ..
                }
            ),
            "got {refused:?}"
        );
        assert_eq!(std::fs::read(&entities).unwrap(), original.to_vec());
        assert!(dir.join(JOURNAL_NAME).is_file(), "journal kept");
        std::fs::remove_dir_all(&root).ok();
    }

    /// Round 5, finding 1: a journal whose pins diverge from its durable
    /// record (even one byte of one digest) is forged provenance and
    /// fails closed; a state at most one step ahead of the record is the
    /// only accepted divergence (the crash window between the journal
    /// and record publications).
    #[test]
    fn provenance_mismatch_refuses_and_one_step_ahead_is_explained() {
        let root = std::env::temp_dir().join(format!("lekalo-tx-prov-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".lekalo")).expect("home");
        let zeros = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let record = PendingRecord {
            transaction: "change-prov0001".to_owned(),
            plan_id: zeros.to_owned(),
            state: STATE_STAGING.to_owned(),
            files: vec![JournalFile {
                path: "lekalo/modules/tasks/entities.yaml".to_owned(),
                before_digest: zeros.to_owned(),
                after_digest: zeros.to_owned(),
                creation: false,
            }],
            history: false,
        };
        write_pending_registry(&root, std::slice::from_ref(&record)).expect("registry");
        let journal_of = |state: &str, after: &str| Journal {
            schema_version: JOURNAL_SCHEMA_VERSION.to_owned(),
            transaction: "change-prov0001".to_owned(),
            plan_id: zeros.to_owned(),
            state: state.to_owned(),
            files: vec![JournalFile {
                path: "lekalo/modules/tasks/entities.yaml".to_owned(),
                before_digest: zeros.to_owned(),
                after_digest: after.to_owned(),
                creation: false,
            }],
            history: false,
        };
        // Exact match: proven.
        assert!(verify_journal_provenance(&root, &journal_of(STATE_STAGING, zeros)).is_ok());
        // One step ahead (journal published, record not yet): explained.
        assert!(verify_journal_provenance(&root, &journal_of(STATE_REPLACING, zeros)).is_ok());
        // Two steps ahead (staging record, committed journal): forged.
        let forged = verify_journal_provenance(&root, &journal_of(STATE_COMMITTED, zeros));
        assert!(
            matches!(
                forged,
                Err(ChangeError::CommitFailed {
                    detail: "journal-provenance",
                    ..
                })
            ),
            "got {forged:?}"
        );
        // A diverging file pin (one digest byte): forged.
        let ones = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
        let diverged = verify_journal_provenance(&root, &journal_of(STATE_STAGING, ones));
        assert!(
            matches!(
                diverged,
                Err(ChangeError::CommitFailed {
                    detail: "journal-provenance",
                    ..
                })
            ),
            "got {diverged:?}"
        );
        // No record at all: forged.
        let empty: Vec<PendingRecord> = Vec::new();
        write_pending_registry(&root, &empty).expect("registry cleared");
        let absent = verify_journal_provenance(&root, &journal_of(STATE_STAGING, zeros));
        assert!(
            matches!(
                absent,
                Err(ChangeError::CommitFailed {
                    detail: "journal-provenance",
                    ..
                })
            ),
            "got {absent:?}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn pending_registry_duplicates_and_nonfile_entries_refuse() {
        let root = std::env::temp_dir().join(format!("lekalo-tx-registry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".lekalo")).unwrap();
        let record = PendingRecord {
            transaction: "change-duplicate01".to_owned(),
            plan_id: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .to_owned(),
            state: STATE_STAGING.to_owned(),
            files: vec![],
            history: false,
        };
        write_pending_registry(&root, &[record.clone(), record]).unwrap();
        assert!(matches!(
            load_pending_registry(&root),
            Err(ChangeError::CommitFailed {
                detail: "pending-registry-duplicate",
                ..
            })
        ));
        std::fs::remove_file(root.join(".lekalo").join(PENDING_NAME)).unwrap();
        std::fs::create_dir(root.join(".lekalo").join(PENDING_NAME)).unwrap();
        assert!(matches!(
            load_pending_registry(&root),
            Err(ChangeError::WriteBoundaryDenied)
        ));
        assert!(matches!(
            write_pending_registry(&root, &[]),
            Err(ChangeError::WriteBoundaryDenied)
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_validation_requires_regular_no_follow_components() {
        let root = std::env::temp_dir().join(format!("lekalo-tx-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join(".lekalo/cache/change/change-backup01");
        std::fs::create_dir_all(home.join(BACKUPS_NAME)).unwrap();
        assert!(validate_backup(&root, "change-backup01", 0, false).is_ok());
        assert!(validate_backup(&root, "change-backup01", 0, true).is_err());
        std::fs::create_dir(home.join(BACKUPS_NAME).join("0")).unwrap();
        assert!(matches!(
            validate_backup(&root, "change-backup01", 0, true),
            Err(ChangeError::WriteBoundaryDenied)
        ));
        std::fs::remove_dir_all(home.join(BACKUPS_NAME)).unwrap();
        std::fs::write(home.join(BACKUPS_NAME), b"not a directory").unwrap();
        assert!(matches!(
            validate_backup(&root, "change-backup01", 0, true),
            Err(ChangeError::WriteBoundaryDenied)
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn pending_registry_and_backup_links_are_not_followed() {
        let root =
            std::env::temp_dir().join(format!("lekalo-tx-read-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join(".lekalo/cache/change/change-links01");
        std::fs::create_dir_all(&home).unwrap();
        std::os::unix::fs::symlink("/lekalo-elsewhere", root.join(".lekalo").join(PENDING_NAME))
            .unwrap();
        assert!(matches!(
            load_pending_registry(&root),
            Err(ChangeError::WriteBoundaryDenied)
        ));
        std::os::unix::fs::symlink("/lekalo-elsewhere", home.join(BACKUPS_NAME)).unwrap();
        assert!(matches!(
            validate_backup(&root, "change-links01", 0, true),
            Err(ChangeError::WriteBoundaryDenied)
        ));
        std::fs::remove_file(home.join(BACKUPS_NAME)).unwrap();
        std::fs::create_dir(home.join(BACKUPS_NAME)).unwrap();
        std::os::unix::fs::symlink("/lekalo-elsewhere", home.join(BACKUPS_NAME).join("0")).unwrap();
        assert!(matches!(
            validate_backup(&root, "change-links01", 0, true),
            Err(ChangeError::WriteBoundaryDenied)
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Temp replacement files carry unpredictable names and exclusively
    /// created entries, so a pre-linked predictable temp path can never be
    /// truncated into an external write.
    #[test]
    fn temp_names_are_unpredictable_and_exclusive() {
        let dir = std::env::temp_dir().join(format!("lekalo-tx-temp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let target = dir.join("doc.yaml");
        std::fs::write(&target, b"before\n").expect("target");
        write_same_dir(&target, b"after\n").expect("replace");
        assert_eq!(std::fs::read(&target).expect("read"), b"after\n");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .expect("dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains("lekalo-tmp"))
            .collect();
        assert!(leftovers.is_empty(), "no temp leftovers");
        std::fs::remove_dir_all(&dir).ok();
    }
}
