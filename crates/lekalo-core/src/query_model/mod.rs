//! Issue #64: the closed, versioned declarative query model.
//!
//! One independent, immutable contract family declaring the portable
//! read semantics of a project's queries: source entities, result
//! cardinality, declared parameters resolved from the query input, a
//! closed equality/range/null/set filter grammar with bounded
//! `and`/`or`/`not` combinators, deterministic sort with a mandatory
//! identity tie-breaker, one closed limit/offset/cursor pagination
//! contract for every target projection, semantic-link includes,
//! tenant-scoped entity declarations with a strict-profile tenant
//! filter requirement, consistency/freshness profiles, result
//! cardinality, read-cost hints, and the foreign/custom escape hatch.
//!
//! Boundaries: the attachment is pure declaration and validation —
//! no execution, no SQL or ORM generation, no adapter behavior, no
//! write surface of any kind. Parameter types, field references,
//! policy references, and scenario references resolve only against
//! the bound Model through [`resolve`]; nothing here reads the
//! filesystem or evaluates anything. Raw target SQL, Eloquent, and
//! ORM-specific syntax have no representation in the portable
//! section, and the foreign escape hatch is the only way to leave the
//! managed grammar.
//!
//! Determinism and denial: canonical bytes are compact UTF-8 JSON
//! with byte-sorted keys; set-like collections normalize to sorted
//! form while sort keys and projection order keep their declared
//! behavioral order. Every bound and every semantic contradiction
//! rejects with an explicit registered diagnostic and no partial
//! result.

pub mod canonical;
mod diagnostic;
pub mod diff;
pub mod facts;
pub mod filter;
pub mod id;
pub mod plan;
pub mod query;
mod resolve;
mod validate;
pub(crate) mod version;
pub(crate) mod wire;

pub use diff::{compare, DiffClass, DiffPath, DiffResult};
pub use facts::QueryGraphFacts;
pub use filter::{FilterExpr, FilterLeaf, FilterOp, FilterValue, Literal};
pub use id::ParameterName;
pub use plan::{PlanStep, QueryPlan};
pub use query::{
    Cardinality, Consistency, CostHint, Direction, ForeignRef, Include, Pagination,
    PaginationStrategy, QueryDecl, SelectionField, SortKey,
};
pub use resolve::{resolve, Resolution};
pub use version::{FAMILY, IDENTITY, IR_IDENTITY, MAX_CANONICAL_BYTES, SCHEMA_VERSION, VERSION};

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{FieldName, NamespacedId, SemanticId};

pub use diagnostic::io_failure;
pub use resolve::Profile;

/// The bound source Model contract: exact accepted version plus digest.
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

/// One tenant-scoped entity declaration: the entity and the field the
/// tenant filter must constrain.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TenancyDecl {
    /// The tenant-scoped entity.
    pub entity: SemanticId,
    /// The tenant key field.
    pub field: FieldName,
}

impl TenancyDecl {
    /// The exact tenant key field name.
    pub fn field(&self) -> &FieldName {
        &self.field
    }

    /// The tenant-scoped entity.
    pub fn entity(&self) -> &SemanticId {
        &self.entity
    }
}

/// One finished query-model attachment: immutable, deterministically
/// ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryModelAttachment {
    attachment_revision: SemVer,
    project_id: SemanticId,
    model_ref: ModelPin,
    ir_digest: Sha256Digest,
    source_map_ref: Option<NamespacedId>,
    tenancy: Vec<TenancyDecl>,
    queries: Vec<QueryDecl>,
}

impl QueryModelAttachment {
    /// Assemble from validated parts (crate internal); collections
    /// arrive in the caller's normalized order.
    pub(crate) fn assemble(
        attachment_revision: SemVer,
        project_id: SemanticId,
        model_ref: ModelPin,
        ir_digest: Sha256Digest,
        source_map_ref: Option<NamespacedId>,
        tenancy: Vec<TenancyDecl>,
        queries: Vec<QueryDecl>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_digest,
            source_map_ref,
            tenancy,
            queries,
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
    pub fn validate_queries(&self) -> Result<(), DiagnosticSet> {
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

    /// Every declared tenant-scoped entity, canonically ordered.
    pub fn tenancy(&self) -> &[TenancyDecl] {
        &self.tenancy
    }

    /// Every declared query, canonically ordered by query id.
    pub fn queries(&self) -> &[QueryDecl] {
        &self.queries
    }

    /// One declared query by id.
    pub fn query(&self, id: &str) -> Option<&QueryDecl> {
        self.queries.iter().find(|decl| decl.query.as_str() == id)
    }

    /// The tenant key field of one entity, when the entity is
    /// tenant-scoped.
    pub fn tenant_field(&self, entity: &str) -> Option<&FieldName> {
        self.tenancy
            .iter()
            .find(|decl| decl.entity.as_str() == entity)
            .map(|decl| &decl.field)
    }
}
