//! The defined recovery path for interrupted managed publication (issue
//! #41).
//!
//! An interrupted publication leaves either an unresolved journal or a
//! stale mutation lock; both block every further mutation. Recovery is the
//! single authority allowed past that barrier, and it converges the project
//! with bounded, verifiable moves only:
//!
//! - **Complete**: a journal whose target generation record exists, is
//!   anchored to the committed lineage, whose every pending move is already
//!   on disk in exactly the recorded state, and whose public manifest
//!   already carries the exact published bytes of that generation, commits
//!   the interrupted generation by writing the custody pointer; the journal
//!   closes as `recovered`.
//! - **Undo**: every other journal converges its affected paths to the
//!   last committed generation's exact bytes — or to absence for a
//!   first publication that never had a committed generation — and
//!   restores the exact pre-transaction public manifest; the journal
//!   closes as `abandoned`.
//!
//! Recovery never invents bytes and never trusts a caller-authored journal
//! on its face (finding 1, r3 shape):
//!
//! - delete authorization is exact-path: a pending path the committed
//!   generation does not own must be an exact reviewed output of the
//!   journal's retained plan — the root-prefix widening is gone, so an
//!   unclaimed descendant under a generated root is never deletable;
//! - the target generation must be anchored to the committed lineage: the
//!   committed generation, one of its retained ancestors (a rollback
//!   destination), or its direct retained successor (an interrupted
//!   apply) — a minted record outside the committed chain is never
//!   adoptable;
//! - every live before-state is validated before the first byte moves: a
//!   path whose live bytes match neither the committed state nor the
//!   reviewed intent's before/after state is manual drift that only the
//!   confirmed resolve path may touch — a forged journal can neither
//!   overwrite it nor write partially before refusing;
//! - the stale mutation lock is cleared only after the OS-level advisory
//!   lock is acquired, positively proving no live holder (finding 3), and
//!   only through the confined capability;
//! - completion requires the public manifest: recovery never advances the
//!   pointer to a generation whose published manifest is missing or
//!   divergent (finding 10) — such a journal converges to the committed
//!   state instead and the blocked destination keeps the run unresolved;
//! - a first publication interrupted anywhere (including a real killed
//!   process with a prepared journal and empty generation pin) converges
//!   to exact absence: published application bytes are removed, the
//!   pre-transaction manifest absence is restored, and a fresh apply
//!   succeeds (finding 11).

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

/// The anchored target record of one journal: the generation named by the
/// journal must sit on the committed lineage — the committed generation
/// itself, one of its retained ancestors (a rollback destination), or its
/// direct retained successor (an interrupted apply). A record minted
/// outside the committed chain — however self-consistent its hashes — is
/// never adoptable custody state (findings 1 and 4): the chain link is the
/// anchor a caller cannot borrow without displacing the real lineage.
fn anchored_record(
    root: &std::path::Path,
    transaction: &Transaction,
    committed: Option<&super::wire::GenerationRecord>,
) -> Result<super::wire::GenerationRecord, DomainResult> {
    let digest = match transaction.generation_digest.value() {
        Some(digest) => digest.generation_digest.as_str(),
        None => return Err(invalid("recovery-unanchored")),
    };
    let record = history::record(root, digest)?;
    let on_lineage = match committed {
        // A first publication has no committed generation: the only
        // adoptable record is the journal's own direct successor claim
        // with no previous link of its own.
        None => record.previous_generation.value().is_none(),
        Some(committed) => {
            record.generation_digest == committed.generation_digest
                || match record.previous_generation.value() {
                    // The direct successor of the committed generation: an
                    // interrupted apply whose record was retained.
                    Some(previous) => previous.generation_digest == committed.generation_digest,
                    // Or an ancestor of the committed generation: a
                    // rollback destination retained in the lineage.
                    None => is_ancestor(root, &record.generation_digest, committed, 0)?,
                }
        }
    };
    if !on_lineage {
        return Err(invalid("recovery-unanchored"));
    }
    Ok(record)
}

/// Whether `candidate` is a retained ancestor of `committed` (bounded walk
/// over the `previous_generation` links).
fn is_ancestor(
    root: &std::path::Path,
    candidate: &str,
    committed: &super::wire::GenerationRecord,
    depth: usize,
) -> Result<bool, DomainResult> {
    if depth > 64 {
        return Err(invalid("recovery-lineage"));
    }
    match committed.previous_generation.value() {
        Some(previous) if previous.generation_digest == candidate => Ok(true),
        Some(previous) => {
            let parent = history::record(root, &previous.generation_digest)?;
            is_ancestor(root, candidate, &parent, depth + 1)
        }
        None => Ok(false),
    }
}

/// The reviewed intent of one journal: the retained reviewed document whose
/// exact outputs and before/after states bound every pending move. A
/// generation plan anchors apply journals; a rollback plan anchors rollback
/// journals. Journals without retained intent (resolve) are bounded by
/// their own pinned before-state.
enum Intent {
    Generation(super::wire::GenerationPlan),
    Rollback(super::wire::RollbackPlan),
}

/// The (before, after) byte digests one intent binds to a path: the states
/// the reviewed transaction itself may have produced or found.
fn intent_bounds(intent: &Intent, path: &str) -> (Option<String>, Option<String>) {
    match intent {
        Intent::Generation(plan) => plan
            .outputs
            .iter()
            .find(|output| output.path == path)
            .map(|output| {
                (
                    output.before_digest.value().cloned(),
                    output.after_digest.value().cloned(),
                )
            })
            .unwrap_or((None, None)),
        Intent::Rollback(plan) => {
            if let Some(op) = plan.restore.iter().find(|op| op.path == path) {
                // The rollback writes the destination bytes.
                (None, Some(op.digest.clone()))
            } else if let Some(op) = plan.delete.iter().find(|op| op.path == path) {
                // The rollback removes the current committed bytes.
                (Some(op.digest.clone()), None)
            } else {
                (None, None)
            }
        }
    }
}

/// Load the retained reviewed intent of one journal (finding 5): the
/// reviewed document is content-addressed in custody, so the scan admits
/// only a fully closed plan whose own digest is exactly the journal pin.
fn retained_intent(root: &std::path::Path, transaction: &Transaction) -> Option<Intent> {
    let digest = transaction.plan_digest.value()?;
    if let Ok(plan) = history::plan(root, digest) {
        return Some(Intent::Generation(plan));
    }
    if let Ok(plan) = history::rollback_plan(root, digest) {
        return Some(Intent::Rollback(plan));
    }
    None
}

/// One validated convergence move. Built for every pending op before the
/// first mutation, so a journal either converges completely or refuses
/// without any write (finding 2).
enum Move {
    /// Restore the exact committed bytes of a committed output.
    RestoreCommitted { path: String, bytes: Vec<u8> },
    /// Remove a file the interrupted transaction published (undo).
    Remove { path: String },
    /// Nothing to do: the live state already is the committed state.
    Noop,
}

/// Preflight every pending op of one journal into a complete move list
/// without writing anything. Every pending path the committed generation
/// does not own must be an exact output of the journal's retained intent;
/// live bytes must match the committed state, the journal's pinned
/// before-state, or the intent's before/after state — anything else is
/// manual drift that refuses the whole journal (findings 1 and 2).
fn convergence_moves(
    root: &std::path::Path,
    transaction: &Transaction,
    committed: Option<&super::wire::GenerationRecord>,
) -> Result<Vec<Move>, DomainResult> {
    let intent = retained_intent(root, transaction);
    let mut moves = Vec::new();
    for pending in &transaction.pending {
        if pending.path == super::custody::MANIFEST_LOGICAL {
            // The manifest move is converged separately, after the
            // application bytes.
            continue;
        }
        let committed_output = committed.and_then(|record| {
            record
                .outputs
                .iter()
                .find(|output| output.path == pending.path)
        });
        let live = fs::read_file(root, &pending.path).map_err(|_| invalid("recovery-drift"))?;
        let live_digest = live.as_deref().map(super::digest);
        if let Some(output) = committed_output {
            // The committed generation owns the path: converge to its
            // exact committed bytes. Manual drift is never overwritten —
            // live bytes that match none of the accepted states refuse the
            // journal (finding 2).
            let reference = match output.after_ref.value() {
                Some(reference) => reference,
                None => return Err(invalid("recovery-snapshot")),
            };
            let bytes = history::object(root, reference)?;
            let committed_digest = super::digest(&bytes);
            let (before, after) = intent
                .as_ref()
                .map(|intent| intent_bounds(intent, &pending.path))
                .unwrap_or((None, None));
            let pinned_before = transaction.before_digest.value().cloned();
            let acceptable = |digest: &str| {
                digest == committed_digest
                    || before.as_deref() == Some(digest)
                    || after.as_deref() == Some(digest)
                    || pinned_before.as_deref() == Some(digest)
            };
            match live_digest.as_deref() {
                Some(digest) if acceptable(digest) => {
                    if digest == committed_digest {
                        moves.push(Move::Noop);
                    } else {
                        moves.push(Move::RestoreCommitted {
                            path: pending.path.clone(),
                            bytes,
                        });
                    }
                }
                // Absent: the interrupted transaction already removed or
                // never wrote the committed bytes; restoring is exact.
                None => moves.push(Move::RestoreCommitted {
                    path: pending.path.clone(),
                    bytes,
                }),
                Some(_) => return Err(invalid("recovery-drift")),
            }
            continue;
        }
        // Nothing committed owns the path. The interrupted transaction may
        // only remove what it provably wrote: the path must be an exact
        // reviewed output of the journal's retained intent, and the live
        // bytes must be the intent's own before or after state (finding 1:
        // exact paths only — no generated-root prefix widening).
        let Some(intent) = &intent else {
            return Err(invalid("recovery-unowned"));
        };
        let (before, after) = intent_bounds(intent, &pending.path);
        if before.is_none() && after.is_none() {
            return Err(invalid("recovery-unowned"));
        }
        match live_digest.as_deref() {
            None => moves.push(Move::Noop),
            Some(digest) => {
                if Some(digest) == after.as_deref() {
                    // The interrupted transaction wrote these bytes; undo
                    // removes them.
                    moves.push(Move::Remove {
                        path: pending.path.clone(),
                    });
                } else if Some(digest) == before.as_deref() {
                    // The transaction never landed; nothing to undo.
                    moves.push(Move::Noop);
                } else {
                    return Err(invalid("recovery-drift"));
                }
            }
        }
    }
    Ok(moves)
}

/// Restore the pre-transaction public manifest of one journal: the exact
/// retained bytes pinned by `beforeManifestDigest`, or absence for a first
/// publication (findings 10 and 11).
fn restore_manifest_before(
    root: &std::path::Path,
    transaction: &Transaction,
) -> Result<(), DomainResult> {
    match transaction.before_manifest_digest.value() {
        Some(digest) => {
            let bytes = history::object(root, digest)?;
            super::custody::restore_manifest_bytes(root, &bytes)
                .map_err(|_| invalid("recovery-manifest"))
        }
        None => {
            if matches!(
                fs::read_file(root, super::custody::MANIFEST_LOGICAL),
                Ok(Some(_))
            ) && !fs::remove_file(root, super::custody::MANIFEST_LOGICAL)
                .map_err(|_| invalid("recovery-manifest"))?
            {
                return Err(invalid("recovery-manifest"));
            }
            Ok(())
        }
    }
}

/// Whether every pending move of `transaction` is already exactly the
/// recorded state of `record` on disk, and the public manifest already
/// carries the generation's exact published bytes (finding 10: completion
/// without the public manifest is a false success).
fn matches_generation_state(
    root: &std::path::Path,
    transaction: &Transaction,
    record: &super::wire::GenerationRecord,
) -> bool {
    let outputs: std::collections::BTreeMap<&str, &str> = record
        .outputs
        .iter()
        .map(|output| (output.path.as_str(), output.digest.as_str()))
        .collect();
    let all_pending_match = transaction.pending.iter().all(|pending| {
        if pending.path == super::custody::MANIFEST_LOGICAL {
            return true;
        }
        match &pending.op {
            PendingOp::Delete => {
                // A delete that landed leaves the path absent, and the
                // destination generation must not own the path.
                if outputs.contains_key(pending.path.as_str()) {
                    return false;
                }
                matches!(fs::read_file(root, &pending.path), Ok(None))
            }
            PendingOp::Publish | PendingOp::Restore => {
                let Some(recorded) = outputs.get(pending.path.as_str()) else {
                    return false;
                };
                match fs::read_file(root, &pending.path) {
                    Ok(Some(bytes)) => &super::digest(&bytes) == recorded,
                    _ => false,
                }
            }
        }
    });
    if !all_pending_match {
        return false;
    }
    // The published manifest must already be this generation's exact
    // retained manifest before the pointer may advance (finding 10).
    let Ok(retained) = super::custody::retained_manifest(root, &record.generation_digest) else {
        return false;
    };
    match fs::read_file(root, super::custody::MANIFEST_LOGICAL) {
        Ok(Some(published)) => super::digest(&published) == super::digest(&retained),
        _ => false,
    }
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
    // The committed generation every undo converges to.
    let committed = match history::current(root) {
        Ok(Some(digest)) => match history::record(root, &digest) {
            Ok(record) => Some(record),
            Err(error) => return error,
        },
        Ok(None) => None,
        Err(error) => return error,
    };
    // A stale mutation lock is itself an interrupted mutation. Recovery is
    // the only path allowed to clear it — and only after acquiring the
    // lock's OS-level advisory lock proves no live holder (finding 3), and
    // the confined capability proves the leaf is a plain single-link file
    // inside the custody home.
    let lock_present = match fs::read_file(root, &history::custody_logical(history::MUTATION_LOCK))
    {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(_) => return invalid("recovery-lock"),
    };
    if lock_present {
        if history::prove_lock_stale(root).is_err() {
            return invalid("recovery-lock-live");
        }
        if fs::remove_file(root, &history::custody_logical(history::MUTATION_LOCK)).is_err() {
            return invalid("recovery-lock");
        }
    }
    let mut reconciled = Vec::new();
    for transaction in &unresolved {
        let action = match transaction.action {
            super::wire::TransactionAction::Apply => "apply".to_owned(),
            super::wire::TransactionAction::Rollback => "rollback".to_owned(),
            super::wire::TransactionAction::Resolve => "resolve".to_owned(),
        };
        // The complete journal is validated — anchor, record, retained
        // intent, snapshots, and every live before-state — before the
        // first byte of this journal moves (finding 2). An unanchorable
        // journal (a forged or foreign generation pin) is not refused
        // outright: it converges through the same exact-path preflight as
        // every other journal, so a journal that provably moved nothing
        // abandons cleanly while one naming real paths refuses without a
        // single write.
        let anchored = match transaction.generation_digest.value() {
            Some(_) => anchored_record(root, transaction, committed.as_ref()).ok(),
            None => None,
        };
        let outcome = match anchored {
            Some(record) if matches_generation_state(root, transaction, &record) => {
                // Completion: the disk already carries exactly the anchored
                // generation's state, public manifest included; commit the
                // custody pointer.
                if history::write_current(root, &record.generation_digest).is_err() {
                    return invalid("recovery-pointer");
                }
                if close(root, transaction, TransactionState::Recovered).is_err() {
                    return invalid("recovery-journal");
                }
                "completed".to_owned()
            }
            _ => match abandon(root, transaction, committed.as_ref()) {
                Ok(outcome) => outcome,
                Err(error) => return error,
            },
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

/// Converge one journal back to the committed state (or absence) and close
/// it as `abandoned`. Every move is prebuilt before the first mutation.
fn abandon(
    root: &std::path::Path,
    transaction: &Transaction,
    committed: Option<&super::wire::GenerationRecord>,
) -> Result<String, DomainResult> {
    // Preflight: the complete move list is built before any write.
    let moves = match convergence_moves(root, transaction, committed) {
        Ok(moves) => moves,
        Err(_) => return Err(invalid("recovery-undo")),
    };
    for one in &moves {
        match one {
            Move::RestoreCommitted { path, bytes } => {
                if fs::write_file(root, path, bytes).is_err() {
                    return Err(invalid("recovery-write"));
                }
            }
            Move::Remove { path } => {
                if fs::remove_file(root, path).is_err() {
                    return Err(invalid("recovery-delete"));
                }
            }
            Move::Noop => {}
        }
    }
    // Restore the exact pre-transaction public manifest with the
    // application bytes it describes — only when the journal itself moved
    // the manifest (findings 10 and 11). A resolve journal never touches
    // it.
    if transaction
        .pending
        .iter()
        .any(|pending| pending.path == super::custody::MANIFEST_LOGICAL)
        && restore_manifest_before(root, transaction).is_err()
    {
        return Err(invalid("recovery-manifest"));
    }
    if close(root, transaction, TransactionState::Abandoned).is_err() {
        return Err(invalid("recovery-journal"));
    }
    Ok("abandoned".to_owned())
}
