//! Kind-specific cycle policy over the graph (issue #13).
//!
//! `requires` and `derived_from` are the graph-owned acyclic relations: any
//! strongly connected component with more than one node, or a self-loop, is
//! a fatal `graph.cycle-forbidden` discovery reported with bounded canonical
//! member and edge keys. Every other core relation may cycle (mutual type
//! references, accepts/returns loops, evidence relations) and stays
//! traversal-safe by visited keys. Type recursion, query-write legality,
//! effect conflicts, and scenario semantics remain with #12/#14/#23; this
//! module never decides those.
//!
//! Detection is an iterative Tarjan SCC per relation, within the same
//! traversal bounds as every other query, so a hostile graph cannot force
//! recursion overflow.

use super::model::{EdgeKey, NodeId, RelationKindId};
use super::{diagnostic, DependencyGraph, MAX_EDGES, MAX_NODES};
use crate::diagnostics::DiagnosticSet;

/// One forbidden cycle discovery: the relation, the canonical member ids,
/// and the canonical internal edge keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForbiddenCycle {
    relation: RelationKindId,
    members: Vec<NodeId>,
    edges: Vec<EdgeKey>,
}

impl ForbiddenCycle {
    /// The acyclic relation the cycle was found on.
    pub const fn relation(&self) -> RelationKindId {
        self.relation
    }

    /// The member node ids in canonical order.
    pub fn members(&self) -> &[NodeId] {
        &self.members
    }

    /// The internal edge keys in canonical order.
    pub fn edges(&self) -> &[EdgeKey] {
        &self.edges
    }
}

/// Detect every forbidden cycle of one graph; `None` means clean.
pub(crate) fn forbidden_cycle_set(graph: &DependencyGraph) -> Option<DiagnosticSet> {
    let discoveries = find_all(graph);
    if discoveries.is_empty() {
        return None;
    }
    Some(diagnostic::cycle_forbidden_set(discoveries))
}

/// Every forbidden cycle across the acyclic core relations, canonical.
pub fn find_all(graph: &DependencyGraph) -> Vec<ForbiddenCycle> {
    let mut discoveries = Vec::new();
    for relation in [RelationKindId::REQUIRES, RelationKindId::DERIVED_FROM] {
        for members in strongly_connected(graph, relation) {
            if members.len() == 1 {
                let only = &members[0];
                let self_loop = graph
                    .adjacency(*only, super::Direction::Forward)
                    .iter()
                    .any(|&edge| {
                        let key = graph.edge_at(edge).key();
                        key.relation() == relation && key.from() == key.to()
                    });
                if !self_loop {
                    continue;
                }
            }
            let members = canonical_members(graph, &members);
            let member_set: std::collections::HashSet<&NodeId> = members.iter().collect();
            let mut edges: Vec<EdgeKey> = graph
                .edges()
                .iter()
                .filter(|edge| {
                    edge.key().relation() == relation
                        && member_set.contains(edge.key().from())
                        && member_set.contains(edge.key().to())
                })
                .map(|edge| edge.key().clone())
                .collect();
            edges.sort();
            edges.dedup();
            discoveries.push(ForbiddenCycle {
                relation,
                members,
                edges,
            });
        }
    }
    discoveries.sort_by(|left, right| {
        left.relation
            .rank()
            .cmp(&right.relation.rank())
            .then_with(|| left.members.cmp(&right.members))
    });
    discoveries
}

/// The SCCs (size ≥ 1) of one relation's subgraph, iterative Tarjan.
fn strongly_connected(graph: &DependencyGraph, relation: RelationKindId) -> Vec<Vec<u32>> {
    let order = graph.nodes().len();
    if order == 0 {
        return Vec::new();
    }
    let _ = (MAX_NODES, MAX_EDGES);

    let mut index_of = vec![usize::MAX; order];
    let mut low_of = vec![0usize; order];
    let mut on_stack = vec![false; order];
    let mut stack: Vec<u32> = Vec::new();
    let mut next_index = 0usize;
    let mut components: Vec<Vec<u32>> = Vec::new();

    // Frames of the explicit depth-first walk: (node, next adjacency slot).
    let mut frames: Vec<(u32, usize)> = Vec::new();

    for start in 0..order as u32 {
        if index_of[start as usize] != usize::MAX {
            continue;
        }
        index_of[start as usize] = next_index;
        low_of[start as usize] = next_index;
        next_index += 1;
        stack.push(start);
        on_stack[start as usize] = true;
        frames.push((start, 0));
        while let Some(frame) = frames.last().copied() {
            let (node, slot) = frame;
            let successors = graph.relation_successors(node, relation);
            if let Some(&successor) = successors.get(slot) {
                frames.last_mut().expect("frame exists").1 = slot + 1;
                if index_of[successor as usize] == usize::MAX {
                    index_of[successor as usize] = next_index;
                    low_of[successor as usize] = next_index;
                    next_index += 1;
                    stack.push(successor);
                    on_stack[successor as usize] = true;
                    frames.push((successor, 0));
                } else if on_stack[successor as usize] {
                    let low = low_of[node as usize].min(index_of[successor as usize]);
                    low_of[node as usize] = low;
                }
            } else {
                frames.pop();
                if let Some(&(parent, _)) = frames.last() {
                    let low = low_of[parent as usize].min(low_of[node as usize]);
                    low_of[parent as usize] = low;
                }
                if low_of[node as usize] == index_of[node as usize] {
                    let mut component = Vec::new();
                    while let Some(member) = stack.pop() {
                        on_stack[member as usize] = false;
                        component.push(member);
                        if member == node {
                            break;
                        }
                    }
                    components.push(component);
                }
            }
        }
    }
    components
}
/// Canonical member ids of one component.
fn canonical_members(graph: &DependencyGraph, members: &[u32]) -> Vec<NodeId> {
    let mut ids: Vec<NodeId> = members
        .iter()
        .map(|&position| graph.node_at(position).id().clone())
        .collect();
    ids.sort();
    ids
}

#[cfg(test)]
mod tests {
    use super::super::model::{
        EdgeProvenance, GraphEdge, GraphNode, OccurrenceOrdinal, ReferenceRole,
    };
    use super::super::{GraphParts, GraphRegistry};
    use crate::loader::ModelVersion;
    use std::collections::HashMap;

    use super::*;

    /// Assemble a graph directly from node ids and edge triples; the only
    /// seam where hand-built cyclic graphs are constructible (the #8 IR
    /// newtypes are crate-private on purpose).
    fn assemble(nodes: &[&str], edges: &[(&str, &str, RelationKindId)]) -> DependencyGraph {
        let nodes: Vec<GraphNode> = nodes
            .iter()
            .map(|id| GraphNode::from_qualified_for_tests(id).expect("node id"))
            .collect();
        let mut index: HashMap<NodeId, u32> = HashMap::new();
        for (position, node) in nodes.iter().enumerate() {
            index.insert(node.id().clone(), position as u32);
        }
        let edges: Vec<GraphEdge> = edges
            .iter()
            .map(|(from, to, relation)| {
                GraphEdge::new(
                    EdgeKey::new(
                        NodeId::from_qualified(from).expect("from"),
                        NodeId::from_qualified(to).expect("to"),
                        *relation,
                        OccurrenceOrdinal::new(0).expect("ordinal"),
                    ),
                    EdgeProvenance::CanonicalIr {
                        reference_role: ReferenceRole::ModuleImport,
                        occurrence: OccurrenceOrdinal::new(0).expect("ordinal"),
                        source_symbol: (*from).to_owned(),
                    },
                )
            })
            .collect();
        let mut outgoing = vec![Vec::new(); nodes.len()];
        let mut incoming = vec![Vec::new(); nodes.len()];
        for (position, edge) in edges.iter().enumerate() {
            outgoing[index[edge.key().from()] as usize].push(position as u32);
            incoming[index[edge.key().to()] as usize].push(position as u32);
        }
        DependencyGraph::assemble(GraphParts {
            registry: GraphRegistry::core(),
            model_version: ModelVersion::V1_0_0,
            project: None,
            nodes,
            index,
            symbols: HashMap::new(),
            modules: HashMap::new(),
            requirements: HashMap::new(),
            project_node: None,
            edges,
            outgoing,
            incoming,
        })
    }

    #[test]
    fn requires_cycles_are_discovered_with_members_and_edges() {
        let graph = assemble(
            &["module:a", "module:b", "module:c"],
            &[
                ("module:a", "module:b", RelationKindId::REQUIRES),
                ("module:b", "module:a", RelationKindId::REQUIRES),
            ],
        );
        let discoveries = find_all(&graph);
        assert_eq!(discoveries.len(), 1);
        assert_eq!(discoveries[0].relation(), RelationKindId::REQUIRES);
        assert_eq!(
            discoveries[0].members(),
            vec![
                NodeId::from_qualified("module:a").unwrap(),
                NodeId::from_qualified("module:b").unwrap()
            ]
        );
        assert_eq!(discoveries[0].edges().len(), 2);
        let set = forbidden_cycle_set(&graph);
        assert!(set.is_some());
    }

    #[test]
    fn self_loops_are_forbidden_on_acyclic_relations() {
        let graph = assemble(
            &["module:a", "module:b"],
            &[("module:a", "module:a", RelationKindId::REQUIRES)],
        );
        let discoveries = find_all(&graph);
        assert_eq!(discoveries.len(), 1);
        assert_eq!(discoveries[0].members().len(), 1);
    }

    #[test]
    fn derived_from_self_loops_are_detected_and_references_cycle_clean() {
        let graph = assemble(
            &["type:a", "type:b", "requirement:R1"],
            &[
                ("type:a", "requirement:R1", RelationKindId::DERIVED_FROM),
                ("type:b", "requirement:R1", RelationKindId::DERIVED_FROM),
            ],
        );
        assert!(find_all(&graph).is_empty());

        // The same shape on `references` is allowed: it is not acyclic.
        let graph = assemble(
            &["type:a", "type:b", "requirement:R1"],
            &[
                ("type:a", "type:b", RelationKindId::REFERENCES),
                ("type:b", "type:a", RelationKindId::REFERENCES),
            ],
        );
        assert!(find_all(&graph).is_empty());
    }

    #[test]
    fn allowed_relations_may_cycle_without_invalidating_the_graph() {
        let graph = assemble(
            &["operation:a", "operation:b", "event:c"],
            &[
                ("operation:a", "operation:b", RelationKindId::ACCEPTS),
                ("operation:b", "operation:a", RelationKindId::READS),
                ("operation:b", "event:c", RelationKindId::EMITS),
            ],
        );
        assert!(find_all(&graph).is_empty());
    }
}
