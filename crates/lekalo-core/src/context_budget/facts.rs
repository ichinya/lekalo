//! Typed fact collection for the context-budget ledger (issue #75).
//!
//! One subject's required semantic facts F, supporting semantic facts O,
//! and optional source recipes S are collected by a deterministic
//! fixed-point walk over the accepted surfaces: root contracts, reachable
//! contracts (types, entities, events) needed to interpret them, incoming
//! policy applicability, declared/detected effects of applicable
//! operations, and directly covering scenarios. Narrative descriptions
//! are deliberately kept **inside** their contract cards (they are part
//! of the declared contract the issue counts as required); free-floating
//! summaries that attach to no selected contract are supporting.
//!
//! Every fact carries its stable id, its class, and its estimated token
//! cost under the pinned estimator. Facts dedup by exact identity with
//! first-writer-wins provenance; the walk is bounded and every bound hit
//! marks the selection incomplete (never a silently smaller F).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::version::MAX_GAPS;
use crate::context::estimate;
use crate::effects::EffectGraph;
use crate::graph::{DependencyGraph, EdgeFilter, GraphNode, NodeKindId};
use crate::ir::{Common, CompiledProject, Definition};

/// The closed fact-class vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum FactClass {
    /// Required to interpret the subject's contract (F).
    RequiredSemantic,
    /// Ranked context that may be dropped (O).
    SupportingSemantic,
    /// Whole-file source bytes (S), never collected here.
    OptionalSource,
}

impl FactClass {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::RequiredSemantic => "required-semantic",
            Self::SupportingSemantic => "supporting-semantic",
            Self::OptionalSource => "optional-source",
        }
    }
}

/// The closed inclusion-reason vocabulary of one required fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum InclusionReason {
    /// The subject's own declared contract.
    SubjectContract,
    /// A contract reachable to interpret another required contract.
    ReferencedContract,
    /// A policy whose applies-to names a selected subject.
    PolicyApplicability,
    /// A declared or detected effect of an applicable operation.
    OperationEffect,
    /// A scenario directly covering a selected subject.
    ScenarioCoverage,
}

impl InclusionReason {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::SubjectContract => "subject-contract",
            Self::ReferencedContract => "referenced-contract",
            Self::PolicyApplicability => "policy-applicability",
            Self::OperationEffect => "operation-effect",
            Self::ScenarioCoverage => "scenario-coverage",
        }
    }
}

/// One typed ledger fact: identity, class, reason, cost, and owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerFact {
    /// The kind-qualified graph id (or synthesized effect-edge id).
    pub id: String,
    pub class: FactClass,
    /// `Some` exactly for required facts.
    pub reason: Option<InclusionReason>,
    /// The owning module of the fact's node, when the graph has one.
    pub module: Option<String>,
    /// The estimated token cost of the fact's semantic content.
    pub tokens: u64,
}

/// One closed gap row: why coverage or completeness is reduced.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum FactGap {
    /// A frontier/work bound stopped the walk.
    ClosureBounded,
    /// Applicable detected effects are absent (no detection capability).
    DetectedEffectsAbsent,
    /// The compiled project carries error contracts this selection
    /// cannot represent.
    ErrorContractsUnrepresentable,
}

impl FactGap {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::ClosureBounded => "closure-bounded",
            Self::DetectedEffectsAbsent => "detected-effects-absent",
            Self::ErrorContractsUnrepresentable => "error-contracts-unrepresentable",
        }
    }
}

/// One subject's collected fact selection.
#[derive(Clone, Debug)]
pub struct FactSelection {
    /// Required facts F, canonical order.
    pub required: Vec<LedgerFact>,
    /// Supporting facts O, canonical order.
    pub supporting: Vec<LedgerFact>,
    /// The distinct operation ids whose effects were counted.
    pub counted_operations: Vec<String>,
    /// The gap vocabulary observed during collection.
    pub gaps: Vec<FactGap>,
    /// Whether the fixed point was proven inside every bound.
    pub complete: bool,
}

impl FactSelection {
    /// The sum of required fact tokens.
    pub fn required_tokens(&self) -> u64 {
        self.required.iter().map(|fact| fact.tokens).sum()
    }

    /// The sum of supporting fact tokens.
    pub fn supporting_tokens(&self) -> u64 {
        self.supporting.iter().map(|fact| fact.tokens).sum()
    }
}

/// The per-project lookup tables shared by every subject of one report.
pub(crate) struct FactContext<'a> {
    pub(crate) definitions: BTreeMap<&'a str, &'a Definition>,
    pub(crate) filter: EdgeFilter,
}

impl<'a> FactContext<'a> {
    pub(crate) fn new(project: &'a CompiledProject) -> Self {
        let definitions: BTreeMap<&'a str, &'a Definition> = project
            .definitions
            .iter()
            .map(|definition| (definition.id().as_str(), definition))
            .collect();
        Self {
            definitions,
            filter: EdgeFilter::new(),
        }
    }
}

/// The semantic content string of one definition (the estimator input).
///
/// Identity, kind, module, version, visibility, portability, description,
/// and the canonical contract fields, joined by single spaces in a fixed
/// order — the same content rule the accepted capsule uses for its cards.
pub(crate) fn definition_content(
    node: &GraphNode,
    definition: &Definition,
    module: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(node.kind().key().to_owned());
    parts.push(node.id().as_str().to_owned());
    if let Some(module) = module {
        parts.push(module.to_owned());
    }
    let common: &Common = definition.common();
    parts.push(common.version.to_string());
    if let Some(visibility) = common.visibility {
        parts.push(visibility.as_str().to_owned());
    }
    if let Some(portability) = common.portability {
        parts.push(portability.as_str().to_owned());
    }
    for requirement in &common.derived_from {
        parts.push(requirement.as_str().to_owned());
    }
    if let Some(description) = &common.description {
        parts.push(description.as_str().to_owned());
    }
    contract_parts(definition, &mut parts);
    parts.join(" ")
}

/// The canonical spelling of one closed type reference: `planner.x`,
/// `planner.x?`, or `list<planner.x>` (the loader type-sugar content).
fn type_spelling(reference: &crate::ir::TypeRef) -> String {
    match reference {
        crate::ir::TypeRef::Ref(symbol) => symbol.as_str().to_owned(),
        crate::ir::TypeRef::List(inner) => format!("list<{}>", type_spelling(inner)),
        crate::ir::TypeRef::Optional(inner) => format!("{}?", type_spelling(inner)),
    }
}

/// The contract field content of one definition, in fixed field order.
fn contract_parts(definition: &Definition, parts: &mut Vec<String>) {
    let field = |field: &crate::ir::Field, parts: &mut Vec<String>| {
        parts.push(field.name.as_str().to_owned());
        if field.required {
            parts.push("required".to_owned());
        }
        parts.push(type_spelling(&field.r#type));
    };
    match definition {
        Definition::Scalar(scalar) => parts.push(scalar.base.as_str().to_owned()),
        Definition::Enum(enumeration) => {
            for value in &enumeration.values {
                parts.push(value.value.as_str().to_owned());
                if let Some(description) = &value.description {
                    parts.push(description.as_str().to_owned());
                }
            }
        }
        Definition::ValueObject(value_object) => {
            for member in &value_object.fields {
                field(member, parts);
            }
        }
        Definition::Entity(entity) => {
            for member in &entity.fields {
                field(member, parts);
            }
            for name in &entity.identity {
                parts.push(name.as_str().to_owned());
            }
        }
        Definition::Command(command) => {
            for member in &command.input {
                field(member, parts);
            }
            for effect in &command.effects {
                parts.push(effect.as_str().to_owned());
            }
        }
        Definition::Query(query) => {
            for read in &query.reads {
                parts.push(read.as_str().to_owned());
            }
            if let Some(returns) = &query.returns {
                parts.push(type_spelling(returns));
            }
        }
        Definition::Policy(policy) => {
            parts.push(policy.decision.as_str().to_owned());
            for applies in &policy.applies_to {
                parts.push(applies.as_str().to_owned());
            }
        }
        Definition::Event(event) => {
            for member in &event.payload {
                field(member, parts);
            }
        }
        Definition::Effect(effect) => {
            parts.push(effect.operation.as_str().to_owned());
            parts.push(effect.entity.as_str().to_owned());
            for emits in &effect.emits {
                parts.push(emits.as_str().to_owned());
            }
        }
        Definition::Endpoint(endpoint) => {
            parts.push(endpoint.method.as_str().to_owned());
            parts.push(endpoint.path.as_str().to_owned());
            parts.push(endpoint.invokes.as_str().to_owned());
        }
        Definition::Scenario(scenario) => {
            parts.push(scenario.summary.as_str().to_owned());
            for covers in &scenario.covers {
                parts.push(covers.as_str().to_owned());
            }
        }
        Definition::TargetBinding(binding) => parts.push(binding.target.as_str().to_owned()),
    }
}

/// The fact id of one graph node.
fn node_fact_id(node: &GraphNode) -> String {
    node.id().as_str().to_owned()
}

/// The synthesized fact id of one effect edge occurrence.
fn effect_fact_id(
    operation: &crate::effects::OperationId,
    edge: &crate::effects::EffectEdge,
) -> String {
    let key = edge.key();
    let subject = match key.subject().field() {
        Some(field) => format!("{}.{}", key.subject().resource(), field),
        None => key.subject().resource().to_string(),
    };
    format!(
        "effect:{}:{}->{}:{}#{}",
        key.kind(),
        operation.as_str(),
        key.subject().resource().kind().key(),
        subject,
        key.occurrence()
    )
}

/// Collect one subject's fact selection by the deterministic fixed point.
pub(crate) fn collect_facts(
    root_nodes: &[&GraphNode],
    graph: &DependencyGraph,
    effects: &EffectGraph,
    context: &FactContext<'_>,
    limits: super::closure::ClosureLimits,
) -> FactSelection {
    let mut selection = FactSelection::new();
    let mut required_ids: BTreeMap<String, InclusionReason> = BTreeMap::new();
    let mut supporting_ids: BTreeSet<String> = BTreeSet::new();
    let mut frontier: VecDeque<(String, InclusionReason)> = VecDeque::new();
    // Nodes discovered as incoming applicability whose own forward edges
    // must not expand into F (their other covered subjects are peers).
    let mut no_expand: BTreeSet<String> = BTreeSet::new();

    // Seed: subject contracts.
    for node in root_nodes {
        required_ids.insert(node_fact_id(node), InclusionReason::SubjectContract);
        frontier.push_back((
            node.id().as_str().to_owned(),
            InclusionReason::ReferencedContract,
        ));
    }

    // The fixed point: every required contract pulls its referenced
    // contracts; incoming policies, effects, and scenarios attach with
    // their own reasons.
    while let Some((semantic, reason)) = frontier.pop_front() {
        if required_ids.len() as u64 > limits.max_facts
            || supporting_ids.len() as u64 > limits.max_facts
        {
            selection.complete = false;
            selection.gaps.push(FactGap::ClosureBounded);
            break;
        }
        let Some(node) = graph.resolve(&semantic) else {
            continue;
        };
        // Referenced contracts of this node; applicability-only nodes
        // (policies/scenarios reached as required facts) do not expand.
        if no_expand.contains(&semantic) {
            continue;
        }
        for edge in graph.direct_dependencies(node.id(), &context.filter) {
            let target = edge.key().to();
            let target_id = target.as_str().to_owned();
            if required_ids.contains_key(&target_id) {
                continue;
            }
            let Some(target_node) = graph.node(target) else {
                continue;
            };
            // Requirements are stable ids only; they never carry contracts.
            if target_node.kind() == NodeKindId::REQUIREMENT
                || target_node.kind() == NodeKindId::MODULE
                || target_node.kind() == NodeKindId::PROJECT
            {
                continue;
            }
            required_ids.insert(target_id.clone(), reason);
            frontier.push_back((target_id, reason));
        }

        // Incoming policy applicability and scenario coverage: the
        // policy/scenario fact itself is required, but its *other*
        // covered subjects are peer context, not contracts needed to
        // interpret this subject — they never enter F, and the
        // policy/scenario node is not re-expanded through its forward
        // applicability edges (its own payload fields still arrive
        // through the contract card content).
        for edge in graph.reverse_dependencies(node.id(), &context.filter) {
            let source = edge.key().from();
            let source_id = source.as_str().to_owned();
            let Some(source_node) = graph.node(source) else {
                continue;
            };
            match source_node.kind() {
                NodeKindId::POLICY => {
                    if required_ids
                        .insert(source_id.clone(), InclusionReason::PolicyApplicability)
                        .is_none()
                    {
                        // Mark expansion so the frontier walk skips this
                        // node's forward applicability edges.
                        no_expand.insert(source_id.clone());
                    }
                }
                NodeKindId::SCENARIO => {
                    if required_ids
                        .insert(source_id.clone(), InclusionReason::ScenarioCoverage)
                        .is_none()
                    {
                        no_expand.insert(source_id.clone());
                    }
                }
                NodeKindId::OPERATION | NodeKindId::ENDPOINT => {
                    // The incoming operation is required context only when
                    // this node is the subject itself; inner details ride
                    // the effect edges below.
                    continue;
                }
                _ => {}
            }
        }

        // Declared and detected effects of applicable operations. Only
        // operations with at least one admitted effect edge are counted;
        // an operation the model declares without effects never inflates
        // the count, and the count is of distinct effect facts.
        if node.kind() == NodeKindId::OPERATION {
            if let Some(operation) =
                crate::effects::OperationId::from_semantic(node.id().semantic_id())
            {
                let edges = effects.operation_edges(&operation);
                if !edges.is_empty()
                    && !selection
                        .counted_operations
                        .contains(&operation.as_str().to_owned())
                {
                    selection
                        .counted_operations
                        .push(operation.as_str().to_owned());
                    for edge in edges {
                        if required_ids.len() as u64 >= limits.max_facts {
                            selection.complete = false;
                            selection.gaps.push(FactGap::ClosureBounded);
                            break;
                        }
                        required_ids
                            .entry(effect_fact_id(&operation, edge))
                            .or_insert(InclusionReason::OperationEffect);
                    }
                }
            }
        }
    }

    // Materialize the required ledger rows.
    for (id, reason) in &required_ids {
        let Some(node) = graph.resolve(id) else {
            // Synthesized effect-edge facts: one token row per id.
            let tokens = estimate::tokens(id);
            selection.required.push(LedgerFact {
                id: id.clone(),
                class: FactClass::RequiredSemantic,
                reason: Some(*reason),
                module: None,
                tokens,
            });
            continue;
        };
        let module = node.module().map(str::to_owned);
        let Some(definition) = context.definitions.get(node.id().semantic_id()) else {
            continue;
        };
        let content = definition_content(node, definition, module.as_deref());
        selection.required.push(LedgerFact {
            id: id.clone(),
            class: FactClass::RequiredSemantic,
            reason: Some(*reason),
            module,
            tokens: estimate::tokens(&content),
        });
    }

    // Supporting facts O: the ranked closure of direct dependency edge
    // witnesses and module cards of the closure modules (narrative
    // context that helps but is not contractually required).
    for node in root_nodes {
        for edge in graph.direct_dependencies(node.id(), &context.filter) {
            let target = edge.key().to();
            let target_id = target.as_str().to_owned();
            if required_ids.contains_key(&target_id) {
                continue;
            }
            let Some(target_node) = graph.node(target) else {
                continue;
            };
            if target_node.kind() == NodeKindId::REQUIREMENT {
                continue;
            }
            supporting_ids.insert(target_id);
        }
    }
    for id in &supporting_ids {
        if (selection.required.len() + selection.supporting.len()) as u64 >= limits.max_facts {
            selection.complete = false;
            selection.gaps.push(FactGap::ClosureBounded);
            break;
        }
        let Some(node) = graph.resolve(id) else {
            continue;
        };
        let module = node.module().map(str::to_owned);
        let tokens = match context.definitions.get(node.id().semantic_id()) {
            Some(definition) => {
                estimate::tokens(&definition_content(node, definition, module.as_deref()))
            }
            None => estimate::tokens(id),
        };
        selection.supporting.push(LedgerFact {
            id: id.clone(),
            class: FactClass::SupportingSemantic,
            reason: None,
            module,
            tokens,
        });
    }

    // The closed confidence gaps. The unrepresentable-error gap applies
    // only when the bound error registry actually carries a binding for
    // one of the selected operations: those contracts are applicable
    // required context this fact grammar cannot express, so the required
    // selection is explicitly incomplete — never a silently reduced F
    // (research: missing applicable attachment facts make the
    // minimum-safe estimate unknown).
    let unrepresentable = selection.counted_operations.iter().any(|operation| {
        crate::error_contract::ErrorRegistry::embedded()
            .map(|registry| {
                registry
                    .bindings()
                    .iter()
                    .any(|binding| binding.operation().as_str() == operation)
            })
            .unwrap_or(false)
    });
    if unrepresentable {
        selection.gaps.push(FactGap::ErrorContractsUnrepresentable);
        selection.complete = false;
    }
    if effects.envelope_count() == 0 {
        selection.gaps.push(FactGap::DetectedEffectsAbsent);
    }
    selection.gaps.sort();
    selection.gaps.dedup();
    if selection.gaps.len() > MAX_GAPS {
        selection.gaps.truncate(MAX_GAPS);
    }
    selection.counted_operations.sort();
    selection
}

impl FactSelection {
    /// The empty selection: complete until a bound proves otherwise.
    pub(crate) fn new() -> Self {
        Self {
            required: Vec::new(),
            supporting: Vec::new(),
            counted_operations: Vec::new(),
            gaps: Vec::new(),
            complete: true,
        }
    }
}
