//! The typed inspect view of one task-links attachment (issue #52).
//!
//! The view is the read-only projection behind
//! `lekalo task-links inspect`: sorted relations, unique constraints,
//! decisions, bindings, and the admitted evidence summary. Missing
//! evidence stays explicitly unavailable; nothing here executes a
//! provider or fabricates coverage.

use serde::Serialize;

use super::wire::Attachment;

/// The typed inspect view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct View {
    /// The attachment identity.
    pub identity: &'static str,
    /// The attachment revision.
    pub attachment_revision: String,
    /// The bound project id.
    pub project_id: String,
    /// The Model/IR pin.
    pub ir_digest: String,
    /// Sorted relation rows.
    pub relations: Vec<RelationRow>,
    /// Sorted unique-constraint rows.
    pub unique_constraints: Vec<ConstraintRow>,
    /// Sorted decision rows (field and action policies).
    pub decisions: Vec<DecisionRow>,
    /// Sorted provider-contract rows.
    pub provider_contracts: Vec<ProviderRow>,
    /// Sorted operation-binding rows.
    pub bindings: Vec<BindingRow>,
    /// Sorted bound-entity rows.
    pub entities: Vec<EntityRow>,
    /// The delivery-rule summary.
    pub delivery: DeliveryRow,
    /// The declared contribution switches: the surfaces this
    /// integration feeds downstream, rendered so a disabled surface is
    /// visible and the CLI's own contribution gates stay auditable.
    pub contributions: ContributionsRow,
    /// The admitted evidence, when joined.
    pub evidence: Option<EvidenceSummary>,
}

/// One rendered contribution-switch row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContributionsRow {
    /// Graph export contribution.
    pub graph: bool,
    /// Reverse-impact contribution.
    pub impact: bool,
    /// Inspect-surface contribution.
    pub inspect: bool,
    /// Scenario-evidence contribution.
    pub scenarios: bool,
    /// Effect-trace contribution.
    pub effect_trace: bool,
}

/// One relation row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RelationRow {
    /// Relation id.
    pub id: String,
    /// Target entity.
    pub target: String,
    /// Cardinality spelling (`0..N`, `1..N`, `0..8`).
    pub cardinality: String,
    /// Ownership.
    pub ownership: String,
    /// Detach semantics.
    pub on_detach: String,
    /// Delete semantics.
    pub on_delete: String,
}

/// One unique-constraint row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConstraintRow {
    /// Constraint id.
    pub id: String,
    /// Constrained entity.
    pub entity: String,
    /// Ordered fields (the identity tuple).
    pub fields: Vec<String>,
    /// Comparison rule.
    pub comparison: String,
}

/// One decision row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DecisionRow {
    /// Policy id.
    pub policy_id: String,
    /// `field` or `action`.
    pub kind: &'static str,
    /// Direction (field policies) or `none` (action policies).
    pub direction: String,
    /// `allow` or `deny`.
    pub decision: String,
    /// Actor class.
    pub actor: String,
    /// Bound fields (field policies) or the bound action (action
    /// policies).
    pub subject: String,
    /// Policy revision pin.
    pub revision: String,
}

/// One provider-contract row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderRow {
    /// Contract id.
    pub contract_id: String,
    /// Provider id.
    pub provider_id: String,
    /// Object type.
    pub object_type: String,
    /// Upstream pin.
    pub upstream_pin: String,
    /// Revision ordering guarantee.
    pub revision_ordering: String,
    /// Remote dedup guarantee.
    pub remote_dedup: String,
}

/// One operation-binding row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BindingRow {
    /// Operation symbol.
    pub operation: String,
    /// Capability reference.
    pub capability_ref: String,
    /// Action policy reference.
    pub action_policy_ref: String,
    /// Effect reference.
    pub effect_ref: String,
    /// Retry guarantee.
    pub retry_guarantee: String,
}

/// One bound-entity row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EntityRow {
    /// Bound entity symbol.
    pub entity: String,
    /// Identity field names (ordered).
    pub identity: Vec<String>,
    /// Whether the entity carries the external identity tuple.
    pub external_identity: bool,
}

/// The delivery-rule summary row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeliveryRow {
    /// Outbox states in canonical order.
    pub outbox_states: Vec<String>,
    /// Bounded attempts.
    pub max_attempts: u64,
    /// Deterministic backoff (ms).
    pub backoff_ms: u64,
    /// Lease duration (ms).
    pub lease_ms: u64,
    /// Always false: no network inside a transaction.
    pub network_inside_transaction: bool,
    /// Always false: no implicit external delete.
    pub external_delete_implicit: bool,
    /// Acceptance-before-ack policy.
    pub uncertainty_policy: String,
}

/// The admitted evidence summary; `None` means unavailable.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvidenceSummary {
    /// Evidence id.
    pub evidence_id: String,
    /// Declared origin (`mock`/`reference`/`native`).
    pub origin: String,
    /// Scenario id.
    pub scenario_id: String,
    /// Step count.
    pub steps: u64,
    /// Owner-attested executed commands. File-origin inputs have no
    /// authenticated execution channel, so this set is always empty;
    /// admitted shape and matching source pins do not establish execution.
    pub executed: std::collections::BTreeSet<String>,
    /// Whether the evidence pins match the attachment (always true for
    /// admitted evidence).
    pub pins_match: bool,
}

/// Whether `symbol` names a row of the view: a relation, unique
/// constraint, decision, provider contract, binding, or a bound
/// entity/project id. `inspect SYMBOL` resolves its operand before
/// anything renders; an unknown symbol is a typed refusal.
pub fn view_contains_symbol(view: &View, symbol: &str) -> bool {
    if view.project_id == symbol || view.identity == symbol {
        return true;
    }
    if view.relations.iter().any(|row| row.id == symbol)
        || view.unique_constraints.iter().any(|row| row.id == symbol)
        || view.decisions.iter().any(|row| row.policy_id == symbol)
        || view
            .provider_contracts
            .iter()
            .any(|row| row.contract_id == symbol)
        || view.bindings.iter().any(|row| row.operation == symbol)
        || view.entities.iter().any(|row| row.entity == symbol)
    {
        return true;
    }
    // Bound entity identity fields resolve too (`tasklinks.task_external_link`
    // and its tuple members), and effect/capability references resolve
    // through the binding rows.
    view.entities
        .iter()
        .any(|row| row.identity.iter().any(|field| field == symbol))
        || view.bindings.iter().any(|row| row.effect_ref == symbol)
        || view.bindings.iter().any(|row| row.capability_ref == symbol)
        // Decision rows carry their bound action as the subject; an
        // action symbol resolves through it. Field rows carry a field list.
        || view.decisions.iter().any(|row| row.subject == symbol)
        || view
            .decisions
            .iter()
            .any(|row| row.subject.split(',').any(|field| field == symbol))
}

/// Project one attachment (and optional admitted evidence) into the
/// inspect view.
pub fn view(attachment: &Attachment, evidence: Option<EvidenceSummary>) -> View {
    let relations = attachment
        .relations()
        .iter()
        .map(|relation| RelationRow {
            id: relation.id.clone(),
            target: relation.target.clone(),
            cardinality: format!(
                "{}..{}",
                relation.min,
                relation
                    .max
                    .map_or_else(|| "N".to_owned(), |max| max.to_string())
            ),
            ownership: relation.ownership.clone(),
            on_detach: relation.on_detach.clone(),
            on_delete: relation.on_delete.clone(),
        })
        .collect();
    let unique_constraints = attachment
        .unique_constraints()
        .iter()
        .map(|constraint| ConstraintRow {
            id: constraint.id.clone(),
            entity: constraint.entity.clone(),
            fields: constraint.fields.clone(),
            comparison: constraint.comparison.clone(),
        })
        .collect();
    let mut decisions: Vec<DecisionRow> = Vec::new();
    for policy in attachment.field_policies() {
        decisions.push(DecisionRow {
            policy_id: policy.policy_id.clone(),
            kind: "field",
            direction: policy.direction.clone(),
            decision: policy.decision.clone(),
            actor: policy.actor.clone(),
            subject: policy.fields.join(","),
            revision: policy.revision.as_str().to_owned(),
        });
    }
    for policy in attachment.action_policies() {
        decisions.push(DecisionRow {
            policy_id: policy.policy_id.clone(),
            kind: "action",
            direction: "none".to_owned(),
            decision: policy.decision.clone(),
            actor: policy.actor.clone(),
            subject: policy.action.clone(),
            revision: policy.revision.as_str().to_owned(),
        });
    }
    decisions.sort_by(|left, right| {
        (left.kind, left.policy_id.as_str()).cmp(&(right.kind, right.policy_id.as_str()))
    });
    let provider_contracts = attachment
        .provider_contracts()
        .iter()
        .map(|contract| ProviderRow {
            contract_id: contract.contract_id.clone(),
            provider_id: contract.provider_id.clone(),
            object_type: contract.object_type.clone(),
            upstream_pin: contract.upstream_pin.clone(),
            revision_ordering: contract.revision_ordering.clone(),
            remote_dedup: contract.remote_dedup.clone(),
        })
        .collect();
    let bindings = attachment
        .operation_bindings()
        .iter()
        .map(|binding| BindingRow {
            operation: binding.operation.clone(),
            capability_ref: binding.capability_ref.clone(),
            action_policy_ref: binding.action_policy_ref.clone(),
            effect_ref: binding.effect_ref.clone(),
            retry_guarantee: binding.retry_guarantee.clone(),
        })
        .collect();
    let entities = attachment
        .entities()
        .iter()
        .map(|entity| EntityRow {
            entity: entity.entity.clone(),
            identity: entity.identity.clone(),
            external_identity: entity.external_identity,
        })
        .collect();
    let rules = attachment.delivery_rules();
    let delivery = DeliveryRow {
        outbox_states: rules.outbox_states.clone(),
        max_attempts: rules.max_attempts,
        backoff_ms: rules.backoff_ms,
        lease_ms: rules.lease_ms,
        network_inside_transaction: rules.network_inside_transaction,
        external_delete_implicit: rules.external_delete_implicit,
        uncertainty_policy: rules.uncertainty_policy.clone(),
    };
    let contributions = attachment.contributions();
    let contributions = ContributionsRow {
        graph: contributions.graph,
        impact: contributions.impact,
        inspect: contributions.inspect,
        scenarios: contributions.scenarios,
        effect_trace: contributions.effect_trace,
    };
    View {
        identity: super::version::IDENTITY,
        attachment_revision: attachment.attachment_revision().as_str().to_owned(),
        project_id: attachment.project_id().to_owned(),
        ir_digest: attachment.ir_digest().as_str().to_owned(),
        relations,
        unique_constraints,
        decisions,
        provider_contracts,
        bindings,
        entities,
        delivery,
        contributions,
        evidence,
    }
}
