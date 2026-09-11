//! Issue #30 foreign/custom implementation contract identity and bounds.
//!
//! The implementation contract is its own family: independent of the
//! product release, of the source Model and IR versions, of the target
//! process protocol, of the target-profile family, and of the diagnostic
//! registry. The limits below are the owner-approved v1 constants
//! (ADR-0033); every normalization rejects with an explicit registered
//! diagnostic instead of truncating by arrival order.

/// The implementation contract family identifier.
pub const FAMILY: &str = "dev.lekalo.implementation";

/// The exact implementation contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.implementation@1.0.0";

/// The exact wire discriminator of the implementation contract.
pub const SCHEMA_VERSION: &str = "lekalo/implementation/v1.0.0";

/// The maximum number of hook contracts one attachment may carry.
pub const MAX_CONTRACTS: usize = 256;

/// The maximum number of target bindings one hook contract may carry.
pub const MAX_TARGETS: usize = 64;

/// The maximum number of acknowledged effect references one hook
/// contract may carry.
pub const MAX_EFFECT_REFS: usize = 64;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 1 << 20;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(SCHEMA_VERSION, "lekalo/implementation/v1.0.0");
        assert_eq!(FAMILY, "dev.lekalo.implementation");
        assert_eq!(VERSION, "1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_CONTRACTS, 256);
        assert_eq!(MAX_TARGETS, 64);
        assert_eq!(MAX_EFFECT_REFS, 64);
        assert_eq!(MAX_CANONICAL_BYTES, 1 << 20);
    }
}
