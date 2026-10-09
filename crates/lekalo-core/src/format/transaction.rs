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
//! - while a durable journal exists, every loader entry path fails closed
//!   with `format.recovery-required`; the ownership lock alone is never
//!   a reader signal;
//! - the next explicit mutating `format` operation recovers — restoring
//!   every manifest file to its verified before digest — before doing new
//!   work. Check and editor modes never recover and never create state;
//! - the same mutating run reclaims crashed pre-journal attempt homes
//!   and orphaned replacement temps under the ownership lock, so no
//!   crash residue can poison an identical retry, and the bounded
//!   attempt chain reclaims its oldest completed home instead of
//!   permanently denying a repeated change set.
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
/// The fixed suffix of the in-directory replacement temps. Orphaned
/// temps of this spelling are reclaimed by the mutating run (finding
/// r2 3): a mid-rename crash otherwise leaves the governed tree
/// unloadable and collides every retry on the fixed name.
const TEMP_SUFFIX: &str = "lekalo-format-new";
/// The immutable backup manifest inside one plan home.
const MANIFEST: &str = "manifest.json";
/// The rename-temp home inside the formatter runtime (finding r2 3):
/// temps live here, never as siblings inside the governed source tree.
const TEMP_HOME: &str = ".lekalo/cache/format/temps";
/// The final segment of [`TEMP_HOME`]: the single reserved name inside
/// the formatter home that is runtime custody, never a plan home. The
/// reader gate and the recovery enumeration share this exact constant
/// (finding r4 3).
const TEMP_HOME_NAME: &str = "temps";

/// The producer-accepted runtime temp spelling: a name ending in
/// `.` + [`TEMP_SUFFIX`] (e.g.
/// `...apply.module.yaml.lekalo-format-new`). The sweep and the emitter
/// agree on this exact suffix (finding r3 3).
fn is_owned_temp_name(name: &str) -> bool {
    name.strip_suffix(TEMP_SUFFIX)
        .is_some_and(|stem| stem.ends_with('.'))
}

/// The exact legacy fixed spelling: a dotfile whose stem ends in
/// `.` + [`TEMP_SUFFIX`] (e.g. `.project.yaml.lekalo-format-new`) and
/// whose stem names an emitted Model document of the governed tree
/// (finding r4 4): the legacy emitter could only strand rename temps
/// for files it actually wrote — `project.yaml` at the `lekalo/` root
/// or a `module.yaml`/kind document inside an admitted module
/// directory. Any other dotfile with the suffix is an unrelated user
/// file; the sweep leaves it untouched (the governed scan's business
/// to name it, never the sweep's to delete it). The exact
/// directory-matches-name pairing is enforced by the sweep's closed
/// [`FormatWrite::legacy_sweep_targets`] enumeration; this predicate
/// pins the name grammar for tests.
#[cfg(test)]
fn is_legacy_temp_name(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(TEMP_SUFFIX))
        .and_then(|stem| stem.strip_suffix('.'))
    else {
        return false;
    };
    stem == "project.yaml"
        || crate::project_fs::MODULE_STEMS
            .iter()
            .any(|owned| stem == format!("{owned}.yaml"))
}
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
    /// Whether this process created the ownership file. Only a freshly
    /// created lock is ever stamped (finding r2 1): an adopted file can
    /// be a hostile link spelling, and its bytes are never this
    /// formatter's to mutate.
    fresh: bool,
}

impl FormatGuard {
    /// Best-effort advisory stamp of the owned plan identity. The stamp
    /// never gates anything; diagnostics read it only as a hint. An
    /// adopted (not freshly created) ownership file is never mutated:
    /// the stamp must never become a write primitive through a planted
    /// link (finding r2 1). Even a freshly created file is re-verified
    /// through its own handle immediately before the write (finding r3
    /// 1): a hard link planted between creation and stamp would share
    /// the inode with an outside victim, and the write would clobber
    /// it — the link count through the held handle is the truth, so a
    /// multiply linked handle is never written.
    fn stamp(&self, plan_id: &str) {
        if !self.fresh {
            return;
        }
        if !singly_linked(&self.file) {
            return;
        }
        use std::io::{Seek, Write};
        let mut file = &self.file;
        if file.set_len(0).is_ok() {
            let _ = file.seek(std::io::SeekFrom::Start(0));
            let _ = file.write_all(format!("{plan_id}\n").as_bytes());
        }
    }
}

/// Whether the open handle's inode currently has exactly one hard link.
/// Best-effort by design: any probe failure keeps the file untouched.
fn singly_linked(file: &std::fs::File) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        file.metadata().map(|m| m.nlink() == 1).unwrap_or(false)
    }
    #[cfg(windows)]
    {
        handle_link_count(file)
            .map(|count| count == 1)
            .unwrap_or(false)
    }
}

/// The hard-link count of an open Windows file handle (stable std
/// exposes no link count, so the Windows query goes through
/// `GetFileInformationByHandle`).
#[cfg(windows)]
fn handle_link_count(file: &std::fs::File) -> Result<u32, ()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut information) };
    if ok == 0 {
        return Err(());
    }
    Ok(information.nNumberOfLinks)
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
        // The rename-temp home is runtime custody, never a plan home
        // (finding r4 3): a `journal.json` planted inside `temps/` would
        // otherwise block every reader forever, while the mutating run —
        // which deliberately filters `temps` out of the plan-home set —
        // could never recover or clear it. The plan-home reader gate and
        // the recovery set now agree on the same closed enumeration.
        if name == TEMP_HOME_NAME {
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
/// Crashed pre-journal attempt homes and orphaned replacement temps are
/// reclaimed in the same owned window whether or not a journal exists
/// (finding r2 2): residue that survives recovery would otherwise
/// poison every identical retry forever. Completed plan homes — journal
/// removed at commit — are rollback material, never recovery targets.
/// The guard is held by the caller across the load/plan/apply window.
pub(crate) fn recover_pending(
    selection: &LoadSelection,
) -> Result<Option<FormatGuard>, RecoveryFailure> {
    let root = crate::loader::root_for_selection(selection).map_err(|_| {
        RecoveryFailure::Recovery(RecoveryError {
            plan_id: "unknown".to_owned(),
            reason: "root-unresolved",
        })
    })?;
    let write = FormatWrite::new(root);
    // A project with no formatter state at all and no reclaimable
    // residue gets no runtime-home writes: the no-op apply on a
    // canonical tree stays byte-identical and creates nothing (the
    // reviewed gate premise). Any state — a plan home of any kind — or
    // any stranded crash residue proceeds to the owned window below
    // (finding r3 4): a legacy source-tree temp is exactly the state
    // the mutating run exists to clean, and the former plan-directory
    // early return skipped its cleanup on precisely the state-less
    // trees the residue keeps unloadable.
    if plan_directories_from(&write).is_empty() && !write.has_reclaimable_residue() {
        return Ok(None);
    }
    // The ownership lock precedes every runtime-home decision: only the
    // exclusive owner may recover, sweep, or create formatter state.
    let guard = write
        .acquire_ownership()
        .map_err(|_| RecoveryFailure::LiveWriter)?;
    // Orphan reclaim precedes recovery (finding r3 3): a process death
    // during recovery strands its own restore temp under a fixed
    // spelling, and the next recovery's exclusive create would collide
    // with that orphan and die in the same restore-failed state forever
    // — recovery used to run before the sweep, so retries never reached
    // cleanup. The same pass reclaims legacy source-tree temps whether
    // or not the optional runtime homes exist (finding r3 4).
    let _ = write.sweep_stale_temps();
    // Reader-gate state first: a journaled plan recovers, or fails the
    // run, before anything else touches the home or the tree.
    let plans = plan_directories_from(&write);
    for plan_id in &plans {
        if write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{JOURNAL}")) {
            recover_plan(&write, plan_id)?;
        }
    }
    // Residue sweep, unconditionally of journal presence (finding
    // r2 2): a crashed attempt between ownership and the manifest write
    // leaves a home with neither record; its unpinned partial backups
    // are reclaimable custody, never recovery material. A manifest-only
    // home keeps its immutable rollback material and stays occupied.
    let plans = plan_directories_from(&write);
    for plan_id in &plans {
        if !write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{JOURNAL}"))
            && !write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{MANIFEST}"))
        {
            let _ = write.remove_dir(&format!("{FORMAT_HOME}/{plan_id}"));
        }
    }
    Ok(Some(guard))
}

/// The plan directories beneath the formatter home (sorted). The rename
/// temp home (`temps`) is runtime custody, never a plan home.
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
        .filter(|entry| entry.file_name() != TEMP_HOME_NAME)
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

/// The conflict-side digest spelling of a deleted pinned source
/// (finding r2 12): the 64-zero spelling that can never equal a real
/// SHA-256 of existing bytes, so the conflict is always reported.
const DELETED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

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
    // journal and every source untouched. A *deleted* source is the most
    // severe divergence of the same class (finding r2 12): it also
    // surfaces as the explicit conflict — never as the generic recovery
    // refusal — because the pin it diverges from is known.
    for file in &manifest.files {
        let current = write
            .read_file(&file.path)
            .map_err(|_| RecoveryFailure::Conflict {
                path: file.path.clone(),
                expected: file.before_sha256.clone(),
                current: DELETED_DIGEST.to_owned(),
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
        write
            .replace_file(&file.path, &backup, "restore")
            .map_err(|_| {
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
    // writes (finding D). Any existing home directory of this identity
    // is occupied, whether or not it still carries a record (finding
    // r2 2): a pre-journal residue could otherwise be re-selected and
    // collide on exclusive backup writes forever. Bounded by reclamation
    // (finding r2 4): when every attempt slot is occupied by a completed
    // home, the oldest one is reclaimed first, so a repeated change set
    // cycles its homes instead of hitting a permanent denial. Reclamation
    // never touches live state: homes with a journal recover (or fail
    // the run) before this loop, and manifest-bearing material is
    // reclaimed only when it is the only remedy that avoids the
    // deterministic ceiling.
    let base = plan_identity(policy_digest, files);
    let home_occupied = |attempt: usize| {
        write
            .physical(
                &format!("{FORMAT_HOME}/{}", plan_home(&base, attempt)),
                false,
            )
            .is_ok()
    };
    let first_free = (1..=ATTEMPT_BOUND)
        .find(|attempt| !home_occupied(*attempt))
        .or_else(|| {
            // Every slot occupied: reclaim the oldest completed home
            // (attempt 1) and reuse exactly that slot — the freed slot is
            // the allocated one, so a completed home is never collided.
            // A journaled home is never reclaimed: it is live recovery
            // material, and recovery has already handled it on the
            // guarded entry path.
            let oldest = plan_home(&base, 1);
            let journaled = write.file_exists(&format!("{FORMAT_HOME}/{oldest}/{JOURNAL}"));
            if journaled {
                return None;
            }
            write
                .remove_dir(&format!("{FORMAT_HOME}/{oldest}"))
                .ok()
                .map(|()| 1)
        })
        .ok_or(ApplyFailure::WriteFailed {
            stage: "plan",
            rollback: "not-started",
        })?;
    let plan_id = plan_home(&base, first_free);
    // The rename temps of this run live under the runtime temp home with
    // the owned plan identity and pid as their namespace (finding r2 3):
    // unique per run, and reclaimable by any later exclusive owner.
    let write = write.with_temp_namespace(&plan_id);

    // The durable marker with the owned identity (reader-gate state for
    // diagnostics; the OS lock above is the real exclusion). Only a
    // freshly created ownership file is ever stamped (finding r2 1);
    // an adopted one is never this process's to mutate.
    guard.stamp(&plan_id);
    let outcome = apply_locked(&write, &plan_id, policy_digest, files, after_bytes);
    // The ownership file is never removed by its owner (finding r2 11):
    // unlinking a held lock opens the fresh-inode window on POSIX, and
    // the Windows share-mode handle refuses the removal silently — a
    // platform-divergent residue. The kernel releases the lock at death;
    // the file's presence alone is never a reader signal.
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

    // Phase 4: preimage-verified replacement in sorted order. The
    // replacement temps carry the "apply" stage key; the rollback below
    // carries "restore".
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
                write.replace_file(path, &after_bytes[index], "apply")
            }
            Err(_) => write.replace_file(path, &after_bytes[index], "apply"),
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
            .replace_file(&file.path, &backup, "restore")
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
    /// The per-run temp namespace: the owned plan identity when known,
    /// else a process-unique spelling. Temp rename files live under the
    /// runtime home with this prefix (finding r2 3).
    temp_namespace: Option<String>,
}

impl FormatWrite {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            temp_namespace: None,
        }
    }

    /// Bind the temp namespace to one plan identity (apply time).
    fn with_temp_namespace(mut self, plan_id: &str) -> Self {
        self.temp_namespace = Some(format!("{}.{}", plan_id, std::process::id()));
        self
    }

    /// Acquire the process-death-released ownership lock: an exclusive
    /// advisory OS lock on the runtime lock file (POSIX `flock`, Windows
    /// share-mode zero). The file is created once and never removed, so
    /// every owner locks the same inode; a held lock refuses a second
    /// owner immediately (non-blocking). The final component is opened
    /// without following links (POSIX `O_NOFOLLOW`; Windows reparse
    /// attribute).
    ///
    /// Freshness is decided by exclusive creation (finding r3 1): the
    /// owner first tries `O_EXCL`/`CREATE_NEW` — success means this
    /// process created the inode, so the stamp can never adopt outside
    /// bytes. When the name already exists the file is adopted only
    /// after the opened handle proves it is a regular, reparse-free,
    /// singly linked file whose canonical resolution stays inside the
    /// project root — a planted hard link to an outside victim (link
    /// count above 1) or any link spelling is refused outright instead
    /// of handing the stamp a write primitive through the link (finding
    /// r2 1). An adopted lock is never stamped.
    fn acquire_ownership(&self) -> Result<FormatGuard, ()> {
        self.ensure_dir(FORMAT_HOME)?;
        let path = self
            .physical_lock_path(&format!("{FORMAT_HOME}/{ACTIVE_LOCK}"))?
            .0;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            // O_NOFOLLOW|O_CLOEXEC|O_EXCL: the create-or-fail open
            // refuses a final symlink instead of following it out of
            // the runtime home, and succeeds only for a name that did
            // not exist — the owned builder binding keeps the
            // OpenOptions local alive across `open` (finding r3 2: the
            // single chained expression previously borrowed a dropped
            // temporary and failed to compile on the unix target with
            // E0716).
            const O_NOFOLLOW: i32 = 0o400000;
            const O_CLOEXEC: i32 = 0o2000000;
            let mut builder = std::fs::OpenOptions::new();
            let options = builder
                .read(true)
                .write(true)
                .create_new(true)
                .custom_flags(O_NOFOLLOW | O_CLOEXEC);
            let file = match options.open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Adoption: the name predates this run (the file is
                    // never removed, so this is the steady state). The
                    // same no-follow flags apply; a symlink still fails
                    // the open outright.
                    let mut adopt_builder = std::fs::OpenOptions::new();
                    let adopt = adopt_builder
                        .read(true)
                        .write(true)
                        .custom_flags(O_NOFOLLOW | O_CLOEXEC);
                    let file = adopt.open(&path).map_err(|_| ())?;
                    Self::verify_adopted_lock(&file, &path, &self.root)?;
                    let fresh = false;
                    use rustix::fs::{flock, FlockOperation};
                    flock(&file, FlockOperation::NonBlockingLockExclusive).map_err(|_| ())?;
                    return Ok(FormatGuard { file, fresh });
                }
                Err(_) => return Err(()),
            };
            // The created inode must still be a regular singly linked
            // file at the expected path (a race replacing the name
            // between create and stat leaves our handle on an unlinked
            // inode — harmless, but the run refuses rather than lock a
            // home it cannot see).
            let metadata = file.metadata().map_err(|_| ())?;
            if !metadata.is_file() || metadata.nlink() != 1 {
                return Err(());
            }
            let on_disk = std::fs::symlink_metadata(&path).map_err(|_| ())?;
            if on_disk.is_symlink()
                || !on_disk.is_file()
                || on_disk.ino() != metadata.ino()
                || on_disk.dev() != metadata.dev()
            {
                return Err(());
            }
            let fresh = true;
            use rustix::fs::{flock, FlockOperation};
            flock(&file, FlockOperation::NonBlockingLockExclusive).map_err(|_| ())?;
            Ok(FormatGuard { file, fresh })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_FLAG_OPEN_REPARSE_POINT: the open never follows a
            // planted reparse spelling; the post-open attribute check
            // refuses it outright.
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            // CREATE_NEW first: success proves this process created the
            // inode, so the stamp can never adopt outside bytes.
            let file = match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .share_mode(0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .open(&path)
            {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Adoption: steady state for the never-removed lock
                    // (or a planted name — the checks below refuse it).
                    // share_mode(0) is the exclusion itself: a live
                    // owner's handle fails this open immediately.
                    let file = std::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .share_mode(0)
                        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                        .open(&path)
                        .map_err(|_| ())?;
                    Self::verify_adopted_lock(&file, &path, &self.root)?;
                    let fresh = false;
                    return Ok(FormatGuard { file, fresh });
                }
                Err(_) => return Err(()),
            };
            let metadata = file.metadata().map_err(|_| ())?;
            if !metadata.is_file() || reparse_point(&metadata) {
                return Err(());
            }
            let on_disk = std::fs::symlink_metadata(&path).map_err(|_| ())?;
            if reparse_point(&on_disk) || !on_disk.is_file() {
                return Err(());
            }
            let fresh = true;
            Ok(FormatGuard { file, fresh })
        }
    }

    /// The adoption verdict for a pre-existing ownership file (finding
    /// r3 1): the opened handle must be a regular, reparse-free,
    /// singly linked file whose canonical resolution stays inside the
    /// project root. A hard-linked lock shares its content with an
    /// external victim (link count above 1 — stable std exposes no
    /// Windows link count, so the query goes through
    /// `GetFileInformationByHandle`); any link spelling or escape is
    /// refused instead of adopted, so the stamp can never write through
    /// a link and the lock can never resolve outside the root.
    fn verify_adopted_lock(file: &std::fs::File, path: &Path, root: &Path) -> Result<(), ()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = file.metadata().map_err(|_| ())?;
            if !metadata.is_file() || metadata.nlink() != 1 {
                return Err(());
            }
        }
        #[cfg(windows)]
        {
            let metadata = file.metadata().map_err(|_| ())?;
            if !metadata.is_file() || reparse_point(&metadata) {
                return Err(());
            }
            if handle_link_count(file)? != 1 {
                return Err(());
            }
        }
        // The resolved real path must stay inside the resolved root:
        // canonicalize is the last word on where the opened inode
        // actually lives (a link component that slipped every
        // attribute check still resolves here and is refused).
        let canonical_target = std::fs::canonicalize(path).map_err(|_| ())?;
        let canonical_root = std::fs::canonicalize(root).map_err(|_| ())?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err(());
        }
        Ok(())
    }

    /// The validated physical path of the ownership file: every chain
    /// component is reparse-checked exactly like [`FormatWrite::physical`]
    /// (the final component may not exist yet), plus the returned
    /// pre-open `symlink_metadata` of the final component when present.
    fn physical_lock_path(
        &self,
        logical: &str,
    ) -> Result<(PathBuf, Option<std::fs::Metadata>), ()> {
        let mut path = self.root.clone();
        let segments: Vec<&str> = logical.split('/').filter(|s| !s.is_empty()).collect();
        let last = segments.len();
        for (position, segment) in segments.iter().enumerate() {
            if *segment == ".." {
                return Err(());
            }
            path.push(segment);
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) if position + 1 == last => return Ok((path, None)),
                Err(_) => return Err(()),
            };
            if metadata.file_type().is_symlink() {
                return Err(());
            }
            #[cfg(windows)]
            {
                if reparse_point(&metadata) {
                    return Err(());
                }
            }
            if position + 1 != last && !metadata.is_dir() {
                return Err(());
            }
        }
        let final_metadata = std::fs::symlink_metadata(&path).ok();
        if let Some(metadata) = &final_metadata {
            if metadata.file_type().is_symlink() {
                return Err(());
            }
            #[cfg(windows)]
            {
                if reparse_point(metadata) {
                    return Err(());
                }
            }
        }
        Ok((path, final_metadata))
    }

    /// Physical path of one logical path, rejecting symlink/junction/
    /// reparse components. When `expect_file` the final component must be
    /// a regular file, else a directory that already exists.
    fn physical(&self, logical: &str, expect_file: bool) -> Result<PathBuf, ()> {
        let mut path = self.root.clone();
        let segments: Vec<&str> = logical.split('/').filter(|s| !s.is_empty()).collect();
        // Defense-in-depth (finding r2 10): every caller feeds validated
        // logicals, but the confinement primitive itself rejects `..`
        // spellings outright.
        if segments.contains(&"..") {
            return Err(());
        }
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
    /// temp carrying the original's permissions, flush, then rename over
    /// the target. The temp lives inside the formatter runtime home
    /// under a unique per-run name — same volume, so the rename stays
    /// atomic — never as a sibling inside the governed source tree, so
    /// a mid-rename crash cannot strand a dotfile there (the governed
    /// scan would reject it and leave the project unloadable, finding
    /// r2 3). The target must be a reparse-free regular file; it is
    /// never truncated in place. `temp_key` distinguishes concurrent
    /// stages so temps never collide on a fixed name.
    fn replace_file(&self, logical: &str, bytes: &[u8], temp_key: &str) -> Result<(), ()> {
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
        // The rename-temp home lives inside the formatter runtime; the
        // exclusive owner creates it on first use (finding r2 3).
        self.ensure_dir(TEMP_HOME)?;
        let home = self.physical(TEMP_HOME, false)?;
        let temp = home.join(format!(
            "{}.{temp_key}.{name}.{TEMP_SUFFIX}",
            self.temp_namespace()
        ));
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
        let renamed = std::fs::rename(&temp, &target);
        if renamed.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        renamed.map_err(|_| ())?;
        sync_dir(&dir_path);
        Ok(())
    }

    /// The per-run temp namespace prefix: unique per process and plan
    /// window, so orphan reclamation (below) never races a live writer's
    /// The per-run temp namespace prefix: unique per process and plan
    /// window, so orphan reclamation (below) never races a live writer's
    /// temps and two attempts never share a temp name. Recovery runs
    /// before any plan identity exists; its fallback namespace is
    /// process-unique too (finding r3 3), so a crashed recovery's orphan
    /// can never collide with the next recovery's temp even ahead of the
    /// entry sweep.
    fn temp_namespace(&self) -> String {
        self.temp_namespace
            .clone()
            .unwrap_or_else(|| format!("recover.{}", std::process::id()))
    }

    /// Reclaim crashed rename temps everywhere they can strand (finding
    /// r2 3, r3 3): the runtime temp home first — its absence or a
    /// hostile spelling never suppresses the legacy pass — then the
    /// legacy source-tree spellings. The caller holds the exclusive
    /// ownership lock, so no live writer's temps exist during this pass.
    fn sweep_stale_temps(&self) -> Result<(), ()> {
        let home = self.sweep_home_temps();
        let legacy = self.sweep_legacy_source_temps();
        home.and(legacy)
    }

    /// Reclaim orphaned temps inside the runtime temp home (finding r2
    /// 3, r3 3): any plain file with the producer's exact temp suffix is
    /// a crashed rename temp — the exclusive owner (this caller) is the
    /// only legitimate writer of this home, so every orphan is
    /// reclaimable custody. The suffix agrees with the emitter's
    /// spelling (finding r3 3): the former `.new` matcher never matched
    /// the emitted `*.lekalo-format-new` names, so a crashed run's
    /// orphan survived every sweep and collided every retry of the next
    /// recovery. A missing temp home is simply empty; links, hostile
    /// home spellings, directories, and foreign names are left untouched
    /// and never followed or removed.
    fn sweep_home_temps(&self) -> Result<(), ()> {
        let Ok(home) = self.physical(TEMP_HOME, false) else {
            return Ok(());
        };
        for entry in std::fs::read_dir(&home).map_err(|_| ())? {
            let entry = entry.map_err(|_| ())?;
            let metadata = entry.metadata().map_err(|_| ())?;
            if metadata.file_type().is_symlink() {
                return Err(());
            }
            #[cfg(windows)]
            {
                if reparse_point(&metadata) {
                    return Err(());
                }
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(());
            };
            if is_owned_temp_name(name) {
                if !metadata.is_file() {
                    return Err(());
                }
                std::fs::remove_file(entry.path()).map_err(|_| ())?;
            }
        }
        sync_dir(&home);
        Ok(())
    }

    /// Reclaim legacy rename temps that an older binary stranded inside
    /// the governed source tree: `.{name}.lekalo-format-new` siblings
    /// at the `lekalo/` project root and in the admitted module
    /// directories (finding r3 4: the former sweep covered only the
    /// module directories — the `project.yaml` sibling survived every
    /// sweep — and was skipped entirely whenever the optional runtime
    /// temp home was absent). The governed scan rejects those dotfiles —
    /// the project stays unloadable until this owned sweep reclaims
    /// them — so the mutating run removes exactly the legacy fixed
    /// spelling for an emitted Model document of the closed inventory
    /// (finding r4 4), never any other source entry: an unrelated user
    /// dotfile with the suffix is left byte-identical, and the loader's
    /// own structure refusal — not deletion — answers it.
    fn sweep_legacy_source_temps(&self) -> Result<(), ()> {
        for (directory, stem) in self.legacy_sweep_targets() {
            let Ok(dir_path) = self.physical(&directory, false) else {
                continue;
            };
            let name = format!(".{stem}.{TEMP_SUFFIX}");
            let metadata = match std::fs::symlink_metadata(&dir_path.join(&name)) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                continue;
            }
            #[cfg(windows)]
            {
                if reparse_point(&metadata) {
                    continue;
                }
            }
            let _ = std::fs::remove_file(dir_path.join(&name));
            sync_dir(&dir_path);
        }
        Ok(())
    }

    /// The closed (directory, emitted-document-name) pairs a legacy
    /// rename temp can possibly occupy (finding r4 4): the project
    /// document at the `lekalo/` root, plus every emitted document name
    /// in every admitted module directory. The sweep and the residue
    /// probe enumerate exactly this set, so cleanup can never widen.
    fn legacy_sweep_targets(&self) -> Vec<(String, String)> {
        let mut targets = vec![("lekalo".to_owned(), "project.yaml".to_owned())];
        if let Ok(home) = self.physical("lekalo/modules", false) {
            if let Ok(read) = std::fs::read_dir(&home) {
                for entry in read.flatten() {
                    let is_module_dir = entry
                        .metadata()
                        .map(|metadata| {
                            metadata.is_dir() && !metadata.file_type().is_symlink()
                        })
                        .unwrap_or(false);
                    if !is_module_dir {
                        continue;
                    }
                    let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                        continue;
                    };
                    for stem in crate::project_fs::MODULE_STEMS {
                        targets.push((
                            format!("lekalo/modules/{name}"),
                            format!("{stem}.yaml"),
                        ));
                    }
                }
            }
        }
        targets
    }

    /// Whether reclaimable crash residue exists anywhere the sweeps look
    /// (finding r3 4): an orphaned runtime temp or a legacy source-tree
    /// temp. The read-only probe behind the recovery entry decision:
    /// stranded residue keeps the project unloadable exactly as a
    /// journal would, so a state-less tree carrying it must still reach
    /// the owned sweep. Anomalies (links, hostile spellings) are skipped
    /// here — the owned sweep refuses them instead.
    fn has_reclaimable_residue(&self) -> bool {
        if let Ok(home) = self.physical(TEMP_HOME, false) {
            if let Ok(read) = std::fs::read_dir(&home) {
                for entry in read.flatten() {
                    let Ok(metadata) = entry.metadata() else {
                        continue;
                    };
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        continue;
                    }
                    #[cfg(windows)]
                    {
                        if reparse_point(&metadata) {
                            continue;
                        }
                    }
                    if let Some(name) = entry.file_name().to_str() {
                        if is_owned_temp_name(name) {
                            return true;
                        }
                    }
                }
            }
        }
        for (directory, stem) in self.legacy_sweep_targets() {
            let name = format!(".{stem}.{TEMP_SUFFIX}");
            let Ok(dir_path) = self.physical(&directory, false) else {
                continue;
            };
            let Ok(metadata) = std::fs::symlink_metadata(dir_path.join(&name)) else {
                continue;
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                continue;
            }
            #[cfg(windows)]
            {
                if reparse_point(&metadata) {
                    continue;
                }
            }
            return true;
        }
        false
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

#[cfg(test)]
mod legacy_name_tests {
    // The closed legacy temp-name grammar (finding r4 4): only the
    // emitted Model documents of the governed tree can ever be owned
    // legacy temps; anything else is a user file the sweep must leave
    // untouched.
    #[test]
    fn legacy_temp_names_are_limited_to_emitted_documents() {
        assert!(super::is_legacy_temp_name(".project.yaml.lekalo-format-new"));
        for kind in crate::project_fs::KIND_STEMS {
            assert!(super::is_legacy_temp_name(&format!(".{kind}.yaml.lekalo-format-new")));
        }
        assert!(super::is_legacy_temp_name(".module.yaml.lekalo-format-new"));
        // Unrelated user files are never owned.
        assert!(!super::is_legacy_temp_name(".private-notes.lekalo-format-new"));
        assert!(!super::is_legacy_temp_name(".unrelated.txt.lekalo-format-new"));
        assert!(!super::is_legacy_temp_name(".invented.yaml.lekalo-format-new"));
        // The owned temp (non-legacy) family stays intact.
        assert!(super::is_owned_temp_name(
            "plan.pid.apply.module.yaml.lekalo-format-new"
        ));
    }
}
