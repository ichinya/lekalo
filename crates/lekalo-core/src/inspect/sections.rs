//! The read-only single-symbol projection over the accepted typed
//! surfaces (issue #15).
//!
//! `project` consumes exactly the accepted #8 IR, #13 dependency graph,
//! and #14 effect graph — never source bytes, target files, generated
//! code, or manifests — and assembles every mandatory section in wire
//! order. The resolver index is built once from the canonical
//! definitions; policies, scenarios, and bindings answer from the
//! canonical IR walk; relations answer from the graph's precomputed
//! adjacency; effect edges answer from the #14 index. Provider data is
//! absent in v1, so ownership and trace are explicitly `unsupported`,
//! and the richer #23 scenario detail is recorded as unavailable in
//! completeness — visibly degraded, never silently missing.

use crate::effects::{EffectKind, OperationId, ResourceId, ResourceKind, SubjectSelector};
use crate::graph::EdgeFilter;
use crate::ir::{CompiledProject, Definition, DefinitionKind, EntityDef, Field, TypeRef};

use super::limits::InspectLimits;
use super::result::{
    BindingItem, Bounds, Completeness, ContractBody, ContractSection, EffectCounts, EffectItem,
    EnumValueProjection, FieldProjection, InspectResult, InvariantItem, Item, ItemsSection,
    PolicyItem, PortabilitySection, ProjectHeader, RelationItem, ScenarioItem, SectionState,
    SelectorProjection, SourceRef, SymbolCard, TypeRefProjection, UnsupportedSection,
};
use super::selector::Selection;

/// Build the complete inspect result for one resolved selection.
///
/// The whole mandatory skeleton is always produced; only the canonical
/// payload bound (checked by the caller) can fail the invocation.
pub(crate) fn project(
    compilation: &crate::ir::Compilation,
    graph: &crate::graph::DependencyGraph,
    effects: &crate::effects::EffectGraph,
    selection: &Selection,
    include_bindings: bool,
    include_scenarios: bool,
) -> InspectResult {
    let limits = InspectLimits::v1();
    let project_ir = &compilation.project;
    let definition = &project_ir.definitions[selection.definition];
    let node = graph
        .resolve(definition.id().as_str())
        .expect("the dependency graph mirrors the compiled definitions");

    let symbol = SymbolCard {
        id: definition.id().as_str().to_owned(),
        kind: definition.kind().as_str(),
        module_id: node.module().unwrap_or_default().to_owned(),
        version: definition.common().version,
        description: definition
            .common()
            .description
            .as_ref()
            .map(|text| text.as_str().to_owned()),
        visibility: definition
            .common()
            .visibility
            .map(|visibility| visibility.as_str()),
        portability: definition
            .common()
            .portability
            .map(|portability| portability.as_str()),
        source: source_of(compilation, definition.id().as_str()),
    };

    let contract = contract_section(definition, &limits);
    let invariants = invariants_section(definition);
    let policies = policies_section(project_ir, definition.id().as_str());
    let effects_section = effects_section(effects, definition, &limits);
    let dependencies = relations_section(graph, node.id(), &limits, true);
    let dependents = relations_section(graph, node.id(), &limits, false);
    let scenarios = scenarios_section(project_ir, definition.id().as_str(), include_scenarios);
    let bindings = bindings_section(project_ir, graph, &symbol.module_id, include_bindings);

    let mut reasons: Vec<&'static str> = vec![
        "ownership-unavailable",
        "trace-unavailable",
        "scenario-details-unavailable",
    ];
    let mut omitted_items = 0usize;
    for omitted in [
        ("bindings-not-requested", &bindings),
        ("scenarios-not-requested", &scenarios),
    ] {
        if omitted
            .1
            .bounds
            .as_ref()
            .is_some_and(|bounds| bounds.reason == Some("not-requested"))
        {
            reasons.push(omitted.0);
        }
    }
    let mut completeness = Completeness {
        state: "partial",
        reasons,
        omitted_items: 0,
    };
    for bounds in [
        &contract.bounds,
        &invariants.bounds,
        &policies.bounds,
        &effects_section.bounds,
        &dependencies.bounds,
        &dependents.bounds,
        &scenarios.bounds,
        &bindings.bounds,
    ]
    .into_iter()
    .flatten()
    {
        omitted_items += bounds.omitted;
    }
    if omitted_items > 0 {
        completeness.reasons.push("items-truncated");
    }
    completeness.omitted_items = omitted_items;
    if completeness.reasons.is_empty() {
        completeness.state = "complete";
    }

    InspectResult {
        model_version: project_ir.model_version.as_str().to_owned(),
        project: project_header(project_ir),
        selector: SelectorProjection {
            mode: selection.mode.as_str(),
            input: selection.input.clone(),
            resolved_id: definition.id().as_str().to_owned(),
        },
        symbol,
        contract,
        invariants,
        policies,
        effects: effects_section,
        dependencies,
        dependents,
        scenarios,
        bindings,
        ownership: UnsupportedSection {
            state: SectionState::Unsupported,
            reason: "owner-not-accepted",
        },
        portability: PortabilitySection {
            state: match definition.common().portability {
                Some(_) => SectionState::Available,
                None => SectionState::Empty,
            },
            mode: definition.common().portability.map(|p| p.as_str()),
        },
        trace: UnsupportedSection {
            state: SectionState::Unsupported,
            reason: "owner-not-accepted",
        },
        completeness,
    }
}

/// The project header, when the source declared a project identity.
fn project_header(project: &CompiledProject) -> Option<ProjectHeader> {
    let id = project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned())?;
    Some(ProjectHeader {
        id,
        model_ref: format!("dev.lekalo.model@{}", project.model_version.as_str()),
        ir_identity: crate::ir::IDENTITY.to_owned(),
        ir_digest: super::sections::ir_digest(project),
    })
}

/// The `sha256` digest of the canonical IR bytes.
pub(crate) fn ir_digest(project: &CompiledProject) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(project.to_canonical_json().as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// The logical source location of one definition: the first source-map
fn source_of(compilation: &crate::ir::Compilation, id: &str) -> Option<SourceRef> {
    compilation
        .source_map
        .entries()
        .iter()
        .find(|entry| entry.semantic_id.as_deref() == Some(id))
        .map(|entry| SourceRef {
            path: entry.path.clone(),
            pointer: entry.pointer.clone(),
            start_byte: entry.start.byte,
            end_byte: entry.end.byte,
        })
}

/// The per-kind contract section; the only truncatable typed content is
/// the field/value/input arrays.
fn contract_section(definition: &Definition, limits: &InspectLimits) -> ContractSection {
    // The item total and frontier come from the SOURCE declaration,
    // never from the already-truncated projection.
    let (source_total, source_frontier) = match definition {
        Definition::Enum(enm) => (
            enm.values.len(),
            enm.values
                .get(limits.section_items)
                .map(|v| v.value.as_str().to_owned()),
        ),
        Definition::ValueObject(value_object) => (
            value_object.fields.len(),
            value_object
                .fields
                .get(limits.section_items)
                .map(|field| field.name.as_str().to_owned()),
        ),
        Definition::Entity(entity) => (
            entity.fields.len(),
            entity
                .fields
                .get(limits.section_items)
                .map(|field| field.name.as_str().to_owned()),
        ),
        Definition::Command(command) => (
            command.input.len(),
            command
                .input
                .get(limits.section_items)
                .map(|field| field.name.as_str().to_owned()),
        ),
        Definition::Event(event) => (
            event.payload.len(),
            event
                .payload
                .get(limits.section_items)
                .map(|field| field.name.as_str().to_owned()),
        ),
        _ => (0, None),
    };
    let body = match definition {
        Definition::Scalar(scalar) => ContractBody::Scalar {
            base: scalar.base.as_str(),
        },
        Definition::Enum(enm) => {
            let values: Vec<EnumValueProjection> = enm
                .values
                .iter()
                .take(limits.section_items)
                .map(|value| EnumValueProjection {
                    value: value.value.as_str().to_owned(),
                    description: value
                        .description
                        .as_ref()
                        .map(|text| text.as_str().to_owned()),
                })
                .collect();
            ContractBody::Enum { values }
        }
        Definition::ValueObject(value_object) => ContractBody::Fields {
            fields: fields_projection(&value_object.fields, limits),
        },
        Definition::Entity(entity) => ContractBody::Fields {
            fields: fields_projection(&entity.fields, limits),
        },
        Definition::Command(command) => ContractBody::Command {
            input: fields_projection(&command.input, limits),
        },
        Definition::Query(query) => ContractBody::Query {
            reads: query
                .reads
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
            output: query.returns.as_ref().map(type_ref_projection),
        },
        Definition::Policy(policy) => ContractBody::Policy {
            decision: policy.decision.as_str(),
        },
        Definition::Event(event) => ContractBody::Event {
            payload: fields_projection(&event.payload, limits),
        },
        Definition::Effect(effect) => ContractBody::Effect {
            operation: effect.operation.as_str(),
            target: effect.entity.as_str().to_owned(),
            emits: effect
                .emits
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
        },
        Definition::Endpoint(endpoint) => ContractBody::Endpoint {
            invokes: endpoint.invokes.as_str().to_owned(),
            method: endpoint.method.as_str(),
            path: endpoint.path.as_str().to_owned(),
        },
        Definition::Scenario(scenario) => ContractBody::Scenario {
            summary: scenario.summary.as_str().to_owned(),
            covers: scenario
                .covers
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
        },
        Definition::TargetBinding(binding) => ContractBody::TargetBinding {
            target_name: binding.target.as_str().to_owned(),
        },
    };
    let bounds = array_bounds(source_total, limits, source_frontier);
    let state = if bounds.is_some() {
        SectionState::Truncated
    } else {
        SectionState::Available
    };
    ContractSection {
        state,
        complete: bounds.is_none(),
        bounds,
        body,
    }
}

/// The bounded field projections in declaration order.
fn fields_projection(fields: &[Field], limits: &InspectLimits) -> Vec<FieldProjection> {
    fields
        .iter()
        .take(limits.section_items)
        .map(|field| FieldProjection {
            name: field.name.as_str().to_owned(),
            r#type: type_ref_projection(&field.r#type),
            required: field.required,
            description: field
                .description
                .as_ref()
                .map(|text| text.as_str().to_owned()),
        })
        .collect()
}

/// The typed TypeRef projection.
fn type_ref_projection(reference: &TypeRef) -> TypeRefProjection {
    match reference {
        TypeRef::Ref(id) => TypeRefProjection::Ref(id.as_str().to_owned()),
        TypeRef::List(inner) => TypeRefProjection::List(Box::new(type_ref_projection(inner))),
        TypeRef::Optional(inner) => {
            TypeRefProjection::Optional(Box::new(type_ref_projection(inner)))
        }
    }
}
/// Assemble bound bookkeeping for a truncated typed array.
fn array_bounds(total: usize, limits: &InspectLimits, frontier: Option<String>) -> Option<Bounds> {
    if total <= limits.section_items {
        return None;
    }
    Some(Bounds {
        limit: limits.section_items,
        returned: limits.section_items,
        omitted: total - limits.section_items,
        reason: Some("section-item-limit"),
        frontier: Some(frontier.expect("a truncated declaration has a frontier")),
    })
}

/// The invariants section: the entity identity membership is the one
/// invariant the accepted Model can declare; everything else is empty.
fn invariants_section(definition: &Definition) -> ItemsSection {
    let identity: Option<&Vec<crate::ir::FieldName>> = match definition {
        Definition::Entity(EntityDef { identity, .. }) => Some(identity),
        _ => None,
    };
    match identity {
        Some(fields) if !fields.is_empty() => ItemsSection {
            state: SectionState::Available,
            complete: true,
            bounds: None,
            summary: None,
            items: vec![Item::Invariant(InvariantItem {
                fields: fields.iter().map(|name| name.as_str().to_owned()).collect(),
            })],
        },
        _ => ItemsSection {
            state: SectionState::Empty,
            complete: true,
            bounds: None,
            summary: None,
            items: Vec::new(),
        },
    }
}

/// The policies section: every policy whose `applies_to` names the
/// symbol, in canonical definition order.
fn policies_section(project: &CompiledProject, id: &str) -> ItemsSection {
    let items: Vec<Item> = project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Policy(policy)
                if policy.applies_to.iter().any(|target| target.as_str() == id) =>
            {
                Some(Item::Policy(PolicyItem {
                    id: policy.id.as_str().to_owned(),
                    decision: policy.decision.as_str(),
                }))
            }
            _ => None,
        })
        .collect();
    finish_items(items)
}

/// The effects section.
///
/// Operations (commands and queries) project their direct declared and
/// detected edges; entities and events project the exact reverse
/// counts from the #14 index; every other kind is empty.
fn effects_section(
    effects: &crate::effects::EffectGraph,
    definition: &Definition,
    limits: &InspectLimits,
) -> ItemsSection {
    match definition.kind() {
        DefinitionKind::Command | DefinitionKind::Query => {
            let Some(operation) = OperationId::from_semantic(definition.id().as_str()) else {
                return ItemsSection {
                    state: SectionState::Unknown,
                    complete: false,
                    bounds: None,
                    summary: None,
                    items: Vec::new(),
                };
            };
            let edges = effects.operation_edges(&operation);
            let total = edges.len();
            let items: Vec<Item> = edges
                .iter()
                .take(limits.section_items)
                .map(|edge| {
                    let declared = matches!(
                        edge.provenance(),
                        crate::effects::EffectProvenance::CanonicalIr { .. }
                    );
                    Item::Effect(EffectItem {
                        json: crate::effects::canonical::edge_bytes(edge),
                        human: effect_human(edge),
                        declared,
                    })
                })
                .collect();
            let mut section = finish_items(items);
            section.complete = total <= limits.section_items;
            if total > limits.section_items {
                section.state = SectionState::Truncated;
                section.bounds = Some(Bounds {
                    limit: limits.section_items,
                    returned: limits.section_items,
                    omitted: total - limits.section_items,
                    reason: Some("section-item-limit"),
                    frontier: None,
                });
            }
            section
        }
        DefinitionKind::Entity | DefinitionKind::Event => {
            let kind = if definition.kind() == DefinitionKind::Event {
                ResourceKind::Event
            } else {
                ResourceKind::Canonical
            };
            let Some(resource) = ResourceId::new(kind, definition.id().as_str()) else {
                return ItemsSection {
                    state: SectionState::Unknown,
                    complete: false,
                    bounds: None,
                    summary: None,
                    items: Vec::new(),
                };
            };
            let selector = SubjectSelector::entity(resource.clone());
            let readers = match effects.readers(&selector) {
                Ok(readers) => readers.len(),
                Err(_) => return unknown_effects_section(),
            };
            let writers = match effects.writers(&selector) {
                Ok(writers) => writers.len(),
                Err(_) => return unknown_effects_section(),
            };
            let emitters = effects
                .declared()
                .iter()
                .chain(effects.detected().iter())
                .filter(|edge| {
                    edge.key().kind() == EffectKind::EmitEvent
                        && edge.key().subject().resource().as_str() == definition.id().as_str()
                })
                .count();
            let counts = EffectCounts {
                reader_count: readers,
                writer_count: writers,
                emitter_count: emitters,
            };
            let any = readers + writers + emitters > 0;
            ItemsSection {
                state: if any {
                    SectionState::Available
                } else {
                    SectionState::Empty
                },
                complete: true,
                bounds: None,
                summary: Some(counts),
                items: Vec::new(),
            }
        }
        _ => ItemsSection {
            state: SectionState::Empty,
            complete: true,
            bounds: None,
            summary: None,
            items: Vec::new(),
        },
    }
}

/// The visibly degraded section when a reverse query crossed its own
/// recorded bound.
fn unknown_effects_section() -> ItemsSection {
    ItemsSection {
        state: SectionState::Unknown,
        complete: false,
        bounds: None,
        summary: None,
        items: Vec::new(),
    }
}

/// One stable human effect line mirroring the effects CLI shape.
fn effect_human(edge: &crate::effects::EffectEdge) -> String {
    let key = edge.key();
    let subject = match key.subject().field() {
        Some(field) => format!("{}#{}", key.subject().resource().as_str(), field.as_str()),
        None => key.subject().resource().as_str().to_owned(),
    };
    format!(
        "{} {} -> {} ({})",
        key.kind().key(),
        key.operation().as_str(),
        subject,
        edge.confidence().as_str()
    )
}

/// The dependencies or dependents section: direct graph relations from
/// the precomputed adjacency, in canonical edge order.
fn relations_section(
    graph: &crate::graph::DependencyGraph,
    node: &crate::graph::NodeId,
    limits: &InspectLimits,
    forward: bool,
) -> ItemsSection {
    let filter = EdgeFilter::new();
    let edges = if forward {
        graph.direct_dependencies(node, &filter)
    } else {
        graph.reverse_dependencies(node, &filter)
    };
    let total = edges.len();
    let items: Vec<Item> = edges
        .iter()
        .take(limits.section_items)
        .map(|edge| {
            let key = edge.key();
            let endpoint = if forward {
                key.to().semantic_id()
            } else {
                key.from().semantic_id()
            };
            Item::Relation(RelationItem {
                relation: key.relation().key(),
                endpoint: endpoint.to_owned(),
                occurrence: key.occurrence().get(),
                provenance_json: crate::graph::canonical::provenance_bytes(edge.provenance()),
                confidence: edge.confidence().as_str(),
            })
        })
        .collect();
    let mut section = finish_items(items);
    if total > limits.section_items {
        section.state = SectionState::Truncated;
        section.complete = false;
        section.bounds = Some(Bounds {
            limit: limits.section_items,
            returned: limits.section_items,
            omitted: total - limits.section_items,
            reason: Some("section-item-limit"),
            frontier: edges.get(limits.section_items).map(|edge| {
                let key = edge.key();
                if forward {
                    key.to().semantic_id().to_owned()
                } else {
                    key.from().semantic_id().to_owned()
                }
            }),
        });
    }
    section
}

/// The scenarios section: every scenario whose `covers` names the
/// symbol, in canonical definition order.
fn scenarios_section(project: &CompiledProject, id: &str, include: bool) -> ItemsSection {
    if !include {
        return not_requested_section();
    }
    let items: Vec<Item> = project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Scenario(scenario)
                if scenario.covers.iter().any(|covered| covered.as_str() == id) =>
            {
                Some(Item::Scenario(ScenarioItem {
                    id: scenario.id.as_str().to_owned(),
                    summary: Some(scenario.summary.as_str().to_owned()),
                }))
            }
            _ => None,
        })
        .collect();
    finish_items(items)
}

/// The bindings section: every declared target binding of the symbol's
/// module. Binding target details stay namespaced and absent in v1: no
/// #29 profile records exist to project.
fn bindings_section(
    project: &CompiledProject,
    graph: &crate::graph::DependencyGraph,
    module_id: &str,
    include: bool,
) -> ItemsSection {
    if !include {
        return not_requested_section();
    }
    if module_id.is_empty() {
        return ItemsSection {
            state: SectionState::Empty,
            complete: true,
            bounds: None,
            summary: None,
            items: Vec::new(),
        };
    }
    let items: Vec<Item> = project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::TargetBinding(binding) => {
                let binding_module = graph
                    .resolve(binding.id.as_str())
                    .and_then(|node| node.module())?;
                if binding_module != module_id {
                    return None;
                }
                Some(Item::Binding(BindingItem {
                    binding_id: binding.id.as_str().to_owned(),
                    module_id: binding_module.to_owned(),
                    target_id: binding.target.as_str().to_owned(),
                }))
            }
            _ => None,
        })
        .collect();
    finish_items(items)
}

/// Close one items section: available with items, or explicitly empty.
fn finish_items(items: Vec<Item>) -> ItemsSection {
    ItemsSection {
        state: if items.is_empty() {
            SectionState::Empty
        } else {
            SectionState::Available
        },
        complete: true,
        bounds: None,
        summary: None,
        items,
    }
}

/// The explicitly not-requested section: present, marked, and counted.
fn not_requested_section() -> ItemsSection {
    ItemsSection {
        state: SectionState::Unsupported,
        complete: false,
        bounds: Some(Bounds {
            limit: 0,
            returned: 0,
            omitted: 0,
            reason: Some("not-requested"),
            frontier: None,
        }),
        summary: None,
        items: Vec::new(),
    }
}
