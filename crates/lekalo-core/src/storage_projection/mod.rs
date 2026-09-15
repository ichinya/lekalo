//! Issue #65: the closed, versioned storage-projection attachment.
//!
//! One independent, immutable contract family separating the
//! target-neutral domain model from target-namespaced storage
//! projections: domain entities with stable keys independent of both
//! the Model symbol id and every table name, closed value types,
//! aggregate ownership, visibility, opaque invariant and state-space
//! references, and semantic relations with closed kinds, explicit
//! cardinality, explicit delete behavior, and mandatory scenario or
//! constraint coverage; storage projections per closed namespace with
//! explicit tables, technical and generated columns, soft-delete and
//! tenant policies, audit timestamps, indexes, join tables, explicit
//! polymorphic materializations, and migration history with visible
//! data risk.
//!
//! Boundaries: this module is pure declaration, validation, deriva-
//! tion, and comparison — no runtime storage, no adapter execution,
//! no SQL generation, no ORM behavior, no transaction semantics (#24),
//! no scenario execution (#23), and no report, trace-manifest, or
//! gate surface. Domain and storage changes classify separately in
//! [`compare`]; a wire-envelope change never masquerades as either.
//!
//! Determinism and denial: canonical bytes are compact UTF-8 JSON with
//! byte-sorted keys; set-like collections normalize to sorted form.
//! Every bound and every semantic contradiction rejects with an
//! explicit registered diagnostic and no partial result.

pub mod canonical;
pub mod derivation;
mod diagnostic;
pub mod diff;
pub mod entity;
pub mod id;
pub mod projection;
pub mod relation;
mod validate;
pub(crate) mod version;
pub(crate) mod wire;

pub use derivation::{
    project, public_fields, ColumnOrigin, DerivedColumn, DerivedForeignKey, DerivedJoin,
    DerivedPolymorphic, DerivedProjection, DerivedTable, OnDelete,
};
pub use diff::{compare, DiffClass, DiffLayer, DiffPath, DiffResult};
pub use entity::{DomainEntity, DomainField, DomainType, Visibility};
pub use id::{EntityKey, StorageName};
pub use projection::{
    DataRisk, GeneratedColumn, GeneratedKind, Index, Join, Migration, Namespace, Polymorphic,
    Projection, StorageType, Table, TechnicalColumn,
};
pub use relation::{DeleteBehavior, Relation, RelationKind, ScenarioRef};
pub use version::{FAMILY, IDENTITY, SCHEMA_VERSION, VERSION};

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{NamespacedId, SemanticId};

/// The bound source Model contract: exact accepted version plus digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelPin {
    /// The accepted Model contract version.
    pub(crate) version: crate::scenario::ModelPin,
    /// The digest of the exact source Model document.
    pub(crate) digest: Sha256Digest,
}

impl ModelPin {
    /// The accepted Model contract version.
    pub const fn version(&self) -> &crate::scenario::ModelPin {
        &self.version
    }

    /// The digest of the exact source Model document.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// One finished storage-projection attachment: immutable,
/// deterministically ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageProjectionAttachment {
    attachment_revision: SemVer,
    project_id: SemanticId,
    model_ref: ModelPin,
    ir_digest: Sha256Digest,
    source_map_ref: Option<NamespacedId>,
    entities: Vec<DomainEntity>,
    relations: Vec<Relation>,
    projections: Vec<Projection>,
}

impl StorageProjectionAttachment {
    /// Assemble from validated parts (crate internal); collections are
    /// stored in the caller's normalized order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        attachment_revision: SemVer,
        project_id: SemanticId,
        model_ref: ModelPin,
        ir_digest: Sha256Digest,
        source_map_ref: Option<NamespacedId>,
        entities: Vec<DomainEntity>,
        relations: Vec<Relation>,
        projections: Vec<Projection>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_digest,
            source_map_ref,
            entities,
            relations,
            projections,
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

    /// The optional bound source-map reference.
    pub fn source_map_ref(&self) -> Option<&NamespacedId> {
        self.source_map_ref.as_ref()
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
    pub const fn model_ref(&self) -> &ModelPin {
        &self.model_ref
    }

    /// The digest of the exact canonical IR payload.
    pub fn ir_digest(&self) -> &Sha256Digest {
        &self.ir_digest
    }

    /// Every declared domain entity, canonical (byte-sorted) order.
    pub fn entities(&self) -> &[DomainEntity] {
        &self.entities
    }

    /// Every declared relation, canonical (byte-sorted) order.
    pub fn relations(&self) -> &[Relation] {
        &self.relations
    }

    /// Every declared projection, canonical (namespace) order.
    pub fn projections(&self) -> &[Projection] {
        &self.projections
    }

    /// Resolve one entity by key.
    pub fn entity(&self, key: &EntityKey) -> Option<&DomainEntity> {
        self.entities
            .iter()
            .find(|entity| entity.entity_key == *key)
    }

    /// Resolve one projection by namespace.
    pub fn projection(&self, namespace: Namespace) -> Option<&Projection> {
        self.projections
            .iter()
            .find(|projection| projection.namespace == namespace)
    }
}
