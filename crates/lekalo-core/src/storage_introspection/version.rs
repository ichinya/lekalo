//! Storage-introspection contract identity and hard denial limits
//! (issue #117).

/// The storage-introspection contract family identifier.
pub const FAMILY: &str = "dev.lekalo.storage-introspection";

/// The exact storage-introspection contract version.
pub const VERSION: &str = "0.4.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.storage-introspection@0.4.0";

/// The exact wire discriminator of the storage-introspection contract.
pub const SCHEMA_VERSION: &str = "lekalo/storage-introspection/v0.4.0";

/// The maximum number of observed tables one evidence document may
/// carry.
pub const MAX_TABLES: usize = 512;

/// The maximum number of observed columns one table may carry.
pub const MAX_COLUMNS: usize = 256;

/// The maximum number of observed indexes one table may carry.
pub const MAX_INDEXES: usize = 64;

/// The maximum number of observed foreign keys one table may carry.
pub const MAX_FOREIGN_KEYS: usize = 64;

/// The maximum number of observed index key parts.
pub const MAX_INDEX_COLUMNS: usize = 16;

/// The maximum number of sql-mode echo tokens.
pub const MAX_SQL_MODE_TOKENS: usize = 16;

/// The maximum byte length of one bounded observed token.
pub const MAX_TOKEN_BYTES: usize = 64;

/// The maximum byte length of one bounded evidence reference.
pub const MAX_EVIDENCE_REF_BYTES: usize = 256;

/// The maximum size in bytes of one canonical evidence payload.
pub const MAX_CANONICAL_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(SCHEMA_VERSION, "lekalo/storage-introspection/v0.4.0");
    }
}
