//! Storage projection declarations of the storage-projection
//! attachment (issue #65).
//!
//! A projection is one target namespace mapping every local entity to
//! exactly one table with explicit storage facts: primary key,
//! technical and generated columns, soft-delete and tenant policies,
//! audit timestamps, indexes, join tables, polymorphic materializa-
//! tions. External entities are never mapped. Nothing here changes
//! the domain model; a storage change is always classifiable sepa-
//! rately from a domain change.

use crate::scenario::id::SemanticId;

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

/// The closed predicate operator of one column predicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PredicateOp {
    /// The column holds no value.
    IsNull,
    /// The column holds a value.
    IsNotNull,
    /// The column equals the literal.
    Eq,
    /// The column differs from the literal.
    Ne,
}

impl PredicateOp {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::IsNull => "is-null",
            Self::IsNotNull => "is-not-null",
            Self::Eq => "eq",
            Self::Ne => "ne",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        match text {
            "is-null" => Ok(Self::IsNull),
            "is-not-null" => Ok(Self::IsNotNull),
            "eq" => Ok(Self::Eq),
            "ne" => Ok(Self::Ne),
            _ => Err(ShapeError::Shape),
        }
    }

    /// Whether the operator carries a comparison literal.
    pub const fn carries_value(self) -> bool {
        matches!(self, Self::Eq | Self::Ne)
    }
}

/// One closed column predicate over one declared column and a typed
/// literal.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ColumnPredicate {
    pub(crate) column: StorageName,
    pub(crate) op: PredicateOp,
    pub(crate) value: Option<super::entity::Literal>,
}

impl ColumnPredicate {
    /// The compared column.
    pub fn column(&self) -> &StorageName {
        &self.column
    }

    /// The closed operator.
    pub const fn op(&self) -> PredicateOp {
        self.op
    }

    /// The typed comparison literal, when the operator carries one.
    pub fn value(&self) -> Option<&super::entity::Literal> {
        self.value.as_ref()
    }
}

/// One bounded conjunction of column predicates (the implicit AND).
pub type PredicateConjunction = Vec<ColumnPredicate>;

/// One declared named CHECK constraint.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CheckConstraint {
    pub(crate) name: StorageName,
    pub(crate) where_: PredicateConjunction,
}

impl CheckConstraint {
    /// The constraint name.
    pub fn name(&self) -> &StorageName {
        &self.name
    }

    /// The bounded predicate conjunction.
    pub fn predicates(&self) -> &PredicateConjunction {
        &self.where_
    }
}

/// One declared index over resolved columns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Index {
    pub(crate) name: Option<StorageName>,
    pub(crate) columns: Vec<StorageName>,
    pub(crate) unique: bool,
    pub(crate) where_: Option<PredicateConjunction>,
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

    /// The optional partial-index predicate.
    pub fn where_(&self) -> Option<&PredicateConjunction> {
        self.where_.as_ref()
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
    pub(crate) checks: Vec<CheckConstraint>,
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

    /// The declared CHECK constraints, canonical (name) order.
    pub fn checks(&self) -> &[CheckConstraint] {
        &self.checks
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

/// One declared target-namespaced storage projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Projection {
    pub(crate) namespace: Namespace,
    pub(crate) tables: Vec<Table>,
    pub(crate) joins: Vec<Join>,
    pub(crate) polymorphics: Vec<Polymorphic>,
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
}
