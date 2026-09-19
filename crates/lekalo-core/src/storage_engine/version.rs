//! Issue #69 storage-engine contract identity and hard denial limits.
//!
//! The storage-engine contract is its own family: independent of the
//! product release, of the source Model and IR versions, of the
//! storage-projection family it binds to by digest, and of the
//! diagnostic registry. The limits below are the owner-approved v1
//! constants (ADR-0042); every normalization rejects with an explicit
//! registered diagnostic instead of truncating by arrival order.

/// The storage-engine contract family identifier.
pub const FAMILY: &str = "dev.lekalo.storage-engine";

/// The exact storage-engine contract version.
pub const VERSION: &str = "0.4.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.storage-engine@0.4.0";

/// The exact wire discriminator of the storage-engine contract.
pub const SCHEMA_VERSION: &str = "lekalo/storage-engine/v0.4.0";

/// The maximum number of schema scopes one introspection declaration
/// may carry.
pub const MAX_SCOPES: usize = 8;

/// The maximum number of extensions one profile allow-list may carry.
pub const MAX_EXTENSIONS: usize = 16;

/// The maximum number of names in the connection-name and
/// session-variable grammars.
pub const MAX_NAME_BYTES: usize = 63;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(SCHEMA_VERSION, "lekalo/storage-engine/v0.4.0");
    }
}
