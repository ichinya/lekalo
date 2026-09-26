//! Issue #70: the closed, versioned HTTP/JSON transport attachment.
//!
//! One independent, immutable contract family binding each Model
//! `endpoint` symbol of one project to its full HTTP/JSON wire
//! surface: identity and operation ids, parameter locations with
//! path-template agreement, JSON request/response projections, the
//! typed error-to-status projection of the error-contract family,
//! explicit security schemes over the authorization actor
//! vocabulary, idempotency and correlation headers, pagination
//! projection of the query-model contract, rate-limit and cache
//! metadata, content versioning, and declared streaming/upload/
//! download capabilities with per-endpoint minimum support. One
//! deterministic projection derives the canonical route surface
//! every runtime (node, laravel, go, rust) and every OpenAPI
//! rendering consume.
//!
//! Boundaries: the attachment is pure declaration and validation —
//! no execution, no middleware, no runtime principals, tokens,
//! secrets, or URLs, and no write surface. Method, path, and
//! `invokes` stay single-sourced in the Model endpoint symbol; the
//! attachment references it and owns everything else. Validation
//! joins the two: path templates match the declared path parameters
//! exactly in both directions, parameter and body fields resolve to
//! declared operation inputs, error entries stay inside the bound
//! error union, security actors project the closed vocabulary, and
//! capability declarations are checked against the resolved profile.
//!
//! Determinism and denial: canonical bytes are compact UTF-8 JSON
//! with byte-sorted keys; set-like collections normalize to sorted
//! form. Every bound and every semantic contradiction rejects with
//! an explicit registered diagnostic and no partial result.

pub mod canonical;
mod diagnostic;
mod diff;
pub mod id;
pub mod mapping;
mod project;
pub mod source;
pub mod types;
pub(crate) mod validate;
pub mod version;
pub(crate) mod wire;

pub use id::{FieldRef, HeaderName, OperationId, SafeToken, WireName};
pub use types::{
    Actor, ApiKeyLocation, ApiVersionBinding, ApiVersionLocation, AuthBinding, BodyBinding,
    CacheBinding, CachePolicy, CapabilityDecl, CapabilityDetail, CapabilityKind, CapabilitySupport,
    CorrelationBinding, EndpointBinding, ErrorDefaults, ErrorEntry, FieldProjection,
    IdempotencyBinding, PaginationBinding, PaginationStyle, ParamBinding, ParamLocation,
    ParamStyle, ProjectionMode, RateLimitBinding, RateLimitScope, ResponseHeader, SchemeKind,
    SecurityScheme, SuccessBinding, TransportDefaults, WireDialect,
};
pub use version::{
    FAMILY, IDENTITY, IR_IDENTITY, MAX_AUTH_SCHEMES, MAX_CANONICAL_BYTES, MAX_CAPABILITIES,
    MAX_ENDPOINTS, MAX_ERROR_MAP, MAX_SCENARIOS, MAX_SCHEMES, MODEL_VERSION, SCHEMA_VERSION,
    VERSION,
};

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::SemanticId;

pub use diagnostic::{io_failure, rule_set};
pub use diff::{compare, DiffClass, DiffPath, DiffResult};
pub use project::{
    binding_json, project, RouteSurface, NAMESPACE_GO, NAMESPACE_LARAVEL, NAMESPACE_NODE,
    NAMESPACE_RUST,
};
pub use source::{read_document, SOURCE_PATH};
pub use validate::{
    validate, validate_capabilities, CapabilityMap, ProfileSupport, ValidationContext,
};

/// The bound source Model contract: exact accepted version plus
/// digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelPin {
    /// The accepted Model contract version.
    pub(crate) version: crate::scenario::ModelPin,
    /// The digest of the exact source Model document.
    pub(crate) digest: Sha256Digest,
}

impl ModelPin {
    /// Assemble from validated parts (crate internal).
    pub(crate) fn new(version: crate::scenario::ModelPin, digest: Sha256Digest) -> Self {
        Self { version, digest }
    }

    /// The accepted Model contract version.
    pub const fn version(&self) -> &crate::scenario::ModelPin {
        &self.version
    }

    /// The digest of the exact source Model document.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// One finished transport-http attachment: immutable,
/// deterministically ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportDocument {
    attachment_revision: SemVer,
    project_id: SemanticId,
    model_ref: ModelPin,
    ir_digest: Sha256Digest,
    wire: WireDialect,
    defaults: TransportDefaults,
    schemes: Vec<SecurityScheme>,
    endpoints: Vec<EndpointBinding>,
}

impl TransportDocument {
    /// Assemble from validated parts (crate internal); collections
    /// arrive in the caller's normalized order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        attachment_revision: SemVer,
        project_id: SemanticId,
        model_ref: ModelPin,
        ir_digest: Sha256Digest,
        wire: WireDialect,
        defaults: TransportDefaults,
        schemes: Vec<SecurityScheme>,
        endpoints: Vec<EndpointBinding>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_digest,
            wire,
            defaults,
            schemes,
            endpoints,
        }
    }

    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set.
    pub fn from_value(json: &serde_json::Value) -> Result<Self, DiagnosticSet> {
        wire::from_value(json)
    }

    /// The canonical payload bytes (compact JSON, byte-sorted keys,
    /// sorted set-like collections, no trailing LF), or a typed
    /// refusal beyond the payload bound.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        canonical::attachment_bytes(self)
    }

    /// The digest of the canonical payload bytes.
    pub fn digest(&self) -> Result<Sha256Digest, DiagnosticSet> {
        let bytes = self.canonical_bytes()?;
        Ok(Sha256Digest::from_hex(&crate::digest::sha256_hex(
            bytes.as_bytes(),
        )))
    }

    /// The exact attachment revision.
    pub fn attachment_revision(&self) -> &SemVer {
        &self.attachment_revision
    }

    /// The stable project identity.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The bound source Model contract.
    pub const fn model_ref(&self) -> &ModelPin {
        &self.model_ref
    }

    /// The digest of the exact canonical IR payload.
    pub fn ir_digest(&self) -> &Sha256Digest {
        &self.ir_digest
    }

    /// The wire dialect declaration.
    pub const fn wire(&self) -> &WireDialect {
        &self.wire
    }

    /// The transport defaults every endpoint inherits.
    pub fn defaults(&self) -> &TransportDefaults {
        &self.defaults
    }

    /// Every declared security scheme, canonically ordered by id.
    pub fn schemes(&self) -> &[SecurityScheme] {
        &self.schemes
    }

    /// Every endpoint binding, canonically ordered by endpoint id.
    pub fn endpoints(&self) -> &[EndpointBinding] {
        &self.endpoints
    }

    /// One endpoint binding by endpoint id.
    pub fn endpoint(&self, id: &str) -> Option<&EndpointBinding> {
        self.endpoints
            .iter()
            .find(|binding| binding.endpoint.as_str() == id)
    }

    /// One declared scheme by id.
    pub fn scheme(&self, id: &str) -> Option<&SecurityScheme> {
        self.schemes.iter().find(|scheme| scheme.id.as_str() == id)
    }
}
