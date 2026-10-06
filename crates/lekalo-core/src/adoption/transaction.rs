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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::diagnostic;
use super::version;
use super::wire::WriteEntry;
use super::Outcome;
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
/// target, or the typed absence of the file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SnapshotRecord {
    pub path: String,
    pub snapshot: String,
    pub present: bool,
}

/// The confined recovery home of one plan digest.
pub(crate) struct RecoveryHome {
    pub root: PathBuf,
    pub journal_relative: String,
    pub snapshots_relative: String,
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
/// (absent files included) and journal each completed capture.
pub(crate) fn snapshot_targets(
    root: &Path,
    home: &RecoveryHome,
    paths: &[String],
    journal: &mut Journal,
) -> Outcome<Vec<SnapshotRecord>> {
    std::fs::create_dir_all(home.root.join("snapshots")).map_err(|_| {
        DomainResult::invalid(diagnostic::recovery_required_set(
            &journal.plan,
            "snapshot-create",
        ))
    })?;
    let mut records = Vec::new();
    for path in paths {
        let target = root.join(path);
        let bytes = std::fs::read(&target);
        let name = version::snapshot_name(path);
        let snapshot_relative = format!("{}/{}", home.snapshots_relative, name);
        match bytes {
            Ok(bytes) => {
                std::fs::write(home.root.join("snapshots").join(&name), &bytes).map_err(|_| {
                    DomainResult::invalid(diagnostic::recovery_required_set(
                        &journal.plan,
                        "snapshot-write",
                    ))
                })?;
                records.push(SnapshotRecord {
                    path: path.clone(),
                    snapshot: snapshot_relative,
                    present: true,
                });
            }
            Err(_) => {
                std::fs::write(home.root.join("snapshots").join(&name), b"").map_err(|_| {
                    DomainResult::invalid(diagnostic::recovery_required_set(
                        &journal.plan,
                        "snapshot-write",
                    ))
                })?;
                records.push(SnapshotRecord {
                    path: path.clone(),
                    snapshot: snapshot_relative,
                    present: false,
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

/// Atomically persist the journal through the confined store.
pub(crate) fn persist_journal(root: &Path, home: &RecoveryHome, journal: &Journal) -> Outcome<()> {
    let bytes = serde_json::to_vec_pretty(journal).expect("journal serializes");
    crate::observed::store::write_confined(root, version::RECOVERY_DIR, "journal.tmp", &bytes)
        .map_err(DomainResult::invalid)?;
    let _ = std::fs::rename(
        root.join(version::RECOVERY_DIR).join("journal.tmp"),
        home.root.join("journal.json"),
    );
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
pub(crate) fn cas_restore(root: &Path, record: &SnapshotRecord) -> Outcome<()> {
    let bytes = std::fs::read(root.join(&record.snapshot)).map_err(|_| {
        DomainResult::invalid(diagnostic::recovery_required_set(
            &record.path,
            "snapshot-read",
        ))
    })?;
    let target = root.join(&record.path);
    if record.present {
        let (dir, name) = record
            .path
            .rsplit_once('/')
            .map(|(parent, name)| (parent.to_owned(), name.to_owned()))
            .ok_or_else(|| {
                DomainResult::invalid(diagnostic::input_invalid_set("path", "outside-scope"))
            })?;
        crate::observed::store::write_confined(root, &dir, &name, &bytes)
            .map_err(DomainResult::invalid)
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

/// Delete a file this very plan created, after checking it still holds
/// the plan's `after` bytes.
pub(crate) fn cas_delete(root: &Path, write: &WriteEntry) -> Outcome<()> {
    let target = root.join(&write.path);
    match std::fs::read(&target) {
        Ok(bytes) => {
            let digest = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
            if write.after.as_deref() != Some(digest.as_str()) {
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
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
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
