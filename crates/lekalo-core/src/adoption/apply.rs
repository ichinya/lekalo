//! Verified selective apply and exact rollback (issue #136, increment
//! I4): only reviewed bytes of one confirmed refactor plan install, and
//! only after the pinned verification evidence really passed on both
//! sides. Failure never records completion; a rollback restores the
//! exact prior bytes and refuses when later edits conflict.
//!
//! Custody model: apply admits an execution document only through
//! [`verification::admit_execution`] — the execution binds to this
//! exact plan, the exact plan-pinned candidate-patch digest, and the
//! complete consumed input set, and its measured results are re-derived
//! at admission: outcomes must cover the declared checks 1:1, the
//! oracle verdict and conclusion must follow from the measurements, the
//! consumed runner bytes and plan-pinned tree must still hold, and the
//! run must name this exact candidate. Documents that are internally
//! inconsistent, measured-empty, stale, swapped, or replayed refuse
//! before any write; the custody recipe is public, so the guarantee is
//! binding and consistency, not a checksum-proof of authenticity.

use super::analysis::{compute_pins, AdoptionContext, Outcome};
use super::diagnostic;
use super::transaction::{self, Journal, JournalStep, SnapshotRecord};
use super::version;
use super::wire::{self, AdoptionReceipt, PatchDocument, RefactorPlan, VerificationDocument};
use super::{verification, Outcome as AdoptionOutcome};
use crate::observed::types::ObservedIndex;
use crate::result::DomainResult;

/// Apply one confirmed refactor plan. The verification execution
/// document must carry admitted execution custody for this exact plan
/// and candidate, conclude `preserved` from measured results, and still
/// match the current tree; the candidate patch must still hash to the
/// plan's pinned digest; the plan digest must name the exact current
/// plan bytes. Every write is snapshotted, journaled, and installed
/// through compare-and-swap.
pub fn apply(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    plan_bytes: &[u8],
    patch_bytes: &[u8],
    execution_bytes: &[u8],
    confirm_digest: &str,
) -> Outcome<AdoptionReceipt> {
    let document =
        wire::refactor_plan_of_bytes(plan_bytes).map_err(|error| error.domain_result())?;
    let value = serde_json::to_value(&document).expect("plan serializes");
    let recomputed = wire::digest(
        "adoption-refactor-plan",
        &wire::without_member(&wire::without_member(&value, "plan"), "rollback"),
    );
    if recomputed != document.plan {
        return Err(DomainResult::denied(diagnostic::confirmation_required_set(
            &document.plan,
            "digest-mismatch",
        )));
    }
    if confirm_digest != document.plan || !version::is_sha256(confirm_digest) {
        return Err(DomainResult::denied(diagnostic::confirmation_required_set(
            &document.plan,
            "confirmation-mismatch",
        )));
    }
    // A plan without a concrete patch is advice and can never apply.
    let Some(patch_digest) = &document.patch else {
        return Err(DomainResult::denied(diagnostic::scope_refused_set(
            "apply",
            "advice-only-plan",
        )));
    };

    // Execution custody before anything else: the document must decode
    // as an execution variant whose measurements this tree can accept —
    // bound to this exact plan, the exact pinned candidate digest, the
    // complete consumed-input set, and measured results that cover the
    // declared checks 1:1 with a re-derived oracle and conclusion. An
    // inconsistent, measured-empty, stale, or replayed `preserved`
    // never reaches a write.
    let execution: VerificationDocument =
        verification::admit_execution(ctx, &document, patch_digest, execution_bytes)?;

    // The candidate patch bytes: a closed, scope-contained document
    // revalidated against the plan's pinned digest and mutation scope.
    let patch = load_patch_for(ctx, &document, patch_bytes, patch_digest)?;

    // Re-fingerprint every consumed input before any write: the same
    // consumed set the plan pinned, recomputed at apply time.
    let current = compute_pins(ctx, index, &consumed_sources(&document, &patch))?;
    let changed = transaction::changed_pins(&document.pins, &current);
    let tree_current = super::refactor::tree_digest(&current);
    let tree_plan = super::refactor::tree_digest(&document.pins);
    let mut changed = changed;
    if tree_current != tree_plan {
        changed.push("verification-baseline-source".to_owned());
    }
    if !changed.is_empty() {
        return Err(DomainResult::denied(diagnostic::plan_stale_set(
            &document.plan,
            &changed,
        )));
    }

    // The mutation set is exactly the patch's files, nothing else.
    let mut targets: Vec<String> = patch.files.iter().map(|file| file.path.clone()).collect();
    targets.sort();
    let Some(home) = transaction::recovery_home(&ctx.root, &document.plan) else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "plan",
            "invalid-digest",
        )));
    };
    let mut journal = Journal::new(&document.plan, "apply");
    let snapshots = transaction::snapshot_targets(&ctx.root, &home, &targets, &mut journal)?;
    let mut writes = Vec::new();
    for file in &patch.files {
        // One exact write entry per action: deletes carry the expected
        // pre-state in `before` (never a synthesized after-digest) so
        // compare-and-swap checks the live bytes against reality.
        let entry = match file.action.as_str() {
            "delete" => wire::WriteEntry {
                path: file.path.clone(),
                action: "delete".to_owned(),
                before: file.before.clone(),
                after: None,
                content: None,
            },
            _ => file.clone(),
        };
        let result = match file.action.as_str() {
            "delete" => transaction::cas_delete(&ctx.root, &entry),
            _ => transaction::cas_install(&ctx.root, &entry),
        };
        if let Err(result) = result {
            return Err(failed_apply(
                &ctx.root,
                &home,
                &mut journal,
                &document.plan,
                &file.path,
                result,
            ));
        }
        // The written step carries its full restoration metadata — the
        // snapshot path, the mutation class, and the exact pre-state —
        // so an interrupted installation is recoverable from the
        // journal alone, with no completed receipt required (X5).
        let snapshot = snapshots
            .iter()
            .find(|record| record.path == file.path)
            .map(|record| record.snapshot.clone());
        journal.steps.push(JournalStep {
            path: file.path.clone(),
            state: "written".to_owned(),
            snapshot,
            action: Some(entry.action.clone()),
            before: entry.before.clone(),
        });
        transaction::persist_journal(&ctx.root, &home, &journal)?;
        writes.push(entry);
    }
    let receipt = AdoptionReceipt {
        schema_version: version::RECEIPT_SCHEMA_VERSION.to_owned(),
        identity: version::RECEIPT_IDENTITY.to_owned(),
        receipt: String::new(),
        phase: "apply".to_owned(),
        project: document.project.clone(),
        plan: document.plan.clone(),
        outcome: "completed".to_owned(),
        writes,
        journal: Some(home.journal_relative.clone()),
        snapshots: snapshots
            .iter()
            .map(|record| record.snapshot.clone())
            .collect(),
        verification: execution.spec.clone(),
        pins: current,
        analysis: Some(document.analysis.clone()),
    };
    let value = serde_json::to_value(&receipt).expect("receipt serializes");
    let digest = wire::digest("adoption-receipt", &wire::without_member(&value, "receipt"));
    let receipt = AdoptionReceipt {
        receipt: digest,
        ..receipt
    };
    // The receipt is inside the recoverable transaction: a persistence
    // failure here is a classified recovery-required refusal over
    // committed writes — the journal names every stranded step with its
    // snapshot and pre-state, so `adoption recovery --restore` can
    // complete the transaction without this receipt ever existing (X5).
    if let Err(result) = transaction::persist_receipt(&ctx.root, &document.plan, &receipt) {
        journal.steps.push(JournalStep {
            path: home.journal_relative.clone(),
            state: "receipt-write-failed".to_owned(),
            snapshot: None,
            action: None,
            before: None,
        });
        let _ = transaction::persist_journal(&ctx.root, &home, &journal);
        return Err(result);
    }
    Ok(receipt)
}

/// Journal one failed installation step and surface the typed
/// recovery-required refusal. The journal is durable before the error
/// returns: a partial application is never reported as either
/// completed or silently lost. Receipt persistence failures after the
/// writes completed are also classified recovery-required.
fn failed_apply(
    root: &std::path::Path,
    home: &transaction::RecoveryHome,
    journal: &mut Journal,
    plan: &str,
    path: &str,
    result: DomainResult,
) -> DomainResult {
    journal.steps.push(JournalStep {
        path: path.to_owned(),
        state: "failed".to_owned(),
        snapshot: None,
        action: None,
        before: None,
    });
    let persisted = transaction::persist_journal(root, home, journal);
    if persisted.is_err() {
        return DomainResult::invalid(diagnostic::recovery_required_set(plan, "journal-write"));
    }
    result
}

/// Re-derive and revalidate the candidate patch at apply time: the
/// patch must still hash to the plan's pinned digest and every file
/// must still sit inside the plan's recorded mutation scope.
fn load_patch_for(
    ctx: &AdoptionContext,
    document: &RefactorPlan,
    patch_bytes: &[u8],
    patch_digest: &str,
) -> AdoptionOutcome<PatchDocument> {
    let patch = wire::patch_of_bytes(patch_bytes).map_err(|error| error.domain_result())?;
    let value = serde_json::to_value(&patch).expect("patch serializes");
    let recomputed = wire::digest("adoption-patch", &wire::without_member(&value, "patch"));
    if recomputed != patch_digest {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "candidate",
            "digest-mismatch",
        )));
    }
    for file in &patch.files {
        if !document.mutation_scope.contains(&file.path) {
            return Err(DomainResult::denied(diagnostic::scope_refused_set(
                &file.path,
                "outside-mutation-scope",
            )));
        }
    }
    let _ = ctx;
    Ok(patch)
}

/// The complete consumed input set of one plan plus its patch files:
/// read scope ∪ mutation scope ∪ the patch paths. Absent files simply
/// contribute no pin; the plan pins exactly this set, so plan-time and
/// apply-time fingerprints can never structurally disagree.
fn consumed_sources(
    document: &RefactorPlan,
    patch: &PatchDocument,
) -> std::collections::BTreeSet<String> {
    let mut sources: std::collections::BTreeSet<String> = document
        .read_scope
        .iter()
        .chain(document.mutation_scope.iter())
        .cloned()
        .collect();
    for file in &patch.files {
        sources.insert(file.path.clone());
    }
    sources
}

/// Preview the exact files and digests a rollback would restore.
pub fn rollback_preview(ctx: &AdoptionContext, receipt_bytes: &[u8]) -> Outcome<AdoptionReceipt> {
    let receipt = wire::receipt_of_bytes(receipt_bytes).map_err(|error| error.domain_result())?;
    if receipt.phase != "apply" && receipt.phase != "confirmation" {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "receipt", "phase",
        )));
    }
    if receipt.outcome != "completed" {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "receipt", "outcome",
        )));
    }
    let _ = ctx;
    Ok(receipt)
}

/// Execute one rollback. The current bytes of every mutated path must
/// still match the receipt's expected post-state; later edits are a
/// conflict refusal that preserves those edits and demands a separately
/// reviewed recovery plan.
pub fn rollback(
    ctx: &AdoptionContext,
    receipt_bytes: &[u8],
    confirm_digest: &str,
) -> Outcome<AdoptionReceipt> {
    let receipt = rollback_preview(ctx, receipt_bytes)?;
    let value = serde_json::to_value(&receipt).expect("receipt serializes");
    let recomputed = wire::digest("adoption-receipt", &wire::without_member(&value, "receipt"));
    if recomputed != receipt.receipt {
        return Err(DomainResult::denied(diagnostic::confirmation_required_set(
            &receipt.plan,
            "receipt-digest-mismatch",
        )));
    }
    if confirm_digest != receipt.receipt || !version::is_sha256(confirm_digest) {
        return Err(DomainResult::denied(diagnostic::confirmation_required_set(
            &receipt.plan,
            "confirmation-mismatch",
        )));
    }
    // Conflict detection: every mutated path must still hold the exact
    // post-state the receipt recorded. Presence follows the recorded
    // action, never an action-name guess: created paths must be gone,
    // every other path must hold the exact recorded post-state.
    let mut conflicts: Vec<String> = Vec::new();
    for write in &receipt.writes {
        let current = std::fs::read(ctx.root.join(&write.path));
        match (write.action.as_str(), current) {
            ("delete", Ok(bytes)) => {
                conflicts.push(write.path.clone());
                let _ = bytes;
            }
            ("delete", Err(_)) => {}
            (action, Ok(bytes)) => {
                let digest = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
                if write.after.as_deref() != Some(digest.as_str()) {
                    conflicts.push(write.path.clone());
                }
                let _ = action;
            }
            (_, Err(_)) => conflicts.push(write.path.clone()),
        }
    }
    if !conflicts.is_empty() {
        return Err(DomainResult::denied(diagnostic::rollback_conflict_set(
            &receipt.receipt,
            &conflicts,
        )));
    }
    // Restore through the exact local snapshots. The snapshot file for
    // every path is derived from the path itself (sha256 hex), the
    // digest-keyed file is reparse-checked before read, and the bytes
    // are verified against the receipt's recorded `before` digest
    // before anything moves.
    let Some(home) = transaction::recovery_home(&ctx.root, &receipt.plan) else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "receipt",
            "plan-digest",
        )));
    };
    let mut journal = Journal::new(&receipt.plan, "rollback");
    let (head, tail) = version::digest_dir(&receipt.plan).unwrap_or_default();
    let records: Vec<SnapshotRecord> = receipt
        .writes
        .iter()
        .map(|write| SnapshotRecord {
            path: write.path.clone(),
            snapshot: format!(
                "{}/{}/{}/snapshots/{}",
                version::RECOVERY_DIR,
                head,
                tail,
                snapshot_name(&write.path)
            ),
            // The exact prior state follows the recorded pre-state:
            // `before` present means restore bytes, absent means the
            // path was created by this plan and is removed again.
            present: write.before.is_some(),
            before_digest: write.before.clone(),
        })
        .collect();
    // Preflight before the first mutation (X6): every snapshot of the
    // whole receipt is read back and digest-verified up front. A single
    // corrupt or missing snapshot refuses the entire rollback with the
    // current files untouched — never a partial restore that stops
    // halfway through the write set.
    for record in &records {
        transaction::read_snapshot(&ctx.root, record)?;
    }
    for record in &records {
        if let Err(result) = transaction::cas_restore(&ctx.root, record) {
            journal.steps.push(JournalStep {
                path: record.path.clone(),
                state: "restore-failed".to_owned(),
                snapshot: None,
                action: None,
                before: None,
            });
            let _ = transaction::persist_journal(&ctx.root, &home, &journal);
            return Err(result);
        }
        journal.steps.push(JournalStep {
            path: record.path.clone(),
            state: "restored".to_owned(),
            snapshot: Some(record.snapshot.clone()),
            action: None,
            before: None,
        });
        transaction::persist_journal(&ctx.root, &home, &journal)?;
    }
    let rollback_receipt = AdoptionReceipt {
        schema_version: version::RECEIPT_SCHEMA_VERSION.to_owned(),
        identity: version::RECEIPT_IDENTITY.to_owned(),
        receipt: String::new(),
        phase: "rollback".to_owned(),
        project: receipt.project.clone(),
        plan: receipt.plan.clone(),
        outcome: "rolled-back".to_owned(),
        writes: receipt.writes.clone(),
        journal: Some(home.journal_relative.clone()),
        snapshots: receipt.snapshots.clone(),
        verification: receipt.verification.clone(),
        pins: receipt.pins.clone(),
        analysis: receipt.analysis.clone(),
    };
    let value = serde_json::to_value(&rollback_receipt).expect("receipt serializes");
    let digest = wire::digest("adoption-receipt", &wire::without_member(&value, "receipt"));
    let rollback_receipt = AdoptionReceipt {
        receipt: digest,
        ..rollback_receipt
    };
    transaction::persist_receipt(&ctx.root, &receipt.plan, &rollback_receipt)?;
    Ok(rollback_receipt)
}

/// The deterministic bounded snapshot file name of one path.
fn snapshot_name(path: &str) -> String {
    version::snapshot_name(path)
}
