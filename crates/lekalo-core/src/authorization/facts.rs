//! Typed authorization contribution facts for the downstream semantic
//! seams (issue #25).
//!
//! #25 owns the semantics, not the seams: the dependency graph (#13)
//! grows its `authorizes` edges already, and impact (#16) and diff
//! (#18) consume their own projections. This module supplies the typed
//! facts those seams consume — deterministic, sorted, and free of any
//! target-specific naming. Nothing here executes or mutates another
//! module.

use super::document::{Decision, Document, MappingState, ScopeDimension};
use serde_json::json;
use serde_json::Value as Json;

/// One `authorizes` fact: a policy covering one operation for one actor
/// type with one decision and its error contract reference.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct AuthorizesFact {
    /// The policy identity.
    pub policy: String,
    /// The covered operation.
    pub operation: String,
    /// The actor type wire spelling.
    pub actor: &'static str,
    /// The decision wire spelling.
    pub decision: &'static str,
    /// The error contract reference.
    pub error_ref: String,
}

/// The closed change labels an authorization diff can report.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ChangeLabel {
    /// A policy was added.
    PolicyAdded,
    /// A policy was removed.
    PolicyRemoved,
    /// A policy body changed without a closed sub-label.
    PolicyChanged,
    /// The actor broadened (for example a public actor appeared, or a
    /// policy switched to a less authenticated actor).
    ActorBroadened,
    /// The actor narrowed (mandatory authentication added).
    ActorNarrowed,
    /// A scope binding was removed or loosened.
    ScopeWeakened,
    /// A scope binding was added or tightened.
    ScopeTightened,
    /// Required capabilities changed.
    CapabilityChanged,
    /// Required roles changed.
    RoleChanged,
    /// Ownership predicates changed.
    OwnershipChanged,
    /// Field permissions changed.
    FieldPermissionChanged,
    /// A deny policy was removed.
    DenyRemoved,
    /// A deny policy was added.
    DenyAdded,
    /// The error contract reference changed.
    ErrorRefChanged,
    /// Adapter mapping evidence changed state.
    MappingStateChanged,
    /// A composition changed.
    CompositionChanged,
}

impl ChangeLabel {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PolicyAdded => "policy-added",
            Self::PolicyRemoved => "policy-removed",
            Self::PolicyChanged => "policy-changed",
            Self::ActorBroadened => "actor-broadened",
            Self::ActorNarrowed => "actor-narrowed",
            Self::ScopeWeakened => "scope-weakened",
            Self::ScopeTightened => "scope-tightened",
            Self::CapabilityChanged => "capability-changed",
            Self::RoleChanged => "role-changed",
            Self::OwnershipChanged => "ownership-changed",
            Self::FieldPermissionChanged => "field-permission-changed",
            Self::DenyRemoved => "deny-removed",
            Self::DenyAdded => "deny-added",
            Self::ErrorRefChanged => "error-ref-changed",
            Self::MappingStateChanged => "mapping-state-changed",
            Self::CompositionChanged => "composition-changed",
        }
    }
}

/// The closed compatibility severity of an authorization change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Breaking {
    /// New allow surface, public exposure, weaker scope, or deny
    /// removal: security-breaking by definition.
    Security,
    /// Mandatory authentication added or write surface narrowed:
    /// behavioral and source-visible.
    Behavioral,
    /// Cosmetic reordering or equivalent restructuring.
    Cosmetic,
    /// The comparison cannot decide.
    Unknown,
}

impl Breaking {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Security => "security-breaking",
            Self::Behavioral => "behavioral",
            Self::Cosmetic => "cosmetic",
            Self::Unknown => "unknown",
        }
    }
}

/// One classified policy change between two documents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyChange {
    /// The changed policy id.
    pub policy: String,
    /// The sorted closed labels.
    pub labels: Vec<ChangeLabel>,
    /// The dominant severity.
    pub breaking: Breaking,
}

/// The sorted `authorizes` facts of one document: the deterministic
/// contribution the dependency graph consumes.
pub fn authorizes_facts(document: &Document) -> Vec<AuthorizesFact> {
    let mut facts = Vec::new();
    for policy in &document.policies {
        for operation in &policy.applies_to {
            facts.push(AuthorizesFact {
                policy: policy.id.clone(),
                operation: operation.clone(),
                actor: policy.actor.as_str(),
                decision: policy.decision.as_str(),
                error_ref: policy.error_ref.clone(),
            });
        }
    }
    facts.sort();
    facts
}

/// The sorted impact items: the ids downstream impact analysis must
/// consider when an authorization-relevant change lands.
pub fn impact_items(document: &Document) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for policy in &document.policies {
        items.push(policy.id.clone());
        items.extend(policy.applies_to.iter().cloned());
        items.push(policy.error_ref.clone());
        items.extend(policy.capabilities.iter().cloned());
        items.extend(policy.roles.iter().cloned());
        if let Some(composition) = &policy.composition {
            items.push(composition.clone());
        }
        for dimension in [
            &policy.scope.tenant,
            &policy.scope.workspace,
            &policy.scope.user,
        ]
        .into_iter()
        .flatten()
        {
            let _ = dimension;
            items.push("authorization.scope".to_owned());
        }
    }
    for mapping in &document.mappings {
        if mapping.state != MappingState::Full {
            items.push("authorization.mapping".to_owned());
        }
    }
    items.sort();
    items.dedup();
    items
}

/// The actor rank used to decide broadening and narrowing. Lower is
/// broader exposure.
fn actor_rank(actor: super::document::ActorType) -> u8 {
    match actor {
        super::document::ActorType::Public => 0,
        super::document::ActorType::Internal => 1,
        super::document::ActorType::Service => 2,
        super::document::ActorType::User => 3,
        super::document::ActorType::SystemJob => 4,
    }
}

/// The declared scope binding count of one policy.
fn scope_count(
    tenant: Option<&()>,
    workspace: Option<&()>,
    user: Option<&()>,
    scope: &super::document::ScopeMap,
) -> usize {
    let _ = (tenant, workspace, user);
    let count =
        |binding: &Option<super::document::Binding>| -> usize { usize::from(binding.is_some()) };
    count(&scope.tenant) + count(&scope.workspace) + count(&scope.user)
}

/// Classify one policy body pair.
fn classify_pair(
    before: &super::document::Policy,
    after: &super::document::Policy,
) -> Vec<ChangeLabel> {
    let mut labels = Vec::new();
    if before.actor != after.actor {
        if actor_rank(after.actor) < actor_rank(before.actor) {
            labels.push(ChangeLabel::ActorBroadened);
        } else {
            labels.push(ChangeLabel::ActorNarrowed);
        }
    }
    if scope_count(None, None, None, &before.scope) > scope_count(None, None, None, &after.scope) {
        labels.push(ChangeLabel::ScopeWeakened);
    } else if scope_count(None, None, None, &before.scope)
        < scope_count(None, None, None, &after.scope)
    {
        labels.push(ChangeLabel::ScopeTightened);
    } else if !scope_equal(&before.scope, &after.scope) {
        labels.push(ChangeLabel::ScopeWeakened);
    }
    if before.capabilities != after.capabilities {
        labels.push(ChangeLabel::CapabilityChanged);
    }
    if before.roles != after.roles {
        labels.push(ChangeLabel::RoleChanged);
    }
    if before.ownership != after.ownership {
        labels.push(ChangeLabel::OwnershipChanged);
    }
    if before.fields != after.fields {
        labels.push(ChangeLabel::FieldPermissionChanged);
    }
    if before.conditions != after.conditions {
        labels.push(ChangeLabel::PolicyChanged);
    }
    if before.error_ref != after.error_ref {
        labels.push(ChangeLabel::ErrorRefChanged);
    }
    if before.composition != after.composition {
        labels.push(ChangeLabel::CompositionChanged);
    }
    labels
}

/// Structural scope equality.
fn scope_equal(before: &super::document::ScopeMap, after: &super::document::ScopeMap) -> bool {
    before == after
}

/// Classify the authorization difference between two documents. The
/// result is sorted by policy id and label, deterministic, and never
/// optimistic: an incomparable pair classifies unknown.
pub fn classify_changes(before: &Document, after: &Document) -> Vec<PolicyChange> {
    let mut changes = Vec::new();
    for policy in &before.policies {
        match after
            .policies
            .iter()
            .find(|candidate| candidate.id == policy.id)
        {
            None => {
                let mut labels = vec![ChangeLabel::PolicyRemoved];
                if policy.decision == Decision::Deny {
                    labels.push(ChangeLabel::DenyRemoved);
                }
                let breaking = dominant(&labels);
                labels.sort();
                changes.push(PolicyChange {
                    policy: policy.id.clone(),
                    labels,
                    breaking,
                });
            }
            Some(newer) => {
                let labels = classify_pair(policy, newer);
                if !labels.is_empty() {
                    let mut sorted = labels;
                    let breaking = dominant(&sorted);
                    sorted.sort();
                    changes.push(PolicyChange {
                        policy: policy.id.clone(),
                        labels: sorted,
                        breaking,
                    });
                }
            }
        }
    }
    for policy in &after.policies {
        if !before
            .policies
            .iter()
            .any(|candidate| candidate.id == policy.id)
        {
            let mut labels = vec![ChangeLabel::PolicyAdded];
            if policy.decision == Decision::Deny {
                labels.push(ChangeLabel::DenyAdded);
            }
            let breaking = dominant(&labels);
            labels.sort();
            changes.push(PolicyChange {
                policy: policy.id.clone(),
                labels,
                breaking,
            });
        }
    }
    changes.sort_by(|a, b| (&a.policy, &a.breaking).cmp(&(&b.policy, &b.breaking)));
    changes
}

/// The dominant severity across labels: security beats behavioral beats
/// cosmetic; absence of labels is unknown.
fn dominant(labels: &[ChangeLabel]) -> Breaking {
    let mut severity = Breaking::Cosmetic;
    for label in labels {
        let candidate = match label {
            ChangeLabel::ActorBroadened
            | ChangeLabel::ScopeWeakened
            | ChangeLabel::DenyRemoved
            | ChangeLabel::PolicyRemoved => Breaking::Security,
            ChangeLabel::ActorNarrowed
            | ChangeLabel::ScopeTightened
            | ChangeLabel::FieldPermissionChanged
            | ChangeLabel::PolicyChanged
            | ChangeLabel::PolicyAdded
            | ChangeLabel::DenyAdded
            | ChangeLabel::CapabilityChanged
            | ChangeLabel::RoleChanged
            | ChangeLabel::OwnershipChanged
            | ChangeLabel::ErrorRefChanged
            | ChangeLabel::MappingStateChanged
            | ChangeLabel::CompositionChanged => Breaking::Behavioral,
        };
        if (candidate as u8) < (severity as u8) {
            severity = candidate;
        }
    }
    severity
}

/// The mapping-state change records between two documents, sorted.
pub fn mapping_changes(before: &Document, after: &Document) -> Vec<PolicyChange> {
    let mut changes = Vec::new();
    for mapping in &after.mappings {
        let previous = before.mappings.iter().find(|candidate| {
            candidate.target == mapping.target
                && candidate.semantic_policy_ref == mapping.semantic_policy_ref
        });
        match previous {
            None => {
                changes.push(PolicyChange {
                    policy: mapping.semantic_policy_ref.clone(),
                    labels: vec![ChangeLabel::MappingStateChanged],
                    breaking: dominant(&[ChangeLabel::MappingStateChanged]),
                });
            }
            Some(previous) if previous.state != mapping.state => {
                changes.push(PolicyChange {
                    policy: mapping.semantic_policy_ref.clone(),
                    labels: vec![ChangeLabel::MappingStateChanged],
                    breaking: dominant(&[ChangeLabel::MappingStateChanged]),
                });
            }
            Some(_) => {}
        }
    }
    changes.sort_by(|a, b| (&a.policy, &a.breaking).cmp(&(&b.policy, &b.breaking)));
    changes
}

/// The compact JSON projection of the classified changes for golden
/// fixtures.
pub fn changes_to_json(changes: &[PolicyChange]) -> Json {
    Json::Array(
        changes
            .iter()
            .map(|change| {
                json!({
                    "breaking": change.breaking.as_str(),
                    "labels": change
                        .labels
                        .iter()
                        .map(|label| Json::String(label.as_str().to_owned()))
                        .collect::<Vec<_>>(),
                    "policy": change.policy,
                })
            })
            .collect(),
    )
}

/// Whether one scope dimension is bound in a scope map; used by tests
/// and downstream projections.
pub fn binds_dimension(scope: &super::document::ScopeMap, dimension: ScopeDimension) -> bool {
    match dimension {
        ScopeDimension::Tenant => scope.tenant.is_some(),
        ScopeDimension::Workspace => scope.workspace.is_some(),
        ScopeDimension::User => scope.user.is_some(),
    }
}
