//! The static authorization review over one compiled project
//! (issue #25).
//!
//! [`review`] consumes the immutable [`Compilation`] plus the optional
//! accepted authorization document and produces one of three outcomes:
//! structurally valid references everywhere ([`Review::Ok`]), a
//! profile-independent contract violation ([`Review::Invalid`], exit 1),
//! or a strict-profile block ([`Review::Denied`], exit 3). The review is
//! pure: it reads the typed IR, never files, adapters, or networks.
//!
//! Profile separation is explicit and documented: reference integrity is
//! invalid in every profile; staleness, protected-effect coverage, and
//! adapter mapping states block only the built-in strict profile. The
//! default profile never silently weakens strict — it reports fewer
//! blocks by design, and strict is the profile adapters must pass.

use super::diagnostic as diag;
use super::document::{Decision, Document, MappingState, ScopeAnchor, ScopeSource, SymbolDecl};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::ir::{Compilation, Definition};
use std::collections::BTreeMap;

/// The review outcome.
#[derive(Clone, Debug)]
pub enum Review {
    /// The document and project are consistent; strict blocks do not
    /// apply.
    Ok,
    /// A profile-independent contract violation (exit 1).
    Invalid(DiagnosticSet),
    /// A strict-profile block (exit 3).
    Denied(DiagnosticSet),
}

/// One protected effect reference from one operation.
struct EffectRef {
    /// The effect class: `read` for query reads, otherwise the effect
    /// operation (`create`, `update`, `delete`).
    class: &'static str,
}

/// One indexed operation.
struct OperationInfo {
    /// The declared input field names, sorted.
    input_fields: Vec<String>,
    /// The entities this operation touches, sorted.
    entities: Vec<String>,
    /// The declared effect references.
    effects: Vec<EffectRef>,
}

/// The typed project index the review resolves references against.
struct ProjectIndex {
    operations: BTreeMap<String, OperationInfo>,
    entities: BTreeMap<String, Vec<String>>,
}

impl ProjectIndex {
    /// Index one compilation: declared entities, command inputs and
    /// effects, and query reads, all in canonical order.
    fn build(compilation: &Compilation) -> Self {
        let mut entities: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for definition in &compilation.project.definitions {
            if let Definition::Entity(entity) = definition {
                let mut fields: Vec<String> = entity
                    .fields
                    .iter()
                    .map(|field| field.name.as_str().to_owned())
                    .collect();
                fields.sort();
                fields.dedup();
                entities.insert(entity.id.as_str().to_owned(), fields);
            }
        }
        let mut operations: BTreeMap<String, OperationInfo> = BTreeMap::new();
        for definition in &compilation.project.definitions {
            match definition {
                Definition::Command(command) => {
                    let mut inputs: Vec<String> = command
                        .input
                        .iter()
                        .map(|field| field.name.as_str().to_owned())
                        .collect();
                    inputs.sort();
                    inputs.dedup();
                    let mut effects = Vec::new();
                    let mut touched: Vec<String> = Vec::new();
                    for effect_id in &command.effects {
                        if let Some(Definition::Effect(effect)) = compilation
                            .project
                            .definitions
                            .iter()
                            .find(|d| d.id().as_str() == effect_id.as_str())
                        {
                            effects.push(EffectRef {
                                class: effect.operation.as_str(),
                            });
                            touched.push(effect.entity.as_str().to_owned());
                        }
                    }
                    touched.sort();
                    touched.dedup();
                    operations.insert(
                        command.id.as_str().to_owned(),
                        OperationInfo {
                            input_fields: inputs,
                            entities: touched,
                            effects,
                        },
                    );
                }
                Definition::Query(query) => {
                    let mut effects = Vec::new();
                    let mut touched: Vec<String> = Vec::new();
                    for read in &query.reads {
                        effects.push(EffectRef { class: "read" });
                        touched.push(read.as_str().to_owned());
                    }
                    touched.sort();
                    touched.dedup();
                    operations.insert(
                        query.id.as_str().to_owned(),
                        OperationInfo {
                            input_fields: Vec::new(),
                            entities: touched,
                            effects,
                        },
                    );
                }
                _ => {}
            }
        }
        Self {
            operations,
            entities,
        }
    }
}

/// The declared symbol table (capabilities then roles) for reference
/// resolution and acyclicity checks.
fn symbol_tables(document: &Document) -> BTreeMap<&str, &SymbolDecl> {
    let mut table = BTreeMap::new();
    for decl in &document.capabilities {
        table.insert(decl.id.as_str(), decl);
    }
    for decl in &document.roles {
        table.insert(decl.id.as_str(), decl);
    }
    table
}

/// Depth-first cycle detection over an id → ids graph.
fn has_cycle(start: &str, edges: &BTreeMap<&str, Vec<String>>) -> bool {
    fn visit(node: &str, edges: &BTreeMap<&str, Vec<String>>, stack: &mut Vec<String>) -> bool {
        if stack.iter().any(|entry| entry == node) {
            return true;
        }
        let Some(targets) = edges.get(node) else {
            return false;
        };
        stack.push(node.to_owned());
        for target in targets {
            if visit(target, edges, stack) {
                return true;
            }
        }
        stack.pop();
        false
    }
    let mut stack = Vec::new();
    visit(start, edges, &mut stack)
}

/// Whether the composition member graph cycles.
fn composition_cycles(document: &Document) -> bool {
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for composition in &document.compositions {
        let entry = edges.entry(composition.id.clone()).or_default();
        entry.extend(composition.all_of.iter().cloned());
        entry.extend(composition.any_of.iter().cloned());
        entry.sort();
        entry.dedup();
    }
    let borrowed: BTreeMap<&str, Vec<String>> = edges
        .iter()
        .map(|(id, targets)| (id.as_str(), targets.clone()))
        .collect();
    document
        .compositions
        .iter()
        .any(|composition| has_cycle(&composition.id, &borrowed))
}

/// Whether the capability/role implication graph cycles.
fn implication_cycles(document: &Document) -> bool {
    let mut edges: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for decl in document.capabilities.iter().chain(document.roles.iter()) {
        edges.insert(decl.id.as_str(), decl.implies.clone());
    }
    let ids: Vec<&str> = edges.keys().copied().collect();
    ids.iter().any(|id| has_cycle(id, &edges))
}

/// Whether one field is declared on any entity one operation touches.
fn field_known(index: &ProjectIndex, operation: &str, field: &str) -> bool {
    index
        .operations
        .get(operation)
        .map(|info| {
            info.entities.iter().any(|entity| {
                index
                    .entities
                    .get(entity)
                    .map(|fields| fields.iter().any(|f| f == field))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Whether one input field is declared by one operation.
fn input_known(index: &ProjectIndex, operation: &str, field: &str) -> bool {
    index
        .operations
        .get(operation)
        .map(|info| info.input_fields.iter().any(|f| f == field))
        .unwrap_or(false)
}

/// One allow policy that covers an operation.
fn covering_allows<'a>(
    document: &'a Document,
    operation: &str,
) -> Vec<&'a super::document::Policy> {
    document
        .policies
        .iter()
        .filter(|policy| {
            policy.decision == Decision::Allow && policy.applies_to.iter().any(|op| op == operation)
        })
        .collect()
}

/// The strict review: coverage, and mapping states.
fn strict_blocks(
    compilation: &Compilation,
    document: &Document,
    index: &ProjectIndex,
) -> Vec<Diagnostic> {
    let mut denied = Vec::new();
    // Stale model pin: the document describes a different model or an
    // older compiled payload; either way strict cannot trust it.
    let current_digest = format!(
        "sha256:{}",
        crate::versioning::plan::sha256_hex(compilation.project.to_canonical_json().as_bytes())
    );
    if document.model_ref.schema_version != compilation.project.model_version.as_str()
        || document.model_ref.ir_digest != current_digest
    {
        denied.push(diag::mapping_stale("model_ref", "stale-model").expect("registered rule"));
    }
    // Protected-effect coverage is operation-level for declared IR
    // effects: every protected operation needs at least one allow
    // policy. Field-level exhaustiveness is enforced by the evaluator
    // against the concrete field surface (the vector matrix proves it);
    // an entity-scoped IR effect cannot statically know its exposed
    // field projection, so guessing here would be optimistic.
    for (operation, info) in &index.operations {
        for effect in &info.effects {
            if covering_allows(document, operation).is_empty() {
                denied.push(
                    diag::effect_unprotected(operation, effect.class).expect("registered rule"),
                );
            }
        }
    }
    // Adapter mapping states: only verified full mapping passes strict.
    for mapping in &document.mappings {
        if mapping.state != MappingState::Full {
            denied.push(
                diag::mapping_stale(&mapping.semantic_policy_ref, mapping.state.as_str())
                    .expect("registered rule"),
            );
        }
    }
    denied
}

/// The profile-independent reference review.
fn reference_review(document: &Document, index: &ProjectIndex) -> Vec<Diagnostic> {
    let mut invalid = Vec::new();
    let symbols = symbol_tables(document);
    for policy in &document.policies {
        for operation in &policy.applies_to {
            if !index.operations.contains_key(operation) {
                invalid.push(diag::ref_unresolved(&policy.id, operation).expect("registered rule"));
            }
        }
        for capability in &policy.capabilities {
            if !symbols.contains_key(capability.as_str()) {
                invalid
                    .push(diag::ref_unresolved(&policy.id, capability).expect("registered rule"));
            }
        }

        for role in &policy.roles {
            if !symbols.contains_key(role.as_str()) {
                invalid.push(diag::ref_unresolved(&policy.id, role).expect("registered rule"));
            }
        }
        if let Some(composition) = &policy.composition {
            if !document.compositions.iter().any(|c| &c.id == composition) {
                invalid
                    .push(diag::ref_unresolved(&policy.id, composition).expect("registered rule"));
            }
        }
        for binding in [
            &policy.scope.tenant,
            &policy.scope.workspace,
            &policy.scope.user,
        ]
        .into_iter()
        .flatten()
        {
            if let ScopeSource::Input(field) = &binding.source {
                if !policy
                    .applies_to
                    .iter()
                    .any(|operation| input_known(index, operation, field))
                {
                    invalid.push(
                        diag::ref_unresolved(&policy.id, &format!("input.{field}"))
                            .expect("registered rule"),
                    );
                }
            }
            let mut resource_fields: Vec<&String> = Vec::new();
            if let ScopeSource::Resource(field) = &binding.source {
                resource_fields.push(field);
            }
            if let ScopeAnchor::Resource(field) = &binding.anchor {
                resource_fields.push(field);
            }
            for field in resource_fields {
                if !policy
                    .applies_to
                    .iter()
                    .all(|operation| field_known(index, operation, field))
                {
                    invalid.push(
                        diag::ref_unresolved(&policy.id, &format!("resource.{field}"))
                            .expect("registered rule"),
                    );
                }
            }
        }
        for ownership in &policy.ownership {
            if !policy
                .applies_to
                .iter()
                .all(|operation| field_known(index, operation, &ownership.resource_field))
            {
                invalid.push(
                    diag::ref_unresolved(
                        &policy.id,
                        &format!("resource.{}", ownership.resource_field),
                    )
                    .expect("registered rule"),
                );
            }
        }
    }
    // Symbol implication targets must exist.
    for decl in document.capabilities.iter().chain(document.roles.iter()) {
        for target in &decl.implies {
            if !symbols.contains_key(target.as_str()) {
                invalid.push(diag::ref_unresolved(&decl.id, target).expect("registered rule"));
            }
        }
    }
    // Composition members must exist; empty compositions are ambiguous.
    for composition in &document.compositions {
        if composition.all_of.is_empty() && composition.any_of.is_empty() {
            invalid.push(
                diag::composition_invalid(&composition.id, "ambiguous-empty")
                    .expect("registered rule"),
            );
        }
        for member in composition.all_of.iter().chain(composition.any_of.iter()) {
            let known = document.policies.iter().any(|p| &p.id == member)
                || document.compositions.iter().any(|c| &c.id == member);
            if !known {
                invalid
                    .push(diag::ref_unresolved(&composition.id, member).expect("registered rule"));
            }
        }
    }
    // Mappings must reference declared policies.
    for mapping in &document.mappings {
        if !document
            .policies
            .iter()
            .any(|p| p.id == mapping.semantic_policy_ref)
        {
            invalid.push(
                diag::ref_unresolved(&mapping.target, &mapping.semantic_policy_ref)
                    .expect("registered rule"),
            );
        }
    }
    // Acyclicity of the implication and composition graphs.
    if implication_cycles(document) {
        invalid.push(diag::composition_invalid("symbol-graph", "cycle").expect("registered rule"));
    }
    if composition_cycles(document) {
        invalid.push(
            diag::composition_invalid("composition-graph", "cycle").expect("registered rule"),
        );
    }
    invalid
}

/// Review one compiled project against its optional authorization
/// document under the selected enforcement level.
/// Review one selected project: resolve the project root, read the
/// optional canonical authorization document, and review it against the
/// compilation. This is the single seam the CLI hands off to; the CLI
/// never reads authorization files itself.
pub fn review_selection(
    selection: &crate::loader::LoadSelection,
    compilation: &Compilation,
    strict: bool,
) -> Result<Review, crate::result::DomainResult> {
    let root = crate::loader::root_for_selection(selection)?;
    match super::parse::read_document(&root) {
        Err(set) => Ok(Review::Invalid(set)),
        Ok(document) => Ok(review(compilation, document.as_ref(), strict)),
    }
}

pub fn review(compilation: &Compilation, document: Option<&Document>, strict: bool) -> Review {
    let index = ProjectIndex::build(compilation);
    let Some(document) = document else {
        // No document: strict blocks every operation with protected
        // effects; the default profile is advisory and passes.
        if strict {
            let denied: Vec<Diagnostic> = index
                .operations
                .iter()
                .filter(|(_, info)| !info.effects.is_empty())
                .map(|(id, info)| {
                    let class = info
                        .effects
                        .first()
                        .map(|effect| effect.class)
                        .unwrap_or("effect");
                    diag::effect_unprotected(id, class).expect("registered rule")
                })
                .collect();
            if !denied.is_empty() {
                return Review::Denied(diag::denied_set(denied));
            }
        }
        return Review::Ok;
    };
    let invalid = reference_review(document, &index);
    if !invalid.is_empty() {
        return Review::Invalid(diag::invalid_set(invalid));
    }
    if strict {
        let denied = strict_blocks(compilation, document, &index);
        if !denied.is_empty() {
            return Review::Denied(diag::denied_set(denied));
        }
    }
    Review::Ok
}

/// The sorted protected-operation ids of one project: the coverage
/// surface fixtures and tests assert against.
pub fn protected_operations(compilation: &Compilation) -> Vec<String> {
    let index = ProjectIndex::build(compilation);
    index
        .operations
        .iter()
        .filter(|(_, info)| !info.effects.is_empty())
        .map(|(id, _)| id.clone())
        .collect()
}
