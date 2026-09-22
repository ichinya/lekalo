//! The full-document OpenAPI projection (issue #46, plan §2–§3).
//!
//! [`render`] consumes one transport attachment joined with the
//! compiled project through
//! [`crate::transport_http::ValidationContext`] — the same validated
//! join every transport projection reads — plus the declared
//! [`RenderConfig`]. The output is one canonical JSON document:
//! compact, byte-sorted keys, deterministic member sets, byte-stable
//! across repeated runs. Nothing is invented: absent declarations
//! stay absent, undeclared data (servers, contact, license, oauth2
//! URLs) never appears, and members the declared sources cannot
//! express surface as `openapi.projection-partial` findings.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value as Json};

use crate::diagnostics::DiagnosticSet;
use crate::ir::{Definition, EndpointDef, Field, TypeRef};
use crate::query_model::QueryDecl;
use crate::transport_http::types::{
    ApiVersionLocation, AuthBinding, BodyBinding, EndpointBinding, ErrorDefaults, ProjectionMode,
    SchemeKind,
};
use crate::transport_http::{TransportDocument, ValidationContext};

use super::diagnostic;
use super::id::{component_name, escape_pointer, paths_pointer, schemas_pointer};
use super::schema::{component_body, SchemaMapper};
use super::types::{DocumentVersion, Finding, RenderConfig};
use super::version::{GENERATOR_ID, GENERATOR_VERSION, MAX_DOCUMENT_BYTES, MAX_OPERATIONS};

/// The fixed success-response description (bounded generator
/// vocabulary, never invented prose).
const SUCCESS_DESCRIPTION: &str = "Success response.";
/// The fixed error-response description.
const ERROR_DESCRIPTION: &str = "Error response.";

/// One finished rendering: the canonical JSON document plus its
/// digest and the projection findings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenApiDocument {
    root: Json,
    canonical: String,
    digest: String,
    findings: Vec<Finding>,
    /// `(json pointer, endpoint id)` of every rendered operation,
    /// byte-sorted by pointer — the fragment/ownership unit.
    operation_pointers: Vec<(String, String)>,
}

impl OpenApiDocument {
    /// The document root as a JSON value.
    pub fn root(&self) -> &Json {
        &self.root
    }

    /// The canonical bytes (compact JSON, byte-sorted keys, no
    /// trailing LF).
    pub fn canonical_bytes(&self) -> &str {
        &self.canonical
    }

    /// The digest of the canonical bytes (`sha256:<hex>`).
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Every not-expressible member, byte-sorted and deduplicated.
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// Every rendered operation pointer with its endpoint id.
    pub fn operation_pointers(&self) -> &[(String, String)] {
        &self.operation_pointers
    }
}

/// Render one attachment into the declared OpenAPI document. Pure and
/// read-only; the attachment must already validate against the bound
/// context (the CLI validates before rendering, exactly like the
/// route-surface projection).
pub fn render(
    document: &TransportDocument,
    context: &ValidationContext<'_>,
    config: &RenderConfig,
) -> Result<OpenApiDocument, DiagnosticSet> {
    let version = config.version;
    if document.endpoints().len() > MAX_OPERATIONS {
        return Err(diagnostic::export_limit_set(MAX_OPERATIONS));
    }
    // G3: `mutualTLS` exists only in OpenAPI 3.1; a declared 3.0
    // render with a mutual-tls scheme refuses honestly — never a
    // silent downgrade.
    if version == DocumentVersion::V3_0
        && document
            .schemes()
            .iter()
            .any(|scheme| scheme.kind == SchemeKind::MutualTls)
    {
        return Err(diagnostic::version_unsupported("scheme-kind", "3.0"));
    }
    let project = context.project;
    let mut mapper = SchemaMapper::new(project, version);
    let mut path_items: BTreeMap<String, Map<String, Json>> = BTreeMap::new();
    let mut operation_pointers: Vec<(String, String)> = Vec::new();
    let mut error_names: BTreeMap<String, String> = BTreeMap::new();
    let mut taken: BTreeMap<String, String> = BTreeMap::new();

    // Pre-pass: the per-error identity variants claim their component
    // names first, so a rendered operation can reference them; type
    // components discovered below yield on collision (the `Error`
    // suffix walk), never the reverse.
    if let Some(registry) = context.errors {
        let mut bases: BTreeMap<String, String> = BTreeMap::new();
        for binding in document.endpoints() {
            for entry in &binding.errors {
                bases
                    .entry(entry.error.as_str().to_owned())
                    .or_insert_with(|| component_name(entry.error.as_str()));
            }
        }
        for (symbol, base) in bases {
            if registry_error(registry, &symbol).is_none() {
                continue;
            }
            taken.insert(base.clone(), symbol.clone());
            error_names.insert(symbol, base);
        }
    }

    // Pass one: every operation, registering the type components it
    // references.
    for binding in document.endpoints() {
        let endpoint = resolve_endpoint(project, binding.endpoint.as_str())?;
        let method = endpoint.method.as_str().to_ascii_lowercase();
        let operation = operation_json(
            document,
            binding,
            endpoint,
            &mut mapper,
            context,
            &error_names,
        )?;
        let template = endpoint.path.as_str().to_owned();
        operation_pointers.push((
            paths_pointer(&template, &method),
            binding.endpoint.as_str().to_owned(),
        ));
        path_items
            .entry(template)
            .or_default()
            .insert(method, operation);
    }
    operation_pointers.sort();

    // Pass two: reusable components. The claimed error names first,
    // then the type components. Rendering a body may register further
    // components (a nested field's own refs), so the pass runs to a
    // fixpoint — every named ref resolves to a rendered body.
    let mut schemas: BTreeMap<String, Json> = BTreeMap::new();
    if let Some(registry) = context.errors {
        for (symbol, base) in &error_names {
            let Some(error) = registry_error(registry, symbol) else {
                continue;
            };
            let body = error_variant(error, &mut mapper, version);
            schemas.insert(base.clone(), with_x_symbol(body, symbol));
        }
    }
    let mut processed: std::collections::BTreeSet<String> = error_names.keys().cloned().collect();
    loop {
        let registered: Vec<(String, String)> = mapper
            .components()
            .iter()
            .map(|(symbol, name)| (symbol.clone(), name.clone()))
            .filter(|(symbol, _)| !processed.contains(symbol))
            .collect();
        if registered.is_empty() {
            break;
        }
        for (symbol, base) in registered {
            processed.insert(symbol.clone());
            let definition = project
                .definitions
                .iter()
                .find(|definition| definition.id().as_str() == symbol)
                .expect("registered component resolves");
            let body =
                component_body(definition, &mut mapper).expect("registered kinds have bodies");
            let mut serial = 0usize;
            let mut name = base.clone();
            while taken.contains_key(&name) {
                serial += 1;
                name = if serial == 1 {
                    format!("{base}Error")
                } else {
                    format!("{base}Error{serial}")
                };
            }
            taken.insert(name.clone(), symbol.clone());
            schemas.insert(name, with_x_symbol(body, &symbol));
        }
    }

    // The shared category responses: `Error<Category>` entries derived
    // from the document-wide error defaults. A category whose default
    // status is uniform across every endpoint renders once; a divided
    // default stays per-operation — never an ambiguous shared entry.
    let mut responses: BTreeMap<String, Json> = BTreeMap::new();
    for (category, status) in uniform_defaults(document) {
        let name = format!("Error{}", pascal(&category));
        responses.insert(name, category_response(version, &category, status));
    }

    // Security schemes: every declared, renderable scheme once.
    let mut security_schemes: BTreeMap<String, Json> = BTreeMap::new();
    for scheme in document.schemes() {
        if let Some(body) = scheme_json(scheme, version) {
            security_schemes.insert(scheme.id.as_str().to_owned(), body);
        } else {
            mapper.partial(scheme.id.as_str(), "scheme-not-expressible");
        }
    }

    let mut paths = Map::new();
    for (template, item) in path_items {
        paths.insert(template, Json::Object(item));
    }

    let mut root = Map::new();
    root.insert(
        "openapi".to_owned(),
        Json::String(version.wire_str().to_owned()),
    );
    let mut info = Map::new();
    info.insert(
        "title".to_owned(),
        Json::String(document.project_id().as_str().to_owned()),
    );
    info.insert(
        "version".to_owned(),
        Json::String(document.attachment_revision().to_string()),
    );
    root.insert("info".to_owned(), Json::Object(info));
    root.insert("paths".to_owned(), Json::Object(paths));

    if !schemas.is_empty() || !responses.is_empty() || !security_schemes.is_empty() {
        let mut components = Map::new();
        if !schemas.is_empty() {
            components.insert("schemas".to_owned(), to_object(schemas));
        }
        if !responses.is_empty() {
            components.insert("responses".to_owned(), to_object(responses));
        }
        if !security_schemes.is_empty() {
            components.insert("securitySchemes".to_owned(), to_object(security_schemes));
        }
        root.insert("components".to_owned(), Json::Object(components));
    }

    // The self-pinning provenance block.
    let mut provenance = Map::new();
    let mut model_ref = Map::new();
    model_ref.insert(
        "modelVersion".to_owned(),
        Json::String(document.model_ref().version().as_str().to_owned()),
    );
    model_ref.insert(
        "digest".to_owned(),
        Json::String(document.model_ref().digest().as_str().to_owned()),
    );
    provenance.insert("modelRef".to_owned(), Json::Object(model_ref));
    let mut ir_ref = Map::new();
    ir_ref.insert(
        "identity".to_owned(),
        Json::String(crate::transport_http::IR_IDENTITY.to_owned()),
    );
    ir_ref.insert(
        "digest".to_owned(),
        Json::String(document.ir_digest().as_str().to_owned()),
    );
    provenance.insert("irRef".to_owned(), Json::Object(ir_ref));
    let mut transport_ref = Map::new();
    transport_ref.insert(
        "schemaVersion".to_owned(),
        Json::String(crate::transport_http::SCHEMA_VERSION.to_owned()),
    );
    transport_ref.insert(
        "digest".to_owned(),
        Json::String(document.digest()?.as_str().to_owned()),
    );
    provenance.insert("transportRef".to_owned(), Json::Object(transport_ref));
    let mut generator = Map::new();
    generator.insert("id".to_owned(), Json::String(GENERATOR_ID.to_owned()));
    generator.insert(
        "version".to_owned(),
        Json::String(GENERATOR_VERSION.to_owned()),
    );
    provenance.insert("generator".to_owned(), Json::Object(generator));
    root.insert("x-lekalo-provenance".to_owned(), Json::Object(provenance));

    let findings = mapper.findings();
    let canonical = Json::Object(root.clone()).to_string();
    if canonical.len() > MAX_DOCUMENT_BYTES {
        return Err(diagnostic::export_limit_set(MAX_DOCUMENT_BYTES));
    }
    let digest = format!("sha256:{}", crate::digest::sha256_hex(canonical.as_bytes()));
    Ok(OpenApiDocument {
        root: Json::Object(root),
        canonical,
        digest,
        findings,
        operation_pointers,
    })
}

/// One rendered operation object.
fn operation_json(
    document: &TransportDocument,
    binding: &EndpointBinding,
    endpoint: &EndpointDef,
    mapper: &mut SchemaMapper,
    context: &ValidationContext<'_>,
    error_names: &BTreeMap<String, String>,
) -> Result<Json, DiagnosticSet> {
    let subject = binding.endpoint.as_str();
    let version = mapper.version();
    let mut operation = Map::new();
    operation.insert(
        "operationId".to_owned(),
        Json::String(binding.effective_operation_id().as_str().to_owned()),
    );
    if !binding.tags.is_empty() {
        operation.insert(
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
        operation.insert("summary".to_owned(), Json::String(summary.clone()));
    }
    operation.insert(
        "x-lekalo-endpoint".to_owned(),
        Json::String(subject.to_owned()),
    );
    operation.insert(
        "x-lekalo-operation".to_owned(),
        Json::String(endpoint.invokes.as_str().to_owned()),
    );

    let query_decl = context
        .query_model
        .and_then(|model| model.query(endpoint.invokes.as_str()));
    let command_input = command_input(context.project, endpoint.invokes.as_str());
    let mut parameters: Vec<Json> = Vec::new();
    for param in &binding.params {
        parameters.push(param_json(param, command_input, query_decl, mapper));
    }
    if let Some(pagination) = &binding.pagination {
        let names: Vec<_> = [
            Some(&pagination.limit_param),
            pagination.offset_param.as_ref(),
            pagination.cursor_param.as_ref(),
        ]
        .into_iter()
        .flatten()
        .collect();
        for name in names {
            parameters.push(json!({
                "name": name.as_str(),
                "in": "query",
                "required": false,
                "schema": pagination_param_schema(name.as_str(), query_decl, mapper),
            }));
        }
    }
    if let Some(idempotency) = &binding.idempotency {
        parameters.push(json!({
            "name": idempotency.header.as_str(),
            "in": "header",
            "required": idempotency.required,
            "schema": {},
        }));
    }
    if let Some(correlation) = &binding.correlation {
        for header in &correlation.headers {
            parameters.push(json!({
                "name": header.as_str(),
                "in": "header",
                "required": false,
                "schema": {},
            }));
        }
    }
    if let Some(api_version) = &binding.api_version {
        if api_version.location == ApiVersionLocation::Header {
            parameters.push(json!({
                "name": api_version.name.as_str(),
                "in": "header",
                "required": false,
                "schema": {},
            }));
        }
    }
    if !parameters.is_empty() {
        operation.insert("parameters".to_owned(), Json::Array(parameters));
    }

    if let Some(body) = &binding.body {
        let schema = request_body_schema(body, command_input, query_decl, subject, mapper);
        operation.insert(
            "requestBody".to_owned(),
            json!({
                "required": true,
                "content": { "application/json": { "schema": schema } },
            }),
        );
    }

    let responses = responses_json(binding, endpoint, mapper, context, error_names)?;
    operation.insert("responses".to_owned(), responses);

    if let Some(auth) = &binding.auth {
        match security_json(document, auth, version) {
            SecurityRendering::Public => {
                operation.insert("security".to_owned(), Json::Array(Vec::new()));
            }
            SecurityRendering::Schemes(requirement) => {
                operation.insert("security".to_owned(), json!([to_requirement(requirement)]));
            }
            SecurityRendering::Annotated(unrenderable) => {
                operation.insert("x-lekalo-scheme".to_owned(), to_string_array(&unrenderable));
            }
            SecurityRendering::Mixed(requirement, unrenderable) => {
                operation.insert("security".to_owned(), json!([to_requirement(requirement)]));
                operation.insert("x-lekalo-scheme".to_owned(), to_string_array(&unrenderable));
            }
        }
        if let Some(policy) = &auth.policy_ref {
            operation.insert(
                "x-lekalo-policy".to_owned(),
                Json::String(policy.as_str().to_owned()),
            );
        }
    }

    if let Some(rate_limit) = &binding.rate_limit {
        operation.insert(
            "x-lekalo-rate-limit".to_owned(),
            json!({
                "limit": rate_limit.limit,
                "windowSeconds": rate_limit.window_seconds,
                "scope": rate_limit.scope.as_str(),
            }),
        );
    }
    if let Some(cache) = &binding.cache {
        operation.insert(
            "x-lekalo-cache".to_owned(),
            json!({
                "policy": cache.policy.as_str(),
                "maxAgeSeconds": cache.max_age_seconds,
                "etag": cache.etag,
            }),
        );
    }
    if let Some(api_version) = &binding.api_version {
        if api_version.location == ApiVersionLocation::Path {
            operation.insert(
                "x-lekalo-api-version".to_owned(),
                json!({ "in": "path", "name": api_version.name.as_str() }),
            );
        }
    }
    if !binding.capabilities.is_empty() {
        operation.insert(
            "x-lekalo-capabilities".to_owned(),
            Json::Array(
                binding
                    .capabilities
                    .iter()
                    .map(|decl| {
                        json!({
                            "capability": decl.capability.as_str(),
                            "minimumSupport": decl.minimum_support.as_str(),
                            "detail": decl.detail.as_str(),
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Some(decl) = query_decl {
        if let Some(filter) = &decl.filter {
            operation.insert("x-lekalo-filter".to_owned(), filter_json(filter));
        }
        if let Some(sort) = &decl.sort {
            operation.insert(
                "x-lekalo-sort".to_owned(),
                Json::Array(
                    sort.iter()
                        .map(|key| {
                            json!({
                                "field": key.field.as_str(),
                                "direction": match key.direction {
                                    crate::query_model::Direction::Asc => "asc",
                                    crate::query_model::Direction::Desc => "desc",
                                },
                            })
                        })
                        .collect(),
                ),
            );
        }
    }
    Ok(Json::Object(operation))
}

/// One declared parameter with its resolved schema.
fn param_json(
    param: &crate::transport_http::types::ParamBinding,
    command_input: Option<&Vec<Field>>,
    query_decl: Option<&QueryDecl>,
    mapper: &mut SchemaMapper,
) -> Json {
    let mut object = Map::new();
    object.insert(
        "name".to_owned(),
        Json::String(param.name.as_str().to_owned()),
    );
    object.insert(
        "in".to_owned(),
        Json::String(param.location.as_str().to_owned()),
    );
    object.insert("required".to_owned(), Json::Bool(param.required));
    if let Some(style) = param.style {
        object.insert("style".to_owned(), Json::String(style.as_str().to_owned()));
    }
    if let Some(explode) = param.explode {
        object.insert("explode".to_owned(), Json::Bool(explode));
    }
    object.insert(
        "schema".to_owned(),
        field_schema(&param.field, command_input, query_decl, mapper),
    );
    Json::Object(object)
}

/// The schema of one field reference: a command input member's type or
/// a declared query-model parameter's type. An unresolvable reference
/// is a finding with an open schema — never a guessed shape.
fn field_schema(
    field: &crate::transport_http::FieldRef,
    command_input: Option<&Vec<Field>>,
    query_decl: Option<&QueryDecl>,
    mapper: &mut SchemaMapper,
) -> Json {
    if field.is_input() {
        if let Some(fields) = command_input {
            if let Some(member) = fields
                .iter()
                .find(|member| member.name.as_str() == field.name().as_str())
            {
                return mapper.map_type(&member.r#type);
            }
        }
        mapper.partial(field.name().as_str(), "input-member-unresolved");
        return json!({});
    }
    if let Some(decl) = query_decl {
        if let Some(parameter) = decl
            .parameters
            .iter()
            .find(|parameter| parameter.name.as_str() == field.name().as_str())
        {
            return type_expr_json(&parameter.parameter_type, mapper);
        }
    }
    mapper.partial(field.name().as_str(), "parameter-unresolved");
    json!({})
}

/// The schema of one pagination parameter: resolved through the bound
/// query-model declaration when the parameter is declared there;
/// otherwise open — the wire declares the name, never a type.
fn pagination_param_schema(
    name: &str,
    query_decl: Option<&QueryDecl>,
    mapper: &mut SchemaMapper,
) -> Json {
    if let Some(decl) = query_decl {
        if let Some(parameter) = decl
            .parameters
            .iter()
            .find(|parameter| parameter.name.as_str() == name)
        {
            return type_expr_json(&parameter.parameter_type, mapper);
        }
    }
    json!({})
}

/// The closed query-model parameter type-expression spelling:
/// `planner.symbol` or `list<planner.symbol>`. Anything else is a
/// finding with an open schema.
fn type_expr_json(text: &str, mapper: &mut SchemaMapper) -> Json {
    let text = text.trim();
    if let Some(inner) = text
        .strip_prefix("list<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        return json!({ "type": "array", "items": type_expr_json(inner, mapper) });
    }
    if text.is_empty()
        || !text.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '.' || character == '_'
        })
    {
        mapper.partial(text, "parameter-type-unresolved");
        return json!({});
    }
    mapper.map_symbol(text)
}

/// One request-body schema: `whole-input` is the invoked command's
/// declared input object; `explicit` is the declared field subset.
fn request_body_schema(
    body: &BodyBinding,
    command_input: Option<&Vec<Field>>,
    query_decl: Option<&QueryDecl>,
    subject: &str,
    mapper: &mut SchemaMapper,
) -> Json {
    match body.mode {
        ProjectionMode::Whole => match command_input {
            Some(fields) => mapper.object_schema(fields),
            None => {
                mapper.partial(subject, "input-undeclared");
                json!({})
            }
        },
        ProjectionMode::Explicit => {
            let mut properties = Map::new();
            let mut required: Vec<String> = Vec::new();
            for field in &body.fields {
                let schema = field_schema(&field.field, command_input, query_decl, mapper);
                properties.insert(field.name.as_str().to_owned(), schema);
                if field.required {
                    required.push(field.name.as_str().to_owned());
                }
            }
            explicit_object(properties, required)
        }
    }
}

/// One success-body schema: `whole-output` is the invoked query's
/// declared return type (a command output is a projection-partial
/// finding — never an invented shape); `explicit` resolves each
/// declared field against the query's returned object fields.
fn success_body_schema(
    body: &BodyBinding,
    returns: Option<&TypeRef>,
    project: &crate::ir::CompiledProject,
    subject: &str,
    mapper: &mut SchemaMapper,
) -> Json {
    match body.mode {
        ProjectionMode::Whole => match returns {
            Some(typeref) => mapper.map_type(typeref),
            None => {
                mapper.partial(subject, "output-undeclared");
                json!({})
            }
        },
        ProjectionMode::Explicit => {
            // The declared output fields of the returned named object.
            let returned_fields: Option<&Vec<Field>> = match returns {
                Some(TypeRef::Ref(symbol)) => {
                    project
                        .definitions
                        .iter()
                        .find_map(|definition| match definition {
                            Definition::Entity(entity) if entity.id.as_str() == symbol.as_str() => {
                                Some(&entity.fields)
                            }
                            Definition::ValueObject(value_object)
                                if value_object.id.as_str() == symbol.as_str() =>
                            {
                                Some(&value_object.fields)
                            }
                            _ => None,
                        })
                }
                _ => None,
            };
            let mut properties = Map::new();
            let mut required: Vec<String> = Vec::new();
            for field in &body.fields {
                let schema = match returned_fields.and_then(|fields| {
                    fields
                        .iter()
                        .find(|member| member.name.as_str() == field.field.name().as_str())
                }) {
                    Some(member) => mapper.map_type(&member.r#type),
                    None => {
                        mapper.partial(field.field.name().as_str(), "output-member-unresolved");
                        json!({})
                    }
                };
                properties.insert(field.name.as_str().to_owned(), schema);
                if field.required {
                    required.push(field.name.as_str().to_owned());
                }
            }
            explicit_object(properties, required)
        }
    }
}

/// One explicit-projection object schema.
fn explicit_object(properties: Map<String, Json>, required: Vec<String>) -> Json {
    let mut object = Map::new();
    object.insert("type".to_owned(), Json::String("object".to_owned()));
    object.insert("properties".to_owned(), Json::Object(properties));
    if !required.is_empty() {
        let mut names = required;
        names.sort();
        let array: Vec<Json> = names.into_iter().map(Json::String).collect();
        object.insert("required".to_owned(), Json::Array(array));
    }
    object.insert("additionalProperties".to_owned(), Json::Bool(false));
    Json::Object(object)
}

/// The responses object of one operation.
fn responses_json(
    binding: &EndpointBinding,
    endpoint: &EndpointDef,
    mapper: &mut SchemaMapper,
    context: &ValidationContext<'_>,
    error_names: &BTreeMap<String, String>,
) -> Result<Json, DiagnosticSet> {
    let subject = binding.endpoint.as_str();
    let mut responses = Map::new();

    // The success projection.
    let mut success = Map::new();
    success.insert(
        "description".to_owned(),
        Json::String(SUCCESS_DESCRIPTION.to_owned()),
    );
    if binding.success.status != 204 {
        let returns = query_returns(context.project, endpoint.invokes.as_str());
        let schema = match &binding.success.body {
            Some(body) => success_body_schema(body, returns, context.project, subject, mapper),
            None => json!({}),
        };
        let mut content = Map::new();
        content.insert("application/json".to_owned(), json!({ "schema": schema }));
        // Declared server-sent events ride beside the JSON projection.
        if binding
            .capabilities
            .iter()
            .any(|decl| decl.capability.as_str() == "streaming" && decl.detail.as_str() == "sse")
        {
            content.insert("text/event-stream".to_owned(), json!({}));
        }
        success.insert("content".to_owned(), Json::Object(content));
    }
    if !binding.success.headers.is_empty() {
        let mut headers = Map::new();
        for header in &binding.success.headers {
            headers.insert(
                header.name.as_str().to_owned(),
                json!({ "required": header.required, "schema": {} }),
            );
        }
        success.insert("headers".to_owned(), Json::Object(headers));
    }
    if let Some(pagination) = &binding.pagination {
        if let Some(field) = &pagination.cursor_field {
            success.insert(
                "x-lekalo-cursor-field".to_owned(),
                Json::String(field.as_str().to_owned()),
            );
        }
    }
    responses.insert(binding.success.status.to_string(), Json::Object(success));

    // The declared error statuses, deduplicated per status: several
    // error ids sharing a status collapse to `oneOf` of their identity
    // variants — each still keyed by the immutable error id.
    let mut by_status: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    for entry in &binding.errors {
        by_status
            .entry(entry.status)
            .or_default()
            .push(entry.error.as_str().to_owned());
    }
    for (status, errors) in &by_status {
        let variants: Vec<Json> = errors
            .iter()
            .map(|error| match error_names.get(error) {
                Some(name) => json!({ "$ref": schemas_pointer(name) }),
                None => {
                    // No bound registry: the identity variant was never
                    // rendered, so the status response stays open and
                    // the gap is reported — never a dangling ref.
                    mapper.partial(error, "error-variant-unrendered");
                    json!({})
                }
            })
            .collect();
        let schema = if variants.len() == 1 {
            variants[0].clone()
        } else {
            json!({ "oneOf": variants })
        };
        responses.insert(
            status.to_string(),
            json!({
                "description": ERROR_DESCRIPTION,
                "content": { "application/json": { "schema": schema } },
            }),
        );
    }

    // The category defaults: statuses not covered by a declared entry
    // reference the shared `Error<Category>` response. Status zero is
    // the declared "no projection" and never serializes.
    for (category, status) in defaults_members(&binding.error_defaults) {
        if status == 0 || by_status.contains_key(&status) {
            continue;
        }
        if responses.contains_key(&status.to_string()) {
            continue;
        }
        let name = format!("Error{}", pascal(&category));
        responses.insert(
            status.to_string(),
            json!({
                "$ref": format!("#/components/responses/{}", escape_pointer(&name))
            }),
        );
    }
    Ok(Json::Object(responses))
}

/// One shared category response body: the envelope with the category
/// pinned and the infrastructure rule documented — no declared
/// id/code, never masquerading as a declared error.
fn category_response(version: DocumentVersion, category: &str, status: u16) -> Json {
    let _ = status;
    json!({
        "description": ERROR_DESCRIPTION,
        "content": { "application/json": { "schema": {
            "type": "object",
            "required": ["error", "ok"],
            "properties": {
                "ok": constant(version, Json::Bool(false)),
                "error": {
                    "type": "object",
                    "required": ["category", "payload"],
                    "properties": {
                        "category": constant(version, Json::String(category.to_owned())),
                        "payload": { "type": "object" }
                    },
                    "additionalProperties": false
                }
            },
            "additionalProperties": false
        } } }
    })
}

/// The #62 identity variant of one declared error: the envelope with
/// the identity quadruple pinned and the public payload fields only.
fn error_variant(
    error: &crate::error_contract::ErrorContract,
    mapper: &mut SchemaMapper,
    version: DocumentVersion,
) -> Json {
    let mut payload_properties = Map::new();
    let mut payload_required: Vec<String> = Vec::new();
    for field in error.payload().public_fields() {
        payload_properties.insert(
            field.name().to_owned(),
            mapper.map_error_type(field.field_type()),
        );
        if field.required() {
            payload_required.push(field.name().to_owned());
        }
    }
    let payload = explicit_object(payload_properties, payload_required);
    json!({
        "type": "object",
        "required": ["error", "ok"],
        "properties": {
            "ok": constant(version, Json::Bool(false)),
            "error": {
                "type": "object",
                "required": ["category", "code", "id", "payload"],
                "properties": {
                    "id": constant(version, Json::String(error.id().as_str().to_owned())),
                    "code": constant(version, Json::String(error.code().as_str().to_owned())),
                    "category": constant(
                        version,
                        Json::String(error.category().as_str().to_owned()),
                    ),
                    "payload": payload
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}

/// One `const` (or the 3.0 single-value `enum`) value.
fn constant(version: DocumentVersion, value: Json) -> Json {
    match version {
        DocumentVersion::V3_1 => json!({ "const": value }),
        DocumentVersion::V3_0 => json!({ "enum": [value] }),
    }
}

/// The document-wide uniform category defaults, byte-sorted by
/// category. A divided default is omitted — the per-operation
/// responses carry it, never an ambiguous shared entry.
fn uniform_defaults(document: &TransportDocument) -> Vec<(String, u16)> {
    let mut bindings = document.endpoints().iter();
    let Some(first) = bindings.next() else {
        return Vec::new();
    };
    let mut uniform: Vec<(String, u16)> = defaults_members(&first.error_defaults);
    for binding in bindings {
        let members = defaults_members(&binding.error_defaults);
        uniform.retain(|(category, status)| {
            members
                .iter()
                .any(|(other, other_status)| other == category && other_status == status)
        });
    }
    uniform.sort();
    uniform
}

/// The six closed category members of one defaults block.
fn defaults_members(defaults: &ErrorDefaults) -> Vec<(String, u16)> {
    vec![
        ("auth".to_owned(), defaults.auth),
        ("conflict".to_owned(), defaults.conflict),
        ("domain".to_owned(), defaults.domain),
        ("infrastructure".to_owned(), defaults.infrastructure),
        ("not-found".to_owned(), defaults.not_found),
        ("validation".to_owned(), defaults.validation),
    ]
}

/// `validation` → `Validation` (the Pascal rule over one token).
fn pascal(text: &str) -> String {
    text.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The security rendering of one declared auth binding.
enum SecurityRendering {
    /// The endpoint is explicitly public.
    Public,
    /// Every declared scheme renders the native requirement object.
    Schemes(BTreeMap<String, Vec<Json>>),
    /// No scheme renders natively; the ids stay annotated.
    Annotated(Vec<String>),
    /// The native requirement plus the annotated remainder.
    Mixed(BTreeMap<String, Vec<Json>>, Vec<String>),
}

/// Project one auth binding onto the operation security vocabulary.
fn security_json(
    document: &TransportDocument,
    auth: &AuthBinding,
    version: DocumentVersion,
) -> SecurityRendering {
    if auth.actor.as_str() == "public" {
        return SecurityRendering::Public;
    }
    let mut requirement: BTreeMap<String, Vec<Json>> = BTreeMap::new();
    let mut unrenderable: Vec<String> = Vec::new();
    for scheme_ref in &auth.schemes {
        let Some(scheme) = document.scheme(scheme_ref.as_str()) else {
            unrenderable.push(scheme_ref.as_str().to_owned());
            continue;
        };
        match scheme.kind {
            SchemeKind::Oauth2 | SchemeKind::Custom => {
                // The native shapes require declared flow URLs the
                // attachment deliberately forbids (ADR-0042 §4); the
                // id stays annotated, never masqueraded.
                unrenderable.push(scheme_ref.as_str().to_owned());
            }
            SchemeKind::MutualTls if version == DocumentVersion::V3_0 => {
                unrenderable.push(scheme_ref.as_str().to_owned());
            }
            _ => {
                requirement.insert(scheme_ref.as_str().to_owned(), Vec::new());
            }
        }
    }
    if requirement.is_empty() {
        SecurityRendering::Annotated(unrenderable)
    } else if unrenderable.is_empty() {
        SecurityRendering::Schemes(requirement)
    } else {
        SecurityRendering::Mixed(requirement, unrenderable)
    }
}

/// One declared security scheme, or nothing when the scheme has no
/// native OpenAPI shape at the declared version.
fn scheme_json(
    scheme: &crate::transport_http::types::SecurityScheme,
    version: DocumentVersion,
) -> Option<Json> {
    let body = match scheme.kind {
        SchemeKind::None => return None,
        SchemeKind::Bearer => {
            let mut object = Map::new();
            object.insert("type".to_owned(), Json::String("http".to_owned()));
            object.insert("scheme".to_owned(), Json::String("bearer".to_owned()));
            if let Some(format) = &scheme.format {
                object.insert(
                    "bearerFormat".to_owned(),
                    Json::String(format.as_str().to_owned()),
                );
            }
            Json::Object(object)
        }
        SchemeKind::ApiKey => {
            let mut object = Map::new();
            object.insert("type".to_owned(), Json::String("apiKey".to_owned()));
            if let Some(location) = scheme.location {
                object.insert("in".to_owned(), Json::String(location.as_str().to_owned()));
            }
            if let Some(name) = &scheme.name {
                object.insert("name".to_owned(), Json::String(name.as_str().to_owned()));
            }
            Json::Object(object)
        }
        SchemeKind::Basic => json!({ "type": "http", "scheme": "basic" }),
        SchemeKind::MutualTls if version == DocumentVersion::V3_1 => {
            json!({ "type": "mutualTLS" })
        }
        // oauth2/custom: no native shape without invented URLs or
        // semantics; mutual-tls at 3.0: not expressible (G3/G4).
        _ => return None,
    };
    Some(with_x_symbol(body, scheme.id.as_str()))
}

/// The declared #62 error of one id, or nothing.
fn registry_error<'a>(
    registry: &'a crate::error_contract::ErrorRegistry,
    symbol: &str,
) -> Option<&'a crate::error_contract::ErrorContract> {
    let id = crate::error_contract::id::ErrorId::new(symbol)?;
    registry.error(&id)
}

/// Attach the semantic-id anchor to one reusable component.
fn with_x_symbol(mut body: Json, symbol: &str) -> Json {
    if let Json::Object(object) = &mut body {
        object.insert(
            "x-lekalo-symbol".to_owned(),
            Json::String(symbol.to_owned()),
        );
    }
    body
}

/// The closed filter-expression annotation spelling (G5: declared
/// semantics, projected — never invented).
fn filter_json(filter: &crate::query_model::FilterExpr) -> Json {
    match filter {
        crate::query_model::FilterExpr::Leaf(leaf) => {
            let mut object = Map::new();
            object.insert(
                "field".to_owned(),
                Json::String(leaf.field.as_str().to_owned()),
            );
            object.insert("op".to_owned(), Json::String(leaf.op.as_str().to_owned()));
            if let Some(value) = &leaf.value {
                object.insert("value".to_owned(), filter_value_json(value));
            }
            Json::Object(object)
        }
        crate::query_model::FilterExpr::And(operands) => {
            json!({ "all": operands.iter().map(filter_json).collect::<Vec<_>>() })
        }
        crate::query_model::FilterExpr::Or(operands) => {
            json!({ "any": operands.iter().map(filter_json).collect::<Vec<_>>() })
        }
        crate::query_model::FilterExpr::Not(operand) => {
            json!({ "not": filter_json(operand) })
        }
    }
}

/// One closed filter value.
fn filter_value_json(value: &crate::query_model::FilterValue) -> Json {
    match value {
        crate::query_model::FilterValue::Param(name) => {
            json!({ "param": name.as_str() })
        }
        crate::query_model::FilterValue::Literal(literal) => literal_json(literal),
    }
}

/// One closed literal.
fn literal_json(literal: &crate::query_model::Literal) -> Json {
    match literal {
        crate::query_model::Literal::Bool(value) => Json::Bool(*value),
        crate::query_model::Literal::Integer(value) => json!(value),
        crate::query_model::Literal::Decimal(value)
        | crate::query_model::Literal::Str(value)
        | crate::query_model::Literal::Date(value)
        | crate::query_model::Literal::DateTime(value)
        | crate::query_model::Literal::Uuid(value)
        | crate::query_model::Literal::Uri(value)
        | crate::query_model::Literal::Enum(value) => Json::String(value.clone()),
        crate::query_model::Literal::Set(values) => {
            Json::Array(values.iter().map(literal_json).collect())
        }
    }
}

/// Resolve one endpoint definition, or refuse.
fn resolve_endpoint<'a>(
    project: &'a crate::ir::CompiledProject,
    subject: &str,
) -> Result<&'a EndpointDef, DiagnosticSet> {
    project
        .definitions
        .iter()
        .find_map(|definition| match definition {
            Definition::Endpoint(def) if def.id.as_str() == subject => Some(def),
            _ => None,
        })
        .ok_or_else(|| diagnostic::binding_unresolved(subject, "symbol-missing"))
}

/// The declared input members of one invoked command, when command.
fn command_input<'a>(
    project: &'a crate::ir::CompiledProject,
    operation: &str,
) -> Option<&'a Vec<Field>> {
    project
        .definitions
        .iter()
        .find_map(|definition| match definition {
            Definition::Command(command) if command.id.as_str() == operation => {
                Some(&command.input)
            }
            _ => None,
        })
}

/// The declared return type of one invoked query, when query.
fn query_returns<'a>(
    project: &'a crate::ir::CompiledProject,
    operation: &str,
) -> Option<&'a TypeRef> {
    project
        .definitions
        .iter()
        .find_map(|definition| match definition {
            Definition::Query(query) if query.id.as_str() == operation => query.returns.as_ref(),
            _ => None,
        })
}

/// One security requirement object: scheme ids to the empty scope list.
fn to_requirement(map: BTreeMap<String, Vec<Json>>) -> Json {
    Json::Object(
        map.into_iter()
            .map(|(name, scopes)| (name, Json::Array(scopes)))
            .collect(),
    )
}

/// One byte-sorted object from a byte-sorted map.
fn to_object(map: BTreeMap<String, Json>) -> Json {
    Json::Object(map.into_iter().collect())
}

/// One byte-sorted string array.
fn to_string_array(values: &[String]) -> Json {
    let mut sorted: Vec<String> = values.to_vec();
    sorted.sort();
    Json::Array(sorted.into_iter().map(Json::String).collect())
}
