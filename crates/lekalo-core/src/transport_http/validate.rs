//! Model-bound semantic validation of the transport-http attachment
//! (issue #70).
//!
//! [`validate`] joins one normalized attachment to the compiled
//! project and the cross-family contexts it declares against: every
//! endpoint reference resolves to a Model `endpoint` symbol whose
//! invoked operation resolves to a command or query; path-template
//! segments match the declared path parameters exactly in both
//! directions; parameter and body fields resolve to declared
//! operation inputs (command input members or query-model
//! parameters); the error map stays inside the bound #62 union with
//! strict completeness; security schemes resolve with the closed
//! actor pairing; pagination is consistent with the bound #64
//! declaration; scenario references resolve; and capability
//! declarations are satisfied by the resolved profile's capability
//! map. Pure and read-only: nothing here reads the filesystem,
//! executes anything, or evaluates a policy.
//!
//! Cross-family contexts are optional because the families are
//! independent: the checks each context owns are enforced exactly
//! when that context is bound, and the strict profile additionally
//! requires the capability map and complete error mappings.

use crate::diagnostics::DiagnosticSet;
use crate::error_contract::ErrorRegistry;
use crate::ir::{CompiledProject, Definition};
use crate::query_model::QueryModelAttachment;

use super::diagnostic;
use super::mapping::{check_mapping, idempotency_demand, IdempotencyDemand};
use super::types::{EndpointBinding, ProjectionMode};
use super::TransportDocument;

/// The closed support vocabulary of one resolved-profile capability
/// (the mirror of the target-profile component registry states).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProfileSupport {
    /// The profile does not provide the capability at all.
    Absent,
    /// The profile provides the capability partially.
    Partial,
    /// The profile provides the capability fully.
    Full,
}

impl ProfileSupport {
    /// The exact token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Partial => "partial",
            Self::Full => "full",
        }
    }

    /// Whether `self` satisfies a declaration requiring `minimum`.
    pub const fn satisfies(self, minimum: super::types::CapabilitySupport) -> bool {
        match (self, minimum) {
            (Self::Full, _) | (Self::Partial, super::types::CapabilitySupport::Partial) => true,
            (Self::Partial, super::types::CapabilitySupport::Full) | (Self::Absent, _) => false,
        }
    }
}

/// The resolved-profile capability map one validation runs against:
/// closed dotted ids to support states.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityMap {
    entries: Vec<(String, ProfileSupport)>,
}

impl CapabilityMap {
    /// Assemble from id-sorted pairs (crate internal).
    fn from_sorted(entries: Vec<(String, ProfileSupport)>) -> Self {
        Self { entries }
    }

    /// The default `http-json` transport component surface, mirrored
    /// exactly from the embedded target-profile component registry
    /// (`target_profile::component`, id `http-json`): `transport.http`
    /// full, and `transport.streaming`, `transport.upload`, and
    /// `transport.download` partial — so a declaration the published
    /// component registry claims is satisfiable on every validation
    /// path.
    pub fn http_json() -> Self {
        Self::from_sorted(vec![
            ("transport.http".to_owned(), ProfileSupport::Full),
            ("transport.streaming".to_owned(), ProfileSupport::Partial),
            ("transport.upload".to_owned(), ProfileSupport::Partial),
            ("transport.download".to_owned(), ProfileSupport::Partial),
        ])
    }

    /// The empty map: no capability has any support.
    pub fn empty() -> Self {
        Self::from_sorted(Vec::new())
    }

    /// The support of one capability id, or `Absent`.
    pub fn support(&self, id: &str) -> ProfileSupport {
        self.entries
            .iter()
            .find(|(capability, _)| capability == id)
            .map(|(_, support)| *support)
            .unwrap_or(ProfileSupport::Absent)
    }
}

/// The validation context: the compiled project plus every optional
/// cross-family context the attachment declares against.
pub struct ValidationContext<'a> {
    /// The compiled project the attachment binds.
    pub project: &'a CompiledProject,
    /// The bound #62 error registry, when one is supplied.
    pub errors: Option<&'a ErrorRegistry>,
    /// The bound #64 query-model attachment, when one is supplied.
    pub query_model: Option<&'a QueryModelAttachment>,
    /// The resolved-profile capability map, when one is supplied.
    pub capabilities: Option<&'a CapabilityMap>,
    /// The strict profile: complete error mappings and the
    /// capability map are required.
    pub strict: bool,
}

impl<'a> ValidationContext<'a> {
    /// The default-profile context over one compiled project.
    pub fn new(project: &'a CompiledProject) -> Self {
        Self {
            project,
            errors: None,
            query_model: None,
            capabilities: None,
            strict: false,
        }
    }

    /// Bind the #62 error registry.
    pub fn with_errors(mut self, errors: &'a ErrorRegistry) -> Self {
        self.errors = Some(errors);
        self
    }

    /// Bind the #64 query-model attachment.
    pub fn with_query_model(mut self, query_model: &'a QueryModelAttachment) -> Self {
        self.query_model = Some(query_model);
        self
    }

    /// Bind the resolved-profile capability map.
    pub fn with_capabilities(mut self, capabilities: &'a CapabilityMap) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Select the strict profile.
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }
}

/// Check only the capability declarations of one attachment against
/// a resolved-profile capability map: every declared capability must
/// be satisfied. Pure and read-only; used by the conformance suite
/// where no project binding exists.
pub fn validate_capabilities(
    document: &TransportDocument,
    map: &CapabilityMap,
) -> Result<(), DiagnosticSet> {
    for binding in document.endpoints() {
        for decl in &binding.capabilities {
            let actual = map.support(decl.capability.profile_capability());
            if !actual.satisfies(decl.minimum_support) {
                return Err(diagnostic::capability_unsatisfied(
                    binding.endpoint.as_str(),
                    decl.capability.profile_capability(),
                    decl.minimum_support.as_str(),
                    actual.as_str(),
                ));
            }
        }
    }
    Ok(())
}

/// Validate one attachment against the context. Pure and read-only;
/// the first contradiction returns its typed registered refusal.
pub fn validate(
    document: &TransportDocument,
    context: &ValidationContext,
) -> Result<(), DiagnosticSet> {
    // Custody: the attachment names exactly the compiled project.
    let project_id = context
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str())
        .unwrap_or_default();
    if document.project_id().as_str() != project_id {
        return Err(diagnostic::rule_invalid(
            diagnostic::CONTRACT_INVALID,
            "project-id",
            Some(document.project_id().as_str()),
        ));
    }
    if document.model_ref().version().as_str() != context.project.model_version.as_str() {
        return Err(diagnostic::rule_invalid(
            diagnostic::CONTRACT_INVALID,
            "model-version",
            Some(document.model_ref().version().as_str()),
        ));
    }
    let symbols = symbol_table(context.project);
    for binding in document.endpoints() {
        validate_endpoint(document, binding, &symbols, context)?;
    }
    Ok(())
}

/// The id-sorted symbol table of one compiled project.
fn symbol_table(project: &CompiledProject) -> std::collections::BTreeMap<&str, &Definition> {
    let mut symbols = std::collections::BTreeMap::new();
    for definition in &project.definitions {
        symbols.insert(id_of(definition), definition);
    }
    symbols
}

/// The semantic id of one definition.
fn id_of(definition: &Definition) -> &str {
    match definition {
        Definition::Scalar(def) => def.id.as_str(),
        Definition::Enum(def) => def.id.as_str(),
        Definition::ValueObject(def) => def.id.as_str(),
        Definition::Entity(def) => def.id.as_str(),
        Definition::Command(def) => def.id.as_str(),
        Definition::Query(def) => def.id.as_str(),
        Definition::Policy(def) => def.id.as_str(),
        Definition::Event(def) => def.id.as_str(),
        Definition::Effect(def) => def.id.as_str(),
        Definition::Endpoint(def) => def.id.as_str(),
        Definition::Scenario(def) => def.id.as_str(),
        Definition::TargetBinding(def) => def.id.as_str(),
    }
}

/// Validate one endpoint binding.
fn validate_endpoint(
    document: &TransportDocument,
    binding: &EndpointBinding,
    symbols: &std::collections::BTreeMap<&str, &Definition>,
    context: &ValidationContext,
) -> Result<(), DiagnosticSet> {
    let subject = binding.endpoint.as_str();
    let endpoint = symbols.get(subject).ok_or_else(|| {
        diagnostic::rule_invalid(
            diagnostic::ENDPOINT_UNRESOLVED,
            "symbol-missing",
            Some(subject),
        )
    })?;
    let Definition::Endpoint(endpoint) = endpoint else {
        return Err(diagnostic::rule_invalid(
            diagnostic::ENDPOINT_UNRESOLVED,
            "symbol-kind",
            Some(subject),
        ));
    };
    let operation = symbols.get(endpoint.invokes.as_str()).ok_or_else(|| {
        diagnostic::rule_invalid(
            diagnostic::ENDPOINT_UNRESOLVED,
            "invokes-unresolved",
            Some(subject),
        )
    })?;
    let (input_fields, is_query) = match operation {
        Definition::Command(command) => (Some(&command.input), false),
        Definition::Query(_) => (None, true),
        _ => {
            return Err(diagnostic::rule_invalid(
                diagnostic::ENDPOINT_UNRESOLVED,
                "invokes-kind",
                Some(subject),
            ));
        }
    };

    // Path-template agreement: the declared path parameters bind
    // exactly the templated segments, in both directions, under one
    // canonical wire spelling.
    let template = path_template_names(endpoint.path.as_str());
    for name in &template {
        let bound = binding
            .params
            .iter()
            .any(|param| param.is_path() && param.name.as_str() == name.as_str());
        if !bound {
            return Err(diagnostic::rule_invalid(
                diagnostic::PARAM_INVALID,
                "path-param-unbound",
                Some(subject),
            ));
        }
    }
    for param in binding.params.iter().filter(|param| param.is_path()) {
        if !template.iter().any(|name| name == param.name.as_str()) {
            return Err(diagnostic::rule_invalid(
                diagnostic::PARAM_INVALID,
                "path-param-unused",
                Some(subject),
            ));
        }
    }

    // Parameter and body fields resolve to declared operation
    // inputs: command input members (`input.<name>`) or declared
    // query-model parameters (bare names). A query endpoint with
    // field references but no bound query model cannot prove its
    // inputs and refuses.
    let query_decl = if is_query {
        context
            .query_model
            .and_then(|model| model.query(endpoint.invokes.as_str()))
    } else {
        None
    };
    let field_resolves = |field: &super::id::FieldRef| -> Option<bool> {
        if let Some(fields) = input_fields {
            Some(
                field.is_input()
                    && fields
                        .iter()
                        .any(|candidate| candidate.name.as_str() == field.name().as_str()),
            )
        } else if let Some(decl) = query_decl {
            Some(
                !field.is_input()
                    && decl
                        .parameters
                        .iter()
                        .any(|candidate| candidate.name.as_str() == field.name().as_str()),
            )
        } else if field.is_input() {
            // A command-style reference on a query endpoint is
            // always checkable and always wrong.
            Some(false)
        } else {
            // A query-model parameter reference without a bound query
            // model is unchecked, not unresolvable: the cross-family
            // check runs exactly when the family is bound.
            None
        }
    };
    for param in &binding.params {
        if field_resolves(&param.field) == Some(false) {
            return Err(diagnostic::rule_invalid(
                diagnostic::PARAM_INVALID,
                "field-unresolved",
                Some(subject),
            ));
        }
    }
    if let Some(body) = &binding.body {
        for field in &body.fields {
            if field_resolves(&field.field) == Some(false) {
                return Err(diagnostic::rule_invalid(
                    diagnostic::PARAM_INVALID,
                    "field-unresolved",
                    Some(subject),
                ));
            }
        }
        // Bodyless methods carry no whole-input body; only an
        // explicit field subset opts a GET or DELETE into a body.
        if body.mode == ProjectionMode::Whole
            && matches!(endpoint.method.as_str(), "GET" | "DELETE")
        {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "bodyless-method",
                Some(subject),
            ));
        }
    }

    // The error map and the idempotency demand are checked against
    // the bound #62 registry when one is supplied.
    let operation_binding = context.errors.and_then(|registry| {
        let id = crate::error_contract::id::ErrorId::new(endpoint.invokes.as_str())?;
        registry.binding(&id)
    });
    // The error map is checked against the bound #62 registry when
    // one is supplied; without one, declared entries are
    // accepted-but-unchecked (the union check runs exactly when the
    // family is bound). The strict profile requires the registry
    // whenever any entry is declared.
    if !binding.errors.is_empty() {
        if let Some(registry) = context.errors {
            match operation_binding {
                Some(operation) => check_mapping(
                    operation,
                    registry,
                    &binding.errors,
                    context.strict,
                    subject,
                )?,
                None => {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CONTRACT_INVALID,
                        "operation-unbound",
                        Some(subject),
                    ));
                }
            }
        } else if context.strict {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "errors-registry-absent",
                Some(subject),
            ));
        }
    }
    if context.strict {
        if let Some(registry) = context.errors {
            match operation_binding {
                Some(operation) => {
                    check_mapping(operation, registry, &binding.errors, true, subject)?
                }
                None => {
                    if !binding.errors.is_empty() {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::CONTRACT_INVALID,
                            "operation-unbound",
                            Some(subject),
                        ));
                    }
                }
            }
        }
    }
    // The idempotency demand: a command endpoint whose bound #62
    // metadata declares `key-required` must carry the binding — both
    // the forced `required` state and the presence of the whole member
    // are enforced (an absent binding would hide the forced header
    // from every consumer projecting the route); `not-applicable`
    // forbids a declared key. Queries are not required to carry the
    // binding, but a declared binding is still checked: an endpoint
    // may opt a safe operation out of the demand by omitting the
    // member, never by declaring it dishonestly.
    let demand = demand_of(operation_binding, context);
    match (&binding.idempotency, demand) {
        (None, IdempotencyDemand::KeyRequired) if !is_query => {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "idempotency-required-missing",
                Some(subject),
            ));
        }
        (Some(idempotency), _) => match (idempotency.required, demand) {
            (false, IdempotencyDemand::KeyRequired) => {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CONTRACT_INVALID,
                    "idempotency-required-missing",
                    Some(subject),
                ));
            }
            (true, IdempotencyDemand::NotApplicable) => {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CONTRACT_INVALID,
                    "idempotency-not-applicable",
                    Some(subject),
                ));
            }
            _ => {}
        },
        (None, _) => {}
    }

    // Security: schemes resolve, the actor pairing is closed, and
    // the policy reference resolves to a Model policy symbol.
    if let Some(auth) = &binding.auth {
        if let Some(policy) = &auth.policy_ref {
            let resolved = symbols.get(policy.as_str()).ok_or_else(|| {
                diagnostic::rule_invalid(
                    diagnostic::SECURITY_INVALID,
                    "policy-unresolved",
                    Some(subject),
                )
            })?;
            if !matches!(resolved, Definition::Policy(_)) {
                return Err(diagnostic::rule_invalid(
                    diagnostic::SECURITY_INVALID,
                    "policy-kind",
                    Some(subject),
                ));
            }
        }
        for scheme_ref in &auth.schemes {
            let scheme = document.scheme(scheme_ref.as_str()).ok_or_else(|| {
                diagnostic::rule_invalid(
                    diagnostic::SECURITY_INVALID,
                    "scheme-unresolved",
                    Some(subject),
                )
            })?;
            if scheme.kind == super::types::SchemeKind::None {
                return Err(diagnostic::rule_invalid(
                    diagnostic::SECURITY_INVALID,
                    "scheme-kind",
                    Some(subject),
                ));
            }
        }
        let has_schemes = !auth.schemes.is_empty();
        if auth.actor.requires_scheme() != has_schemes {
            return Err(diagnostic::rule_invalid(
                diagnostic::SECURITY_INVALID,
                if auth.actor.requires_scheme() {
                    "scheme-missing"
                } else {
                    "public-with-scheme"
                },
                Some(subject),
            ));
        }
    }

    // A command endpoint carries explicit security or refuses.
    if binding.auth.is_none() && !is_query {
        return Err(diagnostic::rule_invalid(
            diagnostic::SECURITY_INVALID,
            "auth-absent",
            Some(subject),
        ));
    }

    // Pagination: only queries, and — with the query model bound —
    // consistent with the #64 declaration.
    if let Some(pagination) = &binding.pagination {
        if !is_query {
            return Err(diagnostic::rule_invalid(
                diagnostic::PAGINATION_INVALID,
                "operation-kind",
                Some(subject),
            ));
        }
        if let Some(model) = context.query_model {
            let Some(decl) = model.query(endpoint.invokes.as_str()) else {
                return Err(diagnostic::rule_invalid(
                    diagnostic::PAGINATION_INVALID,
                    "query-unbound",
                    Some(subject),
                ));
            };
            let cardinality = decl.cardinality;
            if !cardinality.admits_pagination() {
                return Err(diagnostic::rule_invalid(
                    diagnostic::PAGINATION_INVALID,
                    "cardinality",
                    Some(subject),
                ));
            }
            let Some(declared) = decl.pagination.as_ref() else {
                return Err(diagnostic::rule_invalid(
                    diagnostic::PAGINATION_INVALID,
                    "pagination-absent",
                    Some(subject),
                ));
            };
            use crate::query_model::PaginationStrategy;
            let styles_match = matches!(
                (pagination.style, declared.strategy),
                (
                    super::types::PaginationStyle::Offset,
                    PaginationStrategy::Offset
                ) | (
                    super::types::PaginationStyle::Cursor,
                    PaginationStrategy::Cursor
                )
            );
            if !styles_match {
                return Err(diagnostic::rule_invalid(
                    diagnostic::PAGINATION_INVALID,
                    "style-mismatch",
                    Some(subject),
                ));
            }
            if pagination.style == super::types::PaginationStyle::Cursor {
                let key = declared
                    .key_parameter
                    .as_ref()
                    .map(|key| key.as_str())
                    .unwrap_or_default();
                if key.is_empty()
                    || pagination
                        .cursor_param
                        .as_ref()
                        .map_or(true, |param| param.as_str() != key)
                {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::PAGINATION_INVALID,
                        "cursor-param-mismatch",
                        Some(subject),
                    ));
                }
            }
        }
    }

    // Scenario coverage references resolve to Model scenario symbols.
    for scenario in &binding.scenarios {
        let resolved = symbols.get(scenario.as_str()).ok_or_else(|| {
            diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "scenario-unresolved",
                Some(subject),
            )
        })?;
        if !matches!(resolved, Definition::Scenario(_)) {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "scenario-kind",
                Some(subject),
            ));
        }
    }

    // Capability declarations are checked against the resolved
    // profile; strict requires the map when any capability is
    // declared.
    if !binding.capabilities.is_empty() {
        let Some(map) = context.capabilities else {
            if context.strict {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CONTRACT_INVALID,
                    "capability-map-absent",
                    Some(subject),
                ));
            }
            return Ok(());
        };
        for decl in &binding.capabilities {
            let actual = map.support(decl.capability.profile_capability());
            if !actual.satisfies(decl.minimum_support) {
                return Err(diagnostic::capability_unsatisfied(
                    subject,
                    decl.capability.profile_capability(),
                    decl.minimum_support.as_str(),
                    actual.as_str(),
                ));
            }
        }
    }
    Ok(())
}

/// The idempotency demand of one operation binding against the bound
/// registry.
fn demand_of<'a>(
    binding: Option<&'a crate::error_contract::types::OperationErrorContract>,
    context: &ValidationContext<'a>,
) -> IdempotencyDemand {
    match context.errors {
        Some(registry) => idempotency_demand(binding, registry),
        None => IdempotencyDemand::Open,
    }
}

/// Extract the templated segment names of one Model endpoint path:
/// both `{name}` and `:name` spellings normalize to the one bare
/// name. Every templated segment must be bound by exactly one
/// declared path parameter and vice versa.
pub(crate) fn path_template_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            segment
                .strip_prefix('{')
                .and_then(|rest| rest.strip_suffix('}'))
                .or_else(|| segment.strip_prefix(':'))
                .map(|name| name.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_templates_normalize_both_spellings() {
        assert_eq!(
            path_template_names("/tasks/{task_id}/focus"),
            vec!["task_id".to_owned()]
        );
        assert_eq!(
            path_template_names("/tasks/:task_id/focus"),
            vec!["task_id".to_owned()]
        );
        assert!(path_template_names("/tasks/focus").is_empty());
    }

    #[test]
    fn support_states_satisfy_only_their_minimum() {
        use super::super::types::CapabilitySupport;
        assert!(ProfileSupport::Full.satisfies(CapabilitySupport::Full));
        assert!(ProfileSupport::Full.satisfies(CapabilitySupport::Partial));
        assert!(ProfileSupport::Partial.satisfies(CapabilitySupport::Partial));
        assert!(!ProfileSupport::Partial.satisfies(CapabilitySupport::Full));
        assert!(!ProfileSupport::Absent.satisfies(CapabilitySupport::Partial));
    }

    #[test]
    fn the_http_json_default_map_is_the_published_surface() {
        let map = CapabilityMap::http_json();
        assert_eq!(map.support("transport.http"), ProfileSupport::Full);
        assert_eq!(map.support("transport.streaming"), ProfileSupport::Partial);
        assert_eq!(map.support("transport.upload"), ProfileSupport::Partial);
        assert_eq!(map.support("transport.download"), ProfileSupport::Partial);
    }

    #[test]
    fn the_http_json_default_map_mirrors_the_component_registry() {
        // Cross-module coherence: the published http-json component
        // (issue #70 plan S9) claims exactly the transport capabilities
        // the default validation map admits, state for state.
        let component = crate::target_profile::component::definition(
            crate::target_profile::component::Axis::Transport,
            "http-json",
        )
        .expect("the http-json component is registered");
        let map = CapabilityMap::http_json();
        for provided in component.provides {
            let expected = match provided.support {
                crate::target_profile::component::Support::Partial => ProfileSupport::Partial,
                crate::target_profile::component::Support::Full => ProfileSupport::Full,
            };
            assert_eq!(map.support(provided.id), expected, "{}", provided.id);
        }
    }
}
