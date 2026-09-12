//! Storage projection declarations of the storage-projection
//! attachment (issue #65).
//!
//! A projection is one target namespace mapping every local entity to
//! exactly one table with explicit storage facts: primary key,
//! technical and generated columns, soft-delete and tenant policies,
//! audit timestamps, indexes, join tables, polymorphic materializa-
//! tions, and the migration history with a visible data risk per
//! record. External entities are never mapped. Nothing here changes
//! the domain model; a storage change is always classifiable sepa-
//! rately from a domain change.

use crate::scenario::id::{NamespacedId, SemanticId};

use super::id::{EntityKey, StorageName};

/// Why one textual projection record is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeError {
    /// The record or one of its members is outside the closed grammar.
    Shape,
    /// The storage type is outside the closed namespace vocabulary.
    StorageType,
}

/// The closed target namespaces of v1.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Namespace {
    /// The PostgreSQL namespace.
    Postgres,
    /// The Laravel (Eloquent migration) namespace.
    Laravel,
}

impl Namespace {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Laravel => "laravel",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        match text {
            "postgres" => Ok(Self::Postgres),
            "laravel" => Ok(Self::Laravel),
            _ => Err(ShapeError::Shape),
        }
    }

    /// Whether one declared storage-type token exists in this
    /// namespace's closed vocabulary.
    pub fn accepts_storage_type(self, text: &str) -> bool {
        match self {
            Self::Postgres => matches!(
                text,
                "boolean"
                    | "smallint"
                    | "integer"
                    | "bigint"
                    | "real"
                    | "double precision"
                    | "numeric"
                    | "text"
                    | "varchar"
                    | "uuid"
                    | "date"
                    | "timestamp"
                    | "timestamptz"
                    | "bytea"
                    | "json"
                    | "jsonb"
            ),
            Self::Laravel => matches!(
                text,
                "boolean"
                    | "smallint"
                    | "integer"
                    | "biginteger"
                    | "float"
                    | "double"
                    | "decimal"
                    | "string"
                    | "text"
                    | "uuid"
                    | "date"
                    | "datetime"
                    | "timestamp"
                    | "binary"
                    | "json"
            ),
        }
    }

    /// The namespace 64-bit integer type token.
    pub const fn big_integer(self) -> &'static str {
        match self {
            Self::Postgres => "bigint",
            Self::Laravel => "biginteger",
        }
    }

    /// The namespace instant type token (the audit and soft-delete
    /// policy type).
    pub const fn instant(self) -> &'static str {
        match self {
            Self::Postgres => "timestamptz",
            Self::Laravel => "datetime",
        }
    }

    /// The namespace bounded string token for discriminator columns.
    pub const fn discriminator(self) -> &'static str {
        match self {
            Self::Postgres => "varchar",
            Self::Laravel => "string",
        }
    }
}

/// The closed storage-type vocabulary accepted for declared technical,
/// generated, and tenant columns. The per-namespace subset is enforced
/// against the declaring projection.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StorageType(String);

impl StorageType {
    /// Validate against the closed wire vocabulary (namespace subset
    /// is checked at the semantic layer).
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        let valid = matches!(
            text,
            "boolean"
                | "smallint"
                | "integer"
                | "bigint"
                | "biginteger"
                | "real"
                | "double"
                | "double precision"
                | "numeric"
                | "decimal"
                | "text"
                | "varchar"
                | "string"
                | "uuid"
                | "date"
                | "datetime"
                | "timestamp"
                | "timestamptz"
                | "bytea"
                | "binary"
                | "json"
                | "jsonb"
        );
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(ShapeError::StorageType)
        }
    }

    /// The exact declared token.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One declared storage-only technical column.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TechnicalColumn {
    pub(crate) name: StorageName,
    pub(crate) storage_type: StorageType,
    pub(crate) purpose: String,
    pub(crate) nullable: bool,
}

impl TechnicalColumn {
    /// The column name.
    pub fn name(&self) -> &StorageName {
        &self.name
    }

    /// The declared storage type.
    pub fn storage_type(&self) -> &StorageType {
        &self.storage_type
    }

    /// The bounded purpose tag.
    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    /// Whether the column accepts null.
    pub const fn nullable(&self) -> bool {
        self.nullable
    }
}

/// The closed generated-column kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum GeneratedKind {
    /// The storage engine assigns identities.
    Identity,
    /// The storage engine computes the value.
    Computed,
    /// A storage-backed sequence assigns values.
    Sequence,
}

impl GeneratedKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Computed => "computed",
            Self::Sequence => "sequence",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        match text {
            "identity" => Ok(Self::Identity),
            "computed" => Ok(Self::Computed),
            "sequence" => Ok(Self::Sequence),
            _ => Err(ShapeError::Shape),
        }
    }
}

/// One declared generated or computed column.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedColumn {
    pub(crate) name: StorageName,
    pub(crate) kind: GeneratedKind,
    pub(crate) storage_type: Option<StorageType>,
}

impl GeneratedColumn {
    /// The column name.
    pub fn name(&self) -> &StorageName {
        &self.name
    }

    /// The generation kind.
    pub const fn kind(&self) -> GeneratedKind {
        self.kind
    }

    /// The declared storage type, when declared.
    pub fn storage_type(&self) -> Option<&StorageType> {
        self.storage_type.as_ref()
    }
}

/// One declared index over resolved columns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Index {
    pub(crate) name: Option<StorageName>,
    pub(crate) columns: Vec<StorageName>,
    pub(crate) unique: bool,
}

impl Index {
    /// The optional declared index name.
    pub fn name(&self) -> Option<&StorageName> {
        self.name.as_ref()
    }

    /// The indexed columns, in declared order.
    pub fn columns(&self) -> &[StorageName] {
        &self.columns
    }

    /// Whether the index enforces uniqueness.
    pub const fn unique(&self) -> bool {
        self.unique
    }
}

/// One declared table: the storage home of exactly one local entity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Table {
    pub(crate) entity: EntityKey,
    pub(crate) table: StorageName,
    pub(crate) primary_key: Vec<StorageName>,
    pub(crate) technical_columns: Vec<TechnicalColumn>,
    pub(crate) generated_columns: Vec<GeneratedColumn>,
    pub(crate) soft_delete: Option<StorageName>,
    pub(crate) tenant_key: Option<(StorageName, StorageType)>,
    pub(crate) timestamps: Option<(StorageName, StorageName)>,
    pub(crate) indexes: Vec<Index>,
}

impl Table {
    /// The mapped entity key.
    pub fn entity(&self) -> &EntityKey {
        &self.entity
    }

    /// The explicit table name.
    pub fn table(&self) -> &StorageName {
        &self.table
    }

    /// The primary-key columns, in declared order.
    pub fn primary_key(&self) -> &[StorageName] {
        &self.primary_key
    }

    /// The declared technical columns.
    pub fn technical_columns(&self) -> &[TechnicalColumn] {
        &self.technical_columns
    }

    /// The declared generated columns.
    pub fn generated_columns(&self) -> &[GeneratedColumn] {
        &self.generated_columns
    }

    /// The soft-delete column, when the policy is declared.
    pub fn soft_delete(&self) -> Option<&StorageName> {
        self.soft_delete.as_ref()
    }

    /// The tenant partition key with its declared type, when declared.
    pub fn tenant_key(&self) -> Option<&(StorageName, StorageType)> {
        self.tenant_key.as_ref()
    }

    /// The audit timestamps, when declared.
    pub fn timestamps(&self) -> Option<&(StorageName, StorageName)> {
        self.timestamps.as_ref()
    }

    /// The declared indexes.
    pub fn indexes(&self) -> &[Index] {
        &self.indexes
    }
}

/// One explicit join table materializing one many-to-many relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Join {
    pub(crate) relation: SemanticId,
    pub(crate) table: StorageName,
    pub(crate) columns: [StorageName; 2],
    pub(crate) unique_pair: bool,
}

impl Join {
    /// The materialized relation.
    pub fn relation(&self) -> &SemanticId {
        &self.relation
    }

    /// The explicit join-table name.
    pub fn table(&self) -> &StorageName {
        &self.table
    }

    /// The `[owner column, target column]` pair.
    pub fn columns(&self) -> &[StorageName; 2] {
        &self.columns
    }

    /// Whether the owner-target pair is unique.
    pub const fn unique_pair(&self) -> bool {
        self.unique_pair
    }
}

/// One explicit polymorphic materialization: the target capability
/// opt-in of one namespace for one polymorphic relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Polymorphic {
    pub(crate) relation: SemanticId,
    pub(crate) key_column: StorageName,
    pub(crate) type_column: StorageName,
}

impl Polymorphic {
    /// The materialized relation.
    pub fn relation(&self) -> &SemanticId {
        &self.relation
    }

    /// The target-key column on the owner table.
    pub fn key_column(&self) -> &StorageName {
        &self.key_column
    }

    /// The discriminator column on the owner table.
    pub fn type_column(&self) -> &StorageName {
        &self.type_column
    }
}

/// The closed declared data risk of one migration record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DataRisk {
    /// No data moves.
    None,
    /// Existing rows need a backfill before the change applies.
    BackfillRequired,
    /// Existing data is dropped or rewritten.
    Destructive,
}

impl DataRisk {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::BackfillRequired => "backfill_required",
            Self::Destructive => "destructive",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        match text {
            "none" => Ok(Self::None),
            "backfill_required" => Ok(Self::BackfillRequired),
            "destructive" => Ok(Self::Destructive),
            _ => Err(ShapeError::Shape),
        }
    }
}

/// One declared migration-history record with visible data risk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Migration {
    pub(crate) migration_id: NamespacedId,
    pub(crate) tables: Vec<StorageName>,
    pub(crate) risk: DataRisk,
}

impl Migration {
    /// The stable migration identifier.
    pub fn migration_id(&self) -> &NamespacedId {
        &self.migration_id
    }

    /// The named tables, canonical (byte-sorted) order.
    pub fn tables(&self) -> &[StorageName] {
        &self.tables
    }

    /// The declared data risk.
    pub const fn risk(&self) -> DataRisk {
        self.risk
    }
}

/// One declared target-namespaced storage projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Projection {
    pub(crate) namespace: Namespace,
    pub(crate) tables: Vec<Table>,
    pub(crate) joins: Vec<Join>,
    pub(crate) polymorphics: Vec<Polymorphic>,
    pub(crate) migration_history: Vec<Migration>,
}

impl Projection {
    /// The target namespace.
    pub const fn namespace(&self) -> Namespace {
        self.namespace
    }

    /// The declared tables.
    pub fn tables(&self) -> &[Table] {
        &self.tables
    }

    /// The declared join tables.
    pub fn joins(&self) -> &[Join] {
        &self.joins
    }

    /// The declared polymorphic materializations.
    pub fn polymorphics(&self) -> &[Polymorphic] {
        &self.polymorphics
    }

    /// The declared migration history.
    pub fn migration_history(&self) -> &[Migration] {
        &self.migration_history
    }
}
