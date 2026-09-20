//! Issue #117: the closed, versioned storage-introspection evidence
//! attachment.
//!
//! One independent, immutable contract family carrying one
//! adapter-produced, read-only schema introspection of one explicitly
//! configured test schema in explicit checked mode. The runtime adapter
//! — never the core — runs the read-only information-schema queries and
//! emits the document; the core validates it and compares it against a
//! declared projection. `mode` is the constant `checked` and
//! `readOnly` the constant `true`: an unchecked scan is inexpressible.
//! Credentials, hosts, URLs, ports, users, passwords, and data source
//! names are inexpressible in the closed member sets.
//!
//! Boundaries: this module is pure declaration, validation,
//! canonicalization, and drift comparison — no engine connection, no
//! query execution, no guessed repairs. Evidence absent means unknown,
//! never clean.

pub mod canonical;
pub mod check;
pub mod diagnostic;
pub mod id;
mod validate;
pub mod version;
pub mod wire;

pub use check::{introspect_check, Drift, DriftKind, DriftReport};
pub use id::SchemaName;
pub use version::{FAMILY, IDENTITY, SCHEMA_VERSION, VERSION};

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{NamespacedId, SemanticId};

/// One bounded provenance record of the producing run.
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

/// The observed engine identity echo of the introspected instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedEngine {
    pub(crate) engine: EngineEcho,
    pub(crate) engine_version: String,
    pub(crate) sql_mode: Vec<SqlModeEcho>,
    pub(crate) default_storage_engine: String,
    pub(crate) charset: String,
    pub(crate) collation: String,
    pub(crate) time_zone: String,
}

impl ObservedEngine {
    /// The observed engine token.
    pub const fn engine(&self) -> EngineEcho {
        self.engine
    }

    /// The exact observed engine release.
    pub fn engine_version(&self) -> &str {
        &self.engine_version
    }

    /// The observed sql mode, canonical sorted order.
    pub fn sql_mode(&self) -> &[SqlModeEcho] {
        &self.sql_mode
    }

    /// The observed default storage engine.
    pub fn default_storage_engine(&self) -> &str {
        &self.default_storage_engine
    }

    /// The observed default charset.
    pub fn charset(&self) -> &str {
        &self.charset
    }

    /// The observed default collation.
    pub fn collation(&self) -> &str {
        &self.collation
    }

    /// The observed session time zone.
    pub fn time_zone(&self) -> &str {
        &self.time_zone
    }
}

/// The closed engine echo.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EngineEcho {
    /// The observed engine is MySQL.
    Mysql,
    /// The observed engine is MariaDB.
    Mariadb,
}

impl EngineEcho {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Mysql => "mysql",
            Self::Mariadb => "mariadb",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "mysql" => Some(Self::Mysql),
            "mariadb" => Some(Self::Mariadb),
            _ => None,
        }
    }
}

/// One closed observed sql-mode token.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SqlModeEcho {
    AnsiQuotes,
    ErrorForDivisionByZero,
    NoEngineSubstitution,
    NoZeroDate,
    NoZeroInDate,
    OnlyFullGroupBy,
    StrictAllTables,
    StrictTransTables,
    Traditional,
}

impl SqlModeEcho {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::AnsiQuotes => "ANSI_QUOTES",
            Self::ErrorForDivisionByZero => "ERROR_FOR_DIVISION_BY_ZERO",
            Self::NoEngineSubstitution => "NO_ENGINE_SUBSTITUTION",
            Self::NoZeroDate => "NO_ZERO_DATE",
            Self::NoZeroInDate => "NO_ZERO_IN_DATE",
            Self::OnlyFullGroupBy => "ONLY_FULL_GROUP_BY",
            Self::StrictAllTables => "STRICT_ALL_TABLES",
            Self::StrictTransTables => "STRICT_TRANS_TABLES",
            Self::Traditional => "TRADITIONAL",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "ANSI_QUOTES" => Some(Self::AnsiQuotes),
            "ERROR_FOR_DIVISION_BY_ZERO" => Some(Self::ErrorForDivisionByZero),
            "NO_ENGINE_SUBSTITUTION" => Some(Self::NoEngineSubstitution),
            "NO_ZERO_DATE" => Some(Self::NoZeroDate),
            "NO_ZERO_IN_DATE" => Some(Self::NoZeroInDate),
            "ONLY_FULL_GROUP_BY" => Some(Self::OnlyFullGroupBy),
            "STRICT_ALL_TABLES" => Some(Self::StrictAllTables),
            "STRICT_TRANS_TABLES" => Some(Self::StrictTransTables),
            "TRADITIONAL" => Some(Self::Traditional),
            _ => None,
        }
    }
}

/// One observed column.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ObservedColumn {
    pub(crate) name: String,
    pub(crate) storage_type: String,
    pub(crate) nullable: bool,
    pub(crate) charset: Option<String>,
    pub(crate) collation: Option<String>,
    pub(crate) default: Option<String>,
    pub(crate) extra: Option<String>,
}

impl ObservedColumn {
    /// The observed column name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The observed data type spelling.
    pub fn storage_type(&self) -> &str {
        &self.storage_type
    }

    /// Whether the column accepts null.
    pub const fn nullable(&self) -> bool {
        self.nullable
    }

    /// The observed column charset, when textual.
    pub fn charset(&self) -> Option<&str> {
        self.charset.as_deref()
    }

    /// The observed column collation, when textual.
    pub fn collation(&self) -> Option<&str> {
        self.collation.as_deref()
    }

    /// The bounded observed default declaration.
    pub fn default(&self) -> Option<&str> {
        self.default.as_deref()
    }

    /// The bounded observed extra declaration.
    pub fn extra(&self) -> Option<&str> {
        self.extra.as_deref()
    }
}

/// One observed index.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ObservedIndex {
    pub(crate) name: Option<String>,
    pub(crate) columns: Vec<String>,
    pub(crate) unique: bool,
    pub(crate) kind: Option<ObservedIndexKind>,
    pub(crate) prefix_lengths: Option<Vec<u16>>,
}

impl ObservedIndex {
    /// The optional observed index name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The observed key columns in observed order.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// Whether the index enforces uniqueness.
    pub const fn unique(&self) -> bool {
        self.unique
    }

    /// The observed index kind; `None` is the implicit btree.
    pub const fn kind(&self) -> Option<ObservedIndexKind> {
        self.kind
    }

    /// The observed per-column prefix lengths.
    pub fn prefix_lengths(&self) -> Option<&[u16]> {
        self.prefix_lengths.as_deref()
    }
}

/// The closed observed index kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ObservedIndexKind {
    /// The default ordered index.
    Btree,
    /// The full-text index.
    Fulltext,
    /// The spatial index.
    Spatial,
}

impl ObservedIndexKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Btree => "btree",
            Self::Fulltext => "fulltext",
            Self::Spatial => "spatial",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "btree" => Some(Self::Btree),
            "fulltext" => Some(Self::Fulltext),
            "spatial" => Some(Self::Spatial),
            _ => None,
        }
    }
}

/// One observed foreign key.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ObservedForeignKey {
    pub(crate) column: String,
    pub(crate) references_table: String,
    pub(crate) on_delete: ObservedDeleteAction,
}

impl ObservedForeignKey {
    /// The local foreign-key column.
    pub fn column(&self) -> &str {
        &self.column
    }

    /// The referenced table.
    pub fn references_table(&self) -> &str {
        &self.references_table
    }

    /// The observed delete action.
    pub const fn on_delete(&self) -> ObservedDeleteAction {
        self.on_delete
    }
}

/// The closed observed delete actions (the observed set includes the
/// MySQL `NO ACTION` spelling beside the shared three).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ObservedDeleteAction {
    Cascade,
    Restrict,
    SetNull,
    NoAction,
}

impl ObservedDeleteAction {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Cascade => "CASCADE",
            Self::Restrict => "RESTRICT",
            Self::SetNull => "SET NULL",
            Self::NoAction => "NO ACTION",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "CASCADE" => Some(Self::Cascade),
            "RESTRICT" => Some(Self::Restrict),
            "SET NULL" => Some(Self::SetNull),
            "NO ACTION" => Some(Self::NoAction),
            _ => None,
        }
    }
}

/// One observed table with its columns, indexes, and foreign keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedTable {
    pub(crate) name: String,
    pub(crate) engine: String,
    pub(crate) collation: String,
    pub(crate) columns: Vec<ObservedColumn>,
    pub(crate) indexes: Vec<ObservedIndex>,
    pub(crate) foreign_keys: Vec<ObservedForeignKey>,
}

impl ObservedTable {
    /// The observed table name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The observed storage engine of this table.
    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// The observed table collation.
    pub fn collation(&self) -> &str {
        &self.collation
    }

    /// The observed columns, canonical order by name.
    pub fn columns(&self) -> &[ObservedColumn] {
        &self.columns
    }

    /// The observed indexes, canonical order.
    pub fn indexes(&self) -> &[ObservedIndex] {
        &self.indexes
    }

    /// The observed foreign keys, canonical order by column.
    pub fn foreign_keys(&self) -> &[ObservedForeignKey] {
        &self.foreign_keys
    }
}

/// One finished storage-introspection attachment: immutable,
/// deterministically ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageIntrospection {
    attachment_revision: SemVer,
    project_id: SemanticId,
    model_ref: crate::storage_projection::ModelPin,
    ir_digest: Sha256Digest,
    source_map_ref: Option<NamespacedId>,
    engine: ObservedEngine,
    test_schema: SchemaName,
    tables: Vec<ObservedTable>,
    observed_digest: Sha256Digest,
    evidence: Option<Evidence>,
}

impl StorageIntrospection {
    /// Assemble from validated parts (crate internal); collections are
    /// stored in the caller's normalized order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        attachment_revision: SemVer,
        project_id: SemanticId,
        model_ref: crate::storage_projection::ModelPin,
        ir_digest: Sha256Digest,
        source_map_ref: Option<NamespacedId>,
        engine: ObservedEngine,
        test_schema: SchemaName,
        tables: Vec<ObservedTable>,
        observed_digest: Sha256Digest,
        evidence: Option<Evidence>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_digest,
            source_map_ref,
            engine,
            test_schema,
            tables,
            observed_digest,
            evidence,
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

    /// The observed engine identity echo.
    pub const fn engine(&self) -> &ObservedEngine {
        &self.engine
    }

    /// The one bound test schema name.
    pub const fn test_schema(&self) -> &SchemaName {
        &self.test_schema
    }

    /// The observed tables, canonical order by name.
    pub fn tables(&self) -> &[ObservedTable] {
        &self.tables
    }

    /// The declared digest over the canonical observed payload.
    pub const fn observed_digest(&self) -> &Sha256Digest {
        &self.observed_digest
    }

    /// The optional provenance record of the producing run.
    pub fn evidence(&self) -> Option<&Evidence> {
        self.evidence.as_ref()
    }
}
