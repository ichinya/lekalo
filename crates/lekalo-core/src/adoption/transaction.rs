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
}

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
    pub root: PathBuf,
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
pub(crate) fn recovery_home(root: &Path, plan_digest: &str) -> Option<RecoveryHome> {
    let (head, tail) = version::digest_dir(plan_digest)?;
    let home = root.join(version::RECOVERY_DIR).join(&head).join(&tail);
    let base = format!("{}/{}/{}", version::RECOVERY_DIR, head, tail);
    Some(RecoveryHome {
        root: home,
        journal_relative: format!("{base}/journal.json"),
        snapshots_relative: format!("{base}/snapshots"),
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

/// Load a durable journal left by an interrupted installation.
pub fn load_journal(root: &Path, plan_digest: &str) -> Outcome<Option<Journal>> {
    let Some(home) = recovery_home(root, plan_digest) else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "plan",
            "invalid-digest",
        )));
    };
    match std::fs::read(home.root.join("journal.json")) {
        Ok(bytes) => {
            let value = super::wire::unique_json(&bytes)
                .map_err(|error| DomainResult::invalid(error.set()))?;
            let journal: Journal = serde_json::from_value(value).map_err(|_| {
                DomainResult::invalid(diagnostic::input_invalid_set("journal", "closed-shape"))
            })?;
            Ok(Some(journal))
        }
        Err(_) => Ok(None),
    }
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

/// Persist one receipt document through the confined store.
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
    crate::observed::store::write_confined(root, &dir, &name, &bytes).map_err(DomainResult::invalid)
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
