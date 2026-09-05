//! Issue #13: the deterministic dependency graph of semantic symbols.
//!
//! A read-only, typed projection over the accepted #8 [`CompiledProject`]:
//! direct edges with closed provenance and confidence, precomputed reverse
//! indexes, relation-filtered traversal, bounded slices, shortest paths,
//! kind-specific cycle policy, module-boundary analysis, and canonical
//! export. The graph never parses files, never reads source bytes, never
//! validates semantics (#12 owns that), never detects effects (#14), and
//! never writes anything anywhere.
//!
//! Closed registries: the twelve core node kinds and twelve core relations
//! are registry-backed identifiers with fixed ranks. Future kinds and
//! relations (#14 effects, #23 verifies, adapter contributions) enter as
//! versioned extension records on this registry — additive successors, no
//! preemption of core ids. Unknown required extensions fail closed.
//!
//! Determinism: nodes sort by kind rank, module, semantic id, subkind;
//! edges by endpoints, relation, occurrence, provenance kind. Canonical
//! export is compact UTF-8 JSON with byte-sorted object keys, path
//! independent, byte-identical for the same IR. Reverse lookups answer from
//! the precomputed incoming index — never a source rescan or IR rebuild.

pub mod build;
pub mod canonical;
pub mod cycle;
pub mod diagnostic;
pub mod model;
pub mod slice;
pub mod traverse;
pub mod version;

use std::collections::{BTreeMap, HashMap};

use crate::diagnostics::DiagnosticSet;
use crate::loader::ModelVersion;

pub use model::{
    Confidence, EdgeKey, EdgeProvenance, GraphEdge, GraphNode, NodeId, NodeKindId,
    OccurrenceOrdinal, ReferenceRole, RelationKindId,
};
pub use slice::{GraphSlice, ModuleBoundary, SliceSpec, TruncationReason};
pub use traverse::{Direction, EdgeFilter, Path, PathSpec, Traversal, TraversalSpec};
pub use version::{
    FAMILY, IDENTITY, MAX_DEPTH, MAX_EDGES, MAX_EXPORT_BYTES, MAX_FILTER_TERMS, MAX_NODES,
    MAX_PATH_NODES, MAX_PROVENANCE_RECORDS, MAX_RESULT_EDGES, MAX_RESULT_NODES, SCHEMA_VERSION,
    VERSION,
};

pub use build::{build, build_with_registry};

/// The parts one finished construction assembles into a graph.
pub(crate) struct GraphParts {
    pub registry: GraphRegistry,
    pub model_version: ModelVersion,
    pub project: Option<String>,
    pub nodes: Vec<GraphNode>,
    pub index: HashMap<NodeId, u32>,
    pub symbols: HashMap<String, u32>,
    pub modules: HashMap<String, u32>,
    pub requirements: HashMap<String, u32>,
    pub project_node: Option<u32>,
    pub edges: Vec<GraphEdge>,
    pub outgoing: Vec<Vec<u32>>,
    pub incoming: Vec<Vec<u32>>,
}

/// One finished dependency graph: immutable, deterministically ordered, and
/// safe to share across threads. Every query is side-effect free.
pub struct DependencyGraph {
    registry: GraphRegistry,
    model_version: ModelVersion,
    project: Option<String>,
    nodes: Vec<GraphNode>,
    index: HashMap<NodeId, u32>,
    symbols: HashMap<String, u32>,
    modules: HashMap<String, u32>,
    requirements: HashMap<String, u32>,
    project_node: Option<u32>,
    edges: Vec<GraphEdge>,
    outgoing: Vec<Vec<u32>>,
    incoming: Vec<Vec<u32>>,
}

impl DependencyGraph {
    /// Assemble from finished construction parts (crate internal).
    pub(crate) fn assemble(parts: GraphParts) -> Self {
        Self {
            registry: parts.registry,
            model_version: parts.model_version,
            project: parts.project,
            nodes: parts.nodes,
            index: parts.index,
            symbols: parts.symbols,
            modules: parts.modules,
            requirements: parts.requirements,
            project_node: parts.project_node,
            edges: parts.edges,
            outgoing: parts.outgoing,
            incoming: parts.incoming,
        }
    }

    /// The graph contract identity (`dev.lekalo.graph@1.0.0`).
    pub const fn identity(&self) -> &'static str {
        IDENTITY
    }

    /// The graph contract discriminator (`lekalo/graph/v1.0.0`).
    pub const fn schema_version(&self) -> &'static str {
        SCHEMA_VERSION
    }

    /// The exact source Model version the IR was compiled from.
    pub const fn model_version(&self) -> ModelVersion {
        self.model_version
    }

    /// The project semantic id, when the source declared one.
    pub fn project_id(&self) -> Option<&str> {
        self.project.as_deref()
    }

    /// The registry this graph was built under.
    pub const fn registry(&self) -> &GraphRegistry {
        &self.registry
    }

    /// The node positions reachable from one node along exactly one
    /// relation, in canonical edge order (crate internal, for SCC).
    pub(crate) fn relation_successors(&self, position: u32, relation: RelationKindId) -> Vec<u32> {
        self.outgoing[position as usize]
            .iter()
            .filter(|&&edge| self.edges[edge as usize].key().relation() == relation)
            .map(|&edge| self.index[self.edges[edge as usize].key().to()])
            .collect()
    }

    /// Every node in canonical order.
    pub fn nodes(&self) -> &[GraphNode] {
        &self.nodes
    }

    /// Every direct edge in canonical order.
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// One node by exact kind-qualified id.
    pub fn node(&self, id: &NodeId) -> Option<&GraphNode> {
        self.index
            .get(id)
            .map(|&position| &self.nodes[position as usize])
    }

    /// Resolve one bare semantic id to its node.
    ///
    /// Precedence: symbol definitions, then modules, then the project,
    /// then requirements. The classes are disjoint by grammar except
    /// project/module ids, where the module wins (project ids are
    /// single, globally unique identities a module shadowing one is a
    /// source-level anomaly the loader already rejects).
    pub fn resolve(&self, semantic_id: &str) -> Option<&GraphNode> {
        // An exact kind-qualified id answers first.
        if let Some(qualified) = NodeId::from_qualified(semantic_id) {
            if let Some(&position) = self.index.get(&qualified) {
                return Some(&self.nodes[position as usize]);
            }
        }
        let position = self
            .symbols
            .get(semantic_id)
            .or_else(|| self.modules.get(semantic_id))
            .copied()
            .or_else(|| {
                self.project_node
                    .filter(|_| self.project.as_deref() == Some(semantic_id))
            })
            .or_else(|| self.requirements.get(semantic_id).copied())?;
        Some(&self.nodes[position as usize])
    }

    /// Resolve one bare semantic id to its kind-qualified node id.
    pub fn resolve_id(&self, semantic_id: &str) -> Option<&NodeId> {
        self.resolve(semantic_id).map(GraphNode::id)
    }

    /// Outgoing edges of one node in canonical order under `filter`.
    pub fn outgoing<'a>(&'a self, node: &NodeId, filter: &EdgeFilter) -> Vec<&'a GraphEdge> {
        self.adjacent(node, filter, Direction::Forward)
    }

    /// Incoming edges of one node in canonical order under `filter`.
    ///
    /// Answers from the precomputed reverse index; never scans or rebuilds.
    pub fn incoming<'a>(&'a self, node: &NodeId, filter: &EdgeFilter) -> Vec<&'a GraphEdge> {
        self.adjacent(node, filter, Direction::Reverse)
    }

    /// Direct dependencies (forward adjacency) of one node.
    pub fn direct_dependencies<'a>(
        &'a self,
        node: &NodeId,
        filter: &EdgeFilter,
    ) -> Vec<&'a GraphEdge> {
        self.adjacent(node, filter, Direction::Forward)
    }

    /// Direct dependents (reverse adjacency) of one node.
    pub fn reverse_dependencies<'a>(
        &'a self,
        node: &NodeId,
        filter: &EdgeFilter,
    ) -> Vec<&'a GraphEdge> {
        self.adjacent(node, filter, Direction::Reverse)
    }

    fn adjacent<'a>(
        &'a self,
        node: &NodeId,
        filter: &EdgeFilter,
        direction: Direction,
    ) -> Vec<&'a GraphEdge> {
        let Some(&position) = self.index.get(node) else {
            return Vec::new();
        };
        let slots = match direction {
            Direction::Forward => &self.outgoing[position as usize],
            Direction::Reverse => &self.incoming[position as usize],
        };
        slots
            .iter()
            .filter(|&&edge| filter.admits_edge(&self.edges[edge as usize]))
            .map(|&edge| &self.edges[edge as usize])
            .collect()
    }

    /// The node position index (crate internal, for the submodules).
    pub(crate) fn position(&self, id: &NodeId) -> Option<u32> {
        self.index.get(id).copied()
    }

    pub(crate) fn node_at(&self, position: u32) -> &GraphNode {
        &self.nodes[position as usize]
    }

    pub(crate) fn edge_at(&self, position: u32) -> &GraphEdge {
        &self.edges[position as usize]
    }

    pub(crate) fn adjacency(&self, position: u32, direction: Direction) -> &[u32] {
        match direction {
            Direction::Forward => &self.outgoing[position as usize],
            Direction::Reverse => &self.incoming[position as usize],
        }
    }
    /// The canonical bytes of the whole graph; rejects beyond the export
    /// bound instead of truncating.
    pub fn to_canonical_json(&self) -> Result<String, DiagnosticSet> {
        canonical::graph_bytes(self)
    }
}

/// A resolved relation selection: a core relation or a registered
/// extension record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelationSelection {
    /// A core registry relation.
    Core(RelationKindId),
    /// A registered extension relation (matches no v1 canonical edge).
    Extension(String),
}

/// A resolved node-kind selection: a core kind or a registered extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KindSelection {
    /// A core registry kind.
    Core(NodeKindId),
    /// A registered extension kind (matches no v1 canonical node).
    Extension(String),
}

/// The closed graph registry: the twelve core kinds and twelve core
/// relations plus versioned, namespaced extension records.
///
/// Extension records are typed descriptors, never caller strings: the key
/// is namespaced (`vendor.example/kind`), the version is canonical SemVer,
/// and acyclic policy is explicit. Unknown required extensions fail closed;
/// v1 canonical construction never invents extension nodes or edges.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GraphRegistry {
    extension_kinds: BTreeMap<String, ExtensionKind>,
    extension_relations: BTreeMap<String, ExtensionRelation>,
}

/// One registered extension node kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionKind {
    /// The namespaced key (`vendor.example/kind`, at most 64 bytes).
    pub key: String,
    /// The canonical version of the record (`X.Y.Z`).
    pub version: String,
}

/// One registered extension relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionRelation {
    /// The namespaced key (`vendor.example/rel`, at most 64 bytes).
    pub key: String,
    /// The canonical version of the record (`X.Y.Z`).
    pub version: String,
    /// Whether the extension relation is graph-owned acyclic.
    pub acyclic: bool,
}

impl GraphRegistry {
    /// The closed core-only registry.
    pub fn core() -> Self {
        Self::default()
    }

    /// Register one extension node kind; fails closed on a malformed key
    /// or version, or on re-registration.
    pub fn with_extension_kind(mut self, kind: ExtensionKind) -> Result<Self, DiagnosticSet> {
        if !is_extension_key(&kind.key) || !is_registry_version(&kind.version) {
            return Err(diagnostic::input_invalid_detail("extension-kind-invalid"));
        }
        if self
            .extension_kinds
            .insert(kind.key.clone(), kind)
            .is_some()
        {
            return Err(diagnostic::input_invalid_detail("extension-kind-duplicate"));
        }
        Ok(self)
    }

    /// Register one extension relation; fails closed on a malformed record
    /// or on re-registration.
    pub fn with_extension_relation(
        mut self,
        relation: ExtensionRelation,
    ) -> Result<Self, DiagnosticSet> {
        if !is_extension_key(&relation.key) || !is_registry_version(&relation.version) {
            return Err(diagnostic::input_invalid_detail(
                "extension-relation-invalid",
            ));
        }
        if self
            .extension_relations
            .insert(relation.key.clone(), relation)
            .is_some()
        {
            return Err(diagnostic::input_invalid_detail(
                "extension-relation-duplicate",
            ));
        }
        Ok(self)
    }

    /// Resolve a relation key to a core relation or a registered record.
    pub fn relation(&self, key: &str) -> Option<RelationSelection> {
        if let Some(relation) = RelationKindId::from_key(key) {
            return Some(RelationSelection::Core(relation));
        }
        self.extension_relations
            .contains_key(key)
            .then(|| RelationSelection::Extension(key.to_owned()))
    }

    /// Resolve a node-kind key to a core kind or a registered record.
    pub fn kind(&self, key: &str) -> Option<KindSelection> {
        if let Some(kind) = NodeKindId::from_key(key) {
            return Some(KindSelection::Core(kind));
        }
        self.extension_kinds
            .contains_key(key)
            .then(|| KindSelection::Extension(key.to_owned()))
    }

    /// Every registered extension relation key, sorted.
    pub fn extension_relation_keys(&self) -> Vec<&str> {
        self.extension_relations
            .keys()
            .map(String::as_str)
            .collect()
    }

    /// Every registered extension kind key, sorted.
    pub fn extension_kind_keys(&self) -> Vec<&str> {
        self.extension_kinds.keys().map(String::as_str).collect()
    }
}

/// Whether `key` is a namespaced extension record key: at most 64 bytes,
/// lowercase names separated by dots, one `/`, then the kind name.
fn is_extension_key(key: &str) -> bool {
    if key.is_empty() || key.len() > 64 {
        return false;
    }
    let Some((namespace, name)) = key.split_once('/') else {
        return false;
    };
    let namespaced = !namespace.is_empty()
        && namespace.split('.').all(is_lower_name)
        && namespace.ends_with(|byte: char| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    namespaced && is_lower_name(name) && name.len() <= 63
}

/// One lowercase name segment: letter first, letters/digits/hyphens after.
fn is_lower_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    matches!(bytes.first(), Some(first) if first.is_ascii_lowercase())
        && bytes.len() <= 63
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// Whether `version` is a canonical `X.Y.Z` registry version.
fn is_registry_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && version.len() <= 32
}
