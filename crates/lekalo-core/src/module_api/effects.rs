//! Cross-owner write checking over typed effect facts (issue #113).
//!
//! A declaration, a transaction group, or a shared type never grants write
//! permission. For every actual declared write effect (create/update/delete,
//! entity-wide or exact field) the acting operation's owner module is
//! resolved against the aggregate (resource) owner module; any cross-owner
//! write must be covered by an explicit shared-aggregate grant with exact
//! operation and field scope. Broader actual effects and stale/missing
//! grants are refused; the effect edge key, resource, and path evidence are
//! preserved.

use super::diagnostic::Evidence;
use super::resolve::Resolved;
use super::wire::GrantPolicy;
use crate::effects::{EffectGraph, EffectKind};
use crate::ir::{Compilation, Definition};
use std::collections::BTreeMap;

/// The owner module of one semantic symbol id (`module.symbol`).
pub(crate) fn owner_of(symbol: &str) -> Option<&str> {
    symbol.split_once('.').map(|(owner, _)| owner)
}

/// The owner module of one kind-qualified graph id
/// (`operation:module.symbol`, `entity:module.symbol`).
pub(crate) fn owner_of_qualified(qualified: &str) -> Option<&str> {
    let symbol = qualified
        .split_once(':')
        .map(|(_, id)| id)
        .unwrap_or(qualified);
    owner_of(symbol)
}

/// One checked grant: the typed selection scope.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GrantScope {
    aggregate: String,
    operations: Vec<String>,
    fields: Vec<String>,
}

/// Check every declared write edge of the effect graph. Cross-owner writes
/// without a covering grant produce one typed evidence per effect edge,
/// keeping the canonical effect key, resource, and acting operation.
pub fn check_writes(
    resolved: &Resolved,
    _compilation: &Compilation,
    effects: &EffectGraph,
) -> Vec<Evidence> {
    let mut grants: BTreeMap<&str, Vec<&GrantPolicy>> = BTreeMap::new();
    for module in resolved.modules.values() {
        for grant in &module.grants {
            grants
                .entry(grant.consumer.as_str())
                .or_default()
                .push(grant);
        }
    }
    let mut evidence = Vec::new();
    for edge in effects.declared() {
        let kind = edge.key().kind();
        let (operation_name, field_scope) = match kind {
            EffectKind::Create => ("create", None),
            EffectKind::Update => ("update", None),
            EffectKind::Delete => ("delete", None),
            EffectKind::WriteField { .. } => (
                "update",
                edge.key().subject().field().map(|f| f.as_str().to_owned()),
            ),
            _ => continue,
        };
        let resource = edge.key().subject().resource().as_str();
        let operation = edge.key().operation().as_str();
        let Some(consumer) = owner_of_qualified(operation) else {
            continue;
        };
        let Some(producer) = owner_of_qualified(resource) else {
            continue;
        };
        if consumer == producer {
            continue;
        }
        // The producing module must actually declare the shared-aggregate
        // grant for this consumer.
        let Some(producer_policy) = resolved.modules.get(producer) else {
            evidence.push(write_evidence(
                operation,
                resource,
                edge.key().to_canonical_string().as_str(),
                None,
                field_scope.as_deref(),
                consumer,
                producer,
            ));
            continue;
        };
        let covered = producer_policy.grants.iter().any(|grant| {
            grant.consumer == consumer
                && covers(
                    &GrantScope {
                        aggregate: grant.aggregate.clone(),
                        operations: grant.operations.clone(),
                        fields: grant.fields.clone(),
                    },
                    resource,
                    operation_name,
                    field_scope.as_deref(),
                )
        });
        if !covered {
            evidence.push(write_evidence(
                operation,
                resource,
                edge.key().to_canonical_string().as_str(),
                None,
                field_scope.as_deref(),
                consumer,
                producer,
            ));
        }
    }
    evidence
}

/// Whether one grant covers the exact write: aggregate identity, operation
/// membership, and field scope (`*` covers entity-wide and every field; an
/// entity-wide write is never covered by a field-only grant).
fn covers(scope: &GrantScope, resource: &str, operation: &str, field: Option<&str>) -> bool {
    if scope.aggregate != resource {
        return false;
    }
    if !scope
        .operations
        .iter()
        .any(|candidate| candidate == operation)
    {
        return false;
    }
    match field {
        None => scope.fields.iter().any(|candidate| candidate == "*"),
        Some(field) => scope
            .fields
            .iter()
            .any(|candidate| candidate == "*" || candidate == field),
    }
}

/// Assemble the typed evidence of one forbidden cross-owner write.
#[allow(clippy::too_many_arguments)]
fn write_evidence(
    operation: &str,
    resource: &str,
    effect_key: &str,
    grant: Option<&str>,
    field: Option<&str>,
    consumer: &str,
    producer: &str,
) -> Evidence {
    let mut evidence = Evidence::new(
        "module-api.cross-module-write-forbidden",
        operation,
        "cross-owner-write",
    );
    evidence.export = Some(operation.to_owned());
    evidence.member = Some(match field {
        Some(field) => format!("{resource}#{field}"),
        None => resource.to_owned(),
    });
    evidence.path = vec![effect_key.to_owned()];
    evidence.expected = grant.map(|grant| grant.to_owned());
    evidence.consumer = Some(consumer.to_owned());
    evidence.producer = Some(producer.to_owned());
    evidence
}

/// The module-projection adjacency over declared semantic dependencies:
/// one edge per cross-owner reference actually present in the IR, sorted
/// and deduplicated.
pub fn reference_edges(compilation: &Compilation) -> Vec<(String, String)> {
    let project = &compilation.project;
    let mut edges: Vec<(String, String)> = Vec::new();
    for definition in &project.definitions {
        let Some(from) = owner_of(definition.id().as_str()).map(str::to_owned) else {
            continue;
        };
        let mut record = |to: &str| {
            if to != from {
                edges.push((from.clone(), to.to_owned()));
            }
        };
        for reference in references_of(definition) {
            if let Some(owner) = owner_of(reference) {
                record(owner);
            }
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

/// Every semantic symbol reference one definition carries.
pub(crate) fn references_of(definition: &Definition) -> Vec<&str> {
    let mut references = Vec::new();
    fn push_type<'a>(type_ref: &'a crate::ir::TypeRef, references: &mut Vec<&'a str>) {
        match type_ref {
            crate::ir::TypeRef::Ref(symbol) => references.push(symbol.as_str()),
            crate::ir::TypeRef::List(inner) | crate::ir::TypeRef::Optional(inner) => {
                push_type(inner, references)
            }
        }
    }
    match definition {
        Definition::Scalar(_) | Definition::Enum(_) => {}
        Definition::ValueObject(value_object) => {
            for field in &value_object.fields {
                push_type(&field.r#type, &mut references);
            }
        }
        Definition::Entity(entity) => {
            for field in &entity.fields {
                push_type(&field.r#type, &mut references);
            }
        }
        Definition::Command(command) => {
            for field in &command.input {
                push_type(&field.r#type, &mut references);
            }
            for effect in &command.effects {
                references.push(effect.as_str());
            }
        }
        Definition::Query(query) => {
            for read in &query.reads {
                references.push(read.as_str());
            }
            if let Some(returns) = &query.returns {
                push_type(returns, &mut references);
            }
        }
        Definition::Policy(policy) => {
            for operation in &policy.applies_to {
                references.push(operation.as_str());
            }
        }
        Definition::Event(event) => {
            for field in &event.payload {
                push_type(&field.r#type, &mut references);
            }
        }
        Definition::Effect(effect) => {
            references.push(effect.entity.as_str());
            for event in &effect.emits {
                references.push(event.as_str());
            }
        }
        Definition::Endpoint(endpoint) => references.push(endpoint.invokes.as_str()),
        Definition::Scenario(scenario) => {
            for covered in &scenario.covers {
                references.push(covered.as_str());
            }
        }
        Definition::TargetBinding(_) => {}
    }
    references
}
