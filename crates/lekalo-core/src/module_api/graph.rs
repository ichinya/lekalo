//! Boundary graph witnesses (issue #113).
//!
//! Fatal declared import cycles prevent `graph::build` from ever
//! succeeding, so cycle evidence must exist before any graph construction
//! is required. This projection runs over the typed module/import facts of
//! the compiled project only: iterative strongly connected components with
//! a canonical rotation-minimal cycle path and original edge keys, plus the
//! public dependency projection over bundle pins. It never conflates a
//! module import cycle with a symbol cycle or ordinary local recursion.

use super::wire::CycleWitness;
use std::collections::{BTreeMap, BTreeSet};

/// One declared import edge: `from` imports `to`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImportEdge {
    pub from: String,
    pub to: String,
}

/// Build the declared import adjacency from the compiled modules.
pub fn import_edges(modules: &[crate::ir::Module]) -> Vec<ImportEdge> {
    let mut edges: Vec<ImportEdge> = modules
        .iter()
        .flat_map(|module| {
            module
                .imports
                .iter()
                .map(|import| ImportEdge {
                    from: module.id.as_str().to_owned(),
                    to: import.as_str().to_owned(),
                })
        })
        .collect();
    edges.sort();
    edges.dedup();
    edges
}

/// Detect every strongly connected component of the declared import graph
/// with at least two members or a self-loop. The returned witnesses carry
/// the canonical (rotation-minimal, lexicographically smallest) path and
/// the original edge keys, deterministically sorted. Kosaraju's algorithm
/// over the bounded module graph with iterative DFS: no recursion depth
/// scales with hostile input.
pub fn import_sccs(edges: &[ImportEdge]) -> Vec<CycleWitness> {
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut reverse: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in edges {
        adjacency.entry(edge.from.as_str()).or_default().push(edge.to.as_str());
        reverse.entry(edge.to.as_str()).or_default().push(edge.from.as_str());
    }
    let nodes: BTreeSet<&str> = edges
        .iter()
        .flat_map(|edge| [edge.from.as_str(), edge.to.as_str()])
        .collect();
    // First pass: finish order of the forward graph.
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut order: Vec<&str> = Vec::with_capacity(nodes.len());
    for root in &nodes {
        if visited.contains(root) {
            continue;
        }
        let mut stack: Vec<(&str, bool)> = vec![(root, false)];
        while let Some((node, processed)) = stack.pop() {
            if processed {
                order.push(node);
                continue;
            }
            if !visited.insert(node) {
                continue;
            }
            stack.push((node, true));
            for child in adjacency.get(node).into_iter().flatten() {
                if !visited.contains(child) {
                    stack.push((child, false));
                }
            }
        }
    }
    // Second pass: components over the reversed graph in finish order.
    let mut assigned: BTreeMap<&str, usize> = BTreeMap::new();
    let mut components: Vec<Vec<&str>> = Vec::new();
    for root in order.into_iter().rev() {
        if assigned.contains_key(root) {
            continue;
        }
        let component_index = components.len();
        let mut component = Vec::new();
        let mut stack = vec![root];
        assigned.insert(root, component_index);
        while let Some(node) = stack.pop() {
            component.push(node);
            for parent in reverse.get(node).into_iter().flatten() {
                if !assigned.contains_key(parent) {
                    assigned.insert(parent, component_index);
                    stack.push(parent);
                }
            }
        }
        components.push(component);
    }
    sccs_to_witnesses(edges, components)
}

/// Convert components into closed witnesses with canonical paths.
fn sccs_to_witnesses(edges: &[ImportEdge], sccs: Vec<Vec<&str>>) -> Vec<CycleWitness> {
    let mut witnesses = Vec::new();
    for component in sccs {
        if component.len() == 1 {
            let member = component[0];
            if !edges
                .iter()
                .any(|edge| edge.from == member && edge.to == member)
            {
                continue;
            }
        }
        let members: Vec<String> = component.iter().map(|node| (*node).to_owned()).collect();
        // Canonical path: rotate the cycle so the smallest member leads.
        let rotation = smallest_rotation(&members);
        let mut path = rotation.clone();
        if let Some(first) = rotation.first() {
            path.push(first.clone());
        }
        let member_set: BTreeSet<&str> = component.iter().copied().collect();
        let cycle_edges: Vec<String> = edges
            .iter()
            .filter(|edge| {
                member_set.contains(edge.from.as_str()) && member_set.contains(edge.to.as_str())
            })
            .map(|edge| format!("{}->{}", edge.from, edge.to))
            .collect();
        witnesses.push(CycleWitness {
            projection: "imports".to_owned(),
            members,
            path,
            edges: cycle_edges,
        });
    }
    witnesses.sort_by(|left, right| left.members.cmp(&right.members));
    witnesses
}

/// The rotation-minimal, lexicographically smallest cycle spelling.
fn smallest_rotation(members: &[String]) -> Vec<String> {
    let mut rotations: Vec<Vec<String>> = Vec::new();
    for start in 0..members.len() {
        let mut rotated: Vec<String> = members[start..].to_vec();
        rotated.extend_from_slice(&members[..start]);
        rotations.push(rotated);
    }
    rotations.sort();
    rotations.into_iter().next().unwrap_or_default()
}

/// The public dependency projection: edges between bundles from the exact
/// public dependency pins. Used by the check command's public cycle row.
pub fn public_edges(
    bundles: &BTreeMap<String, super::wire::PublicBundle>,
) -> Vec<ImportEdge> {
    let mut edges: Vec<ImportEdge> = bundles
        .iter()
        .flat_map(|(module, bundle)| {
            bundle
                .public_dependencies
                .iter()
                .map(|dependency| ImportEdge {
                    from: module.clone(),
                    to: dependency.module_id.clone(),
                })
        })
        .collect();
    edges.sort();
    edges.dedup();
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_two_node_cycle_and_self_loop() {
        let edges = vec![
            ImportEdge {
                from: "a".to_owned(),
                to: "b".to_owned(),
            },
            ImportEdge {
                from: "b".to_owned(),
                to: "a".to_owned(),
            },
        ];
        let cycles = import_sccs(&edges);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].members, vec!["a", "b"]);
        assert_eq!(cycles[0].path, vec!["a", "b", "a"]);
        assert_eq!(cycles[0].edges, vec!["a->b", "b->a"]);
        let self_loop = vec![ImportEdge {
            from: "a".to_owned(),
            to: "a".to_owned(),
        }];
        let cycles = import_sccs(&self_loop);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].members, vec!["a"]);
        // An acyclic chain has no cycle.
        let chain = vec![
            ImportEdge {
                from: "a".to_owned(),
                to: "b".to_owned(),
            },
            ImportEdge {
                from: "b".to_owned(),
                to: "c".to_owned(),
            },
        ];
        assert!(import_sccs(&chain).is_empty());
    }
}
