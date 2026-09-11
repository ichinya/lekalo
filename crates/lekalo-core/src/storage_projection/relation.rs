//! Semantic relations of the storage-projection attachment (issue
//! #65).
//!
//! A relation declares target-neutral semantics only: closed kind,
//! explicit cardinality, and explicit delete behavior. Nothing here
//! carries Eloquent, SQL, or target syntax; the storage consequences
//! are derived by the published projection rules, never invented.

use crate::scenario::id::{NamespacedId, SemanticId};

use super::id::{EntityKey, StorageName};

/// Why one textual relation record is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeError {
    /// The record or one of its members is outside the closed grammar.
    Shape,
}

/// The closed relation kinds (issue #65).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RelationKind {
    /// Exactly one target per owner and one owner per target.
    OneToOne,
    /// Many targets per owner; the target row carries the foreign key.
    OneToMany,
    /// Many targets per owner joined through an explicit join table.
    ManyToMany,
    /// A reference to an external or remote identity; never stored
    /// locally and never projected.
    ExternalReference,
    /// A child owned by the owner's aggregate.
    AggregateChild,
    /// An optional reference-only association; the owner row carries
    /// the nullable foreign key.
    OptionalReference,
    /// A polymorphic association. Declared only as an explicit target
    /// capability: a projection materializes it only where it opts in
    /// with declared discriminator columns.
    Polymorphic,
}

impl RelationKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::OneToOne => "one_to_one",
            Self::OneToMany => "one_to_many",
            Self::ManyToMany => "many_to_many",
            Self::ExternalReference => "external_reference",
            Self::AggregateChild => "aggregate_child",
            Self::OptionalReference => "optional_reference",
            Self::Polymorphic => "polymorphic",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        match text {
            "one_to_one" => Ok(Self::OneToOne),
            "one_to_many" => Ok(Self::OneToMany),
            "many_to_many" => Ok(Self::ManyToMany),
            "external_reference" => Ok(Self::ExternalReference),
            "aggregate_child" => Ok(Self::AggregateChild),
            "optional_reference" => Ok(Self::OptionalReference),
            "polymorphic" => Ok(Self::Polymorphic),
            _ => Err(ShapeError::Shape),
        }
    }

    /// Whether the kind materializes one foreign-key column on the
    /// target table.
    pub const fn target_foreign_key(self) -> bool {
        matches!(
            self,
            Self::OneToOne | Self::OneToMany | Self::AggregateChild
        )
    }

    /// Whether the kind declares a foreign-key column on the owner
    /// table.
    pub const fn owner_foreign_key(self) -> bool {
        matches!(self, Self::OptionalReference)
    }

    /// Whether the kind declares a foreign-key column at all.
    pub const fn declares_foreign_key(self) -> bool {
        self.target_foreign_key() || self.owner_foreign_key()
    }
}

/// The closed explicit delete behavior. There is no default: every
/// relation names one.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DeleteBehavior {
    /// Deleting the owner deletes the targets.
    Cascade,
    /// Deleting the owner is forbidden while targets exist.
    Restrict,
    /// Deleting the owner severs the relation; both sides survive.
    Detach,
}

impl DeleteBehavior {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Cascade => "cascade",
            Self::Restrict => "restrict",
            Self::Detach => "detach",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        match text {
            "cascade" => Ok(Self::Cascade),
            "restrict" => Ok(Self::Restrict),
            "detach" => Ok(Self::Detach),
            _ => Err(ShapeError::Shape),
        }
    }
}

/// One pinned Scenario IR reference covering a relation.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ScenarioRef {
    pub(crate) scenario_id: String,
    pub(crate) scenario_version: String,
    pub(crate) ir_digest: crate::lockfile::types::Sha256Digest,
}

impl ScenarioRef {
    /// The scenario identifier.
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    /// The pinned scenario version.
    pub fn scenario_version(&self) -> &str {
        &self.scenario_version
    }

    /// The pinned Scenario IR digest.
    pub fn ir_digest(&self) -> &crate::lockfile::types::Sha256Digest {
        &self.ir_digest
    }
}

/// One declared semantic relation. `min`/`max` bound the number of
/// targets per owner; `foreignKey` names the materialized column for
/// the kinds that declare one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Relation {
    pub(crate) relation_id: SemanticId,
    pub(crate) owner: EntityKey,
    pub(crate) target: EntityKey,
    pub(crate) kind: RelationKind,
    pub(crate) min: i64,
    pub(crate) max: i64,
    pub(crate) delete_behavior: DeleteBehavior,
    pub(crate) foreign_key: Option<StorageName>,
    pub(crate) scenarios: Vec<ScenarioRef>,
    pub(crate) constraints: Vec<NamespacedId>,
    pub(crate) description: Option<String>,
}

impl Relation {
    /// The stable relation identifier.
    pub fn relation_id(&self) -> &SemanticId {
        &self.relation_id
    }

    /// The owning entity key.
    pub fn owner(&self) -> &EntityKey {
        &self.owner
    }

    /// The target entity key.
    pub fn target(&self) -> &EntityKey {
        &self.target
    }

    /// The closed relation kind.
    pub const fn kind(&self) -> RelationKind {
        self.kind
    }

    /// The inclusive minimum number of targets per owner.
    pub const fn min(&self) -> i64 {
        self.min
    }

    /// The inclusive maximum number of targets per owner.
    pub const fn max(&self) -> i64 {
        self.max
    }

    /// The explicit delete behavior.
    pub const fn delete_behavior(&self) -> DeleteBehavior {
        self.delete_behavior
    }

    /// The declared foreign-key column, for the kinds that declare
    /// one.
    pub fn foreign_key(&self) -> Option<&StorageName> {
        self.foreign_key.as_ref()
    }

    /// The pinned scenario references, canonical (byte-sorted) order.
    pub fn scenarios(&self) -> &[ScenarioRef] {
        &self.scenarios
    }

    /// The opaque #63 constraint references, canonical order.
    pub fn constraints(&self) -> &[NamespacedId] {
        &self.constraints
    }

    /// The bounded description, when declared.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}
