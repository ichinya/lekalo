//! Exact recorded restoration: rollback to a retained generation and
//! resolve of drifted generated paths (issue #41).
//!
//! Restoration consumes retained exact bytes through the same path and
//! before-state transaction checks as apply. Model and custom sources are
//! never restored, and an old adapter is never executed. Every mutator
//! passes the centralized unresolved-journal barrier before taking the
//! mutation lock and rechecks under it, every restored and deleted byte is
//! revalidated against the committed baseline so a rollback can never
//! silently overwrite manual edits, and the resolve confirmation digest
//! pins the exact drifted before-state it was reviewed against.

use super::diagnostic;
use super::fs;
use super::history;
use super::plan::require_confirmation;
use super::wire::{
    DeleteOp, GenerationDigest, Pending, PendingOp, RestoreOp, RollbackPlan, State, Transaction,
    TransactionAction, TransactionState,
};
use super::{
    detail, plan_digest, verify_rollback_plan, DomainResult, GENERATED_DRIFT, INPUT_INVALID,
    OWNERSHIP_DENIED, PLAN_STALE, ROLLBACK_REFUSED,
};

/// The rollback preview receipt.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReceipt {
    pub action: &'static str,
    pub dry_run: bool,
    pub plan_digest: String,
    pub generation_digest: String,
    pub restored: Vec<String>,
    pub deleted: Vec<String>,
}

fn invalid(reason: &str) -> DomainResult {
    diagnostic::failure(INPUT_INVALID, &detail(reason))
}

/// The live project's canonical lock revision, derived through the
/// accepted pipeline (read-only). `None` when the project's lock is
/// missing or unreadable — the caller's compatibility check fails
/// closed on any divergence.
fn live_lock_digest(root: &std::path::Path) -> Option<String> {
    let lock = crate::lockfile::plan::LockService::read_state_at(root).ok()?;
    match lock {
        crate::lockfile::LockState::Present(lock) => Some(
            crate::lockfile::canonical::lock_digest(&lock)
                .as_str()
                .to_owned(),
        ),
        crate::lockfile::LockState::Absent => None,
    }
}

fn refused(reason: &str) -> DomainResult {
    diagnostic::failure(ROLLBACK_REFUSED, &detail(reason))
}

fn drift(path: &str, reason: &str) -> DomainResult {
    diagnostic::failure(GENERATED_DRIFT, &[("path", path), ("reason", reason)])
}

/// Build the exact rollback plan for the retained destination generation.
pub fn build(root: &std::path::Path, destination: &str) -> Result<RollbackPlan, DomainResult> {
    // The barrier holds for every mutating surface, previews included: an
    // unresolved journal is reconciled before any rollback authority is
    // computed (finding J).
    history::assert_reconciled(root)?;
    let current = match history::current(root)? {
        Some(current) => current,
        None => return Err(refused("no-generation")),
    };
    if current == destination {
        return Err(invalid("already-current"));
    }
    // Both records pass the content-bound custody verification: a tampered
    // generation record is refused before any restore authority exists.
    let (destination_record, _) = history::record_with_custody(root, destination)?;
    let (current_record, _) = history::record_with_custody(root, &current)?;
    let mut restore = Vec::new();
    for output in &destination_record.outputs {
        let object_ref = match output.after_ref.value() {
            Some(reference) => reference.clone(),
            None => return Err(refused("snapshot")),
        };
        // Every retained byte is verified before the plan exists.
        if let Err(error) = history::object(root, &object_ref) {
            let _ = error;
            return Err(refused("snapshot"));
        }
        restore.push(RestoreOp {
            path: output.path.clone(),
            digest: output.digest.clone(),
            object_ref,
        });
    }
    let destination_paths: std::collections::BTreeSet<&str> =
        restore.iter().map(|op| op.path.as_str()).collect();
    let mut delete = Vec::new();
    for output in &current_record.outputs {
        if destination_paths.contains(output.path.as_str()) {
            continue;
        }
        delete.push(DeleteOp {
            path: output.path.clone(),
            digest: output.digest.clone(),
        });
    }
    let mut plan = RollbackPlan {
        schema_version: super::header("managed-rollback-plan").0,
        identity: super::header("managed-rollback-plan").1,
        current_generation: State::known(GenerationDigest {
            generation_digest: current,
        }),
        destination_generation_digest: destination.to_owned(),
        restore,
        delete,
        manifest_digest: super::custody::retained_manifest(root, destination)
            .map(|bytes| super::digest(&bytes))?,
        plan_digest: String::new(),
    };
    plan.plan_digest = plan_digest(&plan);
    Ok(plan)
}

/// The committed baseline digests of the current generation: every path a
/// rollback mutation may touch must still carry exactly these bytes.
fn current_baseline(
    root: &std::path::Path,
) -> Result<std::collections::BTreeMap<String, String>, DomainResult> {
    let current = match history::current(root)? {
        Some(current) => current,
        None => return Err(refused("no-generation")),
    };
    let (record, _) = history::record_with_custody(root, &current)?;
    Ok(record
        .outputs
        .into_iter()
        .map(|output| (output.path, output.digest))
        .collect())
}

/// Revalidate the live baseline of one mutation target: the live bytes
/// must still be the committed bytes (a missing file is drift on a restore
/// path and already-gone on a delete path).
fn check_baseline(
    root: &std::path::Path,
    baseline: &std::collections::BTreeMap<String, String>,
    path: &str,
    restore: bool,
) -> Result<(), DomainResult> {
    let recorded = match baseline.get(path) {
        Some(recorded) => recorded,
        None => return Err(drift(path, "unclaimed")),
    };
    match fs::read_file(root, path) {
        Ok(Some(live)) => {
            if &super::digest(&live) != recorded {
                // Manual drift is resolved only by the exact resolve
                // transaction; a rollback never overwrites an edited file.
                return Err(drift(path, "edited"));
            }
            Ok(())
        }
        Ok(None) => {
            if restore {
                Err(drift(path, "missing"))
            } else {
                Ok(())
            }
        }
        Err(_) => Err(drift(path, "unreadable")),
    }
}

/// Execute (or preview) one confirmed rollback.
pub fn apply(
    root: &std::path::Path,
    plan_bytes: &[u8],
    confirm: Option<&str>,
    dry_run: bool,
) -> DomainResult {
    let plan = match verify_rollback_plan(plan_bytes) {
        Ok(plan) => plan,
        Err(error) => return error,
    };
    if !dry_run {
        if let Err(error) = require_confirmation(confirm, &plan.plan_digest) {
            return error;
        }
    }
    // The centralized barrier holds before and under the lock (finding J).
    if let Err(error) = history::assert_reconciled(root) {
        return error;
    }
    // The current generation must still match the reviewed before-state.
    let current = match history::current(root) {
        Ok(Some(current)) => current,
        Ok(None) => return refused("no-generation"),
        Err(error) => return error,
    };
    let expected = plan
        .current_generation
        .value()
        .map(|digest| digest.generation_digest.clone());
    if Some(&current) != expected.as_ref() {
        return diagnostic::failure(PLAN_STALE, &detail("current-generation"));
    }
    // The destination record is trusted only after full custody
    // verification (identity, inventory digest, retained objects).
    let destination_record = match history::record(root, &plan.destination_generation_digest) {
        Ok(record) => record,
        Err(error) => return error,
    };
    // Ownership compatibility: the destination generation was committed
    // under a different ownership configuration than the project's current
    // generation; restoration would resurrect stale owners.
    let (current_record, _) = match history::record_with_custody(root, &current) {
        Ok(result) => result,
        Err(error) => return error,
    };
    if current_record.ownership_digest != destination_record.ownership_digest {
        return refused("ownership");
    }
    // Canonical Model/IR/lock compatibility (finding 8): the destination
    // record pins the exact lock revision its generation was admitted
    // under. The live project must still carry that exact lock revision —
    // a moved canonical query or a replaced/broken lock file makes the
    // destination incompatible until the caller restores the retained
    // inputs. The lock pin rides the authenticated record identity, so
    // the destination pin is custody evidence, not a caller claim.
    if let (Some(destination_lock), Some(current_lock)) = (
        destination_record.lock_pin.value(),
        current_record.lock_pin.value(),
    ) {
        // The destination's pinned lock must be exactly the live project
        // lock: a moved canonical query or a replaced/broken lock file
        // makes the destination incompatible until the caller restores
        // the retained inputs (finding 8).
        let live = live_lock_digest(root);
        if live.as_deref() != Some(destination_lock.as_str()) {
            return diagnostic::failure(PLAN_STALE, &detail("destination-lock"));
        }
        let _ = current_lock;
    } else if destination_record.lock_pin.value().is_some() {
        return refused("lock");
    }
    // Destination input closure: every pinned input of the destination
    // generation must still exist with its exact recorded bytes — the
    // current Model and attachments are still the ones the destination was
    // generated from.
    for pin in &destination_record.input_closure {
        match fs::read_file(root, &pin.path) {
            Ok(Some(bytes)) => {
                if super::digest(&bytes) != pin.digest {
                    return diagnostic::failure(
                        PLAN_STALE,
                        &[("path", pin.path.as_str()), ("reason", "input")],
                    );
                }
            }
            Ok(None) => {
                return diagnostic::failure(
                    PLAN_STALE,
                    &[("path", pin.path.as_str()), ("reason", "input")],
                )
            }
            Err(_) => {
                return diagnostic::failure(
                    PLAN_STALE,
                    &[("path", pin.path.as_str()), ("reason", "input")],
                )
            }
        }
    }
    // The baseline of every touched path is the committed current bytes:
    // restore and delete targets alike are revalidated before any plan
    // exists, so manual edits always require the exact resolve first.
    let baseline = match current_baseline(root) {
        Ok(baseline) => baseline,
        Err(error) => return error,
    };
    for op in &plan.delete {
        if let Err(error) = check_baseline(root, &baseline, &op.path, false) {
            return error;
        }
    }
    // Every restored byte is re-verified from custody before publishing.
    let mut bytes = Vec::new();
    for op in &plan.restore {
        if let Err(error) = check_baseline(root, &baseline, &op.path, true) {
            return error;
        }
        match history::object(root, &op.object_ref) {
            Ok(object) => {
                if super::digest(&object) != op.digest {
                    return refused("snapshot");
                }
                bytes.push((op, object));
            }
            Err(error) => return error,
        }
    }
    if dry_run {
        return super::render(&RestoreReceipt {
            action: "rollback",
            dry_run: true,
            plan_digest: plan.plan_digest.clone(),
            generation_digest: plan.destination_generation_digest.clone(),
            restored: plan.restore.iter().map(|op| op.path.clone()).collect(),
            deleted: plan.delete.iter().map(|op| op.path.clone()).collect(),
        });
    }
    // The unresolved-journal barrier holds before and under the lock.
    if let Err(error) = history::assert_reconciled(root) {
        return error;
    }
    let guard = match history::lock(root) {
        Ok(guard) => guard,
        Err(error) => return error,
    };
    let result = (|| {
        history::assert_reconciled(root)?;
        // The baseline is revalidated under the mutation lock: a racer
        // that edited a touched file after the preflight cannot win.
        let baseline = current_baseline(root)?;
        for op in &plan.delete {
            check_baseline(root, &baseline, &op.path, false)?;
        }
        for op in &plan.restore {
            check_baseline(root, &baseline, &op.path, true)?;
        }
        staged(root, &plan, &bytes)
    })();
    drop(guard);
    match result {
        Ok(()) => super::render(&RestoreReceipt {
            action: "rollback",
            dry_run: false,
            plan_digest: plan.plan_digest.clone(),
            generation_digest: plan.destination_generation_digest.clone(),
            restored: plan.restore.iter().map(|op| op.path.clone()).collect(),
            deleted: plan.delete.iter().map(|op| op.path.clone()).collect(),
        }),
        Err(error) => error,
    }
}

fn staged(
    root: &std::path::Path,
    plan: &RollbackPlan,
    bytes: &[(&RestoreOp, Vec<u8>)],
) -> Result<(), DomainResult> {
    let transaction_id = history::transaction_id(root, &plan.plan_digest)?;
    let mut transaction = Transaction {
        schema_version: super::header("managed-transaction").0,
        identity: super::header("managed-transaction").1,
        transaction_id,
        state: TransactionState::Prepared,
        action: TransactionAction::Rollback,
        plan_digest: State::known(plan.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: plan.destination_generation_digest.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::known(plan.manifest_digest.clone()),
        pending: Vec::new(),
        affected_files: plan
            .restore
            .iter()
            .map(|op| op.path.clone())
            .chain(plan.delete.iter().map(|op| op.path.clone()))
            .collect(),
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    transaction.affected_files.sort();
    let mut pending: Vec<Pending> = bytes
        .iter()
        .map(|(op, _)| Pending {
            path: op.path.clone(),
            op: PendingOp::Restore,
        })
        .collect();
    pending.extend(plan.delete.iter().map(|op| Pending {
        path: op.path.clone(),
        op: PendingOp::Delete,
    }));
    pending.sort_by(|left, right| (&left.path, &left.op).cmp(&(&right.path, &right.op)));
    transaction.pending = pending;
    transaction.state = TransactionState::Restoring;
    history::write_transaction(root, &transaction)?;
    for (op, bytes) in bytes {
        if fs::write_file(root, &op.path, bytes).is_err() {
            transaction.conflict = State::known(super::wire::Conflict {
                path: op.path.clone(),
                reason: "restore".to_owned(),
            });
            let _ = history::write_transaction(root, &transaction);
            return Err(invalid("restore-write"));
        }
    }
    for op in &plan.delete {
        match fs::remove_file(root, &op.path) {
            Ok(_) => {}
            Err(_) => {
                transaction.conflict = State::known(super::wire::Conflict {
                    path: op.path.clone(),
                    reason: "delete".to_owned(),
                });
                let _ = history::write_transaction(root, &transaction);
                return Err(invalid("restore-delete"));
            }
        }
    }
    history::write_current(root, &plan.destination_generation_digest)?;
    // The retained governed manifest of the destination generation joins
    // the restoration (finding 9): the public ownership statement must
    // describe the restored bytes, not the superseded generation. The
    // exact bytes come from the destination's retained snapshot, already
    // digest-verified when the plan was built.
    let retained = super::custody::retained_manifest(root, &plan.destination_generation_digest)?;
    let physical = root.join(super::custody::MANIFEST_LOGICAL);
    let stage = physical.with_extension("json.tmp");
    if std::fs::write(&stage, &retained).is_err() || std::fs::rename(&stage, &physical).is_err() {
        transaction.conflict = State::known(super::wire::Conflict {
            path: super::custody::MANIFEST_LOGICAL.to_owned(),
            reason: "manifest-restore".to_owned(),
        });
        let _ = history::write_transaction(root, &transaction);
        return Err(invalid("restore-manifest"));
    }
    transaction.state = TransactionState::Committed;
    match history::write_transaction(root, &transaction) {
        Ok(()) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Build and execute (or preview) the resolve of one drifted generated
/// path: exact recorded bytes, same transaction checks, no new baseline.
/// The previewed transaction pins the drifted before-state; only the
/// caller's confirmation over that exact digest can move the file.
pub fn resolve(
    root: &std::path::Path,
    path: &str,
    confirm: Option<&str>,
    dry_run: bool,
) -> DomainResult {
    if !super::rel_path(path) {
        return invalid("path");
    }
    let current = match history::current(root) {
        Ok(Some(current)) => current,
        Ok(None) => {
            return diagnostic::failure(
                OWNERSHIP_DENIED,
                &[("path", path), ("reason", "unclaimed")],
            )
        }
        Err(error) => return error,
    };
    // The record is trusted only after the content-bound custody checks.
    let record = match history::record(root, &current) {
        Ok(record) => record,
        Err(error) => return error,
    };
    let output = match record.outputs.iter().find(|output| output.path == path) {
        Some(output) => output,
        None => {
            return diagnostic::failure(
                OWNERSHIP_DENIED,
                &[("path", path), ("reason", "unclaimed")],
            )
        }
    };
    let object_ref = match output.after_ref.value() {
        Some(reference) => reference.clone(),
        None => return refused("snapshot"),
    };
    let bytes = match history::object(root, &object_ref) {
        Ok(bytes) => bytes,
        Err(error) => return error,
    };
    if super::digest(&bytes) != output.digest {
        return refused("snapshot");
    }
    // The drifted before-state rides the transaction: the confirmation
    // digest covers it, so a stale preview (the file moved after the
    // caller reviewed it) can never publish over unseen bytes.
    if let Err(error) = history::assert_reconciled(root) {
        return error;
    }
    let before_digest = match fs::read_file(root, path) {
        Ok(Some(live)) => {
            if super::digest(&live) == output.digest {
                return invalid("not-drifted");
            }
            State::known(super::digest(&live))
        }
        Ok(None) => State::Absent,
        Err(_) => return drift(path, "unreadable"),
    };
    let mut transaction = Transaction {
        schema_version: super::header("managed-transaction").0,
        identity: super::header("managed-transaction").1,
        transaction_id: String::new(),
        state: TransactionState::Prepared,
        action: TransactionAction::Resolve,
        plan_digest: State::Absent,
        generation_digest: State::known(GenerationDigest {
            generation_digest: current.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: path.to_owned(),
            op: PendingOp::Restore,
        }],
        affected_files: vec![path.to_owned()],
        conflict: State::Absent,
        before_digest,
    };
    transaction.plan_digest = State::known(plan_digest(&transaction));
    let expected = transaction.plan_digest.value().expect("digest").clone();
    if !dry_run {
        if let Err(error) = require_confirmation(confirm, &expected) {
            return error;
        }
    }
    if dry_run {
        return super::render(&RestoreReceipt {
            action: "resolve",
            dry_run: true,
            plan_digest: transaction.plan_digest.value().expect("digest").clone(),
            generation_digest: current,
            restored: vec![path.to_owned()],
            deleted: Vec::new(),
        });
    }
    // The unresolved-journal barrier holds before and under the lock.
    if let Err(error) = history::assert_reconciled(root) {
        return error;
    }
    let guard = match history::lock(root) {
        Ok(guard) => guard,
        Err(error) => return error,
    };
    let result = (|| -> Result<(), DomainResult> {
        history::assert_reconciled(root)?;
        // The pinned before-state is revalidated under the lock: the live
        // bytes must still be exactly the reviewed drifted bytes.
        match fs::read_file(root, path) {
            Ok(Some(live)) => {
                if Some(super::digest(&live)) != transaction.before_digest.value().cloned() {
                    return Err(drift(path, "moved"));
                }
            }
            Ok(None) => {
                if transaction.before_digest.value().is_some() {
                    return Err(drift(path, "moved"));
                }
            }
            Err(_) => return Err(drift(path, "unreadable")),
        }
        transaction.transaction_id = history::transaction_id(root, &expected)?;
        let outcome = staged_resolve(root, &transaction, &bytes);
        if outcome.status() == crate::result::Status::Valid {
            Ok(())
        } else {
            Err(outcome)
        }
    })();
    drop(guard);
    match result {
        Ok(()) => super::render(&RestoreReceipt {
            action: "resolve",
            dry_run: false,
            plan_digest: expected,
            generation_digest: current,
            restored: vec![path.to_owned()],
            deleted: Vec::new(),
        }),
        Err(error) => error,
    }
}

fn staged_resolve(root: &std::path::Path, transaction: &Transaction, bytes: &[u8]) -> DomainResult {
    if let Err(error) = history::write_transaction(root, transaction) {
        return error;
    }
    let mut restoring = transaction.clone();
    restoring.state = TransactionState::Restoring;
    if let Err(error) = history::write_transaction(root, &restoring) {
        return error;
    }
    let path = &transaction.affected_files[0];
    if fs::write_file(root, path, bytes).is_err() {
        return invalid("restore-write");
    }
    restoring.state = TransactionState::Committed;
    match history::write_transaction(root, &restoring) {
        // The caller renders the real receipt; this payload is discarded.
        Ok(()) => DomainResult::receipt("{}".to_owned(), "resolved".to_owned()),
        Err(error) => error,
    }
}
