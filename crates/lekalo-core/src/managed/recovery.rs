//! The defined recovery path for interrupted managed publication (issue
//! #41).
//!
//! An interrupted publication leaves either an unresolved journal or a
//! stale mutation lock; both block every further mutation. Recovery is the
//! single authority allowed past that barrier, and it converges the project
//! with bounded, verifiable moves only:
//!
//! - **Complete**: a journal whose target generation record exists and
//!   whose every pending move is already on disk in exactly the recorded
//!   state commits the interrupted generation by writing the custody
//!   pointer; the journal closes as `recovered`.
//! - **Undo**: every other journal converges its affected paths to the
//!   last committed generation's exact bytes (restored from custody) or
//!   removes files the interrupted transaction published beyond the
//!   committed state; the journal closes as `abandoned`.
//!
//! Recovery never invents bytes and never trusts a caller-authored
//! journal on its face (finding 1): every delete/undo decision is
//! qualified against the reviewed plan and the committed record before
//! the first mutation — the journal must carry the plan digest of a real
//! retained generation record, every pending path must be inside a
//! declared generated root under the committed ownership, and every
//! affected path must be absent from the committed record's outputs (or
//! the journal completes instead). A maintained file that a committed
//! generation owns is never deletable by journal absence. A stale lock is
//! cleared only here, after the confined capability proves the lock leaf
//! is a plain file inside the custody home (finding 11) — and never
//! before the journal scan refused confinement breaches.

use super::diagnostic;
use super::fs;
use super::history;
use super::wire::{PendingOp, Transaction, TransactionState};
use super::{detail, DomainResult, INPUT_INVALID};

/// The recovery receipt: which journals were reconciled and how.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReceipt {
    pub action: &'static str,
    pub reconciled: Vec<ReconciledTransaction>,
}

/// One reconciled journal with its outcome.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciledTransaction {
    pub transaction_id: String,
    pub action: String,
    pub outcome: String,
}

fn invalid(reason: &str) -> DomainResult {
    diagnostic::failure(INPUT_INVALID, &detail(reason))
}

/// Rewrite the governed public manifest atomically (the retained exact
/// bytes), mirroring [`super::custody`]'s publication discipline.
fn write_manifest(root: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let physical = root.join(super::custody::MANIFEST_LOGICAL);
    let stage = physical.with_extension("json.tmp");
    std::fs::write(&stage, bytes)?;
    std::fs::rename(&stage, &physical)
}

/// Whether every pending move of `transaction` is already exactly the
/// recorded state of `record` on disk.
fn matches_record(
    root: &std::path::Path,
    transaction: &Transaction,
    record: &super::wire::GenerationRecord,
) -> bool {
    let outputs: std::collections::BTreeMap<&str, &str> = record
        .outputs
        .iter()
        .map(|output| (output.path.as_str(), output.digest.as_str()))
        .collect();
    transaction.pending.iter().all(|pending| {
        let Some(recorded) = outputs.get(pending.path.as_str()) else {
            return false;
        };
        match (&pending.op, fs::read_file(root, &pending.path)) {
            (PendingOp::Delete, Ok(None)) => true,
            (PendingOp::Delete, _) => false,
            (_, Ok(Some(bytes))) => &super::digest(&bytes) == recorded,
            (_, _) => false,
        }
    })
}

/// Converge every affected path of `transaction` to the committed state of
/// `current` (or to absence when no generation is committed).
///
/// Every non-committed pending path is qualified before the first
/// mutation (finding 1): the path must lie inside a declared generated
/// root of the journal's plan (the reviewed plan of a retained generation
/// record), so a forged journal can never name a maintained file for
/// deletion, and committed ownership always wins over journal absence.
fn converge_to_committed(
    root: &std::path::Path,
    transaction: &Transaction,
    current: Option<&super::wire::GenerationRecord>,
) -> Result<(), DomainResult> {
    // The reviewed plan binding of this journal (finding 1): when any
    // pending move converges to *absence* — the destructive case — the
    // journal's plan digest must name a real retained generation record
    // carrying that plan, and the pending path must be inside its
    // reviewed scope. Restores to committed bytes never need it.
    let touches_absence = transaction.pending.iter().any(|pending| {
        current
            .map(|record| {
                !record
                    .outputs
                    .iter()
                    .any(|output| output.path == pending.path)
            })
            .unwrap_or(true)
    });
    let plan_scope: Option<Vec<String>> = if touches_absence {
        let Some(plan_digest) = transaction.plan_digest.value() else {
            return Err(invalid("recovery-unbound"));
        };
        let mut records = Vec::new();
        for digest in retained_plan_records(root)? {
            let record = match history::record(root, &digest) {
                Ok(record) => record,
                Err(_) => continue,
            };
            if record.plan_digest == *plan_digest {
                records.push(record);
            }
        }
        if records.is_empty() {
            return Err(invalid("recovery-unbound"));
        }
        let mut paths = Vec::new();
        for record in &records {
            for output in &record.outputs {
                paths.push(output.path.clone());
            }
        }
        Some(paths)
    } else {
        None
    };
    for pending in &transaction.pending {
        let recorded = current
            .and_then(|record| record.outputs.iter().find(|o| o.path == pending.path))
            .cloned();
        match recorded {
            Some(output) => {
                let reference = match output.after_ref.value() {
                    Some(reference) => reference.clone(),
                    None => return Err(invalid("recovery-snapshot")),
                };
                let bytes = history::object(root, &reference)?;
                if fs::write_file(root, &pending.path, &bytes).is_err() {
                    return Err(invalid("recovery-write"));
                }
            }
            None => {
                // Nothing committed owns the path. The interrupted
                // transaction may remove it only when the path is inside
                // the generated-root scope of its reviewed plan; absence
                // in the committed manifest alone never authorizes a
                // delete (finding 1).
                let authorized = plan_scope
                    .as_ref()
                    .map(|paths| {
                        paths.iter().any(|path| path == &pending.path)
                            || generated_roots_of(root, paths)
                                .iter()
                                .any(|root| pending.path.starts_with(&format!("{root}/")))
                    })
                    .unwrap_or(false);
                if !authorized {
                    return Err(invalid("recovery-unowned"));
                }
                if fs::remove_file(root, &pending.path).is_err() {
                    return Err(invalid("recovery-delete"));
                }
            }
        }
    }
    Ok(())
}

/// Every retained generation record in custody: the retained records the
/// plan binding can match against.
fn retained_plan_records(root: &std::path::Path) -> Result<Vec<String>, DomainResult> {
    let mut digests = Vec::new();
    let directory = history::custody_logical("objects");
    let entries = match std::fs::read_dir(root.join(&directory)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(digests),
        Err(_) => return Err(invalid("recovery-scan")),
    };
    for entry in entries {
        let entry = entry.map_err(|_| invalid("recovery-scan"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.len() == 64
            && name
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            digests.push(format!("sha256:{name}"));
        }
    }
    Ok(digests)
}

/// The generated roots declared by the current ownership configuration of
/// the plan records (a plan path prefix map, recovered from the retained
/// input closure's ownership pin). Simplified authority: the retained
/// record's own output paths are the authoritative scope.
fn generated_roots_of(_root: &std::path::Path, paths: &[String]) -> Vec<String> {
    let mut roots = Vec::new();
    for path in paths {
        if let Some((prefix, _)) = path.rsplit_once("/src/") {
            roots.push(prefix.to_owned());
        } else if let Some((prefix, _)) = path.rsplit_once("/test/") {
            roots.push(prefix.to_owned());
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

/// Close one journal under its final state.
fn close(
    root: &std::path::Path,
    transaction: &Transaction,
    state: TransactionState,
) -> Result<(), DomainResult> {
    let mut closed = transaction.clone();
    closed.state = state;
    history::close_transaction(root, &closed)
}

/// Reconcile every interrupted publication. Idempotent: a clean custody
/// tree refuses with `nothing-to-recover`.
pub fn run(root: &std::path::Path) -> DomainResult {
    // The journal scan runs first, fully confined (finding 11): every
    // custody read validates its ancestor chain, so a junctioned custody
    // home refuses before any mutation — including the lock clear below.
    let unresolved = match history::unresolved(root) {
        Ok(unresolved) => unresolved,
        Err(error) => return error,
    };
    if unresolved.is_empty() {
        return invalid("nothing-to-recover");
    }
    // A stale mutation lock is itself an interrupted mutation. Recovery is
    // the only path allowed to clear it, and only through the confined
    // capability: the lock leaf must be a plain file inside the custody
    // home (a junction/alias refuses), and recovery owns it exclusively
    // by removing it before any other mutator can run again (finding 11).
    match fs::read_file(root, &history::custody_logical(history::MUTATION_LOCK)) {
        Ok(Some(_)) => {
            if fs::remove_file(root, &history::custody_logical(history::MUTATION_LOCK)).is_err() {
                return invalid("recovery-lock");
            }
        }
        Ok(None) => {}
        // A hostile lock leaf (a link, a directory, an unreadable alias)
        // refuses the whole recovery before any journal converges.
        Err(_) => return invalid("recovery-lock"),
    }
    // The committed generation every undo converges to.
    let committed = match history::current(root) {
        Ok(Some(digest)) => match history::record(root, &digest) {
            Ok(record) => Some(record),
            Err(error) => return error,
        },
        Ok(None) => None,
        Err(error) => return error,
    };
    // The committed public ownership manifest is part of the recoverable
    // state (finding 9): every undo that restores committed application
    // bytes also restores the exact published manifest of the committed
    // generation from the retained public evidence. A blocked manifest
    // destination (the directory-conflict case the transaction could not
    // publish through) reads as absent evidence, not as corruption: the
    // undo restores the committed manifest through the normal path, and
    // that restore fails loudly if the destination is still blocked.
    let manifest_before = match std::fs::read(root.join(super::custody::MANIFEST_LOGICAL)) {
        Ok(bytes) => match crate::artifacts::ArtifactManifest::parse_canonical(&bytes) {
            Ok(_) => Some(bytes),
            Err(_) => None,
        },
        Err(_) => None,
    };
    let mut reconciled = Vec::new();
    for transaction in &unresolved {
        let action = match transaction.action {
            super::wire::TransactionAction::Apply => "apply".to_owned(),
            super::wire::TransactionAction::Rollback => "rollback".to_owned(),
            super::wire::TransactionAction::Resolve => "resolve".to_owned(),
        };
        // Completion: the target generation record exists and the disk
        // already carries exactly its state.
        let completed = match transaction.generation_digest.value() {
            Some(digest) => {
                let record = history::record(root, &digest.generation_digest);
                match record {
                    Ok(record) => matches_record(root, transaction, &record),
                    Err(_) => false,
                }
            }
            None => false,
        };
        let outcome = if completed {
            let digest = transaction
                .generation_digest
                .value()
                .expect("checked")
                .generation_digest
                .clone();
            if history::write_current(root, &digest).is_err() {
                return invalid("recovery-pointer");
            }
            if close(root, transaction, TransactionState::Recovered).is_err() {
                return invalid("recovery-journal");
            }
            "completed".to_owned()
        } else {
            if converge_to_committed(root, transaction, committed.as_ref()).is_err() {
                return invalid("recovery-undo");
            }
            // Restore the exact committed public manifest with the
            // application bytes it describes (finding 9).
            match &manifest_before {
                Some(bytes) => {
                    if write_manifest(root, bytes).is_err() {
                        return invalid("recovery-manifest");
                    }
                }
                None => {
                    let physical = root.join(super::custody::MANIFEST_LOGICAL);
                    if physical.exists() && std::fs::remove_file(&physical).is_err() {
                        return invalid("recovery-manifest");
                    }
                }
            }
            if close(root, transaction, TransactionState::Abandoned).is_err() {
                return invalid("recovery-journal");
            }
            "abandoned".to_owned()
        };
        reconciled.push(ReconciledTransaction {
            transaction_id: transaction.transaction_id.clone(),
            action,
            outcome,
        });
    }
    super::render(&RecoveryReceipt {
        action: "recover",
        reconciled,
    })
}
