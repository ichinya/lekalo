//! Bounded slices and module-boundary analysis (issue #13).
//!
//! [`GraphSlice`] is the context/inspect handoff: a root set closed under a
//! direction and filter within hard node, edge, depth, and byte limits. A
//! slice reports `complete` only when the closure was fully explored;
//! otherwise it carries the bounded truncation reason and the frontier
//! count — it never silently returns the first N results. Module-boundary
//! analysis reports internal, outbound, and inbound edges with the owning
//! module id; visibility legality stays with issue #12.

use std::collections::{HashMap, HashSet, VecDeque};

use super::diagnostic;
use super::model::{GraphEdge, NodeId};
use super::traverse::{Direction, EdgeFilter};
use super::{DependencyGraph, MAX_DEPTH, MAX_EXPORT_BYTES, MAX_FILTER_TERMS};
use super::{MAX_RESULT_EDGES, MAX_RESULT_NODES};
use crate::diagnostics::DiagnosticSet;

/// Why one slice stopped before the full closure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TruncationReason {
    /// The depth bound stopped the frontier.
    DepthBound,
    /// The node bound stopped expansion.
    NodeBound,
    /// The edge bound stopped expansion.
    EdgeBound,
    /// The serialized byte bound stopped the slice.
    ByteBound,
}

impl TruncationReason {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DepthBound => "depth-bound",
            Self::NodeBound => "node-bound",
            Self::EdgeBound => "edge-bound",
            Self::ByteBound => "byte-bound",
        }
    }
}

/// The checked bounds of one slice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SliceSpec {
    filter: EdgeFilter,
    direction: Direction,
    max_depth: usize,
    max_nodes: usize,
    max_edges: usize,
    max_bytes: usize,
}

impl SliceSpec {
    /// The permissive spec at the recorded v1 limits.
    pub fn new(direction: Direction) -> Self {
        Self {
            filter: EdgeFilter::new(),
            direction,
            max_depth: MAX_DEPTH,
            max_nodes: MAX_RESULT_NODES,
            max_edges: MAX_RESULT_EDGES,
            max_bytes: MAX_EXPORT_BYTES,
        }
    }

    /// Narrow the filter.
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

    /// Cap the node count (at most [`MAX_RESULT_NODES`]).
    pub fn with_max_nodes(mut self, max_nodes: usize) -> Result<Self, DiagnosticSet> {
        if max_nodes == 0 || max_nodes > MAX_RESULT_NODES {
            return Err(diagnostic::traversal_limit_set("node-limit"));
        }
        self.max_nodes = max_nodes;
        Ok(self)
    }

    /// Cap the edge count (at most [`MAX_RESULT_EDGES`]).
    pub fn with_max_edges(mut self, max_edges: usize) -> Result<Self, DiagnosticSet> {
        if max_edges == 0 || max_edges > MAX_RESULT_EDGES {
            return Err(diagnostic::traversal_limit_set("edge-limit"));
        }
        self.max_edges = max_edges;
        Ok(self)
    }

    /// Cap the serialized slice size (at most [`MAX_EXPORT_BYTES`]).
    pub fn with_max_bytes(mut self, max_bytes: usize) -> Result<Self, DiagnosticSet> {
        if max_bytes == 0 || max_bytes > MAX_EXPORT_BYTES {
            return Err(diagnostic::traversal_limit_set("byte-limit"));
        }
        self.max_bytes = max_bytes;
        Ok(self)
    }
}

/// One bounded subgraph closure with explicit completeness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphSlice {
    roots: Vec<NodeId>,
    direction: Direction,
    nodes: Vec<NodeId>,
    edges: Vec<GraphEdge>,
    complete: bool,
    truncation: Option<TruncationReason>,
    frontier: usize,
}

impl GraphSlice {
    /// The slice root set (always retained).
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// The slice direction.
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// The slice nodes in canonical order.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// The slice edges in canonical order.
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// Whether the closure was fully explored.
    pub const fn complete(&self) -> bool {
        self.complete
    }

    /// Why the slice is partial, when it is.
    pub const fn truncation(&self) -> Option<TruncationReason> {
        self.truncation
    }

    /// The pending frontier size when the slice stopped.
    pub const fn frontier(&self) -> usize {
        self.frontier
    }
}

/// The edge classes of one module boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleBoundary {
    module: String,
    internal: Vec<GraphEdge>,
    outbound: Vec<GraphEdge>,
    inbound: Vec<GraphEdge>,
}

impl ModuleBoundary {
    /// The analyzed module id.
    pub fn module(&self) -> &str {
        &self.module
    }

    /// Edges with both endpoints owned by the module, canonical order.
    pub fn internal(&self) -> &[GraphEdge] {
        &self.internal
    }

    /// Edges leaving the module, canonical order.
    pub fn outbound(&self) -> &[GraphEdge] {
        &self.outbound
    }

    /// Edges entering the module, canonical order.
    pub fn inbound(&self) -> &[GraphEdge] {
        &self.inbound
    }
}

impl DependencyGraph {
    /// Close a root set into a bounded slice under `spec`.
    pub fn slice(&self, roots: &[NodeId], spec: &SliceSpec) -> Result<GraphSlice, DiagnosticSet> {
        if roots.is_empty() {
            return Err(diagnostic::unknown_node_set(""));
        }
        let mut positions: Vec<u32> = Vec::with_capacity(roots.len());
        for root in roots {
            let Some(&position) = self.index.get(root) else {
                return Err(diagnostic::unknown_node_set(root.as_str()));
            };
            positions.push(position);
        }

        let mut visited: HashSet<u32> = positions.iter().copied().collect();
        let mut nodes: Vec<NodeId> = roots.to_owned();
        let mut edges: Vec<u32> = Vec::new();
        let mut queue: VecDeque<(u32, usize)> = positions
            .iter()
            .map(|&position| (position, 0usize))
            .collect();
        let mut truncation: Option<TruncationReason> = None;
        let mut frontier = 0usize;
        let mut max_depth_seen: HashMap<u32, usize> =
            positions.iter().map(|&position| (position, 0)).collect();

        'walk: while let Some((position, depth)) = queue.pop_front() {
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
                if !visited.contains(&next) && !spec.filter.admits_node(self.node_at(next)) {
                    continue;
                }
                if !visited.contains(&next) {
                    let next_depth = depth + 1;
                    if next_depth > spec.max_depth {
                        truncation = Some(TruncationReason::DepthBound);
                        frontier = queue.len();
                        break 'walk;
                    }
                    if nodes.len() >= spec.max_nodes {
                        truncation = Some(TruncationReason::NodeBound);
                        frontier = queue.len();
                        break 'walk;
                    }
                    visited.insert(next);
                    nodes.push(self.node_at(next).id().clone());
                    max_depth_seen.insert(next, next_depth);
                    queue.push_back((next, next_depth));
                }
                if edges.len() < spec.max_edges {
                    if !edges.contains(&edge_position) {
                        edges.push(edge_position);
                    }
                } else {
                    truncation = Some(TruncationReason::EdgeBound);
                    frontier = queue.len();
                    break 'walk;
                }
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
        Ok(GraphSlice {
            roots: roots.to_owned(),
            direction: spec.direction,
            nodes,
            edges: edges
                .into_iter()
                .map(|edge| self.edge_at(edge).clone())
                .collect(),
            complete: truncation.is_none(),
            truncation,
            frontier,
        })
    }

    /// The boundary classes of one module's edges. Unknown modules fail
    /// closed; visibility legality is issue #12's, never reported here.
    pub fn module_boundary(&self, module: &str) -> Result<ModuleBoundary, DiagnosticSet> {
        if module.len() > MAX_FILTER_TERMS {
            return Err(diagnostic::traversal_limit_set("filter-term-limit"));
        }
        let Some(&_position) = self.modules.get(module) else {
            return Err(diagnostic::unknown_node_set(module));
        };
        let mut boundary = ModuleBoundary {
            module: module.to_owned(),
            internal: Vec::new(),
            outbound: Vec::new(),
            inbound: Vec::new(),
        };
        for edge in self.edges() {
            let from = self.node_at(self.index[edge.key().from()]);
            let to = self.node_at(self.index[edge.key().to()]);
            let from_owned = from.module() == Some(module);
            let to_owned = to.module() == Some(module);
            match (from_owned, to_owned) {
                (true, true) => boundary.internal.push(edge.clone()),
                (true, false) => boundary.outbound.push(edge.clone()),
                (false, true) => boundary.inbound.push(edge.clone()),
                (false, false) => {}
            }
        }
        Ok(boundary)
    }
}
