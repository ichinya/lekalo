//! The deterministic per-runtime route surface projection (issue #70).
//!
//! [`project`] derives one canonical route surface per closed namespace
//! (`node`, `laravel`, `go`, `rust`) from the single transport
//! attachment joined with the Model endpoint symbols: every namespace
//! renders the same canonical decode/encode/error/security contract
//! (which is what the issue #46 OpenAPI projection consumes and what
//! makes cross-runtime parity checkable by byte comparison) plus a
//! namespace-shaped handler identity. The projection is pure
//! declaration data — byte-stable, deterministic, never executing
//! anything, never inventing a field the attachment does not declare.

use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::types::{
    ApiVersionBinding, AuthBinding, BodyBinding, CacheBinding, CapabilityDecl, CorrelationBinding,
    EndpointBinding, IdempotencyBinding, PaginationBinding, ParamBinding, RateLimitBinding,
    SuccessBinding,
};
use super::{TransportDocument, ValidationContext};
use crate::diagnostics::DiagnosticSet;
use crate::ir::{CompiledProject, Definition};

/// The closed node namespace token.
pub const NAMESPACE_NODE: &str = "node";
/// The closed laravel namespace token.
pub const NAMESPACE_LARAVEL: &str = "laravel";
/// The closed go namespace token.
pub const NAMESPACE_GO: &str = "go";
/// The closed rust namespace token.
pub const NAMESPACE_RUST: &str = "rust";

/// Every closed namespace, byte-sorted.
pub const NAMESPACES: [&str; 4] = [
    NAMESPACE_GO,
    NAMESPACE_LARAVEL,
    NAMESPACE_NODE,
    NAMESPACE_RUST,
];

/// One finished route surface: the canonical JSON text plus its
/// namespace identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteSurface {
    namespace: String,
    bytes: String,
}

impl RouteSurface {
    /// The namespace this surface renders.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The canonical bytes (compact JSON, byte-sorted keys, no
    /// trailing LF).
    pub fn canonical_bytes(&self) -> &str {
        &self.bytes
    }
}

/// Project one attachment into the route surface of one closed
/// namespace. The attachment must first validate against the bound
/// project (the projection joins the Model endpoint symbols); an
/// unresolvable endpoint refuses rather than guessing.
pub fn project(
    document: &TransportDocument,
    context: &ValidationContext<'_>,
    namespace: &str,
) -> Result<RouteSurface, DiagnosticSet> {
    if !NAMESPACES.contains(&namespace) {
        return Err(diagnostic::rule_invalid(
            diagnostic::CONTRACT_INVALID,
            "namespace-unknown",
            Some(namespace),
        ));
    }
    let project = context.project;
    let mut routes = Vec::with_capacity(document.endpoints().len());
    for binding in document.endpoints() {
        routes.push(route_json(document, binding, project, namespace)?);
    }
    let mut root = Map::new();
    root.insert("namespace".to_owned(), Json::String(namespace.to_owned()));
    root.insert(
        "projectId".to_owned(),
        Json::String(document.project_id().as_str().to_owned()),
    );
    root.insert(
        "wire".to_owned(),
        Json::String(document.wire().dialect.to_owned()),
    );
    root.insert(
        "errorEnvelope".to_owned(),
        Json::String(document.defaults().error_envelope.to_owned()),
    );
    root.insert("routes".to_owned(), Json::Array(routes));
    Ok(RouteSurface {
        namespace: namespace.to_owned(),
        bytes: Json::Object(root).to_string(),
    })
}

/// One canonical route entry.
fn route_json(
    document: &TransportDocument,
    binding: &EndpointBinding,
    project: &CompiledProject,
    namespace: &str,
) -> Result<Json, DiagnosticSet> {
    let subject = binding.endpoint.as_str();
    let endpoint = project
        .definitions
        .iter()
        .find_map(|definition| match definition {
            Definition::Endpoint(def) if def.id.as_str() == subject => Some(def),
            _ => None,
        })
        .ok_or_else(|| {
            diagnostic::rule_invalid(
                diagnostic::ENDPOINT_UNRESOLVED,
                "symbol-missing",
                Some(subject),
            )
        })?;
    let module = subject.split('.').next().unwrap_or_default();
    let operation = endpoint.invokes.as_str();
    let tail = operation.split('.').next_back().unwrap_or_default();
    let project_root = handler_root(document.project_id().as_str());

    let mut route = Map::new();
    route.insert(
        "operationId".to_owned(),
        Json::String(binding.effective_operation_id().as_str().to_owned()),
    );
    route.insert(
        "method".to_owned(),
        Json::String(endpoint.method.as_str().to_owned()),
    );
    route.insert(
        "pathTemplate".to_owned(),
        Json::String(endpoint.path.as_str().to_owned()),
    );
    route.insert("invokes".to_owned(), Json::String(operation.to_owned()));
    // The namespace-shaped handler identity.
    route.insert(
        "handler".to_owned(),
        Json::String(handler_identity(namespace, &project_root, module, tail)),
    );
    // The canonical decode plan: params plus the body mode.
    let mut decode = Map::new();
    decode.insert(
        "params".to_owned(),
        Json::Array(binding.params.iter().map(param_json).collect()),
    );
    if let Some(body) = &binding.body {
        decode.insert("body".to_owned(), body_json(body));
    }
    route.insert("decode".to_owned(), Json::Object(decode));

    // The canonical encode plan: the success projection, the error
    // map (identity-keyed, status-projected), and the category
    // defaults — everything an OpenAPI rendering needs.
    let mut encode = Map::new();
    encode.insert("success".to_owned(), success_json(&binding.success));
    if !binding.errors.is_empty() {
        encode.insert(
            "errors".to_owned(),
            Json::Array(
                binding
                    .errors
                    .iter()
                    .map(|entry| {
                        let mut object = Map::new();
                        object.insert(
                            "error".to_owned(),
                            Json::String(entry.error.as_str().to_owned()),
                        );
                        object.insert("status".to_owned(), Json::from(entry.status));
                        Json::Object(object)
                    })
                    .collect(),
            ),
        );
    }
    encode.insert(
        "errorDefaults".to_owned(),
        error_defaults_json(&binding.error_defaults),
    );
    route.insert("encode".to_owned(), Json::Object(encode));

    // The security projection.
    if let Some(auth) = &binding.auth {
        route.insert("security".to_owned(), auth_json(auth));
    }
    // The declared headers the runtime must honor.
    let mut headers: Vec<String> = Vec::new();
    if let Some(idempotency) = &binding.idempotency {
        headers.push(idempotency_json(idempotency));
    }
    if let Some(correlation) = &binding.correlation {
        headers.push(correlation_json(correlation));
    }
    if !headers.is_empty() {
        headers.sort();
        route.insert(
            "headers".to_owned(),
            Json::Array(headers.into_iter().map(Json::String).collect()),
        );
    }
    if let Some(pagination) = &binding.pagination {
        route.insert("pagination".to_owned(), pagination_json(pagination));
    }
    if let Some(rate_limit) = &binding.rate_limit {
        route.insert("rateLimit".to_owned(), rate_limit_json(rate_limit));
    }
    if let Some(cache) = &binding.cache {
        route.insert("cache".to_owned(), cache_json(cache));
    }
    if let Some(api_version) = &binding.api_version {
        route.insert("apiVersion".to_owned(), api_version_json(api_version));
    }
    if !binding.capabilities.is_empty() {
        route.insert(
            "capabilities".to_owned(),
            Json::Array(
                binding
                    .capabilities
                    .iter()
                    .map(|decl| capability_json(decl, document, binding))
                    .collect(),
            ),
        );
    }
    Ok(Json::Object(route))
}

/// The project root segment of the handler identity: the PascalCase
/// spelling of the project id (`planner` -> `Planner`), so every
/// project namespaces its handlers under its own root, never a
/// hardcoded one.
fn handler_root(project_id: &str) -> String {
    let mut root = String::with_capacity(project_id.len());
    let mut new_word = true;
    for character in project_id.chars() {
        if character.is_ascii_alphanumeric() {
            if new_word {
                root.extend(character.to_uppercase());
                new_word = false;
            } else {
                root.push(character);
            }
        } else {
            new_word = true;
        }
    }
    root
}

/// The namespace-shaped handler identity: one deterministic spelling
/// per namespace over the project root, the module, and the operation
/// tail.
fn handler_identity(namespace: &str, project_root: &str, module: &str, tail: &str) -> String {
    match namespace {
        NAMESPACE_LARAVEL => format!("{project_root}/{module}/{tail}Controller"),
        NAMESPACE_GO => format!("{module}.{tail}Handler"),
        NAMESPACE_RUST => format!("{module}::{tail}Route"),
        // node and anything else: the module-relative handler module.
        _ => format!("{module}/{tail}.handler"),
    }
}

/// One canonical decode parameter.
fn param_json(param: &ParamBinding) -> Json {
    let mut object = Map::new();
    object.insert(
        "name".to_owned(),
        Json::String(param.name.as_str().to_owned()),
    );
    object.insert(
        "in".to_owned(),
        Json::String(param.location.as_str().to_owned()),
    );
    object.insert("field".to_owned(), Json::String(param.field.as_str()));
    object.insert("required".to_owned(), Json::Bool(param.required));
    if let Some(style) = param.style {
        object.insert("style".to_owned(), Json::String(style.as_str().to_owned()));
    }
    Json::Object(object)
}

/// One canonical body projection.
fn body_json(body: &BodyBinding) -> Json {
    let mut object = Map::new();
    object.insert(
        "mode".to_owned(),
        Json::String(body.mode.body_str().to_owned()),
    );
    if !body.fields.is_empty() {
        object.insert(
            "fields".to_owned(),
            Json::Array(
                body.fields
                    .iter()
                    .map(|field| {
                        let mut entry = Map::new();
                        entry.insert(
                            "name".to_owned(),
                            Json::String(field.name.as_str().to_owned()),
                        );
                        entry.insert("field".to_owned(), Json::String(field.field.as_str()));
                        entry.insert("required".to_owned(), Json::Bool(field.required));
                        Json::Object(entry)
                    })
                    .collect(),
            ),
        );
    }
    Json::Object(object)
}

/// One canonical success projection.
fn success_json(success: &SuccessBinding) -> Json {
    let mut object = Map::new();
    object.insert("status".to_owned(), Json::from(success.status));
    if let Some(body) = &success.body {
        let mut entry = Map::new();
        entry.insert(
            "mode".to_owned(),
            Json::String(body.mode.response_str().to_owned()),
        );
        if !body.fields.is_empty() {
            entry.insert(
                "fields".to_owned(),
                Json::Array(
                    body.fields
                        .iter()
                        .map(|field| {
                            let mut member = Map::new();
                            member.insert(
                                "name".to_owned(),
                                Json::String(field.name.as_str().to_owned()),
                            );
                            member.insert("field".to_owned(), Json::String(field.field.as_str()));
                            member.insert("required".to_owned(), Json::Bool(field.required));
                            Json::Object(member)
                        })
                        .collect(),
                ),
            );
        }
        object.insert("body".to_owned(), Json::Object(entry));
    }
    Json::Object(object)
}

/// The canonical error defaults.
fn error_defaults_json(defaults: &super::types::ErrorDefaults) -> Json {
    let mut object = Map::new();
    object.insert("validation".to_owned(), Json::from(defaults.validation));
    object.insert("auth".to_owned(), Json::from(defaults.auth));
    object.insert("conflict".to_owned(), Json::from(defaults.conflict));
    object.insert("not-found".to_owned(), Json::from(defaults.not_found));
    object.insert("domain".to_owned(), Json::from(defaults.domain));
    object.insert(
        "infrastructure".to_owned(),
        Json::from(defaults.infrastructure),
    );
    Json::Object(object)
}

/// One canonical security projection.
fn auth_json(auth: &AuthBinding) -> Json {
    let mut object = Map::new();
    object.insert(
        "actor".to_owned(),
        Json::String(auth.actor.as_str().to_owned()),
    );
    object.insert(
        "schemes".to_owned(),
        Json::Array(
            auth.schemes
                .iter()
                .map(|scheme| Json::String(scheme.as_str().to_owned()))
                .collect(),
        ),
    );
    if let Some(policy) = &auth.policy_ref {
        object.insert(
            "policyRef".to_owned(),
            Json::String(policy.as_str().to_owned()),
        );
    }
    Json::Object(object)
}

/// The idempotency header declaration with its requirement.
fn idempotency_json(idempotency: &IdempotencyBinding) -> String {
    format!(
        "{}:{}",
        idempotency.header.as_str(),
        if idempotency.required {
            "required"
        } else {
            "optional"
        }
    )
}

/// The correlation headers declaration.
fn correlation_json(correlation: &CorrelationBinding) -> String {
    format!(
        "correlation:{}",
        correlation
            .headers
            .iter()
            .map(|header| header.as_str())
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// One canonical pagination projection.
fn pagination_json(pagination: &PaginationBinding) -> Json {
    let mut object = Map::new();
    object.insert(
        "style".to_owned(),
        Json::String(pagination.style.as_str().to_owned()),
    );
    object.insert(
        "limitParam".to_owned(),
        Json::String(pagination.limit_param.as_str().to_owned()),
    );
    if let Some(offset) = &pagination.offset_param {
        object.insert(
            "offsetParam".to_owned(),
            Json::String(offset.as_str().to_owned()),
        );
    }
    if let Some(cursor) = &pagination.cursor_param {
        object.insert(
            "cursorParam".to_owned(),
            Json::String(cursor.as_str().to_owned()),
        );
    }
    if let Some(field) = &pagination.cursor_field {
        object.insert(
            "cursorField".to_owned(),
            Json::String(field.as_str().to_owned()),
        );
    }
    Json::Object(object)
}

/// One canonical rate-limit projection.
fn rate_limit_json(rate_limit: &RateLimitBinding) -> Json {
    let mut object = Map::new();
    object.insert("limit".to_owned(), Json::from(rate_limit.limit));
    object.insert(
        "windowSeconds".to_owned(),
        Json::from(rate_limit.window_seconds),
    );
    object.insert(
        "scope".to_owned(),
        Json::String(rate_limit.scope.as_str().to_owned()),
    );
    Json::Object(object)
}

/// One canonical cache projection.
fn cache_json(cache: &CacheBinding) -> Json {
    let mut object = Map::new();
    object.insert(
        "policy".to_owned(),
        Json::String(cache.policy.as_str().to_owned()),
    );
    object.insert(
        "maxAgeSeconds".to_owned(),
        Json::from(cache.max_age_seconds),
    );
    object.insert("etag".to_owned(), Json::Bool(cache.etag));
    Json::Object(object)
}

/// One canonical api-version projection.
fn api_version_json(api_version: &ApiVersionBinding) -> Json {
    let mut object = Map::new();
    object.insert(
        "in".to_owned(),
        Json::String(api_version.location.as_str().to_owned()),
    );
    object.insert(
        "name".to_owned(),
        Json::String(api_version.name.as_str().to_owned()),
    );
    Json::Object(object)
}

/// One canonical capability declaration with its resolved profile
/// support: the surface carries the declared minimum, and an
/// unsupported capability stays explicit — never dropped.
fn capability_json(
    decl: &CapabilityDecl,
    _document: &TransportDocument,
    _binding: &EndpointBinding,
) -> Json {
    let mut object = Map::new();
    object.insert(
        "capability".to_owned(),
        Json::String(decl.capability.as_str().to_owned()),
    );
    object.insert(
        "minimumSupport".to_owned(),
        Json::String(decl.minimum_support.as_str().to_owned()),
    );
    object.insert(
        "detail".to_owned(),
        Json::String(decl.detail.as_str().to_owned()),
    );
    Json::Object(object)
}

#[cfg(test)]
mod tests {
    use super::{handler_identity, handler_root};

    #[test]
    fn the_laravel_root_is_derived_from_the_project_id() {
        // The planner fixture spells the committed golden root.
        assert_eq!(handler_root("planner"), "Planner");
        // Every other project namespaces under its own root.
        assert_eq!(handler_root("billing_gateway"), "BillingGateway");
        assert_eq!(handler_root("content-hub"), "ContentHub");
        assert_eq!(handler_root("ledger2"), "Ledger2");
        // No hardcoded Planner root leaks into a foreign project.
        assert_ne!(handler_root("inventory"), "Planner");
    }

    #[test]
    fn the_laravel_handler_identity_uses_the_project_root() {
        assert_eq!(
            handler_identity("laravel", "BillingGateway", "billing", "issue"),
            "BillingGateway/billing/issueController"
        );
        assert_eq!(
            handler_identity("laravel", "Planner", "planner", "focus_task"),
            "Planner/planner/focus_taskController"
        );
        // The other namespaces never carry the root segment.
        assert_eq!(handler_identity("go", "Planner", "planner", "list"), "planner.listHandler");
        assert_eq!(handler_identity("rust", "Planner", "planner", "list"), "planner::listRoute");
        assert_eq!(
            handler_identity("node", "Planner", "planner", "list"),
            "planner/list.handler"
        );
    }
}
