//! Selective definition and binding confirmation (issue #136, increment
//! I2): one reviewed definition set plus at most one binding/candidate
//! pair becomes a closed canonical write plan with exact before/after
//! digests; only the reviewed plan digest authorizes application.
//!
//! Confirmation is not promotion: the plan carries the complete
//! revision binding (re-fingerprinted pins, journaled multi-file
//! updates, exact local snapshots) that the older promotion digest
//! alone never claimed. Re-analysis after confirmation preserves the
//! confirmed bytes and reports new discrepancies instead of
//! overwriting them.

use std::collections::BTreeSet;

use super::analysis::{compute_pins, scope_source_paths, AdoptionContext, Outcome};
use super::diagnostic;
use super::transaction::{self, Journal};
use super::version;
use super::wire::{
    self, AdoptionReceipt, BindingSelection, ConfirmationPlan, ConfirmationSelections, Unresolved,
    WriteEntry,
};
use crate::observed::types::ObservedIndex;
use crate::result::DomainResult;

/// The requested confirmation preview.
pub struct ConfirmRequest {
    /// The exact analysis bytes pinning the reviewed scope.
    pub analysis: Vec<u8>,
    /// The explicitly reviewed definition ids.
    pub definitions: Vec<String>,
    /// The one optional binding resolution.
    pub binding: Option<(String, String)>,
}

/// Build the confirmation plan (preview; no writes).
pub fn plan(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    request: &ConfirmRequest,
) -> Outcome<ConfirmationPlan> {
    let (analysis, analysis_digest) = super::analysis::load_analysis(&request.analysis)?;
    if request.definitions.is_empty() {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "definition",
            "none-selected",
        )));
    }
    if request.definitions.len() > version::MAX_DEFINITIONS {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "definition",
            "over-limit",
        )));
    }
    let index = index
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid_set("index", "missing")))?;
    let canonical_ids: BTreeSet<&str> = ctx
        .model
        .definitions
        .iter()
        .map(|definition| definition.id.as_str())
        .collect();

    // Every selected id must be a proposed definition of this exact
    // analysis; already-canonical definitions refuse as incompatible.
    let mut selected: Vec<String> = Vec::new();
    for id in &request.definitions {
        let proposed = analysis
            .definitions
            .iter()
            .find(|definition| &definition.id == id)
            .ok_or_else(|| {
                DomainResult::invalid(diagnostic::input_invalid_set(id, "not-in-analysis"))
            })?;
        if proposed.status != "proposed" {
            return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                id,
                "already-confirmed",
            )));
        }
        if canonical_ids.contains(id.as_str()) {
            return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                id,
                "already-canonical",
            )));
        }
        selected.push(id.clone());
    }
    selected.sort();
    selected.dedup();
    if selected.len() != request.definitions.len() {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "definition",
            "duplicate-selection",
        )));
    }

    // Render every selected definition against the canonical ids plus
    // the selection itself; a definition whose dependencies are neither
    // canonical nor selected stays unresolved and is never written.
    let plan_symbols = selected.clone();
    let mut writes: Vec<WriteEntry> = Vec::new();
    let mut unresolved: Vec<Unresolved> = Vec::new();
    let mut blocks: BTreeSet<String> = BTreeSet::new();
    let mut documents: Vec<(String, Vec<String>)> = Vec::new();
    for id in &plan_symbols {
        let Some(record) = index.symbol(id) else {
            unresolved.push(Unresolved {
                id: id.clone(),
                reason: "evidence-missing".to_owned(),
            });
            continue;
        };
        let resolved: BTreeSet<String> = canonical_ids
            .iter()
            .map(|id| (*id).to_owned())
            .chain(plan_symbols.iter().cloned())
            .collect();
        let Some(yaml) =
            crate::observed::promote::render_definition(record, index, &resolved, &plan_symbols)
        else {
            unresolved.push(Unresolved {
                id: id.clone(),
                reason: "dependency-not-selected".to_owned(),
            });
            continue;
        };
        let document = crate::observed::promote::document_path(record);
        if let Some(entry) = documents.iter_mut().find(|(path, _)| path == &document) {
            entry.1.push(yaml);
        } else {
            documents.push((document, vec![yaml]));
        }
        blocks.insert(id.clone());
    }
    documents.sort_by(|left, right| left.0.cmp(&right.0));
    for (document, mut document_blocks) in documents {
        document_blocks.sort();
        let content = render_document(&ctx.root, &document, &document_blocks)?;
        writes.push(transaction::write_entry(
            &ctx.root, document, content, "replace",
        ));
    }
    // A document with no prior bytes is a typed create: `before` stays
    // null and rollback removes the file again instead of restoring a
    // zero-byte ghost (F1). The old deleted-between-reads fixup only
    // fired on a race; the exact read is authoritative here.
    for write in &mut writes {
        if write.before.is_none() {
            write.action = "create".to_owned();
        }
    }

    // The one optional binding resolution.
    let binding = match &request.binding {
        Some((proposal_id, candidate)) => {
            let receipt = crate::observed::bindings::propose(&observed(ctx))
                .map_err(DomainResult::invalid)?;
            let proposal = receipt
                .proposals
                .iter()
                .find(|proposal| proposal.proposal == *proposal_id)
                .ok_or_else(|| {
                    DomainResult::invalid(diagnostic::input_invalid_set(
                        "binding",
                        "proposal-unknown",
                    ))
                })?;
            if !selected.contains(&proposal.symbol) {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "binding",
                    "symbol-not-selected",
                )));
            }
            if !proposal
                .candidates
                .iter()
                .any(|entry| entry.native == *candidate)
            {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "binding",
                    "candidate-unknown",
                )));
            }
            Some(BindingSelection {
                proposal: proposal_id.clone(),
                candidate: candidate.clone(),
                symbol: proposal.symbol.clone(),
            })
        }
        None => None,
    };

    // A plan must write something: an all-unresolved selection is a
    // refused scope, never an empty success.
    if writes.is_empty() && binding.is_none() {
        return Err(DomainResult::denied_json(
            serde_json::json!({ "unresolved": unresolved }).to_string(),
            "adoption confirm: nothing to write; every selection is unresolved".to_owned(),
            diagnostic::scope_refused_set("confirm", "nothing-to-write"),
        ));
    }

    // The exact pins: the selected definitions' sources plus every
    // canonical write path the plan will create (F4). Plan and apply
    // must compute the same consumed set — a created document simply
    // contributes no pin until it exists, exactly like apply-time.
    let records: Vec<crate::observed::types::SymbolRecord> = selected
        .iter()
        .filter_map(|id| index.symbol(id).cloned())
        .collect();
    let mut pin_paths = scope_source_paths(Some(index), &records);
    for write in &writes {
        pin_paths.insert(write.path.clone());
    }
    let pins = compute_pins(ctx, Some(index), &pin_paths)?;

    let project = analysis.project.clone();
    let selections = ConfirmationSelections {
        definitions: selected,
        binding: binding.clone(),
    };
    let partial = ConfirmationPlan {
        schema_version: version::CONFIRMATION_PLAN_SCHEMA_VERSION.to_owned(),
        identity: version::CONFIRMATION_PLAN_IDENTITY.to_owned(),
        plan: String::new(),
        project,
        analysis: Some(analysis_digest),
        selections,
        writes,
        pins,
        unresolved,
        rollback: super::wire::RollbackHome {
            journal: String::new(),
            snapshots: String::new(),
        },
    };
    let digest = plan_digest(&partial);
    let rollback = rollback_home(&digest);
    Ok(ConfirmationPlan {
        plan: digest.clone(),
        rollback,
        ..partial
    })
}

/// Apply one reviewed confirmation plan. The digest must name the exact
/// current plan bytes and every pinned input must still match; the
/// canonical documents install through journaled compare-and-swap
/// writes and the receipt lands in the import area.
pub fn apply(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    plan_bytes: &[u8],
    confirm_digest: &str,
) -> Outcome<AdoptionReceipt> {
    let document =
        wire::confirmation_plan_of_bytes(plan_bytes).map_err(|error| error.domain_result())?;
    let value = serde_json::to_value(&document).expect("plan serializes");
    let recomputed = wire::digest(
        "adoption-confirmation-plan",
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
    let index = index
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid_set("index", "missing")))?;

    // Re-fingerprint every consumed input before any write: the same
    // consumed set the plan pinned — evidence sources plus the planned
    // write paths (F4). The plan's `pins.sources` never carries pins
    // for not-yet-existing documents; apply recomputes over the same
    // set, so a clean tree is never structurally stale.
    let records: Vec<crate::observed::types::SymbolRecord> = document
        .selections
        .definitions
        .iter()
        .filter_map(|id| index.symbol(id).cloned())
        .collect();
    let mut pin_paths = scope_source_paths(Some(index), &records);
    for write in &document.writes {
        pin_paths.insert(write.path.clone());
    }
    let current = compute_pins(ctx, Some(index), &pin_paths)?;
    let changed = transaction::changed_pins(&document.pins, &current);
    if !changed.is_empty() {
        return Err(DomainResult::denied(diagnostic::plan_stale_set(
            &document.plan,
            &changed,
        )));
    }

    // The write set: canonical documents plus, when a binding is
    // confirmed, the observed index as a first-class compare-and-swap
    // write entry with exact before/after digests (F2/X7).
    let mut writes: Vec<wire::WriteEntry> = document.writes.clone();
    if let Some(binding) = &document.selections.binding {
        writes.push(index_write_entry(ctx, index, binding)?);
    }
    let mut targets: Vec<String> = writes.iter().map(|w| w.path.clone()).collect();
    targets.sort();
    targets.dedup();

    let Some(home) = transaction::recovery_home(&ctx.root, &document.plan) else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "plan",
            "invalid-digest",
        )));
    };
    let mut journal = Journal::new(&document.plan, "confirmation");
    let snapshots = transaction::snapshot_targets(&ctx.root, &home, &targets, &mut journal)?;
    let snapshot_names: Vec<String> = snapshots
        .iter()
        .map(|record| record.snapshot.clone())
        .collect();

    // Step 1: the binding resolution. The index mutates through the
    // same compare-and-swap discipline as every canonical write: the
    // live index bytes must still match the plan's recorded `before`
    // digest, then the exact planned bytes rename into place. A failure
    // here rolls the tree back and refuses with the typed recovery
    // path; the index is never left mutated without a receipt naming
    // its restoration.
    for write in writes.iter().filter(|write| write.action == "binding") {
        if let Err(result) = install_binding(ctx, index, write, &mut journal, &home, &snapshots) {
            return Err(rollback_partial(
                ctx,
                &home,
                &journal,
                &document.plan,
                &snapshots,
                result,
            ));
        }
    }

    // Step 2: the canonical documents, one compare-and-swap write per
    // path, journaled as they complete. A failure rolls every completed
    // byte (binding included) back and refuses. Every written step
    // carries its full restoration metadata so an interrupted
    // confirmation stranded by a later receipt failure is recoverable
    // from the journal alone (X5).
    for write in &document.writes {
        if let Err(result) = transaction::cas_install(&ctx.root, write) {
            return Err(rollback_partial(
                ctx,
                &home,
                &journal,
                &document.plan,
                &snapshots,
                result,
            ));
        }
        let snapshot = snapshots
            .iter()
            .find(|record| record.path == write.path)
            .map(|record| record.snapshot.clone());
        journal.steps.push(super::transaction::JournalStep {
            path: write.path.clone(),
            state: "written".to_owned(),
            snapshot,
            action: Some(write.action.clone()),
            before: write.before.clone(),
        });
        transaction::persist_journal(&ctx.root, &home, &journal)?;
    }

    // The receipt carries the complete mutation set — canonical writes
    // plus the binding-index entry — so conflict checks and rollback
    // cover every byte the confirmation touched (F2/X7).
    let receipt = AdoptionReceipt {
        schema_version: version::RECEIPT_SCHEMA_VERSION.to_owned(),
        identity: version::RECEIPT_IDENTITY.to_owned(),
        receipt: String::new(),
        phase: "confirmation".to_owned(),
        project: document.project.clone(),
        plan: document.plan.clone(),
        outcome: "completed".to_owned(),
        writes,
        journal: Some(home.journal_relative.clone()),
        snapshots: snapshot_names,
        verification: None,
        pins: current,
        analysis: document.analysis.clone(),
    };
    let value = serde_json::to_value(&receipt).expect("receipt serializes");
    let digest = wire::digest("adoption-receipt", &wire::without_member(&value, "receipt"));
    let receipt = AdoptionReceipt {
        receipt: digest,
        ..receipt
    };
    // The receipt is inside the recoverable transaction (X5): a
    // persistence failure is a classified recovery-required refusal —
    // the journal above names every stranded step with its snapshot and
    // pre-state, so recovery can complete without this receipt.
    transaction::persist_receipt(&ctx.root, &document.plan, &receipt)?;
    Ok(receipt)
}

fn observed(ctx: &AdoptionContext) -> crate::observed::ObservedContext {
    crate::observed::ObservedContext {
        model: ctx.model.clone(),
        root: ctx.root.clone(),
    }
}

/// The index path of the observed seam (the exact store the bindings
/// module saves through).
fn index_relative_path() -> String {
    format!(
        "{}/{}",
        crate::observed::version::INDEX_DIR,
        crate::observed::version::INDEX_NAME
    )
}

/// Build the first-class binding-index write entry (F2/X7): derive the
/// exact post-confirmation index bytes by re-running the accepted
/// bindings confirmation over the loaded index (in memory only), then
/// record the canonical-JSON bytes with the exact live `before` digest
/// and planned `after` digest. The proposed-receipt resolution already
/// validated the proposal/candidate pair at plan time; this recomputes
/// over the same inputs so plan and apply cannot disagree.
fn index_write_entry(
    ctx: &AdoptionContext,
    index: &ObservedIndex,
    binding: &wire::BindingSelection,
) -> Outcome<wire::WriteEntry> {
    // Resolve the exact proposal the plan names from the live proposals
    // of this index; the candidate must be a member (same admission as
    // bindings::confirm, without persisting anything yet).
    let resolved = render_confirmed_index(ctx, index, binding)?;
    let content =
        crate::observed::index::canonical_bytes(&resolved).map_err(DomainResult::invalid)?;
    let before = std::fs::read(ctx.root.join(index_relative_path())).ok();
    Ok(wire::WriteEntry {
        path: index_relative_path(),
        action: "binding".to_owned(),
        before: before
            .as_ref()
            .map(|bytes| format!("sha256:{}", crate::digest::sha256_hex(bytes))),
        after: Some(format!(
            "sha256:{}",
            crate::digest::sha256_hex(content.as_bytes())
        )),
        content: Some(content),
    })
}

/// The exact post-confirmation index bytes: the loaded index with the
/// one reviewed binding applied in memory (the same mutation the
/// accepted bindings::confirm performs), never persisted directly.
fn render_confirmed_index(
    ctx: &AdoptionContext,
    index: &ObservedIndex,
    binding: &wire::BindingSelection,
) -> Outcome<ObservedIndex> {
    let proposals = crate::observed::bindings::proposals_of(index);
    let proposal = proposals
        .iter()
        .find(|proposal| proposal.proposal == binding.proposal)
        .cloned()
        .ok_or_else(|| {
            DomainResult::invalid(diagnostic::input_invalid_set("binding", "proposal-unknown"))
        })?;
    if proposal.symbol != binding.symbol {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "binding",
            "symbol-mismatch",
        )));
    }
    let record = index
        .symbols
        .iter()
        .find(|record| record.id == proposal.symbol)
        .cloned()
        .ok_or_else(|| {
            DomainResult::invalid(diagnostic::input_invalid_set("binding", "symbol-missing"))
        })?;
    let candidates = crate::observed::bindings::ranked_candidates(&record);
    let chosen = candidates
        .iter()
        .find(|candidate| candidate.native == binding.candidate)
        .cloned()
        .ok_or_else(|| {
            DomainResult::invalid(diagnostic::input_invalid_set(
                "binding",
                "candidate-unknown",
            ))
        })?;
    let mut owned = index.clone();
    crate::observed::bindings::apply_confirmation(&mut owned, &proposal, &chosen).ok_or_else(
        || DomainResult::invalid(diagnostic::input_invalid_set("binding", "proposal-unknown")),
    )?;
    let _ = ctx;
    Ok(owned)
}

/// Install the binding-index write entry through compare-and-swap, then
/// journal the completed step. The planned bytes are the exact
/// canonical index the plan derived; the live bytes must still match
/// the plan's `before` digest.
fn install_binding(
    ctx: &AdoptionContext,
    index: &ObservedIndex,
    write: &wire::WriteEntry,
    journal: &mut Journal,
    home: &transaction::RecoveryHome,
    snapshots: &[transaction::SnapshotRecord],
) -> Outcome<()> {
    transaction::cas_install(&ctx.root, write)?;
    let _ = index;
    let snapshot = snapshots
        .iter()
        .find(|record| record.path == write.path)
        .map(|record| record.snapshot.clone());
    journal.steps.push(super::transaction::JournalStep {
        path: write.path.clone(),
        state: "written".to_owned(),
        snapshot,
        action: Some(write.action.clone()),
        before: write.before.clone(),
    });
    transaction::persist_journal(&ctx.root, home, journal)
}

/// Roll a partially applied confirmation back to its exact prior state
/// and surface the typed recovery-required refusal. Every snapshotted
/// target regains its exact prior bytes (or is removed again); the
/// durable journal records the rollback so the partial mutation is
/// never invisible (F2/X4/X5).
fn rollback_partial(
    ctx: &AdoptionContext,
    home: &transaction::RecoveryHome,
    journal: &Journal,
    plan: &str,
    snapshots: &[transaction::SnapshotRecord],
    cause: DomainResult,
) -> DomainResult {
    let mut recovery = journal.clone();
    recovery.phase = "confirmation-rollback".to_owned();
    for record in snapshots {
        let restored = transaction::cas_restore(&ctx.root, record);
        recovery.steps.push(super::transaction::JournalStep {
            path: record.path.clone(),
            state: if restored.is_ok() {
                "restored".to_owned()
            } else {
                "restore-failed".to_owned()
            },
            snapshot: Some(record.snapshot.clone()),
            action: None,
            before: None,
        });
    }
    let _ = transaction::persist_journal(&ctx.root, home, &recovery);
    if recovery
        .steps
        .iter()
        .any(|step| step.state == "restore-failed")
    {
        return DomainResult::invalid(diagnostic::recovery_required_set(plan, "rollback"));
    }
    cause
}

/// The plan identity: the family-specific content address over the whole
/// document without its self-digest member.
pub fn plan_digest(document: &ConfirmationPlan) -> String {
    let value = serde_json::to_value(document).expect("plan serializes");
    wire::digest(
        "adoption-confirmation-plan",
        &wire::without_member(&wire::without_member(&value, "plan"), "rollback"),
    )
}

fn rollback_home(digest: &str) -> super::wire::RollbackHome {
    let (head, tail) = version::digest_dir(digest).unwrap_or_default();
    super::wire::RollbackHome {
        journal: format!("{}/{}/{}/journal.json", version::RECOVERY_DIR, head, tail),
        snapshots: format!("{}/{}/{}/snapshots", version::RECOVERY_DIR, head, tail),
    }
}

/// Render one canonical kind document: existing bytes with the planned
/// definition blocks appended, or the canonical header when absent.
fn render_document(root: &std::path::Path, document: &str, blocks: &[String]) -> Outcome<String> {
    let existing = std::fs::read_to_string(root.join(document)).ok();
    let mut text = match existing {
        Some(text) => {
            let trimmed = text.trim_end_matches(['\n', '\r']);
            let last = trimmed
                .lines()
                .last()
                .map(|line| line.trim_start())
                .unwrap_or_default();
            if !last.starts_with("- ") && !last.starts_with("definitions:") {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    document,
                    "document-shape",
                )));
            }
            format!("{trimmed}\n")
        }
        None => "schema_version: \"0.2.16\"\ndefinitions:\n".to_owned(),
    };
    for block in blocks {
        text.push_str(block);
        text.push('\n');
    }
    Ok(text)
}
