//! Wire normalization of the transport-http attachment (issue #70).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`TransportDocument`]. It fails closed before semantic
//! processing: unknown or missing fields, wrong identities, malformed
//! identifiers, digests, bounds, and contradictory declarations each
//! return one typed registered diagnostic and no partial attachment.
//! Semantic rules (endpoint resolution, path-template agreement,
//! error-union membership, profile capability support) live in the
//! validation context ([`super::validate`]).

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::SemanticId;
use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::id::{FieldRef, HeaderName, OperationId, SafeToken, WireName};
use super::types::{
    Actor, ApiKeyLocation, ApiVersionBinding, ApiVersionLocation, AuthBinding, BodyBinding,
    CacheBinding, CachePolicy, CapabilityDecl, CapabilityDetail, CapabilityKind, CapabilitySupport,
    CorrelationBinding, EndpointBinding, ErrorDefaults, ErrorEntry, FieldProjection,
    IdempotencyBinding, PaginationBinding, PaginationStyle, ParamBinding, ParamLocation,
    ParamStyle, ProjectionMode, RateLimitBinding, RateLimitScope, ResponseHeader, SchemeKind,
    SecurityScheme, SuccessBinding, TransportDefaults, WireDialect,
};
use super::version;
use super::version::{
    MAX_AUTH_SCHEMES, MAX_CAPABILITIES, MAX_CORRELATION_HEADERS, MAX_ENDPOINTS, MAX_ERROR_MAP,
    MAX_FIELDS, MAX_HEADERS, MAX_PARAMS, MAX_SCENARIOS, MAX_SCHEMES, MAX_TAGS,
};
use super::{ModelPin, TransportDocument};

/// The parsed wire object type.
type WireMap = Map<String, Json>;

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "wire",
    "defaults",
    "securitySchemes",
    "endpoints",
];

/// The required top-level members.
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "wire",
    "defaults",
    "securitySchemes",
    "endpoints",
];

/// The closed endpoint-binding member set.
const ENDPOINT_KEYS: &[&str] = &[
    "endpoint",
    "operationId",
    "tags",
    "summary",
    "params",
    "body",
    "success",
    "errors",
    "errorDefaults",
    "auth",
    "idempotency",
    "correlation",
    "pagination",
    "rateLimit",
    "cache",
    "apiVersion",
    "capabilities",
    "scenarios",
];

/// The required endpoint-binding members.
const ENDPOINT_REQUIRED_KEYS: &[&str] = &["endpoint", "success", "errorDefaults"];

/// Normalize one wire document into a validated attachment, or
/// return the typed rejection set with no partial attachment. Pure:
/// no source, model, cache, report, network, process, or target
/// access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<TransportDocument, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("top-level-shape"))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    for required in REQUIRED_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::input_invalid("missing-field"));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::input_invalid("schema-version"));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
        return Err(diagnostic::input_invalid("contract-identity"));
    }
    let attachment_revision = SemVer::parse(
        object
            .get("attachmentRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("attachment-revision"))?,
    )
    .map_err(|_| diagnostic::input_invalid("attachment-revision"))?;
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("project-id"))?,
    )
    .map_err(|_| diagnostic::input_invalid("project-id"))?;
    let model_ref = model_pin(
        object
            .get("modelRef")
            .ok_or_else(|| diagnostic::input_invalid("model-ref"))?,
    )?;
    let ir_digest = ir_digest_member(
        object
            .get("irRef")
            .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?,
    )?;
    let wire = wire_dialect(
        object
            .get("wire")
            .ok_or_else(|| diagnostic::input_invalid("wire"))?,
    )?;
    let defaults = transport_defaults(
        object
            .get("defaults")
            .ok_or_else(|| diagnostic::input_invalid("defaults"))?,
    )?;
    let schemes = security_schemes(
        object
            .get("securitySchemes")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("security-schemes"))?,
    )?;
    let endpoints = endpoints(
        object
            .get("endpoints")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("endpoint-list"))?,
    )?;
    let document = TransportDocument::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_digest,
        wire,
        defaults,
        schemes,
        endpoints,
    );
    Ok(document)
}

/// Decode the bound source Model contract.
fn model_pin(value: &Json) -> Result<ModelPin, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("model-ref-shape"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "modelVersion" | "digest") {
            return Err(diagnostic::input_invalid("model-ref-field"));
        }
    }
    let pin = match object
        .get("modelVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("model-version"))?
    {
        version::MODEL_VERSION => crate::scenario::ModelPin::Current,
        _ => return Err(diagnostic::input_invalid("model-version")),
    };
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("model-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("model-digest"))?;
    Ok(ModelPin::new(pin, digest))
}

/// Decode the bound IR digest.
fn ir_digest_member(value: &Json) -> Result<Sha256Digest, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("ir-ref-shape"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "identity" | "digest") {
            return Err(diagnostic::input_invalid("ir-ref-field"));
        }
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IR_IDENTITY) {
        return Err(diagnostic::input_invalid("ir-identity"));
    }
    Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("ir-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("ir-digest"))
}

/// Decode the wire dialect declaration.
fn wire_dialect(value: &Json) -> Result<WireDialect, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("wire-shape"))?;
    closed_keys(
        object,
        &["dialect", "contentType"],
        &["dialect", "contentType"],
        "wire",
    )?;
    let dialect = object.get("dialect").and_then(Json::as_str);
    if dialect != Some(version::WIRE_DIALECT) {
        return Err(diagnostic::input_invalid("wire-dialect"));
    }
    let content_type = object.get("contentType").and_then(Json::as_str);
    if content_type != Some(version::CONTENT_TYPE) {
        return Err(diagnostic::input_invalid("wire-content-type"));
    }
    Ok(WireDialect {
        dialect: version::WIRE_DIALECT,
        content_type: version::CONTENT_TYPE,
    })
}

/// Check one object's member set against the closed allowed and
/// required lists.
fn closed_keys(
    object: &WireMap,
    allowed: &[&str],
    required: &[&str],
    member: &str,
) -> Result<(), DiagnosticSet> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid(&format!("{member}-field")));
        }
    }
    for key in required {
        if !object.contains_key(*key) {
            return Err(diagnostic::input_invalid(&format!("{member}-missing")));
        }
    }
    Ok(())
}

/// Decode the transport defaults.
fn transport_defaults(value: &Json) -> Result<TransportDefaults, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("defaults-shape"))?;
    closed_keys(
        object,
        &["errorEnvelope", "idempotencyHeader", "correlationHeaders"],
        &["errorEnvelope", "idempotencyHeader", "correlationHeaders"],
        "defaults",
    )?;
    if object.get("errorEnvelope").and_then(Json::as_str) != Some(version::ERROR_ENVELOPE) {
        return Err(diagnostic::input_invalid("error-envelope"));
    }
    let idempotency_header = header_name(
        object
            .get("idempotencyHeader")
            .ok_or_else(|| diagnostic::input_invalid("idempotency-header"))?,
    )?;
    let headers = header_list(
        object
            .get("correlationHeaders")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("correlation-headers"))?,
        MAX_CORRELATION_HEADERS,
    )?;
    Ok(TransportDefaults {
        error_envelope: version::ERROR_ENVELOPE,
        idempotency_header,
        correlation_headers: headers,
    })
}

/// Decode one bounded header name.
fn header_name(value: &Json) -> Result<HeaderName, DiagnosticSet> {
    HeaderName::parse(
        value
            .as_str()
            .ok_or_else(|| diagnostic::input_invalid("header-name"))?,
    )
    .map_err(|_| diagnostic::input_invalid("header-name"))
}

/// Decode a bounded list of header names, unique and sorted.
fn header_list(values: &[Json], bound: usize) -> Result<Vec<HeaderName>, DiagnosticSet> {
    if values.is_empty() || values.len() > bound {
        return Err(diagnostic::input_invalid("header-bound"));
    }
    let mut headers = Vec::with_capacity(values.len());
    for value in values {
        headers.push(header_name(value)?);
    }
    sort_unique(&mut headers, |header| header.as_str(), "header-duplicate")?;
    Ok(headers)
}

/// Decode the declared security schemes.
fn security_schemes(values: &[Json]) -> Result<Vec<SecurityScheme>, DiagnosticSet> {
    if values.len() > MAX_SCHEMES {
        return Err(diagnostic::input_invalid("scheme-bound"));
    }
    let mut schemes = Vec::with_capacity(values.len());
    for value in values {
        schemes.push(security_scheme(value)?);
    }
    sort_unique(
        &mut schemes,
        |scheme| scheme.id.as_str(),
        "scheme-duplicate",
    )?;
    Ok(schemes)
}

/// Decode one declared security scheme. Kind-specific members are
/// enforced fail-closed: bearer requires format, api-key requires in
/// and name, oauth2 requires flowIds, custom requires
/// capabilityToken; kind none carries none of them.
fn security_scheme(value: &Json) -> Result<SecurityScheme, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("scheme-shape"))?;
    closed_keys(
        object,
        &[
            "id",
            "kind",
            "format",
            "in",
            "name",
            "flowIds",
            "capabilityToken",
        ],
        &["id", "kind"],
        "scheme",
    )?;
    let id = WireName::parse(
        object
            .get("id")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("scheme-id"))?,
    )
    .map_err(|_| diagnostic::input_invalid("scheme-id"))?;
    let kind = SchemeKind::parse(
        object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("scheme-kind"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("scheme-kind"))?;
    let format = match object.get("format") {
        Some(value) => Some(
            SafeToken::parse(value.as_str().unwrap_or_default())
                .map_err(|_| diagnostic::input_invalid("scheme-format"))?,
        ),
        None => None,
    };
    let location = match object.get("in") {
        Some(value) => Some(
            ApiKeyLocation::parse(value.as_str().unwrap_or_default())
                .ok_or_else(|| diagnostic::input_invalid("scheme-location"))?,
        ),
        None => None,
    };
    let name = match object.get("name") {
        Some(value) => Some(header_name(value)?),
        None => None,
    };
    let flow_ids = match object.get("flowIds") {
        Some(value) => {
            let list = value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("scheme-flows"))?;
            if list.is_empty() || list.len() > 8 {
                return Err(diagnostic::input_invalid("scheme-flows"));
            }
            let mut flows = Vec::with_capacity(list.len());
            for item in list {
                flows.push(
                    SafeToken::parse(item.as_str().unwrap_or_default())
                        .map_err(|_| diagnostic::input_invalid("scheme-flow"))?,
                );
            }
            sort_unique(&mut flows, |flow| flow.as_str(), "scheme-flow-duplicate")?;
            flows
        }
        None => Vec::new(),
    };
    let capability_token = match object.get("capabilityToken") {
        Some(value) => Some(
            SafeToken::parse(value.as_str().unwrap_or_default())
                .map_err(|_| diagnostic::input_invalid("scheme-capability"))?,
        ),
        None => None,
    };
    let scheme = SecurityScheme {
        id,
        kind,
        format,
        location,
        name,
        flow_ids,
        capability_token,
    };
    check_scheme_shape(&scheme)?;
    Ok(scheme)
}

/// Enforce the kind-specific member pairing of one scheme.
fn check_scheme_shape(scheme: &SecurityScheme) -> Result<(), DiagnosticSet> {
    let shape_ok = match scheme.kind {
        SchemeKind::None => {
            scheme.format.is_none()
                && scheme.location.is_none()
                && scheme.name.is_none()
                && scheme.flow_ids.is_empty()
                && scheme.capability_token.is_none()
        }
        SchemeKind::Bearer => {
            scheme.format.is_some()
                && scheme.location.is_none()
                && scheme.name.is_none()
                && scheme.flow_ids.is_empty()
                && scheme.capability_token.is_none()
        }
        SchemeKind::ApiKey => {
            scheme.format.is_none()
                && scheme.location.is_some()
                && scheme.name.is_some()
                && scheme.flow_ids.is_empty()
                && scheme.capability_token.is_none()
        }
        SchemeKind::Basic => {
            scheme.format.is_none()
                && scheme.location.is_none()
                && scheme.name.is_none()
                && scheme.flow_ids.is_empty()
                && scheme.capability_token.is_none()
        }
        SchemeKind::Oauth2 => {
            scheme.format.is_none()
                && scheme.location.is_none()
                && scheme.name.is_none()
                && !scheme.flow_ids.is_empty()
                && scheme.capability_token.is_none()
        }
        SchemeKind::MutualTls => {
            scheme.format.is_none()
                && scheme.location.is_none()
                && scheme.name.is_none()
                && scheme.flow_ids.is_empty()
                && scheme.capability_token.is_none()
        }
        SchemeKind::Custom => {
            scheme.format.is_none()
                && scheme.location.is_none()
                && scheme.name.is_none()
                && scheme.flow_ids.is_empty()
                && scheme.capability_token.is_some()
        }
    };
    if shape_ok {
        Ok(())
    } else {
        Err(diagnostic::input_invalid("scheme-member-pairing"))
    }
}

/// Decode the endpoint bindings.
fn endpoints(values: &[Json]) -> Result<Vec<EndpointBinding>, DiagnosticSet> {
    if values.is_empty() || values.len() > MAX_ENDPOINTS {
        return Err(diagnostic::input_invalid("endpoint-bound"));
    }
    let mut endpoints = Vec::with_capacity(values.len());
    for value in values {
        endpoints.push(endpoint_binding(value)?);
    }
    sort_unique(
        &mut endpoints,
        |binding| binding.endpoint.as_str(),
        "endpoint-duplicate",
    )?;
    // The effective operation ids must also be unique; an explicit
    // override colliding with a derived default is a declaration
    // contradiction, never an arrival-order win.
    let mut operation_ids: Vec<(String, &str)> = endpoints
        .iter()
        .map(|binding| {
            (
                binding.effective_operation_id().as_str().to_owned(),
                binding.endpoint.as_str(),
            )
        })
        .collect();
    operation_ids.sort();
    if operation_ids
        .windows(2)
        .any(|window| window[0].0 == window[1].0)
    {
        return Err(diagnostic::input_invalid("operation-id-duplicate"));
    }
    Ok(endpoints)
}

/// Decode one endpoint binding.
fn endpoint_binding(value: &Json) -> Result<EndpointBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("endpoint-shape"))?;
    for key in object.keys() {
        if !ENDPOINT_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    for required in ENDPOINT_REQUIRED_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::input_invalid("missing-field"));
        }
    }
    let endpoint = SemanticId::parse(
        object
            .get("endpoint")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("endpoint-ref"))?,
    )
    .map_err(|_| diagnostic::input_invalid("endpoint-ref"))?;
    let operation_id = match object.get("operationId") {
        Some(value) => Some(
            OperationId::parse(value.as_str().unwrap_or_default())
                .map_err(|_| diagnostic::input_invalid("operation-id"))?,
        ),
        None => None,
    };
    let tags = match object.get("tags") {
        Some(value) => {
            let list = value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("tag-list"))?;
            if list.len() > MAX_TAGS {
                return Err(diagnostic::input_invalid("tag-bound"));
            }
            let mut tags = Vec::with_capacity(list.len());
            for item in list {
                tags.push(
                    SafeToken::parse(item.as_str().unwrap_or_default())
                        .map_err(|_| diagnostic::input_invalid("tag"))?,
                );
            }
            sort_unique(&mut tags, |tag| tag.as_str(), "tag-duplicate")?;
            tags
        }
        None => Vec::new(),
    };
    let summary = match object.get("summary") {
        Some(value) => {
            let text = value
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("summary"))?;
            if text.is_empty() || text.len() > 256 {
                return Err(diagnostic::input_invalid("summary"));
            }
            Some(text.to_owned())
        }
        None => None,
    };
    let params = params(
        object
            .get("params")
            .and_then(Json::as_array)
            .map(|list| list.as_slice())
            .unwrap_or_default(),
    )?;
    let body = match object.get("body") {
        Some(value) => Some(body_binding(value, ParamOrResponse::Body)?),
        None => None,
    };
    let success = success_binding(
        object
            .get("success")
            .ok_or_else(|| diagnostic::input_invalid("success"))?,
    )?;
    let errors = error_entries(
        object
            .get("errors")
            .and_then(Json::as_array)
            .map(|list| list.as_slice())
            .unwrap_or_default(),
    )?;
    let error_defaults = error_defaults(
        object
            .get("errorDefaults")
            .ok_or_else(|| diagnostic::input_invalid("error-defaults"))?,
    )?;
    let auth = match object.get("auth") {
        Some(value) => Some(auth_binding(value)?),
        None => None,
    };
    let idempotency = match object.get("idempotency") {
        Some(value) => Some(idempotency_binding(value)?),
        None => None,
    };
    let correlation = match object.get("correlation") {
        Some(value) => Some(correlation_binding(value)?),
        None => None,
    };
    let pagination = match object.get("pagination") {
        Some(value) => Some(pagination_binding(value)?),
        None => None,
    };
    let rate_limit = match object.get("rateLimit") {
        Some(value) => Some(rate_limit_binding(value)?),
        None => None,
    };
    let cache = match object.get("cache") {
        Some(value) => Some(cache_binding(value)?),
        None => None,
    };
    let api_version = match object.get("apiVersion") {
        Some(value) => Some(api_version_binding(value)?),
        None => None,
    };
    let capabilities = capability_decls(
        object
            .get("capabilities")
            .and_then(Json::as_array)
            .map(|list| list.as_slice())
            .unwrap_or_default(),
    )?;
    let scenarios = scenario_refs(
        object
            .get("scenarios")
            .and_then(Json::as_array)
            .map(|list| list.as_slice())
            .unwrap_or_default(),
    )?;
    Ok(EndpointBinding {
        endpoint,
        operation_id,
        tags,
        summary,
        params,
        body,
        success,
        errors,
        error_defaults,
        auth,
        idempotency,
        correlation,
        pagination,
        rate_limit,
        cache,
        api_version,
        capabilities,
        scenarios,
    })
}

/// Sort one collection by the projection and refuse duplicates.
fn sort_unique<T>(
    items: &mut [T],
    key: fn(&T) -> &str,
    duplicate_detail: &'static str,
) -> Result<(), DiagnosticSet> {
    items.sort_by(|left, right| key(left).cmp(key(right)));
    if items
        .windows(2)
        .any(|window| key(&window[0]) == key(&window[1]))
    {
        return Err(diagnostic::input_invalid(duplicate_detail));
    }
    Ok(())
}

/// Decode the declared parameter bindings.
fn params(values: &[Json]) -> Result<Vec<ParamBinding>, DiagnosticSet> {
    if values.len() > MAX_PARAMS {
        return Err(diagnostic::input_invalid("param-bound"));
    }
    let mut params = Vec::with_capacity(values.len());
    for value in values {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("param-shape"))?;
        closed_keys(
            object,
            &["name", "in", "field", "required", "style", "explode"],
            &["name", "in", "field", "required"],
            "param",
        )?;
        let name = WireName::parse(
            object
                .get("name")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("param-name"))?,
        )
        .map_err(|_| diagnostic::input_invalid("param-name"))?;
        let location = ParamLocation::parse(
            object
                .get("in")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("param-location"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("param-location"))?;
        let field = FieldRef::parse(
            object
                .get("field")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("param-field"))?,
        )
        .map_err(|_| diagnostic::input_invalid("param-field"))?;
        let required = object
            .get("required")
            .and_then(Json::as_bool)
            .ok_or_else(|| diagnostic::input_invalid("param-required"))?;
        let style = match object.get("style") {
            Some(value) => Some(
                ParamStyle::parse(value.as_str().unwrap_or_default())
                    .ok_or_else(|| diagnostic::input_invalid("param-style"))?,
            ),
            None => None,
        };
        let explode = match object.get("explode") {
            Some(value) => Some(
                value
                    .as_bool()
                    .ok_or_else(|| diagnostic::input_invalid("param-explode"))?,
            ),
            None => None,
        };
        // Path parameters are always required; a declared optional
        // path parameter is a contradiction, never a default.
        if location == ParamLocation::Path && !required {
            return Err(diagnostic::input_invalid("param-required"));
        }
        if let Some(style) = style {
            let allowed = match location {
                ParamLocation::Path => matches!(style, ParamStyle::Simple),
                ParamLocation::Query => {
                    matches!(style, ParamStyle::Form | ParamStyle::DeepObject)
                }
                ParamLocation::Header | ParamLocation::Cookie => {
                    matches!(style, ParamStyle::Simple)
                }
            };
            if !allowed {
                return Err(diagnostic::input_invalid("param-style"));
            }
        }
        params.push(ParamBinding {
            name,
            location,
            field,
            required,
            style,
            explode,
        });
    }
    // One wire name may arrive in one location only once; a duplicate
    // (name, location) pair is ambiguous decoding — the same name in
    // different locations is fine (e.g. `id` in path + `id` in query).
    let mut seen: Vec<(ParamLocation, String)> = params
        .iter()
        .map(|param| (param.location, param.name.as_str().to_owned()))
        .collect();
    seen.sort();
    if seen.windows(2).any(|window| window[0] == window[1]) {
        return Err(diagnostic::input_invalid("param-duplicate"));
    }
    Ok(params)
}

/// Whether the decoder is reading a request body or a response body.
#[derive(Clone, Copy, PartialEq)]
enum ParamOrResponse {
    Body,
    Response,
}

/// Decode one body or response projection.
fn body_binding(value: &Json, side: ParamOrResponse) -> Result<BodyBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("body-shape"))?;
    closed_keys(
        object,
        &["contentType", "mode", "fields"],
        &["mode"],
        "body",
    )?;
    if let Some(content_type) = object.get("contentType").and_then(Json::as_str) {
        if content_type != version::CONTENT_TYPE {
            return Err(diagnostic::input_invalid("body-content-type"));
        }
    }
    let mode_token = object
        .get("mode")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("body-mode"))?;
    let mode = match side {
        ParamOrResponse::Body => ProjectionMode::parse_body(mode_token),
        ParamOrResponse::Response => ProjectionMode::parse_response(mode_token),
    }
    .ok_or_else(|| diagnostic::input_invalid("body-mode"))?;
    let fields = field_projections(
        object
            .get("fields")
            .and_then(Json::as_array)
            .map(|list| list.as_slice())
            .unwrap_or_default(),
    )?;
    if mode == ProjectionMode::Explicit && fields.is_empty() {
        return Err(diagnostic::input_invalid("body-fields"));
    }
    if mode == ProjectionMode::Whole && !fields.is_empty() {
        return Err(diagnostic::input_invalid("body-fields"));
    }
    Ok(BodyBinding { mode, fields })
}

/// Decode one declared field subset.
fn field_projections(values: &[Json]) -> Result<Vec<FieldProjection>, DiagnosticSet> {
    if values.len() > MAX_FIELDS {
        return Err(diagnostic::input_invalid("field-bound"));
    }
    let mut fields = Vec::with_capacity(values.len());
    for value in values {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("field-shape"))?;
        closed_keys(
            object,
            &["name", "field", "required"],
            &["name", "field", "required"],
            "field",
        )?;
        let name = WireName::parse(
            object
                .get("name")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("field-name"))?,
        )
        .map_err(|_| diagnostic::input_invalid("field-name"))?;
        let field = FieldRef::parse(
            object
                .get("field")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("field-ref"))?,
        )
        .map_err(|_| diagnostic::input_invalid("field-ref"))?;
        let required = object
            .get("required")
            .and_then(Json::as_bool)
            .ok_or_else(|| diagnostic::input_invalid("field-required"))?;
        fields.push(FieldProjection {
            name,
            field,
            required,
        });
    }
    sort_unique(&mut fields, |field| field.name.as_str(), "field-duplicate")?;
    Ok(fields)
}

/// Decode the success response binding.
fn success_binding(value: &Json) -> Result<SuccessBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("success-shape"))?;
    closed_keys(
        object,
        &["status", "body", "headers"],
        &["status"],
        "success",
    )?;
    let status = object
        .get("status")
        .and_then(Json::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .filter(|status| matches!(status, 200 | 201 | 202 | 204))
        .ok_or_else(|| diagnostic::input_invalid("success-status"))?;
    let body = match object.get("body") {
        Some(value) => Some(body_binding(value, ParamOrResponse::Response)?),
        None => None,
    };
    // A 204 response carries no body by definition; a declared body
    // on 204 is a contradiction.
    if status == 204 && body.is_some() {
        return Err(diagnostic::input_invalid("success-body"));
    }
    let headers = response_headers(
        object
            .get("headers")
            .and_then(Json::as_array)
            .map(|list| list.as_slice())
            .unwrap_or_default(),
    )?;
    Ok(SuccessBinding {
        status,
        body,
        headers,
    })
}

/// Decode the declared response headers.
fn response_headers(values: &[Json]) -> Result<Vec<ResponseHeader>, DiagnosticSet> {
    if values.len() > MAX_HEADERS {
        return Err(diagnostic::input_invalid("header-bound"));
    }
    let mut headers = Vec::with_capacity(values.len());
    for value in values {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("response-header-shape"))?;
        closed_keys(
            object,
            &["name", "required"],
            &["name", "required"],
            "response-header",
        )?;
        let name = header_name(
            object
                .get("name")
                .ok_or_else(|| diagnostic::input_invalid("response-header-name"))?,
        )?;
        let required = object
            .get("required")
            .and_then(Json::as_bool)
            .ok_or_else(|| diagnostic::input_invalid("response-header-required"))?;
        headers.push(ResponseHeader { name, required });
    }
    sort_unique(
        &mut headers,
        |header| header.name.as_str(),
        "response-header-duplicate",
    )?;
    Ok(headers)
}

/// Decode the error → status map.
fn error_entries(values: &[Json]) -> Result<Vec<ErrorEntry>, DiagnosticSet> {
    if values.len() > MAX_ERROR_MAP {
        return Err(diagnostic::input_invalid("error-map-bound"));
    }
    let mut entries = Vec::with_capacity(values.len());
    for value in values {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("error-entry-shape"))?;
        closed_keys(
            object,
            &["error", "status"],
            &["error", "status"],
            "error-entry",
        )?;
        let error = SemanticId::parse(
            object
                .get("error")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("error-ref"))?,
        )
        .map_err(|_| diagnostic::input_invalid("error-ref"))?;
        let status = object
            .get("status")
            .and_then(Json::as_u64)
            .and_then(|status| u16::try_from(status).ok())
            .filter(|status| (400..=599).contains(status))
            .ok_or_else(|| diagnostic::input_invalid("error-status"))?;
        entries.push(ErrorEntry { error, status });
    }
    sort_unique(
        &mut entries,
        |entry| entry.error.as_str(),
        "error-duplicate",
    )?;
    Ok(entries)
}

/// Decode the fixed category-default statuses.
fn error_defaults(value: &Json) -> Result<ErrorDefaults, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("error-defaults-shape"))?;
    const KEYS: &[&str] = &[
        "validation",
        "auth",
        "conflict",
        "not-found",
        "domain",
        "infrastructure",
    ];
    closed_keys(object, KEYS, KEYS, "error-defaults")?;
    let status_of = |key: &str| -> Result<u16, DiagnosticSet> {
        object
            .get(key)
            .and_then(Json::as_u64)
            .and_then(|status| u16::try_from(status).ok())
            .filter(|status| (400..=599).contains(status))
            .ok_or_else(|| diagnostic::input_invalid("error-default-status"))
    };
    Ok(ErrorDefaults {
        validation: status_of("validation")?,
        auth: status_of("auth")?,
        conflict: status_of("conflict")?,
        not_found: status_of("not-found")?,
        domain: status_of("domain")?,
        infrastructure: status_of("infrastructure")?,
    })
}

/// Decode one security projection.
fn auth_binding(value: &Json) -> Result<AuthBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("auth-shape"))?;
    closed_keys(
        object,
        &["actor", "schemes", "policyRef"],
        &["actor", "schemes"],
        "auth",
    )?;
    let actor = Actor::parse(
        object
            .get("actor")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("actor"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("actor"))?;
    let list = object
        .get("schemes")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("scheme-list"))?;
    if list.is_empty() || list.len() > MAX_AUTH_SCHEMES {
        return Err(diagnostic::input_invalid("scheme-bound"));
    }
    let mut schemes = Vec::with_capacity(list.len());
    for item in list {
        schemes.push(
            WireName::parse(item.as_str().unwrap_or_default())
                .map_err(|_| diagnostic::input_invalid("scheme-ref"))?,
        );
    }
    sort_unique(&mut schemes, |scheme| scheme.as_str(), "scheme-duplicate")?;
    let policy_ref = match object.get("policyRef") {
        Some(value) => Some(
            SemanticId::parse(value.as_str().unwrap_or_default())
                .map_err(|_| diagnostic::input_invalid("policy-ref"))?,
        ),
        None => None,
    };
    Ok(AuthBinding {
        actor,
        schemes,
        policy_ref,
    })
}

/// Decode one idempotency binding.
fn idempotency_binding(value: &Json) -> Result<IdempotencyBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("idempotency-shape"))?;
    closed_keys(
        object,
        &["header", "required"],
        &["header", "required"],
        "idempotency",
    )?;
    let header = header_name(
        object
            .get("header")
            .ok_or_else(|| diagnostic::input_invalid("idempotency-header"))?,
    )?;
    let required = object
        .get("required")
        .and_then(Json::as_bool)
        .ok_or_else(|| diagnostic::input_invalid("idempotency-required"))?;
    Ok(IdempotencyBinding { header, required })
}

/// Decode one correlation binding.
fn correlation_binding(value: &Json) -> Result<CorrelationBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("correlation-shape"))?;
    closed_keys(object, &["headers"], &["headers"], "correlation")?;
    let headers = header_list(
        object
            .get("headers")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("correlation-headers"))?,
        MAX_CORRELATION_HEADERS,
    )?;
    Ok(CorrelationBinding { headers })
}

/// Decode one pagination binding.
fn pagination_binding(value: &Json) -> Result<PaginationBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("pagination-shape"))?;
    closed_keys(
        object,
        &[
            "style",
            "limitParam",
            "offsetParam",
            "cursorParam",
            "cursorField",
        ],
        &["style", "limitParam"],
        "pagination",
    )?;
    let style = PaginationStyle::parse(
        object
            .get("style")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("pagination-style"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("pagination-style"))?;
    let wire_param = |key: &str, detail: &'static str| -> Result<WireName, DiagnosticSet> {
        WireName::parse(object.get(key).and_then(Json::as_str).unwrap_or_default())
            .map_err(|_| diagnostic::input_invalid(detail))
    };
    let limit_param = wire_param("limitParam", "pagination-limit")?;
    let offset_param = object
        .get("offsetParam")
        .map(|_| wire_param("offsetParam", "pagination-offset"))
        .transpose()?;
    let cursor_param = object
        .get("cursorParam")
        .map(|_| wire_param("cursorParam", "pagination-cursor"))
        .transpose()?;
    let cursor_field = object
        .get("cursorField")
        .map(|_| wire_param("cursorField", "pagination-cursor-field"))
        .transpose()?;
    let binding = PaginationBinding {
        style,
        limit_param,
        offset_param,
        cursor_param,
        cursor_field,
    };
    match style {
        PaginationStyle::Offset => {
            if binding.offset_param.is_none()
                || binding.cursor_param.is_some()
                || binding.cursor_field.is_some()
            {
                return Err(diagnostic::input_invalid("pagination-style"));
            }
        }
        PaginationStyle::Cursor => {
            if binding.cursor_param.is_none()
                || binding.cursor_field.is_none()
                || binding.offset_param.is_some()
            {
                return Err(diagnostic::input_invalid("pagination-style"));
            }
        }
    }
    Ok(binding)
}

/// Decode one rate-limit declaration.
fn rate_limit_binding(value: &Json) -> Result<RateLimitBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("rate-limit-shape"))?;
    closed_keys(
        object,
        &["limit", "windowSeconds", "scope"],
        &["limit", "windowSeconds", "scope"],
        "rate-limit",
    )?;
    let limit = object
        .get("limit")
        .and_then(Json::as_u64)
        .filter(|limit| (1..=1_000_000).contains(limit))
        .ok_or_else(|| diagnostic::input_invalid("rate-limit-value"))?;
    let window_seconds = object
        .get("windowSeconds")
        .and_then(Json::as_u64)
        .filter(|seconds| (1..=86_400).contains(seconds))
        .ok_or_else(|| diagnostic::input_invalid("rate-limit-window"))?;
    let scope = RateLimitScope::parse(
        object
            .get("scope")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("rate-limit-scope"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("rate-limit-scope"))?;
    Ok(RateLimitBinding {
        limit,
        window_seconds,
        scope,
    })
}

/// Decode one cache declaration.
fn cache_binding(value: &Json) -> Result<CacheBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("cache-shape"))?;
    closed_keys(
        object,
        &["policy", "maxAgeSeconds", "etag"],
        &["policy", "maxAgeSeconds", "etag"],
        "cache",
    )?;
    let policy = CachePolicy::parse(
        object
            .get("policy")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("cache-policy"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("cache-policy"))?;
    let max_age_seconds = object
        .get("maxAgeSeconds")
        .and_then(Json::as_u64)
        .filter(|seconds| *seconds <= 604_800)
        .ok_or_else(|| diagnostic::input_invalid("cache-max-age"))?;
    let etag = object
        .get("etag")
        .and_then(Json::as_bool)
        .ok_or_else(|| diagnostic::input_invalid("cache-etag"))?;
    Ok(CacheBinding {
        policy,
        max_age_seconds,
        etag,
    })
}

/// Decode one content-version carrier.
fn api_version_binding(value: &Json) -> Result<ApiVersionBinding, DiagnosticSet> {
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("api-version-shape"))?;
    closed_keys(object, &["in", "name"], &["in", "name"], "api-version")?;
    let location = ApiVersionLocation::parse(
        object
            .get("in")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("api-version-location"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("api-version-location"))?;
    let name = SafeToken::parse(
        object
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("api-version-name"))?,
    )
    .map_err(|_| diagnostic::input_invalid("api-version-name"))?;
    Ok(ApiVersionBinding { location, name })
}

/// Decode the declared capabilities.
fn capability_decls(values: &[Json]) -> Result<Vec<CapabilityDecl>, DiagnosticSet> {
    if values.len() > MAX_CAPABILITIES {
        return Err(diagnostic::input_invalid("capability-bound"));
    }
    let mut capabilities = Vec::with_capacity(values.len());
    for value in values {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?;
        closed_keys(
            object,
            &["capability", "minimumSupport", "detail"],
            &["capability", "minimumSupport", "detail"],
            "capability",
        )?;
        let capability = CapabilityKind::parse(
            object
                .get("capability")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("capability-kind"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("capability-kind"))?;
        let minimum_support = CapabilitySupport::parse(
            object
                .get("minimumSupport")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("capability-support"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("capability-support"))?;
        let detail = CapabilityDetail::parse(
            object
                .get("detail")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("capability-detail"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("capability-detail"))?;
        if !capability.details().contains(&detail.as_str()) {
            return Err(diagnostic::input_invalid("capability-detail"));
        }
        capabilities.push(CapabilityDecl {
            capability,
            minimum_support,
            detail,
        });
    }
    sort_unique(
        &mut capabilities,
        |decl| decl.capability.as_str(),
        "capability-duplicate",
    )?;
    Ok(capabilities)
}

/// Decode the black-box scenario coverage references.
fn scenario_refs(values: &[Json]) -> Result<Vec<SemanticId>, DiagnosticSet> {
    if values.len() > MAX_SCENARIOS {
        return Err(diagnostic::input_invalid("scenario-bound"));
    }
    let mut scenarios = Vec::with_capacity(values.len());
    for value in values {
        scenarios.push(
            SemanticId::parse(value.as_str().unwrap_or_default())
                .map_err(|_| diagnostic::input_invalid("scenario-ref"))?,
        );
    }
    sort_unique(&mut scenarios, |id| id.as_str(), "scenario-duplicate")?;
    Ok(scenarios)
}
