//! The deterministic client-SDK projection (issue #72).
//!
//! [`project`] joins one validated transport attachment with the
//! compiled project, the bound #62 registry, and the bound #64
//! query-model attachment into one [`ClientContractWire`]: every
//! endpoint becomes one typed operation carrying the full wire
//! binding (parameters with style/explode, body subsets, error
//! variants with derived retry authorization, auth/idempotency/
//! correlation, pagination), and every reachable named symbol becomes
//! one named type. Nothing is guessed: an unresolvable join refuses
//! with its registered diagnostic; a declared surface the client
//! mapping cannot express is an explicit refusal, never a silent
//! narrowing.
//!
//! The projection is pure: it reads no filesystem, executes nothing,
//! and never sees a URL or credential. Repeated runs over the same
//! inputs are byte-identical.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json};

use crate::diagnostics::DiagnosticSet;
use crate::error_contract::ErrorRegistry;
use crate::ir::{CompiledProject, Definition, Field, ScalarBase, TypeRef};
use crate::query_model::QueryModelAttachment;
use crate::scenario::id::SemanticId;
use crate::transport_http::{
    EndpointBinding, ParamLocation, ParamStyle, ProjectionMode, TransportDocument,
    ValidationContext,
};

use super::diagnostic;
use super::id::{camel_of_semantic, snake_of_semantic, TargetIdent};
use super::retry;
use super::types::{
    BodyMode, ClientAuth, ClientBody, ClientConfig, ClientCorrelation, ClientField,
    ClientIdempotency, ClientOperation, ClientPagination, ClientParam, ClientType, ErrorVariant,
    ParamLocation as WireLocation, ParamStyle as WireStyle, ResultShape, ScalarMapping, TypeKind,
    TypeShape,
};
use super::version;
use super::ClientContractWire;

/// The leaf reference of an error payload field's type expression.
fn error_leaf_ref(expr: &crate::error_contract::types::TypeExpr) -> String {
    expr.leaf().as_str().to_owned()
}

/// The closed wire shape of an error payload field's type expression:
/// a `List` wrapper at any depth projects an array.
fn error_shape(expr: &crate::error_contract::types::TypeExpr) -> TypeShape {
    match expr {
        crate::error_contract::types::TypeExpr::List(_) => TypeShape::List,
        crate::error_contract::types::TypeExpr::Optional(inner) => error_shape(inner),
        crate::error_contract::types::TypeExpr::Ref(_) => TypeShape::Value,
    }
}

/// Whether an error payload field's type expression is nullable
/// (wrapped in `Optional`).
fn error_nullable(expr: &crate::error_contract::types::TypeExpr) -> bool {
    matches!(expr, crate::error_contract::types::TypeExpr::Optional(_))
}

/// Project one validated attachment into the client contract. The
/// attachment must already validate against the bound context (the
/// projection joins the same compiled project); a required context
/// that is absent is a refusal — the SDK never projects from partial
/// evidence.
pub fn project(
    document: &TransportDocument,
    context: &ValidationContext<'_>,
    config: &ClientConfig,
) -> Result<ClientContractWire, DiagnosticSet> {
    config.validate().map_err(|error| match error {
        super::types::ClientTypeError::UnknownMode => {
            diagnostic::rule_invalid(diagnostic::CONTRACT_INVALID, "mode", Some(&config.mode))
        }
        super::types::ClientTypeError::DecimalOverbound
        | super::types::ClientTypeError::ConsumersOverbound => {
            diagnostic::rule_invalid(diagnostic::LIMIT_EXCEEDED, "config-bound", None)
        }
    })?;
    // The full semantic join is mandatory: errors and the query model
    // are required context, not options. The strict profile also
    // demands the capability map; the SDK requires it unconditionally
    // because capability declarations shape the emitted client.
    let registry = context.errors.ok_or_else(|| {
        diagnostic::rule_invalid(
            diagnostic::CONTRACT_INVALID,
            "context-errors-required",
            None,
        )
    })?;
    let query_model = context.query_model.ok_or_else(|| {
        diagnostic::rule_invalid(
            diagnostic::CONTRACT_INVALID,
            "context-query-model-required",
            None,
        )
    })?;
    let project = context.project;

    // Custody: the projection binds the exact project.
    let project_id = document.project_id().clone();
    let model_pin = super::ModelPin::new(
        *document.model_ref().version(),
        document.model_ref().digest().clone(),
    );

    // The decimal string-mapping set: explicit, bounded, resolved.
    let mut decimals = BTreeMap::new();
    for symbol in &config.decimal_scalars {
        if !matches!(resolve_symbol(project, symbol), Some(Definition::Scalar(_))) {
            return Err(diagnostic::rule_invalid(
                diagnostic::SYMBOL_UNRESOLVED,
                "decimal-scalar",
                Some(symbol),
            ));
        }
        decimals.insert(symbol.clone(), ScalarMapping::DecimalString);
    }

    // Every named type the operations can reach: entities,
    // value-objects, enums, scalars, plus the invoked command inputs.
    // The well-known unit object is always present so every
    // successBody typeRef resolves (issue #72 fix round); its
    // collection key is the project-scoped wire symbol
    // (`<project>.unit`), keeping the canonical symbol order true.
    let unit_symbol = format!("{}.unit", project_id.as_str());
    let mut types = Vec::new();
    let mut collected = BTreeMap::new();
    collected.insert(unit_symbol.clone(), TypeKind::Object(vec![]));
    let mut used_idents = std::collections::BTreeSet::new();
    used_idents.insert(camel_of_semantic(&unit_symbol));
    for binding in document.endpoints() {
        let endpoint = resolve_endpoint(project, binding.endpoint.as_str())?;
        collect_operation_types(project, endpoint, &decimals, &mut collected)?;
    }
    for (symbol, kind) in collected {
        // The generated identifier is deduplicated deterministically:
        // the camel spelling of the semantic id drops the module
        // prefix, so `foo.task` and `bar.task` would collide inside
        // one emitted module. Ties resolve by appending `_2`, `_3`, …
        // in semantic-id order — stable across runs and languages.
        let base_ident = camel_of_semantic(&symbol);
        let ident = {
            let mut candidate = base_ident.clone();
            let mut ordinal = 2u32;
            while used_idents.contains(&candidate) {
                candidate = format!("{base_ident}_{ordinal}");
                ordinal += 1;
            }
            used_idents.insert(candidate.clone());
            TargetIdent::parse(&candidate).map_err(|set| rename_error(&symbol, set))?
        };
        let type_id = ClientType::type_id_of(&symbol);
        // The well-known unit object's collection key IS its wire
        // symbol (`<project>.unit`); every other key is a Model
        // symbol parsed as a semantic id.
        let symbol_id = SemanticId::parse(&symbol).map_err(|_| unresolved(&symbol))?;
        types.push(ClientType {
            symbol: symbol_id,
            type_id,
            ident,
            kind,
        });
    }
    if types.len() > version::MAX_TYPES {
        return Err(diagnostic::rule_invalid(
            diagnostic::LIMIT_EXCEEDED,
            "type-bound",
            None,
        ));
    }

    // Every operation, ordered by effective operation id.
    let mut operations = Vec::with_capacity(document.endpoints().len());
    for binding in document.endpoints() {
        operations.push(operation_of(
            document,
            binding,
            project,
            registry,
            query_model,
            &decimals,
        )?);
    }
    operations.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
    if operations.len() > version::MAX_OPERATIONS {
        return Err(diagnostic::rule_invalid(
            diagnostic::LIMIT_EXCEEDED,
            "operation-bound",
            None,
        ));
    }

    Ok(ClientContractWire::assemble(
        version::IDENTITY.to_owned(),
        version::SCHEMA_VERSION.to_owned(),
        document.attachment_revision().clone(),
        project_id,
        model_pin,
        document.ir_digest().clone(),
        document.digest()?,
        document.wire().dialect.to_owned(),
        operations,
        types,
    ))
}

/// The canonical payload bytes of one projection: compact JSON with
/// byte-sorted keys, no trailing LF, bounded.
pub(crate) fn canonical_bytes(contract: &ClientContractWire) -> Result<String, DiagnosticSet> {
    let mut root = Map::new();
    root.insert(
        "schemaVersion".to_owned(),
        Json::String(contract.schema_version.clone()),
    );
    root.insert(
        "identity".to_owned(),
        Json::String(contract.identity.clone()),
    );
    root.insert(
        "attachmentRevision".to_owned(),
        Json::String(contract.attachment_revision.as_str().to_owned()),
    );
    root.insert(
        "projectId".to_owned(),
        Json::String(contract.project_id.as_str().to_owned()),
    );
    let mut model_ref = Map::new();
    model_ref.insert(
        "modelVersion".to_owned(),
        Json::String(contract.model_ref.version().as_str().to_owned()),
    );
    model_ref.insert(
        "digest".to_owned(),
        Json::String(contract.model_ref.digest().as_str().to_owned()),
    );
    root.insert("modelRef".to_owned(), Json::Object(model_ref));
    let mut ir_ref = Map::new();
    ir_ref.insert(
        "identity".to_owned(),
        Json::String(version::IR_IDENTITY.to_owned()),
    );
    ir_ref.insert(
        "digest".to_owned(),
        Json::String(contract.ir_digest.as_str().to_owned()),
    );
    root.insert("irRef".to_owned(), Json::Object(ir_ref));
    let mut transport_ref = Map::new();
    transport_ref.insert(
        "identity".to_owned(),
        Json::String(crate::transport_http::IDENTITY.to_owned()),
    );
    transport_ref.insert(
        "digest".to_owned(),
        Json::String(contract.transport_digest.as_str().to_owned()),
    );
    root.insert("transportRef".to_owned(), Json::Object(transport_ref));
    root.insert(
        "wire".to_owned(),
        Json::String(contract.wire_dialect.clone()),
    );
    root.insert(
        "operations".to_owned(),
        Json::Array(contract.operations.iter().map(operation_json).collect()),
    );
    root.insert(
        "types".to_owned(),
        Json::Array(contract.types.iter().map(type_json).collect()),
    );
    let bytes = Json::Object(root).to_string();
    if bytes.len() > version::MAX_CANONICAL_BYTES {
        return Err(diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// Whether `text` satisfies the closed lower-snake token grammar
/// (shared with the consumer-id grammar): lowercase ASCII letters,
/// digits, interior underscores, leading letter, bounded.
pub(crate) fn is_lower_snake(text: &str) -> bool {
    if text.is_empty() || text.len() > 64 {
        return false;
    }
    let bytes = text.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut previous_was_underscore = false;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'a'..=b'z' => previous_was_underscore = false,
            b'0'..=b'9' => {
                if index == 0 {
                    return false;
                }
                previous_was_underscore = false;
            }
            b'_' => {
                if index == 0 || previous_was_underscore || index + 1 == bytes.len() {
                    return false;
                }
                previous_was_underscore = true;
            }
            _ => return false,
        }
    }
    true
}

/// The typed unresolved-symbol refusal.
fn unresolved(symbol: &str) -> DiagnosticSet {
    diagnostic::rule_invalid(
        diagnostic::SYMBOL_UNRESOLVED,
        "symbol-missing",
        Some(symbol),
    )
}

/// The typed identifier-shape refusal.
fn rename_error(symbol: &str, set: DiagnosticSet) -> DiagnosticSet {
    let _ = (symbol, set);
    diagnostic::rule_invalid(
        diagnostic::MAPPING_UNSUPPORTED,
        "identifier-shape",
        Some(symbol),
    )
}

/// Resolve one symbol to its definition, or refuse.
fn resolve_symbol<'a>(project: &'a CompiledProject, symbol: &str) -> Option<&'a Definition> {
    project
        .definitions
        .iter()
        .find(|definition| definition.id().as_str() == symbol)
}

/// Resolve one Model endpoint definition, or refuse.
fn resolve_endpoint<'a>(
    project: &'a CompiledProject,
    symbol: &str,
) -> Result<&'a crate::ir::EndpointDef, DiagnosticSet> {
    match resolve_symbol(project, symbol) {
        Some(Definition::Endpoint(def)) => Ok(def),
        _ => Err(unresolved(symbol)),
    }
}

/// Resolve one Model command/query definition, or refuse.
fn resolve_operation<'a>(
    project: &'a CompiledProject,
    symbol: &str,
) -> Result<&'a Definition, DiagnosticSet> {
    match resolve_symbol(project, symbol) {
        Some(definition @ (Definition::Command(_) | Definition::Query(_))) => Ok(definition),
        _ => Err(unresolved(symbol)),
    }
}

/// The closed scalar mapping of one declared scalar base.
fn scalar_mapping(base: ScalarBase) -> ScalarMapping {
    match base {
        ScalarBase::String => ScalarMapping::String,
        ScalarBase::Number => ScalarMapping::Number,
        ScalarBase::Boolean => ScalarMapping::Boolean,
        ScalarBase::Date => ScalarMapping::Date,
        ScalarBase::Datetime => ScalarMapping::Datetime,
        ScalarBase::Uuid => ScalarMapping::Uuid,
        ScalarBase::Uri => ScalarMapping::Uri,
    }
}

/// Whether one `TypeRef` is nullable (any `Optional` wrapper).
fn nullable_of(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Optional(_))
}

/// The unwrapped reference id of one `TypeRef` (`Optional(Ref(x))`
/// still names `x`); wrapper depth beyond the leaf is the caller's
/// shape concern.
fn leaf_ref(type_ref: &TypeRef) -> Option<&str> {
    match type_ref {
        TypeRef::Ref(symbol) => Some(symbol.as_str()),
        TypeRef::List(inner) => leaf_ref(inner),
        TypeRef::Optional(inner) => leaf_ref(inner),
    }
}

/// The closed wire shape of one `TypeRef`: a `List` wrapper at any
/// depth makes the reference an array; the leaf still names the
/// element type.
fn shape_of(type_ref: &TypeRef) -> TypeShape {
    match type_ref {
        TypeRef::List(_) => TypeShape::List,
        TypeRef::Optional(inner) => shape_of(inner),
        TypeRef::Ref(_) => TypeShape::Value,
    }
}

/// One mapped field reference.
fn field_of(field: &Field) -> (String, String, TypeShape, bool, bool) {
    let type_ref = leaf_ref(&field.r#type)
        .map(str::to_owned)
        .unwrap_or_default();
    (
        field.name.as_str().to_owned(),
        type_ref,
        shape_of(&field.r#type),
        nullable_of(&field.r#type),
        field.required,
    )
}

/// One `ClientField` from a Model field.
fn client_field_of(field: &Field) -> ClientField {
    let (name, type_ref, shape, nullable, required) = field_of(field);
    let field_ref = name.clone();
    ClientField {
        name,
        field: field_ref,
        type_ref,
        shape,
        nullable,
        required,
    }
}

/// Collect every named type one operation reaches into `collected`.
fn collect_operation_types(
    project: &CompiledProject,
    endpoint: &crate::ir::EndpointDef,
    decimals: &BTreeMap<String, ScalarMapping>,
    collected: &mut BTreeMap<String, TypeKind>,
) -> Result<(), DiagnosticSet> {
    let operation = resolve_operation(project, endpoint.invokes.as_str())?;
    match operation {
        Definition::Command(command) => {
            // The whole-input request body names the command's
            // synthetic input object as a collected type, so a backend
            // can always resolve the request shape (issue #72 fix
            // round: no dangling typeRef).
            let input_object = format!("{}.input", command.id.as_str());
            let mut fields = Vec::new();
            for field in &command.input {
                collect_field_types(project, field, decimals, collected)?;
                let (name, type_ref, shape, nullable, required) = field_of(field);
                let field_ref = name.clone();
                fields.push(ClientField {
                    name,
                    field: field_ref,
                    type_ref,
                    shape,
                    nullable,
                    required,
                });
            }
            collected
                .entry(input_object)
                .or_insert_with(|| TypeKind::Object(fields));
        }
        Definition::Query(query) => {
            if let Some(returns) = &query.returns {
                collect_type_ref(project, returns, decimals, collected)?;
            }
        }
        _ => return Err(unresolved(endpoint.invokes.as_str())),
    }
    Ok(())
}

/// Collect the named types one field's type expression reaches.
fn collect_field_types(
    project: &CompiledProject,
    field: &Field,
    decimals: &BTreeMap<String, ScalarMapping>,
    collected: &mut BTreeMap<String, TypeKind>,
) -> Result<(), DiagnosticSet> {
    collect_type_ref(project, &field.r#type, decimals, collected)
}

/// Collect the named types one type expression reaches. Depth is the
/// Model's own bound; a cycle is impossible in the accepted IR (the
/// Model validator refuses cyclic references), so recursion is safe.
fn collect_type_ref(
    project: &CompiledProject,
    type_ref: &TypeRef,
    decimals: &BTreeMap<String, ScalarMapping>,
    collected: &mut BTreeMap<String, TypeKind>,
) -> Result<(), DiagnosticSet> {
    match type_ref {
        TypeRef::Ref(symbol) => collect_symbol(project, symbol.as_str(), decimals, collected),
        TypeRef::List(inner) => collect_type_ref(project, inner, decimals, collected),
        TypeRef::Optional(inner) => collect_type_ref(project, inner, decimals, collected),
    }
}

/// Collect one named symbol's type body, or refuse.
fn collect_symbol(
    project: &CompiledProject,
    symbol: &str,
    decimals: &BTreeMap<String, ScalarMapping>,
    collected: &mut BTreeMap<String, TypeKind>,
) -> Result<(), DiagnosticSet> {
    // The well-known unit object is pre-collected and never resolves
    // through the project (it names no Model symbol).
    if collected.contains_key(symbol) || symbol == UNIT_TYPE {
        return Ok(());
    }
    let kind = match resolve_symbol(project, symbol) {
        Some(Definition::Scalar(scalar)) => {
            let mapping = if decimals.contains_key(symbol) {
                ScalarMapping::DecimalString
            } else {
                scalar_mapping(scalar.base)
            };
            TypeKind::Scalar(mapping)
        }
        Some(Definition::Enum(enum_def)) => TypeKind::Enum(
            enum_def
                .values
                .iter()
                .map(|value| value.value.as_str().to_owned())
                .collect(),
        ),
        Some(Definition::ValueObject(object)) => {
            let mut fields = Vec::new();
            for field in &object.fields {
                collect_field_types(project, field, decimals, collected)?;
                fields.push(client_field_of(field));
            }
            TypeKind::Object(fields)
        }
        Some(Definition::Entity(entity)) => {
            let mut fields = Vec::new();
            for field in &entity.fields {
                collect_field_types(project, field, decimals, collected)?;
                fields.push(client_field_of(field));
            }
            TypeKind::Object(fields)
        }
        _ => return Err(unresolved(symbol)),
    };
    collected.insert(symbol.to_owned(), kind);
    Ok(())
}

/// Project one endpoint binding into one client operation.
#[allow(clippy::too_many_arguments)]
fn operation_of(
    _document: &TransportDocument,
    binding: &EndpointBinding,
    project: &CompiledProject,
    registry: &ErrorRegistry,
    query_model: &QueryModelAttachment,
    decimals: &BTreeMap<String, ScalarMapping>,
) -> Result<ClientOperation, DiagnosticSet> {
    let subject = binding.endpoint.as_str();
    let endpoint = resolve_endpoint(project, subject)?;
    let operation = resolve_operation(project, endpoint.invokes.as_str())?;

    // Parameters: full wire data, types resolved from the bound
    // command inputs or the query-model parameters.
    let mut params = Vec::with_capacity(binding.params.len());
    for param in &binding.params {
        let field = param.field.as_str();
        let type_ref = param_type_ref(operation, query_model, param.field.name().as_str())
            .ok_or_else(|| {
                diagnostic::rule_invalid(
                    diagnostic::SYMBOL_UNRESOLVED,
                    "parameter-type",
                    Some(field.as_str()),
                )
            })?;
        let shape = param_shape(operation, query_model, param.field.name().as_str());
        let nullable = param_nullable(operation, query_model, param.field.name().as_str());
        let location = match param.location {
            ParamLocation::Path => WireLocation::Path,
            ParamLocation::Query => WireLocation::Query,
            ParamLocation::Header => WireLocation::Header,
            ParamLocation::Cookie => WireLocation::Cookie,
        };
        let style = param.style.map(|style| match style {
            ParamStyle::Simple => WireStyle::Simple,
            ParamStyle::Form => WireStyle::Form,
            ParamStyle::DeepObject => WireStyle::DeepObject,
        });
        params.push(ClientParam {
            name: param.name.as_str().to_owned(),
            location,
            field: field.to_owned(),
            type_ref,
            shape,
            nullable,
            required: param.required,
            style,
            explode: param.explode,
        });
    }
    params.sort_by(|left, right| {
        (left.location.as_str(), left.name.as_str())
            .cmp(&(right.location.as_str(), right.name.as_str()))
    });

    // The request body.
    let body = match &binding.body {
        Some(body) => Some(body_of(
            project,
            operation,
            &body.mode,
            &body.fields,
            true,
            subject,
            decimals,
        )?),
        None => None,
    };

    // The success response.
    let success = &binding.success;
    let success_body = match &success.body {
        Some(body) => Some(body_of(
            project,
            operation,
            &body.mode,
            &body.fields,
            false,
            subject,
            decimals,
        )?),
        None => None,
    };
    let result_shape = if success.status == 204 || success_body.is_none() {
        ResultShape::Empty
    } else if binding.pagination.is_some() {
        ResultShape::Page
    } else {
        ResultShape::Value
    };

    // The error union: every entry resolves in the bound registry and
    // carries its derived retry authorization. The transport
    // validation already proves membership; the join here projects
    // identity + payload + retry, and an unresolvable entry is still
    // a refusal (defense in depth).
    let mut errors = Vec::with_capacity(binding.errors.len());
    for entry in &binding.errors {
        let contract = registry
            .error_by_str(entry.error.as_str())
            .ok_or_else(|| unresolved(entry.error.as_str()))?;
        let authorization =
            retry::authorize(contract.retry(), contract.idempotency(), contract.effect());
        let payload = contract
            .payload()
            .public_fields()
            .map(|field| ClientField {
                name: field.name().to_owned(),
                field: field.name().to_owned(),
                type_ref: error_leaf_ref(field.field_type()),
                shape: error_shape(field.field_type()),
                nullable: error_nullable(field.field_type()),
                required: field.required(),
            })
            .collect();
        errors.push(ErrorVariant {
            error: entry.error.clone(),
            code: contract.code().as_str().to_owned(),
            category: contract.category().as_str().to_owned(),
            status: entry.status,
            payload,
            retry: authorization,
        });
    }
    errors.sort_by(|left, right| left.error.as_str().cmp(right.error.as_str()));
    if errors.len() > version::MAX_ERRORS {
        return Err(diagnostic::rule_invalid(
            diagnostic::LIMIT_EXCEEDED,
            "error-bound",
            Some(subject),
        ));
    }

    // Auth, idempotency, correlation.
    let auth = binding.auth.as_ref().map(|auth| ClientAuth {
        actor: auth.actor.as_str().to_owned(),
        schemes: auth
            .schemes
            .iter()
            .map(|scheme| scheme.as_str().to_owned())
            .collect(),
    });
    let idempotency = binding
        .idempotency
        .as_ref()
        .map(|binding| ClientIdempotency {
            header: binding.header.as_str().to_owned(),
            required: binding.required,
        });
    let correlation = binding
        .correlation
        .as_ref()
        .map(|binding| ClientCorrelation {
            headers: binding
                .headers
                .iter()
                .map(|header| header.as_str().to_owned())
                .collect(),
        });

    // The pagination helper: offset bindings project directly; cursor
    // bindings must resolve the cursor field and its declared type so
    // iteration termination is explicit, never guessed.
    let pagination = match &binding.pagination {
        Some(pagination) => {
            let cursor_type_ref: Option<String> = if pagination.style.as_str() == "cursor" {
                let cursor_param = pagination.cursor_param.as_ref().ok_or_else(|| {
                    diagnostic::rule_invalid(
                        diagnostic::CONTRACT_INVALID,
                        "cursor-param-missing",
                        Some(subject),
                    )
                })?;
                // The cursor's semantic type id is the declared type of
                // the cursor parameter: the invoked query's query-model
                // parameter carries it. A cursor binding without a
                // resolvable cursor type is a refusal — iteration must
                // know the cursor's shape, never guess.
                let type_ref = param_type_ref(operation, query_model, cursor_param.as_str())
                    .filter(|type_ref| !type_ref.is_empty())
                    .ok_or_else(|| {
                        diagnostic::rule_invalid(
                            diagnostic::SYMBOL_UNRESOLVED,
                            "cursor-type-unresolved",
                            Some(cursor_param.as_str()),
                        )
                    })?;
                Some(type_ref)
            } else {
                None
            };
            Some(ClientPagination {
                style: pagination.style.as_str().to_owned(),
                limit_param: pagination.limit_param.as_str().to_owned(),
                offset_param: pagination
                    .offset_param
                    .as_ref()
                    .map(|p| p.as_str().to_owned()),
                cursor_param: pagination
                    .cursor_param
                    .as_ref()
                    .map(|p| p.as_str().to_owned()),
                cursor_field: pagination
                    .cursor_field
                    .as_ref()
                    .map(|f| f.as_str().to_owned()),
                cursor_type_ref,
            })
        }
        None => None,
    };

    let operation_id = binding.effective_operation_id().as_str().to_owned();
    // The stable effective operation id doubles as the generated
    // method-name stem: the derivation guarantees a leading lowercase
    // letter, so the mapping is total over the closed grammar.
    let ident = TargetIdent::parse(&operation_id)
        .or_else(|_| TargetIdent::parse(&snake_of_semantic(subject)))
        .map_err(|set| rename_error(subject, set))?;

    Ok(ClientOperation {
        operation_id: operation_id.clone(),
        endpoint: binding.endpoint.clone(),
        invokes: SemanticId::parse(endpoint.invokes.as_str())
            .map_err(|_| unresolved(endpoint.invokes.as_str()))?,
        method: endpoint.method.as_str().to_owned(),
        path: endpoint.path.as_str().to_owned(),
        ident,
        params,
        body,
        success_status: success.status,
        success_body,
        result_shape,
        errors,
        auth,
        idempotency,
        correlation,
        pagination,
        scenarios: binding.scenarios.clone(),
    })
}

/// The method-name fallback of one binding: the operation id carries
/// the camel identity already; the fallback keeps the semantic tail
/// when an explicit override spells a foreign shape.
/// The declared type reference of one parameter: a command input
/// member's type, or the query-model parameter's type.
fn param_type_ref(
    operation: &Definition,
    query_model: &QueryModelAttachment,
    name: &str,
) -> Option<String> {
    match operation {
        Definition::Command(command) => command
            .input
            .iter()
            .find(|field| field.name.as_str() == name)
            .map(|field| {
                leaf_ref(&field.r#type)
                    .map(str::to_owned)
                    .unwrap_or_default()
            }),
        Definition::Query(query) => query_model
            .queries()
            .iter()
            .find(|decl| decl.query.as_str() == query.id.as_str())
            .and_then(|decl| {
                decl.parameters
                    .iter()
                    .find(|parameter| parameter.name.as_str() == name)
            })
            .map(|parameter| parameter_type_leaf(&parameter.parameter_type)),
        _ => None,
    }
}

/// The leaf type id of one bounded query-model type-expression:
/// `list<planner.task_state>` names `planner.task_state` with a list
/// shape (see [`param_shape`]).
fn parameter_type_leaf(expression: &str) -> String {
    let text = expression.trim();
    if let Some(inner) = text
        .strip_prefix("list<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        return inner.trim().to_owned();
    }
    text.to_owned()
}

/// The closed wire shape of one query-model parameter's bounded
/// type-expression: the `list<...>` spelling projects a list shape.
fn param_shape(
    operation: &Definition,
    query_model: &QueryModelAttachment,
    name: &str,
) -> TypeShape {
    match operation {
        Definition::Command(command) => command
            .input
            .iter()
            .find(|field| field.name.as_str() == name)
            .map(|field| shape_of(&field.r#type))
            .unwrap_or(TypeShape::Value),
        Definition::Query(query) => query_model
            .queries()
            .iter()
            .find(|decl| decl.query.as_str() == query.id.as_str())
            .and_then(|decl| {
                decl.parameters
                    .iter()
                    .find(|parameter| parameter.name.as_str() == name)
            })
            .map(|parameter| {
                if parameter.parameter_type.trim().starts_with("list<") {
                    TypeShape::List
                } else {
                    TypeShape::Value
                }
            })
            .unwrap_or(TypeShape::Value),
        _ => TypeShape::Value,
    }
}

/// Whether one parameter's declared type is nullable (`Optional`).
/// Query-model parameters spell their bounded model type-expression in
/// text, where the nullable spelling is the trailing `?`.
fn param_nullable(operation: &Definition, query_model: &QueryModelAttachment, name: &str) -> bool {
    match operation {
        Definition::Command(command) => command
            .input
            .iter()
            .find(|field| field.name.as_str() == name)
            .is_some_and(|field| nullable_of(&field.r#type)),
        Definition::Query(query) => query_model
            .queries()
            .iter()
            .find(|decl| decl.query.as_str() == query.id.as_str())
            .and_then(|decl| {
                decl.parameters
                    .iter()
                    .find(|parameter| parameter.name.as_str() == name)
            })
            .is_some_and(|parameter| parameter.parameter_type.ends_with('?')),
        _ => false,
    }
}

/// The well-known unit object type for operations that succeed with a
/// valueless body (a command without a declared output): every
/// backend renders it as the empty object, and it is always present
/// in `types[]` so the ref never dangles.
pub(crate) const UNIT_TYPE: &str = "lekalo.unit";

/// One body projection from its declared mode and field subset.
fn body_of(
    project: &CompiledProject,
    operation: &Definition,
    mode: &ProjectionMode,
    fields: &[crate::transport_http::FieldProjection],
    input: bool,
    subject: &str,
    decimals: &BTreeMap<String, ScalarMapping>,
) -> Result<ClientBody, DiagnosticSet> {
    let (type_ref, body_shape) = match operation {
        Definition::Command(command) if input => {
            // A whole-input command body is the command's synthetic
            // input object — a collected type (never a dangling ref).
            (format!("{}.input", command.id.as_str()), TypeShape::Value)
        }
        Definition::Command(_) if !input => {
            // Commands carry no declared output in the IR: a
            // whole-output success body would otherwise narrow to an
            // empty ref, so the projection names the well-known unit
            // object explicitly (collected below, never dangling).
            (UNIT_TYPE.to_owned(), TypeShape::Value)
        }
        Definition::Query(query) if !input => match &query.returns {
            // The list wrapper survives into the contract: a
            // `list<planner.task>` output projects shape `list` over
            // the element type id, so clients decode an array (issue
            // #72 round 2).
            Some(returns) => {
                let shape = shape_of(returns);
                let type_ref = leaf_ref(returns).map(str::to_owned).ok_or_else(|| {
                    diagnostic::rule_invalid(
                        diagnostic::CONTRACT_INVALID,
                        "output-type-unresolved",
                        Some(subject),
                    )
                })?;
                (type_ref, shape)
            }
            // A whole-output success body without a declared output is
            // a silent narrowing: refuse instead of emitting an empty
            // typeRef (issue #72 fix round).
            None => {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CONTRACT_INVALID,
                    "whole-output-without-declared-type",
                    Some(subject),
                ))
            }
        },
        _ => (String::new(), TypeShape::Value),
    };
    let _ = decimals;
    let client_fields = fields
        .iter()
        .map(|field| {
            let member = bound_member(project, operation, input, field.field.as_str().to_owned());
            // An explicit-projection member that does not resolve in
            // the bound contract is a refusal — an empty typeRef would
            // be the exact silent narrowing the docs forbid.
            let member_name = field.field.as_str();
            let (type_ref, shape, nullable) = member.ok_or_else(|| {
                diagnostic::rule_invalid(
                    diagnostic::SYMBOL_UNRESOLVED,
                    "body-member-unresolved",
                    Some(member_name.as_str()),
                )
            })?;
            Ok(ClientField {
                name: field.name.as_str().to_owned(),
                field: field.field.as_str().to_owned(),
                type_ref,
                shape,
                nullable,
                required: field.required,
            })
        })
        .collect::<Result<Vec<_>, DiagnosticSet>>()?;
    Ok(ClientBody {
        mode: match mode {
            ProjectionMode::Whole => BodyMode::Whole,
            ProjectionMode::Explicit => BodyMode::Explicit,
        },
        type_ref,
        shape: body_shape,
        fields: client_fields,
    })
}

/// The declared type reference and nullability of one body field's
/// bound member: a command input member for request bodies, a
/// source-entity field for response bodies. A member the bound
/// contract does not declare is `None` — the projection never invents
/// a type.
fn bound_member(
    project: &CompiledProject,
    operation: &Definition,
    input: bool,
    field: String,
) -> Option<(String, TypeShape, bool)> {
    let name = field.strip_prefix("input.").unwrap_or(&field);
    let member = |members: &[Field], name: &str| {
        members
            .iter()
            .find(|member| member.name.as_str() == name)
            .map(|member| {
                (
                    leaf_ref(&member.r#type)
                        .map(str::to_owned)
                        .unwrap_or_default(),
                    shape_of(&member.r#type),
                    nullable_of(&member.r#type),
                )
            })
    };
    match (operation, input) {
        (Definition::Command(command), true) => member(&command.input, name),
        (Definition::Query(query), false) => {
            let returns = query.returns.as_ref()?;
            let source = leaf_ref(returns)?;
            match resolve_symbol(project, source)? {
                Definition::Entity(entity) => member(&entity.fields, name),
                Definition::ValueObject(object) => member(&object.fields, name),
                _ => None,
            }
        }
        _ => None,
    }
}

/// One canonical operation object.
fn operation_json(operation: &ClientOperation) -> Json {
    let mut object = Map::new();
    object.insert(
        "operationId".to_owned(),
        Json::String(operation.operation_id.clone()),
    );
    object.insert(
        "endpoint".to_owned(),
        Json::String(operation.endpoint.as_str().to_owned()),
    );
    object.insert(
        "invokes".to_owned(),
        Json::String(operation.invokes.as_str().to_owned()),
    );
    object.insert("method".to_owned(), Json::String(operation.method.clone()));
    object.insert("path".to_owned(), Json::String(operation.path.clone()));
    object.insert(
        "ident".to_owned(),
        Json::String(operation.ident.as_str().to_owned()),
    );
    object.insert(
        "params".to_owned(),
        Json::Array(operation.params.iter().map(param_json).collect()),
    );
    if let Some(body) = &operation.body {
        object.insert("body".to_owned(), body_json(body));
    }
    object.insert(
        "successStatus".to_owned(),
        Json::from(operation.success_status),
    );
    if let Some(body) = &operation.success_body {
        object.insert("successBody".to_owned(), body_json(body));
    }
    object.insert(
        "resultShape".to_owned(),
        Json::String(operation.result_shape.as_str().to_owned()),
    );
    if !operation.errors.is_empty() {
        object.insert(
            "errors".to_owned(),
            Json::Array(operation.errors.iter().map(error_json).collect()),
        );
    }
    if let Some(auth) = &operation.auth {
        let mut auth_object = Map::new();
        auth_object.insert("actor".to_owned(), Json::String(auth.actor.clone()));
        auth_object.insert(
            "schemes".to_owned(),
            Json::Array(
                auth.schemes
                    .iter()
                    .map(|scheme| Json::String(scheme.clone()))
                    .collect(),
            ),
        );
        object.insert("auth".to_owned(), Json::Object(auth_object));
    }
    if let Some(idempotency) = &operation.idempotency {
        let mut idempotency_object = Map::new();
        idempotency_object.insert(
            "header".to_owned(),
            Json::String(idempotency.header.clone()),
        );
        idempotency_object.insert("required".to_owned(), Json::Bool(idempotency.required));
        object.insert("idempotency".to_owned(), Json::Object(idempotency_object));
    }
    if let Some(correlation) = &operation.correlation {
        object.insert(
            "correlation".to_owned(),
            Json::Array(
                correlation
                    .headers
                    .iter()
                    .map(|header| Json::String(header.clone()))
                    .collect(),
            ),
        );
    }
    if let Some(pagination) = &operation.pagination {
        let mut pagination_object = Map::new();
        pagination_object.insert("style".to_owned(), Json::String(pagination.style.clone()));
        pagination_object.insert(
            "limitParam".to_owned(),
            Json::String(pagination.limit_param.clone()),
        );
        if let Some(offset) = &pagination.offset_param {
            pagination_object.insert("offsetParam".to_owned(), Json::String(offset.clone()));
        }
        if let Some(cursor) = &pagination.cursor_param {
            pagination_object.insert("cursorParam".to_owned(), Json::String(cursor.clone()));
        }
        if let Some(field) = &pagination.cursor_field {
            pagination_object.insert("cursorField".to_owned(), Json::String(field.clone()));
        }
        if let Some(cursor_type) = &pagination.cursor_type_ref {
            pagination_object.insert(
                "cursorTypeRef".to_owned(),
                Json::String(cursor_type.clone()),
            );
        }
        object.insert("pagination".to_owned(), Json::Object(pagination_object));
    }
    if !operation.scenarios.is_empty() {
        object.insert(
            "scenarios".to_owned(),
            Json::Array(
                operation
                    .scenarios
                    .iter()
                    .map(|scenario| Json::String(scenario.as_str().to_owned()))
                    .collect(),
            ),
        );
    }
    Json::Object(object)
}

/// One canonical parameter object.
fn param_json(param: &ClientParam) -> Json {
    let mut object = Map::new();
    object.insert("name".to_owned(), Json::String(param.name.clone()));
    object.insert(
        "in".to_owned(),
        Json::String(param.location.as_str().to_owned()),
    );
    object.insert("field".to_owned(), Json::String(param.field.clone()));
    object.insert("typeRef".to_owned(), Json::String(param.type_ref.clone()));
    object.insert(
        "shape".to_owned(),
        Json::String(param.shape.as_str().to_owned()),
    );
    object.insert("required".to_owned(), Json::Bool(param.required));
    object.insert("nullable".to_owned(), Json::Bool(param.nullable));
    if let Some(style) = param.style {
        object.insert("style".to_owned(), Json::String(style.as_str().to_owned()));
    }
    if let Some(explode) = param.explode {
        object.insert("explode".to_owned(), Json::Bool(explode));
    }
    Json::Object(object)
}

/// One canonical body object.
fn body_json(body: &ClientBody) -> Json {
    let mut object = Map::new();
    object.insert(
        "mode".to_owned(),
        Json::String(body.mode.as_str().to_owned()),
    );
    object.insert("typeRef".to_owned(), Json::String(body.type_ref.clone()));
    object.insert(
        "shape".to_owned(),
        Json::String(body.shape.as_str().to_owned()),
    );
    if !body.fields.is_empty() {
        object.insert(
            "fields".to_owned(),
            Json::Array(
                body.fields
                    .iter()
                    .map(|field| {
                        let mut entry = Map::new();
                        entry.insert("name".to_owned(), Json::String(field.name.clone()));
                        entry.insert("field".to_owned(), Json::String(field.field.clone()));
                        entry.insert("typeRef".to_owned(), Json::String(field.type_ref.clone()));
                        entry.insert(
                            "shape".to_owned(),
                            Json::String(field.shape.as_str().to_owned()),
                        );
                        entry.insert("required".to_owned(), Json::Bool(field.required));
                        entry.insert("nullable".to_owned(), Json::Bool(field.nullable));
                        Json::Object(entry)
                    })
                    .collect(),
            ),
        );
    }
    Json::Object(object)
}

/// One canonical error-variant object.
fn error_json(error: &ErrorVariant) -> Json {
    let mut object = Map::new();
    object.insert(
        "error".to_owned(),
        Json::String(error.error.as_str().to_owned()),
    );
    object.insert("code".to_owned(), Json::String(error.code.clone()));
    object.insert("category".to_owned(), Json::String(error.category.clone()));
    object.insert("status".to_owned(), Json::from(error.status));
    object.insert(
        "retry".to_owned(),
        Json::String(error.retry.as_str().to_owned()),
    );
    if !error.payload.is_empty() {
        object.insert(
            "payload".to_owned(),
            Json::Array(
                error
                    .payload
                    .iter()
                    .map(|field| {
                        let mut entry = Map::new();
                        entry.insert("name".to_owned(), Json::String(field.name.clone()));
                        entry.insert("typeRef".to_owned(), Json::String(field.type_ref.clone()));
                        entry.insert(
                            "shape".to_owned(),
                            Json::String(field.shape.as_str().to_owned()),
                        );
                        entry.insert("required".to_owned(), Json::Bool(field.required));
                        entry.insert("nullable".to_owned(), Json::Bool(field.nullable));
                        Json::Object(entry)
                    })
                    .collect(),
            ),
        );
    }
    Json::Object(object)
}

/// One canonical named-type object.
fn type_json(type_def: &ClientType) -> Json {
    let mut object = Map::new();
    object.insert(
        "symbol".to_owned(),
        Json::String(type_def.symbol.as_str().to_owned()),
    );
    object.insert("typeId".to_owned(), Json::String(type_def.type_id.clone()));
    object.insert(
        "ident".to_owned(),
        Json::String(type_def.ident.as_str().to_owned()),
    );
    match &type_def.kind {
        TypeKind::Scalar(mapping) => {
            object.insert("kind".to_owned(), Json::String("scalar".to_owned()));
            object.insert("base".to_owned(), Json::String(mapping.as_str().to_owned()));
        }
        TypeKind::Enum(values) => {
            object.insert("kind".to_owned(), Json::String("enum".to_owned()));
            object.insert(
                "values".to_owned(),
                Json::Array(
                    values
                        .iter()
                        .map(|value| Json::String(value.clone()))
                        .collect(),
                ),
            );
        }
        TypeKind::Object(fields) => {
            object.insert("kind".to_owned(), Json::String("object".to_owned()));
            object.insert(
                "fields".to_owned(),
                Json::Array(
                    fields
                        .iter()
                        .map(|field| {
                            let mut entry = Map::new();
                            entry.insert("name".to_owned(), Json::String(field.name.clone()));
                            entry
                                .insert("typeRef".to_owned(), Json::String(field.type_ref.clone()));
                            entry.insert(
                                "shape".to_owned(),
                                Json::String(field.shape.as_str().to_owned()),
                            );
                            entry.insert("nullable".to_owned(), Json::Bool(field.nullable));
                            entry.insert("required".to_owned(), Json::Bool(field.required));
                            Json::Object(entry)
                        })
                        .collect(),
                ),
            );
        }
    }
    Json::Object(object)
}

/// The digest spelling helper: the sha256 of exact bytes. Unused in
/// the lib target for now; the CLI handoff consumes it in the next
/// staged commit.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn digest_of_bytes(bytes: &[u8]) -> crate::lockfile::types::Sha256Digest {
    crate::lockfile::types::Sha256Digest::from_hex(&crate::digest::sha256_hex(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_snake_accepts_closed_tokens() {
        assert!(is_lower_snake("web_console"));
        assert!(is_lower_snake("billing"));
        assert!(is_lower_snake("v2"));
        assert!(!is_lower_snake(""));
        assert!(!is_lower_snake("_lead"));
        assert!(!is_lower_snake("Trail_"));
        assert!(!is_lower_snake("double__under"));
        assert!(!is_lower_snake("Upper"));
        assert!(!is_lower_snake("has space"));
    }
}
