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
///   does not own must be a plan **create inside a generated root the
///   committed custody can derive** (the common directory of the committed
///   outputs of the same owner), and the journal's pending set must be
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
///   generation, and — r6, codex F2 — the exact confirmed intent must be
///   retained as a custody object: the resolve transaction document as the
///   confirmed flow wrote it (state `prepared`). A bare journal whose
///   caller-authored `beforeDigest` describes live drift is not reviewed
///   evidence and never authorizes overwriting a manual edit.
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

/// The common directory prefix (segment-wise) of the committed outputs of
/// one owner — the generated-root proxy recovery can derive without the
/// caller-owned ownership document: a managed module renders an
/// implementation and a test under its root, so the common prefix of a
/// module's committed outputs is its generated root (r6, codex F1).
fn owner_root_prefix(
    committed: &super::wire::GenerationRecord,
    owner_module: &str,
    owner_target: &str,
) -> Option<String> {
    let mut prefix: Option<Vec<&str>> = None;
    for output in &committed.outputs {
        if output.owner_module != owner_module || output.owner_target != owner_target {
            continue;
        }
        let segments: Vec<&str> = output.path.split('/').collect();
        // The file name is never part of the directory prefix.
        let directories = &segments[..segments.len().saturating_sub(1)];
        prefix = Some(match prefix {
            None => directories.to_vec(),
            Some(current) => current
                .iter()
                .zip(directories.iter())
                .take_while(|(left, right)| left == right)
                .map(|(left, _)| *left)
                .collect(),
        });
    }
    prefix.map(|segments| segments.join("/"))
}

/// Whether the retained plan, the minted record, and the committed record
/// relate exactly as `staged_apply` assembles them (r6, codex F1): every
/// non-delete plan output appears in the record verbatim (path, digest,
/// role, owner), every delete targets a committed-owned output, and every
/// record output the plan does not name is a committed output preserved
/// verbatim. A fabricated successor whose record/plan pair is internally
/// consistent but was never admitted by the apply machinery fails the
/// ownership-scope rule below even when it passes this bijection — but the
/// bijection closes every cheaper structural hole first.
fn plan_matches_record(
    plan: &super::wire::GenerationPlan,
    record: &super::wire::GenerationRecord,
    committed: Option<&super::wire::GenerationRecord>,
) -> Result<(), DomainResult> {
    let committed_outputs: std::collections::BTreeMap<&str, &super::wire::RecordOutput> = committed
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
                // Deletes consume recorded custody: the committed
                // generation must own the path with the reviewed before
                // digest.
                let Some(recorded) = committed_outputs.get(output.path.as_str()) else {
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
        // Preserved custody: a committed output carried verbatim.
        let Some(committed_output) = committed_outputs.get(*path) else {
            return Err(unanchored());
        };
        if preserved.digest != committed_output.digest
            || preserved.role != committed_output.role
            || preserved.owner_module != committed_output.owner_module
            || preserved.owner_target != committed_output.owner_target
        {
            return Err(unanchored());
        }
    }
    Ok(())
}

/// The apply-machinery ownership scope an interrupted apply's *new*
/// outputs must sit inside (r6, codex F1): every record output the
/// committed generation does not own must be a plan **create** (before
/// absent — an apply that found live bytes at a create coordinate was
/// denied) whose path sits under a generated root recovery can derive from
/// committed custody: the common directory of the committed outputs of the
/// same owner. A fabricated successor that names a maintained file
/// outside every generated root as its "create" — the confirmed exploit —
/// is unanchored here, before any byte moves. An owner with no committed
/// outputs cannot prove any root; its journals stay owed (the caller
/// reconciles by hand rather than recovery deleting on a forged word).
fn apply_creates_confined(
    plan: &super::wire::GenerationPlan,
    record: &super::wire::GenerationRecord,
    committed: Option<&super::wire::GenerationRecord>,
) -> Result<(), DomainResult> {
    let Some(committed) = committed else {
        return Ok(()); // first publication: nothing is committed-owned
    };
    let committed_paths: std::collections::BTreeSet<&str> = committed
        .outputs
        .iter()
        .map(|output| output.path.as_str())
        .collect();
    for output in &record.outputs {
        if committed_paths.contains(output.path.as_str()) {
            continue;
        }
        let Some(planned) = plan
            .outputs
            .iter()
            .find(|planned| planned.path == output.path)
        else {
            return Err(unanchored());
        };
        if planned.action != super::wire::OutputAction::Create
            || planned.before_digest.value().is_some()
        {
            return Err(unanchored());
        }
        let Some(prefix) = owner_root_prefix(committed, &output.owner_module, &output.owner_target)
        else {
            return Err(unanchored());
        };
        if prefix.is_empty() || !output.path.starts_with(&format!("{prefix}/")) {
            return Err(unanchored());
        }
    }
    Ok(())
}

/// Resolve the anchor of one journal, refusing every caller-forged pin.
fn anchor_journal(
    root: &std::path::Path,
    transaction: &Transaction,
    committed: Option<&super::wire::GenerationRecord>,
) -> Result<Anchored, DomainResult> {
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
            plan_matches_record(&plan, &record, committed)?;
            apply_creates_confined(&plan, &record, committed)?;
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
            // The retained confirmed intent (r6, codex F2): the resolve
            // flow retains the exact confirmed transaction document as a
            // custody object before it journals anything. Recovery accepts
            // a resolve journal only when that retained intent exists and
            // matches the journal exactly — a bare caller-authored journal
            // whose `beforeDigest` merely *describes* live drift is not
            // reviewed evidence and never overwrites a manual edit.
            let mut intent = transaction.clone();
            intent.state = TransactionState::Prepared;
            let bytes = serde_json::to_vec_pretty(&intent).map_err(|_| unanchored())?;
            history::object(root, &super::digest(&bytes)).map_err(|_| unanchored())?;
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
        // The complete journal is validated — anchor, retained intent,
        // snapshots, and every live before-state — before the first byte of
        // this journal moves (finding 2 and r4 finding 1): every pin the
        // journal carries is bound to the transaction-owned committed
        // lineage, never to whatever object a caller planted in the store.
        // A journal that cannot anchor is refused invalid without being
        // finalized, so recovery stays owed — it never converges, never
        // deletes, and never overwrites on a caller's word.
        let anchored = match anchor_journal(root, transaction, committed.as_ref()) {
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
