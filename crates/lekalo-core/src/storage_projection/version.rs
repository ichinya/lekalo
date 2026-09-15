//! Issue #65 storage-projection contract identity and hard denial
//! limits.
//!
//! The storage-projection contract is its own family: independent of
//! the product release, of the source Model and IR versions, of the
//! Scenario IR, of the error-contract, invariant-transition, and
//! authorization families, and of the diagnostic registry. The limits
//! below are the owner-approved v1 constants (ADR-0025); every
//! normalization rejects with an explicit registered diagnostic
//! instead of truncating by arrival order.

/// The storage-projection contract family identifier.
pub const FAMILY: &str = "dev.lekalo.storage-projection";

/// The exact storage-projection contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.storage-projection@1.0.0";

/// The exact wire discriminator of the storage-projection contract.
pub const SCHEMA_VERSION: &str = "lekalo/storage-projection/v1.0.0";

/// The maximum number of domain entities one attachment may carry.
pub const MAX_ENTITIES: usize = 512;

/// The maximum number of fields one entity may declare.
pub const MAX_FIELDS: usize = 256;

/// The maximum number of relations one attachment may carry.
pub const MAX_RELATIONS: usize = 2048;

/// The maximum number of storage projections one attachment may carry.
pub const MAX_PROJECTIONS: usize = 16;

/// The maximum number of tables one projection may carry.
pub const MAX_TABLES: usize = 512;

/// The maximum number of technical columns one table may declare.
pub const MAX_TECHNICAL_COLUMNS: usize = 64;

/// The maximum number of generated columns one table may declare.
pub const MAX_GENERATED_COLUMNS: usize = 64;

/// The maximum number of indexes one table may declare.
pub const MAX_INDEXES: usize = 64;

/// The maximum number of columns one index may span.
pub const MAX_INDEX_COLUMNS: usize = 16;

/// The maximum number of primary-key columns one table may declare.
pub const MAX_PRIMARY_KEY_COLUMNS: usize = 8;

/// The maximum number of join tables one projection may declare.
pub const MAX_JOINS: usize = 512;

/// The maximum number of polymorphic materializations one projection
/// may declare.
pub const MAX_POLYMORPHICS: usize = 256;

/// The maximum number of migration-history records one projection may
/// carry.
pub const MAX_MIGRATIONS: usize = 256;

/// The maximum number of tables one migration record may name.
pub const MAX_MIGRATION_TABLES: usize = 64;

/// The maximum number of invariant references one entity may carry.
pub const MAX_INVARIANT_REFS: usize = 64;

/// The maximum number of state-space references one entity may carry.
pub const MAX_STATE_SPACE_REFS: usize = 64;

/// The maximum number of scenario references one relation may carry.
pub const MAX_SCENARIO_REFS: usize = 32;

/// The maximum number of constraint references one relation may carry.
pub const MAX_CONSTRAINT_REFS: usize = 32;

/// The maximum inclusive cardinality bound.
pub const MAX_CARDINALITY: i64 = 1_000_000;

/// The maximum declared string length in Unicode scalar values.
pub const MAX_STRING_LENGTH: i64 = 4096;

/// The maximum declared decimal precision.
pub const MAX_DECIMAL_PRECISION: i64 = 38;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(SCHEMA_VERSION, "lekalo/storage-projection/v1.0.0");
    }
}
