//! Incremental, in-memory typed IR fragments. Each accepted normalized
//! definition decodes independently; its source-map ordinal is rebased
//! on assembly, so insertion/deletion never preserves stale bindings.
//! Any fragment refusal falls back to the full accepted compiler to
//! preserve its complete diagnostic set, not just the first failure.
use crate::ir::{Compilation, CompiledProject, SourceMap};
use crate::loader::{NormalizedDefinition, NormalizedModel, SourceMapEntry};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct Fragment {
    normalized: NormalizedDefinition,
    compiled: Compilation,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Fragments {
    rows: BTreeMap<String, Fragment>,
}
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineCounts {
    pub decoded_documents: usize,
    pub reused_documents: usize,
    pub decoded_definitions: usize,
    pub reused_definitions: usize,
    pub emitted_graph_units: usize,
    pub reused_graph_units: usize,
    /// Canonical assembly and cycle policy remain global obligations.
    pub assembled_graph_nodes: usize,
    pub assembled_graph_edges: usize,
    pub graph_cycle_passes: usize,
}

impl Fragments {
    pub(crate) fn compile(
        model: &NormalizedModel,
        previous: &Self,
    ) -> Result<(Compilation, Self, PipelineCounts), crate::ir::diagnostic::IrFailure> {
        let mut next = Self::default();
        let mut counts = PipelineCounts::default();
        let mut project = None;
        let mut modules = Vec::new();
        let mut definitions = Vec::new();
        let mut entries = Vec::new();
        let units = model
            .project
            .iter()
            .map(|d| (d, "project", 0))
            .chain(
                model
                    .modules
                    .iter()
                    .enumerate()
                    .map(|(i, d)| (d, "modules", i)),
            )
            .chain(
                model
                    .definitions
                    .iter()
                    .enumerate()
                    .map(|(i, d)| (d, "definitions", i)),
            );
        for (normalized, kind, ordinal) in units {
            let reused = previous.rows.get(&normalized.id).filter(|old| {
                old.compiled.project.model_version == model.model_version
                    && old.normalized.path == normalized.path
                    && old.normalized.node == normalized.node
                    && old.normalized.span == normalized.span
            });
            let fragment = if let Some(old) = reused {
                counts.reused_definitions += 1;
                old.clone()
            } else {
                let mut partial = NormalizedModel {
                    model_version: model.model_version,
                    project: None,
                    modules: Vec::new(),
                    definitions: Vec::new(),
                };
                match kind {
                    "project" => partial.project = Some(normalized.clone()),
                    "modules" => partial.modules.push(normalized.clone()),
                    _ => partial.definitions.push(normalized.clone()),
                }
                let compiled = match crate::ir::compile(&partial) {
                    Ok(compiled) => compiled,
                    Err(_) => {
                        return crate::ir::compile(model)
                            .map(|full| (full, Self::default(), PipelineCounts::default()))
                    }
                };
                counts.decoded_definitions += 1;
                Fragment {
                    normalized: normalized.clone(),
                    compiled,
                }
            };
            if kind == "project" {
                project = fragment.compiled.project.project.clone();
            }
            modules.extend(fragment.compiled.project.modules.iter().cloned());
            definitions.extend(fragment.compiled.project.definitions.iter().cloned());
            for entry in fragment.compiled.source_map.entries() {
                let mut entry: SourceMapEntry = entry.clone();
                if kind != "project" {
                    let prefix = format!("/{kind}/0");
                    let suffix = entry
                        .pointer
                        .strip_prefix(&prefix)
                        .expect("single IR fragment ordinal");
                    entry.pointer = format!("/{kind}/{ordinal}{suffix}");
                }
                entries.push(entry);
            }
            next.rows.insert(normalized.id.clone(), fragment);
        }
        Ok((
            Compilation {
                project: CompiledProject {
                    model_version: model.model_version,
                    project,
                    modules,
                    definitions,
                },
                source_map: SourceMap::new(entries),
            },
            next,
            counts,
        ))
    }
}
