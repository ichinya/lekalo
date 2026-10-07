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
//! Recovery never invents bytes: every write comes from a verified custody
//! object, and a stale lock is cleared only here, under the same confined
//! filesystem capability as every other mutator.

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
fn converge_to_committed(
    root: &std::path::Path,
    transaction: &Transaction,
    current: Option<&super::wire::GenerationRecord>,
) -> Result<(), DomainResult> {
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
                // Nothing committed owns the path: the interrupted
                // transaction published it, so it goes.
                if fs::remove_file(root, &pending.path).is_err() {
                    return Err(invalid("recovery-delete"));
                }
            }
        }
    }
    Ok(())
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
    // A stale mutation lock is itself an interrupted mutation. Recovery is
    // the only path allowed to clear it; every later step revalidates the
    // bytes it moves.
    let lock_path = root.join(history::custody_logical(history::MUTATION_LOCK));
    if lock_path.exists() {
        let _ = std::fs::remove_file(&lock_path);
    }
    let unresolved = match history::unresolved(root) {
        Ok(unresolved) => unresolved,
        Err(error) => return error,
    };
    if unresolved.is_empty() {
        return invalid("nothing-to-recover");
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
