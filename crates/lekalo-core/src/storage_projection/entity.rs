//! Domain entities of the storage-projection attachment (issue #65).
//!
//! A domain entity binds one stable entity key to one Model symbol,
//! one closed visibility, one optional aggregate role, the declared
//! target-neutral fields with closed value types, and opaque
//! references into the #63 invariant and state-space families. The
//! record never carries storage facts: those live only in target-
//! namespaced projections, which is what keeps a storage-only
//! technical field out of the public surface by construction.

use crate::scenario::id::{FieldName, NamespacedId, SemanticId};

use super::id::EntityKey;

/// Why one textual domain entity or field record is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeError {
    /// The record or one of its members is outside the closed grammar.
    Shape,
}

/// The closed domain visibility. Only public members enter the public
/// DTO projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Visibility {
    /// Visible to every consumer.
    Public,
    /// Visible inside the owning module boundary.
    Internal,
    /// Visible only inside the aggregate.
    Private,
}

impl Visibility {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Private => "private",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        match text {
            "public" => Ok(Self::Public),
            "internal" => Ok(Self::Internal),
            "private" => Ok(Self::Private),
            _ => Err(ShapeError::Shape),
        }
    }

    /// Whether the candidate visibility accepts every observer of
    /// `self` (a widening is non-breaking, a narrowing is breaking).
    pub const fn widens(self, candidate: Self) -> bool {
        matches!(
            (self, candidate),
            (Self::Private, Self::Internal)
                | (Self::Private, Self::Public)
                | (Self::Internal, Self::Public)
        )
    }
}

/// The closed domain value type: the target-neutral leaf vocabulary of
/// a declared field. Parameters are present exactly when the name
/// requires them.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DomainType {
    /// A true or false value.
    Boolean,
    /// A signed integer.
    Integer,
    /// A fixed-point decimal with declared precision and scale.
    Decimal { precision: i64, scale: i64 },
    /// A bounded string with an exact length in Unicode scalar values.
    String { length: i64 },
    /// An unbounded-but-bounded-declaration text.
    Text,
    /// A lowercase UUID.
    Uuid,
    /// A calendar date.
    Date,
    /// An instant.
    Timestamp,
    /// Opaque bytes.
    Binary,
    /// A structured document.
    Json,
}

impl DomainType {
    /// The exact wire name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Decimal { .. } => "decimal",
            Self::String { .. } => "string",
            Self::Text => "text",
            Self::Uuid => "uuid",
            Self::Date => "date",
            Self::Timestamp => "timestamp",
            Self::Binary => "binary",
            Self::Json => "json",
        }
    }

    /// Whether the candidate type accepts every value of `self`
    /// (used by the diff: a widening is non-breaking, everything else
    /// is breaking).
    pub fn widens(&self, candidate: &Self) -> bool {
        match (self, candidate) {
            (Self::String { length: base }, Self::String { length: next }) => next >= base,
            (
                Self::Decimal {
                    precision: bp,
                    scale: bs,
                },
                Self::Decimal {
                    precision: np,
                    scale: ns,
                },
            ) => np >= bp && ns >= bs,
            (Self::Integer, Self::Decimal { .. }) | (Self::Date, Self::Timestamp) => true,
            (Self::String { .. }, Self::Text) => true,
            _ => self == candidate,
        }
    }
}

/// One declared domain field: name, target-neutral value type,
/// optionality, and visibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainField {
    pub(crate) field: FieldName,
    pub(crate) field_type: DomainType,
    pub(crate) required: bool,
    pub(crate) visibility: Visibility,
}

impl DomainField {
    /// The validated field name.
    pub fn name(&self) -> &FieldName {
        &self.field
    }

    /// The declared value type.
    pub const fn field_type(&self) -> &DomainType {
        &self.field_type
    }

    /// Whether the field is mandatory.
    pub const fn required(&self) -> bool {
        self.required
    }

    /// The declared visibility.
    pub const fn visibility(&self) -> Visibility {
        self.visibility
    }
}

/// One declared domain entity. The aggregate role is explicit: either
/// the entity is a root, or it names the root it belongs to, or it is
/// standalone (no role members at all).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainEntity {
    pub(crate) entity_key: EntityKey,
    pub(crate) entity: SemanticId,
    pub(crate) external: bool,
    pub(crate) aggregate_root: bool,
    pub(crate) aggregate_owner: Option<EntityKey>,
    pub(crate) visibility: Visibility,
    pub(crate) fields: Vec<DomainField>,
    pub(crate) invariants: Vec<NamespacedId>,
    pub(crate) state_spaces: Vec<NamespacedId>,
    pub(crate) description: Option<String>,
}

impl DomainEntity {
    /// The stable entity key.
    pub fn entity_key(&self) -> &EntityKey {
        &self.entity_key
    }

    /// The referenced Model symbol id.
    pub fn entity(&self) -> &SemanticId {
        &self.entity
    }

    /// Whether the entity is external or remote and never stored
    /// locally.
    pub const fn external(&self) -> bool {
        self.external
    }

    /// Whether the entity is an aggregate root.
    pub const fn aggregate_root(&self) -> bool {
        self.aggregate_root
    }

    /// The declared aggregate root, when the entity belongs to one.
    pub fn aggregate_owner(&self) -> Option<&EntityKey> {
        self.aggregate_owner.as_ref()
    }

    /// The declared entity visibility.
    pub const fn visibility(&self) -> Visibility {
        self.visibility
    }

    /// The declared domain fields, in canonical (byte-sorted) order.
    pub fn fields(&self) -> &[DomainField] {
        &self.fields
    }

    /// The opaque #63 invariant references.
    pub fn invariants(&self) -> &[NamespacedId] {
        &self.invariants
    }

    /// The opaque #63 state-space references.
    pub fn state_spaces(&self) -> &[NamespacedId] {
        &self.state_spaces
    }

    /// The bounded description, when declared.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}
