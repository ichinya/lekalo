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
    // Created canonical documents have no prior bytes: before stays
    // typed-null and the write creates the file.
    for write in &mut writes {
        if write.before.is_some() && std::fs::metadata(ctx.root.join(&write.path)).is_err() {
            write.action = "create".to_owned();
            write.before = None;
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

    // The exact pins: the selected definitions' sources, the model, the
    // lock, the index digest, and the adapter.
    let records: Vec<crate::observed::types::SymbolRecord> = selected
        .iter()
        .filter_map(|id| index.symbol(id).cloned())
        .collect();
    let pins = compute_pins(ctx, Some(index), &scope_source_paths(Some(index), &records))?;

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
    let document = wire::confirmation_plan_of_bytes(plan_bytes)
        .map_err(|error| DomainResult::invalid(error.set()))?;
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

    // Re-fingerprint every consumed input before any write.
    let records: Vec<crate::observed::types::SymbolRecord> = document
        .selections
        .definitions
        .iter()
        .filter_map(|id| index.symbol(id).cloned())
        .collect();
    let current = compute_pins(ctx, Some(index), &scope_source_paths(Some(index), &records))?;
    let changed = transaction::changed_pins(&document.pins, &current);
    if !changed.is_empty() {
        return Err(DomainResult::denied(diagnostic::plan_stale_set(
            &document.plan,
            &changed,
        )));
    }

    // The write set: canonical documents plus the observed index when a
    // binding is confirmed.
    let mut targets: Vec<String> = document.writes.iter().map(|w| w.path.clone()).collect();
    if document.selections.binding.is_some() {
        targets.push(format!(
            "{}/{}",
            crate::observed::version::INDEX_DIR,
            crate::observed::version::INDEX_NAME
        ));
    }
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
    let mut snapshot_names: Vec<String> = snapshots
        .iter()
        .map(|record| record.snapshot.clone())
        .collect();

    // Step 1: the binding resolution (the index mutates first so a
    // canonical write failure can still roll both back).
    if let Some(binding) = &document.selections.binding {
        if let Err(set) = crate::observed::bindings::confirm(
            &observed(ctx),
            &binding.proposal,
            Some(&binding.candidate),
        ) {
            journal.steps.push(super::transaction::JournalStep {
                path: "bindings".to_owned(),
                state: "failed".to_owned(),
                snapshot: None,
            });
            let _ = transaction::persist_journal(&ctx.root, &home, &journal);
            return Err(DomainResult::invalid(set));
        }
        journal.steps.push(super::transaction::JournalStep {
            path: "bindings".to_owned(),
            state: "written".to_owned(),
            snapshot: None,
        });
        let _ = transaction::persist_journal(&ctx.root, &home, &journal);
    }

    // Step 2: the canonical documents, one compare-and-swap write per
    // path, journaled as they complete.
    for write in &document.writes {
        if let Err(result) = transaction::cas_install(&ctx.root, write) {
            journal.steps.push(super::transaction::JournalStep {
                path: write.path.clone(),
                state: "failed".to_owned(),
                snapshot: None,
            });
            let _ = transaction::persist_journal(&ctx.root, &home, &journal);
            return Err(result);
        }
        journal.steps.push(super::transaction::JournalStep {
            path: write.path.clone(),
            state: "written".to_owned(),
            snapshot: None,
        });
        let _ = transaction::persist_journal(&ctx.root, &home, &journal);
    }

    let receipt = AdoptionReceipt {
        schema_version: version::RECEIPT_SCHEMA_VERSION.to_owned(),
        identity: version::RECEIPT_IDENTITY.to_owned(),
        receipt: String::new(),
        phase: "confirmation".to_owned(),
        project: document.project.clone(),
        plan: document.plan.clone(),
        outcome: "completed".to_owned(),
        writes: document.writes.clone(),
        journal: Some(home.journal_relative.clone()),
        snapshots: std::mem::take(&mut snapshot_names),
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
    transaction::persist_receipt(&ctx.root, &document.plan, &receipt)?;
    Ok(receipt)
}

fn observed(ctx: &AdoptionContext) -> crate::observed::ObservedContext {
    crate::observed::ObservedContext {
        model: ctx.model.clone(),
        root: ctx.root.clone(),
    }
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
