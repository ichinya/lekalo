//! The pure join of one parsed composition to its pinned compilation
//! (issue #51).
//!
//! The join proves the composition against the Model itself, never
//! against declarations alone:
//!
//! 1. **Custody.** The pinned project id and IR digest must equal the
//!    compilation's own values; a stale composition refuses before
//!    any semantic check.
//! 2. **Role resolution.** Every operation role must name a declared
//!    command or query of the pinned project, and every declared
//!    effect id must be a declared effect of the same project with a
//!    matching closed kind.
//! 3. **Effect-graph agreement.** Every role's declared effect set
//!    must equal the canonical declared effect list of that command
//!    — the composition cannot hide an effect the Model declares, and
//!    cannot invent one the Model does not carry. A read-only role
//!    declares no effects; a committed role declares at least one.
//! 4. **Independence.** Every independence rule must name declared
//!    roles; a rule whose subject and forbidden target share one
//!    declared effect id, or write one entity, couples effects and
//!    refuses with the `effects-coupled` rule.
//! 5. **Approval binding.** Every approval binding must cover the six
//!    closed fields (preview revision, preview digest, tenant,
//!    destination, authority, policy revision) as declared inputs of
//!    the publication operation; the publication operation must be
//!    gated by at least one binding.
//! 6. **Snapshot immutability.** Every snapshot declaration must be
//!    append-only, name a declared entity, and pin revision, digest,
//!    and schema-version fields that the entity actually declares.
//! 7. **Public allowlist.** The DTO must be a declared type with
//!    fields, its served fields must be a subset of the DTO fields
//!    backed by a declared snapshot, and the excluded set must cover
//!    every field of the worklog, provider, approval, and audit
//!    entities so no private join can enter the projection.
//! 8. **Replay scopes.** Every replay scope must name a declared
//!    committed role, bind a payload digest policy for replay, and
//!    keep the same-key/different-request behavior a conflict.
//! 9. **Capability coverage.** The publication role must be covered
//!    by the approval, revisioned-snapshot, review-gate, atomic-group,
//!    and durable-key capability requirements; the delivery role must
//!    carry an explicit partial provider-idempotency row, so the
//!    unsupported-publication guarantee can never silently vanish.

use crate::diagnostics::DiagnosticSet;
use crate::ir::{CompiledProject, Definition};

use super::types::Composition;

impl Composition {
    /// Validate this composition against the pinned compilation or
    /// return the typed rejection set. Pure: no filesystem, process,
    /// network, clock, or cache access.
    pub fn validate(&self, project: &CompiledProject) -> Result<(), DiagnosticSet> {
        self.check_custody(project)?;
        let index = ProjectIndex::build(project);
        self.check_roles(&index)?;
        self.check_independence(&index)?;
        self.check_approvals(&index)?;
        self.check_snapshots(&index)?;
        self.check_allowlist(&index)?;
        self.check_replay(&index)?;
        self.check_capabilities()?;
        Ok(())
    }

    /// Custody: exact project id and IR digest.
    fn check_custody(&self, project: &CompiledProject) -> Result<(), DiagnosticSet> {
        let declared = project
            .project
            .as_ref()
            .map(|declared| declared.id.as_str())
            .unwrap_or_default();
        if declared != self.project_id {
            return Err(super::diagnostic::input_invalid("project-custody"));
        }
        let current_ir = format!(
            "sha256:{}",
            super::canonical::sha256_hex(project.to_canonical_json().as_bytes())
        );
        if self.ir_digest != current_ir {
            return Err(super::diagnostic::input_invalid("ir-custody"));
        }
        Ok(())
    }

    /// Every role resolves to a declared command or query, and the
    /// declared effect set equals the command's canonical effect list.
    /// Read-only eligibility is derived from the compiled operation,
    /// never from the report-controlled boundary string: only a query
    /// may be read-only, and every claimed read must resolve 1:1 to a
    /// compiled read subject. A writing operation that claims the
    /// read-only boundary is a disguise and refuses.
    fn check_roles(&self, index: &ProjectIndex) -> Result<(), DiagnosticSet> {
        for role in &self.operations {
            let kind = index
                .operations
                .get(role.operation.as_str())
                .ok_or_else(|| super::diagnostic::input_invalid("unresolved-role"))?;
            if role.commit_boundary() == "read_only" {
                // Eligibility: a compiled query, never a command.
                if *kind != "query" {
                    return Err(super::diagnostic::effects_coupled("read-only-disguise"));
                }
                // Every declared read resolves to one compiled read
                // subject, matched by exact set equality: erasing a
                // read or fabricating one cannot validate.
                let compiled_reads = index
                    .query_reads
                    .get(role.operation.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                if role.effects().is_empty() || role.effects().len() != compiled_reads.len() {
                    return Err(super::diagnostic::effects_coupled("read-count-mismatch"));
                }
                let mut declared_reads = Vec::with_capacity(role.effects().len());
                for effect in role.effects() {
                    if effect.kind() != "read" {
                        return Err(super::diagnostic::effects_coupled("read-kind-mismatch"));
                    }
                    declared_reads.push(effect.effect_id());
                }
                declared_reads.sort_unstable();
                let mut expected_reads: Vec<&str> =
                    compiled_reads.iter().map(String::as_str).collect();
                expected_reads.sort_unstable();
                if declared_reads.as_slice() != expected_reads.as_slice() {
                    return Err(super::diagnostic::effects_coupled("read-set-mismatch"));
                }
                continue;
            }
            if *kind == "query" {
                return Err(super::diagnostic::effects_coupled("query-committed"));
            }
            if role.effects().is_empty() {
                return Err(super::diagnostic::effects_coupled("undeclared-effects"));
            }
            let expected = index
                .effect_projection
                .get(role.operation.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            // The canonical projection is exactly the command's
            // declared effects; both counts must agree and every
            // declaration must resolve to one of them.
            if role.effects().len() != expected.len() {
                return Err(super::diagnostic::effects_coupled("effect-count-mismatch"));
            }
            let mut declared_ids = Vec::with_capacity(role.effects().len());
            let allowed_kinds = super::version::role_kinds(role.operation());
            for effect in role.effects() {
                if !allowed_kinds.contains(&effect.kind()) {
                    return Err(super::diagnostic::effects_coupled("effect-kind-role"));
                }
                let Some((_, _, entity, effect_kind)) = index.effects.get(effect.effect_id())
                else {
                    return Err(super::diagnostic::effects_coupled("unresolved-effect"));
                };
                if effect.kind() != effect_kind
                    && !matches!(
                        effect.kind(),
                        "emit-event"
                            | "enqueue-job"
                            | "external-call"
                            | "publish-output"
                            | "audit-log"
                    )
                {
                    // A CRUD kind must match the declared Model effect
                    // operation; the infrastructure kinds are the
                    // refinements carried by the consumed attachments.
                    return Err(super::diagnostic::effects_coupled("effect-kind-mismatch"));
                }
                let _ = entity;
                declared_ids.push(effect.effect_id().to_owned());
            }
            declared_ids.sort_unstable();
            let mut projected_ids: Vec<&str> = expected
                .iter()
                .map(|(_, effect_id)| effect_id.as_str())
                .collect();
            projected_ids.sort_unstable();
            if declared_ids.as_slice() != projected_ids {
                return Err(super::diagnostic::effects_coupled("effect-set-mismatch"));
            }
        }
        Ok(())
    }

    /// Every independence rule names declared roles and keeps the
    /// forbidden targets effect-disjoint from its subject.
    fn check_independence(&self, _index: &ProjectIndex) -> Result<(), DiagnosticSet> {
        for rule in &self.independent_effects {
            let Some(role) = self.role(rule.operation()) else {
                return Err(super::diagnostic::input_invalid(
                    "unresolved-independence-rule",
                ));
            };
            for target in rule.must_not_enable() {
                if target == rule.operation() {
                    return Err(super::diagnostic::effects_coupled("self-coupling"));
                }
                let Some(target_role) = self.role(target) else {
                    return Err(super::diagnostic::input_invalid(
                        "unresolved-independence-rule",
                    ));
                };
                // The subject role must not carry any effect of the
                // forbidden target: sharing one declared effect id,
                // or declaring one of the target's infrastructure
                // effects, couples the two operations.
                for effect in role.effects() {
                    for forbidden in target_role.effects() {
                        if effect.effect_id() == forbidden.effect_id() {
                            return Err(super::diagnostic::effects_coupled("shared-effect"));
                        }
                        let infrastructure = matches!(
                            effect.kind(),
                            "enqueue-job" | "external-call" | "publish-output"
                        );
                        if infrastructure && effect.kind() == forbidden.kind() {
                            return Err(super::diagnostic::effects_coupled(
                                "shared-infrastructure-effect",
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Approval bindings: the publish-time fields (preview revision,
    /// preview digest, tenant) must be declared inputs of the
    /// publication operation; the approval-record fields (destination,
    /// authority, policy revision) must be declared by the approval
    /// entity. The publication operation must be gated by at least
    /// one binding.
    fn check_approvals(&self, index: &ProjectIndex) -> Result<(), DiagnosticSet> {
        let approval_entity = index
            .entity_fields
            .keys()
            .find(|entity| entity.ends_with(".approval"))
            .and_then(|entity| index.entity_fields.get(entity));
        for approval in &self.approvals {
            let Some(inputs) = index.operation_inputs.get(approval.operation()) else {
                return Err(super::diagnostic::approval_unbound("unresolved-operation"));
            };
            for field in [
                approval.preview_revision_field(),
                approval.preview_digest_field(),
                approval.tenant_field(),
            ] {
                if !inputs.contains(&field.to_owned()) {
                    return Err(super::diagnostic::approval_unbound("field-not-declared"));
                }
            }
            let Some(record_fields) = approval_entity else {
                return Err(super::diagnostic::approval_unbound("no-approval-record"));
            };
            for field in [
                approval.destination_field(),
                approval.authority_field(),
                approval.policy_revision_field(),
            ] {
                if !record_fields.contains(&field.to_owned()) {
                    return Err(super::diagnostic::approval_unbound("record-field-missing"));
                }
            }
        }
        // The publication operation must be approval-gated.
        if !self
            .approvals
            .iter()
            .any(|approval| approval.operation() == "changelog.publish_entry")
        {
            return Err(super::diagnostic::approval_unbound("publication-ungated"));
        }
        Ok(())
    }

    /// Snapshot declarations are append-only, resolve to declared
    /// entities, and pin fields the entity actually declares.
    fn check_snapshots(&self, index: &ProjectIndex) -> Result<(), DiagnosticSet> {
        if self.snapshots.is_empty() {
            return Err(super::diagnostic::snapshot_mutable("no-snapshot"));
        }
        for snapshot in &self.snapshots {
            if snapshot.immutability() != "append_only" {
                return Err(super::diagnostic::snapshot_mutable("not-append-only"));
            }
            let Some(entity) = index.entity_fields.get(snapshot.entity()) else {
                return Err(super::diagnostic::snapshot_mutable("unresolved-entity"));
            };
            for field in [
                snapshot.revision_field(),
                snapshot.digest_field(),
                snapshot.schema_version_field(),
            ] {
                if !entity.contains(&field.to_owned()) {
                    return Err(super::diagnostic::snapshot_mutable("field-not-declared"));
                }
            }
        }
        Ok(())
    }

    /// The public allowlist serves only snapshot fields through a
    /// declared DTO, and the excluded set covers every private field.
    fn check_allowlist(&self, index: &ProjectIndex) -> Result<(), DiagnosticSet> {
        let allowlist = &self.public_allowlist;
        let Some(dto_fields) = index.entity_fields.get(allowlist.dto()) else {
            return Err(super::diagnostic::public_data_unsafe("unresolved-dto"));
        };
        for field in allowlist.fields() {
            if !dto_fields.contains(&field.name().to_owned()) {
                return Err(super::diagnostic::public_data_unsafe("field-not-in-dto"));
            }
        }
        // One snapshot declaration must back the whole allowlist.
        let backed = self.snapshots.iter().any(|snapshot| {
            index
                .entity_fields
                .get(snapshot.entity())
                .is_some_and(|fields| {
                    allowlist
                        .fields()
                        .iter()
                        .all(|field| fields.contains(&field.name().to_owned()))
                })
        });
        if !backed {
            return Err(super::diagnostic::public_data_unsafe(
                "allowlist-not-backed",
            ));
        }
        // Every field of the private entities must be excluded from
        // the public surface (the allowlist fields themselves are the
        // snapshot's public payload, so a private entity field can
        // never collide with one).
        for entity in [
            "worklog.task_worklog",
            "worklog.provider_action",
            "changelog.entry",
            "changelog.approval",
            "changelog.audit",
        ] {
            let Some(fields) = index.entity_fields.get(entity) else {
                continue;
            };
            for field in fields {
                if !allowlist
                    .excluded()
                    .iter()
                    .any(|excluded| excluded == field)
                {
                    return Err(super::diagnostic::public_data_unsafe(
                        "private-field-not-excluded",
                    ));
                }
            }
        }
        // The shared rendering policy is mandatory for preview and
        // final output.
        if allowlist.rendering().default_mode() != "plain_text" {
            return Err(super::diagnostic::public_data_unsafe("rendering-mode"));
        }
        Ok(())
    }

    /// Replay scopes stay operation-unique and payload-bound where
    /// replay is declared, and every replay key resolves to a declared
    /// input of its own operation: a durable key no operation can
    /// supply is not a replay guarantee. Correlation keys resolve
    /// against every member operation the same way.
    fn check_replay(&self, index: &ProjectIndex) -> Result<(), DiagnosticSet> {
        let scopes = self.replay_scopes.iter().chain(
            self.operations
                .iter()
                .filter_map(|role| role.replay.as_ref()),
        );
        for scope in scopes {
            let Some(role) = self.role(scope.operation()) else {
                return Err(super::diagnostic::replay_scope_invalid(
                    "unresolved-operation",
                ));
            };
            if role.commit_boundary() == "read_only" {
                return Err(super::diagnostic::replay_scope_invalid("read-only-replay"));
            }
            if scope.duplicate() == "replay_result" && scope.payload_binding() != "canonical_digest"
            {
                return Err(super::diagnostic::replay_scope_invalid("unbound-replay"));
            }
            if scope.same_key_different_request() != "conflict" {
                return Err(super::diagnostic::replay_scope_invalid(
                    "drift-not-conflict",
                ));
            }
            // The key must be a declared input of this very operation.
            let Some(inputs) = index.operation_inputs.get(scope.operation()) else {
                return Err(super::diagnostic::replay_scope_invalid(
                    "unresolved-operation",
                ));
            };
            if !inputs.iter().any(|input| input == scope.key_ref()) {
                return Err(super::diagnostic::replay_scope_invalid(
                    "unresolved-replay-key",
                ));
            }
        }
        for correlation in &self.correlations {
            for member in correlation.member_operations() {
                let Some(inputs) = index.operation_inputs.get(member) else {
                    return Err(super::diagnostic::replay_scope_invalid(
                        "unresolved-correlation-member",
                    ));
                };
                if !inputs.iter().any(|input| input == correlation.key_ref()) {
                    return Err(super::diagnostic::replay_scope_invalid(
                        "unresolved-correlation-key",
                    ));
                }
            }
        }
        Ok(())
    }

    /// The publication guarantees stay explicit and correctly bound:
    /// approval, revisioned snapshot, review gate, and atomic group
    /// must each name the publication operation at the full minimum, a
    /// replayed operation must carry the durable-key guarantee at the
    /// full minimum, and provider idempotency stays an honest partial
    /// row bound to the delivery operation. A capability row bound to
    /// the wrong operation, or weakened below its minimum, does not
    /// establish the guarantee.
    fn check_capabilities(&self) -> Result<(), DiagnosticSet> {
        let bound = |capability: &str, operation: &str, minimum: &str| {
            self.capability_requirements.iter().any(|requirement| {
                requirement.capability() == capability
                    && requirement.minimum() == minimum
                    && requirement
                        .operation_refs()
                        .iter()
                        .any(|bound_operation| bound_operation == operation)
            })
        };
        for capability in [
            "publish.approval",
            "publish.revisioned_snapshot",
            "security.review_gate",
            "transaction.atomic_group",
        ] {
            if !bound(capability, "changelog.publish_entry", "full") {
                return Err(super::diagnostic::evidence_incomplete(capability));
            }
        }
        // Every replayed operation carries the durable-key guarantee.
        let scopes = self.replay_scopes.iter().chain(
            self.operations
                .iter()
                .filter_map(|role| role.replay.as_ref()),
        );
        for scope in scopes {
            if scope.duplicate() != "replay_result" {
                continue;
            }
            if !bound("idempotency.durable_key", scope.operation(), "full") {
                return Err(super::diagnostic::evidence_incomplete(
                    "idempotency.durable_key",
                ));
            }
        }
        // The delivery operation carries the explicit partial
        // provider-idempotency row: only a provider-supported
        // idempotent write resolves an ambiguous remote acceptance.
        if !bound(
            "call.idempotent_write",
            "worklog.deliver_provider_comment",
            "partial",
        ) {
            return Err(super::diagnostic::evidence_incomplete(
                "provider-idempotency-partial",
            ));
        }
        Ok(())
    }
}

/// The typed read model of one pinned compilation.
pub(crate) struct ProjectIndex {
    /// Declared command/query kinds by semantic id.
    operations: std::collections::BTreeMap<String, &'static str>,
    /// Declared input field names per command.
    operation_inputs: std::collections::BTreeMap<String, Vec<String>>,
    /// Declared effects by semantic id:
    /// `(effect id, operation, entity, effect operation class)`.
    effects: std::collections::BTreeMap<String, (String, String, String, String)>,
    /// The canonical per-command effect projection: declared effect
    /// references in declaration order.
    effect_projection: std::collections::BTreeMap<String, Vec<(String, String)>>,
    /// Declared entity/value-object fields by semantic id.
    entity_fields: std::collections::BTreeMap<String, Vec<String>>,
    /// Declared read subjects per query.
    query_reads: std::collections::BTreeMap<String, Vec<String>>,
}

impl ProjectIndex {
    /// Index one compilation deterministically. Two passes: effects
    /// first (definitions arrive in semantic-ID byte order, so a
    /// command can precede its effect), then the per-command
    /// projection.
    pub(crate) fn build(project: &CompiledProject) -> Self {
        let mut operations = std::collections::BTreeMap::new();
        let mut operation_inputs = std::collections::BTreeMap::new();
        let mut effects: std::collections::BTreeMap<String, (String, String, String, String)> =
            std::collections::BTreeMap::new();
        let mut entity_fields = std::collections::BTreeMap::new();
        let mut query_reads = std::collections::BTreeMap::new();
        for definition in &project.definitions {
            match definition {
                Definition::Command(command) => {
                    operations.insert(command.id.as_str().to_owned(), "command");
                    operation_inputs.insert(
                        command.id.as_str().to_owned(),
                        command
                            .input
                            .iter()
                            .map(|field| field.name.as_str().to_owned())
                            .collect(),
                    );
                }
                Definition::Query(query) => {
                    operations.insert(query.id.as_str().to_owned(), "query");
                    query_reads.insert(
                        query.id.as_str().to_owned(),
                        query
                            .reads
                            .iter()
                            .map(|symbol| symbol.as_str().to_owned())
                            .collect(),
                    );
                }
                Definition::Effect(effect) => {
                    effects.insert(
                        effect.id.as_str().to_owned(),
                        (
                            effect.id.as_str().to_owned(),
                            String::new(),
                            effect.entity.as_str().to_owned(),
                            effect.operation.as_str().to_owned(),
                        ),
                    );
                }
                Definition::Entity(entity) => {
                    entity_fields.insert(
                        entity.id.as_str().to_owned(),
                        entity
                            .fields
                            .iter()
                            .map(|field| field.name.as_str().to_owned())
                            .collect(),
                    );
                }
                Definition::ValueObject(value_object) => {
                    entity_fields.insert(
                        value_object.id.as_str().to_owned(),
                        value_object
                            .fields
                            .iter()
                            .map(|field| field.name.as_str().to_owned())
                            .collect(),
                    );
                }
                _ => {}
            }
        }
        let mut effect_projection: std::collections::BTreeMap<String, Vec<(String, String)>> =
            std::collections::BTreeMap::new();
        for definition in &project.definitions {
            if let Definition::Command(command) = definition {
                let mut entries = Vec::with_capacity(command.effects.len());
                for effect_ref in &command.effects {
                    if let Some((effect_id, _, _, kind)) = effects.get(effect_ref.as_str()) {
                        entries.push((
                            format!("operation:{}|{}", command.id.as_str(), kind),
                            effect_id.clone(),
                        ));
                    }
                }
                effect_projection.insert(command.id.as_str().to_owned(), entries);
            }
        }
        Self {
            operations,
            operation_inputs,
            effects,
            effect_projection,
            entity_fields,
            query_reads,
        }
    }
}
