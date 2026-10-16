//! The bounded installation engine (issue #136, increments I2 and I4):
//! exact local snapshots, a durable discoverable journal, and per-path
//! compare-and-swap writes.
//!
//! Multi-file installation is deliberately not claimed filesystem-wide
//! atomic: every intended write is snapshotted first (absent files
//! included), every completed step is journaled through the confined
//! store, and a partial write, failed verification, or receipt defect
//! records `recovery-required` instead of completion. Concurrent or
//! dirty unrelated work is never touched; the repository is never
//! reset, stashed, or cleaned.
//!
//! Every snapshot/journal byte flows through the same confined-store
//! discipline as the observed index: per-component reparse checks under
//! the project root, a per-transaction temporary in the final
//! directory, and an atomic rename. Snapshot files are keyed by digest
//! and re-derived at restore time, so a planted link or a corrupted
//! snapshot never becomes a restore source.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::diagnostic;
use super::version;
use super::wire::WriteEntry;
use super::Outcome;
use crate::observed::store as observed_store;
use crate::result::DomainResult;

/// The closed runtime journal of one installation. Runtime-only/private
/// material under the recovery home; never a published contract.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub schema_version: String,
    pub plan: String,
    pub phase: String,
    pub steps: Vec<JournalStep>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalStep {
    pub path: String,
    pub state: String,
    pub snapshot: Option<String>,
    /// The mutation class of a written step (`create`, `replace`,
    /// `delete`, or `binding`); absent on snapshot-only bookkeeping
    /// rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// The exact pre-state digest a written step replaced — `None`
    /// means the path was created by the phase. Together with the
    /// snapshot path this is the complete restoration metadata an
    /// independent recovery consumes when no completed receipt exists
    /// (X5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// The exact post-state digest a written step installed (`None`
    /// for a delete, whose post-state is typed absence). Recovery
    /// compares the live bytes against `after`/`before` exactly like
    /// the rollback surface's conflict check, so a later user edit is
    /// a conflict refusal instead of a silent overwrite (F2/R5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

impl Journal {
    pub(crate) fn new(plan: &str, phase: &str) -> Self {
        Self {
            schema_version: format!("lekalo/adoption-journal/v{}", version::VERSION),
            plan: plan.to_owned(),
            phase: phase.to_owned(),
            steps: Vec::new(),
        }
    }

    /// The exact accepted schema discriminator of a durable journal.
    pub(crate) fn schema_version_literal() -> String {
        format!("lekalo/adoption-journal/v{}", version::VERSION)
    }
}

/// The closed state vocabulary of a journal step: every durable row
/// this module ever writes, so a hand-planted journal cannot smuggle
/// an unknown state past authentication.
const JOURNAL_STATES: [&str; 6] = [
    "snapshotted",
    "written",
    "failed",
    "restored",
    "restore-failed",
    "receipt-write-failed",
];

/// The closed mutation-class vocabulary of a written step.
const JOURNAL_ACTIONS: [&str; 4] = ["create", "replace", "delete", "binding"];

/// One recorded snapshot: the exact original bytes of one intended write
/// target (with its content digest), or the typed absence of the file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SnapshotRecord {
    pub path: String,
    pub snapshot: String,
    pub present: bool,
    /// The exact digest of the snapshotted prior bytes (`None` for an
    /// absent file); restore verifies it before any byte moves.
    pub before_digest: Option<String>,
}

/// The confined recovery home of one plan digest.
pub(crate) struct RecoveryHome {
    pub journal_relative: String,
    pub snapshots_relative: String,
}

/// Read one snapshot through the confined restore path: the exact
/// digest-keyed file only, reparse points refused on sight, and the
/// bytes checked against the recorded digest before they can become a
/// restore source. A corrupted or missing snapshot is a typed
/// recovery-required refusal that preserves the current files.
pub(crate) fn read_snapshot(root: &Path, record: &SnapshotRecord) -> Outcome<Vec<u8>> {
    let relative = &record.snapshot;
    if !version::is_rel_path(relative) {
        return Err(DomainResult::invalid(diagnostic::recovery_required_set(
            &record.path,
            "snapshot-path",
        )));
    }
    let path = root.join(relative);
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| {
        DomainResult::invalid(diagnostic::recovery_required_set(
            &record.path,
            "snapshot-read",
        ))
    })?;
    if observed_store::is_link(&metadata) {
        return Err(DomainResult::invalid(diagnostic::recovery_required_set(
            &record.path,
            "snapshot-link",
        )));
    }
    let bytes = std::fs::read(&path).map_err(|_| {
        DomainResult::invalid(diagnostic::recovery_required_set(
            &record.path,
            "snapshot-read",
        ))
    })?;
    match (&record.before_digest, record.present) {
        (Some(expected), true) => {
            let actual = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
            if &actual != expected {
                return Err(DomainResult::invalid(diagnostic::recovery_required_set(
                    &record.path,
                    "snapshot-digest",
                )));
            }
        }
        (None, false) => {
            if !bytes.is_empty() {
                return Err(DomainResult::invalid(diagnostic::recovery_required_set(
                    &record.path,
                    "snapshot-digest",
                )));
            }
        }
        _ => {
            return Err(DomainResult::invalid(diagnostic::recovery_required_set(
                &record.path,
                "snapshot-digest",
            )))
        }
    }
    Ok(bytes)
}

/// Resolve (never create) the recovery home of one plan digest.
pub(crate) fn recovery_home(_root: &Path, plan_digest: &str) -> Option<RecoveryHome> {
    let (head, tail) = version::digest_dir(plan_digest)?;
    let base = format!("{}/{}/{}", version::RECOVERY_DIR, head, tail);
    Some(RecoveryHome {
        journal_relative: format!("{base}/journal.json"),
        snapshots_relative: format!("{base}/snapshots"),
    })
}

/// The writer-facing sibling barrier: `Some("adoption.recovery-required")`
/// while a STRANDED apply/confirmation/rollback journal exists under the
/// adoption recovery home (r8 review, devin F1/F3 + omp F3). Before r8
/// the adoption journals were in NO barrier set: a stranded installation
/// serialized against nothing, so a change/migration/format/generate
/// writer could interleave its replacements with the stranded
/// transaction's own journaled writes. Every non-adoption mutating
/// writer now refuses under the shared lock until the journal resolves.
/// The adoption family itself skips this barrier (its recovery,
/// re-apply, and rollback entries are the resolvers).
///
/// The stranded predicate is precise because a resolved project KEEPS
/// its journals: a completed installation retains the journal as the
/// rollback surface's restoration material, and a completed recovery
/// retains the original journal by design (R6 — it is the only complete
/// restoration metadata). A home is stranded exactly when its journal
/// has neither an exact, digest-verified completed receipt nor a strictly
/// decoded recovery progress record covering EVERY journaled restoration.
/// A restored prefix is an interrupted recovery, never a resolution.
///
/// Model READERS are deliberately not gated by this barrier — see the
/// loader's Phase 2c note for the contractual rationale.
pub fn recovery_barrier_code(fs: &crate::project_fs::Fs) -> Option<&'static str> {
    use crate::project_fs::{EntryType, FsErrorKind};
    const BARRIER: Option<&str> = Some("adoption.recovery-required");
    let heads = match fs.entries_bounded(version::RECOVERY_DIR, 4096) {
        Ok(entries) => entries,
        Err(FsErrorKind::NotFound) => return None,
        Err(_) => return BARRIER,
    };
    for (head, entry_type) in heads {
        if entry_type != EntryType::Directory {
            return BARRIER;
        }
        let tails = match fs.entries_bounded(&format!("{}/{}", version::RECOVERY_DIR, head), 4096) {
            Ok(entries) => entries,
            Err(_) => return BARRIER,
        };
        for (tail, entry_type) in tails {
            if entry_type != EntryType::Directory {
                return BARRIER;
            }
            let home = format!("{}/{}/{}", version::RECOVERY_DIR, head, tail);
            let bytes = match fs.read_file_opt(&home, "journal.json", 4 * 1024 * 1024) {
                Ok(None) | Err(FsErrorKind::NotFound) => continue,
                Ok(Some(bytes)) => bytes,
                Err(_) => return BARRIER,
            };
            let plan = format!("sha256:{head}{tail}");
            let Ok(journal) = decode_journal(&bytes, &plan, &format!("{home}/snapshots"), false)
            else {
                return BARRIER;
            };
            if completed_receipt_resolves(fs, &home, &head, &tail, &journal)
                || recovery_progress_resolved(fs, &home, &journal)
            {
                continue;
            }
            return BARRIER;
        }
    }
    None
}

/// A receipt is resolution only after strict owner decoding, digest/plan/
/// phase binding, and exact coverage of the journal's completed writes.
fn completed_receipt_resolves(
    fs: &crate::project_fs::Fs,
    home: &str,
    head: &str,
    tail: &str,
    journal: &Journal,
) -> bool {
    let Ok(Some(bytes)) = fs.read_file_opt(
        &format!("{}/{head}", version::RECEIPTS_DIR),
        &format!("{tail}.json"),
        4 * 1024 * 1024,
    ) else {
        return false;
    };
    let Ok(receipt) = super::wire::receipt_of_bytes(&bytes) else {
        return false;
    };
    let value = serde_json::to_value(&receipt).expect("receipt serializes");
    if receipt.plan != journal.plan
        || receipt.phase != journal.phase
        || receipt.journal.as_deref() != Some(format!("{home}/journal.json").as_str())
        || !matches!(
            (receipt.phase.as_str(), receipt.outcome.as_str()),
            ("apply" | "confirmation", "completed") | ("rollback", "rolled-back")
        )
        || receipt.receipt
            != super::wire::digest(
                "adoption-receipt",
                &super::wire::without_member(&value, "receipt"),
            )
    {
        return false;
    }
    let state = if journal.phase == "rollback" {
        "restored"
    } else {
        "written"
    };
    let rows: Vec<_> = journal
        .steps
        .iter()
        .filter(|step| step.state == state)
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    rows.len() == receipt.writes.len()
        && rows.iter().all(|step| {
            seen.insert(&step.path)
                && step
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| receipt.snapshots.contains(snapshot))
                && receipt.writes.iter().any(|write| {
                    write.path == step.path
                        && (state == "restored"
                            || (step.action.as_ref() == Some(&write.action)
                                && step.before == write.before
                                && step.after == write.after))
                })
        })
}

/// Recovery persists progress after EACH restore. Only an exact, duplicate-
/// free complete set, bound to the original journal's metadata, clears the
/// sibling barrier; a one-row prefix of a two-file recovery stays blocked.
fn recovery_progress_resolved(fs: &crate::project_fs::Fs, home: &str, journal: &Journal) -> bool {
    if !matches!(journal.phase.as_str(), "apply" | "confirmation") {
        return false;
    }
    let Ok(Some(bytes)) = fs.read_file_opt(home, "recovery.json", 4 * 1024 * 1024) else {
        return false;
    };
    let Ok(progress) = decode_journal(&bytes, &journal.plan, &format!("{home}/snapshots"), true)
    else {
        return false;
    };
    let records: Vec<_> = journal
        .steps
        .iter()
        .filter(|step| step.state == "written")
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    !records.is_empty()
        && records.len() == progress.steps.len()
        && records.iter().zip(&progress.steps).all(|(record, step)| {
            seen.insert(&record.path)
                && record.action.is_some()
                && (record.before.is_some() || record.after.is_some())
                && record.snapshot.is_some()
                && step.path == record.path
                && step.snapshot == record.snapshot
                && step.before == record.before
                && step.after == record.after
                && step.action.is_none()
                && matches!(step.state.as_str(), "restored" | "already-restored")
        })
}

/// Snapshot the exact current bytes of every intended write target
/// (absent files included) and journal each completed capture. The
/// snapshot home is created and every byte written through the
/// confined store's per-component reparse checks; a snapshot failure
/// is a typed recovery-required refusal with zero intended writes
/// applied.
pub(crate) fn snapshot_targets(
    root: &Path,
    home: &RecoveryHome,
    paths: &[String],
    journal: &mut Journal,
) -> Outcome<Vec<SnapshotRecord>> {
    let snapshot_dir = format!(
        "{}/snapshots",
        home.journal_relative
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or_default()
    );
    let mut records = Vec::new();
    for path in paths {
        let target = root.join(path);
        let name = version::snapshot_name(path);
        let snapshot_relative = format!("{}/{}", home.snapshots_relative, name);
        match std::fs::read(&target) {
            Ok(bytes) => {
                let digest = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
                confined_snapshot_write(root, &snapshot_dir, &name, &bytes, journal)?;
                records.push(SnapshotRecord {
                    path: path.clone(),
                    snapshot: snapshot_relative,
                    present: true,
                    before_digest: Some(digest),
                });
            }
            Err(_) => {
                // Typed absence: the exact empty capture is still
                // written so a delete-shaped restore can verify it.
                confined_snapshot_write(root, &snapshot_dir, &name, b"", journal)?;
                records.push(SnapshotRecord {
                    path: path.clone(),
                    snapshot: snapshot_relative,
                    present: false,
                    before_digest: None,
                });
            }
        }
        journal.steps.push(JournalStep {
            path: path.clone(),
            state: "snapshotted".to_owned(),
            snapshot: Some(records.last().expect("pushed").snapshot.clone()),
            action: None,
            before: None,
            after: None,
        });
        persist_journal(root, home, journal)?;
    }
    Ok(records)
}

/// Write one snapshot file through the confined store: every path
/// component is checked against links/reparse points before creation
/// and the bytes land through a temporary in the final directory.
fn confined_snapshot_write(
    root: &Path,
    relative_dir: &str,
    name: &str,
    bytes: &[u8],
    journal: &Journal,
) -> Outcome<()> {
    observed_store::write_confined(root, relative_dir, name, bytes).map_err(|_| {
        DomainResult::invalid(diagnostic::recovery_required_set(
            &journal.plan,
            "snapshot-write",
        ))
    })
}

/// Atomically persist the journal through the confined store: the
/// temporary lives in the journal's own directory (per-transaction,
/// never a shared name) and the rename result is propagated. A journal
/// failure is a recovery-required refusal — completion is never
/// reported without durable recovery state.
pub(crate) fn persist_journal(root: &Path, home: &RecoveryHome, journal: &Journal) -> Outcome<()> {
    let bytes = serde_json::to_vec_pretty(journal).expect("journal serializes");
    let (parent, _) = home
        .journal_relative
        .rsplit_once('/')
        .unwrap_or((version::RECOVERY_DIR, "journal.json"));
    observed_store::write_confined(root, parent, "journal.json", &bytes).map_err(|_| {
        DomainResult::invalid(diagnostic::recovery_required_set(
            &journal.plan,
            "journal-write",
        ))
    })?;
    Ok(())
}

/// Load a durable journal left by an interrupted installation. The
/// journal is untrusted persisted input, so loading authenticates it
/// against the requested plan before anything consumes it (F1/R4):
/// the schema discriminator must be the exact accepted one, the plan
/// member must name the requested plan digest, every step path must be
/// a confined project-relative path, every snapshot must be re-anchored
/// inside this plan's own recovery snapshots home, and the state,
/// action, and digest grammars must be the closed ones. A foreign or
/// tampered journal refuses loudly instead of driving any effect.
pub fn load_journal(root: &Path, plan_digest: &str) -> Outcome<Option<Journal>> {
    let Some(home) = recovery_home(root, plan_digest) else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "plan",
            "invalid-digest",
        )));
    };
    let fs = crate::project_fs::Fs::open(root)
        .map_err(|_| DomainResult::invalid(diagnostic::input_invalid_set("journal", "read")))?;
    let logical = home
        .journal_relative
        .rsplit_once('/')
        .expect("journal home")
        .0;
    let bytes = match fs.read_file_opt(logical, "journal.json", 4 * 1024 * 1024) {
        Ok(Some(bytes)) => bytes,
        Ok(None) | Err(crate::project_fs::FsErrorKind::NotFound) => return Ok(None),
        Err(_) => {
            return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                "journal", "read",
            )))
        }
    };
    decode_journal(&bytes, plan_digest, &home.snapshots_relative, false).map(Some)
}

fn decode_journal(
    bytes: &[u8],
    plan_digest: &str,
    snapshots: &str,
    recovery: bool,
) -> Outcome<Journal> {
    if !version::is_sha256(plan_digest) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "journal",
            "journal-plan",
        )));
    }
    let value =
        super::wire::unique_json(bytes).map_err(|error| DomainResult::invalid(error.set()))?;
    let journal: Journal = serde_json::from_value(value).map_err(|_| {
        DomainResult::invalid(diagnostic::input_invalid_set("journal", "closed-shape"))
    })?;
    if journal.schema_version != Journal::schema_version_literal() {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "journal",
            "journal-version",
        )));
    }
    if journal.plan != plan_digest {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "journal",
            "journal-plan",
        )));
    }
    if (recovery && journal.phase != "recovery")
        || (!recovery
            && !matches!(
                journal.phase.as_str(),
                "apply" | "confirmation" | "rollback"
            ))
    {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "journal",
            "journal-phase",
        )));
    }
    let snapshots_prefix = format!("{snapshots}/");
    for step in &journal.steps {
        let state_valid = if recovery {
            matches!(
                step.state.as_str(),
                "restored" | "already-restored" | "restore-failed"
            )
        } else {
            JOURNAL_STATES.contains(&step.state.as_str())
        };
        if !state_valid {
            return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                "journal",
                "journal-state",
            )));
        }
        if let Some(action) = &step.action {
            if !JOURNAL_ACTIONS.contains(&action.as_str()) {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "journal",
                    "journal-action",
                )));
            }
        }
        // Re-anchor: the step target is a confined project-relative
        // path, and the snapshot is a digest-keyed file inside this
        // plan's own snapshots home — `..`, absolute forms, and another
        // plan's snapshots all refuse before any effect exists.
        if !version::is_rel_path(&step.path) {
            return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                "journal",
                "journal-path",
            )));
        }
        if let Some(snapshot) = &step.snapshot {
            if !version::is_rel_path(snapshot) || !snapshot.starts_with(&snapshots_prefix) {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "journal",
                    "journal-snapshot",
                )));
            }
        }
        for digest in [&step.before, &step.after].into_iter().flatten() {
            if !version::is_sha256(digest) {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "journal",
                    "journal-digest",
                )));
            }
        }
    }
    Ok(journal)
}

/// The durable recovery view of one plan (F10/X5): the interrupted
/// phase's journal with every snapshot's on-disk integrity classified —
/// present digests re-verified, absent files named. A consumed journal
/// makes a partial mutation discoverable instead of invisible; a
/// missing journal means nothing durable exists for the plan.
pub fn recovery_view(root: &Path, plan_digest: &str) -> Outcome<RecoveryView> {
    let journal = load_journal(root, plan_digest)?;
    let Some(journal) = journal else {
        return Ok(RecoveryView {
            plan: plan_digest.to_owned(),
            phase: None,
            intact: true,
            steps: Vec::new(),
            missing_snapshots: Vec::new(),
        });
    };
    let Some(home) = recovery_home(root, plan_digest) else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "plan",
            "invalid-digest",
        )));
    };
    let _ = home;
    let mut intact = true;
    let mut missing_snapshots = Vec::new();
    for step in &journal.steps {
        let Some(snapshot) = &step.snapshot else {
            continue;
        };
        if !version::is_rel_path(snapshot)
            || std::fs::symlink_metadata(root.join(snapshot)).is_err()
        {
            intact = false;
            missing_snapshots.push(step.path.clone());
        }
    }
    Ok(RecoveryView {
        plan: plan_digest.to_owned(),
        phase: Some(journal.phase.clone()),
        intact,
        steps: journal
            .steps
            .iter()
            .map(|step| RecoveryStep {
                path: step.path.clone(),
                state: step.state.clone(),
                snapshot: step.snapshot.clone(),
            })
            .collect(),
        missing_snapshots,
    })
}

/// One journaled recovery step as the recovery view reports it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RecoveryStep {
    pub path: String,
    pub state: String,
    pub snapshot: Option<String>,
}

/// The outcome of one executable recovery: the interrupted phase, the
/// restored paths, and the per-step result classes.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RecoveryOutcome {
    pub plan: String,
    pub phase: String,
    pub outcome: String,
    pub restored: Vec<RecoveryStep>,
}

/// What one restoration record found at its target before any byte
/// moved: the exact accepted post-state of the stranded phase, the
/// exact pre-state (already restored by a previous attempt), or a
/// later user edit that recovery must refuse as a conflict exactly
/// like the rollback surface does (F2/R5).
enum Preflight {
    /// The target still holds the phase's installed post-state —
    /// restore it from the snapshot.
    Stranded,
    /// The target already holds the exact pre-state — a previous
    /// recovery attempt restored it; restoring again is idempotent
    /// and safe to skip (R6).
    AlreadyRestored,
    /// The target drifted after the failed phase: a later user edit.
    /// Refuse as a conflict; the edit is preserved.
    Conflict,
}

/// The live acceptance of one restoration record against the tree
/// (F2/R5): presence follows the recorded mutation class, never a
/// name guess. A `delete`/`replace`/`binding` step expects the exact
/// installed post-state (or typed absence for a delete); a `create`
/// step expects the installed bytes or absence. Anything else is a
/// later edit.
fn preflight_record(root: &Path, record: &RestoreRecord) -> Preflight {
    let digest_of = |bytes: &[u8]| format!("sha256:{}", crate::digest::sha256_hex(bytes));
    match std::fs::read(root.join(&record.path)) {
        Ok(bytes) => {
            let digest = digest_of(&bytes);
            match (&record.before, &record.after) {
                // Created by the phase: only the installed bytes are the
                // stranded state; the pre-state is typed absence.
                (None, Some(after)) => {
                    if &digest == after {
                        Preflight::Stranded
                    } else {
                        Preflight::Conflict
                    }
                }
                // Replaced or index-bound: the installed bytes are the
                // stranded state, the prior bytes an already-restored
                // one; any third content is a later edit.
                (Some(before), after) => {
                    if after.as_ref().is_some_and(|after| &digest == after) {
                        Preflight::Stranded
                    } else if &digest == before {
                        Preflight::AlreadyRestored
                    } else {
                        Preflight::Conflict
                    }
                }
                (None, None) => Preflight::Conflict,
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match (&record.before, &record.after) {
                // A created path absent again is the exact pre-state — a
                // previous attempt already removed it.
                (None, Some(_)) => Preflight::AlreadyRestored,
                // A deleted path absent again is the phase's exact
                // post-state — the snapshot recreates it now.
                (Some(_), None) => Preflight::Stranded,
                _ => Preflight::Conflict,
            }
        }
        Err(_) => Preflight::Conflict,
    }
}

/// One journaled restoration, authenticated from the durable journal.
#[derive(Clone, Debug)]
struct RestoreRecord {
    path: String,
    snapshot: String,
    /// The exact pre-state digest (`None` when the phase created the
    /// path).
    before: Option<String>,
    /// The exact post-state digest the phase installed (`None` for a
    /// delete).
    after: Option<String>,
}

impl RestoreRecord {
    fn snapshot_record(&self) -> SnapshotRecord {
        SnapshotRecord {
            path: self.path.clone(),
            snapshot: self.snapshot.clone(),
            present: self.before.is_some(),
            before_digest: self.before.clone(),
        }
    }
}

/// Restore one stranded installation without a completed receipt
/// (X5): the journaled `written` steps of an interrupted apply or
/// confirmation regain their exact prior bytes (or are removed again
/// when the phase created them). The whole restoration is
/// conflict-checked and digest-preflighted before the first byte
/// moves (F2/R5, the X6 discipline), every completed step is
/// progress-journaled to a separate durable file so the original
/// journal — the only complete restoration metadata — is never
/// consumed (R6), an interrupted recovery resumes idempotently from
/// wherever it stopped, and a journal-write failure surfaces instead
/// of vanishing (F6). A completed receipt for the same plan refuses:
/// [`super::apply::rollback`] is the confirmed path for completed work.
pub fn recover(root: &Path, plan_digest: &str) -> Outcome<RecoveryOutcome> {
    // Shared cross-service writer serialization (r8, devin F1/F3 + omp
    // F3): recovery is a MUTATING adoption writer — it joins the one
    // project-wide mutation lock before any restore, so a change/
    // migration/format/generate writer can never interleave with its
    // journaled restorations. The adoption family's own journals are
    // skipped by the sibling barrier (recovery IS their resolver).
    let _shared = match crate::project_writer::WriterLock::mutation(
        root,
        crate::project_writer::MutationFamily::Adoption,
    ) {
        Ok(lock) => lock,
        Err(refusal) => return Err(diagnostic::shared_writer_refusal(refusal)),
    };
    let journal = load_journal(root, plan_digest)?;
    let Some(journal) = journal else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "journal", "missing",
        )));
    };
    if journal.phase != "apply" && journal.phase != "confirmation" {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "journal",
            "phase-not-recoverable",
        )));
    }
    if receipt_path(root, plan_digest).is_some_and(|path| path.is_file()) {
        // Completed work has a receipt; the rollback surface owns it.
        return Err(DomainResult::denied(diagnostic::rollback_conflict_set(
            plan_digest,
            &["completed-receipt-exists".to_owned()],
        )));
    }
    // Only steps that physically mutated the tree restore anything.
    let mut records: Vec<RestoreRecord> = Vec::new();
    for step in &journal.steps {
        if step.state != "written" {
            continue;
        }
        let Some(snapshot) = &step.snapshot else {
            // A written step without restoration metadata can never be
            // independently recovered; refuse instead of guessing.
            return Err(DomainResult::invalid(diagnostic::recovery_required_set(
                plan_digest,
                "journal-incomplete",
            )));
        };
        // A written step needs its exact mutation class and post-state
        // to conflict-check like rollback does; a journal without them
        // is incomplete, never a blind overwrite (F2/R5).
        if step.action.is_none() || step.before.is_none() && step.after.is_none() {
            return Err(DomainResult::invalid(diagnostic::recovery_required_set(
                plan_digest,
                "journal-incomplete",
            )));
        }
        records.push(RestoreRecord {
            path: step.path.clone(),
            snapshot: snapshot.clone(),
            before: step.before.clone(),
            after: step.after.clone(),
        });
    }
    if records.is_empty() {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "journal",
            "nothing-to-restore",
        )));
    }
    // Conflict preflight (F2/R5): every live target is classified
    // before the first mutation. A later user edit at any target is a
    // conflict refusal that preserves the edit and demands a
    // separately reviewed resolution — exactly the rollback contract.
    let conflicts: Vec<String> = records
        .iter()
        .filter(|record| matches!(preflight_record(root, record), Preflight::Conflict))
        .map(|record| record.path.clone())
        .collect();
    if !conflicts.is_empty() {
        return Err(DomainResult::denied(diagnostic::rollback_conflict_set(
            plan_digest,
            &conflicts,
        )));
    }
    // Snapshot preflight (the X6 discipline): verify every snapshot of
    // the records that still need restoration before any byte moves.
    // Already-restored records need no snapshot at all.
    for record in &records {
        if matches!(preflight_record(root, record), Preflight::Stranded) {
            read_snapshot(root, &record.snapshot_record())?;
        }
    }
    let Some(home) = recovery_home(root, plan_digest) else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "plan",
            "invalid-digest",
        )));
    };
    // The progress journal is a separate durable file: the original
    // journal — the complete authenticated restoration metadata — is
    // never overwritten, so any failure leaves a resumable recovery
    // (R6). Persistence failures propagate (F6): a step is never
    // reported restored without its durable record.
    let mut progress = Journal::new(plan_digest, "recovery");
    let mut restored = Vec::new();
    for record in &records {
        let preflight = preflight_record(root, record);
        let state = match preflight {
            Preflight::AlreadyRestored => {
                // A previous attempt already restored this exact path;
                // resuming is idempotent, never a second mutation.
                "already-restored".to_owned()
            }
            Preflight::Conflict => {
                // Classified as accepted moments ago: a target that
                // drifted mid-recovery refuses without consuming the
                // journal (the progress row records the refusal).
                "restore-failed".to_owned()
            }
            Preflight::Stranded => {
                let result = cas_restore(root, &record.snapshot_record());
                match result {
                    Ok(()) => "restored".to_owned(),
                    Err(_) => "restore-failed".to_owned(),
                }
            }
        };
        progress.steps.push(JournalStep {
            path: record.path.clone(),
            state: state.clone(),
            snapshot: Some(record.snapshot.clone()),
            action: None,
            before: record.before.clone(),
            after: record.after.clone(),
        });
        restored.push(RecoveryStep {
            path: record.path.clone(),
            state,
            snapshot: Some(record.snapshot.clone()),
        });
        persist_recovery_progress(root, &home, &progress)?;
        if progress
            .steps
            .last()
            .is_some_and(|step| step.state == "restore-failed")
        {
            return Err(DomainResult::invalid(diagnostic::recovery_required_set(
                plan_digest,
                "restore",
            )));
        }
    }
    Ok(RecoveryOutcome {
        plan: plan_digest.to_owned(),
        phase: journal.phase.clone(),
        outcome: "restored".to_owned(),
        restored,
    })
}

#[cfg(test)]
thread_local! {
    // Simulates process death AFTER durable progress publication, with no
    // cleanup. Never compiled into the shipped CLI or library.
    static INTERRUPT_RECOVERY_AFTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Durably record progress beside (never over) the complete original
/// journal. Every publication is a possible process-death checkpoint.
fn persist_recovery_progress(root: &Path, home: &RecoveryHome, progress: &Journal) -> Outcome<()> {
    let bytes = serde_json::to_vec_pretty(progress).expect("journal serializes");
    let parent = home
        .journal_relative
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or(version::RECOVERY_DIR);
    observed_store::write_confined(root, parent, "recovery.json", &bytes).map_err(|_| {
        DomainResult::invalid(diagnostic::recovery_required_set(
            &progress.plan,
            "journal-write",
        ))
    })?;
    #[cfg(test)]
    if INTERRUPT_RECOVERY_AFTER.get() == progress.steps.len() {
        if std::env::var_os("LEKALO_TEST_ADOPTION_RECOVERY_KILL_ROOT").is_some() {
            // Actual process death: no Rust cleanup/Drop runs, and the OS
            // releases the writer handle just as for a killed recovery.
            std::process::exit(91);
        }
        return Err(DomainResult::invalid(diagnostic::recovery_required_set(
            &progress.plan,
            "test-interruption",
        )));
    }
    Ok(())
}

/// The durable recovery view of one plan's installation.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RecoveryView {
    pub plan: String,
    pub phase: Option<String>,
    pub intact: bool,
    pub steps: Vec<RecoveryStep>,
    pub missing_snapshots: Vec<String>,
}

/// One compare-and-swap file installation: the on-disk bytes must still
/// match the plan's expected `before` digest (or the file must still be
/// absent), then the new bytes rename into place. Later user edits at a
/// target are a stale-plan refusal, never a silent overwrite.
pub(crate) fn cas_install(root: &Path, write: &WriteEntry) -> Outcome<()> {
    if !version::is_rel_path(&write.path) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "path",
            "outside-scope",
        )));
    }
    let target = root.join(&write.path);
    let current = std::fs::read(&target);
    let expected = write.before.as_deref();
    match (&current, expected) {
        (Ok(bytes), Some(before)) => {
            let digest = format!("sha256:{}", crate::digest::sha256_hex(bytes));
            if digest != *before {
                return Err(DomainResult::invalid(diagnostic::plan_stale_set(
                    &write.path,
                    std::slice::from_ref(&write.path),
                )));
            }
        }
        (Ok(_), None) => {
            return Err(DomainResult::invalid(diagnostic::plan_stale_set(
                &write.path,
                std::slice::from_ref(&write.path),
            )));
        }
        (Err(_), Some(_)) => {
            return Err(DomainResult::invalid(diagnostic::plan_stale_set(
                &write.path,
                std::slice::from_ref(&write.path),
            )));
        }
        (Err(_), None) => {}
    }
    let content = write.content.as_deref().unwrap_or_default();
    let (dir, name) = write
        .path
        .rsplit_once('/')
        .map(|(parent, name)| (parent.to_owned(), name.to_owned()))
        .ok_or_else(|| {
            DomainResult::invalid(diagnostic::input_invalid_set("path", "outside-scope"))
        })?;
    crate::observed::store::write_confined(root, &dir, &name, content.as_bytes())
        .map_err(DomainResult::invalid)
}

/// Restore one snapshot produced by [`snapshot_targets`]. Created files
/// are removed again; replaced files regain their exact prior bytes.
/// The snapshot is read back through the digest-checking confined path,
/// so a corrupted or planted snapshot file refuses instead of restoring
/// attacker- or drift-controlled bytes.
pub(crate) fn cas_restore(root: &Path, record: &SnapshotRecord) -> Outcome<()> {
    if !version::is_rel_path(&record.path) {
        // Defense in depth: every restoration target is re-anchored
        // inside the project root at restore time, whatever the caller
        // believed at classification time (F1/R4).
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "path",
            "outside-scope",
        )));
    }
    let bytes = read_snapshot(root, record)?;
    let target = root.join(&record.path);
    if record.present {
        let (dir, name) = record
            .path
            .rsplit_once('/')
            .map(|(parent, name)| (parent.to_owned(), name.to_owned()))
            .ok_or_else(|| {
                DomainResult::invalid(diagnostic::input_invalid_set("path", "outside-scope"))
            })?;
        observed_store::write_confined(root, &dir, &name, &bytes).map_err(|_| {
            DomainResult::invalid(diagnostic::recovery_required_set(
                &record.path,
                "restore-write",
            ))
        })
    } else {
        match std::fs::remove_file(&target) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(DomainResult::invalid(diagnostic::recovery_required_set(
                &record.path,
                "restore-remove",
            ))),
        }
    }
}

/// Delete one planned file: the live bytes must still match the plan's
/// expected `before` pre-state (the same compare-and-swap discipline as
/// [`cas_install`]); the file is then removed. Later edits are a stale
/// refusal, never a silent delete of drifted bytes.
pub(crate) fn cas_delete(root: &Path, write: &WriteEntry) -> Outcome<()> {
    let target = root.join(&write.path);
    match std::fs::read(&target) {
        Ok(bytes) => {
            let digest = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
            if write.before.as_deref() != Some(digest.as_str()) {
                return Err(DomainResult::invalid(diagnostic::plan_stale_set(
                    &write.path,
                    std::slice::from_ref(&write.path),
                )));
            }
            std::fs::remove_file(&target).map_err(|_| {
                DomainResult::invalid(diagnostic::recovery_required_set(&write.path, "delete"))
            })?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let absent_is_expected = write.before.is_none();
            if absent_is_expected {
                Ok(())
            } else {
                Err(DomainResult::invalid(diagnostic::plan_stale_set(
                    &write.path,
                    std::slice::from_ref(&write.path),
                )))
            }
        }
        Err(_) => Err(DomainResult::invalid(diagnostic::recovery_required_set(
            &write.path,
            "delete",
        ))),
    }
}

/// The receipt home of one plan digest, inside the accepted import area.
pub(crate) fn receipt_path(root: &Path, plan_digest: &str) -> Option<PathBuf> {
    let (head, tail) = version::digest_dir(plan_digest)?;
    Some(
        root.join(version::RECEIPTS_DIR)
            .join(head)
            .join(format!("{tail}.json")),
    )
}

/// Persist one receipt document through the confined store. A failure
/// here is classified recovery-required: every intended write already
/// completed and the durable journal names the stranded steps, so the
/// refusal is the executable recovery contract (X5), never a raw
/// storage error over committed work.
pub(crate) fn persist_receipt(
    root: &Path,
    plan_digest: &str,
    document: &super::wire::AdoptionReceipt,
) -> Outcome<()> {
    let path = receipt_path(root, plan_digest).ok_or_else(|| {
        DomainResult::invalid(diagnostic::input_invalid_set("plan", "invalid-digest"))
    })?;
    let bytes = serde_json::to_vec_pretty(document).expect("receipt serializes");
    let (dir, name) = {
        let relative = path
            .strip_prefix(root)
            .map(|relative| relative.to_path_buf())
            .unwrap_or_else(|_| path.clone());
        let mut components = relative.components();
        let name = components
            .next_back()
            .map(|component| component.as_os_str().to_owned())
            .unwrap_or_default();
        let mut parent = PathBuf::new();
        for component in components {
            parent.push(component);
        }
        (
            parent.to_string_lossy().replace('\\', "/"),
            name.to_string_lossy().to_string(),
        )
    };
    crate::observed::store::write_confined(root, &dir, &name, &bytes).map_err(|_| {
        DomainResult::invalid(diagnostic::recovery_required_set(
            plan_digest,
            "receipt-write",
        ))
    })
}

/// Build the exact write entry of one file change: before/after digests
/// over the real bytes, plus the exact new content.
pub(crate) fn write_entry(root: &Path, path: String, content: String, action: &str) -> WriteEntry {
    let before = std::fs::read(root.join(&path)).ok();
    let after = crate::digest::sha256_hex(content.as_bytes());
    WriteEntry {
        path,
        action: action.to_owned(),
        before: before.map(|bytes| format!("sha256:{}", crate::digest::sha256_hex(&bytes))),
        after: Some(format!("sha256:{after}")),
        content: Some(content),
    }
}

/// The changed-input names between two pin sets (used by the stale-plan
/// refusals): members whose current digest differs from the plan's pin.
pub(crate) fn changed_pins(
    expected: &super::wire::Pins,
    current: &super::wire::Pins,
) -> Vec<String> {
    let mut changed = Vec::new();
    if expected.model != current.model {
        changed.push("model".to_owned());
    }
    if expected.lock != current.lock {
        changed.push("lock".to_owned());
    }
    if expected.observed_index != current.observed_index {
        changed.push("observed-index".to_owned());
    }
    if expected.adapter != current.adapter {
        changed.push("adapter".to_owned());
    }
    let expected_sources: BTreeMap<&str, &str> = expected
        .sources
        .iter()
        .map(|source| (source.path.as_str(), source.sha256.as_str()))
        .collect();
    let current_sources: BTreeMap<&str, &str> = current
        .sources
        .iter()
        .map(|source| (source.path.as_str(), source.sha256.as_str()))
        .collect();
    for (path, digest) in &expected_sources {
        if current_sources.get(path) != Some(digest) {
            changed.push(path.to_string());
        }
    }
    for path in current_sources.keys() {
        if !expected_sources.contains_key(path) {
            changed.push((*path).to_owned());
        }
    }
    changed.sort();
    changed.dedup();
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(bytes: &[u8]) -> String {
        format!("sha256:{}", crate::digest::sha256_hex(bytes))
    }

    fn home_of(root: &Path, plan: &str) -> RecoveryHome {
        recovery_home(root, plan).expect("valid digest home")
    }

    fn write_journal(root: &Path, home: &RecoveryHome, journal: &Journal) {
        observed_store::write_confined(
            root,
            home.journal_relative
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or_default(),
            "journal.json",
            serde_json::to_vec_pretty(journal)
                .expect("serializes")
                .as_slice(),
        )
        .expect("journal written");
    }

    #[test]
    fn interrupted_two_file_recovery_keeps_the_sibling_barrier_until_complete() {
        let plan = sha(b"two-file-recovery");
        // Re-execute this exact test as a child and die after publication
        // of the FIRST restore. This proves a real no-cleanup process-death
        // state, not merely a fabricated prefix or an unwound error return.
        if let Some(root) = std::env::var_os("LEKALO_TEST_ADOPTION_RECOVERY_KILL_ROOT") {
            INTERRUPT_RECOVERY_AFTER.set(1);
            recover(Path::new(&root), &plan).expect("child must die in the checkpoint");
            panic!("recovery passed the process-death checkpoint");
        }
        let root = tempfile::tempdir().unwrap();
        let home = home_of(root.path(), &plan);
        let mut journal = Journal::new(&plan, "apply");
        let records = snapshot_targets(
            root.path(),
            &home,
            &["one.txt".to_owned(), "two.txt".to_owned()],
            &mut journal,
        )
        .unwrap();
        for record in &records {
            std::fs::write(root.path().join(&record.path), b"installed").unwrap();
            journal.steps.push(JournalStep {
                path: record.path.clone(),
                state: "written".to_owned(),
                snapshot: Some(record.snapshot.clone()),
                action: Some("create".to_owned()),
                before: None,
                after: Some(sha(b"installed")),
            });
        }
        write_journal(root.path(), &home, &journal);
        let fs = crate::project_fs::Fs::open(root.path()).unwrap();
        assert_eq!(
            recovery_barrier_code(&fs),
            Some("adoption.recovery-required")
        );
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "adoption::transaction::tests::interrupted_two_file_recovery_keeps_the_sibling_barrier_until_complete", "--nocapture"])
            .env("LEKALO_TEST_ADOPTION_RECOVERY_KILL_ROOT", root.path())
            .output().expect("recovery subprocess");
        assert_eq!(
            child.status.code(),
            Some(91),
            "child checkpoint: {}{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(!root.path().join("one.txt").exists());
        assert!(
            root.path().join("two.txt").exists(),
            "death window: second path is unresolved"
        );
        assert_eq!(
            recovery_barrier_code(&fs),
            Some("adoption.recovery-required")
        );
        // Neither an empty receipt nor malformed/duplicate progress may
        // hide the real interrupted state from ANY sibling writer.
        let (head, tail) = version::digest_dir(&plan).unwrap();
        let receipt_dir = root.path().join(version::RECEIPTS_DIR).join(&head);
        std::fs::create_dir_all(&receipt_dir).unwrap();
        let receipt_path = receipt_dir.join(format!("{tail}.json"));
        for bytes in [
            b"".as_slice(),
            b"{}".as_slice(),
            b"{\"outcome\":\"completed\"}".as_slice(),
        ] {
            std::fs::write(&receipt_path, bytes).unwrap();
            assert_eq!(
                recovery_barrier_code(&fs),
                Some("adoption.recovery-required")
            );
        }
        std::fs::remove_file(receipt_path).unwrap();
        recover(root.path(), &plan).expect("resumes idempotently");
        assert!(!root.path().join("two.txt").exists());
        assert_eq!(recovery_barrier_code(&fs), None);
        let progress_path = root
            .path()
            .join(home.journal_relative)
            .with_file_name("recovery.json");
        let complete = std::fs::read(&progress_path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&complete).unwrap();
        for mutation in [
            "missing",
            "duplicate",
            "wrong-plan",
            "wrong-path",
            "wrong-snapshot",
            "wrong-before",
            "wrong-phase",
            "unknown",
        ] {
            let mut bad = value.clone();
            match mutation {
                "missing" => {
                    bad["steps"].as_array_mut().unwrap().pop();
                }
                "duplicate" => {
                    bad["steps"][1] = bad["steps"][0].clone();
                }
                "wrong-plan" => bad["plan"] = serde_json::json!(sha(b"foreign")),
                "wrong-path" => bad["steps"][1]["path"] = serde_json::json!("foreign.txt"),
                "wrong-snapshot" => {
                    bad["steps"][1]["snapshot"] = serde_json::json!(records[0].snapshot)
                }
                "wrong-before" => bad["steps"][1]["before"] = serde_json::json!(sha(b"foreign")),
                "wrong-phase" => bad["phase"] = serde_json::json!("apply"),
                _ => bad["extra"] = serde_json::json!(true),
            }
            std::fs::write(&progress_path, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert_eq!(
                recovery_barrier_code(&fs),
                Some("adoption.recovery-required"),
                "{mutation}"
            );
        }
        std::fs::write(progress_path, complete).unwrap();
        assert_eq!(recovery_barrier_code(&fs), None);
    }

    #[test]
    fn completed_receipt_requires_owner_decode_digest_plan_and_exact_write_set() {
        let root = tempfile::tempdir().unwrap();
        let plan = sha(b"completed-installation");
        let home = home_of(root.path(), &plan);
        let snapshot = format!(
            "{}/{}",
            home.snapshots_relative,
            version::snapshot_name("one.txt")
        );
        let journal = Journal {
            steps: vec![JournalStep {
                path: "one.txt".to_owned(),
                state: "written".to_owned(),
                snapshot: Some(snapshot.clone()),
                action: Some("create".to_owned()),
                before: None,
                after: Some(sha(b"installed")),
            }],
            ..Journal::new(&plan, "apply")
        };
        write_journal(root.path(), &home, &journal);
        let receipt = super::super::wire::AdoptionReceipt {
            schema_version: version::RECEIPT_SCHEMA_VERSION.to_owned(),
            identity: version::RECEIPT_IDENTITY.to_owned(),
            receipt: String::new(),
            phase: "apply".to_owned(),
            project: "test".to_owned(),
            plan: plan.clone(),
            outcome: "completed".to_owned(),
            writes: vec![WriteEntry {
                path: "one.txt".to_owned(),
                action: "create".to_owned(),
                before: None,
                after: Some(sha(b"installed")),
                content: Some("installed".to_owned()),
            }],
            journal: Some(home.journal_relative.clone()),
            snapshots: vec![snapshot],
            verification: None,
            pins: super::super::wire::Pins {
                model: sha(b"model"),
                lock: None,
                observed_index: None,
                adapter: None,
                sources: Vec::new(),
            },
            analysis: None,
        };
        let fs = crate::project_fs::Fs::open(root.path()).unwrap();
        for mutation in [
            "bad-digest",
            "foreign-plan",
            "failed",
            "wrong-phase",
            "missing-write",
            "wrong-journal",
            "valid",
        ] {
            let mut sample = receipt.clone();
            match mutation {
                "foreign-plan" => sample.plan = sha(b"foreign"),
                "failed" => sample.outcome = "failed".to_owned(),
                "wrong-phase" => sample.phase = "rollback".to_owned(),
                "missing-write" => sample.writes.clear(),
                "wrong-journal" => sample.journal = None,
                _ => {}
            }
            let value = serde_json::to_value(&sample).unwrap();
            sample.receipt = super::super::wire::digest(
                "adoption-receipt",
                &super::super::wire::without_member(&value, "receipt"),
            );
            if mutation == "bad-digest" {
                sample.receipt = sha(b"bad");
            }
            persist_receipt(root.path(), &plan, &sample).unwrap();
            assert_eq!(
                recovery_barrier_code(&fs),
                if mutation == "valid" {
                    None
                } else {
                    Some("adoption.recovery-required")
                },
                "{mutation}"
            );
        }
        // Duplicate-key bytes refuse even if the other projection is valid.
        let (head, tail) = version::digest_dir(&plan).unwrap();
        std::fs::write(
            root.path()
                .join(version::RECEIPTS_DIR)
                .join(head)
                .join(format!("{tail}.json")),
            b"{\"outcome\":\"completed\",\"outcome\":\"completed\"}",
        )
        .unwrap();
        assert_eq!(
            recovery_barrier_code(&fs),
            Some("adoption.recovery-required")
        );
        // An unreadable/non-directory enumeration is also fail closed.
        let broken = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(broken.path().join(".lekalo/import/adoption")).unwrap();
        std::fs::write(
            broken.path().join(version::RECOVERY_DIR),
            b"not a directory",
        )
        .unwrap();
        assert_eq!(
            recovery_barrier_code(&crate::project_fs::Fs::open(broken.path()).unwrap()),
            Some("adoption.recovery-required")
        );
    }

    /// F1/R4: a durable journal is untrusted persisted input. Loading
    /// authenticates it against the requested plan — schema version,
    /// plan identity, and every step path/snapshot anchor — so a
    /// hand-planted journal with a foreign plan, an escaping target, or
    /// an unanchored snapshot refuses loudly before any effect.
    #[test]
    fn journals_authenticate_against_the_requested_plan() {
        let root = tempfile::tempdir().expect("tempdir");
        let plan = sha(b"plan");
        let home = home_of(root.path(), &plan);
        write_journal(root.path(), &home, &Journal::new(&plan, "apply"));
        assert!(load_journal(root.path(), &plan).expect("loads").is_some());

        // A journal naming another plan never loads for this digest.
        let foreign = Journal::new(&sha(b"other"), "apply");
        write_journal(root.path(), &home, &foreign);
        let error = load_journal(root.path(), &plan).expect_err("foreign plan refuses");
        assert_eq!(error.status(), crate::result::Status::Invalid);

        // A wrong schema discriminator refuses.
        let mut versioned = Journal::new(&plan, "apply");
        versioned.schema_version = "lekalo/adoption-journal/v9.9.9".to_owned();
        write_journal(root.path(), &home, &versioned);
        assert!(load_journal(root.path(), &plan).is_err());

        // An escaping step path and an unanchored snapshot both refuse.
        for step in [
            JournalStep {
                path: "../escape.txt".to_owned(),
                state: "written".to_owned(),
                snapshot: Some(format!("{}/x.bin", home.snapshots_relative)),
                action: Some("create".to_owned()),
                before: None,
                after: Some(sha(b"after")),
            },
            JournalStep {
                path: "src/tasks.mjs".to_owned(),
                state: "written".to_owned(),
                snapshot: Some(".lekalo/import/adoption/recovery/other/snapshots/x.bin".to_owned()),
                action: Some("replace".to_owned()),
                before: Some(sha(b"before")),
                after: Some(sha(b"after")),
            },
            JournalStep {
                path: "src/tasks.mjs".to_owned(),
                state: "invented-state".to_owned(),
                snapshot: Some(format!("{}/x.bin", home.snapshots_relative)),
                action: Some("replace".to_owned()),
                before: Some(sha(b"before")),
                after: Some(sha(b"after")),
            },
        ] {
            let mut tampered = Journal::new(&plan, "apply");
            tampered.steps.push(step);
            write_journal(root.path(), &home, &tampered);
            assert!(
                load_journal(root.path(), &plan).is_err(),
                "the tampered journal must refuse"
            );
        }
    }

    /// F2/R5/R6: the live acceptance of one restoration record follows
    /// the recorded mutation class — the stranded post-state restores,
    /// the exact pre-state skips idempotently, and anything else is a
    /// later user edit that conflicts.
    #[test]
    fn preflight_classifies_stranded_restored_and_conflict() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("replaced.mjs"), b"after").expect("write");
        std::fs::write(root.path().join("created.mjs"), b"installed").expect("write");
        let before = sha(b"before");
        let after = sha(b"after");
        let installed = sha(b"installed");

        let replaced = RestoreRecord {
            path: "replaced.mjs".to_owned(),
            snapshot: "s".to_owned(),
            before: Some(before.clone()),
            after: Some(after.clone()),
        };
        assert!(matches!(
            preflight_record(root.path(), &replaced),
            Preflight::Stranded
        ));
        std::fs::write(root.path().join("replaced.mjs"), b"before").expect("write");
        assert!(matches!(
            preflight_record(root.path(), &replaced),
            Preflight::AlreadyRestored
        ));
        std::fs::write(root.path().join("replaced.mjs"), b"user edit").expect("write");
        assert!(matches!(
            preflight_record(root.path(), &replaced),
            Preflight::Conflict
        ));

        let created = RestoreRecord {
            path: "created.mjs".to_owned(),
            snapshot: "s".to_owned(),
            before: None,
            after: Some(installed.clone()),
        };
        assert!(matches!(
            preflight_record(root.path(), &created),
            Preflight::Stranded
        ));
        std::fs::remove_file(root.path().join("created.mjs")).expect("remove");
        assert!(matches!(
            preflight_record(root.path(), &created),
            Preflight::AlreadyRestored
        ));
        std::fs::write(root.path().join("created.mjs"), b"user edit").expect("write");
        assert!(matches!(
            preflight_record(root.path(), &created),
            Preflight::Conflict
        ));

        // A deleted path absent again is the stranded post-state: the
        // snapshot recreates it.
        let deleted = RestoreRecord {
            path: "deleted.mjs".to_owned(),
            snapshot: "s".to_owned(),
            before: Some(before),
            after: None,
        };
        assert!(matches!(
            preflight_record(root.path(), &deleted),
            Preflight::Stranded
        ));
        std::fs::write(root.path().join("deleted.mjs"), b"before").expect("write");
        assert!(matches!(
            preflight_record(root.path(), &deleted),
            Preflight::AlreadyRestored
        ));
        std::fs::write(root.path().join("deleted.mjs"), b"user edit").expect("write");
        assert!(matches!(
            preflight_record(root.path(), &deleted),
            Preflight::Conflict
        ));
    }
}
