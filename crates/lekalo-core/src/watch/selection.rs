//! Affected selection uses both old and new ownership/dependency
//! indexes. Filesystem renames are not semantic rename evidence.
use super::changes::ChangeBatch;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Default)]
pub struct ModuleIndex {
    pub owned_paths: BTreeMap<String, BTreeSet<String>>,
    pub imports: BTreeMap<String, BTreeSet<String>>,
    pub graph_complete: bool,
}
impl ModuleIndex {
    pub fn from_compiled(compilation: &crate::ir::Compilation) -> Self {
        let graph = crate::graph::build(&compilation.project).ok();
        Self::from_graph(compilation, graph.as_ref())
    }
    pub(crate) fn from_graph(
        compilation: &crate::ir::Compilation,
        graph: Option<&crate::graph::DependencyGraph>,
    ) -> Self {
        let mut result = Self::default();
        for module in &compilation.project.modules {
            result.imports.insert(
                module.id.as_str().to_owned(),
                module
                    .imports
                    .iter()
                    .map(|i| i.as_str().to_owned())
                    .collect(),
            );
            result
                .owned_paths
                .entry(module.id.as_str().to_owned())
                .or_default();
        }
        for row in compilation.source_map.entries() {
            if let Some(id) = &row.semantic_id {
                let owner = id.split('.').next().unwrap_or(id);
                if let Some(paths) = result.owned_paths.get_mut(owner) {
                    paths.insert(row.path.clone());
                }
            }
        }
        // Fully qualified references can cross modules without a short
        // import. Include actual typed graph dependencies, not only the
        // declared import list. A rejected graph forces a full fallback.
        if let Some(graph) = graph {
            result.graph_complete = true;
            for edge in graph.edges() {
                let from = edge
                    .key()
                    .from()
                    .semantic_id()
                    .split('.')
                    .next()
                    .unwrap_or("");
                let to = edge
                    .key()
                    .to()
                    .semantic_id()
                    .split('.')
                    .next()
                    .unwrap_or("");
                if from != to && result.imports.contains_key(to) {
                    if let Some(dependencies) = result.imports.get_mut(from) {
                        dependencies.insert(to.to_owned());
                    }
                }
            }
        }
        result
    }
    pub fn import_closure(&self, module: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([module.to_owned()]);
        while let Some(current) = queue.pop_front() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if let Some(next) = self.imports.get(&current) {
                queue.extend(next.iter().cloned());
            }
        }
        seen.remove(module);
        seen
    }
    pub fn reverse_closure(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        let mut affected = seeds.clone();
        let mut queue = VecDeque::from_iter(seeds.iter().cloned());
        while let Some(current) = queue.pop_front() {
            for (module, imports) in &self.imports {
                if imports.contains(&current) && affected.insert(module.clone()) {
                    queue.push_back(module.clone());
                }
            }
        }
        affected
    }
    pub fn owner_of(&self, path: &str) -> Option<&str> {
        self.owned_paths
            .iter()
            .find(|(_, paths)| paths.contains(path))
            .map(|(id, _)| id.as_str())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AffectedSet {
    pub scope: &'static str,
    pub reason: Option<&'static str>,
    pub modules: BTreeSet<String>,
}
impl AffectedSet {
    pub fn full(reason: &'static str) -> Self {
        Self {
            scope: "full",
            reason: Some(reason),
            modules: BTreeSet::new(),
        }
    }
}
pub fn compute(
    changes: &ChangeBatch,
    previous: Option<&ModuleIndex>,
    current: &ModuleIndex,
) -> AffectedSet {
    let Some(previous) = previous else {
        return AffectedSet::full("baseline");
    };
    if !previous.graph_complete || !current.graph_complete {
        return AffectedSet::full("graph-unavailable");
    }
    let mut seeds = BTreeSet::new();
    for row in changes.semantic_rows() {
        if !row.path.starts_with("lekalo/modules/") {
            return AffectedSet::full(if row.path.starts_with("lekalo/targets/") {
                "target-input"
            } else {
                "project-input"
            });
        }
        let mut mapped = false;
        for index in [previous, current] {
            if let Some(owner) = index.owner_of(&row.path) {
                seeds.insert(owner.to_owned());
                mapped = true;
            }
        }
        if !mapped {
            return AffectedSet::full("unknown-owner");
        }
    }
    let mut modules = previous.reverse_closure(&seeds);
    modules.extend(current.reverse_closure(&seeds));
    AffectedSet {
        scope: "selective",
        reason: None,
        modules,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::changes::{ChangeKind, ChangeRow};
    #[test]
    fn selects_both_old_and_new_reverse_dependency_closures() {
        let mut old = ModuleIndex {
            graph_complete: true,
            ..ModuleIndex::default()
        };
        old.owned_paths.insert(
            "planner".into(),
            ["lekalo/modules/planner/entities.yaml".into()].into(),
        );
        old.imports
            .insert("reports".into(), ["planner".into()].into());
        old.imports.insert("unrelated".into(), BTreeSet::new());
        let mut new = old.clone();
        new.imports.remove("reports");
        new.imports
            .insert("new-dependent".into(), ["planner".into()].into());
        let changed = ChangeBatch {
            rows: vec![ChangeRow {
                path: "lekalo/modules/planner/entities.yaml".into(),
                kind: ChangeKind::Modified,
                old_sha256: None,
                new_sha256: None,
                rename_of: None,
                class: "semantic",
            }],
        };
        let selected = compute(&changed, Some(&old), &new);
        assert_eq!(selected.scope, "selective");
        assert_eq!(
            selected.modules,
            ["planner".into(), "reports".into(), "new-dependent".into()].into()
        );
    }
}
