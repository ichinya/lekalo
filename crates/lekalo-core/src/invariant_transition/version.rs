//! Issue #63 invariant-transition contract identity and hard denial limits.
//!
//! The invariant-transition contract is its own family: independent of
//! the product release, of the source Model and IR versions, of the
//! Scenario IR, of the error-contract and authorization families, and
//! of the diagnostic registry. The limits below are the owner-approved
//! v1 constants (ADR-0024); every normalization rejects with an
//! explicit registered diagnostic instead of truncating by arrival
//! order.

/// The invariant-transition contract family identifier.
pub const FAMILY: &str = "dev.lekalo.invariant-transition";

/// The exact invariant-transition contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.invariant-transition@1.0.0";

/// The exact wire discriminator of the invariant-transition contract.
pub const SCHEMA_VERSION: &str = "lekalo/invariant-transition/v1.0.0";

/// The maximum number of state spaces one attachment may carry.
pub const MAX_STATE_SPACES: usize = 256;

/// The maximum number of states one state space may carry.
pub const MAX_STATES: usize = 256;

/// The maximum number of invariants one attachment may carry.
pub const MAX_INVARIANTS: usize = 10_000;

/// The maximum number of transitions one attachment may carry.
pub const MAX_TRANSITIONS: usize = 10_000;

/// The maximum number of verification mappings one attachment may
/// carry.
pub const MAX_MAPPINGS: usize = 10_000;

/// The maximum number of property hints one attachment may carry.
pub const MAX_PROPERTY_HINTS: usize = 256;

/// The maximum depth of one predicate AST.
pub const MAX_PREDICATE_DEPTH: usize = 8;

/// The maximum number of operands one logical predicate node may
/// carry.
pub const MAX_PREDICATE_OPERANDS: usize = 16;

/// The maximum number of members one list or set value may carry.
pub const MAX_VALUE_ITEMS: usize = 64;

/// The maximum number of members one object value may carry.
pub const MAX_VALUE_ENTRIES: usize = 32;

/// The maximum size in bytes of one string or decimal literal.
pub const MAX_LITERAL_BYTES: usize = 256;

/// The maximum number of key or member fields one invariant may bind.
pub const MAX_INVARIANT_FIELDS: usize = 64;

/// The maximum number of from-states one transition may declare.
pub const MAX_FROM_STATES: usize = 64;

/// The maximum number of assignments one transition may declare.
pub const MAX_ASSIGNMENTS: usize = 64;

/// The maximum number of preconditions one transition may declare.
pub const MAX_PRECONDITIONS: usize = 16;

/// The maximum number of typed error references one record may carry.
pub const MAX_ERROR_REFS: usize = 64;

/// The maximum number of requirement references one record may carry.
pub const MAX_REQUIREMENT_REFS: usize = 32;

/// The maximum number of scenario references one record may carry.
pub const MAX_SCENARIO_REFS: usize = 32;

/// The maximum number of capability references one record may carry.
pub const MAX_CAPABILITY_REFS: usize = 32;

/// The maximum number of states one member-of-set allowlist may name.
pub const MAX_ALLOWED_STATES: usize = 256;

/// The maximum inclusive cardinality bound.
pub const MAX_CARDINALITY: i64 = 1_000_000;

/// The maximum magnitude of one integer literal.
pub const MAX_INTEGER: i64 = 9_007_199_254_740_991;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(SCHEMA_VERSION, "lekalo/invariant-transition/v1.0.0");
        assert_eq!(FAMILY, "dev.lekalo.invariant-transition");
        assert_eq!(VERSION, "1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_STATE_SPACES, 256);
        assert_eq!(MAX_STATES, 256);
        assert_eq!(MAX_INVARIANTS, 10_000);
        assert_eq!(MAX_TRANSITIONS, 10_000);
        assert_eq!(MAX_MAPPINGS, 10_000);
        assert_eq!(MAX_PROPERTY_HINTS, 256);
        assert_eq!(MAX_PREDICATE_DEPTH, 8);
        assert_eq!(MAX_PREDICATE_OPERANDS, 16);
        assert_eq!(MAX_VALUE_ITEMS, 64);
        assert_eq!(MAX_VALUE_ENTRIES, 32);
        assert_eq!(MAX_LITERAL_BYTES, 256);
        assert_eq!(MAX_INVARIANT_FIELDS, 64);
        assert_eq!(MAX_FROM_STATES, 64);
        assert_eq!(MAX_ASSIGNMENTS, 64);
        assert_eq!(MAX_PRECONDITIONS, 16);
        assert_eq!(MAX_ERROR_REFS, 64);
        assert_eq!(MAX_REQUIREMENT_REFS, 32);
        assert_eq!(MAX_SCENARIO_REFS, 32);
        assert_eq!(MAX_CAPABILITY_REFS, 32);
        assert_eq!(MAX_ALLOWED_STATES, 256);
        assert_eq!(MAX_CARDINALITY, 1_000_000);
        assert_eq!(MAX_INTEGER, 9_007_199_254_740_991);
        assert_eq!(MAX_CANONICAL_BYTES, 32 * 1024 * 1024);
    }
}
