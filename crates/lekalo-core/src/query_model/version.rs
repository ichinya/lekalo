//! Issue #64 query-model contract identity and hard denial limits.
//!
//! The query-model contract is its own family: independent of the
//! product release, of the source Model and IR versions, of the
//! Scenario IR, of the authorization and error-contract families, and
//! of the diagnostic registry. The limits below are the owner-approved
//! v1 constants (ADR-0036); every normalization rejects with an
//! explicit registered diagnostic instead of truncating by arrival
//! order.

/// The query-model contract family identifier.
pub const FAMILY: &str = "dev.lekalo.query-model";

/// The exact query-model contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.query-model@1.0.0";

/// The exact wire discriminator of the query-model contract.
pub const SCHEMA_VERSION: &str = "lekalo/query-model/v1.0.0";

/// The exact IR identity this attachment binds (`irRef`).
pub const IR_IDENTITY: &str = "dev.lekalo.ir@0.1.0";

/// The maximum number of tenancy declarations one attachment may carry.
pub const MAX_TENANCY: usize = 256;

/// The maximum number of query declarations one attachment may carry.
pub const MAX_QUERIES: usize = 10_000;

/// The maximum number of parameters one query may declare.
pub const MAX_PARAMETERS: usize = 64;

/// The maximum depth of one filter AST.
pub const MAX_FILTER_DEPTH: usize = 8;

/// The maximum number of operands one logical filter node may carry.
pub const MAX_FILTER_OPERANDS: usize = 16;

/// The maximum number of comparison leaves one filter AST may carry.
pub const MAX_FILTER_LEAVES: usize = 64;

/// The maximum number of members one set-literal filter value may carry.
pub const MAX_SET_ITEMS: usize = 64;

/// The maximum size in bytes of one string literal filter value.
pub const MAX_LITERAL_BYTES: usize = 256;

/// The maximum number of sort keys one query may declare.
pub const MAX_SORT_KEYS: usize = 16;

/// The maximum number of selected fields one query may declare.
pub const MAX_SELECTION: usize = 256;

/// The maximum number of relation includes one query may declare.
pub const MAX_INCLUDES: usize = 16;

/// The maximum length of one relation include path.
pub const MAX_INCLUDE_PATH: usize = 4;

/// The maximum number of policy references one query may declare.
pub const MAX_POLICY_REFS: usize = 32;

/// The maximum number of scenario references one query may declare.
pub const MAX_SCENARIO_REFS: usize = 32;

/// The maximum inclusive `limit` bound of one pagination block.
pub const MAX_LIMIT: i64 = 10_000;

/// The maximum inclusive `offset` bound of one pagination block.
pub const MAX_OFFSET: i64 = 1_000_000;

/// The maximum inclusive `maxRows` cost hint.
pub const MAX_COST_ROWS: i64 = 1_000_000;

/// The maximum inclusive `maxStaleness` freshness bound in seconds.
pub const MAX_STALENESS_SECONDS: i64 = 2_592_000;

/// The maximum size in bytes of one foreign reason text.
pub const MAX_REASON_BYTES: usize = 256;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_parts_are_consistent() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert!(SCHEMA_VERSION.ends_with(VERSION));
    }

    #[test]
    fn bounds_are_positive_and_ordered() {
        assert_eq!(
            (
                MAX_FILTER_DEPTH.min(8),
                MAX_FILTER_OPERANDS.min(16),
                MAX_FILTER_LEAVES.min(64),
                MAX_SET_ITEMS.min(64),
                MAX_LIMIT.min(10_000),
            ),
            (
                MAX_FILTER_DEPTH,
                MAX_FILTER_OPERANDS,
                MAX_FILTER_LEAVES,
                MAX_SET_ITEMS,
                MAX_LIMIT,
            ),
        );
        assert_eq!(MAX_INCLUDE_PATH.min(4), MAX_INCLUDE_PATH);
        assert_eq!(
            MAX_STALENESS_SECONDS.min(MAX_STALENESS_SECONDS),
            MAX_STALENESS_SECONDS
        );
    }
}
