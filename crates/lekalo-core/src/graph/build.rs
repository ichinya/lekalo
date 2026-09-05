//! Deterministic graph construction over the accepted typed IR (issue #13).
//!
//! The builder walks the [`CompiledProject`] in canonical order, emits only
//! the direct relations the accepted IR actually represents, builds both
//! adjacency indexes together, and fails closed on fatal input: a duplicate
//! or unresolved typed identity, a cap violation, or a forbidden cycle
//! produces a registered diagnostic set and no graph at all. It never
//! re-reads source, never materializes transitive edges, and never infers
//! the `writes`/`implements`/`verifies` relations whose typed owners have
//! not contributed evidence yet.

use std::collections::HashMap;

use super::cycle;
use super::diagnostic::{self, InputViolation};
use super::model::{
    EdgeKey, EdgeProvenance, GraphEdge, GraphNode, NodeId, NodeKindId, OccurrenceOrdinal,
    ReferenceRole, RelationKindId,
};
use super::{DependencyGraph, GraphParts, GraphRegistry, MAX_EDGES, MAX_NODES};
use crate::diagnostics::DiagnosticSet;
use crate::ir::{CompiledProject, Definition, DefinitionKind, Field, SymbolId, TypeRef};

/// Build the dependency graph of one compiled project under the core
/// registry.
///
/// The graph is either complete or absent: every fatal input violation is
/// collected into one normalized `invalid` set (exit 1 for CLI callers).
pub fn build(project: &CompiledProject) -> Result<DependencyGraph, DiagnosticSet> {
    build_with_registry(project, &GraphRegistry::core())
}

/// Build the dependency graph under an explicit (possibly extended)
/// registry. Extension descriptors widen filters and future builders;
/// canonical IR construction itself always uses core kinds and relations.
pub fn build_with_registry(
    project: &CompiledProject,
    registry: &GraphRegistry,
) -> Result<DependencyGraph, DiagnosticSet> {
    let mut builder = Builder {
        registry,
        project,
        nodes: Vec::new(),
        index: HashMap::new(),
        symbols: HashMap::new(),
        modules: HashMap::new(),
        requirements: HashMap::new(),
        project_node: None,
        edges: Vec::new(),
        violations: Vec::new(),
    };

    builder.add_project_node();
    builder.add_module_nodes();
    builder.add_definition_nodes();
    builder.add_requirement_nodes();
    builder.emit_module_imports();
    builder.emit_definition_edges();
    builder.emit_requirement_edges();

    if builder.violations.is_empty() && builder.nodes.len() > MAX_NODES {
        builder
            .violations
            .push(InputViolation::CapExceeded { cap: "node-limit" });
    }
    if builder.violations.is_empty() && builder.edges.len() > MAX_EDGES {
        builder
            .violations
            .push(InputViolation::CapExceeded { cap: "edge-limit" });
    }
    if !builder.violations.is_empty() {
        return Err(diagnostic::input_invalid_set(builder.violations));
    }
    let graph = builder.finish()?;
    if let Some(set) = cycle::forbidden_cycle_set(&graph) {
        return Err(set);
    }
    Ok(graph)
}

/// One pending edge record: endpoints as identities, resolved after all
/// nodes exist.
struct PendingEdge {
    from: NodeId,
    to: NodeId,
    relation: RelationKindId,
    occurrence: OccurrenceOrdinal,
    provenance: EdgeProvenance,
}

/// The single-pass construction state.
struct Builder<'a> {
    registry: &'a GraphRegistry,
    project: &'a CompiledProject,
    nodes: Vec<GraphNode>,
    /// Kind-qualified id to node position.
    index: HashMap<NodeId, u32>,
    /// Definition symbol id to node position (lookup precedence one).
    symbols: HashMap<String, u32>,
    /// Module id to node position (lookup precedence two).
    modules: HashMap<String, u32>,
    /// Requirement id to node position (lookup precedence four).
    requirements: HashMap<String, u32>,
    /// The project node position (lookup precedence three).
    project_node: Option<u32>,
    edges: Vec<PendingEdge>,
    violations: Vec<InputViolation>,
}

impl<'a> Builder<'a> {
    /// Freeze the canonical order and assemble the indexed graph. Every
    /// endpoint must resolve; a miss is the defensive terminal state for a
    /// construction bug and fails closed like any other fatal input.
    fn finish(self) -> Result<DependencyGraph, DiagnosticSet> {
        let mut violations = Vec::new();
        for edge in &self.edges {
            for endpoint in [edge.from.as_str(), edge.to.as_str()] {
                if !self.index.contains_key(&node_id_from_qualified(endpoint)) {
                    violations.push(InputViolation::UnresolvedTarget {
                        from: String::new(),
                        role: "graph-edge".to_owned(),
                        target: endpoint.to_owned(),
                    });
                }
            }
        }
        if !violations.is_empty() {
            return Err(diagnostic::input_invalid_set(violations));
        }

        let mut nodes = self.nodes;
        nodes.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        let mut index: HashMap<NodeId, u32> = HashMap::with_capacity(nodes.len());
        let mut symbols: HashMap<String, u32> = HashMap::new();
        let mut modules: HashMap<String, u32> = HashMap::new();
        let mut requirements: HashMap<String, u32> = HashMap::new();
        let mut project_node: Option<u32> = None;
        for (position, node) in nodes.iter().enumerate() {
            let position = position as u32;
            index.insert(node.id().clone(), position);
            match node.kind() {
                NodeKindId::PROJECT => project_node = Some(position),
                NodeKindId::MODULE => {
                    modules.insert(node.id().semantic_id().to_owned(), position);
                }
                NodeKindId::REQUIREMENT => {
                    requirements.insert(node.id().semantic_id().to_owned(), position);
                }
                _ => {
                    symbols.insert(node.id().semantic_id().to_owned(), position);
                }
            }
        }

        let mut edges: Vec<GraphEdge> = self
            .edges
            .into_iter()
            .map(|pending| {
                GraphEdge::new(
                    EdgeKey::new(
                        pending.from,
                        pending.to,
                        pending.relation,
                        pending.occurrence,
                    ),
                    pending.provenance,
                )
            })
            .collect();
        edges.sort_by_key(|edge| edge.sort_key());
        edges.dedup_by(|left, right| left.key() == right.key());

        let mut outgoing = vec![Vec::new(); nodes.len()];
        let mut incoming = vec![Vec::new(); nodes.len()];
        for (position, edge) in edges.iter().enumerate() {
            let from = index[edge.key().from()];
            let to = index[edge.key().to()];
            outgoing[from as usize].push(position as u32);
            incoming[to as usize].push(position as u32);
        }

        Ok(DependencyGraph::assemble(GraphParts {
            registry: self.registry.clone(),
            model_version: self.project.model_version,
            project: self
                .project
                .project
                .as_ref()
                .map(|project| project.id.as_str().to_owned()),
            nodes,
            index,
            symbols,
            modules,
            requirements,
            project_node,
            edges,
            outgoing,
            incoming,
        }))
    }

    /// Register one node; a duplicate identity is a fatal input violation.
    fn add_node(&mut self, node: GraphNode) {
        if self.index.contains_key(node.id()) {
            self.violations.push(InputViolation::DuplicateNode {
                node: node.id().as_str().to_owned(),
            });
            return;
        }
        let Ok(position) = u32::try_from(self.nodes.len()) else {
            self.violations
                .push(InputViolation::CapExceeded { cap: "node-limit" });
            return;
        };
        self.index.insert(node.id().clone(), position);
        self.nodes.push(node);
    }

    fn add_project_node(&mut self) {
        let Some(project) = &self.project.project else {
            return;
        };
        if let Some(node) = GraphNode::new(NodeKindId::PROJECT, project.id.as_str(), None, None) {
            self.project_node = Some(self.nodes.len() as u32);
            self.add_node(node);
        }
    }

    fn add_module_nodes(&mut self) {
        for module in &self.project.modules {
            if let Some(node) = GraphNode::new(
                NodeKindId::MODULE,
                module.id.as_str(),
                Some(module.id.as_str().to_owned()),
                None,
            ) {
                self.modules
                    .insert(module.id.as_str().to_owned(), self.nodes.len() as u32);
                self.add_node(node);
            }
        }
    }

    fn add_definition_nodes(&mut self) {
        for definition in &self.project.definitions {
            let symbol = definition.id();
            let (kind, subkind) = definition_node_kind(definition.kind());
            if let Some(node) =
                GraphNode::new(kind, symbol.as_str(), owner(symbol.as_str()), subkind)
            {
                self.symbols
                    .insert(symbol.as_str().to_owned(), self.nodes.len() as u32);
                self.add_node(node);
            }
        }
    }

    /// Every requirement named by any `derived_from` (the project
    /// definition included) becomes a stable node: requirements stay
    /// unresolved identifiers by design (#36 owns any later provider
    /// resolution), so a missing requirement is a node, not an error.
    fn add_requirement_nodes(&mut self) {
        if let Some(project) = &self.project.project {
            for requirement in &project.common.derived_from {
                self.add_requirement_node(requirement.as_str());
            }
        }
        for definition in &self.project.definitions {
            for requirement in &definition.common().derived_from {
                self.add_requirement_node(requirement.as_str());
            }
        }
    }

    fn add_requirement_node(&mut self, requirement: &str) {
        if self.requirements.contains_key(requirement) {
            return;
        }
        if let Some(node) = GraphNode::new(NodeKindId::REQUIREMENT, requirement, None, None) {
            self.requirements
                .insert(requirement.to_owned(), self.nodes.len() as u32);
            self.add_node(node);
        }
    }

    fn emit_module_imports(&mut self) {
        for module in &self.project.modules {
            for (ordinal, import) in module.imports.iter().enumerate() {
                let Some(to) = self.qualified_module(import.as_str()) else {
                    self.violations.push(InputViolation::UnresolvedTarget {
                        from: module.id.as_str().to_owned(),
                        role: ReferenceRole::ModuleImport.as_str().to_owned(),
                        target: import.as_str().to_owned(),
                    });
                    continue;
                };
                let Some(occurrence) = OccurrenceOrdinal::new(ordinal) else {
                    continue;
                };
                self.edges.push(PendingEdge {
                    from: NodeId::new(NodeKindId::MODULE, module.id.as_str())
                        .expect("module id is node-safe"),
                    to,
                    relation: RelationKindId::REQUIRES,
                    occurrence,
                    provenance: canonical_provenance(
                        ReferenceRole::ModuleImport,
                        occurrence,
                        module.id.as_str(),
                    ),
                });
            }
        }
    }

    fn emit_definition_edges(&mut self) {
        for definition in &self.project.definitions {
            let symbol = definition.id().as_str();
            match definition {
                Definition::Scalar(_) | Definition::Enum(_) => {}
                Definition::ValueObject(value_object) => {
                    self.emit_field_leaves(
                        symbol,
                        ReferenceRole::ValueObjectField,
                        RelationKindId::REFERENCES,
                        &value_object.fields,
                    );
                }
                Definition::Entity(entity) => {
                    self.emit_field_leaves(
                        symbol,
                        ReferenceRole::EntityField,
                        RelationKindId::REFERENCES,
                        &entity.fields,
                    );
                }
                Definition::Command(command) => {
                    self.emit_field_leaves(
                        symbol,
                        ReferenceRole::CommandInput,
                        RelationKindId::ACCEPTS,
                        &command.input,
                    );
                    for (ordinal, effect) in command.effects.iter().enumerate() {
                        self.emit_symbol_edge(
                            symbol,
                            effect,
                            ReferenceRole::CommandEffect,
                            RelationKindId::REFERENCES,
                            ordinal,
                        );
                    }
                }
                Definition::Query(query) => {
                    for (ordinal, read) in query.reads.iter().enumerate() {
                        self.emit_symbol_edge(
                            symbol,
                            read,
                            ReferenceRole::QueryReads,
                            RelationKindId::READS,
                            ordinal,
                        );
                    }
                    if let Some(returns) = &query.returns {
                        self.emit_type_edge(
                            symbol,
                            type_leaf(returns),
                            ReferenceRole::QueryReturns,
                            RelationKindId::RETURNS,
                            0,
                        );
                    }
                }
                Definition::Policy(policy) => {
                    for (ordinal, operation) in policy.applies_to.iter().enumerate() {
                        self.emit_symbol_edge(
                            symbol,
                            operation,
                            ReferenceRole::PolicyAppliesTo,
                            RelationKindId::AUTHORIZES,
                            ordinal,
                        );
                    }
                }
                Definition::Event(event) => {
                    self.emit_field_leaves(
                        symbol,
                        ReferenceRole::EventPayload,
                        RelationKindId::REFERENCES,
                        &event.payload,
                    );
                }
                Definition::Effect(effect) => {
                    self.emit_symbol_edge(
                        symbol,
                        &effect.entity,
                        ReferenceRole::EffectEntity,
                        RelationKindId::REFERENCES,
                        0,
                    );
                    for (ordinal, emitted) in effect.emits.iter().enumerate() {
                        self.emit_symbol_edge(
                            symbol,
                            emitted,
                            ReferenceRole::EffectEmits,
                            RelationKindId::EMITS,
                            ordinal,
                        );
                    }
                }
                Definition::Endpoint(endpoint) => {
                    self.emit_symbol_edge(
                        symbol,
                        &endpoint.invokes,
                        ReferenceRole::EndpointInvokes,
                        RelationKindId::EXPOSES,
                        0,
                    );
                }
                Definition::Scenario(scenario) => {
                    for (ordinal, covered) in scenario.covers.iter().enumerate() {
                        self.emit_symbol_edge(
                            symbol,
                            covered,
                            ReferenceRole::ScenarioCovers,
                            RelationKindId::REFERENCES,
                            ordinal,
                        );
                    }
                }
                Definition::TargetBinding(_) => {}
            }
        }
    }

    /// `derived_from` edges: requirement provenance of every definition.
    fn emit_requirement_edges(&mut self) {
        if let Some(project) = &self.project.project {
            let symbol = project.id.as_str();
            for (ordinal, requirement) in project.common.derived_from.iter().enumerate() {
                self.emit_requirement_edge(symbol, requirement.as_str(), ordinal);
            }
        }
        for definition in &self.project.definitions {
            let symbol = definition.id().as_str();
            for (ordinal, requirement) in definition.common().derived_from.iter().enumerate() {
                self.emit_requirement_edge(symbol, requirement.as_str(), ordinal);
            }
        }
    }

    /// One `derived_from` edge from a definition or the project itself.
    fn emit_requirement_edge(&mut self, symbol: &str, requirement: &str, ordinal: usize) {
        let Some(occurrence) = OccurrenceOrdinal::new(ordinal) else {
            return;
        };
        let Some(to) = NodeId::new(NodeKindId::REQUIREMENT, requirement) else {
            return;
        };
        let from = if self.project_node_position(symbol) {
            NodeId::new(NodeKindId::PROJECT, symbol).expect("project id is node-safe")
        } else {
            self.qualified_self_symbol(symbol)
        };
        self.edges.push(PendingEdge {
            from,
            to,
            relation: RelationKindId::DERIVED_FROM,
            occurrence,
            provenance: canonical_provenance(ReferenceRole::DerivedFrom, occurrence, symbol),
        });
    }

    /// Whether this semantic id is the project node's id.
    fn project_node_position(&self, symbol: &str) -> bool {
        self.project
            .project
            .as_ref()
            .is_some_and(|project| project.id.as_str() == symbol)
    }

    /// One type-leaf edge to the node of the referenced symbol, whatever
    /// type-ish kind the IR gave it; the target must exist.
    fn emit_type_edge(
        &mut self,
        symbol: &str,
        target: &SymbolId,
        role: ReferenceRole,
        relation: RelationKindId,
        ordinal: usize,
    ) {
        let Some(occurrence) = OccurrenceOrdinal::new(ordinal) else {
            return;
        };
        let Some(to) = self.qualified_probe(target.as_str()) else {
            self.violations.push(InputViolation::UnresolvedTarget {
                from: symbol.to_owned(),
                role: role.as_str().to_owned(),
                target: target.as_str().to_owned(),
            });
            return;
        };
        let from = self.qualified_self_symbol(symbol);
        self.edges.push(PendingEdge {
            from,
            to,
            relation,
            occurrence,
            provenance: canonical_provenance(role, occurrence, symbol),
        });
    }

    /// One symbol edge to a target resolved as any definition kind.
    fn emit_symbol_edge(
        &mut self,
        symbol: &str,
        target: &SymbolId,
        role: ReferenceRole,
        relation: RelationKindId,
        ordinal: usize,
    ) {
        self.emit_type_edge(symbol, target, role, relation, ordinal);
    }

    /// Type leaves of one field list under `role` (field index = ordinal).
    fn emit_field_leaves(
        &mut self,
        symbol: &str,
        role: ReferenceRole,
        relation: RelationKindId,
        fields: &[Field],
    ) {
        for (ordinal, field) in fields.iter().enumerate() {
            self.emit_type_edge(symbol, type_leaf(&field.r#type), role, relation, ordinal);
        }
    }

    /// The qualified id of the definition this builder is emitting from.
    ///
    /// Every definition node was registered above, so the probe cannot
    /// miss; the violation keeps the impossible state non-panicking.
    fn qualified_self_symbol(&mut self, symbol: &str) -> NodeId {
        match self.qualified_probe(symbol) {
            Some(id) => id,
            None => {
                self.violations.push(InputViolation::MissingNode {
                    node: symbol.to_owned(),
                });
                NodeId::new(NodeKindId::ENTITY, symbol).expect("definition id is node-safe")
            }
        }
    }

    /// Resolve a definition symbol to its qualified node id: exactly one
    /// definition of any kind may carry it.
    fn qualified_probe(&self, symbol: &str) -> Option<NodeId> {
        for kind in [
            NodeKindId::TYPE,
            NodeKindId::ENTITY,
            NodeKindId::OPERATION,
            NodeKindId::POLICY,
            NodeKindId::EVENT,
            NodeKindId::EFFECT,
            NodeKindId::ENDPOINT,
            NodeKindId::SCENARIO,
            NodeKindId::TARGET_BINDING,
        ] {
            let id = NodeId::new(kind, symbol)?;
            if self.index.contains_key(&id) {
                return Some(id);
            }
        }
        None
    }

    /// Resolve one module id to its qualified node id.
    fn qualified_module(&self, module: &str) -> Option<NodeId> {
        let id = NodeId::new(NodeKindId::MODULE, module)?;
        self.index.contains_key(&id).then_some(id)
    }
}

/// Rebuild a [`NodeId`] from its exact qualified wire form.
fn node_id_from_qualified(qualified: &str) -> NodeId {
    NodeId::from_qualified(qualified).expect("edge endpoints are registered node ids")
}

/// Canonical provenance for one decoded IR reference site.
fn canonical_provenance(
    role: ReferenceRole,
    occurrence: OccurrenceOrdinal,
    source_symbol: &str,
) -> EdgeProvenance {
    EdgeProvenance::CanonicalIr {
        reference_role: role,
        occurrence,
        source_symbol: source_symbol.to_owned(),
    }
}

/// The node kind and subkind of one definition kind.
fn definition_node_kind(kind: DefinitionKind) -> (NodeKindId, Option<&'static str>) {
    match kind {
        DefinitionKind::Scalar => (NodeKindId::TYPE, Some("scalar")),
        DefinitionKind::Enum => (NodeKindId::TYPE, Some("enum")),
        DefinitionKind::ValueObject => (NodeKindId::TYPE, Some("value-object")),
        DefinitionKind::Entity => (NodeKindId::ENTITY, None),
        DefinitionKind::Command => (NodeKindId::OPERATION, Some("command")),
        DefinitionKind::Query => (NodeKindId::OPERATION, Some("query")),
        DefinitionKind::Policy => (NodeKindId::POLICY, None),
        DefinitionKind::Event => (NodeKindId::EVENT, None),
        DefinitionKind::Effect => (NodeKindId::EFFECT, None),
        DefinitionKind::Endpoint => (NodeKindId::ENDPOINT, None),
        DefinitionKind::Scenario => (NodeKindId::SCENARIO, None),
        DefinitionKind::TargetBinding => (NodeKindId::TARGET_BINDING, None),
    }
}

/// The owning module id of one symbol id: its first dot segment, exactly
/// the accepted #12 owner derivation. A dot-less id owns no module.
fn owner(symbol: &str) -> Option<String> {
    symbol.split_once('.').map(|(module, _)| module.to_owned())
}

/// The leaf symbol of a type expression: unwrap list/optional wrappers.
fn type_leaf(type_ref: &TypeRef) -> &SymbolId {
    match type_ref {
        TypeRef::Ref(symbol) => symbol,
        TypeRef::List(inner) | TypeRef::Optional(inner) => type_leaf(inner),
    }
}
