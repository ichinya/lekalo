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
    DeleteOp, GenerationDigest, GenerationRecord, Pending, PendingOp, RestoreOp, RollbackPlan,
    State, Transaction, TransactionAction, TransactionState,
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

/// The live project's canonical lock revision — see
/// [`super::authority::live_lock_digest`]. Kept as a thin local alias so
/// the rollback compatibility check reads against the same definition the
/// apply path rechecks under the mutation lock.
fn live_lock_digest(root: &std::path::Path) -> Option<String> {
    super::authority::live_lock_digest(root)
}

fn refused(reason: &str) -> DomainResult {
    diagnostic::failure(ROLLBACK_REFUSED, &detail(reason))
}

fn drift(path: &str, reason: &str) -> DomainResult {
    diagnostic::failure(GENERATED_DRIFT, &[("path", path), ("reason", reason)])
}

/// Destination compatibility (finding 8): the destination generation was
/// admitted under an exact canonical Model/IR/lock authority, and the live
/// project must still derive exactly that authority before its bytes may
/// be restored. The lock revision rides the authenticated record identity,
/// so the destination pins are custody evidence, not caller claims; the
/// derived authority pins additionally bind the stable generation identity
/// to the Model and IR it was rendered from, so a moved canonical Model —
/// even under unchanged lock bytes — makes the destination incompatible
/// until the caller restores the retained inputs. Every check runs before
/// any mutation and again under the mutation lock; a caller that cannot
/// derive the live authority fails closed.
fn check_destination_compatibility(
    root: &std::path::Path,
    destination_record: &GenerationRecord,
    current_record: &GenerationRecord,
    authority: Option<&super::authority::Authority>,
) -> Result<(), DomainResult> {
    if let (Some(destination_lock), Some(current_lock)) = (
        destination_record.lock_pin.value(),
        current_record.lock_pin.value(),
    ) {
        // The destination's pinned lock must be exactly the live project
        // lock: a replaced or broken lock file makes the destination
        // incompatible until the caller restores the retained inputs.
        let live = live_lock_digest(root);
        if live.as_deref() != Some(destination_lock.as_str()) {
            return Err(diagnostic::failure(PLAN_STALE, &detail("destination-lock")));
        }
        let _ = current_lock;
    } else if destination_record.lock_pin.value().is_some() {
        return Err(refused("lock"));
    }
    // The derived authority pins bind the committed closure to the exact
    // Model/IR/lock the generation was admitted under. A caller that does
    // not present the live derived authority can never prove compatibility.
    if destination_record
        .input_closure
        .iter()
        .any(|pin| pin.path.starts_with(super::AUTHORITY_PIN_PREFIX))
    {
        let Some(live) = authority else {
            return Err(refused("authority"));
        };
        for (name, live_digest) in [
            ("model", &live.model_digest),
            ("ir", &live.ir_digest),
            ("lock", &live.lock_digest),
        ] {
            let pinned = format!("{}{name}", super::AUTHORITY_PIN_PREFIX);
            match destination_record
                .input_closure
                .iter()
                .find(|pin| pin.path == pinned)
            {
                Some(pin) if &pin.digest == live_digest => {}
                _ => {
                    return Err(diagnostic::failure(
                        PLAN_STALE,
                        &detail("destination-authority"),
                    ))
                }
            }
        }
    }
    Ok(())
}

/// The mandatory dependency closure of the destination generation
/// (finding 13): the retained plan and coverage of the destination pin the
/// adapter entry and every validated custom binding the restored handlers
/// depend on. Each dependency must still exist live with its exact pinned
/// bytes; a missing or changed binding would restore handlers whose
/// imports no longer resolve. Retained evidence is mandatory: a record
/// without its retained plan/coverage objects has no provable dependency
/// closure and refuses.
fn check_dependency_closure(
    root: &std::path::Path,
    destination_record: &GenerationRecord,
    authority: Option<&super::authority::Authority>,
) -> Result<(), DomainResult> {
    let plan = history::plan(root, &destination_record.plan_digest)?;
    let coverage = history::coverage(root, &plan.coverage_digest)?;
    for check in &coverage.extensions {
        if !check.validated {
            continue;
        }
        let Some(file) = super::bindings::binding_file(&check.binding) else {
            return Err(stale(&check.binding, "binding"));
        };
        match fs::read_file(root, file) {
            Ok(Some(bytes)) => {
                if super::digest(&bytes) != check.contract_digest {
                    return Err(stale(file, "binding-digest"));
                }
            }
            Ok(None) => return Err(stale(file, "binding-missing")),
            Err(_) => return Err(stale(file, "binding-unreadable")),
        }
    }
    // The locked adapter entry is a mandatory dependency of the rendered
    // generation: the live locked adapter is the same package the
    // destination pinned (the lock revision is revalidated above), so its
    // entry bytes must still exist and match the locked entry artifact.
    if let Some(adapter) = authority.and_then(|authority| authority.adapter.as_ref()) {
        match fs::read_file(root, &coverage.adapter.entry) {
            Ok(Some(bytes)) => {
                if super::digest(&bytes) != adapter.entry_digest {
                    return Err(stale(&coverage.adapter.entry, "adapter-entry"));
                }
            }
            Ok(None) => return Err(stale(&coverage.adapter.entry, "adapter-entry-missing")),
            Err(_) => return Err(stale(&coverage.adapter.entry, "adapter-entry-unreadable")),
        }
    }
    Ok(())
}

fn stale(path: &str, reason: &str) -> DomainResult {
    diagnostic::failure(PLAN_STALE, &[("path", path), ("reason", reason)])
}

/// The authoritative rollback plan of one interrupted rollback journal,
/// rebuilt purely from transaction-owned custody evidence (r4 finding 1):
/// the anchored destination record and the committed record the rollback
/// ran from. A journal's own plan pin must equal this rebuilt digest — a
/// minted rollback plan object in the store never bounds a recovery move,
/// exactly as a minted generation plan never bounds an apply journal.
pub(crate) fn recovery_plan(
    root: &std::path::Path,
    destination: &GenerationRecord,
    committed: &GenerationRecord,
) -> Option<RollbackPlan> {
    let mut restore = Vec::new();
    for output in &destination.outputs {
        let object_ref = output.after_ref.value()?.clone();
        history::object(root, &object_ref).ok()?;
        restore.push(RestoreOp {
            path: output.path.clone(),
            digest: output.digest.clone(),
            object_ref,
        });
    }
    let destination_paths: std::collections::BTreeSet<&str> =
        restore.iter().map(|op| op.path.as_str()).collect();
    let mut delete = Vec::new();
    for output in &committed.outputs {
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
            generation_digest: committed.generation_digest.clone(),
        }),
        destination_generation_digest: destination.generation_digest.clone(),
        restore,
        delete,
        manifest_digest: super::digest(
            &super::custody::retained_manifest(root, &destination.generation_digest).ok()?,
        ),
        plan_digest: String::new(),
    };
    plan.plan_digest = plan_digest(&plan);
    Some(plan)
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
    // The current record is additionally proven to biject with the
    // published manifest (finding 4): a caller-retargeted current record
    // is never restore authority.
    let (destination_record, _) = history::record_with_custody(root, destination)?;
    let current_record = history::current_record(root)?;
    if current_record.generation_digest != current {
        return Err(refused("record-identity"));
    }
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
    // The current record is consumed only through the public-manifest
    // bijection gate (finding 4).
    let record = history::current_record(root)?;
    Ok(record
        .outputs
        .into_iter()
        .map(|output| (output.path, output.digest))
        .collect())
}

/// Revalidate the live baseline of one mutation target: the live bytes
/// must still be the committed bytes. A path the current generation does
/// not own is an unclaimed collision only when live bytes occupy it — an
/// absent path is a legitimate rollback recreation of an output a later
/// generation deleted (finding 3/14): the destination's retained bytes are
/// digest-verified, and the refusal is reserved for foreign live bytes.
fn check_baseline(
    root: &std::path::Path,
    baseline: &std::collections::BTreeMap<String, String>,
    path: &str,
    restore: bool,
) -> Result<(), DomainResult> {
    let recorded = match baseline.get(path) {
        Some(recorded) => recorded,
        None => {
            if !restore {
                return Err(drift(path, "unclaimed"));
            }
            // Recreating a deleted output: refuse only foreign live bytes.
            return match fs::read_file(root, path) {
                Ok(None) => Ok(()),
                Ok(Some(_)) => Err(drift(path, "unclaimed")),
                Err(_) => Err(drift(path, "unreadable")),
            };
        }
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
///
/// `authority` is the live generation authority the caller derived from the
/// current project (the CLI derives it through the accepted loader
/// pipeline); the destination is compatible only while it still matches
/// the destination's derived custody pins (finding 8).
pub fn apply(
    root: &std::path::Path,
    plan_bytes: &[u8],
    confirm: Option<&str>,
    dry_run: bool,
    authority: Option<&super::authority::Authority>,
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
    let current_record = match history::current_record(root) {
        Ok(record) => record,
        Err(error) => return error,
    };
    if current_record.ownership_digest != destination_record.ownership_digest {
        return refused("ownership");
    }
    // Canonical Model/IR/lock compatibility (finding 8): the destination
    // generation is restorable only while the live project still derives
    // the exact authority it was admitted under.
    if let Err(error) =
        check_destination_compatibility(root, &destination_record, &current_record, authority)
    {
        return error;
    }
    // Destination input closure: every pinned input of the destination
    // generation must still exist with its exact recorded bytes — the
    // current Model and attachments are still the ones the destination was
    // generated from. Derived authority pins (`.lekalo/managed-authority/*`)
    // are the committed Model/IR/lock custody evidence, not project files:
    // they are revalidated against the live authority above.
    for pin in &destination_record.input_closure {
        if pin.path.starts_with(super::AUTHORITY_PIN_PREFIX) {
            continue;
        }
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
    // Mandatory dependency closure (finding 13): the restored handlers
    // import the destination's qualified custom bindings and bind the
    // locked adapter entry, so those dependencies must still exist with
    // their exact pinned bytes before any restoration authority exists.
    // The evidence comes from the retained plan/coverage objects, never
    // from caller input.
    if let Err(error) = check_dependency_closure(root, &destination_record, authority) {
        return error;
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
        // Destination compatibility is revalidated under the mutation
        // lock: a racer that moved the lock, the canonical Model, or the
        // IR after the preflight cannot win (finding P).
        check_destination_compatibility(root, &destination_record, &current_record, authority)?;
        // The baseline is revalidated under the mutation lock: a racer
        // that edited a touched file after the preflight cannot win.
        let baseline = current_baseline(root)?;
        for op in &plan.delete {
            check_baseline(root, &baseline, &op.path, false)?;
        }
        for op in &plan.restore {
            check_baseline(root, &baseline, &op.path, true)?;
        }
        // The dependency closure is revalidated under the mutation lock
        // (finding 13): a racer that deleted or edited the adapter entry
        // or a qualified binding after the preflight cannot win.
        check_dependency_closure(root, &destination_record, authority)?;
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
    // The exact pre-transaction public manifest, pinned before anything
    // moves (findings 10 and 11): recovery restores these bytes when the
    // rollback journal is abandoned.
    let before_manifest = fs::read_file(root, super::custody::MANIFEST_LOGICAL)
        .map_err(|_| invalid("restore-manifest"))?;
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
        before_manifest_digest: match &before_manifest {
            Some(manifest) => State::known(history::write_object(root, manifest)?),
            None => State::Absent,
        },
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
    // Retain the reviewed rollback intent before any mutation (finding 5):
    // recovery anchors a rollback journal's pending moves to these exact
    // reviewed restore/delete bounds.
    history::write_object(
        root,
        &serde_json::to_vec_pretty(plan).expect("typed rollback plan"),
    )?;
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
    // The governed manifest move is part of the transaction (finding 11):
    // recovery converges it from the pinned before/after evidence.
    pending.push(Pending {
        path: super::custody::MANIFEST_LOGICAL.to_owned(),
        op: PendingOp::Publish,
    });
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
    // The retained governed manifest of the destination generation joins
    // the restoration BEFORE the custody pointer moves (finding 11): the
    // public ownership statement must describe the restored bytes at every
    // instant the committed pointer names them, and the retained bytes are
    // re-verified against the plan's manifest pin at execution. A crash
    // after the manifest restore but before the pointer commit is a
    // completable journal; a crash before the manifest restore converges
    // back to the committed state. The restore goes through the confined
    // staging capability (finding 9), never a raw truncate-and-rename.
    let retained = super::custody::retained_manifest(root, &plan.destination_generation_digest)?;
    if super::digest(&retained) != plan.manifest_digest {
        transaction.conflict = State::known(super::wire::Conflict {
            path: super::custody::MANIFEST_LOGICAL.to_owned(),
            reason: "manifest-restore".to_owned(),
        });
        let _ = history::write_transaction(root, &transaction);
        return Err(invalid("restore-manifest"));
    }
    if super::custody::restore_manifest_bytes(root, &retained).is_err() {
        transaction.conflict = State::known(super::wire::Conflict {
            path: super::custody::MANIFEST_LOGICAL.to_owned(),
            reason: "manifest-restore".to_owned(),
        });
        let _ = history::write_transaction(root, &transaction);
        return Err(invalid("restore-manifest"));
    }
    history::write_current(root, &plan.destination_generation_digest)?;
    transaction.state = TransactionState::Committed;
    match history::write_transaction(root, &transaction) {
        Ok(()) => Ok(()),
        Err(error) => Err(error),
    }
}

/// The confirmed resolve intent, independent of journal transport metadata.
/// The writer assigns the attempt id and advances the phase only after
/// confirmation; recovery must rederive this same digest from either emitted
/// phase without needing the caller's preview or the optional audit object.
pub(crate) fn resolve_digest(transaction: &Transaction) -> String {
    let mut intent = transaction.clone();
    intent.transaction_id.clear();
    intent.state = TransactionState::Prepared;
    intent.conflict = State::Absent;
    plan_digest(&intent)
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
    // The record is trusted only after the content-bound custody checks
    // and the public-manifest bijection (finding 4).
    let record = match history::current_record(root) {
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
    transaction.plan_digest = State::known(resolve_digest(&transaction));
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
    // Retain the confirmed transaction document as an audit object (r6,
    // codex F2): the exact `prepared` shape the confirmation covered.
    // Since r6-r7 this object is deliberately NOT recovery's anchor — its
    // content address is a deterministic function of the journal itself,
    // so a journal forger could always mint it; recovery instead converges
    // a resolve journal behaviorally and never restores a byte on the
    // journal's own word (see recovery.rs).
    let mut intent = transaction.clone();
    intent.state = TransactionState::Prepared;
    if let Err(error) = history::write_object(
        root,
        &serde_json::to_vec_pretty(&intent).expect("typed intent"),
    ) {
        return error;
    }
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
