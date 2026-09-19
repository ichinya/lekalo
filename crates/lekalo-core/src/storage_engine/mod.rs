//! The storage-engine attachment type (issue #69): one closed,
//! versioned profile declaring the engine identity, version pin,
//! policies, tenancy, concurrency, introspection, and test lifecycle.
//!
//! Boundaries: pure declaration and validation — the profile never
//! executes, never connects, never carries credentials, and never
//! embeds provider output. Engine capability answers live in the
//! embedded per-engine profiles and version matrices, not here.

pub mod canonical;
mod diagnostic;
pub mod drift;
pub mod id;
pub mod input;
pub mod introspection;
pub mod postgres;
mod validate;
pub(crate) mod version;
pub(crate) mod wire;

pub use drift::compare as compare_drift;
pub use id::{ConnectionName, Engine, VersionPin};
pub use input::engine_input as input_document;
pub use introspection::IntrospectionEvidence;
pub use version::{FAMILY, IDENTITY, SCHEMA_VERSION, VERSION};

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{SemanticId, SemanticId as ProjectId};

/// The declared SQL-shaping policies of one profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policies {
    pub(crate) identifier_quote_always: bool,
    pub(crate) json: JsonPolicy,
    pub(crate) enum_policy: EnumPolicy,
    pub(crate) array: ArrayPolicy,
    pub(crate) time: TimePolicy,
    pub(crate) pagination: PaginationPolicy,
}

/// The closed JSON column policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum JsonPolicy {
    /// The binary JSON type (the default).
    Jsonb,
    /// The textual JSON type (explicit opt-in only).
    Json,
}

impl JsonPolicy {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Jsonb => "jsonb",
            Self::Json => "json",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, crate::storage_engine::id::ShapeError> {
        match text {
            "jsonb" => Ok(Self::Jsonb),
            "json" => Ok(Self::Json),
            _ => Err(crate::storage_engine::id::ShapeError::Shape),
        }
    }
}

/// The closed enum policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EnumPolicy {
    /// A bounded CHECK predicate over a varchar column.
    Check,
    /// A native engine enum type created per column.
    NativeEnum,
}

impl EnumPolicy {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::NativeEnum => "native_enum",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, crate::storage_engine::id::ShapeError> {
        match text {
            "check" => Ok(Self::Check),
            "native_enum" => Ok(Self::NativeEnum),
            _ => Err(crate::storage_engine::id::ShapeError::Shape),
        }
    }
}

/// The closed array policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ArrayPolicy {
    /// Native engine arrays.
    Native,
    /// Arrays encoded as JSON documents.
    Json,
    /// Declared array fields refuse to render.
    Unsupported,
}

impl ArrayPolicy {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Json => "json",
            Self::Unsupported => "unsupported",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, crate::storage_engine::id::ShapeError> {
        match text {
            "native" => Ok(Self::Native),
            "json" => Ok(Self::Json),
            "unsupported" => Ok(Self::Unsupported),
            _ => Err(crate::storage_engine::id::ShapeError::Shape),
        }
    }
}

/// The closed time policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TimePolicy {
    /// Always timezoned instants in v1.
    pub(crate) instant_timestamptz: bool,
    /// Whether the timezone-naive timestamp may be rendered.
    pub(crate) local_allowed: bool,
}

/// The closed pagination policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PaginationPolicy {
    /// Whether LIMIT/OFFSET may be rendered.
    pub(crate) offset_allowed: bool,
    /// Whether keyset cursors may be rendered.
    pub(crate) cursor_keyset: bool,
}

/// The declared tenancy enforcement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tenancy {
    pub(crate) enforcement: Enforcement,
    pub(crate) rls: Option<RlsPolicy>,
}

/// The closed tenancy enforcement mechanisms.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Enforcement {
    /// No declared enforcement.
    None,
    /// The application scopes every query.
    Application,
    /// Engine row-level security.
    Rls,
}

impl Enforcement {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Application => "application",
            Self::Rls => "rls",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, crate::storage_engine::id::ShapeError> {
        match text {
            "none" => Ok(Self::None),
            "application" => Ok(Self::Application),
            "rls" => Ok(Self::Rls),
            _ => Err(crate::storage_engine::id::ShapeError::Shape),
        }
    }
}

/// The declared row-level-security policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RlsPolicy {
    pub(crate) session_variable: crate::storage_engine::id::SessionVariable,
    pub(crate) force: bool,
}

/// The declared optimistic-versioning policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Concurrency {
    pub(crate) version_column: bool,
    pub(crate) wait: bool,
}

/// The declared checked-mode introspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Introspection {
    pub(crate) scopes: Vec<crate::storage_engine::id::ScopeName>,
    pub(crate) connection: ConnectionName,
}

/// The declared test-database lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestLifecycle {
    pub(crate) isolation: Isolation,
    pub(crate) provision: Provision,
    pub(crate) cleanup: Cleanup,
    pub(crate) connection: ConnectionName,
}

/// The closed test isolation boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Isolation {
    /// One database per run.
    Database,
    /// One schema per run.
    Schema,
    /// One rolled-back transaction per case.
    Transaction,
}

impl Isolation {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Schema => "schema",
            Self::Transaction => "transaction",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, crate::storage_engine::id::ShapeError> {
        match text {
            "database" => Ok(Self::Database),
            "schema" => Ok(Self::Schema),
            "transaction" => Ok(Self::Transaction),
            _ => Err(crate::storage_engine::id::ShapeError::Shape),
        }
    }
}

/// The closed provisioning mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Provision {
    /// Create, then drop.
    CreateDrop,
    /// Clone from a template.
    Template,
    /// Assume the surface exists.
    None,
}

impl Provision {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::CreateDrop => "create_drop",
            Self::Template => "template",
            Self::None => "none",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, crate::storage_engine::id::ShapeError> {
        match text {
            "create_drop" => Ok(Self::CreateDrop),
            "template" => Ok(Self::Template),
            "none" => Ok(Self::None),
            _ => Err(crate::storage_engine::id::ShapeError::Shape),
        }
    }
}

/// The closed cleanup mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Cleanup {
    /// Drop the test surface.
    Drop,
    /// Truncate the test tables.
    Truncate,
    /// Roll the transaction back.
    Rollback,
}

impl Cleanup {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Drop => "drop",
            Self::Truncate => "truncate",
            Self::Rollback => "rollback",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, crate::storage_engine::id::ShapeError> {
        match text {
            "drop" => Ok(Self::Drop),
            "truncate" => Ok(Self::Truncate),
            "rollback" => Ok(Self::Rollback),
            _ => Err(crate::storage_engine::id::ShapeError::Shape),
        }
    }
}

/// One allow-listed extension.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Extension {
    pub(crate) name: crate::storage_engine::id::ExtensionName,
    pub(crate) required: bool,
}

/// One finished storage-engine attachment: immutable,
/// deterministically ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageEngineAttachment {
    attachment_revision: SemVer,
    project_id: ProjectId,
    model_ref: crate::storage_projection::ModelPin,
    ir_digest: Sha256Digest,
    projection_ref: Sha256Digest,
    engine: Engine,
    engine_version: VersionPin,
    policies: Policies,
    tenancy: Option<Tenancy>,
    concurrency: Option<Concurrency>,
    introspection: Option<Introspection>,
    test_lifecycle: Option<TestLifecycle>,
    extensions: Vec<Extension>,
}

impl StorageEngineAttachment {
    /// Assemble from validated parts (crate internal); collections are
    /// stored in the caller's normalized order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        attachment_revision: SemVer,
        project_id: ProjectId,
        model_ref: crate::storage_projection::ModelPin,
        ir_digest: Sha256Digest,
        projection_ref: Sha256Digest,
        engine: Engine,
        engine_version: VersionPin,
        policies: Policies,
        tenancy: Option<Tenancy>,
        concurrency: Option<Concurrency>,
        introspection: Option<Introspection>,
        test_lifecycle: Option<TestLifecycle>,
        extensions: Vec<Extension>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_digest,
            projection_ref,
            engine,
            engine_version,
            policies,
            tenancy,
            concurrency,
            introspection,
            test_lifecycle,
            extensions,
        }
    }

    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set.
    pub fn from_value(json: &serde_json::Value) -> Result<Self, DiagnosticSet> {
        wire::from_value(json)
    }

    /// The canonical payload bytes (compact JSON, byte-sorted keys,
    /// no trailing LF), or a typed refusal beyond the payload bound.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        canonical::attachment_bytes(self)
    }

    /// The internal semantic check (wire normalization and typed
    /// revalidation share one entry).
    pub(crate) fn semantic_self_check(&self) -> Result<(), DiagnosticSet> {
        validate::semantic_self_check(self)
    }

    /// The semantic self-check over this attachment; pure and
    /// read-only.
    pub fn validate_attachment(&self) -> Result<(), DiagnosticSet> {
        self.semantic_self_check()
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
    pub const fn model_ref(&self) -> &crate::storage_projection::ModelPin {
        &self.model_ref
    }

    /// The digest of the exact canonical IR payload.
    pub fn ir_digest(&self) -> &Sha256Digest {
        &self.ir_digest
    }

    /// The digest of the bound storage-projection attachment.
    pub fn projection_ref(&self) -> &Sha256Digest {
        &self.projection_ref
    }

    /// The declared engine.
    pub const fn engine(&self) -> Engine {
        self.engine
    }

    /// The exact engine version pin.
    pub const fn engine_version(&self) -> &VersionPin {
        &self.engine_version
    }

    /// The declared SQL-shaping policies.
    pub const fn policies(&self) -> &Policies {
        &self.policies
    }

    /// The declared tenancy enforcement, when declared.
    pub fn tenancy(&self) -> Option<&Tenancy> {
        self.tenancy.as_ref()
    }

    /// The declared optimistic-versioning policy, when declared.
    pub fn concurrency(&self) -> Option<&Concurrency> {
        self.concurrency.as_ref()
    }

    /// The declared checked-mode introspection, when declared.
    pub fn introspection(&self) -> Option<&Introspection> {
        self.introspection.as_ref()
    }

    /// The declared test-database lifecycle, when declared.
    pub fn test_lifecycle(&self) -> Option<&TestLifecycle> {
        self.test_lifecycle.as_ref()
    }

    /// The extension allow-list, canonical (byte-sorted) order.
    pub fn extensions(&self) -> &[Extension] {
        &self.extensions
    }
}

/// The typed read-only I/O failure of one CLI handoff.
pub fn io_failure(detail: &'static str) -> crate::diagnostics::DiagnosticSet {
    diagnostic::io_invalid(detail)
}

/// The typed unsupported refusal of one CLI handoff.
pub fn unsupported_failure(detail: &'static str) -> crate::diagnostics::DiagnosticSet {
    diagnostic::unsupported_version_set(detail)
}

impl Policies {
    /// Whether emitted identifiers are always quoted (the only v1
    /// answer).
    pub const fn identifier_quote_always(&self) -> bool {
        self.identifier_quote_always
    }

    /// The JSON column policy.
    pub const fn json(&self) -> JsonPolicy {
        self.json
    }

    /// The enum policy.
    pub const fn enum_policy(&self) -> EnumPolicy {
        self.enum_policy
    }

    /// The array policy.
    pub const fn array(&self) -> ArrayPolicy {
        self.array
    }

    /// The time policy.
    pub const fn time(&self) -> &TimePolicy {
        &self.time
    }

    /// The pagination policy.
    pub const fn pagination(&self) -> &PaginationPolicy {
        &self.pagination
    }
}

impl TimePolicy {
    /// Whether instants render as the timezoned type.
    pub const fn instant_timestamptz(&self) -> bool {
        self.instant_timestamptz
    }

    /// Whether the timezone-naive timestamp may be rendered.
    pub const fn local_allowed(&self) -> bool {
        self.local_allowed
    }
}

impl PaginationPolicy {
    /// Whether LIMIT/OFFSET may be rendered.
    pub const fn offset_allowed(&self) -> bool {
        self.offset_allowed
    }

    /// Whether keyset cursors may be rendered.
    pub const fn cursor_keyset(&self) -> bool {
        self.cursor_keyset
    }
}

impl Tenancy {
    /// The enforcement mechanism.
    pub const fn enforcement(&self) -> Enforcement {
        self.enforcement
    }

    /// The RLS policy, when enforcement is RLS.
    pub fn rls(&self) -> Option<&RlsPolicy> {
        self.rls.as_ref()
    }
}

impl RlsPolicy {
    /// The tenant session variable.
    pub fn session_variable(&self) -> &crate::storage_engine::id::SessionVariable {
        &self.session_variable
    }

    /// Whether RLS applies to the table owner too.
    pub const fn force(&self) -> bool {
        self.force
    }
}

impl Concurrency {
    /// Whether a version column is declared.
    pub const fn version_column(&self) -> bool {
        self.version_column
    }

    /// Whether lock acquisition waits.
    pub const fn wait(&self) -> bool {
        self.wait
    }
}

impl Introspection {
    /// The observed schema allow-list.
    pub fn scopes(&self) -> &[crate::storage_engine::id::ScopeName] {
        &self.scopes
    }

    /// The opaque connection name.
    pub fn connection(&self) -> &ConnectionName {
        &self.connection
    }
}

impl TestLifecycle {
    /// The isolation boundary.
    pub const fn isolation(&self) -> Isolation {
        self.isolation
    }

    /// The provisioning mode.
    pub const fn provision(&self) -> Provision {
        self.provision
    }

    /// The cleanup mode.
    pub const fn cleanup(&self) -> Cleanup {
        self.cleanup
    }

    /// The opaque connection name.
    pub fn connection(&self) -> &ConnectionName {
        &self.connection
    }
}

impl Extension {
    /// The extension name.
    pub fn name(&self) -> &crate::storage_engine::id::ExtensionName {
        &self.name
    }

    /// Whether the extension must already exist.
    pub const fn required(&self) -> bool {
        self.required
    }
}
