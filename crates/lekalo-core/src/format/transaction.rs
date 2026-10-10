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
//!   from verified backups and report `format.write-failed`; a rollback
//!   destination whose live bytes are neither the recorded before nor
//!   after pin — a concurrent user edit or deletion — is preserved
//!   untouched with the journal retained, and the run ends in the
//!   explicit conflict decision instead of a declared restoration
//!   (finding r5 1);
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

use crate::format::policy::Policy;
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
/// The legacy-temp quarantine home inside the runtime temp home
/// (finding r5 3): structurally proven legacy temps are MOVED here —
/// never deleted — while the run proves admission through a full load.
/// A move is committed (deleted) only against the validated admitted
/// inventory; every refusal restores the exact bytes to their exact
/// original locations.
const CUSTODY_HOME: &str = ".lekalo/cache/format/temps/legacy";
/// The durable quarantine record: written before the first move, the
/// crash-safe authority for re-deciding an interrupted quarantine.
const CUSTODY_RECORD: &str = "record.json";
/// The custody location token of the `lekalo/` root: a governed
/// grammar spelling (leading digit) that still reads as the reserved
/// root namespace.
const CUSTODY_ROOT_TOKEN: &str = "0lekalo-root";
/// The internal, versioned quarantine-record identity (never a public
/// contract; the quarantine owner validates this version).
const CUSTODY_SCHEMA: &str = "lekalo/format-legacy-custody/v0.8.0";

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
/// to name it, never the sweep's to delete it). The name grammar is
/// one input of the ownership proof (finding r5 3): the directory and
/// the source document must be provable too. The production proof is
/// the location-aware [`CustodyMove::validate`]; this tree-spelling
/// predicate pins the name grammar for tests.
#[cfg(test)]
fn is_legacy_temp_name(name: &str) -> bool {
    name.strip_prefix('.').is_some_and(is_legacy_custody_name)
}

/// The custody spelling of the same name (the leading dot dropped, so
/// the quarantined file itself satisfies the governed path-segment
/// grammar the `.lekalo` runtime tree is scanned under): the exact
/// emitted-document temp spelling, e.g. `module.yaml.lekalo-format-new`.
fn is_legacy_custody_name(name: &str) -> bool {
    name.strip_suffix(TEMP_SUFFIX)
        .and_then(|stem| stem.strip_suffix('.'))
        .is_some_and(|stem| {
            stem == "project.yaml"
                || crate::project_fs::MODULE_STEMS
                    .iter()
                    .any(|owned| stem == format!("{owned}.yaml"))
        })
}

/// One quarantined legacy temp: the exact directory and file name it
/// was moved from (finding r5 3). The record carries locations, never
/// bytes — the move is a rename, so the bytes are the user's own.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CustodyMove {
    /// The logical directory the temp was moved from: `lekalo` or an
    /// admitted `lekalo/modules/<dir>`.
    directory: String,
    /// The exact legacy temp file name.
    name: String,
}

impl CustodyMove {
    /// The closed validation (finding A discipline): an emitted-document
    /// temp name inside one of the two legal legacy locations, with the
    /// module-directory spelling inside the governed segment grammar
    /// (finding r5 3, devin 2). The name grammar is location-aware —
    /// the `project.yaml` document spelling belongs to the `lekalo/`
    /// root alone, module-stem spellings to module directories alone —
    /// so no two valid moves can ever map to one custody file path.
    fn validate(&self) -> Result<(), &'static str> {
        if self.directory == "lekalo" {
            if self.name != format!(".project.yaml.{TEMP_SUFFIX}") {
                return Err("custody-name");
            }
            return Ok(());
        }
        let Some(directory) = self.directory.strip_prefix("lekalo/modules/") else {
            return Err("custody-directory");
        };
        if directory.is_empty()
            || directory.contains('/')
            || crate::project_fs::path_violation(directory).is_some()
        {
            return Err("custody-directory");
        }
        let Some(stem) = self
            .name
            .strip_prefix('.')
            .and_then(|rest| rest.strip_suffix(TEMP_SUFFIX))
            .and_then(|stem| stem.strip_suffix('.'))
        else {
            return Err("custody-name");
        };
        if stem == "project.yaml"
            || !crate::project_fs::MODULE_STEMS
                .iter()
                .any(|owned| stem == format!("{owned}.yaml"))
        {
            return Err("custody-name");
        }
        Ok(())
    }

    /// The emitted Model document this temp was the rename of.
    fn document_path(&self) -> String {
        let document = self
            .name
            .strip_prefix('.')
            .and_then(|stem| stem.strip_suffix(TEMP_SUFFIX))
            .and_then(|stem| stem.strip_suffix('.'))
            .unwrap_or(&self.name);
        format!("{}/{}", self.directory, document)
    }

    /// The physical source path of the temp file itself.
    fn source_path(&self) -> String {
        format!("{}/{}", self.directory, self.name)
    }
}

/// The durable quarantine record (finding r5 3): every pending move,
/// written before the first move and removed only after every move is
/// committed or restored. Exact closed field set; fully validated on
/// read.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CustodyRecord {
    schema_version: String,
    moves: Vec<CustodyMove>,
}

impl CustodyRecord {
    fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != CUSTODY_SCHEMA {
            return Err("custody-identity");
        }
        if self.moves.is_empty() || self.moves.len() > RECORD_MAX_FILES {
            return Err("custody-limit");
        }
        let mut moves = self.moves.clone();
        moves.sort();
        moves.dedup();
        if moves.len() != self.moves.len() {
            return Err("custody-duplicate");
        }
        for pending in &self.moves {
            pending.validate()?;
        }
        Ok(())
    }
}
/// The internal, versioned runtime-record identity (never a public
/// contract; the reader owns this version).
const RUNTIME_SCHEMA: &str = "lekalo/format-transaction/v0.8.0";

/// The exact policy pin every legitimate transaction record carries
/// (finding r4 5): the digest of the embedded contract this binary
/// plans under, recomputed once per process. A record whose pin names
/// any other policy — however internally hash-consistent — was not
/// produced by this formatter's policy and is refused before any
/// restore, so recovery can never admit foreign preimages under a
/// self-consistent but forged pin.
fn supported_policy_digest() -> &'static str {
    use std::sync::LazyLock;
    static PIN: LazyLock<String> = LazyLock::new(|| {
        Policy::embedded()
            .expect("embedded policy is pinned")
            .digest
    });
    &PIN
}

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
    /// The validated project root the guard's runtime home lives under:
    /// the quarantine settle/restore paths derive their write capability
    /// from it (finding r5 3).
    root: PathBuf,
    /// The legacy temps this run holds in quarantine custody (finding r5
    /// 3): committed only against the validated admitted inventory,
    /// restored byte-exact on every refusal. Empty when the guarded
    /// window took no custody.
    custody: Vec<CustodyMove>,
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
#[derive(Debug)]
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
    /// The serialized recoverable record set exceeds the bounded record
    /// grammar its own recovery reader parses (finding r5 2): the plan
    /// is refused whole — the registered bounded-plan failure — before
    /// any runtime write, so an admitted plan can never emit a manifest
    /// its recovery reader would refuse.
    PlanLimit {
        limit: &'static str,
        measured: u64,
        allowed: u64,
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
            Self::PlanLimit {
                limit,
                measured,
                allowed,
            } => {
                let mut data = crate::diagnostics::DataObject::new();
                data.insert(
                    "limit".to_owned(),
                    crate::diagnostics::types::DataValue::Token(limit.to_owned()),
                );
                data.insert(
                    "measured".to_owned(),
                    crate::diagnostics::types::DataValue::Count(measured),
                );
                data.insert(
                    "allowed".to_owned(),
                    crate::diagnostics::types::DataValue::Count(allowed),
                );
                // The registered bounded-plan refusal (LEK-FORMAT-008):
                // a formatter-specific bound was reached and the whole
                // plan was refused — here the record bound the plan's own
                // recovery reader enforces (finding r5 2).
                let diagnostics =
                    crate::diagnostics::normalize::build("format.limit-exceeded", None, None, data)
                        .map(|diagnostic| vec![diagnostic])
                        .unwrap_or_default();
                let set = DomainResult::from_wire_set(Status::Invalid, diagnostics);
                DomainResult::invalid(set)
            }
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
/// poison every identical retry forever. Legacy source-tree temps are
/// quarantined into runtime custody under the same window (finding r5
/// 3) — moved, never deleted — and the returned guard carries them so
/// the caller settles them against the validated admitted inventory
/// (commit) or restores them (every refusal). Completed plan homes —
/// journal removed at commit — are rollback material, never recovery
/// targets. The guard is held by the caller across the load/plan/apply
/// window.
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
    let mut guard = write
        .acquire_ownership()
        .map_err(|_| RecoveryFailure::LiveWriter)?;
    // Orphan reclaim precedes recovery (finding r3 3): a process death
    // during recovery strands its own restore temp under a fixed
    // spelling, and the next recovery's exclusive create would collide
    // with that orphan and die in the same restore-failed state forever
    // — recovery used to run before the sweep, so retries never reached
    // cleanup.
    let _ = write.sweep_home_temps();
    // Legacy source-tree temps are QUARANTINED, never deleted (finding
    // r5 3): every structurally provable legacy temp moves into runtime
    // custody so the governed tree can be admitted at all, and the
    // durable record makes an interrupted quarantine re-decidable. The
    // bytes are committed only against the validated admitted inventory
    // — by the caller, after its load succeeds — and restored on every
    // refusal below.
    let custody = write.quarantine_legacy_temps().map_err(|error| {
        // Quarantine failed after moving nothing the run can trust:
        // restore whatever is restorable before failing closed.
        write.restore_legacy_custody_best_effort();
        RecoveryFailure::Recovery(error)
    })?;
    guard.custody = custody;
    // Reader-gate state first: a journaled plan recovers, or fails the
    // run, before anything else touches the home or the tree. Every
    // failure exit restores the quarantine first (finding r5 3): the
    // run's refusal must never leave user bytes in custody.
    let plans = plan_directories_from(&write);
    for plan_id in &plans {
        if write.file_exists(&format!("{FORMAT_HOME}/{plan_id}/{JOURNAL}")) {
            if let Err(failure) = recover_plan(&write, plan_id) {
                write.restore_legacy_custody(&guard.custody);
                guard.custody.clear();
                return Err(failure);
            }
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

/// Settle the quarantine against the validated admitted inventory
/// (finding r5 3): the apply pipeline calls this immediately after its
/// load succeeded — the completed ownership proof. Every quarantined
/// temp whose emitted document is admitted is deleted (proven
/// formatter-owned); any other is restored byte-exact. The durable
/// record is removed only after every move settled.
pub(crate) fn settle_quarantine(
    guard: &mut FormatGuard,
    admitted: &[String],
) -> Result<(), RecoveryError> {
    if guard.custody.is_empty() {
        return Ok(());
    }
    let write = FormatWrite::new(guard.root.clone());
    let outcome = write.settle_legacy_custody(&guard.custody, admitted);
    guard.custody.clear();
    outcome.map_err(|reason| RecoveryError {
        plan_id: "legacy-custody".to_owned(),
        reason,
    })
}

/// Restore every quarantined legacy temp to its exact original location
/// (finding r5 3): the answer of every refusal path after the guarded
/// window took custody. Bytes are preserved, never deleted unproven.
pub(crate) fn restore_quarantine(guard: &mut FormatGuard) {
    if guard.custody.is_empty() {
        return;
    }
    let write = FormatWrite::new(guard.root.clone());
    write.restore_legacy_custody(&guard.custody);
    guard.custody.clear();
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

/// Target-defined open(2) flag values for the ownership open (finding
/// r4 10): the former single Linux spelling silently disabled the
/// no-follow protection on Apple targets — Linux `0o400000` is Darwin
/// `O_NOCTTY`, and the close-on-exec values differ too (XNU fcntl.h:
/// `O_NOFOLLOW` `0x00000100`, `O_CLOEXEC` `0x01000000`). Each supported
/// target states its own ABI values; any other unix target refuses to
/// compile instead of silently misflagging the open.
#[cfg(unix)]
mod open_flags {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub const NOFOLLOW: i32 = 0o400000; // 0x020000
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub const CLOEXEC: i32 = 0o2000000; // 0x080000
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos",
        target_os = "watchos"
    ))]
    pub const NOFOLLOW: i32 = 0x0000_0100;
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos",
        target_os = "watchos"
    ))]
    pub const CLOEXEC: i32 = 0x0100_0000;
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos",
        target_os = "watchos"
    )))]
    std::compile_error!(
        "unsupported unix target: no verified O_NOFOLLOW/O_CLOEXEC values for this ABI"
    );
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
    // The verified backup bytes of every pinned file, collected in the
    // preflight below and consumed by the semantic-neutrality oracle and
    // the restore loop.
    let mut preflight: Vec<(String, Vec<u8>)> = Vec::with_capacity(manifest.files.len());
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
        preflight.push((file.path.clone(), backup));
    }
    // Semantic neutrality of the restore, decided BEFORE any write
    // (finding r4 5): a forged record can digest-pin a semantic edit as
    // its backup, and restoring it would change the Model ahead of the
    // formatter's own oracle. The normal oracle over the current tree
    // must equal the oracle over the projected post-restore tree (every
    // pinned file swapped to its verified backup); a forged backup can
    // only pass by producing identical Model bytes — exactly the
    // formatter's own no-semantic-change invariant. An oracle that
    // refuses either side (a malformed or unadmittable backup) is the
    // same fail-closed refusal, never a partial restore.
    {
        let snapshot =
            crate::loader::load_validated_root_opt(write.root_path(), true).map_err(|_| {
                RecoveryFailure::Recovery(RecoveryError {
                    plan_id: plan_id.to_owned(),
                    reason: "restore-oracle-refused",
                })
            })?;
        let model_version = snapshot.model.model_version;
        let mut overlay: Vec<crate::loader::LoadedDocument> =
            Vec::with_capacity(snapshot.documents.len());
        for document in &snapshot.documents {
            let mut bytes = document.bytes.clone();
            for (path, backup) in &preflight {
                if path == &document.document.path {
                    bytes = backup.clone();
                }
            }
            overlay.push(crate::loader::LoadedDocument {
                document: document.document.clone(),
                bytes,
            });
        }
        let neutrality = match (
            super::semantic_oracle(&snapshot.documents, model_version),
            super::semantic_oracle(&overlay, model_version),
        ) {
            (Ok(before_state), Ok(after_state)) => before_state == after_state,
            _ => false,
        };
        if !neutrality {
            return Err(RecoveryFailure::Recovery(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "restore-semantic",
            }));
        }
    }
    // Verified restore: every manifest file back to its before digest,
    // reverse order, from the preflight-verified backups. Custody is
    // re-derived live immediately before every replacement (finding r5
    // 1): the entry preflight proved each destination's bytes before the
    // first restore, but the ownership lock serializes formatters —
    // never editors — so a destination whose live bytes became neither
    // pinned state mid-restore is preserved untouched and the run ends
    // in the explicit conflict decision with the journal retained,
    // exactly like the entry preflight.
    for file in manifest.files.iter().rev() {
        match rollback_custody(write, file) {
            RollbackCustody::OwnedAfter => {}
            RollbackCustody::AlreadyBefore => continue,
            RollbackCustody::Conflict {
                path,
                expected,
                current,
            } => {
                return Err(RecoveryFailure::Conflict {
                    path,
                    expected,
                    current,
                })
            }
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

    // Phase 0 (finding r5 2): the producer and the bounded recovery
    // reader agree on ONE record grammar, decided before any runtime
    // write. The reader parses journal/manifest records only within
    // RECORD_BOUND bytes and RECORD_MAX_FILES rows, so a plan whose
    // serialized manifest exceeds that bound could otherwise be admitted,
    // half-applied, crashed, and left blocking every reader with
    // `manifest-unreadable` forever — an interrupted accepted plan no
    // explicit format could ever recover. The whole recoverable record
    // set is serialized here, size- and row-bounded, and round-tripped
    // through the reader's exact decoder and validation: admission
    // refuses what recovery cannot read.
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
    let manifest_bytes = match serde_json::to_vec(&manifest) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Err(ApplyFailure::WriteFailed {
                stage: "manifest",
                rollback: "not-started",
            })
        }
    };
    if manifest.files.len() > RECORD_MAX_FILES {
        return Err(ApplyFailure::PlanLimit {
            limit: "max-files",
            measured: manifest.files.len() as u64,
            allowed: RECORD_MAX_FILES as u64,
        });
    }
    if manifest_bytes.len() as u64 > RECORD_BOUND {
        return Err(ApplyFailure::PlanLimit {
            limit: "max-record-bytes",
            measured: manifest_bytes.len() as u64,
            allowed: RECORD_BOUND,
        });
    }
    // The reader's own decoder and validation must accept exactly these
    // bytes (manifest round-trip through recovery's grammar): a record
    // that cannot round-trip is never published.
    if serde_json::from_slice::<Manifest>(&manifest_bytes)
        .ok()
        .and_then(|record| record.validate(plan_id).ok())
        .is_none()
    {
        return Err(ApplyFailure::WriteFailed {
            stage: "manifest",
            rollback: "not-started",
        });
    }
    let journal = Journal {
        schema_version: RUNTIME_SCHEMA.to_owned(),
        plan_id: plan_id.to_owned(),
        manifest_sha256: manifest.digest(),
    };
    let journal_bytes = match serde_json::to_vec(&journal) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Err(ApplyFailure::WriteFailed {
                stage: "journal",
                rollback: "not-started",
            })
        }
    };
    if journal_bytes.len() as u64 > RECORD_BOUND {
        return Err(ApplyFailure::PlanLimit {
            limit: "max-record-bytes",
            measured: journal_bytes.len() as u64,
            allowed: RECORD_BOUND,
        });
    }
    if journal.validate(plan_id).is_err() {
        return Err(ApplyFailure::WriteFailed {
            stage: "journal",
            rollback: "not-started",
        });
    }

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

    // Phase 3: the backup manifest, then the durable journal — the
    // exact Phase 0 bytes, already bounded and round-tripped through the
    // recovery reader's grammar (finding r5 2).
    write
        .write_new(&format!("{home}/{MANIFEST}"), &manifest_bytes)
        .map_err(|_| ApplyFailure::WriteFailed {
            stage: "manifest",
            rollback: "not-started",
        })?;
    let journal_path = format!("{home}/{JOURNAL}");
    write
        .write_new(&journal_path, &journal_bytes)
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
            let outcome = restore_prefix(write, plan_id, &manifest, index);
            return Err(rollback_failure(write, &journal_path, "replace", outcome));
        }
    }

    // Phase 5: post-apply verification through the same read path.
    for (index, (path, _, after)) in files.iter().enumerate() {
        let verified = match write.read_file(path) {
            Ok(bytes) => crate::digest::sha256_hex(&bytes) == *after && bytes == after_bytes[index],
            Err(_) => false,
        };
        if !verified {
            let outcome = restore_prefix(write, plan_id, &manifest, files.len());
            return Err(rollback_failure(write, &journal_path, "verify", outcome));
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

/// The terminal outcome of an ordinary rollback (finding r5 1): full
/// verified restoration, a typed custody conflict that preserves every
/// unowned byte and retains the journal, or the fail-closed recovery
/// error. A rollback never declares success over bytes it cannot prove
/// ownership of.
enum Rollback {
    /// Every destination was proven owned (or already restored) and
    /// rewritten to its verified before digest; the journal may clear.
    Restored,
    /// A destination's live bytes are neither pinned state — a third
    /// digest or a deletion. They are preserved untouched and the
    /// journal stays armed for the explicit conflict decision.
    Conflict {
        path: String,
        expected: String,
        current: String,
    },
    /// The rollback material itself failed; fail closed through the
    /// recovery refusal with the journal retained.
    Failed(RecoveryError),
}

/// The live custody verdict of one rollback destination (finding r5 1):
/// the bytes a restore would overwrite, recomputed now — never a stale
/// journal claim. The formatter ownership lock serializes formatter
/// instances; it does not prevent an editor from writing a source, so
/// only bytes that are provably this transaction's own last write (the
/// recorded after digest) — or the already-restored before state — may
/// be overwritten. Any third digest, and any deletion, is preserved.
enum RollbackCustody {
    /// The destination still carries this transaction's own last write.
    OwnedAfter,
    /// The destination is already at its recorded before digest.
    AlreadyBefore,
    /// Third bytes or a deletion this transaction cannot prove ownership
    /// of: preserve them and end in the explicit conflict decision.
    Conflict {
        path: String,
        expected: String,
        current: String,
    },
}

fn rollback_custody(write: &FormatWrite, file: &PlanFile) -> RollbackCustody {
    let Ok(current) = write.read_file(&file.path) else {
        // The most severe divergence of the same class (finding r2 12):
        // a deleted destination is a user decision, never a restore
        // target — recreating it would destroy that decision.
        return RollbackCustody::Conflict {
            path: file.path.clone(),
            expected: file.before_sha256.clone(),
            current: DELETED_DIGEST.to_owned(),
        };
    };
    let digest = crate::digest::sha256_hex(&current);
    if digest == file.before_sha256 {
        RollbackCustody::AlreadyBefore
    } else if digest == file.after_sha256 {
        RollbackCustody::OwnedAfter
    } else {
        RollbackCustody::Conflict {
            path: file.path.clone(),
            expected: file.before_sha256.clone(),
            current: digest,
        }
    }
}

/// Restore the manifest's first `prefix` files (the completed set) in
/// reverse order from verified backups. Every destination's live bytes
/// are checked before the first restore and re-derived immediately
/// before each replacement (finding r5 1): a concurrent user edit — or
/// deletion — of an already replaced file is preserved untouched, the
/// journal is retained, and the caller ends in the typed conflict
/// decision instead of declaring a restoration that never happened.
fn restore_prefix(
    write: &FormatWrite,
    plan_id: &str,
    manifest: &Manifest,
    prefix: usize,
) -> Rollback {
    let home = format!("{FORMAT_HOME}/{plan_id}");
    let files = &manifest.files[..prefix.min(manifest.files.len())];
    // Preflight every destination's live custody before the first
    // restore: a rollback that cannot prove ownership of one
    // destination restores nothing, so a conflicted tree keeps every
    // byte exactly where the failure left it.
    for file in files {
        if let RollbackCustody::Conflict {
            path,
            expected,
            current,
        } = rollback_custody(write, file)
        {
            return Rollback::Conflict {
                path,
                expected,
                current,
            };
        }
    }
    for file in files.iter().rev() {
        match rollback_custody(write, file) {
            RollbackCustody::OwnedAfter => {}
            RollbackCustody::AlreadyBefore => continue,
            RollbackCustody::Conflict {
                path,
                expected,
                current,
            } => {
                return Rollback::Conflict {
                    path,
                    expected,
                    current,
                }
            }
        }
        let backup_path = format!("{home}/before/{}", backup_logical(&file.path));
        let backup = match write.read_file(&backup_path) {
            Ok(backup) => backup,
            Err(_) => {
                return Rollback::Failed(RecoveryError {
                    plan_id: plan_id.to_owned(),
                    reason: "backup-unreadable",
                })
            }
        };
        if crate::digest::sha256_hex(&backup) != file.before_sha256 {
            return Rollback::Failed(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "backup-mismatch",
            });
        }
        if write.replace_file(&file.path, &backup, "restore").is_err() {
            return Rollback::Failed(RecoveryError {
                plan_id: plan_id.to_owned(),
                reason: "restore-failed",
            });
        }
    }
    Rollback::Restored
}

/// The terminal apply failure of one rollback outcome (finding r5 1):
/// only a fully verified restoration clears the journal and reports
/// `rollback=restored`. A custody conflict preserves the concurrent
/// bytes, keeps the journal armed, and surfaces as the registered
/// source-changed denial; material failures stay the recovery refusal.
fn rollback_failure(
    write: &FormatWrite,
    journal_path: &str,
    stage: &'static str,
    outcome: Rollback,
) -> ApplyFailure {
    match outcome {
        Rollback::Restored => {
            let _ = write.remove_file(journal_path);
            ApplyFailure::WriteFailed {
                stage,
                rollback: "restored",
            }
        }
        Rollback::Conflict {
            path,
            expected,
            current,
        } => ApplyFailure::SourceChanged {
            path,
            expected,
            current,
        },
        Rollback::Failed(error) => ApplyFailure::RecoveryError(error),
    }
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
        // The policy pin is an authenticity claim, not just a shape
        // (finding r4 5): only the digest of the embedded contract this
        // binary plans under is supported. A self-consistent forged
        // record naming any other policy — the all-zeros spelling the
        // probes used — refused here, before any restore runs.
        if self.policy_digest != supported_policy_digest() {
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

    /// The validated project root the capability was derived from (the
    /// recovery oracle re-reads the tree through the loader's own path).
    fn root_path(&self) -> PathBuf {
        self.root.clone()
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
            // E0716). The flag values are target ABI constants, not
            // portable spellings (finding r4 10): Linux and the Apple
            // targets disagree, so each target states its own values
            // and any other unix target fails to compile instead of
            // silently misflagging the open.
            const O_NOFOLLOW: i32 = open_flags::NOFOLLOW;
            const O_CLOEXEC: i32 = open_flags::CLOEXEC;
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
                    return Ok(FormatGuard {
                        file,
                        fresh,
                        root: self.root.clone(),
                        custody: Vec::new(),
                    });
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
            Ok(FormatGuard {
                file,
                fresh,
                root: self.root.clone(),
                custody: Vec::new(),
            })
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
                    return Ok(FormatGuard {
                        file,
                        fresh,
                        root: self.root.clone(),
                        custody: Vec::new(),
                    });
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
            Ok(FormatGuard {
                file,
                fresh,
                root: self.root.clone(),
                custody: Vec::new(),
            })
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
    /// and never followed or removed. The quarantine home
    /// (`temps/legacy`) is a directory here and never matches the
    /// producer suffix, so custody is never swept (finding r5 3).
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

    /// Whether a regular reparse-free file with this exact name exists
    /// directly inside one logical directory.
    fn regular_file_in(&self, directory: &str, name: &str) -> bool {
        let Ok(dir_path) = self.physical(directory, false) else {
            return false;
        };
        match std::fs::symlink_metadata(dir_path.join(name)) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return false;
                }
                #[cfg(windows)]
                {
                    if reparse_point(&metadata) {
                        return false;
                    }
                }
                true
            }
            Err(_) => false,
        }
    }

    /// The closed set of legacy temps this run can PROVE structural
    /// ownership of (findings r4 4, r5 3): the exact emitted-document
    /// temp spelling, in a directory the governed grammar admits, whose
    /// module marker and whose corresponding source document both
    /// exist as real reparse-free files. Enumeration never widens: a
    /// hidden or otherwise non-admitted directory name (devin 2), a
    /// portable but marker-less directory, or an absent source document
    /// (finding r5 3) contribute nothing — those files are not provably
    /// formatter-owned, so they never move and never delete. The final
    /// ownership decision still belongs to the validated admitted
    /// inventory (`settle_legacy_custody`).
    fn legacy_quarantine_candidates(&self) -> Vec<CustodyMove> {
        let mut moves: Vec<CustodyMove> = Vec::new();
        // The project document's sibling at the `lekalo/` root.
        if self.physical("lekalo/project.yaml", true).is_ok()
            && self.regular_file_in("lekalo", &format!(".project.yaml.{TEMP_SUFFIX}"))
        {
            moves.push(CustodyMove {
                directory: "lekalo".to_owned(),
                name: format!(".project.yaml.{TEMP_SUFFIX}"),
            });
        }
        // The module directories: grammar-admitted names that carry the
        // module marker (an admitted module), and only the kind stems
        // whose source document actually exists in that directory.
        if let Ok(modules) = self.physical("lekalo/modules", false) {
            if let Ok(read) = std::fs::read_dir(&modules) {
                for entry in read.flatten() {
                    let Ok(metadata) = entry.metadata() else {
                        continue;
                    };
                    if !metadata.is_dir() || metadata.file_type().is_symlink() {
                        continue;
                    }
                    #[cfg(windows)]
                    {
                        if reparse_point(&metadata) {
                            continue;
                        }
                    }
                    let Some(directory_name) = entry.file_name().to_str().map(str::to_owned) else {
                        continue;
                    };
                    // The governed module-segment grammar (devin 2): the
                    // same predicate `project_fs` admits module
                    // directories under. Hidden and otherwise
                    // non-portable spellings can never have emitted a
                    // document, so they are never enumerated.
                    if crate::project_fs::path_violation(&directory_name).is_some() {
                        continue;
                    }
                    let directory = format!("lekalo/modules/{directory_name}");
                    // The admitted-module marker: no marker, no module,
                    // no ownership (finding r5 3).
                    if self
                        .physical(&format!("{directory}/module.yaml"), true)
                        .is_err()
                    {
                        continue;
                    }
                    for stem in crate::project_fs::MODULE_STEMS {
                        let document = format!("{stem}.yaml");
                        // The source document the temp renamed: an absent
                        // sibling is a user file the legacy emitter never
                        // wrote (finding r5 3).
                        if self
                            .physical(&format!("{directory}/{document}"), true)
                            .is_err()
                        {
                            continue;
                        }
                        let name = format!(".{document}.{TEMP_SUFFIX}");
                        if self.regular_file_in(&directory, &name) {
                            moves.push(CustodyMove {
                                directory: directory.clone(),
                                name,
                            });
                        }
                    }
                }
            }
        }
        moves.sort();
        moves.dedup();
        moves
    }

    /// The custody location token of one move's directory: the reserved
    /// root token at the `lekalo/` root, else the module directory name.
    /// Both spellings satisfy the governed path-segment grammar (the
    /// `.lekalo` runtime tree is scanned under it), and a root/module
    /// token collision is harmless: root custody files are always the
    /// `project.yaml` document spelling, module custody files always a
    /// module-stem spelling.
    fn custody_token(directory: &str) -> String {
        if directory == "lekalo" {
            CUSTODY_ROOT_TOKEN.to_owned()
        } else {
            directory
                .rsplit_once('/')
                .map(|(_, token)| token.to_owned())
                .unwrap_or_else(|| CUSTODY_ROOT_TOKEN.to_owned())
        }
    }

    /// The custody file-name spelling of one legacy temp: the original
    /// name without its leading dot, so the quarantined file itself
    /// satisfies the governed path-segment grammar the `.lekalo` runtime
    /// tree is scanned under (finding r5 3).
    fn custody_file_name(name: &str) -> String {
        name.strip_prefix('.').unwrap_or(name).to_owned()
    }

    /// The logical custody path of one move's quarantined file.
    fn custody_path(&self, pending: &CustodyMove) -> String {
        format!(
            "{CUSTODY_HOME}/{}/{}",
            Self::custody_token(&pending.directory),
            Self::custody_file_name(&pending.name)
        )
    }

    /// Read and fully validate the durable custody record, if any. A
    /// record that fails its closed validation is fail-closed custody
    /// state: the caller refuses rather than guesses (finding A
    /// discipline).
    fn read_custody_record(&self) -> Result<Option<CustodyRecord>, &'static str> {
        let Ok(bytes) =
            self.read_file_bounded(&format!("{CUSTODY_HOME}/{CUSTODY_RECORD}"), RECORD_BOUND)
        else {
            return Ok(None);
        };
        let record: CustodyRecord =
            serde_json::from_slice(&bytes).map_err(|_| "custody-malformed")?;
        record.validate()?;
        Ok(Some(record))
    }

    /// Durably replace the custody record: written to a fresh exclusive
    /// temp name and renamed over the record, so a crash can never leave
    /// a torn or missing authority while custody files exist (the old
    /// record stays a valid superset until the rename lands).
    fn write_custody_record(&self, record: &CustodyRecord) -> Result<(), ()> {
        let bytes = serde_json::to_vec(record).map_err(|_| ())?;
        self.ensure_dir(CUSTODY_HOME)?;
        let staged = format!("{CUSTODY_HOME}/{CUSTODY_RECORD}.staged");
        self.remove_file(&staged).ok();
        self.write_new(&staged, &bytes)?;
        let record_logical = format!("{CUSTODY_HOME}/{CUSTODY_RECORD}");
        let (record_dir, _) = split_logical(&record_logical);
        let Ok(dir_path) = self.physical(record_dir, false) else {
            return Err(());
        };
        let staged_path = self.physical(&staged, true)?;
        std::fs::rename(&staged_path, dir_path.join(CUSTODY_RECORD)).map_err(|_| ())?;
        sync_dir(&dir_path);
        Ok(())
    }

    /// Quarantine every structurally provable legacy temp (finding r5
    /// 3): the durable record is written FIRST, then each temp is moved
    /// (renamed, never copied or deleted) into runtime custody so the
    /// governed tree can be structurally admitted. The caller holds the
    /// exclusive ownership lock. Returns the pending moves; the caller
    /// settles them against the validated admitted inventory. An
    /// interrupted quarantine is re-decided from the record on the next
    /// run: moves already in custody are kept, moves already committed
    /// or gone are dropped, and a stale empty record is removed.
    fn quarantine_legacy_temps(&self) -> Result<Vec<CustodyMove>, RecoveryError> {
        let refuse = |reason: &'static str| RecoveryError {
            plan_id: "legacy-custody".to_owned(),
            reason,
        };
        let recorded = match self.read_custody_record() {
            Ok(Some(record)) => record.moves,
            Ok(None) => Vec::new(),
            Err(reason) => return Err(refuse(reason)),
        };
        let mut pending: Vec<CustodyMove> = recorded;
        pending.extend(self.legacy_quarantine_candidates());
        pending.retain(|held| {
            self.regular_file_in(&held.directory, &held.name)
                || self.physical(&self.custody_path(held), true).is_ok()
        });
        pending.sort();
        pending.dedup();
        if pending.is_empty() {
            // Nothing in custody and nothing provable in the tree: a
            // stale record (every move committed or restored before its
            // removal) is reclaimed, and the custody home is cleaned.
            self.remove_file(&format!("{CUSTODY_HOME}/{CUSTODY_RECORD}"))
                .ok();
            self.clean_custody_homes();
            return Ok(Vec::new());
        }
        let record = CustodyRecord {
            schema_version: CUSTODY_SCHEMA.to_owned(),
            moves: pending.clone(),
        };
        if let Err(reason) = record.validate() {
            return Err(refuse(reason));
        }
        self.write_custody_record(&record)
            .map_err(|_| refuse("custody-record"))?;
        for held in &pending {
            let in_tree = self.regular_file_in(&held.directory, &held.name);
            let custody_logical = self.custody_path(held);
            let in_custody = self.physical(&custody_logical, true).is_ok();
            match (in_tree, in_custody) {
                // Already moved by an interrupted earlier run.
                (false, true) => continue,
                // Committed or gone: nothing to move.
                (false, false) => continue,
                // A file re-appeared at the exact proven spelling while
                // the original is still in custody: two live claims on
                // one name. Fail closed — never delete either.
                (true, true) => return Err(refuse("custody-conflict")),
                (true, false) => {}
            }
            let token_dir = format!("{CUSTODY_HOME}/{}", Self::custody_token(&held.directory));
            self.ensure_dir(&token_dir)
                .map_err(|_| refuse("custody-move"))?;
            let source = self
                .physical(&held.source_path(), true)
                .map_err(|_| refuse("custody-move"))?;
            let target = self
                .physical(&token_dir, false)
                .map_err(|_| refuse("custody-move"))?;
            std::fs::rename(&source, target.join(Self::custody_file_name(&held.name)))
                .map_err(|_| refuse("custody-move"))?;
            if let Ok(dir_path) = self.physical(&held.directory, false) {
                sync_dir(&dir_path);
            }
        }
        Ok(pending)
    }

    /// Settle the quarantine against the validated admitted inventory
    /// (finding r5 3): a quarantined temp is DELETED only when its
    /// emitted document is part of the load's admitted inventory — the
    /// completed ownership proof — and restored byte-exact otherwise.
    /// Called by the apply pipeline immediately after its load
    /// succeeded; the caller holds the ownership lock.
    fn settle_legacy_custody(
        &self,
        custody: &[CustodyMove],
        admitted: &[String],
    ) -> Result<(), &'static str> {
        for pending in custody {
            let custody_logical = self.custody_path(pending);
            if admitted.contains(&pending.document_path()) {
                // The completed proof: exact spelling, legal location,
                // existing source, validated admission. Delete.
                self.remove_file(&custody_logical)
                    .map_err(|_| "custody-commit")?;
            } else {
                // Not admitted after all: restore the exact bytes to
                // their exact original location.
                self.restore_one_custody(pending)
                    .map_err(|_| "custody-restore")?;
            }
        }
        self.remove_file(&format!("{CUSTODY_HOME}/{CUSTODY_RECORD}"))
            .map_err(|_| "custody-commit")?;
        self.clean_custody_homes();
        Ok(())
    }

    /// Restore every quarantined temp to its exact original location
    /// (finding r5 3): the answer of every refusal path. A move whose
    /// original location is occupied stays in custody with a rewritten
    /// record naming exactly the un-restored moves — fail closed, never
    /// overwrite, never orphan custody without its durable authority.
    fn restore_legacy_custody(&self, custody: &[CustodyMove]) {
        let mut unrestored: Vec<CustodyMove> = Vec::new();
        for pending in custody {
            if self.restore_one_custody(pending).is_err() {
                unrestored.push(pending.clone());
            }
        }
        if unrestored.is_empty() {
            self.remove_file(&format!("{CUSTODY_HOME}/{CUSTODY_RECORD}"))
                .ok();
            self.clean_custody_homes();
        } else {
            let _ = self.write_custody_record(&CustodyRecord {
                schema_version: CUSTODY_SCHEMA.to_owned(),
                moves: unrestored,
            });
            // The occupied originals keep their custody files and their
            // durable record; the next run re-decides them.
        }
    }

    /// Best-effort restore of every recorded move (used when the
    /// quarantine pass itself fails and no guard carries the moves).
    fn restore_legacy_custody_best_effort(&self) {
        if let Ok(Some(record)) = self.read_custody_record() {
            self.restore_legacy_custody(&record.moves);
        }
    }

    /// Restore one quarantined temp by rename to its original logical
    /// path. Fails (leaving the bytes in custody) when the target is
    /// occupied or any path check refuses.
    fn restore_one_custody(&self, pending: &CustodyMove) -> Result<(), ()> {
        let custody_logical = self.custody_path(pending);
        let source = self.physical(&custody_logical, true)?;
        let target_dir = self.physical(&pending.directory, false)?;
        let target = target_dir.join(&pending.name);
        if std::fs::symlink_metadata(&target).is_ok() {
            return Err(());
        }
        std::fs::rename(&source, &target).map_err(|_| ())?;
        sync_dir(&target_dir);
        Ok(())
    }

    /// Best-effort removal of the empty custody directories after every
    /// move was committed or restored. The runtime temp home itself is
    /// removed too when the quarantine was its only occupant, so a
    /// quarantining run leaves no empty runtime residue behind.
    fn clean_custody_homes(&self) {
        if let Ok(home) = self.physical(CUSTODY_HOME, false) {
            if let Ok(read) = std::fs::read_dir(&home) {
                for entry in read.flatten() {
                    if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                        let _ = std::fs::remove_dir(entry.path());
                    }
                }
            }
        }
        let _ = self.remove_dir(CUSTODY_HOME);
        let _ = std::fs::remove_dir(
            self.physical(TEMP_HOME, false)
                .unwrap_or_else(|_| PathBuf::new()),
        );
    }

    /// Whether reclaimable crash residue exists anywhere the owned
    /// window looks (findings r3 4, r5 3): an orphaned runtime temp, a
    /// structurally provable legacy source-tree temp, or quarantine
    /// custody state (a durable record or a custody file from an
    /// interrupted quarantine). The read-only probe behind the recovery
    /// entry decision: stranded residue keeps the project unloadable
    /// exactly as a journal would, so a state-less tree carrying it must
    /// still reach the owned window, and an interrupted quarantine must
    /// always be re-decided. Anomalies (links, hostile spellings) are
    /// skipped here — the owned window refuses them instead.
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
        // Quarantine custody state: a durable record, or any custody
        // file of an interrupted quarantine (finding r5 3).
        if self
            .physical(&format!("{CUSTODY_HOME}/{CUSTODY_RECORD}"), true)
            .is_ok()
        {
            return true;
        }
        if let Ok(home) = self.physical(CUSTODY_HOME, false) {
            if let Ok(read) = std::fs::read_dir(&home) {
                for entry in read.flatten() {
                    let Ok(metadata) = entry.metadata() else {
                        continue;
                    };
                    if !metadata.is_dir() || metadata.file_type().is_symlink() {
                        continue;
                    }
                    #[cfg(windows)]
                    {
                        if reparse_point(&metadata) {
                            continue;
                        }
                    }
                    let Ok(tokens) = std::fs::read_dir(entry.path()) else {
                        continue;
                    };
                    for token in tokens.flatten() {
                        let Ok(metadata) = token.metadata() else {
                            continue;
                        };
                        if !metadata.is_file() || metadata.file_type().is_symlink() {
                            continue;
                        }
                        if let Some(name) = token.file_name().to_str() {
                            if is_legacy_custody_name(name) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        // Structurally provable legacy temps still stranded in the
        // governed tree.
        !self.legacy_quarantine_candidates().is_empty()
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
    // untouched. The custody spelling (finding r5 3) is the same grammar
    // minus the leading dot, so the quarantined file itself satisfies
    // the governed path-segment grammar of the `.lekalo` runtime tree.
    #[test]
    fn legacy_temp_names_are_limited_to_emitted_documents() {
        assert!(super::is_legacy_temp_name(
            ".project.yaml.lekalo-format-new"
        ));
        for kind in crate::project_fs::KIND_STEMS {
            assert!(super::is_legacy_temp_name(&format!(
                ".{kind}.yaml.lekalo-format-new"
            )));
        }
        assert!(super::is_legacy_temp_name(".module.yaml.lekalo-format-new"));
        // Unrelated user files are never owned.
        assert!(!super::is_legacy_temp_name(
            ".private-notes.lekalo-format-new"
        ));
        assert!(!super::is_legacy_temp_name(
            ".unrelated.txt.lekalo-format-new"
        ));
        assert!(!super::is_legacy_temp_name(
            ".invented.yaml.lekalo-format-new"
        ));
        // The custody spelling is the dot-less mirror of the same set.
        assert!(super::is_legacy_custody_name(
            "project.yaml.lekalo-format-new"
        ));
        assert!(super::is_legacy_custody_name(
            "module.yaml.lekalo-format-new"
        ));
        assert!(!super::is_legacy_custody_name(
            "private-notes.lekalo-format-new"
        ));
        // The owned temp (non-legacy) family stays intact.
        assert!(super::is_owned_temp_name(
            "plan.pid.apply.module.yaml.lekalo-format-new"
        ));
    }

    // The custody record's closed validation (finding r5 3): only the
    // two legal legacy locations, grammar-admitted module directories,
    // and unique moves pass.
    #[test]
    fn custody_records_accept_only_proven_locations() {
        let record = |moves: &[(&str, &str)]| super::CustodyRecord {
            schema_version: super::CUSTODY_SCHEMA.to_owned(),
            moves: moves
                .iter()
                .map(|(directory, name)| super::CustodyMove {
                    directory: directory.to_string(),
                    name: name.to_string(),
                })
                .collect(),
        };
        assert!(record(&[
            ("lekalo", ".project.yaml.lekalo-format-new"),
            (
                "lekalo/modules/scheduling",
                ".module.yaml.lekalo-format-new"
            ),
        ])
        .validate()
        .is_ok());
        // Hidden and otherwise non-admitted directories are refused
        // (devin 2), as are invented locations, unrelated names, wrong
        // identities, duplicates, and empty records.
        assert!(
            record(&[("lekalo/modules/.evil", ".module.yaml.lekalo-format-new")])
                .validate()
                .is_err()
        );
        // The name grammar is location-aware: the `project.yaml`
        // document spelling belongs to the root alone, module-stem
        // spellings to module directories alone — no two valid moves
        // can collide on one custody file path.
        assert!(
            record(&[(
                "lekalo/modules/0lekalo-root",
                ".project.yaml.lekalo-format-new",
            )])
            .validate()
            .is_err(),
            "a module directory never carries the root document spelling"
        );
        assert!(
            record(&[("lekalo", ".module.yaml.lekalo-format-new")])
                .validate()
                .is_err(),
            "the root never carries a module-stem spelling"
        );
        assert!(
            record(&[("lekalo/targets", ".project.yaml.lekalo-format-new")])
                .validate()
                .is_err()
        );
        assert!(record(&[("lekalo", ".private-notes.lekalo-format-new")])
            .validate()
            .is_err());
        assert!(record(&[
            ("lekalo", ".project.yaml.lekalo-format-new"),
            ("lekalo", ".project.yaml.lekalo-format-new")
        ])
        .validate()
        .is_err());
        assert!(record(&[]).validate().is_err());
        let mut foreign = record(&[("lekalo", ".project.yaml.lekalo-format-new")]);
        foreign.schema_version = "lekalo/format-legacy-custody/v0.9.0".to_owned();
        assert!(foreign.validate().is_err());
        // The document projection drives the admission settle.
        let root = super::CustodyMove {
            directory: "lekalo".to_string(),
            name: ".project.yaml.lekalo-format-new".to_string(),
        };
        assert_eq!(root.document_path(), "lekalo/project.yaml");
        let module = super::CustodyMove {
            directory: "lekalo/modules/scheduling".to_string(),
            name: ".entities.yaml.lekalo-format-new".to_string(),
        };
        assert_eq!(
            module.document_path(),
            "lekalo/modules/scheduling/entities.yaml"
        );
        assert_eq!(
            module.source_path(),
            "lekalo/modules/scheduling/.entities.yaml.lekalo-format-new"
        );
    }
}

#[cfg(test)]
mod transaction_probes {
    //! Direct probes of the confined transaction (findings r5 1, r5 2).
    //! Sandboxes live under `target/` and are addressed through the
    //! invocation-relative project selector, so no working-directory
    //! mutation is needed.
    use super::*;

    fn sandbox(name: &str) -> (String, PathBuf) {
        let selector = format!("target/lekalo-format-txn-{name}-{}", std::process::id());
        let root = PathBuf::from(&selector);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("lekalo/modules")).expect("sandbox tree");
        std::fs::write(
            root.join("lekalo/project.yaml"),
            b"schema_version: '0.2.16'\n",
        )
        .expect("project document");
        (selector, root)
    }

    fn plan_homes(root: &Path) -> Vec<String> {
        let home = root.join(".lekalo/cache/format");
        let Ok(read) = std::fs::read_dir(&home) else {
            return Vec::new();
        };
        let mut names: Vec<String> = read
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != ACTIVE_LOCK && name != TEMP_HOME_NAME)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_rollback_preserves_bytes_it_cannot_prove_ownership_of() {
        // Finding r5 1 (the post-apply verification caller): a verify
        // failure rolls back the full replaced prefix; a destination
        // whose live bytes are neither pinned state — here the
        // formatter's own phase-4 bytes under a mismatched after pin —
        // must be preserved, the journal retained, and the run ended in
        // the typed conflict denial instead of a declared restoration.
        let (selector, root) = sandbox("rollback-custody");
        let source = root.join("lekalo/modules/scheduling/module.yaml");
        std::fs::create_dir_all(source.parent().expect("parent")).expect("module dir");
        let original = b"original: bytes\n";
        std::fs::write(&source, original).expect("seed");
        let before = crate::digest::sha256_hex(original);
        let candidate = b"replacement: bytes\n";
        let live = crate::digest::sha256_hex(candidate);
        // The constructed inconsistency: the plan's after pin names a
        // third spelling, so phase 5's verification fails and rollback
        // finds live bytes that are neither pin.
        let forged_after = crate::digest::sha256_hex(b"neither pin");
        let files = vec!["lekalo/modules/scheduling/module.yaml".to_owned()]
            .into_iter()
            .map(|path| (path, before.clone(), forged_after.clone()))
            .collect::<Vec<_>>();
        let policy_digest = supported_policy_digest().to_owned();
        let failure = apply(
            &Some(selector.clone()),
            &policy_digest,
            &files,
            &[candidate.to_vec()],
            None,
        )
        .expect_err("the verify stage fails");
        match failure {
            ApplyFailure::SourceChanged {
                path,
                expected,
                current,
            } => {
                assert_eq!(path, "lekalo/modules/scheduling/module.yaml");
                assert_eq!(expected, before, "the conflict names the before pin");
                assert_eq!(current, live, "the conflict names the live third bytes");
            }
            other => panic!("the typed conflict denial, got {other:?}"),
        }
        let homes = plan_homes(&root);
        assert_eq!(homes.len(), 1, "one plan home");
        assert!(
            root.join(".lekalo/cache/format")
                .join(&homes[0])
                .join(JOURNAL)
                .exists(),
            "the journal stays armed for the explicit conflict decision"
        );
        assert_eq!(
            std::fs::read(&source).expect("source"),
            candidate,
            "the unowned live bytes are preserved untouched"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_oversized_plan_is_refused_before_any_runtime_write() {
        // Finding r5 2: the producer and the bounded recovery reader
        // agree on one record grammar. A plan whose serialized manifest
        // exceeds the 256 KiB record bound — reachable far below the
        // 4096-file policy limit, here with long valid 64-character
        // module names — is refused whole with the registered
        // bounded-plan failure, before any backup, journal, or
        // replacement exists.
        let (selector, root) = sandbox("record-bound");
        let digest = crate::digest::sha256_hex(b"payload");
        let files: Vec<(String, String, String)> = (0..1100)
            .map(|index| {
                let directory = format!("{index:04}{}", "n".repeat(60));
                (
                    format!("lekalo/modules/{directory}/module.yaml"),
                    digest.clone(),
                    digest.clone(),
                )
            })
            .collect();
        let policy_digest = supported_policy_digest().to_owned();
        let failure = apply(&Some(selector), &policy_digest, &files, &[], None)
            .expect_err("the oversized plan is refused");
        match failure {
            ApplyFailure::PlanLimit {
                limit,
                measured,
                allowed,
            } => {
                assert_eq!(limit, "max-record-bytes");
                assert_eq!(allowed, RECORD_BOUND);
                assert!(measured > RECORD_BOUND, "the manifest exceeds the bound");
            }
            other => panic!("the bounded-plan refusal, got {other:?}"),
        }
        let result = failure.into_result();
        assert_eq!(result.status(), crate::result::Status::Invalid);
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.id() == "format.limit-exceeded"),
            "the registered bounded-plan failure"
        );
        assert!(
            plan_homes(&root).is_empty(),
            "no plan home, journal, or backup was created"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_in_bound_plan_emits_a_manifest_the_recovery_reader_accepts() {
        // Finding r5 2 boundary control: a near-bound plan (long valid
        // module names, just under the record byte bound) applies, and
        // its emitted manifest is within the bounded reader's grammar —
        // size-bounded, decodable, and identity-valid — so an admitted
        // plan can never crash into an unreadable recovery record.
        let (selector, root) = sandbox("record-roundtrip");
        let payload = b"payload: bytes\n";
        let digest = crate::digest::sha256_hex(payload);
        let files: Vec<(String, String, String)> = (0..900)
            .map(|index| {
                let directory = format!("{index:04}{}", "n".repeat(60));
                let path = format!("lekalo/modules/{directory}/module.yaml");
                let physical = root.join(&path);
                std::fs::create_dir_all(physical.parent().expect("parent")).expect("dir");
                std::fs::write(&physical, payload).expect("source");
                (path, digest.clone(), digest.clone())
            })
            .collect();
        let after_bytes: Vec<Vec<u8>> = files.iter().map(|_| payload.to_vec()).collect();
        let policy_digest = supported_policy_digest().to_owned();
        apply(&Some(selector), &policy_digest, &files, &after_bytes, None)
            .expect("the in-bound plan applies");
        let homes = plan_homes(&root);
        assert_eq!(homes.len(), 1, "one committed plan home");
        let home = root.join(".lekalo/cache/format").join(&homes[0]);
        assert!(
            !home.join(JOURNAL).exists(),
            "the journal cleared at commit"
        );
        let bytes = std::fs::read(home.join(MANIFEST)).expect("manifest");
        assert!(
            bytes.len() as u64 <= RECORD_BOUND,
            "the emitted manifest is within the recovery reader bound"
        );
        let manifest: Manifest = serde_json::from_slice(&bytes).expect("decodes");
        manifest
            .validate(&homes[0])
            .expect("the reader's own validation accepts the emitted record");
        let _ = std::fs::remove_dir_all(&root);
    }
}
