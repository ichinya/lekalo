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

use super::version::{MAX_CLOSURE_EDGES, MAX_CLOSURE_NODES};
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
pub fn dependency_closure(
    roots: &[&NodeId],
    graph: &DependencyGraph,
    filter: &EdgeFilter,
) -> DependencyClosure {
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
                if visited.insert(target.as_str().to_owned()) {
                    frontier.push_back(target);
                }
                if visited.len() > MAX_CLOSURE_NODES || closure.transitive.len() > MAX_CLOSURE_EDGES
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
                if visited.len() > MAX_CLOSURE_NODES || closure.transitive.len() > MAX_CLOSURE_EDGES
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
) -> HopAttribution {
    let mut attribution = HopAttribution {
        complete: true,
        hops: BTreeMap::new(),
    };
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
