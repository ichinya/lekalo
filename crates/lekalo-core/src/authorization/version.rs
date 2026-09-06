//! Authorization contract version identity (issue #25).
//!
//! The authorization contract, its strict profile, and the product
//! release are independent version families by design: none of these
//! values ever borrows another family's version.

/// The contract family identity (version-free).
pub const FAMILY: &str = "dev.lekalo.authorization";

/// The exact contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity.
pub const IDENTITY: &str = "dev.lekalo.authorization@1.0.0";

/// The exact wire discriminator of the canonical authorization document.
pub const SCHEMA_VERSION: &str = "lekalo/authorization/v1.0.0";

/// The exact `$id` of the published wire schema.
pub const SCHEMA_ID: &str = "https://dev.lekalo/authorization.schema.v1.0.0.json";

/// The independent strict review profile family identity.
pub const PROFILE_IDENTITY: &str = "dev.lekalo.authorization-profile@1.0.0";

/// The canonical project-relative path of the authorization source.
pub const SOURCE_PATH: &str = "lekalo/authorization.yaml";

/// The closed v1 bounds (owner-approved; tested at N and N+1).
pub mod limits {
    /// Maximum declared policies per document.
    pub const POLICIES: usize = 256;
    /// Maximum declared capabilities or roles per document.
    pub const SYMBOL_DECLS: usize = 256;
    /// Maximum declared compositions per document.
    pub const COMPOSITIONS: usize = 256;
    /// Maximum adapter mapping evidence records per document.
    pub const MAPPINGS: usize = 256;
    /// Maximum referenced operations per policy.
    pub const APPLIES_TO: usize = 256;
    /// Maximum fields per permission side.
    pub const FIELDS: usize = 256;
    /// Maximum capability/role implications per declaration.
    pub const IMPLIES: usize = 64;
    /// Maximum condition nodes per predicate tree.
    pub const CONDITION_NODES: usize = 64;
    /// Maximum condition nesting depth.
    pub const CONDITION_DEPTH: usize = 4;
    /// Maximum composition nesting depth.
    pub const COMPOSITION_DEPTH: usize = 8;
    /// Maximum ownership predicates per policy.
    pub const OWNERSHIP: usize = 64;
    /// Maximum membership literals per `in` comparison.
    pub const IN_VALUES: usize = 64;
    /// Maximum reason references per mapping record.
    pub const REASON_REFS: usize = 16;
    /// Maximum source document bytes.
    pub const SOURCE_BYTES: usize = 1_048_576;
}
