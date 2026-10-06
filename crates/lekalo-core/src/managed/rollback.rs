//! Exact recorded restoration: rollback to a retained generation and
//! resolve of drifted generated paths (issue #41).
//!
//! Restoration consumes retained exact bytes through the same path and
//! before-state transaction checks as apply. Model and custom sources are
//! never restored, and an old adapter is never executed.

use super::diagnostic;
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

fn refused(reason: &str) -> DomainResult {
    diagnostic::failure(ROLLBACK_REFUSED, &detail(reason))
}

fn drift(path: &str, reason: &str) -> DomainResult {
    diagnostic::failure(GENERATED_DRIFT, &[("path", path), ("reason", reason)])
}

/// Read the exact bytes of one project-relative path (files only).
fn read_project_file(root: &std::path::Path, relative: &str) -> std::io::Result<Option<Vec<u8>>> {
    let path = root.join(relative);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not-a-file",
        ));
    }
    std::fs::read(&path).map(Some)
}

/// Build the exact rollback plan for the retained destination generation.
pub fn build(root: &std::path::Path, destination: &str) -> Result<RollbackPlan, DomainResult> {
    let current = match history::current(root)? {
        Some(current) => current,
        None => return Err(refused("no-generation")),
    };
    if current == destination {
        return Err(invalid("already-current"));
    }
    let (destination_record, manifest_digest) = history::record_with_custody(root, destination)?;
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
        manifest_digest,
        plan_digest: String::new(),
    };
    plan.plan_digest = plan_digest(&plan);
    Ok(plan)
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
    // The current generation must still match the reviewed before-state.
    let current = match history::current(root) {
        Ok(current) => current,
        Err(error) => return error,
    };
    let expected = plan
        .current_generation
        .value()
        .map(|digest| digest.generation_digest.clone());
    if current != expected {
        return diagnostic::failure(PLAN_STALE, &detail("current-generation"));
    }
    if let Err(error) = history::record(root, &plan.destination_generation_digest) {
        return error;
    }
    // Deletion targets must match their recorded bytes exactly; a missing
    // file is already gone, an edited file blocks the rollback.
    for op in &plan.delete {
        match read_project_file(root, &op.path) {
            Ok(Some(bytes)) => {
                if super::digest(&bytes) != op.digest {
                    return drift(&op.path, "edited");
                }
            }
            Ok(None) => {}
            Err(_) => return drift(&op.path, "unreadable"),
        }
    }
    // Every restored byte is re-verified from custody before publishing.
    let mut bytes = Vec::new();
    for op in &plan.restore {
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
    let guard = match history::lock(root) {
        Ok(guard) => guard,
        Err(error) => return error,
    };
    let result = staged(root, &plan, &bytes);
    drop(guard);
    if result.status() == crate::result::Status::Valid {
        super::render(&RestoreReceipt {
            action: "rollback",
            dry_run: false,
            plan_digest: plan.plan_digest.clone(),
            generation_digest: plan.destination_generation_digest.clone(),
            restored: plan.restore.iter().map(|op| op.path.clone()).collect(),
            deleted: plan.delete.iter().map(|op| op.path.clone()).collect(),
        })
    } else {
        result
    }
}

fn staged(
    root: &std::path::Path,
    plan: &RollbackPlan,
    bytes: &[(&RestoreOp, Vec<u8>)],
) -> DomainResult {
    let transaction_id = match history::transaction_id(root, &plan.plan_digest) {
        Ok(id) => id,
        Err(error) => return error,
    };
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
    if let Err(error) = history::write_transaction(root, &transaction) {
        return error;
    }
    for (op, bytes) in bytes {
        let path = root.join(&op.path);
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                transaction.conflict = State::known(super::wire::Conflict {
                    path: op.path.clone(),
                    reason: "restore".to_owned(),
                });
                let _ = history::write_transaction(root, &transaction);
                return invalid("restore-mkdir");
            }
        }
        if std::fs::write(&path, bytes).is_err() {
            transaction.conflict = State::known(super::wire::Conflict {
                path: op.path.clone(),
                reason: "restore".to_owned(),
            });
            let _ = history::write_transaction(root, &transaction);
            return invalid("restore-write");
        }
    }
    for op in &plan.delete {
        let path = root.join(&op.path);
        if let Err(error) = std::fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                transaction.conflict = State::known(super::wire::Conflict {
                    path: op.path.clone(),
                    reason: "delete".to_owned(),
                });
                let _ = history::write_transaction(root, &transaction);
                return invalid("restore-delete");
            }
        }
    }
    if let Err(error) = history::write_current(root, &plan.destination_generation_digest) {
        return error;
    }
    transaction.state = TransactionState::Committed;
    match history::write_transaction(root, &transaction) {
        // The caller renders the real receipt; this payload is discarded.
        Ok(()) => DomainResult::receipt("{}".to_owned(), "committed".to_owned()),
        Err(error) => error,
    }
}

/// Build and execute (or preview) the resolve of one drifted generated
/// path: exact recorded bytes, same transaction checks, no new baseline.
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
    match read_project_file(root, path) {
        Ok(Some(live)) => {
            if super::digest(&live) == output.digest {
                return invalid("not-drifted");
            }
        }
        Ok(None) => {}
        Err(_) => return drift(path, "unreadable"),
    }
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
    let guard = match history::lock(root) {
        Ok(guard) => guard,
        Err(error) => return error,
    };
    transaction.transaction_id = match history::transaction_id(root, &expected) {
        Ok(id) => id,
        Err(error) => return error,
    };
    let result = staged_resolve(root, &transaction, &bytes);
    drop(guard);
    if result.status() == crate::result::Status::Valid {
        super::render(&RestoreReceipt {
            action: "resolve",
            dry_run: false,
            plan_digest: expected,
            generation_digest: current,
            restored: vec![path.to_owned()],
            deleted: Vec::new(),
        })
    } else {
        result
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
    let path = root.join(&transaction.affected_files[0]);
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return invalid("restore-mkdir");
        }
    }
    if std::fs::write(&path, bytes).is_err() {
        return invalid("restore-write");
    }
    restoring.state = TransactionState::Committed;
    match history::write_transaction(root, &restoring) {
        // The caller renders the real receipt; this payload is discarded.
        Ok(()) => DomainResult::receipt("{}".to_owned(), "resolved".to_owned()),
        Err(error) => error,
    }
}
