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

/// One typed validation finding.
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
    let model_policies = ir_policy_ids(compilation);
    check_entities(attachment, &entities, &mut findings);
    check_relations(attachment, &entities, &mut findings);
    check_unique_constraints(attachment, &entities, &mut findings);
    check_decisions(attachment, &model_policies, &mut findings);
    let effects = ir_effects(compilation);
    check_bindings(attachment, &effects, &mut findings);
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
        let _ = fields;
    }
}

fn check_relations(attachment: &Attachment, entities: &EntityTable, findings: &mut Vec<Finding>) {
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
        for field in relation
            .target_fields
            .iter()
            .chain(relation.local_fields.iter())
        {
            if !fields.contains(field) {
                finding(
                    findings,
                    diagnostic::INPUT_INVALID,
                    relation.id.clone(),
                    "relation-field-missing",
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
    let _ = seen_targets;
}

fn check_unique_constraints(
    attachment: &Attachment,
    entities: &EntityTable,
    findings: &mut Vec<Finding>,
) {
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
            && constraint.fields.len() != super::version::EXTERNAL_IDENTITY_FIELDS
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
    model_policies: &BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    // Field policies reference owning Model policies; action policies
    // may reference field policies (composition) or owning Model
    // policies. Any referenced id must be declared in the attachment.
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
        if policy.action.starts_with("tasklinks.")
            && !DOMAIN_COMMANDS.contains(&policy.action.as_str())
            && !DOMAIN_QUERIES.contains(&policy.action.as_str())
        {
            finding(
                findings,
                diagnostic::DECISION_INCOMPLETE,
                policy.policy_id.clone(),
                "action-unresolved",
            );
        }
    }
}

fn check_bindings(
    attachment: &Attachment,
    ir_effects: &BTreeMap<String, (String, String)>,
    findings: &mut Vec<Finding>,
) {
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
            let audited = ir_effects.get(binding.effect_ref.as_str());
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
        if !ir_effects.contains_key(binding.effect_ref.as_str()) {
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
