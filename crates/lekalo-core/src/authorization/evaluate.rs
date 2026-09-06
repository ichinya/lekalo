//! The pure authorization evaluator (issue #25).
//!
//! [`evaluate`] decides one request against one accepted document. It
//! receives an explicit actor principal, an explicit request context,
//! and an explicit scope: it never reads files, calls identity
//! providers, invokes middleware, runs adapters, or touches networks or
//! secrets. Deny-by-default: a matching deny dominates, a matching
//! allow is required, and everything else is denied.

use super::document::{
    ActorField, ActorType, Decision, Document, Literal, Operand, Policy, Predicate, ScopeAnchor,
    ScopeDimension, ScopeSource,
};
use std::collections::{BTreeMap, BTreeSet};

/// The maximum composition depth walked during evaluation; the review
/// rejects deeper graphs, so this is a belt-and-braces bound.
const COMPOSITION_DEPTH: usize = 8;

/// One typed evaluation principal. Values are bounded, synthetic or
/// test-supplied; nothing here is a runtime secret.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Principal {
    /// The closed actor type of the principal.
    pub actor: ActorType,
    /// Whether the principal has passed authentication. Public
    /// principals are unauthenticated by definition.
    pub authenticated: bool,
    /// The principal identifier.
    pub id: String,
    /// The principal tenant.
    pub tenant_id: Option<String>,
    /// The principal workspace.
    pub workspace_id: Option<String>,
    /// The held capabilities.
    pub capabilities: BTreeSet<String>,
    /// The held roles.
    pub roles: BTreeSet<String>,
    /// The named job, for `system.job` principals.
    pub job: Option<String>,
}

/// The runtime scope values supplied by the caller.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RequestScope {
    /// The tenant scope value.
    pub tenant: Option<String>,
    /// The workspace scope value.
    pub workspace: Option<String>,
    /// The user scope value.
    pub user: Option<String>,
}

/// One typed resource view: the entity identity plus the concrete
/// declared field values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceView {
    /// The resource entity semantic id.
    pub entity: String,
    /// The concrete field values.
    pub fields: BTreeMap<String, Literal>,
}

/// The requested field access mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldAccess {
    /// The field is read.
    Read,
    /// The field is written.
    Write,
}

/// One typed request against one operation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    /// The protected operation semantic id.
    pub operation: String,
    /// The concrete input field values.
    pub input: BTreeMap<String, Literal>,
    /// The concrete resource view, when the request targets one.
    pub resource: Option<ResourceView>,
    /// The runtime scope values.
    pub scope: RequestScope,
    /// The exact field being accessed, when the request is field-level.
    pub field: Option<(String, FieldAccess)>,
}

/// Why an evaluation denied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DenyReason {
    /// A non-public principal without authentication.
    MissingAuthentication,
    /// No policy matched the request.
    NoMatch,
    /// A matched policy was a deny.
    DenyPolicy,
    /// A scope binding failed.
    Scope,
    /// A required capability is missing.
    Capability,
    /// A required role is missing.
    Role,
    /// An ownership predicate failed.
    Ownership,
    /// A typed condition failed.
    Condition,
    /// The requested field is not granted by any matching allow.
    Field,
    /// A composition member failed.
    Composition,
}

/// The evaluation outcome.
#[derive(Clone, Debug, PartialEq)]
pub enum Evaluation {
    /// The request is allowed by one policy.
    Allowed {
        /// The allowing policy id.
        policy: String,
    },
    /// The request is denied.
    Denied {
        /// The denying policy id, when a policy produced the denial.
        policy: Option<String>,
        /// The error-contract reference, when a policy produced the
        /// denial.
        error_ref: Option<String>,
        /// Why the request denied.
        reason: DenyReason,
    },
}

/// Resolve the actor-side field value.
fn actor_value(principal: &Principal, field: ActorField) -> Option<String> {
    match field {
        ActorField::Id => Some(principal.id.clone()),
        ActorField::TenantId => principal.tenant_id.clone(),
        ActorField::WorkspaceId => principal.workspace_id.clone(),
    }
}

/// One resolved comparison value: scalar text or a string list.
enum Value {
    /// One scalar value.
    Text(String),
    /// One bounded list value.
    List(Vec<String>),
}

/// Resolve a typed source value.
fn source_value(source: &ScopeSource, principal: &Principal, request: &Request) -> Option<Value> {
    match source {
        ScopeSource::Actor(field) => actor_value(principal, *field).map(Value::Text),
        ScopeSource::Input(field) => request.input.get(field).map(literal_value),
        ScopeSource::Resource(field) => request
            .resource
            .as_ref()
            .and_then(|resource| resource.fields.get(field))
            .map(literal_value),
        ScopeSource::Scope(dimension) => scope_value(&request.scope, *dimension).map(Value::Text),
    }
}

/// Resolve a typed anchor value.
fn anchor_value(anchor: &ScopeAnchor, request: &Request) -> Option<Value> {
    match anchor {
        ScopeAnchor::Resource(field) => request
            .resource
            .as_ref()
            .and_then(|resource| resource.fields.get(field))
            .map(literal_value),
        ScopeAnchor::Scope(dimension) => scope_value(&request.scope, *dimension).map(Value::Text),
    }
}

/// The comparable value of one resource or input literal.
fn literal_value(literal: &Literal) -> Value {
    match literal {
        Literal::List(items) => Value::List(items.clone()),
        other => Value::Text(other.as_text()),
    }
}

/// The scope value for one dimension.
fn scope_value(scope: &RequestScope, dimension: ScopeDimension) -> Option<String> {
    match dimension {
        ScopeDimension::Tenant => scope.tenant.clone(),
        ScopeDimension::Workspace => scope.workspace.clone(),
        ScopeDimension::User => scope.user.clone(),
    }
}

/// Whether one scope binding holds. Equality and ownership compare
/// scalar values; membership also holds against a declared list value
/// containing the source.
fn binding_holds(
    source: &ScopeSource,
    relation: super::document::Relation,
    anchor: &ScopeAnchor,
    principal: &Principal,
    request: &Request,
) -> bool {
    let Some(source) = source_value(source, principal, request) else {
        return false;
    };
    let Some(anchor) = anchor_value(anchor, request) else {
        return false;
    };
    match (source, anchor) {
        (Value::Text(source), Value::Text(anchor)) => source == anchor,
        (Value::Text(source), Value::List(items)) => match relation {
            super::document::Relation::Membership => items.contains(&source),
            _ => false,
        },
        _ => false,
    }
}

/// Resolve one condition operand to a comparable literal.
fn operand_literal(operand: &Operand, principal: &Principal, request: &Request) -> Option<Literal> {
    match operand {
        Operand::Actor(field) => actor_value(principal, *field).map(Literal::Str),
        Operand::Input(field) => request.input.get(field).cloned(),
        Operand::Resource(field) => request
            .resource
            .as_ref()
            .and_then(|r| r.fields.get(field))
            .cloned(),
        Operand::Scope(dimension) => scope_value(&request.scope, *dimension).map(Literal::Str),
    }
}

/// Whether one condition holds.
fn condition_holds(predicate: &Predicate, principal: &Principal, request: &Request) -> bool {
    match predicate {
        Predicate::All(items) => items.iter().all(|p| condition_holds(p, principal, request)),
        Predicate::Any(items) => items.iter().any(|p| condition_holds(p, principal, request)),
        Predicate::Not(inner) => !condition_holds(inner, principal, request),
        Predicate::Equals(left, right) => {
            operand_literal(left, principal, request).as_ref() == Some(right)
        }
        Predicate::NotEquals(left, right) => {
            let resolved = operand_literal(left, principal, request);
            match resolved {
                Some(value) => value != *right,
                None => false,
            }
        }
        Predicate::In(left, values) => operand_literal(left, principal, request)
            .map(|value| values.contains(&value))
            .unwrap_or(false),
    }
}

/// Whether the policy's own requirements hold (scope, capabilities,
/// roles, ownership, conditions, and the named job).
fn requirements_hold(policy: &Policy, principal: &Principal, request: &Request) -> bool {
    if let Some(job) = &policy.job {
        if principal.job.as_deref() != Some(job.as_str()) {
            return false;
        }
    }
    let scope_ok = [
        (&policy.scope.tenant, ScopeDimension::Tenant),
        (&policy.scope.workspace, ScopeDimension::Workspace),
        (&policy.scope.user, ScopeDimension::User),
    ]
    .iter()
    .all(|(binding, _)| match binding {
        None => true,
        Some(binding) => binding_holds(
            &binding.source,
            binding.relation,
            &binding.anchor,
            principal,
            request,
        ),
    });
    if !scope_ok {
        return false;
    }
    if !policy
        .capabilities
        .iter()
        .all(|c| principal.capabilities.contains(c))
    {
        return false;
    }
    if !policy.roles.iter().all(|r| principal.roles.contains(r)) {
        return false;
    }
    for ownership in &policy.ownership {
        let Some(actor) = actor_value(principal, ownership.actor_field) else {
            return false;
        };
        let Some(resource) = &request.resource else {
            return false;
        };
        let Some(value) = resource.fields.get(&ownership.resource_field) else {
            return false;
        };
        if value.as_text() != actor {
            return false;
        }
    }
    if let Some(conditions) = &policy.conditions {
        if !condition_holds(conditions, principal, request) {
            return false;
        }
    }
    true
}

/// Whether one composition member is a satisfied allow.
fn composition_member_holds(
    member: &str,
    document: &Document,
    principal: &Principal,
    request: &Request,
    depth: usize,
) -> bool {
    if depth > COMPOSITION_DEPTH {
        return false;
    }
    if let Some(policy) = document.policies.iter().find(|p| p.id == member) {
        return policy.decision == Decision::Allow
            && policy_applies(policy, &request.operation, Some(principal.actor))
            && requirements_hold(policy, principal, request);
    }
    if let Some(composition) = document.compositions.iter().find(|c| c.id == member) {
        return composition_holds(composition, document, principal, request, depth + 1);
    }
    false
}

/// Whether one composition holds.
fn composition_holds(
    composition: &super::document::Composition,
    document: &Document,
    principal: &Principal,
    request: &Request,
    depth: usize,
) -> bool {
    if depth > COMPOSITION_DEPTH {
        return false;
    }
    composition
        .all_of
        .iter()
        .all(|member| composition_member_holds(member, document, principal, request, depth))
        && (composition.any_of.is_empty()
            || composition.any_of.iter().any(|member| {
                composition_member_holds(member, document, principal, request, depth)
            }))
}

/// Whether one policy applies to the operation and actor. `actor` of
/// `None` skips the actor filter (composition members).
fn policy_applies(policy: &Policy, operation: &str, actor: Option<ActorType>) -> bool {
    if let Some(actor) = actor {
        if policy.actor != actor {
            return false;
        }
    }
    policy.applies_to.iter().any(|op| op == operation)
}

/// Whether the policy grants the requested field access.
fn field_granted(policy: &Policy, field: &str, access: FieldAccess) -> bool {
    let Some(fields) = &policy.fields else {
        return false;
    };
    let granted = match access {
        FieldAccess::Read => &fields.read,
        FieldAccess::Write => &fields.write,
    };
    granted.iter().any(|granted| granted == field)
}

/// Evaluate one request. Deny-by-default, deny dominates, canonical
/// candidate order decides which allow is reported.
pub fn evaluate(document: &Document, principal: &Principal, request: &Request) -> Evaluation {
    if principal.actor.requires_authentication() && !principal.authenticated {
        return Evaluation::Denied {
            policy: None,
            error_ref: None,
            reason: DenyReason::MissingAuthentication,
        };
    }
    // Deny dominance: every satisfied deny policy decides first.
    for policy in &document.policies {
        if policy.decision != Decision::Deny {
            continue;
        }
        if !policy_applies(policy, &request.operation, Some(principal.actor)) {
            continue;
        }
        if requirements_hold(policy, principal, request) {
            return Evaluation::Denied {
                policy: Some(policy.id.clone()),
                error_ref: Some(policy.error_ref.clone()),
                reason: DenyReason::DenyPolicy,
            };
        }
    }
    // A matching allow is required; candidates walk in canonical order.
    let mut field_blocked = false;
    for policy in &document.policies {
        if policy.decision != Decision::Allow {
            continue;
        }
        if !policy_applies(policy, &request.operation, Some(principal.actor)) {
            continue;
        }
        if !requirements_hold(policy, principal, request) {
            continue;
        }
        if let Some(composition_ref) = &policy.composition {
            let holds = document
                .compositions
                .iter()
                .find(|composition| &composition.id == composition_ref)
                .map(|composition| composition_holds(composition, document, principal, request, 1))
                .unwrap_or(false);
            if !holds {
                continue;
            }
        }
        if let Some((field, access)) = &request.field {
            if !field_granted(policy, field, *access) {
                field_blocked = true;
                continue;
            }
        }
        return Evaluation::Allowed {
            policy: policy.id.clone(),
        };
    }
    // A fully matching allow that lacked only the field grant reports
    // the dedicated field reason; everything else is default-deny.
    let reason = if field_blocked {
        DenyReason::Field
    } else {
        DenyReason::NoMatch
    };
    Evaluation::Denied {
        policy: None,
        error_ref: None,
        reason,
    }
}

/// Whether one scenario authorization assertion (#23) is satisfied by
/// the evaluator: the asserted outcome must equal the evaluated outcome
/// of the named policy for the request context. The scenario module
/// owns parsing and execution; this seam only supplies the semantics
/// its `authorization` assertion kind promises.
pub fn satisfies_scenario_assertion(
    document: &Document,
    assertion: &crate::scenario::Assertion,
    principal: &Principal,
    request: &Request,
) -> bool {
    let crate::scenario::Assertion::Authorization {
        policy, outcome, ..
    } = assertion
    else {
        return false;
    };
    let Some(policy) = document
        .policies
        .iter()
        .find(|candidate| candidate.id == policy.as_str())
    else {
        return false;
    };
    let applies = policy.applies_to.contains(&request.operation) && policy.actor == principal.actor;
    let mut holds = applies && requirements_hold(policy, principal, request);
    if holds {
        if let Some(composition_ref) = &policy.composition {
            holds = document
                .compositions
                .iter()
                .find(|composition| &composition.id == composition_ref)
                .map(|composition| composition_holds(composition, document, principal, request, 1))
                .unwrap_or(false);
        }
    }
    use crate::scenario::assertion::AuthOutcome;
    match (policy.decision, outcome) {
        (Decision::Allow, AuthOutcome::Allowed) => holds,
        (Decision::Deny, AuthOutcome::Denied) => holds,
        (Decision::Allow, AuthOutcome::Denied) => applies && !holds,
        (Decision::Deny, AuthOutcome::Allowed) => false,
    }
}
