//! Canonical serialization of the transport-http attachment (issue
//! #70).
//!
//! Compact UTF-8 JSON with byte-sorted object keys: security schemes
//! are sorted by id, endpoints by endpoint id, and every set-like
//! member keeps its normalized sorted order while body/response
//! field subsets keep their canonical (name-sorted) order. Repeated
//! runs are byte-identical.

use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::types::{
    ApiVersionBinding, AuthBinding, BodyBinding, CacheBinding, CapabilityDecl, CorrelationBinding,
    EndpointBinding, ErrorDefaults, FieldProjection, IdempotencyBinding, PaginationBinding,
    ParamBinding, RateLimitBinding, SecurityScheme, SuccessBinding,
};
use super::{TransportDefaults, TransportDocument, WireDialect};
use crate::diagnostics::DiagnosticSet;

/// The canonical payload bytes, or the typed export-limit refusal.
pub(crate) fn attachment_bytes(document: &TransportDocument) -> Result<String, DiagnosticSet> {
    let mut root = Map::new();
    root.insert(
        "schemaVersion".to_owned(),
        Json::String(super::version::SCHEMA_VERSION.to_owned()),
    );
    root.insert(
        "identity".to_owned(),
        Json::String(super::version::IDENTITY.to_owned()),
    );
    root.insert(
        "attachmentRevision".to_owned(),
        Json::String(document.attachment_revision().as_str().to_owned()),
    );
    root.insert(
        "projectId".to_owned(),
        Json::String(document.project_id().as_str().to_owned()),
    );
    let mut model_ref = Map::new();
    model_ref.insert(
        "modelVersion".to_owned(),
        Json::String(document.model_ref().version().as_str().to_owned()),
    );
    model_ref.insert(
        "digest".to_owned(),
        Json::String(document.model_ref().digest().as_str().to_owned()),
    );
    root.insert("modelRef".to_owned(), Json::Object(model_ref));
    let mut ir_ref = Map::new();
    ir_ref.insert(
        "identity".to_owned(),
        Json::String(super::version::IR_IDENTITY.to_owned()),
    );
    ir_ref.insert(
        "digest".to_owned(),
        Json::String(document.ir_digest().as_str().to_owned()),
    );
    root.insert("irRef".to_owned(), Json::Object(ir_ref));
    root.insert("wire".to_owned(), wire_json(document.wire()));
    root.insert("defaults".to_owned(), defaults_json(document.defaults()));
    root.insert(
        "securitySchemes".to_owned(),
        Json::Array(
            document
                .schemes()
                .iter()
                .map(scheme_json)
                .collect::<Vec<_>>(),
        ),
    );
    root.insert(
        "endpoints".to_owned(),
        Json::Array(
            document
                .endpoints()
                .iter()
                .map(endpoint_json)
                .collect::<Vec<_>>(),
        ),
    );
    let bytes = Json::Object(root).to_string();
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The canonical wire dialect object.
fn wire_json(wire: &WireDialect) -> Json {
    let mut object = Map::new();
    object.insert("dialect".to_owned(), Json::String(wire.dialect.to_owned()));
    object.insert(
        "contentType".to_owned(),
        Json::String(wire.content_type.to_owned()),
    );
    Json::Object(object)
}

/// The canonical defaults object.
fn defaults_json(defaults: &TransportDefaults) -> Json {
    let mut object = Map::new();
    object.insert(
        "errorEnvelope".to_owned(),
        Json::String(defaults.error_envelope.to_owned()),
    );
    object.insert(
        "idempotencyHeader".to_owned(),
        Json::String(defaults.idempotency_header.as_str().to_owned()),
    );
    object.insert(
        "correlationHeaders".to_owned(),
        Json::Array(
            defaults
                .correlation_headers
                .iter()
                .map(|header| Json::String(header.as_str().to_owned()))
                .collect(),
        ),
    );
    Json::Object(object)
}

/// The canonical security scheme object.
fn scheme_json(scheme: &SecurityScheme) -> Json {
    let mut object = Map::new();
    object.insert("id".to_owned(), Json::String(scheme.id.as_str().to_owned()));
    object.insert(
        "kind".to_owned(),
        Json::String(scheme.kind.as_str().to_owned()),
    );
    if let Some(format) = &scheme.format {
        object.insert(
            "format".to_owned(),
            Json::String(format.as_str().to_owned()),
        );
    }
    if let Some(location) = scheme.location {
        object.insert("in".to_owned(), Json::String(location.as_str().to_owned()));
    }
    if let Some(name) = &scheme.name {
        object.insert("name".to_owned(), Json::String(name.as_str().to_owned()));
    }
    if !scheme.flow_ids.is_empty() {
        object.insert(
            "flowIds".to_owned(),
            Json::Array(
                scheme
                    .flow_ids
                    .iter()
                    .map(|flow| Json::String(flow.as_str().to_owned()))
                    .collect(),
            ),
        );
    }
    if let Some(token) = &scheme.capability_token {
        object.insert(
            "capabilityToken".to_owned(),
            Json::String(token.as_str().to_owned()),
        );
    }
    Json::Object(object)
}

/// The canonical endpoint binding object.
fn endpoint_json(binding: &EndpointBinding) -> Json {
    let mut object = Map::new();
    object.insert(
        "endpoint".to_owned(),
        Json::String(binding.endpoint.as_str().to_owned()),
    );
    if let Some(operation_id) = &binding.operation_id {
        object.insert(
            "operationId".to_owned(),
            Json::String(operation_id.as_str().to_owned()),
        );
    }
    if !binding.tags.is_empty() {
        object.insert(
            "tags".to_owned(),
            Json::Array(
                binding
                    .tags
                    .iter()
                    .map(|tag| Json::String(tag.as_str().to_owned()))
                    .collect(),
            ),
        );
    }
    if let Some(summary) = &binding.summary {
        object.insert("summary".to_owned(), Json::String(summary.clone()));
    }
    if !binding.params.is_empty() {
        object.insert(
            "params".to_owned(),
            Json::Array(binding.params.iter().map(param_json).collect()),
        );
    }
    if let Some(body) = &binding.body {
        object.insert("body".to_owned(), body_json(body, true));
    }
    object.insert("success".to_owned(), success_json(&binding.success));
    if !binding.errors.is_empty() {
        object.insert(
            "errors".to_owned(),
            Json::Array(
                binding
                    .errors
                    .iter()
                    .map(|entry| {
                        let mut error = Map::new();
                        error.insert(
                            "error".to_owned(),
                            Json::String(entry.error.as_str().to_owned()),
                        );
                        error.insert("status".to_owned(), Json::from(entry.status));
                        Json::Object(error)
                    })
                    .collect(),
            ),
        );
    }
    object.insert(
        "errorDefaults".to_owned(),
        error_defaults_json(&binding.error_defaults),
    );
    if let Some(auth) = &binding.auth {
        object.insert("auth".to_owned(), auth_json(auth));
    }
    if let Some(idempotency) = &binding.idempotency {
        object.insert("idempotency".to_owned(), idempotency_json(idempotency));
    }
    if let Some(correlation) = &binding.correlation {
        object.insert("correlation".to_owned(), correlation_json(correlation));
    }
    if let Some(pagination) = &binding.pagination {
        object.insert("pagination".to_owned(), pagination_json(pagination));
    }
    if let Some(rate_limit) = &binding.rate_limit {
        object.insert("rateLimit".to_owned(), rate_limit_json(rate_limit));
    }
    if let Some(cache) = &binding.cache {
        object.insert("cache".to_owned(), cache_json(cache));
    }
    if let Some(api_version) = &binding.api_version {
        object.insert("apiVersion".to_owned(), api_version_json(api_version));
    }
    if !binding.capabilities.is_empty() {
        object.insert(
            "capabilities".to_owned(),
            Json::Array(binding.capabilities.iter().map(capability_json).collect()),
        );
    }
    if !binding.scenarios.is_empty() {
        object.insert(
            "scenarios".to_owned(),
            Json::Array(
                binding
                    .scenarios
                    .iter()
                    .map(|id| Json::String(id.as_str().to_owned()))
                    .collect(),
            ),
        );
    }
    Json::Object(object)
}

/// The canonical parameter binding object.
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
    if let Some(explode) = param.explode {
        object.insert("explode".to_owned(), Json::Bool(explode));
    }
    Json::Object(object)
}

/// The canonical body/response projection object.
fn body_json(body: &BodyBinding, request: bool) -> Json {
    let mut object = Map::new();
    if request {
        object.insert(
            "contentType".to_owned(),
            Json::String(super::version::CONTENT_TYPE.to_owned()),
        );
    }
    object.insert(
        "mode".to_owned(),
        Json::String(if request {
            body.mode.body_str().to_owned()
        } else {
            body.mode.response_str().to_owned()
        }),
    );
    if !body.fields.is_empty() {
        object.insert(
            "fields".to_owned(),
            Json::Array(body.fields.iter().map(field_json).collect()),
        );
    }
    Json::Object(object)
}

/// The canonical field projection object.
fn field_json(field: &FieldProjection) -> Json {
    let mut object = Map::new();
    object.insert(
        "name".to_owned(),
        Json::String(field.name.as_str().to_owned()),
    );
    object.insert("field".to_owned(), Json::String(field.field.as_str()));
    object.insert("required".to_owned(), Json::Bool(field.required));
    Json::Object(object)
}

/// The canonical success binding object.
fn success_json(success: &SuccessBinding) -> Json {
    let mut object = Map::new();
    object.insert("status".to_owned(), Json::from(success.status));
    if let Some(body) = &success.body {
        object.insert("body".to_owned(), body_json(body, false));
    }
    if !success.headers.is_empty() {
        object.insert(
            "headers".to_owned(),
            Json::Array(
                success
                    .headers
                    .iter()
                    .map(|header| {
                        let mut entry = Map::new();
                        entry.insert(
                            "name".to_owned(),
                            Json::String(header.name.as_str().to_owned()),
                        );
                        entry.insert("required".to_owned(), Json::Bool(header.required));
                        Json::Object(entry)
                    })
                    .collect(),
            ),
        );
    }
    Json::Object(object)
}

/// The canonical error-defaults object.
fn error_defaults_json(defaults: &ErrorDefaults) -> Json {
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

/// The canonical security projection object.
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

/// The canonical idempotency binding object.
fn idempotency_json(idempotency: &IdempotencyBinding) -> Json {
    let mut object = Map::new();
    object.insert(
        "header".to_owned(),
        Json::String(idempotency.header.as_str().to_owned()),
    );
    object.insert("required".to_owned(), Json::Bool(idempotency.required));
    Json::Object(object)
}

/// The canonical correlation binding object.
fn correlation_json(correlation: &CorrelationBinding) -> Json {
    let mut object = Map::new();
    object.insert(
        "headers".to_owned(),
        Json::Array(
            correlation
                .headers
                .iter()
                .map(|header| Json::String(header.as_str().to_owned()))
                .collect(),
        ),
    );
    Json::Object(object)
}

/// The canonical pagination binding object.
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

/// The canonical rate-limit binding object.
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

/// The canonical cache binding object.
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

/// The canonical api-version binding object.
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

/// The canonical capability declaration object.
fn capability_json(capability: &CapabilityDecl) -> Json {
    let mut object = Map::new();
    object.insert(
        "capability".to_owned(),
        Json::String(capability.capability.as_str().to_owned()),
    );
    object.insert(
        "minimumSupport".to_owned(),
        Json::String(capability.minimum_support.as_str().to_owned()),
    );
    object.insert(
        "detail".to_owned(),
        Json::String(capability.detail.as_str().to_owned()),
    );
    Json::Object(object)
}
