//! Issue #16 impact contract identity and hard denial limits.
//!
//! The impact contract is its own family: independent of the product
//! release, of the source Model versions, of the IR, graph, and effect
//! contracts, and of the diagnostic registry. The limits below are the
//! owner-approved v1 constants (ADR-0017): every traversal, allocation,
//! and export rejects with an explicit registered diagnostic instead of
//! truncating by arrival order.

/// The impact contract family identifier.
pub const FAMILY: &str = "dev.lekalo.impact";

/// The exact impact contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.impact@1.0.0";

/// The exact wire discriminator of the impact contract.
pub const SCHEMA_VERSION: &str = "lekalo/impact/v1.0.0";

/// The exact identity of the deterministic v1 impact algorithm.
pub const ALGORITHM: &str = "dev.lekalo.impact.algorithm@1.0.0";

/// The default traversal depth (the depth the issue examples use).
pub const DEFAULT_DEPTH: u16 = 3;

/// The maximum traversal depth one request may ask for.
pub const MAX_DEPTH: u16 = 256;

/// The maximum number of affected items the radius sections may carry
/// together; exhaustion is a fatal `impact.traversal-limit`, never a
/// silent truncation.
pub const MAX_ITEMS: usize = 50_000;

/// The maximum number of edge references the explanation paths may carry
/// together.
pub const MAX_PATH_EDGES: usize = 250_000;

/// The maximum number of entries one changed-input set may carry.
pub const MAX_ENTRIES: usize = 50_000;

/// The maximum number of roots one request may name.
pub const MAX_ROOTS: usize = 128;

/// The maximum number of filter terms (relations, kinds, module, target)
/// one request may carry.
pub const MAX_FILTER_TERMS: usize = 128;

/// The maximum number of provenance references one explanation path
/// carries.
pub const MAX_PROVENANCE_RECORDS: usize = 8;

/// The maximum size in bytes of one canonical impact export.
pub const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.impact");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/impact/v1.0.0");
        assert_eq!(ALGORITHM, "dev.lekalo.impact.algorithm@1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(DEFAULT_DEPTH, 3);
        assert_eq!(MAX_DEPTH, 256);
        assert_eq!(MAX_ITEMS, 50_000);
        assert_eq!(MAX_PATH_EDGES, 250_000);
        assert_eq!(MAX_ENTRIES, 50_000);
        assert_eq!(MAX_ROOTS, 128);
        assert_eq!(MAX_FILTER_TERMS, 128);
        assert_eq!(MAX_PROVENANCE_RECORDS, 8);
        assert_eq!(MAX_EXPORT_BYTES, 32 * 1024 * 1024);
    }
}
