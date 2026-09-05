//! Bounded deterministic traversal, shortest paths, and filters (issue #13).
//!
//! Traversal is an iterative frontier walk over the precomputed indexes:
//! filters admit edges and discovered nodes before enqueueing (the root is
//! always retained), visited keys follow the filter semantics, results come
//! back in canonical order, and every bound rejects with an explicit
//! registered diagnostic instead of truncating by arrival order. Shortest
//! paths are bounded deterministic BFS with canonical tie breaks (the
//! adjacency lists are already canonical) and the confidence meet of the
//! contributing edges.

use std::collections::{HashMap, HashSet, VecDeque};

use super::diagnostic;
use super::model::{Confidence, GraphEdge, GraphNode, NodeId};
use super::{DependencyGraph, KindSelection, RelationSelection, MAX_DEPTH, MAX_FILTER_TERMS};
use super::{MAX_PATH_NODES, MAX_RESULT_EDGES, MAX_RESULT_NODES};
use crate::diagnostics::DiagnosticSet;

/// The direction of one traversal relative to its root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// Along edge direction: dependencies.
    Forward,
    /// Against edge direction: dependents.
    Reverse,
}

impl Direction {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Reverse => "reverse",
        }
    }
}

/// The typed edge and discovered-node filter of one query.
///
/// An empty relation or kind list admits everything. Filter terms are
/// bounded; unknown or unregistered keys fail closed at construction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EdgeFilter {
    relations: Vec<RelationSelection>,
    kinds: Vec<KindSelection>,
    module: Option<String>,
}

impl EdgeFilter {
    /// The permissive filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Admit only these relations (duplicates collapse; at most
    /// [`MAX_FILTER_TERMS`] terms; unregistered keys are refused).
    pub fn with_relation_keys(
        mut self,
        registry: &super::GraphRegistry,
        keys: &[&str],
    ) -> Result<Self, DiagnosticSet> {
        if keys.len() > MAX_FILTER_TERMS {
            return Err(diagnostic::traversal_limit_set("filter-term-limit"));
        }
        for key in keys {
            let Some(selection) = registry.relation(key) else {
                return Err(diagnostic::unknown_relation_set(key));
            };
            if !self.relations.contains(&selection) {
                self.relations.push(selection);
            }
        }
        Ok(self)
    }

    /// Admit only discovered nodes of these kinds.
    pub fn with_kind_keys(
        mut self,
        registry: &super::GraphRegistry,
        keys: &[&str],
    ) -> Result<Self, DiagnosticSet> {
        if keys.len() > MAX_FILTER_TERMS {
            return Err(diagnostic::traversal_limit_set("filter-term-limit"));
        }
        for key in keys {
            let Some(selection) = registry.kind(key) else {
                return Err(diagnostic::unknown_node_set(key));
            };
            if !self.kinds.contains(&selection) {
                self.kinds.push(selection);
            }
        }
        Ok(self)
    }

    /// Admit only discovered nodes owned by this module.
    pub fn with_module(mut self, module: impl Into<String>) -> Result<Self, DiagnosticSet> {
        let module = module.into();
        if module.is_empty() || module.len() > MAX_FILTER_TERMS {
            return Err(diagnostic::traversal_limit_set("filter-term-limit"));
        }
        self.module = Some(module);
        Ok(self)
    }

    /// Whether one edge passes the relation filter (crate internal).
    pub(crate) fn admits_edge(&self, edge: &GraphEdge) -> bool {
        if self.relations.is_empty() {
            return true;
        }
        let relation = edge.key().relation();
        self.relations.iter().any(
            |selection| matches!(selection, RelationSelection::Core(core) if *core == relation),
        )
    }

    /// Whether one discovered node passes the kind and module filters
    /// (crate internal).
    pub(crate) fn admits_node(&self, node: &GraphNode) -> bool {
        if !self.kinds.is_empty()
            && !self.kinds.iter().any(
                |selection| matches!(selection, KindSelection::Core(core) if *core == node.kind()),
            )
        {
            return false;
        }
        match &self.module {
            Some(module) => node.module() == Some(module.as_str()),
            None => true,
        }
    }
}

/// The checked bounds of one transitive traversal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraversalSpec {
    filter: EdgeFilter,
    direction: Direction,
    max_depth: usize,
    max_nodes: usize,
    max_edges: usize,
}

impl TraversalSpec {
    /// The permissive spec at the recorded v1 limits.
    pub fn new(direction: Direction) -> Self {
        Self {
            filter: EdgeFilter::new(),
            direction,
            max_depth: MAX_DEPTH,
            max_nodes: MAX_RESULT_NODES,
            max_edges: MAX_RESULT_EDGES,
        }
    }

    /// Narrow the relation/node-kind/module filter.
    pub fn with_filter(mut self, filter: EdgeFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Cap the depth (at most [`MAX_DEPTH`]).
    pub fn with_max_depth(mut self, max_depth: usize) -> Result<Self, DiagnosticSet> {
        if max_depth == 0 || max_depth > MAX_DEPTH {
            return Err(diagnostic::traversal_limit_set("depth-limit"));
        }
        self.max_depth = max_depth;
        Ok(self)
    }

    /// Cap the returned node count (at most [`MAX_RESULT_NODES`]).
    pub fn with_max_nodes(mut self, max_nodes: usize) -> Result<Self, DiagnosticSet> {
        if max_nodes == 0 || max_nodes > MAX_RESULT_NODES {
            return Err(diagnostic::traversal_limit_set("node-limit"));
        }
        self.max_nodes = max_nodes;
        Ok(self)
    }

    /// Cap the returned edge count (at most [`MAX_RESULT_EDGES`]).
    pub fn with_max_edges(mut self, max_edges: usize) -> Result<Self, DiagnosticSet> {
        if max_edges == 0 || max_edges > MAX_RESULT_EDGES {
            return Err(diagnostic::traversal_limit_set("edge-limit"));
        }
        self.max_edges = max_edges;
        Ok(self)
    }
}

/// One completed bounded traversal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Traversal {
    root: NodeId,
    direction: Direction,
    nodes: Vec<NodeId>,
    edges: Vec<GraphEdge>,
    max_depth_seen: usize,
    complete: bool,
}

impl Traversal {
    /// The traversal root (always retained, whatever the filter admits).
    pub fn root(&self) -> &NodeId {
        &self.root
    }

    /// The traversal direction.
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// The reached nodes in canonical order, root included.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// The reached edges in canonical order.
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// The deepest frontier distance from the root (root = 0).
    pub const fn max_depth_seen(&self) -> usize {
        self.max_depth_seen
    }

    /// Whether the closure was fully explored within the bounds.
    pub const fn complete(&self) -> bool {
        self.complete
    }
}

/// The checked bounds of one shortest-path query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathSpec {
    filter: EdgeFilter,
    max_nodes: usize,
}

impl PathSpec {
    /// The permissive spec at the recorded path bound.
    pub fn new() -> Self {
        Self {
            filter: EdgeFilter::new(),
            max_nodes: MAX_PATH_NODES,
        }
    }

    /// Narrow the filter.
    pub fn with_filter(mut self, filter: EdgeFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Cap the path length in nodes (at most [`MAX_PATH_NODES`]).
    pub fn with_max_nodes(mut self, max_nodes: usize) -> Result<Self, DiagnosticSet> {
        if !(2..=MAX_PATH_NODES).contains(&max_nodes) {
            return Err(diagnostic::traversal_limit_set("path-limit"));
        }
        self.max_nodes = max_nodes;
        Ok(self)
    }
}

impl Default for PathSpec {
    fn default() -> Self {
        Self::new()
    }
}

/// One shortest path: ordered nodes, ordered edges, length, confidence
/// meet, and completeness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Path {
    from: NodeId,
    to: NodeId,
    nodes: Vec<NodeId>,
    edges: Vec<GraphEdge>,
    confidence: Confidence,
    complete: bool,
}

impl Path {
    /// The path start.
    pub fn from(&self) -> &NodeId {
        &self.from
    }

    /// The path end.
    pub fn to(&self) -> &NodeId {
        &self.to
    }

    /// The ordered node ids, `from` first.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// The ordered edges; edge `i` connects nodes `i` and `i + 1`.
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// The edge count (the hop length).
    pub fn length(&self) -> usize {
        self.edges.len()
    }

    /// The least-trustworthy confidence along the path (the meet).
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Whether the path respects every bound of the spec.
    pub const fn complete(&self) -> bool {
        self.complete
    }
}

impl DependencyGraph {
    /// Transitive closure from one root in one direction under `spec`.
    ///
    /// Bound exhaustion is an explicit `graph.traversal-limit` failure —
    /// never a silently truncated result.
    pub fn transitive(
        &self,
        root: &NodeId,
        spec: &TraversalSpec,
    ) -> Result<Traversal, DiagnosticSet> {
        let Some(&root_position) = self.index.get(root) else {
            return Err(diagnostic::unknown_node_set(root.as_str()));
        };
        let mut visited: HashMap<u32, usize> = HashMap::new();
        let mut queued_edges: HashSet<u32> = HashSet::new();
        visited.insert(root_position, 0);
        let mut queue = VecDeque::from([(root_position, 0usize)]);
        let mut nodes = vec![root.clone()];
        let mut edges: Vec<u32> = Vec::new();
        let mut max_depth_seen = 0usize;

        while let Some((position, depth)) = queue.pop_front() {
            for &edge_position in self.adjacency(position, spec.direction) {
                let edge = self.edge_at(edge_position);
                if !spec.filter.admits_edge(edge) {
                    continue;
                }
                let next = match spec.direction {
                    Direction::Forward => self.position(edge.key().to()),
                    Direction::Reverse => self.position(edge.key().from()),
                };
                let Some(next) = next else {
                    continue;
                };
                if next != root_position && !spec.filter.admits_node(self.node_at(next)) {
                    continue;
                }
                if queued_edges.insert(edge_position) {
                    if edges.len() >= spec.max_edges {
                        return Err(diagnostic::traversal_limit_set("edge-limit"));
                    }
                    edges.push(edge_position);
                }
                if visited.contains_key(&next) {
                    continue;
                }
                let next_depth = depth + 1;
                if next_depth > spec.max_depth {
                    return Err(diagnostic::traversal_limit_set("depth-limit"));
                }
                if nodes.len() >= spec.max_nodes {
                    return Err(diagnostic::traversal_limit_set("node-limit"));
                }
                visited.insert(next, next_depth);
                max_depth_seen = max_depth_seen.max(next_depth);
                nodes.push(self.node_at(next).id().clone());
                queue.push_back((next, next_depth));
            }
        }

        nodes.sort_by(|left, right| {
            let left = self.index[left];
            let right = self.index[right];
            self.node_at(left)
                .sort_key()
                .cmp(&self.node_at(right).sort_key())
        });
        edges.sort_by(|&left, &right| {
            self.edge_at(left)
                .sort_key()
                .cmp(&self.edge_at(right).sort_key())
        });
        Ok(Traversal {
            root: root.clone(),
            direction: spec.direction,
            nodes,
            edges: edges
                .into_iter()
                .map(|edge| self.edge_at(edge).clone())
                .collect(),
            max_depth_seen,
            complete: true,
        })
    }

    /// The deterministic shortest path between two nodes under `spec`.
    ///
    /// Ties break by canonical edge order (BFS over the already-canonical
    /// adjacency lists). No path is an explicit `graph.path-not-found`
    /// result — never an empty success.
    pub fn shortest_path(
        &self,
        from: &NodeId,
        to: &NodeId,
        spec: &PathSpec,
    ) -> Result<Path, DiagnosticSet> {
        let Some(&from_position) = self.index.get(from) else {
            return Err(diagnostic::unknown_node_set(from.as_str()));
        };
        let Some(&to_position) = self.index.get(to) else {
            return Err(diagnostic::unknown_node_set(to.as_str()));
        };
        if from_position == to_position {
            return Ok(Path {
                from: from.clone(),
                to: to.clone(),
                nodes: vec![from.clone()],
                edges: Vec::new(),
                confidence: Confidence::Canonical,
                complete: true,
            });
        }

        let mut parents: HashMap<u32, Option<u32>> = HashMap::new();
        parents.insert(from_position, None);
        let mut queue = VecDeque::from([from_position]);
        let mut found = false;

        while !found {
            let Some(position) = queue.pop_front() else {
                break;
            };
            for &edge_position in self.adjacency(position, Direction::Forward) {
                let edge = self.edge_at(edge_position);
                if !spec.filter.admits_edge(edge) {
                    continue;
                }
                let Some(next) = self.position(edge.key().to()) else {
                    continue;
                };
                if next != to_position && !spec.filter.admits_node(self.node_at(next)) {
                    continue;
                }
                if parents.contains_key(&next) {
                    continue;
                }
                if parents.len() + 1 > spec.max_nodes {
                    return Err(diagnostic::traversal_limit_set("path-limit"));
                }
                parents.insert(next, Some(position));
                if next == to_position {
                    found = true;
                    break;
                }
                queue.push_back(next);
            }
        }

        if !found {
            return Err(diagnostic::path_not_found_set(from.as_str(), to.as_str()));
        }

        let mut node_positions = vec![to_position];
        let mut edge_positions = Vec::new();
        let mut cursor = to_position;
        while cursor != from_position {
            let parent = parents[&cursor];
            let Some(parent) = parent else {
                break;
            };
            let edge_position = self
                .adjacency(parent, Direction::Forward)
                .iter()
                .copied()
                .find(|&candidate| {
                    let edge = self.edge_at(candidate);
                    spec.filter.admits_edge(edge) && self.position(edge.key().to()) == Some(cursor)
                });
            let Some(edge_position) = edge_position else {
                break;
            };
            edge_positions.push(edge_position);
            node_positions.push(parent);
            cursor = parent;
        }
        node_positions.reverse();
        edge_positions.reverse();

        let nodes = node_positions
            .iter()
            .map(|&position| self.node_at(position).id().clone())
            .collect();
        let mut confidence = Confidence::Canonical;
        let edges: Vec<GraphEdge> = edge_positions
            .into_iter()
            .map(|position| {
                let edge = self.edge_at(position);
                confidence = confidence.meet(edge.confidence());
                edge.clone()
            })
            .collect();

        Ok(Path {
            from: from.clone(),
            to: to.clone(),
            nodes,
            edges,
            confidence,
            complete: true,
        })
    }
}
