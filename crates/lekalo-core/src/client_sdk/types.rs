//! The closed typed surface of the client-SDK projection (issue #72).
//!
//! Every value here is language-neutral declaration data: one
//! operation, one named type, one error variant, one retry policy.
//! Language backends render these values; they never reinterpret
//! them. Nullability is explicit (`optional`), presence is explicit
//! (`required`), decimals are string-backed by policy, and dates are
//! validated ISO strings — never a language runtime type.

use crate::scenario::id::SemanticId;

use super::id::TargetIdent;
use super::retry::RetryAuthorization;

/// The closed wire location of one client operation parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ParamLocation {
    /// A path-template segment.
    Path,
    /// A query parameter.
    Query,
    /// A request header.
    Header,
    /// A cookie.
    Cookie,
}

impl ParamLocation {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Query => "query",
            Self::Header => "header",
            Self::Cookie => "cookie",
        }
    }
}

/// The closed serialization style of one parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ParamStyle {
    /// Comma-delimited values (the RFC 6570 simple style).
    Simple,
    /// Ampersand-delimited explode-friendly values.
    Form,
    /// Deep-object nesting.
    DeepObject,
}

impl ParamStyle {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Form => "form",
            Self::DeepObject => "deepObject",
        }
    }
}

/// One client operation parameter: the full wire binding the runtime
/// must honor, including the style/explode pair the route JSON omits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientParam {
    /// The wire parameter name.
    pub name: String,
    /// Where the parameter arrives.
    pub location: ParamLocation,
    /// The bound semantic field (`input.task_id` or a query-model
    /// parameter name).
    pub field: String,
    /// The semantic id of the field's declared type.
    pub type_ref: String,
    /// Whether the field is nullable (`Optional(T)` in the Model).
    pub nullable: bool,
    /// Whether the parameter must be present.
    pub required: bool,
    /// The closed serialization style.
    pub style: Option<ParamStyle>,
    /// Whether composite values explode.
    pub explode: Option<bool>,
}

/// One request-body/response field projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientField {
    /// The wire field name.
    pub name: String,
    /// The bound semantic field reference.
    pub field: String,
    /// The semantic id of the field's declared type.
    pub type_ref: String,
    /// Whether the field is nullable.
    pub nullable: bool,
    /// Whether the field must be present.
    pub required: bool,
}

/// The closed body projection mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum BodyMode {
    /// The JSON body is exactly the declared input/output value.
    Whole,
    /// A declared subset of fields.
    Explicit,
}

impl BodyMode {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Explicit => "explicit",
        }
    }
}

/// One declared JSON body projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientBody {
    /// The closed mode.
    pub mode: BodyMode,
    /// The type id of the bound input/output symbol.
    pub type_ref: String,
    /// The declared field subset (`explicit` mode only).
    pub fields: Vec<ClientField>,
}

/// One declared security-scheme reference the client honors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientAuth {
    /// The closed actor token (`identity.user`).
    pub actor: String,
    /// The required scheme ids.
    pub schemes: Vec<String>,
}

/// One declared idempotency binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientIdempotency {
    /// The idempotency header name.
    pub header: String,
    /// Whether the key is required.
    pub required: bool,
}

/// One declared correlation-header binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientCorrelation {
    /// The declared header names.
    pub headers: Vec<String>,
}

/// One declared pagination binding with its bounded iteration
/// contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientPagination {
    /// The closed style token (`offset` or `cursor`).
    pub style: String,
    /// The wire limit parameter name.
    pub limit_param: String,
    /// The wire offset parameter name (`offset` style).
    pub offset_param: Option<String>,
    /// The wire cursor parameter name (`cursor` style).
    pub cursor_param: Option<String>,
    /// The response field carrying the next cursor (`cursor` style).
    pub cursor_field: Option<String>,
    /// The semantic id of the cursor's declared type (`cursor` style).
    pub cursor_type_ref: Option<String>,
}

/// One declared error variant of one operation, with its exact retry
/// authorization derived from the #62 contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorVariant {
    /// The declared error's semantic id.
    pub error: SemanticId,
    /// The immutable machine code (`LEK-ERR-001`).
    pub code: String,
    /// The closed category token.
    pub category: String,
    /// The projected HTTP status.
    pub status: u16,
    /// The public payload fields (name, type id, required). Private
    /// fields never cross the boundary.
    pub payload: Vec<ClientField>,
    /// The retry authorization this specific error grants.
    pub retry: RetryAuthorization,
}

/// The declared pagination helper shape, when the endpoint declares
/// pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultShape {
    /// One decoded value (the declared output type).
    Value,
    /// No body (204-style success).
    Empty,
    /// A pageable result with bounded iteration.
    Page,
}

impl ResultShape {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Empty => "empty",
            Self::Page => "page",
        }
    }
}

/// One client operation: the full typed callable the backends render.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientOperation {
    /// The stable effective operation id (method-name binding).
    pub operation_id: String,
    /// The Model endpoint symbol this operation projects.
    pub endpoint: SemanticId,
    /// The invoked Model command/query symbol.
    pub invokes: SemanticId,
    /// The HTTP method (from the Model endpoint, never restated by
    /// the client).
    pub method: String,
    /// The path template (from the Model endpoint).
    pub path: String,
    /// The generated identifier of the operation method.
    pub ident: TargetIdent,
    /// The declared parameters in canonical order.
    pub params: Vec<ClientParam>,
    /// The request body binding, when declared.
    pub body: Option<ClientBody>,
    /// The success binding: status, optional body, headers.
    pub success_status: u16,
    /// The success body binding, when declared.
    pub success_body: Option<ClientBody>,
    /// The declared result shape (value/empty/page).
    pub result_shape: ResultShape,
    /// The declared error variants in error-id order.
    pub errors: Vec<ErrorVariant>,
    /// The declared auth binding, when the endpoint is not public.
    pub auth: Option<ClientAuth>,
    /// The declared idempotency binding.
    pub idempotency: Option<ClientIdempotency>,
    /// The declared correlation headers.
    pub correlation: Option<ClientCorrelation>,
    /// The declared pagination helper, when projected.
    pub pagination: Option<ClientPagination>,
    /// The black-box scenario coverage references.
    pub scenarios: Vec<SemanticId>,
}

/// The closed language-neutral type expression of the projection.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TypeExpr {
    /// A named reference to a projected type.
    Ref(String),
    /// A list of the inner expression.
    List(Box<TypeExpr>),
    /// An explicitly nullable wrapper.
    Optional(Box<TypeExpr>),
}

/// The closed scalar base mapping of one named type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ScalarMapping {
    /// UTF-8 text.
    String,
    /// A JSON number.
    Number,
    /// True/false.
    Boolean,
    /// An ISO `date` string — never a runtime date type.
    Date,
    /// An ISO `date-time` string — never a runtime date type.
    Datetime,
    /// A UUID string.
    Uuid,
    /// A URI string.
    Uri,
    /// A decimal carried as its exact string spelling. Decimals are
    /// only produced for string-backed declared scalars with the
    /// agreed decimal format; a numeric wire value is never coerced.
    DecimalString,
}

impl ScalarMapping {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Date => "date",
            Self::Datetime => "datetime",
            Self::Uuid => "uuid",
            Self::Uri => "uri",
            Self::DecimalString => "decimal-string",
        }
    }
}

/// The closed kind of one named projected type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeKind {
    /// A scalar alias with its explicit mapping.
    Scalar(ScalarMapping),
    /// A closed string enum over its exact values.
    Enum(Vec<String>),
    /// A structural object over its declared fields.
    Object(Vec<ClientField>),
}

/// One named projected type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientType {
    /// The semantic id of the source symbol.
    pub symbol: SemanticId,
    /// The language-neutral type id the backends name from.
    pub type_id: String,
    /// The generated identifier of the type.
    pub ident: TargetIdent,
    /// The closed kind.
    pub kind: TypeKind,
}

impl ClientType {
    /// The type id of one named symbol (`planner.task` ->
    /// `planner.task`); the projection keeps the full semantic id so
    /// cross-module references stay unambiguous.
    pub fn type_id_of(symbol: &str) -> String {
        symbol.to_owned()
    }
}

/// The closed configuration of one projection run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientConfig {
    /// The generation mode: `generated` (reproducible emission) or
    /// `checked` (read-only maintained-client validation).
    pub mode: String,
    /// The required decimal string mapping of one declared scalar.
    /// Every other scalar keeps its default mapping; the projection
    /// refuses an unknown scalar id.
    pub decimal_scalars: Vec<String>,
    /// The declared consumer bindings this projection serves; an
    /// empty list means the projection serves no registered consumer
    /// (generation-only runs).
    pub consumers: Vec<super::id::ConsumerId>,
}

impl ClientConfig {
    /// The default generated-mode configuration.
    pub fn generated() -> Self {
        Self {
            mode: super::version::MODE_GENERATED.to_owned(),
            decimal_scalars: Vec::new(),
            consumers: Vec::new(),
        }
    }

    /// Whether this configuration selects the read-only checked mode.
    pub fn is_checked(&self) -> bool {
        self.mode == super::version::MODE_CHECKED
    }
}

/// A typed validation failure of one client configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientTypeError {
    /// The mode token is not one of the closed vocabulary.
    UnknownMode,
    /// The decimal list crosses its bound.
    DecimalOverbound,
    /// The consumer list crosses its bound.
    ConsumersOverbound,
}

impl ClientConfig {
    /// Validate one configuration against the closed bounds.
    pub fn validate(&self) -> Result<(), ClientTypeError> {
        if self.mode != super::version::MODE_GENERATED && self.mode != super::version::MODE_CHECKED
        {
            return Err(ClientTypeError::UnknownMode);
        }
        if self.decimal_scalars.len() > super::version::MAX_TYPES {
            return Err(ClientTypeError::DecimalOverbound);
        }
        if self.consumers.len() > super::version::MAX_CONSUMERS {
            return Err(ClientTypeError::ConsumersOverbound);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_shapes_are_closed() {
        assert_eq!(ResultShape::Value.as_str(), "value");
        assert_eq!(ResultShape::Empty.as_str(), "empty");
        assert_eq!(ResultShape::Page.as_str(), "page");
    }

    #[test]
    fn decimal_mapping_is_explicit() {
        assert_eq!(ScalarMapping::DecimalString.as_str(), "decimal-string");
        assert_eq!(ScalarMapping::Date.as_str(), "date");
    }

    #[test]
    fn config_validates_mode_and_bounds() {
        let mut config = ClientConfig::generated();
        assert_eq!(config.validate(), Ok(()));
        config.mode = "yolo".to_owned();
        assert_eq!(config.validate(), Err(ClientTypeError::UnknownMode));
        config.mode = super::super::version::MODE_CHECKED.to_owned();
        assert_eq!(config.validate(), Ok(()));
        assert!(config.is_checked());
        config.decimal_scalars = vec!["x".to_owned(); super::super::version::MAX_TYPES + 1];
        assert_eq!(config.validate(), Err(ClientTypeError::DecimalOverbound));
    }
}
