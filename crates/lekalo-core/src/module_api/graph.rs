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
            module.imports.iter().map(|import| ImportEdge {
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
/// an actual directed cycle path (every adjacent pair, including the
/// closing hop, is an original edge — never an invented rotation of the
/// member set) plus the original edge keys, deterministically sorted.
/// Kosaraju's algorithm over the bounded module graph with iterative DFS:
/// no recursion depth scales with hostile input.
pub fn import_sccs(edges: &[ImportEdge]) -> Vec<CycleWitness> {
    sccs_labeled(edges, "imports")
}

/// The labeled SCC projection shared by the import and reference
/// projections (r3 finding: the projection name rides the witness).
pub fn sccs_labeled(edges: &[ImportEdge], projection: &str) -> Vec<CycleWitness> {
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut reverse: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in edges {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
        reverse
            .entry(edge.to.as_str())
            .or_default()
            .push(edge.from.as_str());
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
    sccs_to_witnesses(edges, components, projection)
}

/// Convert components into closed witnesses with actual cycle paths.
fn sccs_to_witnesses(
    edges: &[ImportEdge],
    sccs: Vec<Vec<&str>>,
    projection: &str,
) -> Vec<CycleWitness> {
    let edge_set: BTreeSet<(&str, &str)> = edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (from, to) in &edge_set {
        adjacency.entry(from).or_default().push(to);
    }
    let mut witnesses = Vec::new();
    for component in sccs {
        let member_set: BTreeSet<&str> = component.iter().copied().collect();
        if member_set.len() == 1 {
            let member = component[0];
            if !edge_set.contains(&(member, member)) {
                continue;
            }
        }
        let members: Vec<String> = {
            let mut sorted = component.clone();
            sorted.sort();
            sorted.iter().map(|node| (*node).to_owned()).collect()
        };
        // The witness path is an actual directed cycle inside the
        // component (r3 finding 11/codex): a DFS that stays within the
        // component and returns to its root, never a rotation of the
        // member set. Every adjacent pair is verified against the
        // original edges below.
        let Some(cycle) = directed_cycle(&adjacency, &member_set) else {
            continue;
        };
        // Canonical spelling: rotate the real cycle so its smallest node
        // leads; only valid rotations of an actual cycle are considered.
        let rotation = smallest_rotation(&cycle);
        let mut path: Vec<String> = rotation.iter().map(|node| String::from(*node)).collect();
        if let Some(first) = rotation.first() {
            path.push(String::from(*first));
        }
        // Verification: every adjacent witness pair, including the
        // closing hop, must be an original directed edge — a witness that
        // invented an edge is a contract violation, never a report row.
        assert!(path
            .windows(2)
            .all(|pair| edge_set.contains(&(pair[0].as_str(), pair[1].as_str()))));
        let cycle_edges: Vec<String> = edge_set
            .iter()
            .filter(|(from, to)| member_set.contains(from) && member_set.contains(to))
            .map(|(from, to)| format!("{from}->{to}"))
            .collect();
        witnesses.push(CycleWitness {
            projection: projection.to_owned(),
            members,
            path,
            edges: cycle_edges,
        });
    }
    witnesses.sort_by(|left, right| left.members.cmp(&right.members));
    witnesses
}

/// Find one actual directed cycle inside a strongly connected component:
/// deterministic DFS from the smallest member following only component
/// edges until the root repeats. Returns the cycle nodes in walk order
/// (the root is not repeated; the closing hop is implicit).
fn directed_cycle<'a>(
    adjacency: &BTreeMap<&'a str, Vec<&'a str>>,
    component: &BTreeSet<&'a str>,
) -> Option<Vec<&'a str>> {
    let root = *component.first()?;
    // Iterative DFS over (node, neighbor cursor) frames.
    let mut path: Vec<&'a str> = vec![root];
    let mut on_path: BTreeSet<&'a str> = [root].into_iter().collect();
    let mut cursors: Vec<usize> = vec![0];
    while let Some(node) = path.last().copied() {
        let neighbors = adjacency.get(node).cloned().unwrap_or_default();
        let cursor = cursors.last_mut()?;
        if let Some(next) = neighbors.get(*cursor).copied() {
            *cursor += 1;
            if next == root {
                // A real directed cycle closes here.
                return Some(path.clone());
            }
            if component.contains(next) && on_path.insert(next) {
                path.push(next);
                cursors.push(0);
            }
            continue;
        }
        // Exhausted: backtrack.
        path.pop();
        cursors.pop();
    }
    None
}

/// The rotation-minimal, lexicographically smallest spelling of one
/// actual directed cycle (only valid rotations of a real cycle arrive
/// here).
fn smallest_rotation<'a>(cycle: &'a [&'a str]) -> Vec<&'a str> {
    let mut rotations: Vec<Vec<&str>> = Vec::new();
    for start in 0..cycle.len() {
        let mut rotated: Vec<&str> = cycle[start..].to_vec();
        rotated.extend_from_slice(&cycle[..start]);
        rotations.push(rotated);
    }
    rotations.sort();
    rotations.into_iter().next().unwrap_or_default()
}

/// The public dependency projection: edges between bundles from the exact
/// public dependency pins. Used by the check command's public cycle row.
pub fn public_edges(bundles: &BTreeMap<String, super::wire::PublicBundle>) -> Vec<ImportEdge> {
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

    #[test]
    fn witness_paths_are_real_directed_cycles() {
        // r3 review finding: rotating SCC members invented nonexistent
        // edges. Every witness pair must be an original edge.
        let edge = |from: &str, to: &str| ImportEdge {
            from: from.to_owned(),
            to: to.to_owned(),
        };
        let edges = vec![edge("a", "b"), edge("b", "c"), edge("c", "a")];
        for witness in import_sccs(&edges) {
            let original: BTreeSet<(&str, &str)> = edges
                .iter()
                .map(|e| (e.from.as_str(), e.to.as_str()))
                .collect();
            assert!(witness
                .path
                .windows(2)
                .all(|pair| original.contains(&(pair[0].as_str(), pair[1].as_str()))));
            assert_eq!(witness.path.first(), witness.path.last());
        }
        // A wide SCC whose member rotation order is not a directed cycle:
        // edges a->b->c->d->a plus chords a->c, d->b. The old rotation
        // [a,b,c,d,a] contains b->c and c->d (present) — use a shape where
        // rotation invents edges: cycle b->d->c->b plus spoke a->b, a->c.
        let edges = vec![
            edge("a", "b"),
            edge("a", "c"),
            edge("b", "d"),
            edge("d", "c"),
            edge("c", "b"),
        ];
        // SCC: {b, c, d} (a is not in it). Members sorted: b, c, d — the
        // rotation [b,c,d,b] would need c->d and d->b, neither exists.
        let witnesses = import_sccs(&edges);
        let witness = witnesses
            .iter()
            .find(|w| w.members == vec!["b".to_owned(), "c".to_owned(), "d".to_owned()])
            .expect("b/c/d component");
        let original: BTreeSet<(&str, &str)> = edges
            .iter()
            .map(|e| (e.from.as_str(), e.to.as_str()))
            .collect();
        assert!(
            witness
                .path
                .windows(2)
                .all(|pair| original.contains(&(pair[0].as_str(), pair[1].as_str()))),
            "path {:?} must be a real directed cycle",
            witness.path
        );
    }
}
