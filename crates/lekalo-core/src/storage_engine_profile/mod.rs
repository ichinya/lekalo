//! Issue #117: the closed, versioned storage-engine-profile attachment.
//!
//! One independent, immutable contract family carrying the exact
//! capability evidence of one storage engine generation: the engine
//! identity block (closed engine token, exact version, closed variant,
//! declared sql mode, declared charset/collation/time zone, bounded
//! provenance) and the closed capability map where every record carries
//! `full|partial|unsupported` with bounded evidence and `partial`
//! requiring its bounds note. MySQL and MariaDB are separate profiles,
//! never one optimistic family. The optional test-lifecycle section
//! declares the ephemeral test-database capability with the closed
//! `production: forbidden` token, and the adapters list carries
//! runtime-adapter mapping evidence without ever selecting a runtime.
//!
//! Boundaries: this module is pure declaration, validation,
//! canonicalization, capability bridging, and comparison — no engine
//! connection, no query execution, no credentials or hosts anywhere,
//! and no application-runtime selection. Every bound and every
//! semantic contradiction rejects with an explicit registered
//! diagnostic and no partial result.

pub mod canonical;
pub mod capabilities;
pub mod diagnostic;
pub mod diff;
pub mod id;
pub mod portability;
mod validate;
pub mod version;
pub mod wire;

pub use capabilities::{to_snapshot, ProfileSupport};
pub use diff::{compare, DiffClass, DiffLayer, DiffPath, DiffResult};
pub use id::{EngineToken, ProfileCapabilityId, SqlModeToken, VariantToken};
pub use portability::{
    named_postgres_divergences, portability, NamedDivergence, PortabilityReport, ReportSupport,
};
pub use version::{FAMILY, IDENTITY, SCHEMA_VERSION, VERSION};

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{NamespacedId, SemanticId};

/// One bounded provenance record.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Evidence {
    pub(crate) kind: EvidenceKind,
    pub(crate) reference: String,
}

impl Evidence {
    /// The closed provenance kind.
    pub const fn kind(&self) -> EvidenceKind {
        self.kind
    }

    /// The bounded reference declaration.
    pub fn reference(&self) -> &str {
        &self.reference
    }
}

/// The closed provenance kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EvidenceKind {
    /// Vendor documentation.
    VendorDocs,
    /// The engine reference manual.
    EngineReference,
    /// Adapter documentation.
    AdapterDocs,
    /// One recorded observed run.
    ObservedRun,
}

impl EvidenceKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::VendorDocs => "vendor-docs",
            Self::EngineReference => "engine-reference",
            Self::AdapterDocs => "adapter-docs",
            Self::ObservedRun => "observed-run",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "vendor-docs" => Some(Self::VendorDocs),
            "engine-reference" => Some(Self::EngineReference),
            "adapter-docs" => Some(Self::AdapterDocs),
            "observed-run" => Some(Self::ObservedRun),
            _ => None,
        }
    }
}

/// The exact engine identity block: everything capability semantics
/// depend on, declared and bounded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineIdentity {
    pub(crate) engine: EngineToken,
    pub(crate) engine_version: String,
    pub(crate) variant: VariantToken,
    pub(crate) sql_mode: Vec<SqlModeToken>,
    pub(crate) default_storage_engine: String,
    pub(crate) charset: String,
    pub(crate) collation: String,
    pub(crate) time_zone: String,
    pub(crate) evidence: Evidence,
}

impl EngineIdentity {
    /// The closed engine token.
    pub const fn engine(&self) -> EngineToken {
        self.engine
    }

    /// The exact declared engine release (major.minor.patch).
    pub fn engine_version(&self) -> &str {
        &self.engine_version
    }

    /// The closed distribution variant.
    pub const fn variant(&self) -> VariantToken {
        self.variant
    }

    /// The declared sql mode, canonical sorted order.
    pub fn sql_mode(&self) -> &[SqlModeToken] {
        &self.sql_mode
    }

    /// The declared default storage engine.
    pub fn default_storage_engine(&self) -> &str {
        &self.default_storage_engine
    }

    /// The declared default charset.
    pub fn charset(&self) -> &str {
        &self.charset
    }

    /// The declared default collation.
    pub fn collation(&self) -> &str {
        &self.collation
    }

    /// The declared session time zone.
    pub fn time_zone(&self) -> &str {
        &self.time_zone
    }

    /// The identity provenance record.
    pub const fn evidence(&self) -> &Evidence {
        &self.evidence
    }
}

/// One closed capability support record.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Capability {
    pub(crate) support: Support,
    pub(crate) bounds: Option<String>,
    pub(crate) evidence: Evidence,
}

impl Capability {
    /// The closed support state.
    pub const fn support(&self) -> Support {
        self.support
    }

    /// The mandatory bounded gap note of partial support.
    pub fn bounds(&self) -> Option<&str> {
        self.bounds.as_deref()
    }

    /// The capability provenance record.
    pub const fn evidence(&self) -> &Evidence {
        &self.evidence
    }
}

/// The closed capability support states. Absence of the whole record is
/// `unknown` and is never yes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Support {
    /// Fully present.
    Full,
    /// Present with documented, bounded gaps.
    Partial,
    /// Declared absent.
    Unsupported,
}

impl Support {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "full" => Some(Self::Full),
            "partial" => Some(Self::Partial),
            "unsupported" => Some(Self::Unsupported),
            _ => None,
        }
    }
}

/// The declared ephemeral test-database lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestLifecycle {
    pub(crate) create: Capability,
    pub(crate) drop: Capability,
    pub(crate) isolation: TestIsolation,
    pub(crate) test_schema_prefix: String,
    pub(crate) evidence: Evidence,
}

impl TestLifecycle {
    /// The create-schema capability.
    pub const fn create(&self) -> &Capability {
        &self.create
    }

    /// The drop-schema capability.
    pub const fn drop(&self) -> &Capability {
        &self.drop
    }

    /// The closed isolation token.
    pub const fn isolation(&self) -> TestIsolation {
        self.isolation
    }

    /// The mandatory test-schema prefix.
    pub fn test_schema_prefix(&self) -> &str {
        &self.test_schema_prefix
    }

    /// The lifecycle provenance record.
    pub const fn evidence(&self) -> &Evidence {
        &self.evidence
    }
}

/// The closed test-isolation vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TestIsolation {
    /// One schema per run.
    SchemaPerRun,
}

impl TestIsolation {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::SchemaPerRun => "schema-per-run",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "schema-per-run" => Some(Self::SchemaPerRun),
            _ => None,
        }
    }
}

/// One runtime-adapter mapping evidence record.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct AdapterEvidence {
    pub(crate) name: AdapterToken,
    pub(crate) dialect: String,
    pub(crate) evidence: Evidence,
}

impl AdapterEvidence {
    /// The closed adapter token.
    pub const fn name(&self) -> AdapterToken {
        self.name
    }

    /// The bounded dialect declaration.
    pub fn dialect(&self) -> &str {
        &self.dialect
    }

    /// The adapter provenance record.
    pub const fn evidence(&self) -> &Evidence {
        &self.evidence
    }
}

/// The closed adapter tokens.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum AdapterToken {
    /// The Drizzle ORM adapter.
    Drizzle,
}

impl AdapterToken {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Drizzle => "drizzle",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "drizzle" => Some(Self::Drizzle),
            _ => None,
        }
    }
}

/// One finished storage-engine-profile attachment: immutable,
/// deterministically ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageEngineProfile {
    attachment_revision: SemVer,
    project_id: SemanticId,
    model_ref: crate::storage_projection::ModelPin,
    ir_digest: Sha256Digest,
    source_map_ref: Option<NamespacedId>,
    engine: EngineIdentity,
    capabilities: Vec<(ProfileCapabilityId, Capability)>,
    test_lifecycle: TestLifecycle,
    adapters: Vec<AdapterEvidence>,
}

impl StorageEngineProfile {
    /// Assemble from validated parts (crate internal); collections are
    /// stored in the caller's normalized order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        attachment_revision: SemVer,
        project_id: SemanticId,
        model_ref: crate::storage_projection::ModelPin,
        ir_digest: Sha256Digest,
        source_map_ref: Option<NamespacedId>,
        engine: EngineIdentity,
        capabilities: Vec<(ProfileCapabilityId, Capability)>,
        test_lifecycle: TestLifecycle,
        adapters: Vec<AdapterEvidence>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_digest,
            source_map_ref,
            engine,
            capabilities,
            test_lifecycle,
            adapters,
        }
    }

    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set.
    pub fn from_value(json: &serde_json::Value) -> Result<Self, DiagnosticSet> {
        wire::from_value(json)
    }

    /// The canonical payload bytes (compact JSON, byte-sorted keys,
    /// sorted set-like collections, no trailing LF), or a typed refusal
    /// beyond the payload bound.
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

    /// The optional bound source-map reference.
    pub fn source_map_ref(&self) -> Option<&NamespacedId> {
        self.source_map_ref.as_ref()
    }

    /// The engine identity block.
    pub const fn engine(&self) -> &EngineIdentity {
        &self.engine
    }

    /// Every declared capability, canonical (byte-sorted id) order.
    pub fn capabilities(&self) -> &[(ProfileCapabilityId, Capability)] {
        &self.capabilities
    }

    /// Resolve one capability by id; `None` is `unknown` and is never
    /// yes.
    pub fn capability(&self, id: &ProfileCapabilityId) -> Option<&Capability> {
        self.capabilities
            .iter()
            .find(|(declared, _)| declared == id)
            .map(|(_, capability)| capability)
    }

    /// The declared test lifecycle.
    pub const fn test_lifecycle(&self) -> &TestLifecycle {
        &self.test_lifecycle
    }

    /// The declared adapter evidence, canonical order.
    pub fn adapters(&self) -> &[AdapterEvidence] {
        &self.adapters
    }
}
