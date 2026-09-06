//! Issue #24 transaction-concurrency contract identity and hard denial
//! limits.
//!
//! The transaction-concurrency contract is its own family: independent of
//! the product release, of the source Model and IR versions, of the
//! effect graph, of the Scenario IR, of the error-contract family, and of
//! the diagnostic registry. The limits below are the owner-approved v1
//! constants (ADR-0020); every normalization rejects with an explicit
//! registered diagnostic instead of truncating by arrival order.

/// The transaction-concurrency contract family identifier.
pub const FAMILY: &str = "dev.lekalo.transaction-concurrency";

/// The exact transaction-concurrency contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.transaction-concurrency@1.0.0";

/// The exact wire discriminator of the transaction-concurrency contract.
pub const SCHEMA_VERSION: &str = "lekalo/transaction-concurrency/v1.0.0";

/// The maximum number of operation contracts one attachment may carry.
pub const MAX_OPERATIONS: usize = 10_000;

/// The maximum number of atomic effect groups one operation (and one
/// attachment) may carry.
pub const MAX_GROUPS: usize = 1_024;

/// The maximum number of effect references one atomic group may carry.
pub const MAX_EFFECTS_PER_GROUP: usize = 256;

/// The maximum number of preconditions one operation may carry.
pub const MAX_PRECONDITIONS: usize = 256;

/// The maximum number of failure boundaries one operation may carry.
pub const MAX_FAILURE_BOUNDARIES: usize = 512;

/// The maximum number of invariants one attachment may carry.
pub const MAX_INVARIANTS: usize = 256;

/// The maximum number of concurrency cases one attachment may carry.
pub const MAX_CASES: usize = 512;

/// The maximum number of participants (and invocations, and expected
/// outcomes) one case may carry.
pub const MAX_PARTICIPANTS: usize = 64;

/// The maximum number of schedule nodes one case may carry.
pub const MAX_SCHEDULE_NODES: usize = 1_024;

/// The maximum number of barriers one case may carry.
pub const MAX_BARRIERS: usize = 128;

/// The maximum number of distinct lock resources one attachment may
/// declare.
pub const MAX_LOCK_RESOURCES: usize = 128;

/// The maximum number of key fields one invariant may carry.
pub const MAX_KEY_FIELDS: usize = 64;

/// The maximum number of capability-requirement records one attachment
/// may carry (owner-approved v1 addition to the brief's per-reference
/// bound).
pub const MAX_CAPABILITY_REQUIREMENTS: usize = 256;

/// The maximum number of capability-requirement references one
/// operation or case may carry.
pub const MAX_CAPABILITY_REFS: usize = 32;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.transaction-concurrency");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/transaction-concurrency/v1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_OPERATIONS, 10_000);
        assert_eq!(MAX_GROUPS, 1_024);
        assert_eq!(MAX_EFFECTS_PER_GROUP, 256);
        assert_eq!(MAX_PRECONDITIONS, 256);
        assert_eq!(MAX_FAILURE_BOUNDARIES, 512);
        assert_eq!(MAX_INVARIANTS, 256);
        assert_eq!(MAX_CASES, 512);
        assert_eq!(MAX_PARTICIPANTS, 64);
        assert_eq!(MAX_SCHEDULE_NODES, 1_024);
        assert_eq!(MAX_BARRIERS, 128);
        assert_eq!(MAX_LOCK_RESOURCES, 128);
        assert_eq!(MAX_KEY_FIELDS, 64);
        assert_eq!(MAX_CAPABILITY_REQUIREMENTS, 256);
        assert_eq!(MAX_CAPABILITY_REFS, 32);
        assert_eq!(MAX_CANONICAL_BYTES, 32 * 1024 * 1024);
    }
}
