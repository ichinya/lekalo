//! The closed declaration surface of the transport-http attachment
//! (issue #70).
//!
//! One endpoint binding attaches one Model `endpoint` symbol to its
//! full HTTP/JSON wire surface: parameter bindings, the JSON
//! request/response schemas, the typed error-to-status projection,
//! the explicit security scheme projection, idempotency and
//! correlation headers, pagination projection, rate-limit and cache
//! metadata, content versioning, declared streaming/upload/download
//! capabilities, and black-box scenario coverage. Every surface is
//! closed and bounded; nothing here can express runtime principals,
//! secrets, middleware names, or raw target code.

use crate::scenario::id::SemanticId;

use super::id::{FieldRef, HeaderName, OperationId, SafeToken, WireName};

/// The closed security-scheme kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SchemeKind {
    /// No credential (only valid with the `public` actor).
    None,
    /// A bearer token.
    Bearer,
    /// An API key in a declared location.
    ApiKey,
    /// Basic authentication.
    Basic,
    /// OAuth2 with declared flow ids.
    Oauth2,
    /// Client-certificate mutual TLS.
    MutualTls,
    /// A custom scheme identified by capability token.
    Custom,
}

impl SchemeKind {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bearer => "bearer",
            Self::ApiKey => "api-key",
            Self::Basic => "basic",
            Self::Oauth2 => "oauth2",
            Self::MutualTls => "mutual-tls",
            Self::Custom => "custom",
        }
    }

    /// The kind for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "none" => Self::None,
            "bearer" => Self::Bearer,
            "api-key" => Self::ApiKey,
            "basic" => Self::Basic,
            "oauth2" => Self::Oauth2,
            "mutual-tls" => Self::MutualTls,
            "custom" => Self::Custom,
            _ => return None,
        })
    }
}

/// The closed api-key location vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ApiKeyLocation {
    /// The `Authorization`-style header slot.
    Header,
    /// A query parameter slot.
    Query,
    /// A cookie slot.
    Cookie,
}

impl ApiKeyLocation {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::Query => "query",
            Self::Cookie => "cookie",
        }
    }

    /// The location for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "header" => Self::Header,
            "query" => Self::Query,
            "cookie" => Self::Cookie,
            _ => return None,
        })
    }
}

/// One declared security scheme: explicit, closed, secret-free.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityScheme {
    /// The scheme id endpoints reference.
    pub id: WireName,
    /// The closed scheme kind.
    pub kind: SchemeKind,
    /// The bearer format (`bearer` only).
    pub format: Option<SafeToken>,
    /// The api-key location (`api-key` only).
    pub location: Option<ApiKeyLocation>,
    /// The api-key field name (`api-key` header slot only).
    pub name: Option<HeaderName>,
    /// The declared oauth2 flow ids (`oauth2` only).
    pub flow_ids: Vec<SafeToken>,
    /// The custom capability token (`custom` only).
    pub capability_token: Option<SafeToken>,
}

/// The closed parameter-location vocabulary.
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

    /// The location for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "path" => Self::Path,
            "query" => Self::Query,
            "header" => Self::Header,
            "cookie" => Self::Cookie,
            _ => return None,
        })
    }
}

/// The closed parameter serialization-style vocabulary.
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

    /// The style for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "simple" => Self::Simple,
            "form" => Self::Form,
            "deepObject" => Self::DeepObject,
            _ => return None,
        })
    }
}

/// One declared parameter binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParamBinding {
    /// The wire parameter name.
    pub name: WireName,
    /// Where the parameter arrives.
    pub location: ParamLocation,
    /// The declared operation input member or query-model parameter.
    pub field: FieldRef,
    /// Whether the parameter must be present.
    pub required: bool,
    /// The closed serialization style.
    pub style: Option<ParamStyle>,
    /// Whether composite values explode.
    pub explode: Option<bool>,
}

impl ParamBinding {
    /// Whether this is a path-template binding.
    pub const fn is_path(&self) -> bool {
        matches!(self.location, ParamLocation::Path)
    }
}

/// The closed body/response projection mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProjectionMode {
    /// The JSON body is exactly the declared input/output value.
    Whole,
    /// A declared subset of fields.
    Explicit,
}

impl ProjectionMode {
    /// The exact wire token of the request-body mode.
    pub const fn body_str(self) -> &'static str {
        match self {
            Self::Whole => "whole-input",
            Self::Explicit => "explicit",
        }
    }

    /// The exact wire token of the response-body mode.
    pub const fn response_str(self) -> &'static str {
        match self {
            Self::Whole => "whole-output",
            Self::Explicit => "explicit",
        }
    }

    /// The request-body mode for one wire token, or nothing.
    pub fn parse_body(text: &str) -> Option<Self> {
        Some(match text {
            "whole-input" => Self::Whole,
            "explicit" => Self::Explicit,
            _ => return None,
        })
    }

    /// The response-body mode for one wire token, or nothing.
    pub fn parse_response(text: &str) -> Option<Self> {
        Some(match text {
            "whole-output" => Self::Whole,
            "explicit" => Self::Explicit,
            _ => return None,
        })
    }
}

/// One declared body/response field projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldProjection {
    /// The wire field name.
    pub name: WireName,
    /// The bound input member or output member.
    pub field: FieldRef,
    /// Whether the field must be present.
    pub required: bool,
}

/// One declared JSON request body binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyBinding {
    /// The closed mode.
    pub mode: ProjectionMode,
    /// The declared field subset (`explicit` mode only).
    pub fields: Vec<FieldProjection>,
}

/// One declared response header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseHeader {
    /// The header name.
    pub name: HeaderName,
    /// Whether the header must be present.
    pub required: bool,
}

/// One declared success response binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuccessBinding {
    /// The success status (200/201/202/204).
    pub status: u16,
    /// The optional response body projection.
    pub body: Option<BodyBinding>,
    /// The declared response headers.
    pub headers: Vec<ResponseHeader>,
}

/// One declared error → status mapping entry. Keys on the immutable
/// #62 error identity; the wire body is always the canonical
/// quadruple with public payload fields only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorEntry {
    /// The declared error's semantic id.
    pub error: SemanticId,
    /// The projected status.
    pub status: u16,
}

/// The fixed category-default statuses of the error projection. One
/// status per closed #62 category keeps validation, auth, domain,
/// and infrastructure failures distinguishable on the wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErrorDefaults {
    pub validation: u16,
    pub auth: u16,
    pub conflict: u16,
    pub not_found: u16,
    pub domain: u16,
    pub infrastructure: u16,
}

impl ErrorDefaults {
    /// The default status of one closed category token, or nothing.
    pub fn status_of(&self, category: &str) -> Option<u16> {
        Some(match category {
            "validation" => self.validation,
            "auth" => self.auth,
            "conflict" => self.conflict,
            "not-found" => self.not_found,
            "domain" => self.domain,
            "infrastructure" => self.infrastructure,
            _ => return None,
        })
    }
}

/// The closed #25 actor vocabulary the security section projects.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Actor {
    /// No authentication; only explicitly public surfaces.
    Public,
    /// An authenticated end user.
    IdentityUser,
    /// An authenticated workload identity.
    IdentityService,
    /// A scheduler identity plus a named job.
    SystemJob,
    /// A trusted runtime identity at a declared boundary.
    Internal,
}

impl Actor {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::IdentityUser => "identity.user",
            Self::IdentityService => "identity.service",
            Self::SystemJob => "system.job",
            Self::Internal => "internal",
        }
    }

    /// The actor for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "public" => Self::Public,
            "identity.user" => Self::IdentityUser,
            "identity.service" => Self::IdentityService,
            "system.job" => Self::SystemJob,
            "internal" => Self::Internal,
            _ => return None,
        })
    }

    /// Whether the actor requires at least one scheme.
    pub const fn requires_scheme(self) -> bool {
        !matches!(self, Self::Public)
    }
}

/// One declared per-endpoint security projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthBinding {
    /// The closed actor.
    pub actor: Actor,
    /// The required scheme ids (declared in the attachment).
    pub schemes: Vec<WireName>,
    /// The optional policy reference (a Model policy symbol).
    pub policy_ref: Option<SemanticId>,
}

/// One declared idempotency-key binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyBinding {
    /// The idempotency header name.
    pub header: HeaderName,
    /// Whether the key is required.
    pub required: bool,
}

/// One declared correlation-header binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorrelationBinding {
    /// The declared header names.
    pub headers: Vec<HeaderName>,
}

/// The closed pagination-style vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PaginationStyle {
    /// limit/offset pagination.
    Offset,
    /// Cursor pagination.
    Cursor,
}

impl PaginationStyle {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Offset => "offset",
            Self::Cursor => "cursor",
        }
    }

    /// The style for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "offset" => Self::Offset,
            "cursor" => Self::Cursor,
            _ => return None,
        })
    }
}

/// One declared pagination projection of the #64 contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginationBinding {
    /// The closed style.
    pub style: PaginationStyle,
    /// The wire limit parameter name.
    pub limit_param: WireName,
    /// The wire offset parameter name (`offset` style).
    pub offset_param: Option<WireName>,
    /// The wire cursor parameter name (`cursor` style).
    pub cursor_param: Option<WireName>,
    /// The response field carrying the next cursor.
    pub cursor_field: Option<WireName>,
}

/// The closed rate-limit scope vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RateLimitScope {
    /// Per authenticated actor.
    Actor,
    /// Per tenant.
    Tenant,
    /// Per endpoint.
    Endpoint,
}

impl RateLimitScope {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Actor => "actor",
            Self::Tenant => "tenant",
            Self::Endpoint => "endpoint",
        }
    }

    /// The scope for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "actor" => Self::Actor,
            "tenant" => Self::Tenant,
            "endpoint" => Self::Endpoint,
            _ => return None,
        })
    }
}

/// One declared rate limit (declaration only; header rendering
/// belongs to the runtime).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitBinding {
    /// The request count bound.
    pub limit: u64,
    /// The window in seconds.
    pub window_seconds: u64,
    /// The closed scope.
    pub scope: RateLimitScope,
}

/// The closed cache policy vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CachePolicy {
    /// Never stored.
    NoStore,
    /// Private (single-actor) caching.
    Private,
    /// Shared caching.
    Public,
}

impl CachePolicy {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoStore => "no-store",
            Self::Private => "private",
            Self::Public => "public",
        }
    }

    /// The policy for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "no-store" => Self::NoStore,
            "private" => Self::Private,
            "public" => Self::Public,
            _ => return None,
        })
    }
}

/// One declared cache policy (declaration only).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheBinding {
    /// The closed policy.
    pub policy: CachePolicy,
    /// The max-age in seconds.
    pub max_age_seconds: u64,
    /// Whether ETag validation is declared.
    pub etag: bool,
}

/// The closed api-version location vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ApiVersionLocation {
    /// A path segment.
    Path,
    /// A request header.
    Header,
}

impl ApiVersionLocation {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Header => "header",
        }
    }

    /// The location for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "path" => Self::Path,
            "header" => Self::Header,
            _ => return None,
        })
    }
}

/// One declared content-version carrier (declaration only).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiVersionBinding {
    /// The closed location.
    pub location: ApiVersionLocation,
    /// The version name (`v1`).
    pub name: SafeToken,
}

/// The closed capability-kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CapabilityKind {
    /// Streaming responses.
    Streaming,
    /// Multipart uploads.
    Upload,
    /// Binary downloads.
    Download,
}

impl CapabilityKind {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::Upload => "upload",
            Self::Download => "download",
        }
    }

    /// The kind for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "streaming" => Self::Streaming,
            "upload" => Self::Upload,
            "download" => Self::Download,
            _ => return None,
        })
    }

    /// The closed detail vocabulary of this kind.
    pub const fn details(self) -> &'static [&'static str] {
        match self {
            Self::Streaming => &["chunked", "sse", "ws"],
            Self::Upload => &["multipart"],
            Self::Download => &["binary"],
        }
    }

    /// The profile capability id this declaration resolves against.
    pub const fn profile_capability(self) -> &'static str {
        match self {
            Self::Streaming => "transport.streaming",
            Self::Upload => "transport.upload",
            Self::Download => "transport.download",
        }
    }
}

/// The closed capability-detail vocabulary (closed per kind at
/// validation).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CapabilityDetail {
    /// Server-sent events (streaming).
    Sse,
    /// Chunked transfer (streaming).
    Chunked,
    /// WebSockets (streaming).
    Ws,
    /// Multipart form data (upload).
    Multipart,
    /// A binary payload (download).
    Binary,
}

impl CapabilityDetail {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sse => "sse",
            Self::Chunked => "chunked",
            Self::Ws => "ws",
            Self::Multipart => "multipart",
            Self::Binary => "binary",
        }
    }

    /// The detail for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "sse" => Self::Sse,
            "chunked" => Self::Chunked,
            "ws" => Self::Ws,
            "multipart" => Self::Multipart,
            "binary" => Self::Binary,
            _ => return None,
        })
    }
}

/// The closed minimum-support vocabulary of one capability
/// declaration (mirrors the target-profile support states).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CapabilitySupport {
    /// At least partial support.
    Partial,
    /// Full support.
    Full,
}

impl CapabilitySupport {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Partial => "partial",
            Self::Full => "full",
        }
    }

    /// The support for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "partial" => Self::Partial,
            "full" => Self::Full,
            _ => return None,
        })
    }
}

/// One declared per-endpoint capability with its minimum support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityDecl {
    /// The closed capability kind.
    pub capability: CapabilityKind,
    /// The minimum resolved-profile support this endpoint requires.
    pub minimum_support: CapabilitySupport,
    /// The closed detail id.
    pub detail: CapabilityDetail,
}

/// One transport endpoint binding: the full wire surface of one
/// Model `endpoint` symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointBinding {
    /// The Model `endpoint` symbol this binding attaches.
    pub endpoint: SemanticId,
    /// The explicit operation id override.
    pub operation_id: Option<OperationId>,
    /// Bounded OpenAPI-facing metadata tags.
    pub tags: Vec<SafeToken>,
    /// The bounded summary text.
    pub summary: Option<String>,
    /// The declared parameter bindings.
    pub params: Vec<ParamBinding>,
    /// The JSON request body binding.
    pub body: Option<BodyBinding>,
    /// The success response binding.
    pub success: SuccessBinding,
    /// The error → status map.
    pub errors: Vec<ErrorEntry>,
    /// The fixed category-default statuses.
    pub error_defaults: ErrorDefaults,
    /// The security projection.
    pub auth: Option<AuthBinding>,
    /// The idempotency-key binding.
    pub idempotency: Option<IdempotencyBinding>,
    /// The correlation headers.
    pub correlation: Option<CorrelationBinding>,
    /// The pagination projection.
    pub pagination: Option<PaginationBinding>,
    /// The rate-limit declaration.
    pub rate_limit: Option<RateLimitBinding>,
    /// The cache declaration.
    pub cache: Option<CacheBinding>,
    /// The content-version carrier.
    pub api_version: Option<ApiVersionBinding>,
    /// The declared capabilities.
    pub capabilities: Vec<CapabilityDecl>,
    /// The black-box scenario coverage references.
    pub scenarios: Vec<SemanticId>,
}

impl EndpointBinding {
    /// The effective operation identity: the explicit override or
    /// the deterministic derivation.
    pub fn effective_operation_id(&self) -> OperationId {
        self.operation_id
            .clone()
            .unwrap_or_else(|| OperationId::derive(&self.endpoint))
    }
}

/// The transport defaults every endpoint inherits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportDefaults {
    /// The closed error envelope.
    pub error_envelope: &'static str,
    /// The idempotency header name.
    pub idempotency_header: HeaderName,
    /// The correlation header names.
    pub correlation_headers: Vec<HeaderName>,
}

/// The wire dialect declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WireDialect {
    /// The closed dialect token.
    pub dialect: &'static str,
    /// The closed content type.
    pub content_type: &'static str,
}
