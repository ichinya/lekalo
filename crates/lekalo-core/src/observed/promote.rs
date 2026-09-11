//! The explicit observed → contracted promotion workflow (issue #39).
//!
//! Promotion is the authority brownfield-adoption action: observed facts
//! (explicit or confirmed bindings with current evidence) become canonical
//! Model definitions through a planned and explicitly confirmed two-phase
//! write. Silent promotion is forbidden; inferred facts never promote;
//! unknown evidence refuses; policy definitions refuse (their
//! `applies_to`/`decision` semantics cannot come from scan evidence
//! without over-inference). The write touches only Lekalo-owned canonical
//! model documents — never a source file.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::types::{
    BindingState, BindingStatus, HistoryEntry, HistoryEvent, PromotionReceipt, ReferenceRole,
    SymbolKind,
};
use super::version;
use super::{index, types};

/// What to promote: one symbol, or every eligible symbol of one module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PromotionSelection {
    Symbol(String),
    Module(String),
}

/// One planned canonical document write.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanEntry {
    pub symbol: String,
    pub document: String,
    /// The exact YAML block appended to the document (no trailing LF).
    pub yaml: String,
}

/// The receipt of `observe promote --dry-run`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PromotionPlanReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub phase: &'static str,
    pub mode: &'static str,
    pub plan: String,
    pub symbols: Vec<String>,
    pub documents: Vec<String>,
    pub entries: Vec<PlanEntry>,
    pub ineligible: Vec<Ineligible>,
}

/// One symbol that cannot promote, with the fixed refusal reason.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Ineligible {
    pub symbol: String,
    pub detail: &'static str,
}

/// The receipt of `observe promote --confirm <plan-id>`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PromotionApplyReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub phase: &'static str,
    pub mode: &'static str,
    pub plan: String,
    pub symbols: Vec<String>,
    pub documents: Vec<String>,
}

/// Compute the promotion plan for one selection without writing.
pub fn plan(
    ctx: &super::ObservedContext,
    selection: &PromotionSelection,
) -> Result<PromotionPlanReceipt, DiagnosticSet> {
    let index = index::load_index(ctx)?.ok_or_else(|| diagnostic::index_missing_set("promote"))?;
    let records = select_records(&index, selection)?;
    let canonical_ids = canonical_id_set(ctx);
    let mut symbols: Vec<String> = Vec::new();
    let mut entries: Vec<PlanEntry> = Vec::new();
    let mut ineligible: Vec<Ineligible> = Vec::new();
    // Pass one: eligibility. Every eligible id joins the resolution set
    // before any rendering, so intra-plan references resolve regardless
    // of record order.
    let mut eligible: Vec<&types::SymbolRecord> = Vec::new();
    for record in &records {
        if let Err(detail) = eligibility(record, &index) {
            ineligible.push(Ineligible {
                symbol: record.id.clone(),
                detail,
            });
            continue;
        }
        symbols.push(record.id.clone());
        eligible.push(record);
    }
    // Pass two: rendering against canonical ids plus the full plan set.
    // A rendering failure removes the symbol from the plan entirely: it
    // is recorded only as ineligible, so the plan never advertises a
    // symbol that would write nothing.
    for record in eligible {
        let Some(yaml) = render_definition(record, &index, &canonical_ids, &symbols) else {
            symbols.retain(|id| id != &record.id);
            ineligible.push(Ineligible {
                symbol: record.id.clone(),
                detail: "unresolved-reference",
            });
            continue;
        };
        entries.push(PlanEntry {
            symbol: record.id.clone(),
            document: document_path(record),
            yaml,
        });
    }
    // The planned symbol set must be exactly the rendered set — never
    // empty, and never a symbol that failed rendering — so a
    // partially-written plan (symbols without entries, or an
    // all-ineligible plan) cannot exist.
    if symbols.is_empty() || symbols.len() != entries.len() {
        return Err(diagnostic::promotion_refused_set(
            selection_label(selection),
            "no-eligible-symbols",
        ));
    }
    entries.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    let mut documents: Vec<String> = entries.iter().map(|entry| entry.document.clone()).collect();
    documents.sort();
    documents.dedup();
    let plan = plan_digest(&index, &entries);
    Ok(PromotionPlanReceipt {
        status: "valid",
        operation: "promote",
        phase: "plan",
        mode: version::MODE,
        plan,
        symbols,
        documents,
        entries,
        ineligible,
    })
}

/// Apply one exact plan: revalidate, write the canonical documents, mark
/// the records promoted with their adoption receipts, and persist.
pub fn apply(
    ctx: &super::ObservedContext,
    selection: &PromotionSelection,
    plan_id: &str,
) -> Result<PromotionApplyReceipt, DiagnosticSet> {
    let expected = plan(ctx, selection)?;
    if expected.plan != plan_id {
        return Err(diagnostic::promotion_plan_mismatch_set("plan-id"));
    }
    let mut index =
        index::load_index(ctx)?.ok_or_else(|| diagnostic::index_missing_set("promote"))?;
    let mut documents: HashMap<String, Vec<String>> = HashMap::new();
    for entry in &expected.entries {
        documents
            .entry(entry.document.clone())
            .or_default()
            .push(entry.yaml.clone());
    }
    let mut paths: Vec<&String> = documents.keys().collect();
    paths.sort();
    for path in paths {
        append_document(ctx, path, &documents[path.as_str()])?;
    }
    // Defense in depth: a confirmed apply must never claim success for a
    // plan that writes nothing. Every promoted symbol must have exactly
    // one written canonical entry behind it.
    let written: BTreeSet<&str> = expected
        .entries
        .iter()
        .map(|entry| entry.symbol.as_str())
        .collect();
    if written.len() != expected.symbols.len() {
        return Err(diagnostic::promotion_refused_set(
            selection_label(selection),
            "nothing-written",
        ));
    }
    for symbol in &expected.symbols {
        let record = index
            .symbols
            .iter_mut()
            .find(|record| record.id == *symbol)
            .expect("planned record exists");
        record.mark_promoted(PromotionReceipt {
            plan: expected.plan.clone(),
            revision: index.revision.clone(),
            adapter: index.adapter.id.clone(),
        });
        record.state = BindingState::Current;
        record.history.push(HistoryEntry {
            event: HistoryEvent::Promoted,
            revision: index.revision.clone(),
            from: None,
            to: None,
        });
        trim_history(record);
    }
    index.symbols.sort_by(|left, right| left.id.cmp(&right.id));
    index::save_index(ctx, &index)?;
    Ok(PromotionApplyReceipt {
        status: "valid",
        operation: "promote",
        phase: "apply",
        mode: version::MODE,
        plan: expected.plan,
        symbols: expected.symbols,
        documents: expected.documents,
    })
}

// ---------------------------------------------------------------------------
// Eligibility and rendering
// ---------------------------------------------------------------------------

/// The fixed eligibility decision for one record; `Err` is the bounded
/// refusal reason.
fn eligibility(
    record: &types::SymbolRecord,
    index: &types::ObservedIndex,
) -> Result<(), &'static str> {
    if !record.kind.promotable() {
        return Err("unsupported-kind");
    }
    if record.promoted {
        return Err("already-promoted");
    }
    if record.state != BindingState::Current {
        return Err("binding-not-current");
    }
    if !matches!(
        record.status,
        BindingStatus::Explicit | BindingStatus::Confirmed
    ) {
        return Err("inferred-binding");
    }
    match record.kind {
        SymbolKind::Scalar if record.evidence.base.is_some() => Ok(()),
        SymbolKind::Scalar => Err("unknown-scalar-base"),
        SymbolKind::Enum if !record.evidence.values.is_empty() => Ok(()),
        SymbolKind::Enum => Err("unknown-enum-values"),
        SymbolKind::ValueObject if !record.evidence.fields.is_empty() => Ok(()),
        SymbolKind::ValueObject => Err("unknown-fields"),
        SymbolKind::Entity => {
            if record.evidence.fields.is_empty() {
                Err("unknown-fields")
            } else if record.evidence.identity_fields.is_empty() {
                Err("unknown-identity")
            } else {
                Ok(())
            }
        }
        SymbolKind::Query => {
            if reads_of(record).is_empty() {
                Err("unknown-reads")
            } else {
                Ok(())
            }
        }
        SymbolKind::Effect => {
            if writes_of(record).is_empty() {
                Err("unknown-effect-target")
            } else {
                Ok(())
            }
        }
        SymbolKind::Endpoint => {
            if endpoint_of(index, &record.id).is_some() {
                Ok(())
            } else {
                Err("unknown-endpoint-route")
            }
        }
        SymbolKind::Command | SymbolKind::Event | SymbolKind::Policy => Ok(()),
    }
}

/// Render the canonical YAML block for one record; `None` when a
/// reference resolves neither canonically nor inside the plan.
fn render_definition(
    record: &types::SymbolRecord,
    index: &types::ObservedIndex,
    canonical_ids: &BTreeSet<String>,
    plan_symbols: &[String],
) -> Option<String> {
    let resolves = |target: &str| -> bool {
        canonical_ids.contains(target) || plan_symbols.iter().any(|id| id == target)
    };
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("  - id: {}", record.id));
    lines.push(format!("    kind: {}", record.kind.key()));
    lines.push("    version: 1".to_owned());
    match record.kind {
        SymbolKind::Scalar => {
            lines.push(format!("    base: {}", record.evidence.base.as_deref()?));
        }
        SymbolKind::Enum => {
            lines.push("    values:".to_owned());
            for value in &record.evidence.values {
                lines.push(format!("      - value: {}", value.value));
                if let Some(description) = &value.description {
                    lines.push(format!("        description: \"{description}\""));
                }
            }
        }
        SymbolKind::ValueObject | SymbolKind::Entity => {
            lines.push("    fields:".to_owned());
            for field in &record.evidence.fields {
                if !resolves(&field.r#type) {
                    return None;
                }
                lines.push(format!("      - name: {}", field.name));
                lines.push(format!("        type: \"{}\"", field.r#type));
                if field.required {
                    lines.push("        required: true".to_owned());
                }
            }
            if record.kind == SymbolKind::Entity {
                lines.push("    identity:".to_owned());
                for name in &record.evidence.identity_fields {
                    lines.push(format!("      - {name}"));
                }
            }
        }
        SymbolKind::Query => {
            lines.push("    reads:".to_owned());
            for read in reads_of(record) {
                if !resolves(&read) {
                    return None;
                }
                lines.push(format!("      - {read}"));
            }
        }
        SymbolKind::Effect => {
            let (entity, operation) = writes_of(record).first()?.clone();
            if !resolves(&entity) {
                return None;
            }
            lines.push(format!("    operation: {operation}"));
            lines.push(format!("    entity: {entity}"));
        }
        SymbolKind::Endpoint => {
            let endpoint = endpoint_of(index, &record.id)?;
            if !resolves(&endpoint.symbol) {
                return None;
            }
            lines.push(format!("    method: {}", endpoint.method));
            lines.push(format!("    path: {}", endpoint.path));
            lines.push(format!("    invokes: {}", endpoint.symbol));
        }
        SymbolKind::Command | SymbolKind::Event | SymbolKind::Policy => {}
    }
    Some(lines.join("\n"))
}

/// The deduplicated read targets of a query record.
fn reads_of(record: &types::SymbolRecord) -> Vec<String> {
    let mut reads: Vec<String> = record
        .evidence
        .references
        .iter()
        .filter(|reference| reference.role == ReferenceRole::Read)
        .map(|reference| reference.target.clone())
        .collect();
    reads.sort();
    reads.dedup();
    reads
}

/// The deduplicated write targets of an effect record, as
/// `(entity, operation)` pairs in canonical order.
fn writes_of(record: &types::SymbolRecord) -> Vec<(String, &'static str)> {
    let mut writes: Vec<(String, &'static str)> = record
        .evidence
        .references
        .iter()
        .filter_map(|reference| match reference.role {
            ReferenceRole::Create => Some((reference.target.clone(), "create")),
            ReferenceRole::Update => Some((reference.target.clone(), "update")),
            ReferenceRole::Delete => Some((reference.target.clone(), "delete")),
            _ => None,
        })
        .collect();
    writes.sort();
    writes.dedup();
    writes
}

/// The endpoint route bound to a symbol, if any.
fn endpoint_of<'a>(
    index: &'a types::ObservedIndex,
    symbol: &str,
) -> Option<&'a types::EndpointRecord> {
    index
        .endpoints
        .iter()
        .find(|endpoint| endpoint.symbol == symbol)
}

// ---------------------------------------------------------------------------
// Plan plumbing
// ---------------------------------------------------------------------------

fn select_records(
    index: &types::ObservedIndex,
    selection: &PromotionSelection,
) -> Result<Vec<types::SymbolRecord>, DiagnosticSet> {
    match selection {
        PromotionSelection::Symbol(symbol) => index
            .symbol(symbol)
            .cloned()
            .map(|record| vec![record])
            .ok_or_else(|| diagnostic::unknown_symbol_set(symbol)),
        PromotionSelection::Module(module) => {
            let mut records: Vec<types::SymbolRecord> = index
                .symbols
                .iter()
                .filter(|record| {
                    types::ObservedIndex::module_of(&record.id) == Some(module.as_str())
                })
                .cloned()
                .collect();
            if records.is_empty() {
                return Err(diagnostic::unknown_module_set(module));
            }
            records.sort_by(|left, right| left.id.cmp(&right.id));
            Ok(records)
        }
    }
}

fn selection_label(selection: &PromotionSelection) -> &str {
    match selection {
        PromotionSelection::Symbol(symbol) => symbol,
        PromotionSelection::Module(module) => module,
    }
}

fn canonical_id_set(ctx: &super::ObservedContext) -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for definition in &ctx.model.definitions {
        ids.insert(definition.id.clone());
    }
    for module in &ctx.model.modules {
        ids.insert(module.id.clone());
    }
    if let Some(project) = &ctx.model.project {
        ids.insert(project.id.clone());
    }
    ids
}

fn document_path(record: &types::SymbolRecord) -> String {
    let module = types::ObservedIndex::module_of(&record.id).unwrap_or_default();
    format!("lekalo/modules/{module}/{}.yaml", record.kind.stem())
}

/// The plan identity: sha256 over the canonical payload (the index
/// digest, then every document path with its exact YAML block, in plan
/// order). Never over absolute paths or timestamps.
fn plan_digest(index: &types::ObservedIndex, entries: &[PlanEntry]) -> String {
    let mut payload = index::index_digest(index).unwrap_or_default();
    payload.push('\n');
    for entry in entries {
        payload.push_str(&entry.document);
        payload.push('\u{1f}');
        payload.push_str(&entry.yaml);
        payload.push('\n');
    }
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// Append planned definition blocks to one canonical kind document,
/// creating it with the canonical header when absent. The write goes
/// through the confined store.
fn append_document(
    ctx: &super::ObservedContext,
    document: &str,
    blocks: &[String],
) -> Result<(), DiagnosticSet> {
    let mut path = ctx.root.clone();
    for segment in document.split('/') {
        path.push(segment);
    }
    let existing = std::fs::read_to_string(&path).ok();
    let mut text = match existing {
        Some(text) => {
            let trimmed = text.trim_end_matches(['\n', '\r']);
            let last = trimmed
                .lines()
                .last()
                .map(|line| line.trim_start())
                .unwrap_or_default();
            if !last.starts_with("- ") && !last.starts_with("definitions:") {
                return Err(diagnostic::promotion_plan_mismatch_set("document-shape"));
            }
            format!("{trimmed}\n")
        }
        None => "schema_version: \"1.0.0\"\ndefinitions:\n".to_owned(),
    };
    for block in blocks {
        text.push_str(block);
        text.push('\n');
    }
    let name = document.rsplit('/').next().unwrap_or("definitions.yaml");
    let dir = document
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_owned())
        .ok_or_else(|| diagnostic::index_io_set("path-escapes-root"))?;
    super::store::write_confined(&ctx.root, &dir, name, text.as_bytes())
}

/// Keep the bounded history tail (the oldest entries fall off).
fn trim_history(record: &mut types::SymbolRecord) {
    if record.history.len() > version::MAX_HISTORY {
        let excess = record.history.len() - version::MAX_HISTORY;
        record.history.drain(..excess);
    }
}
