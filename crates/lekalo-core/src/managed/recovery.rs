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
//! on its face (finding 1, r3 shape, and r4 finding 1: every journal pin
//! binds to the transaction-owned committed lineage, never to whatever
//! object a caller planted in the store):
//!
//! - a journal with pending moves must *anchor*: its generation pin must
//!   sit on the committed lineage and — for apply journals — its plan pin
//!   must equal the anchored record's authenticated `plan_digest`, so a
//!   minted plan object never bounds a deletion (r4 finding 1a); a
//!   rollback journal's retained plan must name the same destination and
//!   run from the committed generation; a resolve journal's generation
//!   pin must be the committed generation. Pending paths outside the
//!   journal's own `affectedFiles` refuse. Unanchorable journals are
//!   `invalid` and never finalized — recovery stays owed;
//! - delete authorization is exact-path: a pending path the committed
//!   generation does not own must be an exact reviewed output of the
//!   journal's retained plan — the root-prefix widening is gone, so an
//!   unclaimed descendant under a generated root is never deletable;
//! - since r6-r7 the retained plan itself must **re-derive** from live,
//!   caller-owned, digest-pinned evidence before any apply journal
//!   anchors (devin F2 / codex F1+F2): the plan closure names the
//!   ownership document whose canonical hash equals the record's
//!   authenticated `ownershipDigest` (so confinement derives from the
//!   caller-declared generated roots, on first publication included —
//!   the derived-prefix proxy that was vacuous on a virgin project is
//!   gone), the plan closure equals the live policy-selected file-pin set,
//!   the record closure equals the writer's exact base/plan/authority
//!   assembly, and the plan's outputs are exactly the deterministic
//!   inventory rendered from the retained coverage under that ownership
//!   and authority — with the action/before-state classification the
//!   apply machinery itself produces. A bound apply additionally
//!   required its execution evidence at publication (codex F4), so its
//!   retained evidence object must exist before the journal anchors;
//! - only journal shapes the machinery itself writes ever anchor
//!   (devin F1c): `publishing` on a resolve journal, `restoring` on an
//!   apply journal, and every other never-emitted state/pending shape is
//!   refused — and a resolve journal pins its own self digest
//!   (the prepared intent, excluding attempt id and phase), so a forged
//!   plan digest never anchors; a resolve journal converges only
//!   behaviorally (devin F1 / codex F3): the confirmed restore already
//!   landed (live == committed) or it never did (live == the pinned
//!   drift) — recovery never restores a byte on a resolve journal's own
//!   word, so a caller-planted "retained intent" object anchors nothing;
//! - the target generation must be anchored to the committed lineage: the
//!   committed generation, one of its retained ancestors (a rollback
//!   destination), or its direct retained successor (an interrupted
//!   apply) — a minted record outside the committed chain is never
//!   adoptable;
//! - every live before-state is validated before the first byte moves: a
//!   path whose live bytes match neither the committed state nor the
//!   anchored intent's before/after state is manual drift that only the
//!   confirmed resolve path may touch — a forged journal can neither
//!   overwrite it nor write partially before refusing; the journal's own
//!   `beforeDigest` pin is caller-authored and is an accepted live state
//!   only for resolve journals, whose confirmation digest covered it
//!   (r4 finding 1b: a forged apply-journal pin must never silently
//!   overwrite drift);
//! - the stale mutation lock is cleared only after the OS-level advisory
//!   lock is acquired, positively proving no live holder (finding 3), and
//!   only through the confined capability;
//! - completion requires the public manifest: recovery never advances the
//!   pointer to a generation whose published manifest is missing or
//!   divergent (finding 10) — such a journal converges to the committed
//!   state instead and the blocked destination keeps the run unresolved;
//!   the pre-transaction manifest restores only from lineage-bound
//!   evidence — the committed generation's retained manifest, the live
//!   bytes' own digest, or (first publication only) a retained object —
//!   never from an arbitrary store pin (r4 finding 1c); a manifest
//!   destination blocked by non-file data refuses the journal before any
//!   byte moves instead of finalizing divergent custody (r4 finding 10);
//! - a first publication interrupted anywhere (including a real killed
//!   process with a prepared journal and empty generation pin) converges
//!   to exact absence: published application bytes are removed, the
//!   pre-transaction manifest absence is restored, and a fresh apply
//!   succeeds (finding 11).

use super::diagnostic;
use super::fs;
use super::history;
use super::wire::{PendingOp, Transaction, TransactionAction, TransactionState};
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
/// journal must sit on the committed lineage — and, since r6 (devin F1 /
/// codex F10), *which* lineage position anchors depends on the journal's
/// action, because the two transaction shapes run in opposite directions:
///
/// - **Apply** (an interrupted generation): the anchored record is the
///   committed generation itself (the apply finished everything but the
///   journal close) or its *direct retained successor* (the record the
///   interrupted apply minted);
/// - **Rollback** (an interrupted restoration): the anchored record is the
///   *destination* — the committed generation itself or any retained
///   **ancestor**, however deep the chain: a mid-chain destination B in
///   A→B→C has its own predecessor A, and the interrupted rollback to it
///   is exactly the shape the old `Some(previous)` arm could never anchor
///   (recovery-unanchored forever: a wedged project no caller action
///   except manual journal surgery could clear).
///
/// A record minted outside the committed chain — however self-consistent
/// its hashes — is never adoptable custody state (findings 1 and 4): the
/// chain link is the anchor a caller cannot borrow without displacing the
/// real lineage.
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
            if record.generation_digest == committed.generation_digest {
                true
            } else {
                match transaction.action {
                    // The direct successor of the committed generation: an
                    // interrupted apply whose record was retained.
                    TransactionAction::Apply => matches!(
                        record.previous_generation.value(),
                        Some(previous) if previous.generation_digest == committed.generation_digest
                    ),
                    // A rollback destination: any retained ancestor of the
                    // committed generation, mid-chain included (r6, devin
                    // F1 / codex F10 — the interrupted-rollback wedge).
                    TransactionAction::Rollback => {
                        is_ancestor(root, &record.generation_digest, committed, 0)?
                    }
                    // A resolve journal names the committed generation
                    // exactly (checked by its own anchor branch).
                    TransactionAction::Resolve => false,
                }
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

/// Whether a journal carries a state/action/pending shape the machinery
/// ever writes (r6-r7, devin F1c): the writers emit exactly —
///
/// - apply: `prepared` with an empty pending set (nothing moved yet), or
///   `publishing` with the complete pending set and its generation pin;
/// - rollback: `restoring` with the complete pending set, generation pin,
///   and the governed manifest move;
/// - resolve: `prepared` or `restoring` with exactly one restore op on
///   exactly one affected path (resolve never touches the manifest).
///
/// Every other combination (`publishing` on a resolve journal, `restoring`
/// on an apply journal, an empty pending set in `publishing`, a resolve
/// journal batching paths) is a shape no writer emits and never anchors —
/// recovery stays owed instead of converging on a forged word.
fn journal_shape_is_emitted(transaction: &Transaction) -> bool {
    let manifest = super::custody::MANIFEST_LOGICAL;
    match transaction.action {
        TransactionAction::Apply => match transaction.state {
            TransactionState::Prepared => transaction.pending.is_empty(),
            TransactionState::Publishing => {
                !transaction.pending.is_empty()
                    && transaction.generation_digest.value().is_some()
                    && transaction
                        .pending
                        .iter()
                        .any(|pending| pending.path == manifest)
            }
            _ => false,
        },
        TransactionAction::Rollback => {
            transaction.state == TransactionState::Restoring
                && !transaction.pending.is_empty()
                && transaction.generation_digest.value().is_some()
                && transaction
                    .pending
                    .iter()
                    .any(|pending| pending.path == manifest)
        }
        TransactionAction::Resolve => {
            matches!(
                transaction.state,
                TransactionState::Prepared | TransactionState::Restoring
            ) && transaction.pending.len() == 1
                && transaction.pending[0].op == PendingOp::Restore
                && transaction.pending[0].path != manifest
                && transaction.affected_files.len() == 1
                && transaction.affected_files[0] == transaction.pending[0].path
        }
    }
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

/// The transaction-owned anchor of one journal (r4 finding 1; r6 codex
/// F1/F2/F4): recovery authorization derives only from evidence the
/// custody machinery itself committed — never from caller-authored
/// journal pins — and, since r6, the anchored evidence must additionally
/// be *qualified* the way the apply machinery itself would have admitted
/// it:
///
/// - **Apply**: the journal's generation pin must sit on the committed
///   lineage (the committed generation or its direct retained successor),
///   the journal's plan pin must equal that record's authenticated
///   `plan_digest`, the retained plan must biject with the record exactly
///   as `staged_apply` assembles it (plan outputs verbatim; every other
///   record output preserved verbatim from the committed generation; every
///   delete committed-owned), every record output the committed generation
///   does not own must be a rendered create inside a generated root the
///   closure-pinned ownership policy declares, both closure sets must equal
///   the machinery's re-derived assembly, and the pending set must be
///   exactly the plan's output set — a fabricated direct successor whose
///   "creates" reach outside every generated root, or a truncated pending
///   set, never grants mutation authority (codex F1/F4).
/// - **Rollback**: the generation pin (the destination, mid-chain
///   ancestors included) must sit on the lineage, the retained rollback
///   plan is rebuilt from the anchored records and the journal's plan pin
///   must name exactly it, and the journal's pending set must be exactly
///   the rebuilt restore+delete set (codex F4: a subset journal can never
///   finalize a partial destination as `completed`).
/// - **Resolve**: the generation pin must be exactly the committed
///   generation and the confirmation digest must rederive from the journal's
///   prepared intent, excluding the attempt id and transient phase. No
///   caller-held preview or retained audit object is needed. Recovery only
///   observes whether restoration already landed or preserves pinned drift
///   for a fresh confirmed retry; it never writes on a resolve journal's word.
///
/// A journal whose pending set is empty moved nothing and abandons
/// trivially without an anchor. Anything else is `recovery-unanchored`:
/// refused invalid, never finalized, recovery stays owed.
enum Anchored {
    Apply {
        record: super::wire::GenerationRecord,
        plan: super::wire::GenerationPlan,
    },
    Rollback {
        record: super::wire::GenerationRecord,
        plan: super::wire::RollbackPlan,
    },
    Resolve,
    Empty,
}

fn unanchored() -> DomainResult {
    invalid("recovery-unanchored")
}

/// The custody the anchored apply ran **from** (r6-r7, codex F1/F2): the
/// base whose outputs the plan's before-digests describe. For a direct
/// successor journal that is the committed generation; for a journal whose
/// anchored record already *is* the committed generation (the pointer
/// moved and only the close was interrupted) it is that record's own
/// previous generation — the state the apply actually replaced. A base of
/// `None` is a first publication.
fn base_custody(
    root: &std::path::Path,
    record: &super::wire::GenerationRecord,
    committed: Option<&super::wire::GenerationRecord>,
) -> Result<Option<super::wire::GenerationRecord>, DomainResult> {
    match committed {
        None => Ok(None),
        Some(committed) => {
            if record.generation_digest == committed.generation_digest {
                match committed.previous_generation.value() {
                    None => Ok(None),
                    Some(previous) => Ok(Some(history::record(root, &previous.generation_digest)?)),
                }
            } else {
                // A direct successor ran from the committed generation
                // (the lineage check in `anchored_record`).
                Ok(Some(committed.clone()))
            }
        }
    }
}

/// Whether the retained plan, the minted record, and the base custody
/// relate exactly as `staged_apply` assembles them (r6, codex F1): every
/// non-delete plan output appears in the record verbatim (path, digest,
/// role, owner), every delete targets a base-owned output, and every
/// record output the plan does not name is a base output preserved
/// verbatim. A fabricated successor whose record/plan pair is internally
/// consistent but was never admitted by the apply machinery fails the
/// re-derivation checks below even when it passes this bijection — but
/// the bijection closes every cheaper structural hole first.
fn plan_matches_record(
    plan: &super::wire::GenerationPlan,
    record: &super::wire::GenerationRecord,
    base: Option<&super::wire::GenerationRecord>,
) -> Result<(), DomainResult> {
    let base_outputs: std::collections::BTreeMap<&str, &super::wire::RecordOutput> = base
        .map(|record| {
            record
                .outputs
                .iter()
                .map(|output| (output.path.as_str(), output))
                .collect()
        })
        .unwrap_or_default();
    let record_outputs: std::collections::BTreeMap<&str, &super::wire::RecordOutput> = record
        .outputs
        .iter()
        .map(|output| (output.path.as_str(), output))
        .collect();
    for output in &plan.outputs {
        match output.action {
            super::wire::OutputAction::Delete => {
                // Deletes consume recorded custody: the base generation
                // must own the path with the reviewed before digest.
                let Some(recorded) = base_outputs.get(output.path.as_str()) else {
                    return Err(unanchored());
                };
                if output.before_digest.value() != Some(&recorded.digest) {
                    return Err(unanchored());
                }
            }
            _ => {
                let Some(recorded) = record_outputs.get(output.path.as_str()) else {
                    return Err(unanchored());
                };
                let after = output.after_digest.value().cloned().unwrap_or_default();
                if recorded.digest != after
                    || recorded.role != output.role
                    || recorded.owner_module != output.owner_module
                    || recorded.owner_target != output.owner_target
                {
                    return Err(unanchored());
                }
            }
        }
    }
    for (path, preserved) in &record_outputs {
        if plan
            .outputs
            .iter()
            .any(|output| output.path.as_str() == *path)
        {
            continue;
        }
        // Preserved custody: a base output carried verbatim.
        let Some(base_output) = base_outputs.get(*path) else {
            return Err(unanchored());
        };
        if preserved.digest != base_output.digest
            || preserved.role != base_output.role
            || preserved.owner_module != base_output.owner_module
            || preserved.owner_target != base_output.owner_target
        {
            return Err(unanchored());
        }
    }
    Ok(())
}

/// The ownership evidence an anchored apply must bind to (r6-r7, devin
/// F2 / codex F1): the plan's input closure names the caller's ownership
/// document (the CLI pins it for every plan), the live file at that pin
/// must still carry the pinned bytes, the parsed document's canonical
/// hash must equal the record's authenticated `ownership_digest`, and it
/// must declare the plan's module/target in managed mode. The returned
/// assignment carries the declared generated root — the *caller-owned*
/// confinement authority the apply machinery itself uses, so a forged
/// record/plan pair never widens deletion outside a root the caller
/// declared (the r6 derived-prefix proxy — vacuous on a virgin project —
/// is gone).
fn plan_closure_ownership(
    root: &std::path::Path,
    plan: &super::wire::GenerationPlan,
    record: &super::wire::GenerationRecord,
) -> Result<super::wire::TargetAssignment, DomainResult> {
    for pin in &plan.input_closure {
        let Ok(Some(bytes)) = fs::read_file(root, &pin.path) else {
            return Err(unanchored());
        };
        if super::digest(&bytes) != pin.digest {
            return Err(unanchored());
        }
        let Ok(document) = super::parse_ownership(&bytes) else {
            continue;
        };
        if super::hash(&document) != record.ownership_digest {
            continue;
        }
        let Some(module) = document
            .modules
            .iter()
            .find(|module| module.module == plan.modules[0])
        else {
            continue;
        };
        let Some(target) = module
            .targets
            .iter()
            .find(|target| target.target == plan.targets[0])
        else {
            continue;
        };
        if module.mode != super::wire::Mode::Managed {
            return Err(unanchored());
        }
        let derived = super::plan::input_closure(root, &document, Some(&pin.path))
            .map_err(|_| unanchored())?;
        if derived != plan.input_closure {
            // A materialized extra file is not a policy-selected input.
            // Both the retained plan's and the record's sets must be
            // exactly what the machinery would mint (r7 devin F2 / codex F1).
            return Err(unanchored());
        }
        return Ok(target.clone());
    }
    Err(unanchored())
}

/// The anchored record must carry exactly the live derived generation
/// authority and full input set (r7 devin F2 / codex F1): the lock pin
/// equals the live authority, and every closure member equals the writer's
/// exact union of preserved base pins, policy-derived plan pins, and the
/// reserved Model/IR/lock cohort. A resolving but undeclared extra pin is
/// just as unanchorable as a missing one. A forged closure pin or a record
/// minted against a superseded Model/lock never anchors — the caller
/// cannot advance the lineage to custody the live authority does not
/// derive.
fn record_closure_matches(
    record: &super::wire::GenerationRecord,
    plan: &super::wire::GenerationPlan,
    base: Option<&super::wire::GenerationRecord>,
    authority: &super::authority::Authority,
) -> Result<(), DomainResult> {
    if record.lock_pin.value() != Some(&authority.lock_digest)
        || record.input_closure != super::apply::record_closure(plan, base, Some(authority))
    {
        return Err(unanchored());
    }
    Ok(())
}

/// The retained plan must be the exact deterministic inventory the apply
/// machinery renders from the retained coverage under the live ownership
/// and the live derived authority (r6-r7, devin F2 / codex F1+F2):
///
/// - the coverage object named by the plan's `coverageDigest` is retained
///   and admits against the closure-pinned ownership assignment;
/// - the rendered inventory (paths, roles, after digests — Model-derived
///   types included, via the derived semantics) equals the plan's
///   non-delete outputs exactly;
/// - every plan output's action/before-state is exactly what the apply
///   machinery's `classify` produced against the base custody (Create iff
///   unclaimed and before-absent; Replace/Unchanged iff base-owned with
///   before == recorded, Replace iff recorded ≠ after, Unchanged iff
///   recorded == after; Delete iff base-owned with before == recorded);
/// - the plan's `beforeManifest` pin names the base generation (absent
///   exactly on a first publication) and its action is the promotion /
///   regeneration the base implies.
///
/// A forged plan cannot satisfy this: its "creates" for unclaimed victim
/// files (inside or outside the derived root) are not rendered paths, and
/// its "replace" rows over unchanged bytes classify as Unchanged. The
/// rendered inventory is returned for the bound-evidence check.
#[allow(clippy::too_many_arguments)]
fn plan_rederives(
    plan: &super::wire::GenerationPlan,
    base: Option<&super::wire::GenerationRecord>,
    coverage: &super::wire::Coverage,
    assignment: &super::wire::TargetAssignment,
    authority: &super::authority::Authority,
) -> Result<Vec<super::canonical::Rendered>, DomainResult> {
    let module = super::wire::ModuleAssignment {
        module: plan.modules[0].clone(),
        mode: super::wire::Mode::Managed,
        targets: vec![assignment.clone()],
    };
    let admission =
        super::coverage::admit(&module, assignment, coverage).map_err(|_| unanchored())?;
    if plan.extensions != admission.extensions {
        return Err(unanchored());
    }
    let inventory = super::canonical::render_inventory(
        coverage,
        &assignment.generated_root,
        &authority.semantics,
    )
    .map_err(|_| unanchored())?;
    let base_outputs: std::collections::BTreeMap<&str, &super::wire::RecordOutput> = base
        .map(|record| {
            record
                .outputs
                .iter()
                .map(|output| (output.path.as_str(), output))
                .collect()
        })
        .unwrap_or_default();
    let mut planned_paths: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for output in &plan.outputs {
        planned_paths.insert(output.path.as_str());
        if output.owner_module != plan.modules[0] || output.owner_target != plan.targets[0] {
            return Err(unanchored());
        }
        match output.action {
            super::wire::OutputAction::Delete => {
                let Some(recorded) = base_outputs.get(output.path.as_str()) else {
                    return Err(unanchored());
                };
                if output.before_digest.value() != Some(&recorded.digest) {
                    return Err(unanchored());
                }
            }
            super::wire::OutputAction::Create
            | super::wire::OutputAction::Replace
            | super::wire::OutputAction::Unchanged => {
                let after = output.after_digest.value().ok_or_else(unanchored)?;
                let Some(bytes) = inventory
                    .iter()
                    .find(|artifact| artifact.path == output.path)
                    .map(|artifact| artifact.bytes.as_slice())
                else {
                    // The renderer never produced this path: a forged
                    // "create" for an unclaimed file, inside or outside
                    // the declared root (codex F2 / devin F2).
                    return Err(unanchored());
                };
                if super::digest(bytes) != *after {
                    return Err(unanchored());
                }
                let rendered_role = inventory
                    .iter()
                    .find(|artifact| artifact.path == output.path)
                    .expect("rendered")
                    .role;
                if rendered_role != output.role {
                    return Err(unanchored());
                }
                match base_outputs.get(output.path.as_str()) {
                    Some(recorded) => {
                        if output.before_digest.value() != Some(&recorded.digest) {
                            return Err(unanchored());
                        }
                        let unchanged = recorded.digest == *after;
                        match output.action {
                            super::wire::OutputAction::Unchanged if !unchanged => {
                                return Err(unanchored())
                            }
                            super::wire::OutputAction::Replace if unchanged => {
                                return Err(unanchored())
                            }
                            super::wire::OutputAction::Create => return Err(unanchored()),
                            _ => {}
                        }
                    }
                    None => {
                        if output.action != super::wire::OutputAction::Create
                            || output.before_digest.value().is_some()
                        {
                            return Err(unanchored());
                        }
                    }
                }
            }
        }
    }
    let rendered_paths: std::collections::BTreeSet<&str> = inventory
        .iter()
        .map(|artifact| artifact.path.as_str())
        .collect();
    if rendered_paths != planned_paths {
        return Err(unanchored());
    }
    // The plan's generation pin and action are the base's own.
    match (&plan.before_manifest, base) {
        (super::wire::State::Known { value }, Some(base)) => {
            if value.generation_digest != base.generation_digest {
                return Err(unanchored());
            }
        }
        (super::wire::State::Absent, None) => {}
        _ => return Err(unanchored()),
    }
    let expected_action = if base.is_some() {
        super::wire::PlanAction::Regeneration
    } else {
        super::wire::PlanAction::Promotion
    };
    if plan.action != expected_action && plan.action != super::wire::PlanAction::Upgrade {
        return Err(unanchored());
    }
    Ok(inventory)
}

/// Resolve the anchor of one journal, refusing every caller-forged pin.
fn anchor_journal(
    root: &std::path::Path,
    transaction: &Transaction,
    committed: Option<&super::wire::GenerationRecord>,
    derive_authority: &dyn Fn(&str) -> Result<super::authority::Authority, DomainResult>,
) -> Result<Anchored, DomainResult> {
    // Only journal shapes the machinery itself writes ever anchor (r6-r7,
    // devin F1c: `publishing` on a resolve journal and every other
    // never-emitted combination is refused — recovery stays owed).
    if !journal_shape_is_emitted(transaction) {
        return Err(unanchored());
    }
    if transaction.pending.is_empty() {
        return Ok(Anchored::Empty);
    }
    // Journal self-consistency: a pending path the reviewed transaction
    // never declared affected is a forged move (r4 finding 1).
    let manifest = super::custody::MANIFEST_LOGICAL;
    if transaction.pending.iter().any(|pending| {
        pending.path != manifest && !transaction.affected_files.contains(&pending.path)
    }) {
        return Err(unanchored());
    }
    // The pending set must cover exactly the reviewed transaction's
    // affected inventory (r6, codex F4): the machinery journals every
    // affected output plus the manifest move — a subset journal can never
    // finalize a partial destination as `completed` or bound a partial
    // undo.
    let pending_paths: std::collections::BTreeSet<&str> = transaction
        .pending
        .iter()
        .map(|pending| pending.path.as_str())
        .collect();
    let affected: std::collections::BTreeSet<&str> = pending_paths
        .into_iter()
        .filter(|path| *path != manifest)
        .collect();
    match transaction.action {
        TransactionAction::Apply => {
            let record = anchored_record(root, transaction, committed)?;
            let plan_digest = transaction.plan_digest.value().ok_or_else(unanchored)?;
            if *plan_digest != record.plan_digest {
                return Err(unanchored());
            }
            let plan = history::plan(root, &record.plan_digest).map_err(|_| unanchored())?;
            // The custody the apply ran from, and the exact relationship
            // the minted record bears to it (r6-r7, codex F1).
            let base = base_custody(root, &record, committed)?;
            plan_matches_record(&plan, &record, base.as_ref())?;
            // The retained plan must re-derive from live, caller-owned,
            // digest-pinned evidence (r6-r7, devin F2 / codex F1+F2):
            // the closure's ownership document (hash-bound to the
            // record's `ownershipDigest`), every closure pin resolving
            // against live bytes, the live derived authority pins, and
            // the deterministic inventory rendered from the retained
            // coverage under that ownership and authority — with the
            // action/before-state classification the apply machinery
            // itself produces. A forged plan/record pair — vacuously
            // self-consistent in the caller-writable store — cannot
            // satisfy any of this on a virgin or committed project.
            let assignment = plan_closure_ownership(root, &plan, &record)?;
            let authority = derive_authority(&plan.modules[0]).map_err(|_| unanchored())?;
            record_closure_matches(&record, &plan, base.as_ref(), &authority)?;
            let coverage =
                history::coverage(root, &plan.coverage_digest).map_err(|_| unanchored())?;
            let inventory =
                plan_rederives(&plan, base.as_ref(), &coverage, &assignment, &authority)?;
            // A bound generation's publication carried execution evidence
            // (r6 codex F4): the interrupted apply retained it inside the
            // transaction, so recovery requires exactly that retained
            // document before its journal may anchor.
            if coverage.extensions.iter().any(|check| check.validated) {
                let resolve = |test_path: &str| {
                    inventory
                        .iter()
                        .find(|artifact| artifact.path == test_path)
                        .map(|artifact| super::digest(&artifact.bytes))
                };
                let mut retained = false;
                for candidate in history::objects_for_probe(root) {
                    let Ok(bytes) = history::object(root, &candidate) else {
                        continue;
                    };
                    let Ok(evidence) = super::parse_equivalence(&bytes) else {
                        continue;
                    };
                    if super::equivalence::verify_execution_evidence(
                        &plan,
                        &coverage,
                        &assignment.generated_root,
                        &resolve,
                        &evidence,
                    )
                    .is_ok()
                    {
                        retained = true;
                        break;
                    }
                }
                if !retained {
                    return Err(unanchored());
                }
            }
            let planned: std::collections::BTreeSet<&str> = plan
                .outputs
                .iter()
                .map(|output| output.path.as_str())
                .collect();
            if affected != planned {
                return Err(unanchored());
            }
            // A create is journaled as a publish; deletes of committed
            // outputs are journaled as deletes. A restore op in an apply
            // journal is a shape the machinery never emits.
            for pending in &transaction.pending {
                if pending.path == manifest {
                    continue;
                }
                let planned_op = plan
                    .outputs
                    .iter()
                    .find(|output| output.path == pending.path)
                    .map(|output| output.action == super::wire::OutputAction::Delete);
                let expect_delete = planned_op.unwrap_or(false);
                let is_delete = pending.op == PendingOp::Delete;
                let is_publish = pending.op == PendingOp::Publish;
                if (expect_delete && !is_delete) || (!expect_delete && !is_publish) {
                    return Err(unanchored());
                }
            }
            Ok(Anchored::Apply { record, plan })
        }
        TransactionAction::Rollback => {
            let record = anchored_record(root, transaction, committed)?;
            let Some(current) = committed else {
                return Err(unanchored());
            };
            // The authoritative intent is rebuilt from custody evidence —
            // the anchored destination record and the committed record the
            // rollback ran from — and the journal's plan pin must name
            // exactly it (r4 finding 1): a minted rollback plan object
            // never bounds a recovery move.
            let Some(plan) = super::rollback::recovery_plan(root, &record, current) else {
                return Err(unanchored());
            };
            let pinned = transaction.plan_digest.value().ok_or_else(unanchored)?;
            if *pinned != plan.plan_digest {
                return Err(unanchored());
            }
            let reviewed: std::collections::BTreeSet<&str> = plan
                .restore
                .iter()
                .map(|op| op.path.as_str())
                .chain(plan.delete.iter().map(|op| op.path.as_str()))
                .collect();
            if affected != reviewed {
                return Err(unanchored());
            }
            Ok(Anchored::Rollback { record, plan })
        }
        TransactionAction::Resolve => {
            let Some(committed) = committed else {
                return Err(unanchored());
            };
            let generation = transaction
                .generation_digest
                .value()
                .map(|digest| digest.generation_digest.clone())
                .ok_or_else(unanchored)?;
            if generation != committed.generation_digest {
                return Err(unanchored());
            }
            // The confirmation digest is the journal's own self digest
            // (r6-r7, devin F1a): `staged_resolve` pins
            // `planDigest = resolve_digest(transaction)`, excluding the
            // later-assigned attempt id and transient phase, so a forged
            // `planDigest` value — one the confirmed flow could never have
            // written — never anchors. The r6 retained-intent object check
            // is gone: its name was a deterministic function of this very
            // journal, so a journal forger could always mint it too;
            // instead the resolve convergence below is behavioral and
            // never restores a byte on the journal's word.
            let pinned = transaction.plan_digest.value().ok_or_else(unanchored)?;
            if *pinned != super::rollback::resolve_digest(transaction) {
                return Err(unanchored());
            }
            Ok(Anchored::Resolve)
        }
    }
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
/// does not own must be an exact output of the journal's *anchored*
/// intent; live bytes must match the committed state or the anchored
/// intent's before/after state — anything else is manual drift that
/// refuses the whole journal (findings 1, 2, and r4-1: the journal's own
/// `beforeDigest` pin is caller-authored and never an accepted live
/// state for apply/rollback journals; only a resolve journal's confirmed
/// drift bound may converge).
fn convergence_moves(
    root: &std::path::Path,
    transaction: &Transaction,
    committed: Option<&super::wire::GenerationRecord>,
    anchored: &Anchored,
) -> Result<Vec<Move>, DomainResult> {
    let intent = anchored_intent(anchored);
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
            let pinned_before = if matches!(anchored, Anchored::Resolve) {
                transaction.before_digest.value().cloned()
            } else {
                None
            };
            let acceptable = |digest: &str| {
                digest == committed_digest
                    || before.as_deref() == Some(digest)
                    || after.as_deref() == Some(digest)
                    || pinned_before.as_deref() == Some(digest)
            };
            match live_digest.as_deref() {
                Some(digest) if acceptable(digest) => {
                    if matches!(anchored, Anchored::Resolve) {
                        // A resolve journal never restores on recovery's
                        // own word (r6-r7, devin F1 / codex F3): live ==
                        // committed means the confirmed restore already
                        // landed (nothing to do — the completion branch
                        // closes it `recovered`); live == the pinned drift
                        // means it never landed, and the *caller* re-runs
                        // `managed resolve --confirm` — recovery closes the
                        // journal `abandoned` and preserves the drift. The
                        // retained-intent object of r6 was mintable from
                        // the journal itself and anchored nothing.
                        moves.push(Move::Noop);
                    } else if digest == committed_digest {
                        moves.push(Move::Noop);
                    } else {
                        moves.push(Move::RestoreCommitted {
                            path: pending.path.clone(),
                            bytes,
                        });
                    }
                }
                // Resolve recovery never writes on a journal's word, even
                // when the reviewed drift was absence. Preserve that absence
                // for a fresh confirmed retry; an unexpected disappearance
                // after reviewing present drift is itself new drift.
                None if matches!(anchored, Anchored::Resolve) => {
                    if transaction.before_digest.value().is_some() {
                        return Err(invalid("recovery-drift"));
                    }
                    moves.push(Move::Noop);
                }
                // Apply/rollback undo restores exact committed bytes.
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
        // reviewed output of the journal's *anchored* intent, and the live
        // bytes must be the intent's own before or after state (finding 1
        // and r4-1: exact paths only — no minted-plan bounds, no
        // generated-root prefix widening).
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

/// The reviewed intent of an anchored journal: apply journals are bounded
/// by the anchored record's authenticated plan (resolved and held by the
/// anchor); rollback journals by their anchored rollback plan; resolve
/// journals carry no plan intent.
fn anchored_intent(anchored: &Anchored) -> Option<Intent> {
    match anchored {
        Anchored::Apply { plan, .. } => Some(Intent::Generation(plan.clone())),
        Anchored::Rollback { plan, .. } => Some(Intent::Rollback(plan.clone())),
        Anchored::Resolve | Anchored::Empty => None,
    }
}

/// The validated pre-transaction manifest action of one journal (findings
/// 10, 11, r4-1/r4-10, and r6 codex F3 / devin F6): the journal's
/// `beforeManifestDigest` pin resolves ONLY against transaction-owned
/// lineage evidence — the committed generation's exact retained manifest
/// (the pre-transaction state of every legitimate apply/rollback journal
/// on a committed project) or the live manifest's own digest (the
/// transaction never moved it). For a first publication (no committed
/// generation) an object-store pin remains admissible: the pre-transaction
/// manifest was never committed anywhere else, and a planted manifest
/// cannot biject with any custody record (the publish bijection gate
/// refuses it on the next mutation). An absence pin is legal only for a
/// first publication; anything else — including a manifest destination
/// blocked by non-file data — refuses the journal so recovery stays owed
/// instead of finalizing divergent custody. Validation is read-only:
/// `abandon` resolves the complete action BEFORE the first application
/// byte moves and executes it afterwards, so an unanchorable manifest pin
/// leaves the tree exactly as it was (r6 codex F3: refusal never again
/// means "already restored the file").
enum ManifestAction {
    /// The transaction never moved the manifest (or the pin is the live
    /// digest): nothing to execute.
    Keep,
    /// Restore these exact retained bytes over the public manifest.
    Restore(Vec<u8>),
    /// Converge the manifest coordinate to absence (first publication
    /// whose pre-transaction state had no manifest).
    Remove,
}

fn resolve_manifest_before(
    root: &std::path::Path,
    transaction: &Transaction,
    committed: Option<&super::wire::GenerationRecord>,
) -> Result<ManifestAction, DomainResult> {
    // A blocked manifest destination refuses before anything moves
    // (r4 finding 10): a directory, link, or alias at the governed
    // coordinate is a converged impossibility, never a silent skip.
    let live = fs::read_file(root, super::custody::MANIFEST_LOGICAL)
        .map_err(|_| invalid("recovery-manifest"))?;
    match transaction.before_manifest_digest.value() {
        Some(pin) => {
            if live.as_deref().map(super::digest).as_deref() == Some(pin) {
                // The transaction never moved the manifest.
                return Ok(ManifestAction::Keep);
            }
            let bytes = match committed {
                Some(record) => {
                    let retained =
                        super::custody::retained_manifest(root, &record.generation_digest)
                            .map_err(|_| invalid("recovery-unanchored"))?;
                    if super::digest(&retained) != *pin {
                        // A caller-minted object pin never restores the
                        // public manifest (r4 finding 1c).
                        return Err(invalid("recovery-unanchored"));
                    }
                    retained
                }
                None => history::object(root, pin).map_err(|_| invalid("recovery-unanchored"))?,
            };
            Ok(ManifestAction::Restore(bytes))
        }
        None => match committed {
            // A committed generation's pre-transaction manifest always
            // exists (its retained snapshot); an absence pin is a forged
            // first-publication shape (r4 findings 1c and 10).
            Some(_) => Err(invalid("recovery-unanchored")),
            None => match live {
                Some(_) => Ok(ManifestAction::Remove),
                None => Ok(ManifestAction::Keep),
            },
        },
    }
}

/// Whether every pending move of `transaction` is already exactly the
/// recorded state of `record` on disk, **every output of the anchored
/// generation** is present with its exact recorded bytes (r6, codex F4:
/// completion certified over the journal's pending subset alone could
/// report `completed` while a recorded generated output was missing — a
/// state the ordinary read-only check rejects), and the public manifest
/// already carries the generation's exact published bytes (finding 10:
/// completion without the public manifest is a false success).
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
    // The complete destination inventory must be live and exact: any
    // missing or divergent recorded output keeps recovery owed (r6,
    // codex F4).
    let all_outputs_match = record.outputs.iter().all(|output| {
        matches!(fs::read_file(root, &output.path), Ok(Some(bytes)) if super::digest(&bytes) == output.digest)
    });
    if !all_outputs_match {
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
///
/// `derive_authority` re-derives the generation authority of one module
/// from the live project (the CLI passes the accepted loader pipeline;
/// tests pass their fixture authority). Apply journals anchor only while
/// the live project still derives the exact canonical Model/IR/lock their
/// record pins — a forged or superseded lineage never anchors, and
/// recovery stays owed rather than converging on stale authority.
pub fn run(
    root: &std::path::Path,
    derive_authority: &dyn Fn(&str) -> Result<super::authority::Authority, DomainResult>,
) -> DomainResult {
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
        // The complete journal is validated — anchor, retained intent,
        // snapshots, and every live before-state — before the first byte of
        // this journal moves (finding 2 and r4 finding 1): every pin the
        // journal carries is bound to the transaction-owned committed
        // lineage, never to whatever object a caller planted in the store.
        // A journal that cannot anchor is refused invalid without being
        // finalized, so recovery stays owed — it never converges, never
        // deletes, and never overwrites on a caller's word.
        let anchored = match anchor_journal(root, transaction, committed.as_ref(), derive_authority)
        {
            Ok(anchored) => anchored,
            Err(error) => return error,
        };
        let anchored_record_ref = match &anchored {
            Anchored::Apply { record, .. } | Anchored::Rollback { record, .. } => Some(record),
            Anchored::Resolve => committed.as_ref(),
            Anchored::Empty => None,
        };
        let outcome = match anchored_record_ref {
            Some(record) if matches_generation_state(root, transaction, record) => {
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
            _ => match abandon(root, transaction, committed.as_ref(), &anchored) {
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
/// it as `abandoned`. Every move — application bytes AND the manifest
/// action — is prebuilt and fully validated before the first mutation
/// (r6, codex F3 / devin F6: the manifest pin resolves in the preflight
/// phase, so an invalid manifest-evidence journal refuses with zero
/// filesystem effects and retries fail identically instead of partially
/// converging); any refusal leaves the journal unresolved (recovery stays
/// owed).
fn abandon(
    root: &std::path::Path,
    transaction: &Transaction,
    committed: Option<&super::wire::GenerationRecord>,
    anchored: &Anchored,
) -> Result<String, DomainResult> {
    // Preflight: the complete move list is built before any write.
    let moves = match convergence_moves(root, transaction, committed, anchored) {
        Ok(moves) => moves,
        Err(_) => return Err(invalid("recovery-undo")),
    };
    // The manifest action is resolved and validated in the preflight
    // phase too — but only when the journal itself moved the manifest
    // (findings 10 and 11; a resolve journal never touches it). A blocked
    // manifest destination or an unresolvable manifest pin refuses before
    // any application byte moves: converging the files and then
    // discovering an unwritable manifest would finalize divergent custody
    // (r4 finding 10), and validating after the moves left the tree half
    // converged on a bad pin (r6 codex F3).
    let moves_manifest = transaction
        .pending
        .iter()
        .any(|pending| pending.path == super::custody::MANIFEST_LOGICAL);
    let manifest_action = if moves_manifest {
        match resolve_manifest_before(root, transaction, committed) {
            Ok(action) => Some(action),
            Err(_) => return Err(invalid("recovery-manifest")),
        }
    } else {
        None
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
    // application bytes it describes — the action was already validated
    // above; only the execution remains.
    if let Some(action) = manifest_action {
        let executed = match action {
            ManifestAction::Keep => Ok(()),
            ManifestAction::Restore(bytes) => super::custody::restore_manifest_bytes(root, &bytes)
                .map_err(|_| invalid("recovery-manifest")),
            ManifestAction::Remove => {
                if !fs::remove_file(root, super::custody::MANIFEST_LOGICAL)
                    .map_err(|_| invalid("recovery-manifest"))?
                {
                    Err(invalid("recovery-manifest"))
                } else {
                    Ok(())
                }
            }
        };
        executed?;
    }
    if close(root, transaction, TransactionState::Abandoned).is_err() {
        return Err(invalid("recovery-journal"));
    }
    Ok("abandoned".to_owned())
}
