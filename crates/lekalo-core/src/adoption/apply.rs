//! Verified selective apply and exact rollback (issue #136, increment
//! I4): only reviewed bytes of one confirmed refactor plan install, and
//! only after the pinned verification specification really passed on
//! both sides in a run this boundary executed itself. Failure never
//! records completion; a rollback restores the exact prior bytes and
//! refuses when later edits conflict.
//!
//! Custody model (F11/X1/R1): verification authority is a run, not a
//! document. Apply re-executes the pinned specification through the
//! injected host capability over the exact current tree and the exact
//! candidate bytes — after every pin and the whole-tree state digest
//! still match — and admits only its own fresh run's measured
//! conclusion. An execution document is a derived record of one run;
//! no caller-authored verification bytes can authorize a write,
//! however internally consistent they are.

use super::analysis::{compute_pins, AdoptionContext, Outcome};
use super::diagnostic;
use super::transaction::{self, Journal, JournalStep, SnapshotRecord};
use super::verification::{self, VerificationHost};
use super::version;
use super::wire::{self, AdoptionReceipt, PatchDocument, RefactorPlan, VerificationDocument};
use super::Outcome as AdoptionOutcome;
use crate::observed::types::ObservedIndex;
use crate::result::DomainResult;

/// Apply one confirmed refactor plan through trusted verification at
/// the apply boundary (F11/X1/R1): the pinned specification is really
/// re-executed here, through the injected host capability, over the
/// exact current tree and the exact candidate bytes — the measured
/// result this run produces is the only verification authority, so a
/// caller-authored execution document, however internally consistent,
/// never reaches a write. The plan digest must name the exact current
/// plan bytes; every pinned input and the whole-tree state digest must
/// still match; the candidate patch must still hash to the plan's
/// pinned digest. Every write is snapshotted, journaled (with its exact
/// post-state), and installed through compare-and-swap.
pub fn apply(
    ctx: &AdoptionContext,
    host: &dyn VerificationHost,
    index: Option<&ObservedIndex>,
    plan_bytes: &[u8],
    patch_bytes: &[u8],
    spec_bytes: &[u8],
    confirm_digest: &str,
) -> Outcome<AdoptionReceipt> {
    let document =
        wire::refactor_plan_of_bytes(plan_bytes).map_err(|error| error.domain_result())?;
    // Shared cross-service writer serialization (r6, findings devin F2 +
    // omp F2): the adoption installer joins the one project-wide mutation
    // lock before any verification run, snapshot, or CAS install, so a
    // change/migration/format writer can never interleave with its
    // journaled writes. Refusals project through the adoption family's
    // own closed rules (r8, devin F4 + omp F4): the busy rule is
    // denied-allowed, sibling recovery rules ride at their admitted
    // status.
    let _shared = match crate::project_writer::WriterLock::mutation(
        &ctx.root,
        crate::project_writer::MutationFamily::Adoption,
    ) {
        Ok(lock) => lock,
        Err(refusal) => return Err(diagnostic::shared_writer_refusal(refusal)),
    };
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

    // The pinned verification specification — the same closed document
    // the plan bound — is the only verification input this boundary
    // accepts (F11/X1/R1). An execution document, a specification for
    // another plan, or a wrong-version file refuses here before any
    // barrier or run.
    let spec = wire::verification_of_bytes(spec_bytes).map_err(|error| error.domain_result())?;
    if spec.variant != "specification" {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "not-a-specification",
        )));
    }
    let spec_value = serde_json::to_value(&spec).expect("spec serializes");
    let spec_digest = wire::digest("adoption-verification", &spec_value);
    if document.verification.as_deref() != Some(spec_digest.as_str()) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "plan-mismatch",
        )));
    }

    // The candidate patch bytes: a closed, scope-contained document
    // revalidated against the plan's pinned digest and mutation scope.
    let patch = load_patch_for(ctx, &document, patch_bytes, patch_digest)?;

    // Re-fingerprint every consumed input before any run and any
    // write: the same consumed set the plan pinned — evidence ∪ patch
    // paths ∪ the specification's argv entries — plus the whole-tree
    // state digest, so a drifted tree never executes or installs
    // (F3/R2).
    let current = compute_pins(ctx, index, &consumed_sources(&document, &patch, &spec))?;
    let mut changed = transaction::changed_pins(&document.pins, &current);
    let tree_current = super::verification::tree_state_digest(&ctx.root)?;
    if document.tree.as_deref() != Some(tree_current.as_str()) {
        changed.push("verification-baseline-source".to_owned());
    }
    if !changed.is_empty() {
        return Err(DomainResult::denied(diagnostic::plan_stale_set(
            &document.plan,
            &changed,
        )));
    }

    // Trusted verification (F11/X1/R1): the specification is really
    // executed, right here, over this exact tree and candidate. Only
    // this run's measured conclusion can authorize the writes below —
    // binding the plan, the exact patch, the specification, the
    // interpreter digest pin, the consumed manifest, and the tree
    // state the barriers just proved.
    let execution = verification::execute(ctx, host, index, &document, spec_bytes, Some(&patch))?;
    if execution.conclusion.as_deref() != Some("preserved") {
        return Err(DomainResult::invalid(diagnostic::verification_failed_set(
            execution
                .oracle
                .as_ref()
                .map(|oracle| oracle.detail.as_str())
                .unwrap_or("conclusion"),
            &spec_digest,
        )));
    }
    if !run_covers_candidate(&execution, patch_digest) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "candidate-mismatch",
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
            after: entry.after.clone(),
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
            after: None,
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
        after: None,
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

/// The complete consumed input set of one plan plus its patch files
/// and the specification's argv entries: read scope ∪ mutation scope ∪
/// patch paths ∪ argv rel-paths. Absent files simply contribute no
/// pin; the plan pins exactly this set, so plan-time and apply-time
/// fingerprints can never structurally disagree.
fn consumed_sources(
    document: &RefactorPlan,
    patch: &PatchDocument,
    spec: &VerificationDocument,
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
    if let Some(tool) = &spec.tool {
        for arg in &tool.args {
            if version::is_rel_path(arg) {
                sources.insert(arg.clone());
            }
        }
    }
    sources
}

/// Whether the trusted run bound the candidate revision this apply
/// installs (the same disposable-copy token the run records).
fn run_covers_candidate(execution: &VerificationDocument, patch_digest: &str) -> bool {
    execution.confinement.iter().any(|token| {
        token
            .strip_prefix("disposable-copy:")
            .is_some_and(|bound| bound == patch_digest)
    })
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
    // Shared cross-service writer serialization (r8, devin F1/F3 + omp
    // F3): rollback is a MUTATING adoption writer — it joins the one
    // project-wide mutation lock for its whole conflict-preflight ->
    // restore window, so a change/migration/format/generate writer can
    // never interleave with the snapshotted restores. Refusals project
    // through the adoption family's closed rules.
    let _shared = match crate::project_writer::WriterLock::mutation(
        &ctx.root,
        crate::project_writer::MutationFamily::Adoption,
    ) {
        Ok(lock) => lock,
        Err(refusal) => return Err(diagnostic::shared_writer_refusal(refusal)),
    };
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
                after: None,
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
            after: None,
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
