//! Storage-engine-profile contract identity and hard denial limits
//! (issue #117).
//!
//! The storage-engine-profile contract is its own family: independent
//! of the product release, of the source Model and IR versions, of the
//! storage-projection family it binds beside, and of the diagnostic
//! registry. Every normalization rejects with an explicit registered
//! diagnostic instead of truncating by arrival order.

/// The storage-engine-profile contract family identifier.
pub const FAMILY: &str = "dev.lekalo.storage-engine-profile";

/// The exact storage-engine-profile contract version.
pub const VERSION: &str = "0.4.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.storage-engine-profile@0.4.0";

/// The exact wire discriminator of the storage-engine-profile contract.
pub const SCHEMA_VERSION: &str = "lekalo/storage-engine-profile/v0.4.0";

/// The maximum number of capability records one profile may carry.
pub const MAX_CAPABILITIES: usize = 64;

/// The maximum number of adapter evidence records one profile may
/// carry.
pub const MAX_ADAPTERS: usize = 16;

/// The maximum byte length of one bounded declaration token.
pub const MAX_TOKEN_BYTES: usize = 64;

/// The maximum byte length of one bounded evidence reference.
pub const MAX_EVIDENCE_REF_BYTES: usize = 256;

/// The maximum inclusive sql-mode token list length.
pub const MAX_SQL_MODE_TOKENS: usize = 16;

/// The maximum byte length of the declared test-schema prefix.
pub const MAX_TEST_SCHEMA_PREFIX_BYTES: usize = 32;

/// The maximum size in bytes of one canonical profile payload.
pub const MAX_CANONICAL_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(SCHEMA_VERSION, "lekalo/storage-engine-profile/v0.4.0");
    }
}
