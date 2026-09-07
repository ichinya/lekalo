//! Issue #26 extended-effects contract identity and hard denial limits.
//!
//! The extended-effects contract is its own family: independent of the
//! product release, of the source Model and IR versions, of the effect
//! graph, of the Scenario IR, of the error-contract family, and of the
//! diagnostic registry. The limits below are the owner-approved v1
//! constants (ADR-0023); every normalization rejects with an explicit
//! registered diagnostic instead of truncating by arrival order.

/// The extended-effects contract family identifier.
pub const FAMILY: &str = "dev.lekalo.extended-effects";

/// The exact extended-effects contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.extended-effects@1.0.0";

/// The exact wire discriminator of the extended-effects contract.
pub const SCHEMA_VERSION: &str = "lekalo/extended-effects/v1.0.0";

/// The maximum number of contracts one kind may carry in one attachment.
pub const MAX_CONTRACTS: usize = 10_000;

/// The maximum number of partial-failure cases one attachment may carry.
pub const MAX_CASES: usize = 512;

/// The maximum number of steps one case may carry.
pub const MAX_STEPS: usize = 64;

/// The maximum number of schedule nodes one case may carry.
pub const MAX_SCHEDULE_NODES: usize = 1_024;

/// The maximum number of joins one schedule node may carry.
pub const MAX_JOINS: usize = 64;

/// The maximum number of expected outcomes one case may carry.
pub const MAX_OUTCOMES: usize = 64;

/// The maximum number of typed error references one external-call
/// contract may carry.
pub const MAX_ERROR_REFS: usize = 64;

/// The maximum number of key fields one cache key contract may carry.
pub const MAX_KEY_FIELDS: usize = 64;

/// The maximum number of capability-requirement records one attachment
/// may carry.
pub const MAX_CAPABILITY_REQUIREMENTS: usize = 256;

/// The maximum number of capability-requirement references one contract
/// or case may carry.
pub const MAX_CAPABILITY_REFS: usize = 32;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(SCHEMA_VERSION, "lekalo/extended-effects/v1.0.0");
        assert_eq!(FAMILY, "dev.lekalo.extended-effects");
        assert_eq!(VERSION, "1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_CONTRACTS, 10_000);
        assert_eq!(MAX_CASES, 512);
        assert_eq!(MAX_STEPS, 64);
        assert_eq!(MAX_SCHEDULE_NODES, 1_024);
        assert_eq!(MAX_JOINS, 64);
        assert_eq!(MAX_OUTCOMES, 64);
        assert_eq!(MAX_ERROR_REFS, 64);
        assert_eq!(MAX_KEY_FIELDS, 64);
        assert_eq!(MAX_CAPABILITY_REQUIREMENTS, 256);
        assert_eq!(MAX_CAPABILITY_REFS, 32);
        assert_eq!(MAX_CANONICAL_BYTES, 32 * 1024 * 1024);
    }
}
