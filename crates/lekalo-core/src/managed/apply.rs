//! The confirmed generation apply: staged publication with whole-transaction
//! recovery (issue #41).
//!
//! Apply rechecks every reviewed pin against the live project before any
//! byte moves — the plan digest, the coverage pin, the ownership roots, the
//! input closure, and the recorded generated before-state — then journals
//! each phase, publishes inside one recoverable transaction, and records
//! the committed generation. Dry runs touch nothing.

use super::canonical::{render_inventory, Rendered};
use super::diagnostic;
use super::history;
use super::plan::{preflight, require_confirmation, BeforeState};
use super::wire::{
    GenerationDigest, GenerationPlan, GenerationRecord, InputPin, OutputAction, Ownership, Pending,
    PendingOp, RecordOutput, State, Transaction, TransactionAction, TransactionState,
};
use super::{
    detail, parse_coverage, verify_plan, DomainResult, GENERATED_DRIFT, INPUT_INVALID, PLAN_STALE,
};
use crate::result::Status;
use std::collections::{BTreeMap, BTreeSet};

/// The apply receipt: what the confirmed transaction did (or would do).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReceipt {
    pub action: &'static str,
    pub dry_run: bool,
    pub plan_digest: String,
    pub generation_digest: String,
    pub created: Vec<String>,
    pub replaced: Vec<String>,
    pub deleted: Vec<String>,
    pub unchanged: Vec<String>,
}

fn invalid(reason: &str) -> DomainResult {
    diagnostic::failure(INPUT_INVALID, &detail(reason))
}

fn drift(path: &str, reason: &str) -> DomainResult {
    diagnostic::failure(GENERATED_DRIFT, &[("path", path), ("reason", reason)])
}

fn stale(path: &str, reason: &str) -> DomainResult {
    diagnostic::failure(PLAN_STALE, &[("path", path), ("reason", reason)])
}

/// Read the exact bytes of one project-relative path (files only; symlinks
/// and special files fail closed).
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

/// Collect actual input digests for the plan closure.
fn inputs_actual(
    root: &std::path::Path,
    closure: &[InputPin],
) -> Result<BTreeMap<String, String>, DomainResult> {
    let mut actual = BTreeMap::new();
    for input in closure {
        match read_project_file(root, &input.path) {
            Ok(Some(bytes)) => {
                actual.insert(input.path.clone(), super::digest(&bytes));
            }
            Ok(None) => {}
            Err(_) => return Err(stale(&input.path, "unreadable")),
        }
    }
    Ok(actual)
}

/// Collect the recorded ownership and the actual digests of every recorded
/// generated path.
fn before_state(root: &std::path::Path) -> Result<BeforeState, DomainResult> {
    let recorded: BTreeMap<String, RecordOutput> = match history::current(root) {
        Ok(Some(current)) => history::record(root, &current)?
            .outputs
            .into_iter()
            .map(|o| (o.path.clone(), o))
            .collect(),
        Ok(None) => BTreeMap::new(),
        Err(error) => return Err(error),
    };
    let mut actual = BTreeMap::new();
    for path in recorded.keys() {
        match read_project_file(root, path) {
            Ok(Some(bytes)) => {
                actual.insert(path.clone(), super::digest(&bytes));
            }
            Ok(None) => {}
            Err(_) => return Err(drift(path, "unreadable")),
        }
    }
    let map: BTreeMap<String, super::plan::Recorded> = recorded
        .iter()
        .map(|(path, output)| {
            (
                path.clone(),
                super::plan::Recorded {
                    digest: output.digest.clone(),
                    role: output.role,
                    owner_module: output.owner_module.clone(),
                    owner_target: output.owner_target.clone(),
                    object_ref: output.before_ref.value().cloned(),
                },
            )
        })
        .collect();
    Ok(BeforeState {
        recorded: map,
        actual,
    })
}

/// The generated-root coordinate map of one ownership document.
fn generated_roots(ownership: &Ownership) -> BTreeMap<(String, String), String> {
    ownership
        .modules
        .iter()
        .flat_map(|module| {
            module.targets.iter().map(move |target| {
                (
                    (module.module.clone(), target.target.clone()),
                    target.generated_root.clone(),
                )
            })
        })
        .collect()
}

/// The ownership assignment of the plan's single module, if managed.
fn assignment<'a>(
    ownership: &'a Ownership,
    module: &str,
) -> Option<&'a super::wire::ModuleAssignment> {
    ownership
        .modules
        .iter()
        .find(|candidate| candidate.module == module)
}

fn target<'a>(
    parent: &'a super::wire::ModuleAssignment,
    target: &str,
) -> Option<&'a super::wire::TargetAssignment> {
    parent
        .targets
        .iter()
        .find(|candidate| candidate.target == target)
}

/// Write one project file, creating bounded parent directories.
fn publish_file(root: &std::path::Path, relative: &str, bytes: &[u8]) -> Result<(), DomainResult> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| invalid("publish-mkdir"))?;
    }
    std::fs::write(&path, bytes).map_err(|_| invalid("publish-write"))
}

/// Execute (or preview) one confirmed generation apply.
///
/// `coverage_bytes` is the pinned adapter evidence the plan was derived
/// from; its canonical digest must equal the plan's `coverageDigest`. The
/// ownership document rechecks every output against its exact declared
/// generated root before publication.
pub fn apply(
    root: &std::path::Path,
    plan_bytes: &[u8],
    coverage_bytes: &[u8],
    ownership: &Ownership,
    confirm: Option<&str>,
    dry_run: bool,
) -> DomainResult {
    let plan = match verify_plan(plan_bytes) {
        Ok(plan) => plan,
        Err(error) => return error,
    };
    if plan.modules.len() != 1 || plan.targets.len() != 1 {
        return invalid("plan-scope");
    }
    if !dry_run {
        if let Err(error) = require_confirmation(confirm, &plan.plan_digest) {
            return error;
        }
    }
    let coverage = match parse_coverage(coverage_bytes) {
        Ok(coverage) => coverage,
        Err(error) => return error,
    };
    if super::hash(&coverage) != plan.coverage_digest {
        return stale("coverage", "digest");
    }
    let module = match assignment(ownership, &plan.modules[0]) {
        Some(module) => module,
        None => return stale("module", "unowned"),
    };
    let target = match target(module, &plan.targets[0]) {
        Some(target) => target,
        None => return stale("target", "unowned"),
    };
    let admission = match super::coverage::admit(module, target, &coverage) {
        Ok(admission) => admission,
        Err(error) => return error,
    };
    // The deterministic inventory derived here must match the reviewed plan
    // exactly: same paths, same actions, same after digests.
    let inventory = match render_inventory(&coverage, &admission.generated_root) {
        Ok(inventory) => inventory,
        Err(error) => return error,
    };
    if let Err(error) = inventory_matches(&plan, &inventory) {
        return error;
    }
    // Whole-custody gates: unresolved journals block any further mutation.
    match history::unresolved(root) {
        Ok(unresolved) => {
            if let Some(transaction) = unresolved.first() {
                return diagnostic::failure(
                    super::RECOVERY_REQUIRED,
                    &[
                        ("pin", transaction.transaction_id.as_str()),
                        ("reason", "unresolved-transaction"),
                    ],
                );
            }
        }
        Err(error) => return error,
    }
    // Preflight: every reviewed pin is rechecked against the live project.
    let inputs = match inputs_actual(root, &plan.input_closure) {
        Ok(inputs) => inputs,
        Err(error) => return error,
    };
    let before = match before_state(root) {
        Ok(before) => before,
        Err(error) => return error,
    };
    let roots = generated_roots(ownership);
    if let Err(error) = preflight(&plan, &roots, &inputs, &before) {
        return error;
    }
    let mut created = Vec::new();
    let mut replaced = Vec::new();
    let mut deleted = Vec::new();
    let mut unchanged = Vec::new();
    for output in &plan.outputs {
        match output.action {
            OutputAction::Create => created.push(output.path.clone()),
            OutputAction::Replace => replaced.push(output.path.clone()),
            OutputAction::Delete => deleted.push(output.path.clone()),
            OutputAction::Unchanged => unchanged.push(output.path.clone()),
        }
    }
    // The generation identity over the closure + complete inventory only.
    let generation = generation_identity(&plan);
    if dry_run {
        return super::render(&ApplyReceipt {
            action: "apply",
            dry_run: true,
            plan_digest: plan.plan_digest.clone(),
            generation_digest: generation,
            created,
            replaced,
            deleted,
            unchanged,
        });
    }
    let guard = match history::lock(root) {
        Ok(guard) => guard,
        Err(error) => return error,
    };
    let result = staged_apply(root, &plan, &inventory, &before, generation.clone());
    drop(guard);
    if result.status() == Status::Valid {
        super::render(&ApplyReceipt {
            action: "apply",
            dry_run: false,
            plan_digest: plan.plan_digest.clone(),
            generation_digest: generation,
            created,
            replaced,
            deleted,
            unchanged,
        })
    } else {
        result
    }
}

/// The stable generation digest over the pinned input closure and the
/// complete output inventory only.
fn generation_identity(plan: &GenerationPlan) -> String {
    super::inventory_digest(
        plan.input_closure.iter(),
        plan.outputs.iter().map(|output| {
            (
                output.path.as_str(),
                &output.role,
                output.owner_module.as_str(),
                output.owner_target.as_str(),
                output
                    .after_digest
                    .value()
                    .map(|digest| digest.as_str())
                    .unwrap_or_default(),
            )
        }),
    )
}

/// The reviewed plan must be the exact deterministic inventory.
fn inventory_matches(plan: &GenerationPlan, inventory: &[Rendered]) -> Result<(), DomainResult> {
    let mut produced: std::collections::BTreeMap<&str, String> = std::collections::BTreeMap::new();
    for artifact in inventory {
        produced.insert(artifact.path.as_str(), super::digest(&artifact.bytes));
    }
    let mut planned: BTreeSet<&str> = BTreeSet::new();
    for output in &plan.outputs {
        match output.action {
            OutputAction::Delete => {
                if produced.contains_key(output.path.as_str()) {
                    return Err(invalid("delete-produced"));
                }
            }
            OutputAction::Create | OutputAction::Replace | OutputAction::Unchanged => {
                let after = match output.after_digest.value() {
                    Some(after) => after,
                    None => return Err(invalid("output-action")),
                };
                match produced.get(output.path.as_str()) {
                    Some(digest) if digest == after => {}
                    _ => return Err(stale(&output.path, "inventory")),
                }
                planned.insert(output.path.as_str());
            }
        }
    }
    let rendered: BTreeSet<&str> = produced.keys().copied().collect();
    if rendered != planned {
        return Err(super::diagnostic::failure(
            super::ARTIFACT_COVERAGE_INCOMPLETE,
            &detail("inventory"),
        ));
    }
    Ok(())
}

/// The staged transaction: journal prepared, snapshots, publishing,
/// committed. Any failure leaves the journal unresolved, which blocks
/// every further mutation until reconciliation.
fn staged_apply(
    root: &std::path::Path,
    plan: &GenerationPlan,
    inventory: &[Rendered],
    before: &BeforeState,
    generation: String,
) -> DomainResult {
    let previous = match history::current(root) {
        Ok(current) => current,
        Err(error) => return error,
    };
    let transaction_id = match history::transaction_id(root, &plan.plan_digest) {
        Ok(id) => id,
        Err(error) => return error,
    };
    let mut transaction = Transaction {
        schema_version: super::header("managed-transaction").0,
        identity: super::header("managed-transaction").1,
        transaction_id: transaction_id.clone(),
        state: TransactionState::Prepared,
        action: TransactionAction::Apply,
        plan_digest: State::known(plan.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: Vec::new(),
        affected_files: plan.outputs.iter().map(|o| o.path.clone()).collect(),
        conflict: State::Absent,
    };
    transaction.affected_files.sort();
    if let Err(error) = history::write_transaction(root, &transaction) {
        return error;
    }
    // Content-addressed snapshots: exact before bytes of every replaced or
    // deleted file, exact after bytes of every published file.
    let bytes_of: BTreeMap<&str, &[u8]> = inventory
        .iter()
        .map(|artifact| (artifact.path.as_str(), artifact.bytes.as_slice()))
        .collect();
    let mut pending = Vec::new();
    for output in &plan.outputs {
        match output.action {
            OutputAction::Delete => pending.push(Pending {
                path: output.path.clone(),
                op: PendingOp::Delete,
            }),
            OutputAction::Create | OutputAction::Replace | OutputAction::Unchanged => {
                pending.push(Pending {
                    path: output.path.clone(),
                    op: PendingOp::Publish,
                });
            }
        }
    }
    pending.sort_by(|left, right| (&left.path, &left.op).cmp(&(&right.path, &right.op)));
    transaction.pending = pending;
    transaction.state = TransactionState::Publishing;
    if let Err(error) = history::write_transaction(root, &transaction) {
        return error;
    }
    // Build the committed record while every before byte is still on disk.
    let mut record_outputs = Vec::new();
    for output in &plan.outputs {
        if output.action == OutputAction::Delete {
            continue;
        }
        let after = match output.after_digest.value() {
            Some(after) => after.clone(),
            None => continue,
        };
        let bytes = match bytes_of.get(output.path.as_str()) {
            Some(bytes) => *bytes,
            None => return invalid("inventory"),
        };
        let after_ref = match history::write_object(root, bytes) {
            Ok(reference) => State::known(reference),
            Err(error) => return error,
        };
        let before_ref = match output.action {
            OutputAction::Create => State::Absent,
            _ => {
                match before
                    .recorded
                    .get(&output.path)
                    .and_then(|recorded| recorded.object_ref.clone())
                {
                    Some(reference) => State::known(reference),
                    None => match read_project_file(root, &output.path) {
                        // First-generation replaces may lack a retained
                        // object; snapshot the live bytes so rollback stays
                        // exact.
                        Ok(Some(live)) => match history::write_object(root, &live) {
                            Ok(reference) => State::known(reference),
                            Err(error) => return error,
                        },
                        Ok(None) => State::Absent,
                        Err(_) => return drift(&output.path, "snapshot"),
                    },
                }
            }
        };
        record_outputs.push(RecordOutput {
            path: output.path.clone(),
            role: output.role,
            owner_module: output.owner_module.clone(),
            owner_target: output.owner_target.clone(),
            digest: after,
            before_ref,
            after_ref,
        });
    }
    let ownership_digest = plan.coverage_digest.clone();
    let record = GenerationRecord {
        schema_version: super::header("managed-generation-record").0,
        identity: super::header("managed-generation-record").1,
        plan_digest: plan.plan_digest.clone(),
        generation_digest: generation.clone(),
        previous_generation: previous
            .map(|digest| {
                State::known(GenerationDigest {
                    generation_digest: digest,
                })
            })
            .unwrap_or(State::Absent),
        input_closure: plan.input_closure.clone(),
        outputs: record_outputs,
        ownership_digest,
        lock_pin: State::Absent,
    };
    if super::generation_digest(&record) != generation {
        return invalid("generation-identity");
    }
    let after_manifest = match history::write_record(root, &record) {
        Ok(custody) => custody,
        Err(error) => return error,
    };
    transaction.after_manifest_digest = State::known(after_manifest.clone());
    if let Err(error) = history::write_transaction(root, &transaction) {
        return error;
    }
    // Publication: create/replace first, then deletions.
    for output in &plan.outputs {
        match output.action {
            OutputAction::Create | OutputAction::Replace | OutputAction::Unchanged => {
                let bytes = bytes_of.get(output.path.as_str()).copied().unwrap_or(&[]);
                if publish_file(root, &output.path, bytes).is_err() {
                    transaction.conflict = State::known(super::wire::Conflict {
                        path: output.path.clone(),
                        reason: "publish".to_owned(),
                    });
                    let _ = history::write_transaction(root, &transaction);
                    return invalid("publish-write");
                }
            }
            OutputAction::Delete => {
                let path = root.join(&output.path);
                if let Err(error) = std::fs::remove_file(&path) {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        transaction.conflict = State::known(super::wire::Conflict {
                            path: output.path.clone(),
                            reason: "delete".to_owned(),
                        });
                        let _ = history::write_transaction(root, &transaction);
                        return invalid("publish-delete");
                    }
                }
            }
        }
    }
    // Pointer commit completes the generation.
    if let Err(error) = history::write_current(root, &generation) {
        return error;
    }
    transaction.state = TransactionState::Committed;
    match history::write_transaction(root, &transaction) {
        // The caller renders the real receipt; this payload is discarded.
        Ok(()) => DomainResult::receipt("{}".to_owned(), "committed".to_owned()),
        Err(error) => error,
    }
}
