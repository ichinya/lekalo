//! The closed typed authorization document (issue #25).
//!
//! [`Document`] is the typed, canonical form of one accepted
//! `lekalo/authorization.yaml`. Parsing is fail-closed: every field,
//! grammar, bound, duplicate, actor/auth pairing, and scope shape is
//! validated here, before any review or evaluation. The parser never
//! accepts runtime principals, provider names, middleware names, or
//! secrets: the vocabulary is closed actors, declared field names, and
//! bounded symbolic references only.

use super::diagnostic as diag;
use super::version::limits;
use crate::diagnostics::DiagnosticSet;
use serde_json::Value as Json;

/// The maximum diagnostics a single parse/review collects before it
/// stops (the accepted #11 per-result bound).
const MAX_DIAGNOSTICS: usize = 256;

/// A bounded scalar literal, or a bounded string list (list values
/// appear only in resource views and membership anchors).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Literal {
    /// A bounded string.
    Str(String),
    /// A base-10 integer.
    Int(i64),
    /// A boolean.
    Bool(bool),
    /// A bounded list of strings.
    List(Vec<String>),
}

impl Literal {
    /// The canonical JSON rendering of this literal.
    pub fn to_json(&self) -> Json {
        match self {
            Self::Str(text) => Json::String(text.clone()),
            Self::Int(value) => Json::Number((*value).into()),
            Self::Bool(value) => Json::Bool(*value),
            Self::List(items) => Json::Array(items.iter().cloned().map(Json::String).collect()),
        }
    }

    /// The bounded wire text used in evaluation evidence.
    pub fn as_text(&self) -> String {
        match self {
            Self::Str(text) => text.clone(),
            Self::Int(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
            Self::List(items) => items.join(","),
        }
    }
}

/// The closed actor vocabulary. Every non-public actor requires an
/// explicit authenticated principal at evaluation; missing
/// authentication is denied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum ActorType {
    /// No authentication; only explicit public policies/effects.
    #[default]
    Public,
    /// An authenticated end user (`identity.user`).
    User,
    /// An authenticated workload or service identity.
    Service,
    /// An explicit service or scheduler job identity plus a named job.
    SystemJob,
    /// A trusted runtime identity at a declared boundary; never a
    /// public bypass.
    Internal,
}

impl ActorType {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::User => "identity.user",
            Self::Service => "identity.service",
            Self::SystemJob => "system.job",
            Self::Internal => "internal",
        }
    }

    /// Registry lookup by exact wire spelling.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "public" => Some(Self::Public),
            "identity.user" => Some(Self::User),
            "identity.service" => Some(Self::Service),
            "system.job" => Some(Self::SystemJob),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }

    /// Whether this actor requires an authenticated principal.
    pub const fn requires_authentication(self) -> bool {
        !matches!(self, Self::Public)
    }
}

/// The closed policy decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Decision {
    /// Grant the covered access.
    Allow,
    /// Refuse the covered access; deny dominates every allow.
    Deny,
}

impl Decision {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    /// Registry lookup by exact wire spelling.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// The closed adapter mapping states. Only `full` can pass strict.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum MappingState {
    /// Every semantic policy construct maps to the target.
    Full,
    /// Some constructs map; the rest is explicitly uncovered.
    Partial,
    /// The target cannot express the policy semantics.
    Unsupported,
    /// No verified evidence exists.
    Unknown,
}

impl MappingState {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }

    /// Registry lookup by exact wire spelling.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "full" => Some(Self::Full),
            "partial" => Some(Self::Partial),
            "unsupported" => Some(Self::Unsupported),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// The closed scope dimensions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ScopeDimension {
    /// Tenant isolation.
    Tenant,
    /// Workspace isolation.
    Workspace,
    /// Per-user isolation.
    User,
}

impl ScopeDimension {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tenant => "tenant",
            Self::Workspace => "workspace",
            Self::User => "user",
        }
    }

    /// Registry lookup by exact wire spelling.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "tenant" => Some(Self::Tenant),
            "workspace" => Some(Self::Workspace),
            "user" => Some(Self::User),
            _ => None,
        }
    }
}

/// The closed actor-side binding fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActorField {
    /// The principal identifier.
    Id,
    /// The principal tenant.
    TenantId,
    /// The principal workspace.
    WorkspaceId,
}

impl ActorField {
    /// The exact wire spelling (the typed source form).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Id => "actor.id",
            Self::TenantId => "actor.tenant_id",
            Self::WorkspaceId => "actor.workspace_id",
        }
    }
}

/// One typed scope source: an actor field, a declared input or resource
/// field, or a scope dimension. Policy literals are not sources.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScopeSource {
    /// An actor-side field.
    Actor(ActorField),
    /// A declared input field of the protected operation.
    Input(String),
    /// A declared field of the protected resource.
    Resource(String),
    /// A scope dimension value supplied by the runtime context.
    Scope(ScopeDimension),
}

/// One typed binding anchor: the resource field or scope dimension the
/// source is checked against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScopeAnchor {
    /// A declared field of the protected resource.
    Resource(String),
    /// A scope dimension value.
    Scope(ScopeDimension),
}

/// The closed binding relations. No relation is ever inferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Relation {
    /// Exact equality between the source and anchor values.
    Equality,
    /// The source value is a member of the anchor set.
    Membership,
    /// Resource-field to actor-field ownership equality.
    Ownership,
}

/// One declared scope binding for one dimension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    /// The typed source.
    pub source: ScopeSource,
    /// The explicit relation.
    pub relation: Relation,
    /// The typed anchor.
    pub anchor: ScopeAnchor,
}

/// The per-dimension scope map; at most one binding per dimension.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScopeMap {
    /// The tenant binding, when declared.
    pub tenant: Option<Binding>,
    /// The workspace binding, when declared.
    pub workspace: Option<Binding>,
    /// The user binding, when declared.
    pub user: Option<Binding>,
}

/// One typed condition operand.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operand {
    /// An actor-side field.
    Actor(ActorField),
    /// A declared input field.
    Input(String),
    /// A declared resource field.
    Resource(String),
    /// A scope dimension value.
    Scope(ScopeDimension),
}

/// The closed, non-executable condition language: typed operands, three
/// operators, and fixed-depth all/any/not composition. No calls, scripts,
/// loops, arithmetic, regex, time, or target syntax exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Predicate {
    /// Every sub-predicate holds.
    All(Vec<Predicate>),
    /// At least one sub-predicate holds.
    Any(Vec<Predicate>),
    /// The sub-predicate does not hold.
    Not(Box<Predicate>),
    /// Typed equality.
    Equals(Operand, Literal),
    /// Typed inequality.
    NotEquals(Operand, Literal),
    /// Typed membership over a bounded literal set.
    In(Operand, Vec<Literal>),
}

/// The field-level read/write permission sets of one policy.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FieldPermissions {
    /// The fields this policy allows reading.
    pub read: Vec<String>,
    /// The fields this policy allows writing.
    pub write: Vec<String>,
}

/// One declared capability or role symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolDecl {
    /// The document-local symbol identity.
    pub id: String,
    /// The explicit, acyclic implications, sorted and unique.
    pub implies: Vec<String>,
}

/// One ownership predicate: resource-field to actor-field equality over
/// declared IR fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ownership {
    /// The declared resource field.
    pub resource_field: String,
    /// The actor-side field.
    pub actor_field: ActorField,
}

/// One authorization policy: the closed combination of actor, decision,
/// scope bindings, capabilities, roles, ownership predicates, conditions,
/// field permissions, composition reference, and required error
/// contract reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    /// The document-local policy identity.
    pub id: String,
    /// The closed actor type.
    pub actor: ActorType,
    /// The closed decision.
    pub decision: Decision,
    /// The required error-contract reference carried by every denied
    /// path this policy can produce.
    pub error_ref: String,
    /// The protected operations this policy covers, sorted and unique.
    pub applies_to: Vec<String>,
    /// The named job, required exactly for `system.job` actors.
    pub job: Option<String>,
    /// The declared scope bindings.
    pub scope: ScopeMap,
    /// The required capabilities, sorted and unique.
    pub capabilities: Vec<String>,
    /// The required roles, sorted and unique.
    pub roles: Vec<String>,
    /// The ownership predicates.
    pub ownership: Vec<Ownership>,
    /// The typed condition tree, when declared.
    pub conditions: Option<Predicate>,
    /// The field-level permissions, when declared.
    pub fields: Option<FieldPermissions>,
    /// The composition this policy participates in, when declared.
    pub composition: Option<String>,
}

/// One composition declaration: closed all_of/any_of over policy and
/// composition references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Composition {
    /// The document-local composition identity.
    pub id: String,
    /// Every referenced member must match; sorted and unique.
    pub all_of: Vec<String>,
    /// At least one referenced member must match; sorted and unique.
    pub any_of: Vec<String>,
}

/// One neutral adapter mapping-evidence record. Names, execution, and
/// transport projection stay with the adapter owners.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mapping {
    /// The target/profile identity the evidence is about.
    pub target: String,
    /// The policy this evidence maps.
    pub semantic_policy_ref: String,
    /// The recorded mapping state.
    pub state: MappingState,
    /// The safe logical evidence reference, when supplied.
    pub evidence_ref: Option<String>,
    /// The observed revision digest of the mapped target state.
    pub observed_revision: String,
    /// The bounded reason references, sorted and unique.
    pub reason_refs: Vec<String>,
}

/// The pinned compiled-model reference: the exact Model schema version
/// plus the canonical IR payload digest the policies were written
/// against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRef {
    /// The accepted Model schema version.
    pub schema_version: String,
    /// The canonical IR digest.
    pub ir_digest: String,
}

/// The closed canonical authorization document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    /// The pinned compiled-model reference.
    pub model_ref: ModelRef,
    /// Declared capabilities, sorted by id.
    pub capabilities: Vec<SymbolDecl>,
    /// Declared roles, sorted by id.
    pub roles: Vec<SymbolDecl>,
    /// Declared policies, sorted by id.
    pub policies: Vec<Policy>,
    /// Declared compositions, sorted by id.
    pub compositions: Vec<Composition>,
    /// Declared adapter mapping evidence, sorted by (target, policy).
    pub mappings: Vec<Mapping>,
}

/// A parse collector: bounded diagnostics in deterministic order.
struct Collector {
    diagnostics: Vec<crate::diagnostics::Diagnostic>,
}

impl Collector {
    fn push(&mut self, diagnostic: Option<crate::diagnostics::Diagnostic>) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            if let Some(diagnostic) = diagnostic {
                self.diagnostics.push(diagnostic);
            }
        }
    }
}

/// A one-character-class grammar check matching the accepted Model
/// segment grammar: `[a-z][a-z0-9_]{0,62}`.
fn is_segment(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > 63 {
        return false;
    }
    let first = bytes[0].is_ascii_lowercase();
    first
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// The `[a-z][a-z0-9_]{0,62}(.[a-z][a-z0-9_]{0,62}){1,2}` identifier
/// grammar shared by semantic symbol references and document-local ids.
fn is_symbol_id(text: &str) -> bool {
    if text.len() > 190 {
        return false;
    }
    let segments: Vec<&str> = text.split('.').collect();
    (2..=3).contains(&segments.len()) && segments.iter().all(|s| is_segment(s))
}

/// The declared-field-name grammar (one segment).
fn is_field_name(text: &str) -> bool {
    is_segment(text)
}

/// Reject strings that can never be a bounded symbolic value.
fn is_clean_symbol(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 190
        && text
            .chars()
            .all(|c| c.is_ascii_graphic() && c != '<' && c != '>' && c != '"' && c != '\\')
}
/// Reject unknown object keys. The closed schema never widens by
/// mutation: an unrecognized key is a document-invalid violation, never
/// an ignored field.
fn check_keys(
    object: &serde_json::Map<String, Json>,
    allowed: &[&str],
    detail: &str,
    collector: &mut Collector,
) {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            collector.push(diag::document_invalid(detail, Some(key)));
        }
    }
}

/// Parse one closed string property.
fn string_property<'a>(object: &'a serde_json::Map<String, Json>, key: &str) -> Option<&'a str> {
    match object.get(key) {
        Some(Json::String(text)) => Some(text.as_str()),
        _ => None,
    }
}

/// Parse an exact wire-key enum property.
fn enum_property<T: Copy>(
    object: &serde_json::Map<String, Json>,
    key: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    string_property(object, key).and_then(parse)
}

/// Count predicate nodes; `None` means a closed bound was exceeded.
fn count_nodes(nodes: &mut usize, depth: &mut usize, at: usize) -> bool {
    *nodes += 1;
    *depth = (*depth).max(at);
    *nodes <= limits::CONDITION_NODES && *depth <= limits::CONDITION_DEPTH
}

/// Collect one literal.
fn parse_literal(value: &Json) -> Option<Literal> {
    match value {
        Json::String(text)
            if !text.is_empty() && text.chars().all(|c| !c.is_control()) && text.len() <= 128 =>
        {
            Some(Literal::Str(text.clone()))
        }
        Json::Number(number) => number.as_i64().map(Literal::Int),
        Json::Bool(value) => Some(Literal::Bool(*value)),
        _ => None,
    }
}

/// Collect one typed operand.
fn parse_operand(text: &str) -> Option<Operand> {
    if let Some(field) = text.strip_prefix("input.") {
        return is_field_name(field).then(|| Operand::Input(field.to_owned()));
    }
    if let Some(field) = text.strip_prefix("resource.") {
        return is_field_name(field).then(|| Operand::Resource(field.to_owned()));
    }
    if let Some(dimension) = text.strip_prefix("scope.") {
        return ScopeDimension::from_key(dimension).map(Operand::Scope);
    }
    match text {
        "actor.id" => Some(Operand::Actor(ActorField::Id)),
        "actor.tenant_id" => Some(Operand::Actor(ActorField::TenantId)),
        "actor.workspace_id" => Some(Operand::Actor(ActorField::WorkspaceId)),
        _ => None,
    }
}

/// Collect one condition tree, enforcing the closed grammar and bounds.
fn parse_predicate(
    value: &Json,
    nodes: &mut usize,
    depth: &mut usize,
    at: usize,
) -> Option<Predicate> {
    let object = value.as_object()?;
    // Exactly one closed shape per node; extra or mixed keys are
    // invalid (the Rust gate mirrors the schema's additionalProperties
    // false, which JSON-Schema alone cannot enforce here).
    let keys: Vec<&str> = object.keys().map(String::as_str).collect();
    let shape_ok = matches!(
        keys.as_slice(),
        ["all"] | ["any"] | ["not"] | ["left", "op", "right"] | ["left", "op", "values"]
    );
    if !shape_ok {
        return None;
    }
    let predicate = if let Some(Json::Array(items)) = object.get("all") {
        let mut parsed = Vec::new();
        for item in items {
            parsed.push(parse_predicate(item, nodes, depth, at + 1)?);
        }
        Predicate::All(parsed)
    } else if let Some(Json::Array(items)) = object.get("any") {
        let mut parsed = Vec::new();
        for item in items {
            parsed.push(parse_predicate(item, nodes, depth, at + 1)?);
        }
        Predicate::Any(parsed)
    } else if let Some(inner) = object.get("not") {
        Predicate::Not(Box::new(parse_predicate(inner, nodes, depth, at + 1)?))
    } else if let (Some(left), Some(op)) = (
        string_property(object, "left"),
        string_property(object, "op"),
    ) {
        let operand = parse_operand(left)?;
        match op {
            "equals" => Predicate::Equals(operand, parse_literal(object.get("right")?)?),
            "not_equals" => Predicate::NotEquals(operand, parse_literal(object.get("right")?)?),
            "in" => {
                let Json::Array(values) = object.get("values")? else {
                    return None;
                };
                if values.len() > limits::IN_VALUES {
                    return None;
                }
                let mut parsed = Vec::new();
                for value in values {
                    parsed.push(parse_literal(value)?);
                }
                Predicate::In(operand, parsed)
            }
            _ => return None,
        }
    } else {
        return None;
    };
    if count_nodes(nodes, depth, at) {
        Some(predicate)
    } else {
        None
    }
}

/// Collect one typed binding.
fn parse_binding(
    value: &Json,
    rule: &str,
    dimension: &str,
    collector: &mut Collector,
) -> Option<Binding> {
    let object = value.as_object()?;
    check_keys(
        object,
        &["source", "relation", "anchor"],
        "binding-keys",
        collector,
    );
    let source_text = string_property(object, "source")?;
    let relation = enum_property(object, "relation", |key| match key {
        "equality" => Some(Relation::Equality),
        "membership" => Some(Relation::Membership),
        "ownership" => Some(Relation::Ownership),
        _ => None,
    })?;
    let anchor_text = string_property(object, "anchor")?;
    let source = if let Some(field) = source_text.strip_prefix("input.") {
        is_field_name(field)
            .then(|| ScopeSource::Input(field.to_owned()))
            .or_else(|| {
                collector.push(diag::scope_invalid(rule, dimension, "source-grammar"));
                None
            })?
    } else if let Some(field) = source_text.strip_prefix("resource.") {
        is_field_name(field)
            .then(|| ScopeSource::Resource(field.to_owned()))
            .or_else(|| {
                collector.push(diag::scope_invalid(rule, dimension, "source-grammar"));
                None
            })?
    } else if let Some(dimension) = source_text.strip_prefix("scope.") {
        ScopeDimension::from_key(dimension)
            .map(ScopeSource::Scope)
            .or_else(|| {
                collector.push(diag::scope_invalid(rule, dimension, "source-grammar"));
                None
            })?
    } else {
        match source_text {
            "actor.id" => ScopeSource::Actor(ActorField::Id),
            "actor.tenant_id" => ScopeSource::Actor(ActorField::TenantId),
            "actor.workspace_id" => ScopeSource::Actor(ActorField::WorkspaceId),
            _ => {
                collector.push(diag::scope_invalid(rule, dimension, "source-unknown"));
                return None;
            }
        }
    };
    let anchor = if let Some(field) = anchor_text.strip_prefix("resource.") {
        if !is_field_name(field) {
            collector.push(diag::scope_invalid(rule, dimension, "anchor-grammar"));
            return None;
        }
        ScopeAnchor::Resource(field.to_owned())
    } else if let Some(dimension) = anchor_text.strip_prefix("scope.") {
        match ScopeDimension::from_key(dimension) {
            Some(dimension) => ScopeAnchor::Scope(dimension),
            None => {
                collector.push(diag::scope_invalid(rule, dimension, "anchor-grammar"));
                return None;
            }
        }
    } else {
        collector.push(diag::scope_invalid(rule, dimension, "anchor-unknown"));
        return None;
    };
    Some(Binding {
        source,
        relation,
        anchor,
    })
}

/// Sort and reject duplicates; a duplicate is never silently merged.
fn sorted_unique(values: Vec<String>, rule: &str, collector: &mut Collector) -> Vec<String> {
    let mut sorted = values;
    sorted.sort();
    let before = sorted.len();
    sorted.dedup();
    if sorted.len() != before {
        collector.push(diag::policy_invalid(rule, "duplicate"));
    }
    sorted
}

/// Collect one symbol declaration (capability or role).
fn parse_symbol_decl(value: &Json, collector: &mut Collector) -> Option<SymbolDecl> {
    let object = value.as_object()?;
    check_keys(object, &["id", "implies"], "symbol-keys", collector);
    let id = string_property(object, "id")?.to_owned();
    if !is_symbol_id(&id) {
        collector.push(diag::document_invalid("symbol-id", Some(&id)));
        return None;
    }
    let mut implies = Vec::new();
    match object.get("implies") {
        None | Some(Json::Null) => {}
        Some(Json::Array(items)) => {
            if items.len() > limits::IMPLIES {
                collector.push(diag::limit_exceeded("implies", items.len() as u64));
                return None;
            }
            for item in items {
                match item.as_str() {
                    Some(text) if is_symbol_id(text) => implies.push(text.to_owned()),
                    _ => {
                        collector.push(diag::document_invalid("implies-id", None));
                        return None;
                    }
                }
            }
        }
        Some(_) => {
            collector.push(diag::document_invalid("implies-shape", None));
            return None;
        }
    }
    let implies = sorted_unique(implies, "authorization.policy-invalid", collector);
    Some(SymbolDecl { id, implies })
}

/// Collect one policy entry.
fn parse_policy(value: &Json, collector: &mut Collector) -> Option<Policy> {
    let object = value.as_object()?;
    check_keys(
        object,
        &[
            "id",
            "actor",
            "decision",
            "error_ref",
            "applies_to",
            "job",
            "scope",
            "capabilities",
            "roles",
            "ownership",
            "conditions",
            "fields",
            "composition",
        ],
        "policy-keys",
        collector,
    );
    let rule = "authorization.policy-invalid";
    let id = string_property(object, "id")?.to_owned();
    if !is_symbol_id(&id) {
        collector.push(diag::document_invalid("policy-id", Some(&id)));
        return None;
    }
    let actor_text = string_property(object, "actor")?;
    let Some(actor) = ActorType::from_key(actor_text) else {
        collector.push(diag::actor_invalid(&id, "actor-unknown"));
        return None;
    };
    let Some(decision) = enum_property(object, "decision", Decision::from_key) else {
        collector.push(diag::policy_invalid(&id, "decision-unknown"));
        return None;
    };
    let error_ref = string_property(object, "error_ref")?.to_owned();
    if !is_symbol_id(&error_ref) {
        collector.push(diag::error_ref_unresolved(&id, &diag::bounded(&error_ref)));
        return None;
    }
    let mut applies_to = Vec::new();
    match object.get("applies_to") {
        Some(Json::Array(items)) => {
            if items.is_empty() || items.len() > limits::APPLIES_TO {
                collector.push(diag::limit_exceeded("applies_to", items.len() as u64));
                return None;
            }
            for item in items {
                match item.as_str() {
                    Some(text) if is_symbol_id(text) => applies_to.push(text.to_owned()),
                    _ => {
                        collector.push(diag::ref_unresolved(&id, "applies_to"));
                        return None;
                    }
                }
            }
        }
        _ => {
            collector.push(diag::policy_invalid(&id, "applies-to-shape"));
            return None;
        }
    }
    let applies_to = sorted_unique(applies_to, rule, collector);
    let job = match object.get("job") {
        None | Some(Json::Null) => None,
        Some(Json::String(text)) if is_symbol_id(text) => Some(text.to_owned()),
        Some(_) => {
            collector.push(diag::ref_unresolved(&id, "job"));
            return None;
        }
    };
    // The closed actor/auth pairing: only system.job carries a named job.
    match (actor, &job) {
        (ActorType::SystemJob, Some(_)) => {}
        (_, None) => {
            if actor == ActorType::SystemJob {
                collector.push(diag::actor_invalid(&id, "job-missing"));
                return None;
            }
        }
        (_, Some(_)) => {
            collector.push(diag::actor_invalid(&id, "job-forbidden"));
            return None;
        }
    }
    let mut scope = ScopeMap::default();
    match object.get("scope") {
        None | Some(Json::Null) => {}
        Some(scope_value) => {
            let scope_object = scope_value.as_object()?;
            check_keys(
                scope_object,
                &["tenant", "workspace", "user"],
                "scope-keys",
                collector,
            );
            for (key, binding) in scope_object {
                let Some(dimension) = ScopeDimension::from_key(key) else {
                    collector.push(diag::scope_invalid(&id, key, "dimension-unknown"));
                    continue;
                };
                let Some(parsed) = parse_binding(binding, &id, key, collector) else {
                    continue;
                };
                let slot = match dimension {
                    ScopeDimension::Tenant => &mut scope.tenant,
                    ScopeDimension::Workspace => &mut scope.workspace,
                    ScopeDimension::User => &mut scope.user,
                };
                if slot.is_some() {
                    collector.push(diag::scope_invalid(&id, key, "duplicate"));
                    continue;
                }
                *slot = Some(parsed);
            }
        }
    }
    let mut capabilities = Vec::new();
    if let Some(Json::Array(items)) = object.get("capabilities") {
        if items.len() > limits::SYMBOL_DECLS {
            collector.push(diag::limit_exceeded("capabilities", items.len() as u64));
            return None;
        }
        for item in items {
            match item.as_str() {
                Some(text) if is_symbol_id(text) => capabilities.push(text.to_owned()),
                _ => {
                    collector.push(diag::ref_unresolved(&id, "capabilities"));
                    return None;
                }
            }
        }
    }
    let capabilities = sorted_unique(capabilities, rule, collector);
    let mut roles = Vec::new();
    if let Some(Json::Array(items)) = object.get("roles") {
        if items.len() > limits::SYMBOL_DECLS {
            collector.push(diag::limit_exceeded("roles", items.len() as u64));
            return None;
        }
        for item in items {
            match item.as_str() {
                Some(text) if is_symbol_id(text) => roles.push(text.to_owned()),
                _ => {
                    collector.push(diag::ref_unresolved(&id, "roles"));
                    return None;
                }
            }
        }
    }
    let roles = sorted_unique(roles, rule, collector);
    let mut ownership = Vec::new();
    if let Some(Json::Array(items)) = object.get("ownership") {
        if items.len() > limits::OWNERSHIP {
            collector.push(diag::limit_exceeded("ownership", items.len() as u64));
            return None;
        }
        for item in items {
            let Some(entry) = item.as_object() else {
                collector.push(diag::policy_invalid(&id, "ownership-shape"));
                return None;
            };
            let Some(resource_field) = string_property(entry, "resource_field") else {
                collector.push(diag::policy_invalid(&id, "ownership-shape"));
                return None;
            };
            if !is_field_name(resource_field) {
                collector.push(diag::policy_invalid(&id, "ownership-field"));
                return None;
            }
            let Some(actor_field) = enum_property(entry, "actor_field", |key| match key {
                "actor.id" => Some(ActorField::Id),
                "actor.tenant_id" => Some(ActorField::TenantId),
                "actor.workspace_id" => Some(ActorField::WorkspaceId),
                _ => None,
            }) else {
                collector.push(diag::policy_invalid(&id, "ownership-actor-field"));
                return None;
            };
            ownership.push(Ownership {
                resource_field: resource_field.to_owned(),
                actor_field,
            });
        }
    }
    let conditions = match object.get("conditions") {
        None | Some(Json::Null) => None,
        Some(condition) => {
            let mut nodes = 0usize;
            let mut depth = 0usize;
            match parse_predicate(condition, &mut nodes, &mut depth, 1) {
                Some(parsed) => Some(parsed),
                None => {
                    collector.push(diag::composition_invalid(&id, "condition-grammar"));
                    return None;
                }
            }
        }
    };
    let mut fields = FieldPermissions::default();
    let mut fields_present = false;
    if let Some(fields_value) = object.get("fields") {
        fields_present = true;
        let Some(fields_object) = fields_value.as_object() else {
            collector.push(diag::policy_invalid(&id, "fields-shape"));
            return None;
        };
        check_keys(fields_object, &["read", "write"], "fields-keys", collector);
        for (side, key) in [("read", &mut fields.read), ("write", &mut fields.write)] {
            match fields_object.get(side) {
                None | Some(Json::Null) => {}
                Some(Json::Array(items)) => {
                    if items.len() > limits::FIELDS {
                        collector.push(diag::limit_exceeded(side, items.len() as u64));
                        return None;
                    }
                    let mut parsed = Vec::new();
                    for item in items {
                        match item.as_str() {
                            Some(text) if is_field_name(text) => parsed.push(text.to_owned()),
                            _ => {
                                collector.push(diag::policy_invalid(&id, "field-name"));
                                return None;
                            }
                        }
                    }
                    *key = sorted_unique(parsed, rule, collector);
                }
                Some(_) => {
                    collector.push(diag::policy_invalid(&id, "fields-shape"));
                    return None;
                }
            }
        }
    }
    let fields = fields_present.then_some(fields);
    let composition = match object.get("composition") {
        None | Some(Json::Null) => None,
        Some(Json::String(text)) if is_symbol_id(text) => Some(text.to_owned()),
        Some(_) => {
            collector.push(diag::composition_invalid(&id, "composition-ref"));
            return None;
        }
    };
    Some(Policy {
        id,
        actor,
        decision,
        error_ref,
        applies_to,
        job,
        scope,
        capabilities,
        roles,
        ownership,
        conditions,
        fields,
        composition,
    })
}

/// Collect one composition declaration.
fn parse_composition(value: &Json, collector: &mut Collector) -> Option<Composition> {
    let object = value.as_object()?;
    check_keys(
        object,
        &["id", "all_of", "any_of"],
        "composition-keys",
        collector,
    );
    let id = string_property(object, "id")?.to_owned();
    if !is_symbol_id(&id) {
        collector.push(diag::document_invalid("composition-id", Some(&id)));
        return None;
    }
    let mut members = Vec::new();
    for side in ["all_of", "any_of"] {
        match object.get(side) {
            None | Some(Json::Null) => {}
            Some(Json::Array(items)) => {
                if items.len() > limits::COMPOSITIONS {
                    collector.push(diag::limit_exceeded(side, items.len() as u64));
                    return None;
                }
                for item in items {
                    match item.as_str() {
                        Some(text) if is_symbol_id(text) => members.push((side, text.to_owned())),
                        _ => {
                            collector.push(diag::composition_invalid(&id, "member-ref"));
                            return None;
                        }
                    }
                }
            }
            Some(_) => {
                collector.push(diag::composition_invalid(&id, "member-shape"));
                return None;
            }
        }
    }
    let all_of = sorted_unique(
        members
            .iter()
            .filter(|(side, _)| *side == "all_of")
            .map(|(_, id)| id.clone())
            .collect(),
        "authorization.policy-invalid",
        collector,
    );
    let any_of = sorted_unique(
        members
            .iter()
            .filter(|(side, _)| *side == "any_of")
            .map(|(_, id)| id.clone())
            .collect(),
        "authorization.policy-invalid",
        collector,
    );
    Some(Composition { id, all_of, any_of })
}

/// Collect one mapping-evidence record.
fn parse_mapping(value: &Json, collector: &mut Collector) -> Option<Mapping> {
    let object = value.as_object()?;
    check_keys(
        object,
        &[
            "target",
            "authorization_contract_ref",
            "semantic_policy_ref",
            "mapping_state",
            "evidence_ref",
            "observed_revision",
            "reason_refs",
        ],
        "mapping-keys",
        collector,
    );
    let target = string_property(object, "target")?.to_owned();
    if !is_clean_symbol(&target) {
        collector.push(diag::document_invalid("mapping-target", None));
        return None;
    }
    let semantic_policy_ref = string_property(object, "semantic_policy_ref")?.to_owned();
    if !is_symbol_id(&semantic_policy_ref) {
        collector.push(diag::ref_unresolved(&target, "semantic_policy_ref"));
        return None;
    }
    let Some(state) = enum_property(object, "mapping_state", MappingState::from_key) else {
        collector.push(diag::mapping_stale(&target, "unknown-state"));
        return None;
    };
    let evidence_ref = match object.get("evidence_ref") {
        None | Some(Json::Null) => None,
        Some(Json::String(text)) => {
            let ok = !text.is_empty()
                && text.len() <= 1024
                && !text.starts_with('/')
                && !text.contains('\\')
                && !text.contains(':')
                && !text.contains("..")
                && text.split('/').all(|segment| {
                    !segment.is_empty()
                        && segment.bytes().all(|b| {
                            b.is_ascii_lowercase()
                                || b.is_ascii_digit()
                                || matches!(b, b'.' | b'_' | b'-')
                        })
                });
            if !ok {
                collector.push(diag::document_invalid("evidence-path", None));
                return None;
            }
            Some(text.to_owned())
        }
        Some(_) => {
            collector.push(diag::document_invalid("evidence-path", None));
            return None;
        }
    };
    let observed_revision = string_property(object, "observed_revision")?.to_owned();
    let digest_ok = observed_revision.len() == 71
        && observed_revision.starts_with("sha256:")
        && observed_revision[7..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !digest_ok {
        collector.push(diag::mapping_stale(&target, "revision"));
        return None;
    }
    let mut reason_refs = Vec::new();
    if let Some(Json::Array(items)) = object.get("reason_refs") {
        if items.len() > limits::REASON_REFS {
            collector.push(diag::limit_exceeded("reason_refs", items.len() as u64));
            return None;
        }
        for item in items {
            match item.as_str() {
                Some(text) if is_symbol_id(text) => reason_refs.push(text.to_owned()),
                _ => {
                    collector.push(diag::document_invalid("reason-ref", None));
                    return None;
                }
            }
        }
    }
    let reason_refs = sorted_unique(reason_refs, "authorization.policy-invalid", collector);
    Some(Mapping {
        target,
        semantic_policy_ref,
        state,
        evidence_ref,
        observed_revision,
        reason_refs,
    })
}

/// Detect duplicate mapping records: one target/policy pair is declared
/// at most once.
fn mappings_sorted_unique(mut mappings: Vec<Mapping>, collector: &mut Collector) -> Vec<Mapping> {
    mappings.sort_by(|a, b| {
        (&a.target, &a.semantic_policy_ref).cmp(&(&b.target, &b.semantic_policy_ref))
    });
    let before = mappings.len();
    mappings
        .dedup_by(|a, b| a.target == b.target && a.semantic_policy_ref == b.semantic_policy_ref);
    if mappings.len() != before {
        collector.push(diag::policy_invalid(
            "authorization.policy-invalid",
            "duplicate-mapping",
        ));
    }
    mappings
}

impl Document {
    /// Parse the closed document from its JSON form. Every violation is
    /// a bounded registered diagnostic; the whole document is rejected
    /// when any diagnostic exists (fail-closed, no partial documents).
    pub fn from_json(value: &Json) -> Result<Self, DiagnosticSet> {
        let mut collector = Collector {
            diagnostics: Vec::new(),
        };
        let Some(object) = value.as_object() else {
            return Err(diag::invalid_set(
                diag::document_invalid("root-shape", None)
                    .into_iter()
                    .collect(),
            ));
        };
        check_keys(
            object,
            &[
                "schema_version",
                "identity",
                "model_ref",
                "profile_ref",
                "capabilities",
                "roles",
                "policies",
                "compositions",
                "mappings",
            ],
            "top-level-keys",
            &mut collector,
        );
        // Closed contract identity: unknown versions and identities are
        // invalid before anything else is inspected.
        if string_property(object, "schema_version") != Some(super::version::SCHEMA_VERSION) {
            collector.push(diag::document_invalid("schema-version", None));
        }
        if string_property(object, "identity") != Some(super::version::IDENTITY) {
            collector.push(diag::document_invalid("identity", None));
        }
        if string_property(object, "profile_ref") != Some(super::version::PROFILE_IDENTITY) {
            if let Some(profile) = string_property(object, "profile_ref") {
                collector.push(diag::profile_invalid(profile));
            } else {
                collector.push(diag::profile_invalid("missing"));
            }
        }
        let model_ref = match object.get("model_ref").and_then(Json::as_object) {
            Some(model) => {
                check_keys(
                    model,
                    &["schema_version", "ir_digest"],
                    "model-ref-keys",
                    &mut collector,
                );
                let schema_version = string_property(model, "schema_version")
                    .unwrap_or_default()
                    .to_owned();
                if schema_version != "0.1.0" && schema_version != "1.0.0" {
                    collector.push(diag::document_invalid("model-version", None));
                }
                let ir_digest = string_property(model, "ir_digest").unwrap_or_default();
                let digest_ok = ir_digest.len() == 71
                    && ir_digest.starts_with("sha256:")
                    && ir_digest[7..]
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
                if !digest_ok {
                    collector.push(diag::document_invalid("ir-digest", None));
                }
                ModelRef {
                    schema_version,
                    ir_digest: ir_digest.to_owned(),
                }
            }
            None => {
                collector.push(diag::document_invalid("model-ref", None));
                ModelRef {
                    schema_version: String::new(),
                    ir_digest: String::new(),
                }
            }
        };
        let mut capabilities = Vec::new();
        let mut roles = Vec::new();
        for (key, sink) in [("capabilities", &mut capabilities), ("roles", &mut roles)] {
            if let Some(Json::Array(items)) = object.get(key) {
                if items.len() > limits::SYMBOL_DECLS {
                    collector.push(diag::limit_exceeded(key, items.len() as u64));
                } else {
                    for item in items {
                        if let Some(decl) = parse_symbol_decl(item, &mut collector) {
                            sink.push(decl);
                        }
                    }
                }
            }
        }
        let mut policies = Vec::new();
        match object.get("policies") {
            Some(Json::Array(items)) => {
                if items.len() > limits::POLICIES {
                    collector.push(diag::limit_exceeded("policies", items.len() as u64));
                } else {
                    for item in items {
                        if let Some(policy) = parse_policy(item, &mut collector) {
                            policies.push(policy);
                        }
                    }
                }
            }
            _ => collector.push(diag::document_invalid("policies-shape", None)),
        }
        let mut compositions = Vec::new();
        if let Some(Json::Array(items)) = object.get("compositions") {
            if items.len() > limits::COMPOSITIONS {
                collector.push(diag::limit_exceeded("compositions", items.len() as u64));
            } else {
                for item in items {
                    if let Some(composition) = parse_composition(item, &mut collector) {
                        compositions.push(composition);
                    }
                }
            }
        }
        let mut mappings = Vec::new();
        if let Some(Json::Array(items)) = object.get("mappings") {
            if items.len() > limits::MAPPINGS {
                collector.push(diag::limit_exceeded("mappings", items.len() as u64));
            } else {
                for item in items {
                    if let Some(mapping) = parse_mapping(item, &mut collector) {
                        mappings.push(mapping);
                    }
                }
            }
        }
        if !collector.diagnostics.is_empty() {
            return Err(diag::invalid_set(collector.diagnostics));
        }
        let mut seen: Vec<&str> = policies.iter().map(|p| p.id.as_str()).collect();
        seen.sort_unstable();
        let policy_ids = seen;
        for composition in &compositions {
            if policy_ids.binary_search(&composition.id.as_str()).is_ok() {
                return Err(diag::invalid_set(
                    diag::composition_invalid(&composition.id, "id-collision")
                        .into_iter()
                        .collect(),
                ));
            }
        }
        let mut all_symbol_ids: Vec<String> = capabilities.iter().map(|d| d.id.clone()).collect();
        all_symbol_ids.extend(roles.iter().map(|d| d.id.clone()));
        all_symbol_ids.sort();
        all_symbol_ids.dedup();
        if all_symbol_ids.len() != capabilities.len() + roles.len() {
            return Err(diag::invalid_set(
                diag::policy_invalid("authorization.policy-invalid", "duplicate-symbol")
                    .into_iter()
                    .collect(),
            ));
        }
        capabilities.sort_by(|a, b| a.id.cmp(&b.id));
        roles.sort_by(|a, b| a.id.cmp(&b.id));
        policies.sort_by(|a, b| a.id.cmp(&b.id));
        {
            let before = policies.len();
            policies.dedup_by(|a, b| a.id == b.id);
            if policies.len() != before {
                return Err(diag::invalid_set(
                    diag::policy_invalid("authorization.policy-invalid", "duplicate-policy")
                        .into_iter()
                        .collect(),
                ));
            }
        }
        compositions.sort_by(|a, b| a.id.cmp(&b.id));
        {
            let before = compositions.len();
            compositions.dedup_by(|a, b| a.id == b.id);
            if compositions.len() != before {
                return Err(diag::invalid_set(
                    diag::composition_invalid(
                        "authorization.policy-invalid",
                        "duplicate-composition",
                    )
                    .into_iter()
                    .collect(),
                ));
            }
        }
        let mappings = mappings_sorted_unique(mappings, &mut collector);
        if !collector.diagnostics.is_empty() {
            return Err(diag::invalid_set(collector.diagnostics));
        }
        Ok(Document {
            model_ref,
            capabilities,
            roles,
            policies,
            compositions,
            mappings,
        })
    }
}
