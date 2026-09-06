//! Issue #17 context-capsule contract identity and hard limits.
//!
//! The context contract version is its own family: independent of the
//! product release, of the source Model versions, of the IR/graph/effect
//! contract versions, and of the diagnostic registry. The limits below are
//! the owner-approved v1 constants (ADR-0018): every bound rejects with an
//! explicit registered diagnostic or an explicit in-band truncation record
//! instead of truncating by arrival order.

/// The context contract family identifier.
pub const FAMILY: &str = "dev.lekalo.context";

/// The exact context contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.context@1.0.0";

/// The exact wire discriminator of the context contract.
pub const SCHEMA_VERSION: &str = "lekalo/context/v1.0.0";

/// The maximum token budget one capsule request may carry (fatal beyond it).
pub const MAX_BUDGET_TOKENS: u64 = 1_000_000;

/// The maximum number of roots one capsule may carry (fatal beyond it).
pub const MAX_ROOTS: usize = 128;

/// The maximum number of manifest rows one capsule may carry (fatal
/// beyond it; a capsule rejects instead of silently dropping rows).
pub const MAX_MANIFEST_ITEMS: usize = 50_000;

/// The maximum depth of the supporting closure walk beyond the direct
/// dependencies (bounded, and reported through the `closure-bounded` gap
/// when it stops early).
pub const MAX_CLOSURE_DEPTH: usize = 2;

/// The maximum number of nodes the supporting closure may visit.
pub const MAX_CLOSURE_NODES: usize = 2_000;

/// The maximum number of edges the supporting closure may collect.
pub const MAX_CLOSURE_EDGES: usize = 10_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.context");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/context/v1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_BUDGET_TOKENS, 1_000_000);
        assert_eq!(MAX_ROOTS, 128);
        assert_eq!(MAX_MANIFEST_ITEMS, 50_000);
        assert_eq!(MAX_CLOSURE_DEPTH, 2);
        assert_eq!(MAX_CLOSURE_NODES, 2_000);
        assert_eq!(MAX_CLOSURE_EDGES, 10_000);
    }
}
