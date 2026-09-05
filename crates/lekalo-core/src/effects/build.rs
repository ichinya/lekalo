//! Deterministic declared-effect projection and evidence attachment
//! (issue #14).
//!
//! The builder walks the [`CompiledProject`] in canonical order and emits
//! exactly the effects the accepted Model declares: query `reads` become
//! entity-level `read` edges, command `effects` become CRUD edges on the
//! referenced effect's entity, and effect `emits` become `emit-event`
//! edges — every edge attributed to the operation that declares it. It
//! never invents field sets, jobs, externals, cache, publication, audit,
//! transaction, or target-write facts from names: those kinds enter only
//! through a typed evidence envelope. Every declared reference must
//! resolve and point at the declared kind; any miss is a fatal
//! `graph.input-invalid` and no graph is produced.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::diagnostics::types::DataObject;
use crate::diagnostics::DiagnosticSet;
use crate::ir::{CompiledProject, Definition, DefinitionKind, EffectOperation};

use super::diagnostic::{self, token};
use super::edge::{EffectEdge, EffectKey};
use super::identity::{EffectOrigin, OperationId, ResourceId, ResourceKind, Subject};
use super::kind::EffectKind;
use super::provenance::{
    validate_envelope, DeclaredRole, EffectProvenance, EvidenceEnvelope, TrustState,
};
use super::version::MAX_EFFECTS;
use super::EffectGraph;

/// Build the declared effect projection of one compiled project.
///
/// The graph is either complete or absent: every fatal input violation is
/// collected into one normalized `invalid` set (exit 1 for CLI callers).
pub fn build(project: &CompiledProject) -> Result<EffectGraph, DiagnosticSet> {
    let mut builder = Builder {
        project,
        kinds: definition_kinds(project),
        digest: ir_digest(project),
        edges: Vec::new(),
        cap_violation: false,
    };
    builder.project_declared_effects()?;
    if builder.cap_violation || builder.edges.len() > MAX_EFFECTS {
        return Err(diagnostic::cap_exceeded_set("effect-limit"));
    }
    builder.finish()
}

/// The semantic id to definition-kind map of one project.
fn definition_kinds(project: &CompiledProject) -> HashMap<&str, DefinitionKind> {
    project
        .definitions
        .iter()
        .map(|definition| (definition.id().as_str(), definition.kind()))
        .collect()
}

/// The `sha256` digest of the canonical IR bytes: the exact revision the
/// declared projection was built from.
fn ir_digest(project: &CompiledProject) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project.to_canonical_json().as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// The single-pass declared construction state.
struct Builder<'a> {
    project: &'a CompiledProject,
    kinds: HashMap<&'a str, DefinitionKind>,
    digest: String,
    edges: Vec<EffectEdge>,
    /// Set when the recorded edge cap was crossed: construction then
    /// fails closed instead of truncating by arrival order.
    cap_violation: bool,
}

impl<'a> Builder<'a> {
    /// Freeze the canonical order and assemble the indexed graph.
    fn finish(self) -> Result<EffectGraph, DiagnosticSet> {
        let mut edges = self.edges;
        edges.sort_by_key(|edge| edge.sort_key());
        edges.dedup_by(|left, right| left.key() == right.key());
        Ok(EffectGraph::assemble(
            self.project
                .project
                .as_ref()
                .map(|project| project.id.as_str().to_owned()),
            self.project.model_version,
            self.digest,
            edges,
            Vec::new(),
            0,
        ))
    }

    /// Every declared effect of every operation-carrying definition, in
    /// canonical order.
    fn project_declared_effects(&mut self) -> Result<(), DiagnosticSet> {
        for definition in &self.project.definitions {
            match definition {
                Definition::Query(query) => {
                    let operation = match OperationId::from_semantic(query.id.as_str()) {
                        Some(operation) => operation,
                        None => return Err(malformed_operation(query.id.as_str())),
                    };
                    for (ordinal, read) in query.reads.iter().enumerate() {
                        self.emit_declared_read(&operation, read.as_str(), ordinal)?;
                    }
                }
                Definition::Command(command) => {
                    let operation = match OperationId::from_semantic(command.id.as_str()) {
                        Some(operation) => operation,
                        None => return Err(malformed_operation(command.id.as_str())),
                    };
                    for (ordinal, effect) in command.effects.iter().enumerate() {
                        self.emit_declared_command_effect(&operation, effect.as_str(), ordinal)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// One declared read: the query reads one canonical entity resource.
    fn emit_declared_read(
        &mut self,
        operation: &OperationId,
        read: &str,
        ordinal: usize,
    ) -> Result<(), DiagnosticSet> {
        if self.kinds.get(read) != Some(&DefinitionKind::Entity) {
            return Err(unresolved_declared(operation, read));
        }
        let Ok(occurrence) = u32::try_from(ordinal) else {
            return Ok(());
        };
        let Some(resource) = ResourceId::new(ResourceKind::Canonical, read) else {
            return Err(malformed_subject(operation, read));
        };
        self.push(
            operation,
            EffectKind::Read,
            Subject::new(resource),
            EffectOrigin::Occurrence(occurrence),
            DeclaredRole::QueryReads,
            occurrence,
        );
        Ok(())
    }

    /// One declared command effect: CRUD on the referenced effect's
    /// entity, plus the effect's declared event emissions.
    fn emit_declared_command_effect(
        &mut self,
        operation: &OperationId,
        effect: &str,
        ordinal: usize,
    ) -> Result<(), DiagnosticSet> {
        let definition = self
            .project
            .definitions
            .iter()
            .find(|definition| definition.id().as_str() == effect);
        let Some(Definition::Effect(effect_definition)) = definition else {
            return Err(unresolved_declared(operation, effect));
        };
        let entity = effect_definition.entity.as_str();
        if self.kinds.get(entity) != Some(&DefinitionKind::Entity) {
            return Err(unresolved_declared(operation, entity));
        }
        let Ok(occurrence) = u32::try_from(ordinal) else {
            return Ok(());
        };
        let Some(resource) = ResourceId::new(ResourceKind::Canonical, entity) else {
            return Err(malformed_subject(operation, entity));
        };
        let kind = match effect_definition.operation {
            EffectOperation::Create => EffectKind::Create,
            EffectOperation::Update => EffectKind::Update,
            EffectOperation::Delete => EffectKind::Delete,
        };
        self.push(
            operation,
            kind,
            Subject::new(resource),
            EffectOrigin::Declared(effect.to_owned()),
            DeclaredRole::CommandEffect,
            occurrence,
        );
        for (emit_ordinal, emitted) in effect_definition.emits.iter().enumerate() {
            let Ok(emit_occurrence) = u32::try_from(emit_ordinal) else {
                continue;
            };
            if self.kinds.get(emitted.as_str()) != Some(&DefinitionKind::Event) {
                return Err(unresolved_declared(operation, emitted.as_str()));
            }
            let Some(resource) = ResourceId::new(ResourceKind::Event, emitted.as_str()) else {
                return Err(malformed_subject(operation, emitted.as_str()));
            };
            self.push(
                operation,
                EffectKind::EmitEvent,
                Subject::new(resource),
                EffectOrigin::Declared(effect.to_owned()),
                DeclaredRole::EffectEmits,
                emit_occurrence,
            );
        }
        Ok(())
    }

    /// Push one declared edge; a crossed cap records the violation and
    /// the whole construction fails closed at the end.
    fn push(
        &mut self,
        operation: &OperationId,
        kind: EffectKind,
        subject: Subject,
        origin: EffectOrigin,
        role: DeclaredRole,
        occurrence: u32,
    ) {
        if self.edges.len() >= MAX_EFFECTS {
            self.cap_violation = true;
            return;
        }
        let symbol = operation.semantic_id().to_owned();
        self.edges.push(EffectEdge::new(
            EffectKey::new(operation.clone(), kind, subject, origin, occurrence),
            EffectProvenance::CanonicalIr {
                ir_digest: self.digest.clone(),
                role,
                occurrence,
                symbol,
            },
            None,
            None,
        ));
    }
}

/// The fatal set for a declared reference that does not resolve to its
/// declared kind.
fn unresolved_declared(operation: &OperationId, target: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("target".to_owned(), token(target));
    data.insert("detail".to_owned(), token("unresolved-declared"));
    match diagnostic::one(
        diagnostic::INPUT_INVALID,
        Some(diagnostic::bounded(operation.as_str())),
        data,
    ) {
        Ok(set_diagnostic) => diagnostic::invalid_set(vec![set_diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for an operation id that failed its grammar.
fn malformed_operation(symbol: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("malformed-operation"));
    match diagnostic::one(
        diagnostic::INPUT_INVALID,
        Some(diagnostic::bounded(symbol)),
        data,
    ) {
        Ok(set_diagnostic) => diagnostic::invalid_set(vec![set_diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for a subject id that failed its grammar.
fn malformed_subject(operation: &OperationId, subject: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("target".to_owned(), token(subject));
    data.insert("detail".to_owned(), token("malformed-subject"));
    match diagnostic::one(
        diagnostic::INPUT_INVALID,
        Some(diagnostic::bounded(operation.as_str())),
        data,
    ) {
        Ok(set_diagnostic) => diagnostic::invalid_set(vec![set_diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// Attach one detected-evidence envelope to a finished graph without
/// mutating the canonical projection: the returned graph shares the exact
/// declared edges and appends the validated evidence records (issue #14
/// acceptance: adapters add detected effects; the canonical model stays
/// untouched).
pub(crate) fn attach(
    graph: &EffectGraph,
    envelope: &EvidenceEnvelope,
) -> Result<EffectGraph, DiagnosticSet> {
    if let Err(violation) = validate_envelope(envelope, super::version::MAX_ENVELOPE_ENTRIES) {
        return Err(diagnostic::envelope_violation_set(&violation));
    }
    if envelope.trust == TrustState::Rejected {
        return Err(diagnostic::input_invalid_detail("rejected-trust"));
    }
    let mut detected = Vec::with_capacity(envelope.entries.len());
    for entry in &envelope.entries {
        if entry.trust == Some(TrustState::Rejected) {
            return Err(diagnostic::input_invalid_detail("rejected-trust"));
        }
        detected.push(EffectEdge::new(
            EffectKey::new(
                entry.operation.clone(),
                entry.kind,
                entry.subject.clone(),
                EffectOrigin::Occurrence(entry.occurrence),
                entry.occurrence,
            ),
            EffectProvenance::Evidence {
                adapter_id: envelope.adapter_id.clone(),
                target_id: envelope.target_id.clone(),
                protocol_version: envelope.protocol_version.clone(),
                evidence_digest: envelope.evidence_digest.clone(),
                trust: entry.trust.unwrap_or(envelope.trust),
            },
            entry.transaction_group.clone(),
            entry.sensitivity.clone(),
        ));
    }
    if graph.detected().len() + detected.len() > MAX_EFFECTS {
        return Err(diagnostic::cap_exceeded_set("effect-limit"));
    }
    let mut combined = graph.detected().to_vec();
    combined.extend(detected);
    combined.sort_by_key(|edge| edge.sort_key());
    combined.dedup_by(|left, right| left.key() == right.key());
    Ok(EffectGraph::assemble(
        graph.project_id().map(|project| project.to_owned()),
        graph.model_version(),
        graph.ir_digest().to_owned(),
        graph.declared().to_vec(),
        combined,
        graph.envelope_count() + 1,
    ))
}
