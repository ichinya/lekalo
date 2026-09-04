//! The semantic validation rules (issue #12).
//!
//! Every rule is a pure function over the compiled project and the #8
//! source map. Execution is phase-aware — resolution, then semantic, then
//! portability — and inside a phase follows the registry's sorted rule-id
//! order; definitions are visited in canonical (semantic-ID byte) order and
//! reference occurrences in the fixed site order below, so one input always
//! yields the same diagnostic multiset. The #11 normalization then fixes
//! the total wire order.
//!
//! Reference-kind matrix: the kind expectations below mirror the published
//! model contract exactly (`model.ref-kind-mismatch`), so every model-valid
//! project validates and no model-invalid project passes. Event references
//! have no site beyond `effect.emits`, so no distinct event rules exist;
//! target bindings name target configs with no in-Model registry, and the
//! model layer already owns `model.target-unresolved`, so target resolution
//! stays deferred to the adapters issue (#27).

use std::collections::{BTreeMap, BTreeSet};

use super::context::{expected_kinds, Context};
use super::profile::ValidationProfile;
use crate::diagnostics::types::{DataObject, DataValue};
use crate::ir::{Definition, DefinitionKind, Portability, SymbolId, TypeRef, Visibility};

/// The kinds a type reference may point at.
const NAMED_TYPES: &[&str] = &["enum", "entity", "scalar", "value-object"];
/// The kinds a policy may apply to.
const POLICY_OPERATIONS: &[&str] = &["command"];
/// The kinds an endpoint may invoke.
const OPERATIONS: &[&str] = &["command", "query"];
/// The single kind a command effect may reference.
const EFFECTS: &[&str] = &["effect"];
/// The single kind a query may read and an effect may target.
const ENTITIES: &[&str] = &["entity"];
/// The single kind an effect may emit.
const EVENTS: &[&str] = &["event"];

pub(super) fn resolution_phase(context: &mut Context, profile: &ValidationProfile) {
    if profile.is_enabled("semantic.type-ref-unresolved") {
        unresolved_type_refs(context, profile);
    }
    if profile.is_enabled("semantic.command-effect-unresolved") {
        unresolved_refs(
            context,
            profile,
            "semantic.command-effect-unresolved",
            "effects",
        );
    }
    if profile.is_enabled("semantic.query-read-unresolved") {
        unresolved_refs(context, profile, "semantic.query-read-unresolved", "reads");
    }
    if profile.is_enabled("semantic.policy-operation-unresolved") {
        unresolved_refs(
            context,
            profile,
            "semantic.policy-operation-unresolved",
            "applies_to",
        );
    }
    if profile.is_enabled("semantic.effect-resource-unresolved") {
        unresolved_refs(
            context,
            profile,
            "semantic.effect-resource-unresolved",
            "entity",
        );
    }
    if profile.is_enabled("semantic.effect-emits-unresolved") {
        unresolved_refs(
            context,
            profile,
            "semantic.effect-emits-unresolved",
            "emits",
        );
    }
    if profile.is_enabled("semantic.endpoint-operation-unresolved") {
        unresolved_refs(
            context,
            profile,
            "semantic.endpoint-operation-unresolved",
            "invokes",
        );
    }
    if profile.is_enabled("semantic.scenario-operation-unresolved") {
        unresolved_refs(
            context,
            profile,
            "semantic.scenario-operation-unresolved",
            "covers",
        );
    }
}

pub(super) fn semantic_phase(context: &mut Context, profile: &ValidationProfile) {
    if profile.is_enabled("semantic.type-ref-kind-mismatch") {
        type_ref_kinds(context, profile);
    }
    if profile.is_enabled("semantic.command-effect-kind-mismatch") {
        ref_kinds(
            context,
            profile,
            "semantic.command-effect-kind-mismatch",
            "effects",
            EFFECTS,
        );
    }
    if profile.is_enabled("semantic.query-read-kind-mismatch") {
        ref_kinds(
            context,
            profile,
            "semantic.query-read-kind-mismatch",
            "reads",
            ENTITIES,
        );
    }
    if profile.is_enabled("semantic.policy-operation-kind-mismatch") {
        ref_kinds(
            context,
            profile,
            "semantic.policy-operation-kind-mismatch",
            "applies_to",
            POLICY_OPERATIONS,
        );
    }
    if profile.is_enabled("semantic.effect-resource-kind-mismatch") {
        ref_kinds(
            context,
            profile,
            "semantic.effect-resource-kind-mismatch",
            "entity",
            ENTITIES,
        );
    }
    if profile.is_enabled("semantic.effect-emits-kind-mismatch") {
        ref_kinds(
            context,
            profile,
            "semantic.effect-emits-kind-mismatch",
            "emits",
            EVENTS,
        );
    }
    if profile.is_enabled("semantic.endpoint-operation-kind-mismatch") {
        ref_kinds(
            context,
            profile,
            "semantic.endpoint-operation-kind-mismatch",
            "invokes",
            OPERATIONS,
        );
    }
    if profile.is_enabled("semantic.entity-identity-field-missing") {
        entity_identity(context, profile);
    }
    if profile.is_enabled("semantic.type-recursion") {
        type_recursion(context, profile);
    }
    if profile.is_enabled("semantic.visibility-boundary-violation") {
        cross_module_visibility(context, profile);
    }
    if profile.is_enabled("semantic.public-output-private-type") {
        public_output(context, profile);
    }
}

pub(super) fn portability_phase(context: &mut Context, profile: &ValidationProfile) {
    if profile.is_enabled("semantic.portable-target-reference") {
        portability(context, profile);
    }
}

/// One reference occurrence: the exact source-map pointer suffix below the
/// subject definition and the fully qualified target the loader expanded.
struct Site<'def> {
    suffix: String,
    target: &'def SymbolId,
}

/// Whether one site suffix belongs to the given reference field.
fn is_field(suffix: &str, field: &str) -> bool {
    suffix == format!("/{field}") || suffix.starts_with(&format!("/{field}/"))
}

/// Whether one site suffix is a type-leaf site (`fields`, `input`,
/// `payload`, or `returns`).
fn is_type_site(suffix: &str) -> bool {
    suffix.starts_with("/fields/")
        || suffix.starts_with("/input/")
        || suffix.starts_with("/payload/")
        || suffix == "/returns"
        || suffix.starts_with("/returns/")
}

/// Every reference site of one definition in the fixed canonical order:
/// the symbol-reference fields first (`effects`, `reads`, `applies_to`,
/// `entity`, `emits`, `invokes`, `covers`, each in array order), then the
/// type leaves (`fields`, `input`, `payload`, `returns`, each in array
/// order with wrapper segments appended exactly as the decoder recorded).
fn all_sites(definition: &Definition) -> Vec<Site<'_>> {
    let mut sites = Vec::new();
    fn push_array<'def>(
        field: &'static str,
        items: &[&'def SymbolId],
        sites: &mut Vec<Site<'def>>,
    ) {
        for (ordinal, target) in items.iter().enumerate() {
            sites.push(Site {
                suffix: format!("/{field}/{ordinal}"),
                target,
            });
        }
    }
    match definition {
        Definition::Command(command) => {
            push_array(
                "effects",
                &command.effects.iter().collect::<Vec<_>>(),
                &mut sites,
            );
        }
        Definition::Query(query) => {
            push_array("reads", &query.reads.iter().collect::<Vec<_>>(), &mut sites);
        }
        Definition::Policy(policy) => {
            push_array(
                "applies_to",
                &policy.applies_to.iter().collect::<Vec<_>>(),
                &mut sites,
            );
        }
        Definition::Effect(effect) => {
            sites.push(Site {
                suffix: "/entity".to_owned(),
                target: &effect.entity,
            });
            push_array(
                "emits",
                &effect.emits.iter().collect::<Vec<_>>(),
                &mut sites,
            );
        }
        Definition::Endpoint(endpoint) => {
            sites.push(Site {
                suffix: "/invokes".to_owned(),
                target: &endpoint.invokes,
            });
        }
        Definition::Scenario(scenario) => {
            push_array(
                "covers",
                &scenario.covers.iter().collect::<Vec<_>>(),
                &mut sites,
            );
        }
        _ => {}
    }
    fn walk<'def>(type_ref: &'def TypeRef, suffix: String, sites: &mut Vec<Site<'def>>) {
        match type_ref {
            TypeRef::Ref(target) => sites.push(Site { suffix, target }),
            TypeRef::List(inner) => walk(inner, format!("{suffix}/list"), sites),
            TypeRef::Optional(inner) => walk(inner, format!("{suffix}/optional"), sites),
        }
    }
    fn fields<'def>(prefix: &str, items: &'def [crate::ir::Field], sites: &mut Vec<Site<'def>>) {
        for (index, field) in items.iter().enumerate() {
            walk(&field.r#type, format!("{prefix}/{index}/type"), sites);
        }
    }
    match definition {
        Definition::ValueObject(value_object) => {
            fields("/fields", &value_object.fields, &mut sites)
        }
        Definition::Entity(entity) => fields("/fields", &entity.fields, &mut sites),
        Definition::Command(command) => fields("/input", &command.input, &mut sites),
        Definition::Event(event) => fields("/payload", &event.payload, &mut sites),
        Definition::Query(query) => {
            if let Some(returns) = &query.returns {
                walk(returns, "/returns".to_owned(), &mut sites);
            }
        }
        _ => {}
    }
    sites
}

/// Resolve one definition by semantic id via the canonical byte order.
fn resolve<'def>(context: &Context<'def>, target: &SymbolId) -> Option<&'def Definition> {
    let definitions = context.project().definitions.as_slice();
    let index = definitions
        .binary_search_by(|definition| {
            definition
                .id()
                .as_str()
                .as_bytes()
                .cmp(target.as_str().as_bytes())
        })
        .ok()?;
    definitions.get(index)
}
/// Emit one located diagnostic with the closed reference data.
fn emit_reference(
    context: &mut Context,
    profile: &ValidationProfile,
    rule_id: &str,
    subject: &str,
    site: &Site,
    expected: Option<&[&str]>,
) {
    let span = context.span_of(subject, &site.suffix);
    let mut data = DataObject::new();
    data.insert(
        "definition".to_owned(),
        DataValue::Token(subject.to_owned()),
    );
    data.insert(
        "reference".to_owned(),
        DataValue::Token(site.target.as_str().to_owned()),
    );
    if let Some(expected) = expected {
        data.insert("expected".to_owned(), expected_kinds(expected));
    }
    context.emit(profile, rule_id, Some(subject), span, data);
}

/// Emit one kind-mismatch diagnostic with the sorted expected-kind list.
fn emit_kind(
    context: &mut Context,
    profile: &ValidationProfile,
    rule_id: &str,
    subject: &str,
    site: &Site,
    expected: &[&str],
    found: DefinitionKind,
) {
    let target = site.target;
    let suffix = site.suffix.as_str();
    let span = context.span_of(subject, suffix);
    let mut data = DataObject::new();
    data.insert(
        "definition".to_owned(),
        DataValue::Token(subject.to_owned()),
    );
    data.insert(
        "reference".to_owned(),
        DataValue::Token(target.as_str().to_owned()),
    );
    data.insert("expected".to_owned(), expected_kinds(expected));
    data.insert(
        "found".to_owned(),
        DataValue::Token(found.as_str().to_owned()),
    );
    context.emit(profile, rule_id, Some(subject), span, data);
}

/// `semantic.type-ref-unresolved`: a type leaf names an unknown symbol.
fn unresolved_type_refs(context: &mut Context, profile: &ValidationProfile) {
    let project = context.project();
    for definition in &project.definitions {
        let subject = definition.id().as_str().to_owned();
        for site in all_sites(definition) {
            if is_type_site(&site.suffix) && resolve(context, site.target).is_none() {
                emit_reference(
                    context,
                    profile,
                    "semantic.type-ref-unresolved",
                    &subject,
                    &site,
                    None,
                );
            }
        }
    }
}

/// One `*-unresolved` rule over one reference field.
fn unresolved_refs(
    context: &mut Context,
    profile: &ValidationProfile,
    rule_id: &str,
    field: &'static str,
) {
    let project = context.project();
    for definition in &project.definitions {
        let subject = definition.id().as_str().to_owned();
        for site in all_sites(definition) {
            if is_field(&site.suffix, field) && resolve(context, site.target).is_none() {
                emit_reference(context, profile, rule_id, &subject, &site, None);
            }
        }
    }
}

/// `semantic.type-ref-kind-mismatch`: a resolved type leaf names a
/// definition that is not a named type.
fn type_ref_kinds(context: &mut Context, profile: &ValidationProfile) {
    let project = context.project();
    for definition in &project.definitions {
        let subject = definition.id().as_str().to_owned();
        for site in all_sites(definition) {
            if !is_type_site(&site.suffix) {
                continue;
            }
            let Some(found) = resolve(context, site.target) else {
                continue;
            };
            if !NAMED_TYPES.contains(&found.kind().as_str()) {
                emit_kind(
                    context,
                    profile,
                    "semantic.type-ref-kind-mismatch",
                    &subject,
                    &site,
                    NAMED_TYPES,
                    found.kind(),
                );
            }
        }
    }
}

/// One `*-kind-mismatch` rule over one reference field.
fn ref_kinds(
    context: &mut Context,
    profile: &ValidationProfile,
    rule_id: &str,
    field: &'static str,
    expected: &[&str],
) {
    let project = context.project();
    for definition in &project.definitions {
        let subject = definition.id().as_str().to_owned();
        for site in all_sites(definition) {
            if !is_field(&site.suffix, field) {
                continue;
            }
            let Some(found) = resolve(context, site.target) else {
                continue;
            };
            if !expected.contains(&found.kind().as_str()) {
                emit_kind(
                    context,
                    profile,
                    rule_id,
                    &subject,
                    &site,
                    expected,
                    found.kind(),
                );
            }
        }
    }
}

/// `semantic.entity-identity-field-missing`: an identity names a field the
/// entity does not declare.
fn entity_identity(context: &mut Context, profile: &ValidationProfile) {
    let project = context.project();
    for definition in &project.definitions {
        let Definition::Entity(entity) = definition else {
            continue;
        };
        let subject = entity.id.as_str().to_owned();
        let names: Vec<&str> = entity
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        for (ordinal, identity) in entity.identity.iter().enumerate() {
            if names.contains(&identity.as_str()) {
                continue;
            }
            let suffix = format!("/identity/{ordinal}");
            let span = context.span_of(&subject, &suffix);
            let mut data = DataObject::new();
            data.insert("definition".to_owned(), DataValue::Token(subject.clone()));
            data.insert(
                "field".to_owned(),
                DataValue::Token(identity.as_str().to_owned()),
            );
            context.emit(
                profile,
                "semantic.entity-identity-field-missing",
                Some(&subject),
                span,
                data,
            );
        }
    }
}

/// `semantic.type-recursion`: a cycle among entity/value-object types.
///
/// Exactly the published model-contract prohibition (`model.type-recursion`):
/// every type leaf of a record type is an edge to its target when that
/// target is again a record type, regardless of `list`/`optional` wrappers.
/// Each cycle is reported once, at its lexicographically smallest member,
/// naming the smallest in-cycle successor; the span is that member's first
/// edge into the cycle.
fn type_recursion(context: &mut Context, profile: &ValidationProfile) {
    let project = context.project();
    let mut edges: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut edge_suffixes: BTreeMap<(&str, &str), String> = BTreeMap::new();
    for definition in &project.definitions {
        match definition {
            Definition::Entity(_) | Definition::ValueObject(_) => {}
            _ => continue,
        }
        let id = definition.id().as_str();
        let entry = edges.entry(id).or_default();
        for site in all_sites(definition) {
            if !site.suffix.starts_with("/fields/") {
                continue;
            }
            let Some(found) = resolve(context, site.target) else {
                continue;
            };
            match found.kind() {
                DefinitionKind::Entity | DefinitionKind::ValueObject => {}
                _ => continue,
            }
            entry.insert(site.target.as_str());
            edge_suffixes
                .entry((id, site.target.as_str()))
                .or_insert_with(|| site.suffix.clone());
        }
    }
    for cycle in cycles(&edges) {
        let representative = cycle[0];
        let reference = *edges
            .get(representative)
            .and_then(|successors| {
                successors
                    .iter()
                    .find(|successor| cycle.contains(successor))
            })
            .expect("a cycle member always has an in-cycle successor");
        let suffix = edge_suffixes
            .get(&(representative, reference))
            .cloned()
            .unwrap_or_default();
        let mut data = DataObject::new();
        data.insert(
            "definition".to_owned(),
            DataValue::Token((*representative).to_owned()),
        );
        data.insert(
            "reference".to_owned(),
            DataValue::Token(reference.to_owned()),
        );
        let span = context.span_of(representative, &suffix);
        context.emit(
            profile,
            "semantic.type-recursion",
            Some(representative),
            span,
            data,
        );
    }
}

/// The strongly connected components of the record-type edge graph with at
/// least one edge (Kosaraju, iterative; adjacency is sorted, so both the
/// component members and the reported representatives are deterministic).
///
/// A single node forms a cycle exactly when its required edge points at
/// itself.
fn cycles<'a>(edges: &'a BTreeMap<&'a str, BTreeSet<&'a str>>) -> Vec<Vec<&'a str>> {
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut finish: Vec<&str> = Vec::with_capacity(edges.len());
    for root in edges.keys().copied() {
        if visited.contains(root) {
            continue;
        }
        let mut stack: Vec<(&str, usize)> = vec![(root, 0)];
        while let Some((node, child)) = stack.pop() {
            let successors: Vec<&str> = edges.get(node).into_iter().flatten().copied().collect();
            if child == 0 {
                visited.insert(node);
            }
            let mut descended = false;
            for (position, successor) in successors.iter().enumerate().skip(child) {
                if !visited.contains(*successor) {
                    stack.push((node, position + 1));
                    stack.push((*successor, 0));
                    descended = true;
                    break;
                }
            }
            if !descended {
                finish.push(node);
            }
        }
    }
    let mut reverse: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (node, successors) in edges {
        for successor in successors {
            reverse.entry(*successor).or_default().push(*node);
        }
    }
    let mut assigned: BTreeSet<&str> = BTreeSet::new();
    let mut components = Vec::new();
    for node in finish.into_iter().rev() {
        if assigned.contains(node) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![node];
        assigned.insert(node);
        while let Some(member) = stack.pop() {
            component.push(member);
            for predecessor in reverse.get(member).into_iter().flatten() {
                if !assigned.contains(*predecessor) {
                    assigned.insert(predecessor);
                    stack.push(predecessor);
                }
            }
        }
        let self_loop = component
            .first()
            .and_then(|member| edges.get(*member))
            .is_some_and(|successors| successors.contains(&component[0]));
        if component.len() > 1 || self_loop {
            component.sort_unstable();
            components.push(component);
        }
    }
    components.sort_unstable();
    components
}

/// `semantic.visibility-boundary-violation`: a reference crosses a module
/// boundary into a module-private symbol.
fn cross_module_visibility(context: &mut Context, profile: &ValidationProfile) {
    let project = context.project();
    for definition in &project.definitions {
        let Some(owner) = Context::owner(definition.id().as_str()) else {
            continue;
        };
        let subject = definition.id().as_str().to_owned();
        for site in all_sites(definition) {
            let Some(found) = resolve(context, site.target) else {
                continue;
            };
            let Some(target_owner) = Context::owner(site.target.as_str()) else {
                continue;
            };
            if target_owner == owner {
                continue;
            }
            if found.common().visibility != Some(Visibility::Module) {
                continue;
            }
            emit_reference(
                context,
                profile,
                "semantic.visibility-boundary-violation",
                &subject,
                &site,
                None,
            );
        }
    }
}

/// `semantic.public-output-private-type`: a project-visible query returns a
/// module-private named type.
fn public_output(context: &mut Context, profile: &ValidationProfile) {
    let project = context.project();
    for definition in &project.definitions {
        let Definition::Query(query) = definition else {
            continue;
        };
        if query.common.visibility == Some(Visibility::Module) {
            continue;
        }
        let subject = query.id.as_str().to_owned();
        for site in all_sites(definition) {
            let is_output = site.suffix == "/returns" || site.suffix.starts_with("/returns/");
            if !is_output {
                continue;
            }
            let Some(found) = resolve(context, site.target) else {
                continue;
            };
            if found.common().visibility != Some(Visibility::Module) {
                continue;
            }
            emit_reference(
                context,
                profile,
                "semantic.public-output-private-type",
                &subject,
                &site,
                None,
            );
        }
    }
}

/// `semantic.portable-target-reference`: a portable definition directly
/// references a target-specific definition.
fn portability(context: &mut Context, profile: &ValidationProfile) {
    let project = context.project();
    for definition in &project.definitions {
        if definition.common().portability != Some(Portability::Portable) {
            continue;
        }
        let subject = definition.id().as_str().to_owned();
        for site in all_sites(definition) {
            let Some(found) = resolve(context, site.target) else {
                continue;
            };
            if found.common().portability != Some(Portability::TargetSpecific) {
                continue;
            }
            emit_reference(
                context,
                profile,
                "semantic.portable-target-reference",
                &subject,
                &site,
                None,
            );
        }
    }
}
