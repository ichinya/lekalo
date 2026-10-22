//! Requirement extraction over the typed IR (issue #19).
//!
//! One eligible semantic definition is one subject; fields, effects, and
//! attachments are obligation rows under that subject. Project and module
//! containers plus native target-binding metadata stay out of the
//! portable-business denominator and are inventoried separately. The
//! traversal is bounded by the reviewed recipe limits and never launches
//! anything: adjacency comes from the canonical dependency graph.

use super::wire::Recipe;
use crate::ir::{CompiledProject, Definition, TypeRef};
use std::collections::{BTreeMap, BTreeSet};

/// One extracted subject with its typed identity and direct references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subject {
    /// The typed `kind:id` identity.
    pub typed_id: String,
    /// The closed definition-kind tag.
    pub kind: &'static str,
    /// The fully qualified semantic id.
    pub id: String,
    /// The owning module semantic id.
    pub module: String,
    /// Direct semantic references, sorted and deduplicated.
    pub deps: Vec<String>,
    /// The declared portability annotation, when present.
    pub declared_portability: Option<&'static str>,
    /// The commands that trigger this effect (effects only).
    pub triggers: Vec<String>,
    /// The events this effect emits (effects only).
    pub emits: Vec<String>,
    /// The symbols this scenario covers (scenarios only).
    pub covers: Vec<String>,
    /// The bounded dependency-closure witnesses (typed ids).
    pub witnesses: Vec<String>,
    /// The bounded reverse-dependency witnesses: the typed ids of the
    /// symbols that (transitively) depend on this subject. A scenario
    /// covering a dependent is an affected witness of this subject.
    pub dependents: Vec<String>,
}

/// The extracted subject set with its bounded-traversal state.
#[derive(Clone, Debug, Default)]
pub struct Subjects {
    /// Subjects in typed-id byte order.
    pub rows: Vec<Subject>,
    /// The Model-declared target names (inventory, not denominator).
    pub declared_targets: Vec<String>,
    /// The scenario reverse join: scenario id to the subject ids it
    /// covers, used for verification and affected-surface joins.
    pub covered_by: BTreeMap<String, Vec<String>>,
    /// Whether any recipe bound was exhausted.
    pub truncated: bool,
}

/// The bounded traversal budget shared across the whole assessment.
#[derive(Clone, Copy)]
pub(crate) struct Budget {
    depth: u64,
    edges: u64,
    work: u64,
    work_spent: u64,
    edge_visits: u64,
    exhausted: bool,
}

impl Budget {
    fn new(recipe: &Recipe) -> Self {
        Self {
            depth: recipe.document.limits.max_traversal_depth,
            edges: recipe.document.limits.max_dependency_edges,
            work: recipe.document.limits.max_work_units,
            work_spent: 0,
            edge_visits: 0,
            exhausted: false,
        }
    }

    fn charge(&mut self, units: u64) {
        self.work_spent = self.work_spent.saturating_add(units);
        if self.work_spent > self.work {
            self.exhausted = true;
        }
    }
}

/// Flatten one typed reference into its leaf symbol references.
fn type_refs(reference: &TypeRef, out: &mut Vec<String>) {
    match reference {
        TypeRef::Ref(id) => out.push(id.as_str().to_owned()),
        TypeRef::List(inner) | TypeRef::Optional(inner) => type_refs(inner, out),
    }
}

fn field_refs(fields: &[crate::ir::Field]) -> Vec<String> {
    let mut out = Vec::new();
    for field in fields {
        type_refs(&field.r#type, &mut out);
    }
    out
}

/// Extract every eligible subject from one compiled project.
pub fn extract(project: &CompiledProject, recipe: &Recipe) -> Subjects {
    let mut budget = Budget::new(recipe);
    let mut rows: Vec<Subject> = Vec::new();
    for definition in &project.definitions {
        let (kind, deps, triggers, emits, covers) = match definition {
            Definition::Scalar(_) => ("scalar", Vec::new(), vec![], vec![], vec![]),
            Definition::Enum(_) => ("enum", Vec::new(), vec![], vec![], vec![]),
            Definition::ValueObject(d) => (
                "value-object",
                field_refs(&d.fields),
                vec![],
                vec![],
                vec![],
            ),
            Definition::Entity(d) => ("entity", field_refs(&d.fields), vec![], vec![], vec![]),
            Definition::Command(d) => {
                let mut deps = field_refs(&d.input);
                if let Some(returns) = &d.returns {
                    type_refs(returns, &mut deps);
                }
                deps.extend(d.effects.iter().map(|id| id.as_str().to_owned()));
                ("command", deps, vec![], vec![], vec![])
            }
            Definition::Query(d) => {
                let mut deps: Vec<String> =
                    d.reads.iter().map(|id| id.as_str().to_owned()).collect();
                deps.extend(field_refs(&d.input));
                if let Some(returns) = &d.returns {
                    type_refs(returns, &mut deps);
                }
                ("query", deps, vec![], vec![], vec![])
            }
            Definition::Policy(d) => (
                "policy",
                d.applies_to
                    .iter()
                    .map(|id| id.as_str().to_owned())
                    .collect(),
                vec![],
                vec![],
                vec![],
            ),
            Definition::Event(d) => ("event", field_refs(&d.payload), vec![], vec![], vec![]),
            Definition::Effect(d) => (
                "effect",
                vec![d.entity.as_str().to_owned()],
                vec![],
                d.emits.iter().map(|id| id.as_str().to_owned()).collect(),
                vec![],
            ),
            Definition::Endpoint(d) => (
                "endpoint",
                vec![d.invokes.as_str().to_owned()],
                vec![],
                vec![],
                vec![],
            ),
            Definition::Scenario(d) => (
                "scenario",
                d.covers.iter().map(|id| id.as_str().to_owned()).collect(),
                vec![],
                vec![],
                d.covers.iter().map(|id| id.as_str().to_owned()).collect(),
            ),
            // Native target-binding metadata stays outside the subject set.
            Definition::TargetBinding(_) => continue,
        };
        let module = id_module(definition.id().as_str());
        rows.push(Subject {
            typed_id: format!("{}:{}", kind, definition.id().as_str()),
            kind,
            id: definition.id().as_str().to_owned(),
            module: module.to_owned(),
            deps: sorted_unique(deps),
            declared_portability: definition.common().portability.map(|p| p.as_str()),
            triggers,
            emits,
            covers,
            witnesses: Vec::new(),
            dependents: Vec::new(),
        });
    }
    // The triggers of every effect: the commands whose declared effects
    // reach it, used to join hook coverage onto the write path.
    let mut triggers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for definition in &project.definitions {
        if let Definition::Command(command) = definition {
            for effect in &command.effects {
                triggers
                    .entry(effect.as_str().to_owned())
                    .or_default()
                    .insert(command.id.as_str().to_owned());
            }
        }
    }
    for row in &mut rows {
        if row.kind == "effect" {
            if let Some(found) = triggers.get(&row.id) {
                row.triggers = found.iter().cloned().collect();
            }
        }
    }
    // Declared target inventory.
    let mut declared_targets: Vec<String> = project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::TargetBinding(binding) => Some(binding.target.as_str().to_owned()),
            _ => None,
        })
        .collect();
    declared_targets.sort();
    declared_targets.dedup();

    // One bounded shared closure pass: adjacency by semantic id, work
    // charged once per edge across all subjects.
    let index: BTreeMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| (row.id.as_str(), i))
        .collect();
    let adjacency: Vec<Vec<usize>> = rows
        .iter()
        .map(|row| {
            row.deps
                .iter()
                .filter_map(|dep| index.get(dep.as_str()).copied())
                .collect()
        })
        .collect();
    let witnesses_per_subject: Vec<Vec<String>> = (0..rows.len())
        .map(|root| {
            let mut seen = vec![false; rows.len()];
            let mut queue = std::collections::VecDeque::new();
            for dep in &adjacency[root] {
                queue.push_back((*dep, 1u64));
            }
            let mut found = BTreeSet::new();
            while let Some((node, depth)) = queue.pop_front() {
                budget.charge(1);
                budget.edge_visits += 1;
                if budget.edge_visits > budget.edges || depth > budget.depth {
                    budget.exhausted = true;
                    break;
                }
                if seen[node] {
                    continue;
                }
                seen[node] = true;
                found.insert(rows[node].typed_id.clone());
                for dep in &adjacency[node] {
                    if !seen[*dep] {
                        queue.push_back((*dep, depth + 1));
                    }
                }
            }
            if budget.exhausted {
                found.clear();
            }
            found.into_iter().collect()
        })
        .collect();
    // The bounded reverse closure: which symbols (transitively) depend
    // on this subject. Scenario coverage of a dependent is an affected
    // witness of the subject, joined within the same recipe bounds.
    let mut reverse_adjacency: Vec<Vec<usize>> = vec![Vec::new(); rows.len()];
    for (root, deps) in adjacency.iter().enumerate() {
        for dep in deps {
            reverse_adjacency[*dep].push(root);
        }
    }
    let dependents_per_subject: Vec<Vec<String>> = (0..rows.len())
        .map(|root| {
            let mut seen = vec![false; rows.len()];
            let mut queue = std::collections::VecDeque::new();
            for dependent in &reverse_adjacency[root] {
                queue.push_back((*dependent, 1u64));
            }
            let mut found = BTreeSet::new();
            while let Some((node, depth)) = queue.pop_front() {
                budget.charge(1);
                budget.edge_visits += 1;
                if budget.edge_visits > budget.edges || depth > budget.depth {
                    budget.exhausted = true;
                    break;
                }
                if seen[node] {
                    continue;
                }
                seen[node] = true;
                found.insert(rows[node].typed_id.clone());
                for dependent in &reverse_adjacency[node] {
                    if !seen[*dependent] {
                        queue.push_back((*dependent, depth + 1));
                    }
                }
            }
            if budget.exhausted {
                found.clear();
            }
            found.into_iter().collect()
        })
        .collect();
    let truncated = budget.exhausted;
    for ((row, witnesses), dependents) in rows
        .iter_mut()
        .zip(witnesses_per_subject)
        .zip(dependents_per_subject)
    {
        row.witnesses = witnesses;
        row.dependents = dependents;
    }
    let mut covered_by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in &rows {
        if row.kind == "scenario" {
            covered_by.insert(row.id.clone(), row.covers.clone());
        }
    }
    Subjects {
        rows,
        declared_targets,
        covered_by,
        truncated,
    }
}

fn id_module(id: &str) -> &str {
    match id.rsplit_once('.') {
        Some((module, _)) => module,
        None => id,
    }
}

fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}
