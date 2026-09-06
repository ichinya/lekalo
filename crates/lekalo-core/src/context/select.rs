//! Deterministic capsule selection over the accepted surfaces (issue #17).
//!
//! The selector consumes the compiled IR, the accepted #13 dependency
//! graph, and the accepted #14 effect graph — all read-only — and emits one
//! ranked, explainable candidate list projected into a [`Selected`]
//! capsule. Selection is pure: no filesystem access, no clock, no
//! environment, no floats; the same inputs always produce the same capsule
//! bytes.
//!
//! Sections are the closed v1 vocabulary in protection order: the root
//! cards and their policies, effects, direct dependencies, scenarios,
//! public impact, and bindings are protected semantic facts (emitted as
//! typed records, never collapsed into prose); the type cards and the
//! bounded closure are ranked supporting context. The budget walk follows
//! that fixed order and excludes whatever no longer fits, one explainable
//! manifest row per candidate.

use std::collections::{HashMap, HashSet, VecDeque};

use super::diagnostic;
use super::model::{
    CardFact, Contract, EdgeFact, EffectFact, ExclusionReason, Fact, Gap, ManifestRow, WireField,
    WireType,
};
use super::version::{
    MAX_BUDGET_TOKENS, MAX_CLOSURE_DEPTH, MAX_CLOSURE_EDGES, MAX_CLOSURE_NODES, MAX_MANIFEST_ITEMS,
    MAX_ROOTS,
};
use crate::diagnostics::DiagnosticSet;
use crate::effects::EffectGraph;
use crate::graph::{DependencyGraph, EdgeFilter, GraphNode, NodeKindId, RelationKindId};
use crate::ir::{Common, CompiledProject, Definition, DefinitionKind};
use crate::loader::{ModelVersion, Position, SourceMapEntry};

/// The closed capsule scope: one symbol, or the typed changed set (a typed
/// handoff in the command line; this module never infers changed symbols).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapsuleScope {
    /// Build the capsule for exactly one symbol.
    Symbol(String),
    /// Build one capsule over the explicitly supplied changed symbols.
    Changed(Vec<String>),
}

/// One selected capsule: everything both projections render.
#[derive(Clone, Debug)]
pub(crate) struct Selected {
    pub project: Option<String>,
    pub model_version: ModelVersion,
    pub ir_digest: String,
    pub mode: &'static str,
    pub budget: u64,
    pub roots: Vec<String>,
    pub sections: Vec<(&'static str, Vec<Fact>)>,
    pub manifest: Vec<ManifestRow>,
    pub gaps: Vec<Gap>,
    pub estimated: u64,
    pub minimum_required: u64,
    pub fits: bool,
    pub complete: bool,
    pub spans: Vec<SpanRow>,
}

/// One declaration-span row of the opt-in evidence sidecar (logical
/// project-relative paths only, resolved through the #8 source map).
#[derive(Clone, Debug)]
pub(crate) struct SpanRow {
    pub id: String,
    pub path: String,
    pub start: Position,
    pub end: Position,
}

/// Plan one capsule.
///
/// A budget of zero or beyond the recorded v1 bound, an unknown root, an
/// empty scope, and an over-bound root set or manifest are fatal `invalid`
/// sets; every other outcome is a valid capsule (a small budget is in-band
/// truncation metadata, never an error).
pub(crate) fn plan(
    scope: &CapsuleScope,
    budget: u64,
    spans: bool,
    project: &CompiledProject,
    source_map: &[SourceMapEntry],
    graph: &DependencyGraph,
    effects: &EffectGraph,
) -> Result<Selected, DiagnosticSet> {
    if budget == 0 || budget > MAX_BUDGET_TOKENS {
        return Err(diagnostic::input_invalid_set("budget-out-of-range"));
    }
    let root_strings: Vec<String> = match scope {
        CapsuleScope::Symbol(symbol) => vec![symbol.clone()],
        CapsuleScope::Changed(symbols) => symbols.clone(),
    };
    if root_strings.is_empty() {
        return Err(diagnostic::input_invalid_set("empty-scope"));
    }
    // Resolve, deduplicate, and canonically order the roots.
    let mut roots: Vec<&GraphNode> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for root in &root_strings {
        let Some(node) = graph.resolve(root) else {
            return Err(crate::graph::diagnostic::unknown_node_set(root));
        };
        if seen.insert(node.id().as_str().to_owned()) {
            roots.push(node);
        }
    }
    if roots.len() > MAX_ROOTS {
        return Err(diagnostic::input_invalid_set("root-limit-exceeded"));
    }
    roots.sort_by_key(|node| node.sort_key());

    // The definition lookup every card builder shares.
    let definitions: HashMap<&str, &Definition> = project
        .definitions
        .iter()
        .map(|definition| (definition.id().as_str(), definition))
        .collect();

    let mut sections: Vec<(&'static str, Vec<Fact>)> = Vec::new();
    let mut push = |key: &'static str, facts: Vec<Fact>| {
        if !facts.is_empty() {
            sections.push((key, facts));
        }
    };

    // Roots: identity, purpose, and the canonical kind contract.
    let root_cards: Vec<Fact> = roots
        .iter()
        .filter_map(|node| root_card(node, project, &definitions))
        .collect();
    let root_ids: Vec<String> = roots
        .iter()
        .map(|node| node.id().as_str().to_owned())
        .collect();

    // Policies: every policy authorizing a root.
    let filter = EdgeFilter::new();
    let mut policy_facts: Vec<Fact> = Vec::new();
    let mut policy_seen: HashSet<String> = HashSet::new();
    for root in &roots {
        for edge in graph.reverse_dependencies(root.id(), &filter) {
            if edge.key().relation() != RelationKindId::AUTHORIZES {
                continue;
            }
            let source = edge.key().from();
            if !policy_seen.insert(source.as_str().to_owned()) {
                continue;
            }
            if let Some(Definition::Policy(policy)) = definitions.get(source.semantic_id()) {
                policy_facts.push(Fact::Policy {
                    id: source.as_str().to_owned(),
                    decision: policy.decision.as_str(),
                    applies_to: policy
                        .applies_to
                        .iter()
                        .map(|id| id.as_str().to_owned())
                        .collect(),
                    description: policy
                        .common
                        .description
                        .as_ref()
                        .map(|text| text.as_str().to_owned()),
                });
            }
        }
    }

    // Effects: every declared edge of the root operations (operations
    // directly, and operations declaring a root effect symbol).
    let mut operations: Vec<crate::effects::OperationId> = Vec::new();
    let mut operation_seen: HashSet<String> = HashSet::new();
    let mut remember_operation = |semantic: &str| {
        if let Some(operation) = crate::effects::OperationId::from_semantic(semantic) {
            if operation_seen.insert(operation.as_str().to_owned()) {
                operations.push(operation);
            }
        }
    };
    for root in &roots {
        match root.kind() {
            NodeKindId::OPERATION => remember_operation(root.id().semantic_id()),
            NodeKindId::EFFECT => {
                for edge in graph.reverse_dependencies(root.id(), &filter) {
                    let Some(source) = graph.node(edge.key().from()) else {
                        continue;
                    };
                    if source.kind() == NodeKindId::OPERATION {
                        remember_operation(source.id().semantic_id());
                    }
                }
            }
            _ => {}
        }
    }
    operations.sort();
    let mut effect_facts: Vec<Fact> = Vec::new();
    for operation in &operations {
        for edge in effects.operation_edges(operation) {
            let key = edge.key();
            let subject = key.subject();
            effect_facts.push(Fact::Effect(EffectFact {
                operation: key.operation().as_str().to_owned(),
                kind: key.kind().key().to_owned(),
                action: key.kind().action().map(|action| action.key()),
                resource_kind: subject.resource().kind().key().to_owned(),
                resource: subject.resource().as_str().to_owned(),
                field: subject.field().map(|field| field.as_str().to_owned()),
                occurrence: key.occurrence(),
                confidence: edge.confidence().as_str(),
            }));
        }
    }

    // Dependencies: every direct outgoing edge of every root.
    let mut dependency_facts: Vec<Fact> = Vec::new();
    for root in &roots {
        for edge in graph.direct_dependencies(root.id(), &filter) {
            dependency_facts.push(edge_fact(edge));
        }
    }

    // Scenarios: every scenario directly covering a root.
    let mut scenario_facts: Vec<Fact> = Vec::new();
    let mut scenario_seen: HashSet<String> = HashSet::new();
    for root in &roots {
        for edge in graph.reverse_dependencies(root.id(), &filter) {
            let Some(source) = graph.node(edge.key().from()) else {
                continue;
            };
            if source.kind() != NodeKindId::SCENARIO {
                continue;
            }
            if !scenario_seen.insert(source.id().as_str().to_owned()) {
                continue;
            }
            if let Some(Definition::Scenario(scenario)) = definitions.get(source.id().semantic_id())
            {
                scenario_facts.push(Fact::Scenario {
                    id: source.id().as_str().to_owned(),
                    summary: scenario.summary.as_str().to_owned(),
                    covers: scenario
                        .covers
                        .iter()
                        .map(|id| id.as_str().to_owned())
                        .collect(),
                });
            }
        }
    }

    // Public impact: direct dependents that are not module-private.
    let mut impact_facts: Vec<Fact> = Vec::new();
    for root in &roots {
        for edge in graph.reverse_dependencies(root.id(), &filter) {
            let Some(source) = graph.node(edge.key().from()) else {
                continue;
            };
            if matches!(source.kind(), NodeKindId::SCENARIO | NodeKindId::POLICY) {
                continue;
            }
            if definitions
                .get(source.id().semantic_id())
                .is_some_and(|definition| {
                    definition.common().visibility == Some(crate::ir::Visibility::Module)
                })
            {
                continue;
            }
            impact_facts.push(edge_fact(edge));
        }
    }

    // Bindings: the target bindings of the roots' modules.
    let root_modules: HashSet<&str> = roots.iter().filter_map(|node| node.module()).collect();
    let mut binding_facts: Vec<Fact> = Vec::new();
    for definition in &project.definitions {
        if definition.kind() != DefinitionKind::TargetBinding {
            continue;
        }
        let node_module = node_module(definition.id().as_str(), graph);
        let Some(owner) = node_module.as_deref() else {
            continue;
        };
        if !root_modules.contains(owner) {
            continue;
        }
        let Definition::TargetBinding(binding) = definition else {
            continue;
        };
        binding_facts.push(Fact::Binding {
            id: format!("target-binding:{}", definition.id().as_str()),
            target: binding.target.as_str().to_owned(),
        });
    }

    // Types: supporting cards for the type symbols the roots reference.
    let mut type_facts: Vec<Fact> = Vec::new();
    let mut type_seen: HashSet<String> = HashSet::new();
    for root in &roots {
        for edge in graph.direct_dependencies(root.id(), &filter) {
            let target = edge.key().to();
            let Some(target_node) = graph.node(target) else {
                continue;
            };
            if target_node.kind() != NodeKindId::TYPE {
                continue;
            }
            if !type_seen.insert(target.as_str().to_owned()) {
                continue;
            }
            if let Some(card) = type_card(target_node, &definitions) {
                type_facts.push(Fact::Card(card));
            }
        }
    }

    // Closure: the bounded supporting walk beyond the direct dependencies.
    let (closure_facts, complete) = closure(&roots, graph, &filter);

    push("symbol", root_cards);
    push("policies", policy_facts);
    push("effects", effect_facts);
    push("dependencies", dependency_facts);
    push("scenarios", scenario_facts);
    push("public-impact", impact_facts);
    push("bindings", binding_facts);
    push("types", type_facts);
    push("closure", closure_facts);

    // Canonical order inside each section; sections in rank order.
    for (_, facts) in &mut sections {
        facts.sort_by_key(fact_id);
    }
    sections.sort_by_key(|(key, _)| super::model::section_rank(key));

    // The budget walk: the only selection decision in the capsule.
    let mut manifest: Vec<ManifestRow> = Vec::new();
    let mut ordered_sections: Vec<(&'static str, Vec<Fact>)> = Vec::new();
    let mut estimated: u64 = 0;
    let mut minimum_required: u64 = 0;
    for (key, facts) in &sections {
        let mut included: Vec<Fact> = Vec::new();
        for fact in facts {
            let tokens = fact.tokens();
            minimum_required += tokens;
            if estimated + tokens <= budget {
                estimated += tokens;
                included.push(fact.clone());
                manifest.push(ManifestRow {
                    id: fact.id(),
                    section: key,
                    tokens,
                    reason: None,
                });
            } else {
                manifest.push(ManifestRow {
                    id: fact.id(),
                    section: key,
                    tokens,
                    reason: Some(ExclusionReason::Budget),
                });
            }
        }
        if !included.is_empty() {
            ordered_sections.push((key, included));
        }
    }
    if manifest.len() > MAX_MANIFEST_ITEMS {
        return Err(crate::graph::diagnostic::traversal_limit_set(
            "context-manifest-limit",
        ));
    }
    let fits = estimated == minimum_required;

    // The closed gap vocabulary.
    let mut gaps: Vec<Gap> = vec![Gap {
        gap: "error-contracts-unrepresentable",
        symbols: Vec::new(),
    }];
    if effects.envelope_count() == 0 {
        gaps.push(Gap {
            gap: "detected-effects-absent",
            symbols: Vec::new(),
        });
    }
    let undescribed: Vec<String> = roots
        .iter()
        .filter(|node| {
            definitions
                .get(node.id().semantic_id())
                .is_some_and(|definition| definition.common().description.is_none())
        })
        .map(|node| node.id().as_str().to_owned())
        .collect();
    if !undescribed.is_empty() {
        gaps.push(Gap {
            gap: "no-description",
            symbols: undescribed,
        });
    }
    if !sections.iter().any(|(key, _)| *key == "policies") {
        gaps.push(Gap {
            gap: "no-policies",
            symbols: Vec::new(),
        });
    }
    let silent_operations: Vec<String> = operations
        .iter()
        .filter(|operation| effects.operation_edges(operation).is_empty())
        .map(|operation| operation.as_str().to_owned())
        .collect();
    if !silent_operations.is_empty() {
        gaps.push(Gap {
            gap: "no-effects",
            symbols: silent_operations,
        });
    }
    if !sections.iter().any(|(key, _)| *key == "scenarios") {
        gaps.push(Gap {
            gap: "no-scenario-coverage",
            symbols: Vec::new(),
        });
    }
    let project_has_bindings = project
        .definitions
        .iter()
        .any(|definition| definition.kind() == DefinitionKind::TargetBinding);
    if project_has_bindings && !sections.iter().any(|(key, _)| *key == "bindings") {
        gaps.push(Gap {
            gap: "no-relevant-bindings",
            symbols: Vec::new(),
        });
    }
    if !complete {
        gaps.push(Gap {
            gap: "closure-bounded",
            symbols: Vec::new(),
        });
    }
    gaps.sort();

    // The opt-in declaration-span evidence path (logical paths only).
    let span_rows = if spans {
        spans_for(&roots, source_map)
    } else {
        Vec::new()
    };

    Ok(Selected {
        project: graph.project_id().map(str::to_owned),
        model_version: graph.model_version(),
        ir_digest: effects.ir_digest().to_owned(),
        mode: match scope {
            CapsuleScope::Symbol(_) => "symbol",
            CapsuleScope::Changed(_) => "changed",
        },
        budget,
        roots: root_ids,
        sections: ordered_sections,
        manifest,
        gaps,
        estimated,
        minimum_required,
        fits,
        complete,
        spans: span_rows,
    })
}

fn fact_id(fact: &Fact) -> String {
    fact.id()
}

fn edge_fact(edge: &crate::graph::GraphEdge) -> Fact {
    let key = edge.key();
    Fact::Edge(EdgeFact {
        relation: key.relation().key().to_owned(),
        from: key.from().as_str().to_owned(),
        to: key.to().as_str().to_owned(),
        occurrence: key.occurrence().get(),
        confidence: edge.confidence().as_str(),
    })
}

/// The owning module of one definition, resolved through the graph.
fn node_module(semantic: &str, graph: &DependencyGraph) -> Option<String> {
    graph
        .resolve(semantic)
        .and_then(|node| node.module().map(str::to_owned))
}

/// The identity-and-contract card of one root.
fn root_card(
    node: &GraphNode,
    project: &CompiledProject,
    definitions: &HashMap<&str, &Definition>,
) -> Option<Fact> {
    let qualified = node.id().as_str();
    let semantic = node.id().semantic_id();
    let common: Option<&Common> = match node.kind() {
        NodeKindId::PROJECT => project.project.as_ref().map(|found| &found.common),
        NodeKindId::MODULE => project
            .modules
            .iter()
            .find(|module| module.id.as_str() == semantic)
            .map(|found| &found.common),
        NodeKindId::REQUIREMENT => None,
        _ => definitions
            .get(semantic)
            .map(|definition| definition.common()),
    };
    let contract = definitions
        .get(semantic)
        .and_then(|definition| contract_of(definition));
    let (version, visibility, portability, description, derived_from) = match &common {
        Some(common) => (
            Some(common.version),
            common.visibility.map(crate::ir::Visibility::as_str),
            common.portability.map(crate::ir::Portability::as_str),
            common
                .description
                .as_ref()
                .map(|text| text.as_str().to_owned()),
            common
                .derived_from
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect::<Vec<_>>(),
        ),
        None => (None, None, None, None, Vec::new()),
    };
    Some(Fact::Card(CardFact {
        qualified: qualified.to_owned(),
        kind: node.kind().key().to_owned(),
        subkind: node.subkind().map(str::to_owned),
        module: node.module().map(str::to_owned),
        version,
        visibility,
        portability,
        derived_from,
        description,
        contract,
        covers_section: "symbol",
    }))
}

/// The canonical contract of one definition.
fn contract_of(definition: &Definition) -> Option<Contract> {
    let field = |field: &crate::ir::Field| WireField {
        name: field.name.as_str().to_owned(),
        required: field.required,
        r#type: wire_type(&field.r#type),
    };
    match definition {
        Definition::Scalar(scalar) => Some(Contract::Scalar {
            base: scalar.base.as_str(),
        }),
        Definition::Enum(enumeration) => Some(Contract::Enum {
            values: enumeration
                .values
                .iter()
                .map(|value| {
                    (
                        value.value.as_str().to_owned(),
                        value
                            .description
                            .as_ref()
                            .map(|text| text.as_str().to_owned()),
                    )
                })
                .collect(),
        }),
        Definition::ValueObject(value_object) => Some(Contract::Fields {
            fields: value_object.fields.iter().map(field).collect(),
            identity: Vec::new(),
        }),
        Definition::Entity(entity) => Some(Contract::Fields {
            fields: entity.fields.iter().map(field).collect(),
            identity: entity
                .identity
                .iter()
                .map(|name| name.as_str().to_owned())
                .collect(),
        }),
        Definition::Command(command) => Some(Contract::Command {
            input: command.input.iter().map(field).collect(),
            effects: command
                .effects
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
        }),
        Definition::Query(query) => Some(Contract::Query {
            reads: query
                .reads
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
            returns: query.returns.as_ref().map(wire_type),
        }),
        Definition::Policy(policy) => Some(Contract::Policy {
            applies_to: policy
                .applies_to
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
            decision: policy.decision.as_str(),
        }),
        Definition::Event(event) => Some(Contract::Event {
            payload: event.payload.iter().map(field).collect(),
        }),
        Definition::Effect(effect) => Some(Contract::Effect {
            operation: effect.operation.as_str(),
            entity: effect.entity.as_str().to_owned(),
            emits: effect
                .emits
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
        }),
        Definition::Endpoint(endpoint) => Some(Contract::Endpoint {
            invokes: endpoint.invokes.as_str().to_owned(),
            method: endpoint.method.as_str(),
            path: endpoint.path.as_str().to_owned(),
        }),
        Definition::Scenario(scenario) => Some(Contract::Scenario {
            covers: scenario
                .covers
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
            summary: scenario.summary.as_str().to_owned(),
        }),
        Definition::TargetBinding(binding) => Some(Contract::Binding {
            target: binding.target.as_str().to_owned(),
        }),
    }
}

/// The supporting card of one type node.
fn type_card(node: &GraphNode, definitions: &HashMap<&str, &Definition>) -> Option<CardFact> {
    let definition = definitions.get(node.id().semantic_id())?;
    let contract = contract_of(definition)?;
    Some(CardFact {
        kind: NodeKindId::TYPE.key().to_owned(),
        qualified: node.id().as_str().to_owned(),
        subkind: None,
        module: None,
        version: Some(definition.common().version),
        visibility: None,
        portability: None,
        derived_from: Vec::new(),
        description: definition
            .common()
            .description
            .as_ref()
            .map(|text| text.as_str().to_owned()),
        contract: Some(contract),
        covers_section: "types",
    })
}

fn wire_type(r#type: &crate::ir::TypeRef) -> WireType {
    match r#type {
        crate::ir::TypeRef::Ref(id) => WireType::Ref(id.as_str().to_owned()),
        crate::ir::TypeRef::List(inner) => WireType::List(Box::new(wire_type(inner))),
        crate::ir::TypeRef::Optional(inner) => WireType::Optional(Box::new(wire_type(inner))),
    }
}

/// The bounded supporting closure: a forward walk beyond depth one. The
/// walk reports `complete = false` — and the capsule carries the
/// `closure-bounded` gap — when a recorded bound stopped it.
fn closure(
    roots: &[&GraphNode],
    graph: &DependencyGraph,
    filter: &EdgeFilter,
) -> (Vec<Fact>, bool) {
    let mut visited: HashSet<String> = HashSet::new();
    let mut frontier: VecDeque<(&crate::graph::NodeId, usize)> = VecDeque::new();
    let mut edges: Vec<EdgeFact> = Vec::new();
    let mut complete = true;
    for root in roots {
        visited.insert(root.id().as_str().to_owned());
        frontier.push_back((root.id(), 0));
    }
    'walk: while let Some((node, depth)) = frontier.pop_front() {
        if depth >= MAX_CLOSURE_DEPTH {
            continue;
        }
        for edge in graph.direct_dependencies(node, filter) {
            let key = edge.key();
            let target = key.to();
            let next_depth = depth + 1;
            // Depth-one edges are the dependencies section's facts.
            if next_depth == 1 {
                if visited.insert(target.as_str().to_owned()) {
                    frontier.push_back((target, next_depth));
                }
                continue;
            }
            if edges.len() >= MAX_CLOSURE_EDGES || visited.len() >= MAX_CLOSURE_NODES {
                complete = false;
                break 'walk;
            }
            edges.push(EdgeFact {
                relation: key.relation().key().to_owned(),
                from: key.from().as_str().to_owned(),
                to: key.to().as_str().to_owned(),
                occurrence: key.occurrence().get(),
                confidence: edge.confidence().as_str(),
            });
            if visited.insert(target.as_str().to_owned()) {
                frontier.push_back((target, next_depth));
            }
        }
    }
    edges.sort_by(|left, right| {
        (
            left.relation.as_str(),
            left.from.as_str(),
            left.to.as_str(),
            left.occurrence,
        )
            .cmp(&(
                right.relation.as_str(),
                right.from.as_str(),
                right.to.as_str(),
                right.occurrence,
            ))
    });
    let facts = edges.into_iter().map(Fact::Edge).collect();
    (facts, complete)
}

/// The declaration-span rows of the roots (kind-aware, first match in
/// canonical order), resolved through the #8 source map. Logical
/// project-relative paths only; no absolute path ever enters a row.
fn spans_for(roots: &[&GraphNode], entries: &[SourceMapEntry]) -> Vec<SpanRow> {
    let mut rows = Vec::new();
    for node in roots {
        let semantic = node.id().semantic_id();
        let match_entry = entries.iter().find(|entry| {
            entry.semantic_id.as_deref() == Some(semantic)
                && pointer_matches(node.kind(), &entry.pointer)
        });
        if let Some(entry) = match_entry {
            rows.push(SpanRow {
                id: node.id().as_str().to_owned(),
                path: entry.path.clone(),
                start: entry.start,
                end: entry.end,
            });
        }
    }
    rows
}

/// Whether one source-map entry can declare a node of this kind.
fn pointer_matches(kind: NodeKindId, pointer: &str) -> bool {
    match kind {
        NodeKindId::PROJECT => pointer == "/project",
        NodeKindId::MODULE => pointer.starts_with("/modules/"),
        _ => !pointer.starts_with("/modules/"),
    }
}

/// The selector seam keeps no state; the estimator identity test pins the
/// public profile identity from the module root.
#[cfg(test)]
mod tests {
    use crate::context::estimate;

    #[test]
    fn estimator_identity_is_stable() {
        assert_eq!(estimate::IDENTITY, "dev.lekalo.estimator.chars-4@1.0.0");
    }
}
