//! Static validation of a task-links attachment against one compiled
//! project (issue #52).
//!
//! Checks are closed and deterministic: relation cardinality and field
//! existence against the typed IR entities, exact external-identity
//! uniqueness, installation scope references, decision completeness
//! (actor, policy, capability, revision pins), forbidden planning-field
//! protection, operation binding joins, and delivery-rule boundaries.
//! Every finding is one registered `tasklink.*` diagnostic; the report
//! is derived output with sorted findings and exact counts.

use crate::ir::{Compilation, Definition};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    diagnostic::{self, Failure},
    wire::Attachment,
};

/// The protected planner fields no inbound write may ever touch.
pub const PROTECTED_PLANNING_FIELDS: [&str; 4] =
    ["planned_for", "position", "focused_at", "paused_at"];

/// The closed set of domain commands the pilot declares.
pub const DOMAIN_COMMANDS: [&str; 8] = [
    "tasklinks.attach_link",
    "tasklinks.import_link",
    "tasklinks.ingest_event",
    "tasklinks.request_action",
    "tasklinks.detach_link",
    "tasklinks.resolve_conflict",
    "tasklinks.dispatch_outbox",
    "tasklinks.ack_delivery",
];

/// The closed query set.
pub const DOMAIN_QUERIES: [&str; 3] = [
    "tasklinks.list_links",
    "tasklinks.list_conflicts",
    "tasklinks.delivery_status",
];

/// The exact ordered external-identity tuple the pilot's uniqueness
/// guarantee is defined over (scope
/// `workspace-installation-provider-object-external-id`).
pub const EXTERNAL_IDENTITY_TUPLE: [&str; 5] = [
    "workspace_id",
    "installation_id",
    "provider_id",
    "object_type",
    "external_id",
];

/// The typed validation finding.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
pub struct Finding {
    /// The registered rule id.
    pub rule: &'static str,
    /// The immutable diagnostic code.
    pub code: &'static str,
    /// The bounded subject symbol.
    pub subject: String,
    /// The fixed detail tag.
    pub detail: &'static str,
}

/// The immutable `LEK-TASKLINK` code of one registered rule.
fn code_for_rule(rule: &str) -> &'static str {
    match rule {
        diagnostic::INPUT_INVALID => "LEK-TASKLINK-001",
        diagnostic::DECISION_INCOMPLETE => "LEK-TASKLINK-002",
        diagnostic::FANOUT_NOT_ADMITTED => "LEK-TASKLINK-003",
        diagnostic::DELIVERY_CONTRACT_INVALID => "LEK-TASKLINK-004",
        diagnostic::CONFLICT_RESOLUTION_INVALID => "LEK-TASKLINK-005",
        diagnostic::HISTORY_OR_OWNERSHIP_VIOLATION => "LEK-TASKLINK-006",
        diagnostic::REQUIRED_GUARANTEE_UNSUPPORTED => "LEK-TASKLINK-007",
        diagnostic::EVIDENCE_INVALID => "LEK-TASKLINK-008",
        _ => "LEK-TASKLINK-001",
    }
}

/// Record one finding with its registered code.
fn finding(
    findings: &mut Vec<Finding>,
    rule: &'static str,
    subject: impl Into<String>,
    detail: &'static str,
) {
    findings.push(Finding {
        rule,
        code: code_for_rule(rule),
        subject: subject.into(),
        detail,
    });
}

/// The derived report of one validation run.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The exact report discriminator.
    pub schema_version: &'static str,
    /// The exact report identity.
    pub identity: &'static str,
    /// The exact attachment revision validated.
    pub attachment_revision: String,
    /// The bound project id.
    pub project_id: String,
    /// The Model/IR pin the attachment was joined against.
    pub model_ref: ReportModelRef,
    /// `strict` or `permissive`.
    pub profile: String,
    /// `valid`, `invalid`, or `gaps-admitted` (permissive only).
    pub status: String,
    /// Sorted findings.
    pub findings: Vec<Finding>,
    /// Exact checked-object counts.
    pub checks: Checks,
}

/// The Model/IR pin block of one report.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportModelRef {
    /// Source Model version.
    pub schema_version: &'static str,
    /// Canonical IR digest.
    pub ir_digest: String,
}

/// The exact counts block of one report.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checks {
    /// Relations checked against the compilation.
    pub relations_checked: u64,
    /// Unique constraints checked.
    pub unique_constraints_checked: u64,
    /// Field and action decisions checked.
    pub decisions_checked: u64,
    /// Operation bindings checked.
    pub bindings_checked: u64,
    /// Provider contracts checked.
    pub provider_contracts_checked: u64,
    /// Bound entities checked.
    pub entities_checked: u64,
    /// Total findings.
    pub findings: u64,
}

/// Validate one attachment against the compilation under `profile`
/// (`strict` denies missing/unsupported required semantics; `permissive`
/// reports gaps without claiming pass).
pub fn validate(
    attachment: &Attachment,
    compilation: &Compilation,
    profile: &str,
) -> Result<Report, Failure> {
    if profile != "strict" && profile != "permissive" {
        return Err(diagnostic::input_invalid("profile"));
    }
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned());
    let Some(project_id) = project_id else {
        return Err(diagnostic::input_invalid("project-missing"));
    };
    if attachment.project_id() != project_id {
        return Err(diagnostic::input_invalid("project-mismatch"));
    }
    let mut findings: Vec<Finding> = Vec::new();
    let entities = ir_entities(compilation);
    check_entities(attachment, &entities, &mut findings);
    check_relations(attachment, &entities, &mut findings);
    check_unique_constraints(attachment, &entities, &mut findings);
    check_decisions(attachment, compilation, &entities, &mut findings);
    check_bindings(attachment, compilation, &mut findings);
    check_delivery(attachment, &mut findings);
    findings.sort();
    findings.dedup();
    let checks = Checks {
        relations_checked: attachment.relations().len() as u64,
        unique_constraints_checked: attachment.unique_constraints().len() as u64,
        decisions_checked: (attachment.field_policies().len() + attachment.action_policies().len())
            as u64,
        bindings_checked: attachment.operation_bindings().len() as u64,
        provider_contracts_checked: attachment.provider_contracts().len() as u64,
        entities_checked: attachment.entities().len() as u64,
        findings: findings.len() as u64,
    };
    let status = if findings.is_empty() {
        "valid"
    } else if profile == "permissive" && !findings.iter().any(|f| blocking(f.rule)) {
        "gaps-admitted"
    } else {
        "invalid"
    };
    Ok(Report {
        schema_version: super::version::REPORT_SCHEMA_VERSION,
        identity: super::version::REPORT_IDENTITY,
        attachment_revision: attachment.attachment_revision().as_str().to_owned(),
        project_id,
        model_ref: ReportModelRef {
            schema_version: "0.2.16",
            ir_digest: attachment.ir_digest().as_str().to_owned(),
        },
        profile: profile.to_owned(),
        status: status.to_owned(),
        findings,
        checks,
    })
}

/// Whether a finding blocks under every profile (errors do; the
/// unsupported-guarantee rule stays profile-admitted in permissive).
fn blocking(rule: &str) -> bool {
    rule != diagnostic::REQUIRED_GUARANTEE_UNSUPPORTED
}

/// Whether the finding set invalidates the run under `profile`.
pub fn invalidates(report: &Report) -> bool {
    report.status == "invalid"
}

fn ir_entities(compilation: &Compilation) -> BTreeMap<String, (Vec<String>, Vec<String>)> {
    let mut entities = BTreeMap::new();
    for definition in &compilation.project.definitions {
        if let Definition::Entity(entity) = &definition {
            let fields: Vec<String> = entity
                .fields
                .iter()
                .map(|field| field.name.as_str().to_owned())
                .collect();
            let identity: Vec<String> = entity
                .identity
                .iter()
                .map(|member| member.as_str().to_owned())
                .collect();
            entities.insert(entity.id.as_str().to_owned(), (fields, identity));
        }
    }
    entities
}

type EntityTable = BTreeMap<String, (Vec<String>, Vec<String>)>;

/// The declared Model policy ids of the compilation.
fn ir_policy_ids(compilation: &Compilation) -> BTreeSet<String> {
    compilation
        .project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Policy(policy) => Some(policy.id.as_str().to_owned()),
            _ => None,
        })
        .collect()
}

fn check_entities(attachment: &Attachment, entities: &EntityTable, findings: &mut Vec<Finding>) {
    for binding in attachment.entities() {
        let Some((fields, identity)) = entities.get(&binding.entity) else {
            finding(
                findings,
                diagnostic::INPUT_INVALID,
                binding.entity.clone(),
                "entity-unresolved",
            );
            continue;
        };
        if binding.identity.is_empty() {
            finding(
                findings,
                diagnostic::INPUT_INVALID,
                binding.entity.clone(),
                "identity-member-missing",
            );
        }
        for member in &binding.identity {
            if !identity.contains(member) {
                finding(
                    findings,
                    diagnostic::INPUT_INVALID,
                    binding.entity.clone(),
                    "identity-member-missing",
                );
            }
        }
        // `externalIdentity` is a typed claim, not prose: only the link
        // aggregate carries it, it must actually be bound in the
        // attachment, and the link entity must carry every tuple field.
        if binding.external_identity {
            if !is_link_entity(binding.entity.as_str()) {
                finding(
                    findings,
                    diagnostic::INPUT_INVALID,
                    binding.entity.clone(),
                    "external-identity-entity",
                );
            }
            for field in EXTERNAL_IDENTITY_TUPLE {
                if !fields.contains(&field.to_owned()) {
                    finding(
                        findings,
                        diagnostic::INPUT_INVALID,
                        binding.entity.clone(),
                        "identity-tuple-incomplete",
                    );
                }
            }
        }
        let _ = fields;
    }
    // The attachment must bind exactly the link aggregate; the identity
    // semantics hang off it. A missing binding leaves the uniqueness
    // guarantee unanchored.
    if !attachment
        .entities()
        .iter()
        .any(|binding| is_link_entity(binding.entity.as_str()) && binding.external_identity)
    {
        finding(
            findings,
            diagnostic::INPUT_INVALID,
            "tasklinks.task_external_link".to_owned(),
            "external-identity-unbound",
        );
    }
}

/// The declaring link aggregate of the pilot (`tasklinks.task_external_link`).
fn is_link_entity(entity: &str) -> bool {
    entity == "tasklinks.task_external_link"
}

fn check_relations(attachment: &Attachment, entities: &EntityTable, findings: &mut Vec<Finding>) {
    // The relation's local entity is the declaring link aggregate; every
    // `localFields` entry must exist on that entity, while `targetFields`
    // are checked against the relation target.
    let local_entity = attachment
        .entities()
        .iter()
        .map(|binding| binding.entity.as_str())
        .find(|entity| is_link_entity(entity))
        .unwrap_or("tasklinks.task_external_link");
    let local_fields_table = entities.get(local_entity);
    let mut seen_targets: BTreeSet<&str> = BTreeSet::new();
    for relation in attachment.relations() {
        let Some((fields, _)) = entities.get(&relation.target) else {
            finding(
                findings,
                diagnostic::INPUT_INVALID,
                relation.id.clone(),
                "relation-target-unresolved",
            );
            continue;
        };
        for field in &relation.target_fields {
            if !fields.contains(field) {
                finding(
                    findings,
                    diagnostic::INPUT_INVALID,
                    relation.id.clone(),
                    "relation-field-missing",
                );
            }
        }
        for field in &relation.local_fields {
            let present = local_fields_table
                .map(|(fields, _)| fields.contains(field))
                .unwrap_or(false);
            if !present {
                finding(
                    findings,
                    diagnostic::INPUT_INVALID,
                    relation.id.clone(),
                    "relation-local-field-missing",
                );
            }
        }
        if relation.on_detach != "retain-history" && relation.on_detach != "forbid" {
            finding(
                findings,
                diagnostic::HISTORY_OR_OWNERSHIP_VIOLATION,
                relation.id.clone(),
                "detach-not-history-preserving",
            );
        }
        if relation.max.is_some() && relation.max == Some(0) && relation.min > 0 {
            finding(
                findings,
                diagnostic::INPUT_INVALID,
                relation.id.clone(),
                "cardinality-inconsistent",
            );
        }
        if relation.ownership == "external" && relation.on_delete == "detach" {
            // An externally owned object is a reference, never a
            // cascade-owned record: deleting the local row must not
            // reach the provider.
            finding(
                findings,
                diagnostic::HISTORY_OR_OWNERSHIP_VIOLATION,
                relation.id.clone(),
                "external-cascade-forbidden",
            );
        }
        seen_targets.insert(relation.target.as_str());
    }
    // The cardinality declarations must cover both directions of the
    // pilot's aggregate: Task -> links (`0..N`) and the link child
    // scope. An unconstrained target set leaves the relation semantics
    // declared nowhere.
    let link_entity = "tasklinks.task_external_link";
    if !seen_targets.contains(link_entity) {
        finding(
            findings,
            diagnostic::INPUT_INVALID,
            "tasklinks.rel.task_links".to_owned(),
            "relation-coverage-missing",
        );
    }
}

fn check_unique_constraints(
    attachment: &Attachment,
    entities: &EntityTable,
    findings: &mut Vec<Finding>,
) {
    // The external-identity constraint is the pilot's core identity
    // guarantee, so it must exist exactly: one constraint on the link
    // entity, over the exact ordered tuple, under the exact scope. Any
    // other constraint row is still shape- and field-checked below.
    let exact_present = attachment.unique_constraints().iter().any(|constraint| {
        constraint.entity == "tasklinks.task_external_link"
            && constraint.scope == "workspace-installation-provider-object-external-id"
            && constraint.fields.len() == EXTERNAL_IDENTITY_TUPLE.len()
            && constraint
                .fields
                .iter()
                .zip(EXTERNAL_IDENTITY_TUPLE.iter())
                .all(|(field, expected)| field == expected)
    });
    if !exact_present {
        finding(
            findings,
            diagnostic::INPUT_INVALID,
            "tasklinks.uq_external_identity".to_owned(),
            "identity-tuple-missing",
        );
    }
    for constraint in attachment.unique_constraints() {
        let Some((fields, _)) = entities.get(&constraint.entity) else {
            finding(
                findings,
                diagnostic::INPUT_INVALID,
                constraint.id.clone(),
                "constraint-entity-unresolved",
            );
            continue;
        };
        for field in &constraint.fields {
            if !fields.contains(field) {
                finding(
                    findings,
                    diagnostic::INPUT_INVALID,
                    constraint.id.clone(),
                    "constraint-field-missing",
                );
            }
        }
        if constraint.scope == "workspace-installation-provider-object-external-id"
            && (constraint.fields.len() != EXTERNAL_IDENTITY_TUPLE.len()
                || !constraint
                    .fields
                    .iter()
                    .zip(EXTERNAL_IDENTITY_TUPLE.iter())
                    .all(|(field, expected)| field == expected))
        {
            finding(
                findings,
                diagnostic::INPUT_INVALID,
                constraint.id.clone(),
                "identity-tuple-incomplete",
            );
        }
        if constraint.comparison == "declared-normalization"
            && constraint.normalization_ref.is_none()
        {
            finding(
                findings,
                diagnostic::INPUT_INVALID,
                constraint.id.clone(),
                "normalization-unpinned",
            );
        }
    }
}

fn check_decisions(
    attachment: &Attachment,
    compilation: &Compilation,
    entities: &EntityTable,
    findings: &mut Vec<Finding>,
) {
    // Field policies reference owning Model policies; action policies
    // may reference field policies (composition) or owning Model
    // policies. Any referenced id must be declared in the attachment,
    // and the composition graph must be acyclic.
    let model_policies = ir_policy_ids(compilation);
    let declared_commands: BTreeSet<&str> = compilation
        .project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Command(_) | Definition::Query(_) => Some(definition.id().as_str()),
            _ => None,
        })
        .collect();
    let policy_ids: BTreeSet<&str> = attachment
        .field_policies()
        .iter()
        .map(|policy| policy.policy_id.as_str())
        .chain(
            attachment
                .action_policies()
                .iter()
                .map(|policy| policy.policy_id.as_str()),
        )
        .collect();
    let referenced_ok = |policy_ref: &str| {
        // A Model-owner policy reference must resolve to a declared
        // Model policy of the compilation; an in-attachment policy id
        // resolves through composition.
        model_policies.contains(policy_ref) || policy_ids.contains(policy_ref)
    };
    for policy in attachment.field_policies() {
        if policy.policy_ref.is_empty() || !referenced_ok(&policy.policy_ref) {
            finding(
                findings,
                diagnostic::DECISION_INCOMPLETE,
                policy.policy_id.clone(),
                "policy-ref-unresolved",
            );
        }
        // The bound entity and every bound field must exist in the
        // compiled Model: a policy graph that asserts authority over a
        // ghost entity or ghost fields is incomplete.
        let bound_fields = match entities.get(&policy.entity) {
            Some((fields, _)) => Some(fields),
            None => {
                finding(
                    findings,
                    diagnostic::DECISION_INCOMPLETE,
                    policy.policy_id.clone(),
                    "policy-entity-unresolved",
                );
                None
            }
        };
        if let Some(fields) = bound_fields {
            for field in &policy.fields {
                if !fields.contains(field) {
                    finding(
                        findings,
                        diagnostic::DECISION_INCOMPLETE,
                        policy.policy_id.clone(),
                        "policy-field-missing",
                    );
                }
            }
        }
        if policy.direction == "inbound"
            && policy.decision == "allow"
            && policy
                .fields
                .iter()
                .any(|field| PROTECTED_PLANNING_FIELDS.contains(&field.as_str()))
        {
            finding(
                findings,
                diagnostic::HISTORY_OR_OWNERSHIP_VIOLATION,
                policy.policy_id.clone(),
                "protected-planning-field",
            );
        }
    }
    for policy in attachment.action_policies() {
        if policy.policy_ref.is_empty() || !referenced_ok(&policy.policy_ref) {
            finding(
                findings,
                diagnostic::DECISION_INCOMPLETE,
                policy.policy_id.clone(),
                "policy-ref-unresolved",
            );
        }
        // The bound action must be a declared command or query of the
        // compilation, whatever its prefix: caller-named actions
        // resolve exactly like the built-in ones.
        if !declared_commands.contains(policy.action.as_str()) {
            finding(
                findings,
                diagnostic::DECISION_INCOMPLETE,
                policy.policy_id.clone(),
                "action-unresolved",
            );
        }
    }
    // The in-attachment policy composition graph must be acyclic: a
    // self-cycle or a mutual cycle composes authority from itself.
    let mut edges: BTreeMap<&str, &str> = BTreeMap::new();
    for policy in attachment.field_policies() {
        if policy_ids.contains(policy.policy_ref.as_str()) {
            edges.insert(policy.policy_id.as_str(), policy.policy_ref.as_str());
        }
    }
    for policy in attachment.action_policies() {
        if policy_ids.contains(policy.policy_ref.as_str()) {
            edges.insert(policy.policy_id.as_str(), policy.policy_ref.as_str());
        }
    }
    // Iterative depth-first search with the classic three-color mark.
    const WHITE: u8 = 0;
    const GRAY: u8 = 1;
    const BLACK: u8 = 2;
    let mut color: BTreeMap<&str, u8> = policy_ids.iter().map(|id| (*id, WHITE)).collect();
    for root in policy_ids.iter().copied() {
        if color.get(root).copied().unwrap_or(BLACK) != WHITE {
            continue;
        }
        let mut stack: Vec<(&str, bool)> = vec![(root, false)];
        while let Some((node, leaving)) = stack.pop() {
            if leaving {
                color.insert(node, BLACK);
                continue;
            }
            match color.get(node).copied().unwrap_or(BLACK) {
                GRAY => continue,
                BLACK => continue,
                _ => color.insert(node, GRAY),
            };
            stack.push((node, true));
            if let Some(next) = edges.get(node) {
                match color.get(next).copied().unwrap_or(BLACK) {
                    GRAY => {
                        finding(
                            findings,
                            diagnostic::DECISION_INCOMPLETE,
                            (*node).to_owned(),
                            "policy-ref-cycle",
                        );
                    }
                    WHITE => stack.push((next, false)),
                    _ => {}
                }
            }
        }
    }
    check_bindings(attachment, compilation, findings);
    let _ = entities;
}

fn check_bindings(attachment: &Attachment, compilation: &Compilation, findings: &mut Vec<Finding>) {
    let effects = ir_effects(compilation);
    let declared_commands: BTreeSet<&str> = compilation
        .project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Command(_) | Definition::Query(_) => Some(definition.id().as_str()),
            _ => None,
        })
        .collect();
    let declared_symbols: BTreeSet<&str> = compilation
        .project
        .definitions
        .iter()
        .map(|definition| definition.id().as_str())
        .collect();
    let capabilities: BTreeSet<&str> = attachment
        .capability_requirements()
        .iter()
        .map(|capability| capability.capability_id.as_str())
        .collect();
    let contracts: BTreeSet<&str> = attachment
        .provider_contracts()
        .iter()
        .map(|contract| contract.contract_id.as_str())
        .collect();
    for capability in attachment.capability_requirements() {
        if !contracts.contains(capability.contract_id.as_str()) {
            finding(
                findings,
                diagnostic::DECISION_INCOMPLETE,
                capability.capability_id.clone(),
                "capability-contract-unresolved",
            );
        }
    }
    // Provider contract wire schemas are typed resolved dependency
    // nodes: every request/response/error ref must name a declared
    // definition of the compilation, never a free symbol.
    for contract in attachment.provider_contracts() {
        for schema_ref in [
            &contract.request_schema_ref,
            &contract.response_schema_ref,
            &contract.error_schema_ref,
        ] {
            if !declared_symbols.contains(schema_ref.as_str()) {
                finding(
                    findings,
                    diagnostic::INPUT_INVALID,
                    contract.contract_id.clone(),
                    "contract-schema-ref-unresolved",
                );
            }
        }
    }
    let policies: BTreeSet<&str> = attachment
        .action_policies()
        .iter()
        .map(|policy| policy.policy_id.as_str())
        .collect();
    for binding in attachment.operation_bindings() {
        // Conflict resolution is audited by construction: its bound
        // effect must be a declared update on the conflict entity (a
        // valid, immutable transition), not an unrelated write.
        if binding.action == "tasklinks.resolve_conflict" {
            let audited = effects.get(binding.effect_ref.as_str());
            let valid = matches!(
                audited,
                Some((entity, operation)) if entity == "tasklinks.conflict" && operation == "update"
            );
            if !valid {
                finding(
                    findings,
                    diagnostic::CONFLICT_RESOLUTION_INVALID,
                    binding.operation.clone(),
                    "resolution-transition-invalid",
                );
            }
        }
        // The bound operation and action must both be declared
        // commands of the compilation: an operation binding that binds
        // a ghost operation asserts delivery of nothing.
        if !declared_commands.contains(binding.operation.as_str())
            || !declared_commands.contains(binding.action.as_str())
        {
            finding(
                findings,
                diagnostic::DECISION_INCOMPLETE,
                binding.operation.clone(),
                "binding-action-unresolved",
            );
        }
        if !capabilities.contains(binding.capability_ref.as_str()) {
            finding(
                findings,
                diagnostic::DECISION_INCOMPLETE,
                binding.operation.clone(),
                "binding-capability-unresolved",
            );
        }
        if !policies.contains(binding.action_policy_ref.as_str()) {
            finding(
                findings,
                diagnostic::DECISION_INCOMPLETE,
                binding.operation.clone(),
                "binding-policy-unresolved",
            );
        }
        if !effects.contains_key(binding.effect_ref.as_str()) {
            finding(
                findings,
                diagnostic::FANOUT_NOT_ADMITTED,
                binding.operation.clone(),
                "binding-effect-unresolved",
            );
        }
        if binding.retry_guarantee == "none-declared" {
            finding(
                findings,
                diagnostic::DELIVERY_CONTRACT_INVALID,
                binding.operation.clone(),
                "retry-guarantee-missing",
            );
        }
    }
}

/// The declared Model effects of the compilation: id -> (entity, operation).
fn ir_effects(compilation: &Compilation) -> BTreeMap<String, (String, String)> {
    compilation
        .project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Effect(effect) => Some((
                effect.id.as_str().to_owned(),
                (
                    effect.entity.as_str().to_owned(),
                    effect.operation.as_str().to_owned(),
                ),
            )),
            _ => None,
        })
        .collect()
}

fn check_delivery(attachment: &Attachment, findings: &mut Vec<Finding>) {
    let rules = attachment.delivery_rules();
    if rules.network_inside_transaction {
        finding(
            findings,
            diagnostic::DELIVERY_CONTRACT_INVALID,
            "deliveryRules".to_owned(),
            "network-inside-transaction",
        );
    }
    if rules.external_delete_implicit {
        finding(
            findings,
            diagnostic::HISTORY_OR_OWNERSHIP_VIOLATION,
            "deliveryRules".to_owned(),
            "implicit-external-delete",
        );
    }
}
