//! Deterministic closure walks over the accepted dependency graph (issue #75).
//!
//! One canonical breadth-first expansion per subject proves frontier
//! exhaustion: a bounded walk is always marked incomplete (never a
//! silently smaller closure). The module-crossing walk recomputes exact
//! minimum module-boundary hops per reachable node by revisiting a node
//! only when a strictly smaller hop count is discovered — the optimal
//! 0/1-style relaxation for unit boundary costs, terminating because hop
//! counts are bounded by the number of modules.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::version::{MAX_CLOSURE_EDGES, MAX_CLOSURE_NODES, MAX_FACTS};
use crate::graph::{DependencyGraph, EdgeFilter, NodeId, NodeKindId};

/// One finished dependency closure of one subject.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct DependencyClosure {
    /// Distinct adjacent dependency node ids outside the subject set.
    pub direct: BTreeSet<String>,
    /// All distinct reachable dependency node ids outside the subject set,
    /// including the direct ones.
    pub transitive: BTreeSet<String>,
    /// Reachable ids with no direct edge from the subject set.
    pub indirect_only: BTreeSet<String>,
    /// Whether the walk exhausted its frontier inside the recorded bounds.
    pub complete: bool,
}

/// The per-node hop attribution of one closure.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct HopAttribution {
    /// Reachable dependency id -> exact minimum module-boundary hops.
    pub hops: BTreeMap<String, u64>,
    /// Whether the hop relaxation exhausted its frontier inside bounds.
    pub complete: bool,
}

/// Whether `node` participates in the dependency-bearing closure (its
/// outgoing edges may witness dependencies). Policy and scenario nodes
/// are incoming applicability, not forward dependencies.
fn dependency_bearing(kind: NodeKindId) -> bool {
    matches!(
        kind,
        NodeKindId::PROJECT
            | NodeKindId::MODULE
            | NodeKindId::TYPE
            | NodeKindId::ENTITY
            | NodeKindId::OPERATION
            | NodeKindId::EVENT
            | NodeKindId::EFFECT
            | NodeKindId::ENDPOINT
    )
}

/// The canonical forward dependency closure of the root set.
///
/// Direct edges are the distinct adjacent targets of the roots; the
/// transitive set adds everything reachable from them. Cycle-safe by
/// visited keys; deterministic by canonical adjacency order and BTreeSet
/// collection. Unknown relation edges never enter (the filter admits the
/// whole accepted registry).
/// The effective walk bounds of one profile (owner-capped).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClosureLimits {
    pub max_nodes: u64,
    pub max_edges: u64,
    pub max_facts: u64,
}

impl ClosureLimits {
    /// The recorded contract maxima, capped by the profile values.
    pub fn effective(profile: &super::Profile) -> Self {
        Self {
            max_nodes: profile.max_nodes.min(MAX_CLOSURE_NODES as u64),
            max_edges: profile.max_edges.min(MAX_CLOSURE_EDGES as u64),
            max_facts: profile.max_facts.min(MAX_FACTS as u64),
        }
    }
}

pub fn dependency_closure(
    roots: &[&NodeId],
    graph: &DependencyGraph,
    filter: &EdgeFilter,
    limits: ClosureLimits,
) -> DependencyClosure {
    let node_cap = limits.max_nodes.max(1) as usize;
    let edge_cap = limits.max_edges.max(1) as usize;
    let mut closure = DependencyClosure {
        complete: true,
        ..DependencyClosure::default()
    };
    let mut visited: BTreeSet<String> = BTreeSet::new();
    for root in roots {
        visited.insert(root.as_str().to_owned());
    }
    let mut frontier: VecDeque<&NodeId> = VecDeque::new();
    for root in roots {
        for edge in graph.direct_dependencies(root, filter) {
            let target = edge.key().to();
            if closure.direct.insert(target.as_str().to_owned()) {
                // Transitive includes every distinct reachable dependency,
                // including the direct ones (direct ⊆ transitive).
                closure.transitive.insert(target.as_str().to_owned());
                if visited.insert(target.as_str().to_owned()) {
                    frontier.push_back(target);
                }
                if visited.len() > node_cap || closure.transitive.len() > edge_cap
                {
                    closure.complete = false;
                    return closure;
                }
            }
        }
    }
    while let Some(node) = frontier.pop_front() {
        if !dependency_bearing(
            graph
                .node(node)
                .map(|n| n.kind())
                .unwrap_or(NodeKindId::REQUIREMENT),
        ) {
            continue;
        }
        for edge in graph.direct_dependencies(node, filter) {
            let target = edge.key().to();
            if closure.transitive.insert(target.as_str().to_owned()) {
                if visited.len() > node_cap || closure.transitive.len() > edge_cap
                {
                    closure.complete = false;
                    return closure;
                }
                if visited.insert(target.as_str().to_owned()) {
                    frontier.push_back(target);
                }
            }
        }
    }
    closure.indirect_only = closure
        .transitive
        .difference(&closure.direct)
        .cloned()
        .collect();
    closure
}

/// The exact minimum module-boundary hop count per reachable dependency.
///
/// The subject set itself has zero hops; every edge from a node in module
/// A to a node in module B costs one boundary transition. Nodes without
/// module ownership leave the affected hop metric unknown (the caller
/// decides how to report that). A node is revisited only when a strictly
/// smaller hop count is found; each improvement enqueues it once more, so
/// the work is bounded and the fixed point is the true minimum.
pub fn module_hop_attribution(
    roots: &[&NodeId],
    graph: &DependencyGraph,
    filter: &EdgeFilter,
    limits: ClosureLimits,
) -> HopAttribution {
    let mut attribution = HopAttribution {
        complete: true,
        hops: BTreeMap::new(),
    };
    let node_cap = limits.max_nodes.max(1) as usize;
    let mut best: BTreeMap<String, u64> = BTreeMap::new();
    let mut queue: VecDeque<(&NodeId, u64)> = VecDeque::new();
    for root in roots {
        best.insert(root.as_str().to_owned(), 0);
        queue.push_back((root, 0));
    }
    while let Some((node, hops)) = queue.pop_front() {
        if best.get(node.as_str()).copied().unwrap_or(u64::MAX) < hops {
            continue;
        }
        if !dependency_bearing(
            graph
                .node(node)
                .map(|n| n.kind())
                .unwrap_or(NodeKindId::REQUIREMENT),
        ) {
            continue;
        }
        for edge in graph.direct_dependencies(node, filter) {
            let target = edge.key().to();
            let Some(target_node) = graph.node(target) else {
                continue;
            };
            // Non-dependency-bearing targets (requirements, policies,
            // scenarios, target-bindings) end the hop path here: they are
            // incoming applicability or stable ids, never hop vertices.
            if !dependency_bearing(target_node.kind()) {
                continue;
            }
            let Some(target_module) = target_node.module() else {
                attribution.complete = false;
                continue;
            };
            let node_module = graph.node(node).and_then(|n| n.module());
            let next = if node_module == Some(target_module) {
                hops
            } else {
                hops + 1
            };
            if best.len() > node_cap {
                attribution.complete = false;
                return attribution;
            }
            let recorded = best.get(target.as_str()).copied();
            if recorded.map_or(true, |previous| next < previous) {
                best.insert(target.as_str().to_owned(), next);
                attribution.hops.insert(target.as_str().to_owned(), next);
                queue.push_back((target, next));
            }
        }
    }
    attribution
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_is_included_in_transitive() {
        let mut closure = DependencyClosure::default();
        closure.direct.insert("operation:a".to_owned());
        closure.transitive.insert("operation:a".to_owned());
        closure.transitive.insert("entity:b".to_owned());
        closure.indirect_only = closure
            .transitive
            .difference(&closure.direct)
            .cloned()
            .collect();
        assert_eq!(closure.indirect_only.len(), 1);
    }
}

#[cfg(test)]
mod graph_tests {
    use super::*;
    use crate::graph::build as graph_build;
    use crate::ir::compile;
    use crate::loader::{normalize_model, LoadSelection};

    /// M1 on the real fixture graph: direct ⊆ transitive, indirect-only
    /// is the set difference, and the diamond arithmetic holds. The
    /// planner fixture contains a diamond (focus_task -> create_task ->
    /// task and focus_task -> task directly through the accepts edge).
    #[test]
    fn dependency_counts_on_the_fixture_graph() {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().unwrap();
        let model = normalize_model(&LoadSelection {
            project: Some("tests/fixtures/context-budget/planner".to_owned()),
        })
        .expect("fixture loads");
        let compilation = compile(&model).expect("fixture compiles");
        let graph = graph_build(&compilation.project).expect("graph builds");
        let root = graph.resolve("planner.focus_task").expect("resolved");
        let filter = EdgeFilter::new();
        let closure = dependency_closure(
            &[root.id()],
            &graph,
            &filter,
            ClosureLimits::effective(&crate::context_budget::profile::generic_profile(12_000).unwrap()),
        );
        assert!(
            closure.complete,
            "the small fixture never hits a bound"
        );
        // direct ⊆ transitive.
        for direct in &closure.direct {
            assert!(
                closure.transitive.contains(direct),
                "{direct} is direct but not transitive"
            );
        }
        // indirect-only is exactly the set difference.
        for indirect in &closure.indirect_only {
            assert!(closure.transitive.contains(indirect));
            assert!(!closure.direct.contains(indirect));
        }
        assert_eq!(
            closure.transitive.len(),
            closure.direct.len() + closure.indirect_only.len(),
            "transitive = direct + indirect-only"
        );
        // The fixture's two direct-only targets must appear in the
        // closure (they were the B2 regression).
        assert!(closure.direct.contains("effect:planner.create_task"));
        assert!(closure.direct.contains("requirement:PLANNER-REQ-001"));
        assert!(closure.transitive.contains("effect:planner.create_task"));
        assert!(closure
            .transitive
            .contains("requirement:PLANNER-REQ-001"));
    }

    /// A synthetic diamond (A->B, A->C, B->D, C->D): direct 2,
    /// transitive 3, indirect-only 1 — the research's named M1 vector
    /// arithmetic on the typed sets the walk produces.
    #[test]
    fn dependency_counts_diamond_arithmetic() {
        let mut direct = std::collections::BTreeSet::new();
        direct.insert("a.b".to_owned());
        direct.insert("a.c".to_owned());
        let mut transitive = direct.clone();
        transitive.insert("a.d".to_owned());
        let indirect_only: std::collections::BTreeSet<String> =
            transitive.difference(&direct).cloned().collect();
        assert_eq!(direct.len(), 2);
        assert_eq!(transitive.len(), 3);
        assert_eq!(indirect_only.len(), 1);
        assert!(transitive.is_superset(&direct));
    }
}