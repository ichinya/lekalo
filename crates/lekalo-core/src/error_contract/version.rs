//! Issue #62: the error-contract family identity and hard rejection bounds.
//!
//! The error-contract and error-registry contracts are their own family:
//! independent of the product release, of the Model/IR/diagnostic/protocol
//! contract versions, and of every other registered family. The limits
//! below are the owner-approved v1 constants (ADR-0022): every oversized
//! input rejects with an explicit registered diagnostic instead of
//! truncating by arrival order.

/// The error-contract family identifier.
pub const FAMILY: &str = "dev.lekalo.error-contract";

/// The exact error-contract (operation binding) version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.error-contract@1.0.0";

/// The exact wire discriminator of the operation error binding.
pub const SCHEMA_VERSION: &str = "lekalo/error-contract/v1.0.0";

/// The error-registry family identifier.
pub const REGISTRY_FAMILY: &str = "dev.lekalo.error-registry";

/// The exact wire discriminator of the error registry.
pub const REGISTRY_SCHEMA_VERSION: &str = "lekalo/error-registry/v1.0.0";

/// The exact embedded error-registry identity.
pub const REGISTRY_IDENTITY: &str = "dev.lekalo.error-registry@1.0.0";

/// The current error-registry contract version.
pub const REGISTRY_VERSION: &str = "1.0.0";

/// The maximum number of error contracts one registry may declare.
pub const MAX_ERRORS: usize = 10_000;

/// The maximum number of operation bindings one registry may declare.
pub const MAX_BINDINGS: usize = 10_000;

/// The maximum number of tombstones one registry may carry.
pub const MAX_TOMBSTONES: usize = 10_000;

/// The maximum number of members one closed error union may declare.
pub const MAX_UNION_MEMBERS: usize = 256;

/// The maximum number of payload fields one error contract may declare.
pub const MAX_PAYLOAD_FIELDS: usize = 64;

/// The maximum number of payload values one public payload may carry.
pub const MAX_PAYLOAD_VALUES: usize = 64;

/// The maximum number of message templates one error contract may carry
/// (one public plus one private).
pub const MAX_MESSAGE_TEMPLATES: usize = 2;

/// The maximum number of coverage references one error contract may carry.
pub const MAX_COVERAGE_REFS: usize = 64;

/// The maximum byte length of one bounded text value (message template
/// ids, waiver owner/reason, invariants, payload text values).
pub const MAX_TEXT_BYTES: usize = 2000;

/// The maximum byte length of one bounded token (refs, target names).
pub const TOKEN_BYTES: usize = 256;

/// The maximum size in bytes of one canonical registry result.
pub const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.error-contract");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/error-contract/v1.0.0");
        assert_eq!(
            REGISTRY_IDENTITY,
            format!("{REGISTRY_FAMILY}@{REGISTRY_VERSION}")
        );
        assert_eq!(REGISTRY_FAMILY, "dev.lekalo.error-registry");
        assert_eq!(REGISTRY_SCHEMA_VERSION, "lekalo/error-registry/v1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_ERRORS, 10_000);
        assert_eq!(MAX_BINDINGS, 10_000);
        assert_eq!(MAX_TOMBSTONES, 10_000);
        assert_eq!(MAX_UNION_MEMBERS, 256);
        assert_eq!(MAX_PAYLOAD_FIELDS, 64);
        assert_eq!(MAX_PAYLOAD_VALUES, 64);
        assert_eq!(MAX_MESSAGE_TEMPLATES, 2);
        assert_eq!(MAX_COVERAGE_REFS, 64);
        assert_eq!(MAX_TEXT_BYTES, 2000);
        assert_eq!(TOKEN_BYTES, 256);
        assert_eq!(MAX_EXPORT_BYTES, 32 * 1024 * 1024);
    }
}
