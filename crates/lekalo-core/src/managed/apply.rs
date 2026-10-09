//! The confirmed generation apply: staged publication with whole-transaction
//! recovery (issue #41).
//!
//! Apply rechecks every reviewed pin against the live project before any
//! byte moves — the plan digest, the coverage pin, the ownership roots, the
//! input closure, and the recorded generated before-state — then journals
//! each phase, publishes inside one recoverable transaction, and records
//! the committed generation. Dry runs touch nothing. Every filesystem move
//! runs through the confined capability, the unresolved-journal barrier is
//! rechecked under the mutation lock, deletes are custody-checked at the
//! moment of consumption, and the committed record merges every unaffected
//! managed module so scoped applies never drop preserved outputs.

use super::canonical::{render_inventory, Rendered};
use super::diagnostic;
use super::fs;
use super::history;
use super::plan::{preflight, require_confirmation, BeforeState};
use super::wire::{
    GenerationDigest, GenerationPlan, GenerationRecord, InputPin, OutputAction, Ownership, Pending,
    PendingOp, RecordOutput, State, Transaction, TransactionAction, TransactionState,
};
use super::{
    detail, hash, parse_coverage, verify_plan, DomainResult, GENERATED_DRIFT, INPUT_INVALID,
    PLAN_STALE,
};
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

/// Collect actual input digests for the plan closure.
fn inputs_actual(
    root: &std::path::Path,
    closure: &[InputPin],
) -> Result<BTreeMap<String, String>, DomainResult> {
    let mut actual = BTreeMap::new();
    for input in closure {
        match fs::read_file(root, &input.path) {
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
fn before_state(
    root: &std::path::Path,
    plan: Option<&GenerationPlan>,
) -> Result<BeforeState, DomainResult> {
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
        match fs::read_file(root, path) {
            Ok(Some(bytes)) => {
                actual.insert(path.clone(), super::digest(&bytes));
            }
            Ok(None) => {}
            Err(_) => return Err(drift(path, "unreadable")),
        }
    }
    // Unclaimed files at a would-be create path are never invisible: every
    // plan output coordinate is statted directly.
    if let Some(plan) = plan {
        for output in &plan.outputs {
            if actual.contains_key(&output.path) {
                continue;
            }
            match fs::read_file(root, &output.path) {
                Ok(Some(bytes)) => {
                    actual.insert(output.path.clone(), super::digest(&bytes));
                }
                Ok(None) => {}
                Err(_) => return Err(drift(&output.path, "unreadable")),
            }
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
    authority: Option<&super::authority::Authority>,
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
    if let Err(error) = history::assert_reconciled(root) {
        return error;
    }
    // Preflight: every reviewed pin is rechecked against the live project.
    let inputs = match inputs_actual(root, &plan.input_closure) {
        Ok(inputs) => inputs,
        Err(error) => return error,
    };
    let before = match before_state(root, Some(&plan)) {
        Ok(before) => before,
        Err(error) => return error,
    };
    let roots = super::plan::generated_roots(ownership);
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
    if dry_run {
        return super::render(&ApplyReceipt {
            action: "apply",
            dry_run: true,
            plan_digest: plan.plan_digest.clone(),
            generation_digest: generation_identity(&plan, &BTreeMap::new(), &plan.input_closure),
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
    // Recheck under the mutation lock: the barrier, the inputs, and the
    // before-state are revalidated now that this caller owns mutation, so
    // a racer that moved a pin between preflight and lock cannot win.
    let result = (|| -> Result<String, DomainResult> {
        history::assert_reconciled(root)?;
        let inputs = inputs_actual(root, &plan.input_closure)?;
        let before = before_state(root, Some(&plan))?;
        let roots = super::plan::generated_roots(ownership);
        preflight(&plan, &roots, &inputs, &before)?;
        staged_apply(
            root,
            &plan,
            &inventory,
            ownership,
            &before,
            authority,
            &coverage,
            &admission.generated_root,
        )
    })();
    drop(guard);
    match result {
        Ok(committed) => super::render(&ApplyReceipt {
            action: "apply",
            dry_run: false,
            plan_digest: plan.plan_digest.clone(),
            generation_digest: committed,
            created,
            replaced,
            deleted,
            unchanged,
        }),
        Err(error) => error,
    }
}

/// The stable generation digest over the pinned input closure and the
/// complete committed output inventory: the plan's own outputs plus every
/// preserved output of unaffected managed modules.
fn generation_identity(
    plan: &GenerationPlan,
    preserved: &BTreeMap<String, RecordOutput>,
    closure: &[InputPin],
) -> String {
    let plan_outputs = plan.outputs.iter().filter_map(|output| {
        if output.action == OutputAction::Delete {
            return None;
        }
        Some((
            output.path.as_str(),
            &output.role,
            output.owner_module.as_str(),
            output.owner_target.as_str(),
            output
                .after_digest
                .value()
                .map(|digest| digest.as_str())
                .unwrap_or_default(),
        ))
    });
    let preserved_outputs = preserved.values().map(|output| {
        (
            output.path.as_str(),
            &output.role,
            output.owner_module.as_str(),
            output.owner_target.as_str(),
            output.digest.as_str(),
        )
    });
    super::inventory_digest(closure.iter(), plan_outputs.chain(preserved_outputs))
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
///
/// The committed record is the whole project custody: every output of an
/// unaffected managed module is preserved verbatim and every preserved
/// input pin rides the closure, so a scoped apply never orphans another
/// module's files (one file, one owner, at every generation).
#[allow(clippy::too_many_arguments)]
fn staged_apply(
    root: &std::path::Path,
    plan: &GenerationPlan,
    inventory: &[Rendered],
    ownership: &Ownership,
    before: &BeforeState,
    authority: Option<&super::authority::Authority>,
    coverage: &super::wire::Coverage,
    generated_root: &str,
) -> Result<String, DomainResult> {
    let previous = history::current(root)?;
    let previous_record = match &previous {
        Some(digest) => Some(history::record(root, digest)?),
        None => None,
    };
    // Preserved custody: outputs owned by any other module stay in the
    // committed record; the plan never touches their bytes.
    let mut preserved: BTreeMap<String, RecordOutput> = BTreeMap::new();
    if let Some(record) = &previous_record {
        for output in &record.outputs {
            if plan
                .outputs
                .iter()
                .any(|planned| planned.path == output.path)
            {
                continue;
            }
            preserved.insert(output.path.clone(), output.clone());
        }
    }
    // Input provenance: the committed closure unions the previous closure
    // with the plan's; the plan's own pins win on a shared path. The
    // canonical generation authority rides the closure as derived pins
    // (finding 8): the exact canonical Model, typed IR, and lock revision
    // this generation was admitted under are custody evidence, so a
    // Model/IR/lock move always changes the committed closure.
    let mut closure: BTreeMap<String, InputPin> = BTreeMap::new();
    if let Some(record) = &previous_record {
        for pin in &record.input_closure {
            if pin.path.starts_with(super::AUTHORITY_PIN_PREFIX) {
                continue;
            }
            closure.insert(pin.path.clone(), pin.clone());
        }
    }
    for pin in &plan.input_closure {
        closure.insert(pin.path.clone(), pin.clone());
    }
    if let Some(authority) = authority {
        for (name, digest_value) in [
            ("model", &authority.model_digest),
            ("ir", &authority.ir_digest),
            ("lock", &authority.lock_digest),
        ] {
            closure.insert(
                format!("{}{name}", super::AUTHORITY_PIN_PREFIX),
                InputPin {
                    path: format!(".lekalo/managed-authority/{name}"),
                    digest: digest_value.clone(),
                },
            );
        }
    }
    let closure: Vec<InputPin> = closure.into_values().collect();
    let transaction_id = history::transaction_id(root, &plan.plan_digest)?;
    let mut transaction = Transaction {
        schema_version: super::header("managed-transaction").0,
        identity: super::header("managed-transaction").1,
        transaction_id: transaction_id.clone(),
        state: TransactionState::Prepared,
        action: TransactionAction::Apply,
        plan_digest: State::known(plan.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: String::new(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: Vec::new(),
        affected_files: plan.outputs.iter().map(|o| o.path.clone()).collect(),
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    transaction.affected_files.sort();
    history::write_transaction(root, &transaction)?;
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
    history::write_transaction(root, &transaction)?;
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
            None => return Err(invalid("inventory")),
        };
        let after_ref = State::known(history::write_object(root, bytes)?);
        let before_ref = match output.action {
            OutputAction::Create => State::Absent,
            _ => {
                match before
                    .recorded
                    .get(&output.path)
                    .and_then(|recorded| recorded.object_ref.clone())
                {
                    Some(reference) => State::known(reference),
                    None => match fs::read_file(root, &output.path) {
                        // First-generation replaces may lack a retained
                        // object; snapshot the live bytes so rollback stays
                        // exact.
                        Ok(Some(live)) => State::known(history::write_object(root, &live)?),
                        Ok(None) => State::Absent,
                        Err(_) => return Err(drift(&output.path, "snapshot")),
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
    for output in preserved.values() {
        record_outputs.push(output.clone());
    }
    record_outputs.sort_by(|left, right| left.path.cmp(&right.path));
    if super::has_duplicates(record_outputs.iter().map(|o| &o.path)) {
        return Err(invalid("record-order"));
    }
    // The ownership pin is the exact ownership configuration this commit
    // was admitted under, never the coverage evidence; the lock pin is
    // the exact lock revision the authority derived its semantics under
    // (findings 3 and 8). Both ride the authenticated custody identity.
    let lock_pin = authority
        .map(|authority| State::known(authority.lock_digest.clone()))
        .unwrap_or(State::Absent);
    let record = GenerationRecord {
        schema_version: super::header("managed-generation-record").0,
        identity: super::header("managed-generation-record").1,
        plan_digest: plan.plan_digest.clone(),
        generation_digest: String::new(),
        previous_generation: previous
            .map(|digest| {
                State::known(GenerationDigest {
                    generation_digest: digest,
                })
            })
            .unwrap_or(State::Absent),
        input_closure: closure.clone(),
        outputs: record_outputs,
        ownership_digest: hash(ownership),
        lock_pin: lock_pin.clone(),
        record_digest: String::new(),
    };
    let generation = super::generation_digest(&record);
    let mut record = record;
    record.generation_digest = generation.clone();
    if super::generation_digest(&record) != generation {
        return Err(invalid("generation-identity"));
    }
    if !super::pin(lock_pin.value().map(|lock| lock.as_str()).unwrap_or("")) {
        // The lock revision is mandatory custody evidence (finding 8);
        // the caller must derive the generation authority first.
        return Err(invalid("generation-identity"));
    }
    record.record_digest = super::record_identity(&record);
    let after_manifest = history::write_record(root, &record)?;
    transaction.generation_digest = State::known(GenerationDigest {
        generation_digest: generation.clone(),
    });
    transaction.after_manifest_digest = State::known(after_manifest.clone());
    history::write_transaction(root, &transaction)?;
    // Publication: create/replace first, then deletions. Deletes are
    // custody-checked at the moment of consumption: the live bytes must
    // still be the reviewed before bytes under the lock.
    for output in &plan.outputs {
        match output.action {
            OutputAction::Create | OutputAction::Replace | OutputAction::Unchanged => {
                let bytes = bytes_of.get(output.path.as_str()).copied().unwrap_or(&[]);
                if fs::write_file(root, &output.path, bytes).is_err() {
                    transaction.conflict = State::known(super::wire::Conflict {
                        path: output.path.clone(),
                        reason: "publish".to_owned(),
                    });
                    let _ = history::write_transaction(root, &transaction);
                    return Err(invalid("publish-write"));
                }
            }
            OutputAction::Delete => {
                let live = match fs::read_file(root, &output.path) {
                    Ok(live) => live,
                    Err(_) => {
                        transaction.conflict = State::known(super::wire::Conflict {
                            path: output.path.clone(),
                            reason: "delete".to_owned(),
                        });
                        let _ = history::write_transaction(root, &transaction);
                        return Err(drift(&output.path, "delete"));
                    }
                };
                let matches = live
                    .map(|bytes| {
                        Some(super::digest(&bytes)) == output.before_digest.value().cloned()
                    })
                    .unwrap_or(false);
                if !matches {
                    transaction.conflict = State::known(super::wire::Conflict {
                        path: output.path.clone(),
                        reason: "delete".to_owned(),
                    });
                    let _ = history::write_transaction(root, &transaction);
                    return Err(drift(&output.path, "edited"));
                }
                match fs::remove_file(root, &output.path) {
                    Ok(_) => {}
                    Err(_) => {
                        transaction.conflict = State::known(super::wire::Conflict {
                            path: output.path.clone(),
                            reason: "delete".to_owned(),
                        });
                        let _ = history::write_transaction(root, &transaction);
                        return Err(invalid("publish-delete"));
                    }
                }
            }
        }
    }
    // Custody publication: the governed ownership manifest of the exact
    // committed bytes (finding E), bound to the derived generation
    // authority. The project authority is mandatory here. Publication is
    // journal-covered: a failure leaves the committed transaction with a
    // conflict the recovery path completes from the retained after
    // evidence (finding 10).
    match authority {
        Some(authority) => {
            if super::custody::publish(
                root,
                plan,
                coverage,
                generated_root,
                authority,
                inventory,
                &record,
            )
            .is_err()
            {
                transaction.conflict = State::known(super::wire::Conflict {
                    path: super::custody::MANIFEST_LOGICAL.to_owned(),
                    reason: "manifest-publish".to_owned(),
                });
                transaction.state = TransactionState::Publishing;
                let _ = history::write_transaction(root, &transaction);
                return Err(invalid("manifest-publish"));
            }
        }
        None => return Err(invalid("generation-identity")),
    }
    // Retain the exact published manifest of this generation (finding 9):
    // the rollback path restores the destination's own governed manifest
    // from this snapshot, so the public ownership statement always
    // describes the restored bytes.
    let manifest_bytes = match std::fs::read(root.join(super::custody::MANIFEST_LOGICAL)) {
        Ok(bytes) => bytes,
        Err(_) => return Err(invalid("manifest-retain")),
    };
    if let Err(error) = super::custody::retain_manifest(root, &generation, &manifest_bytes) {
        transaction.conflict = State::known(super::wire::Conflict {
            path: super::custody::MANIFEST_LOGICAL.to_owned(),
            reason: "manifest-retain".to_owned(),
        });
        let _ = history::write_transaction(root, &transaction);
        return Err(error);
    }
    // Pointer commit completes the generation: the application bytes,
    // the governed manifest, and its retained snapshot are all
    // consistent, so recovery treats the transaction as complete.
    history::write_current(root, &generation)?;
    transaction.state = TransactionState::Committed;
    history::write_transaction(root, &transaction)?;
    Ok(generation)
}
