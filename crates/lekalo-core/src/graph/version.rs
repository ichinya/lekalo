//! Issue #13 graph contract identity and hard denial limits.
//!
//! The graph contract version is its own family: independent of the product
//! release, of the source Model versions, of the IR contract, and of the
//! diagnostic registry. The limits below are the owner-approved v1 constants
//! (ADR-0012): every traversal, slice, path, and export rejects with an
//! explicit registered diagnostic instead of truncating by arrival order.

/// The graph contract family identifier.
pub const FAMILY: &str = "dev.lekalo.graph";

/// The exact graph contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.graph@1.0.0";

/// The exact wire discriminator of the graph contract.
pub const SCHEMA_VERSION: &str = "lekalo/graph/v1.0.0";

/// The maximum number of nodes one graph may hold (fatal beyond it).
pub const MAX_NODES: usize = 100_000;

/// The maximum number of direct edges one graph may hold. Transitive
/// edges are never materialized, so this bounds the whole edge list.
pub const MAX_EDGES: usize = 1_000_000;

/// The maximum depth of one traversal, path, or slice.
pub const MAX_DEPTH: usize = 256;

/// The maximum number of nodes one query may return.
pub const MAX_RESULT_NODES: usize = 50_000;

/// The maximum number of edges one query may return.
pub const MAX_RESULT_EDGES: usize = 250_000;

/// The maximum number of nodes one shortest path may contain.
pub const MAX_PATH_NODES: usize = 256;

/// The maximum size in bytes of one canonical export.
pub const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

/// The maximum number of filter terms (relations, kinds, modules) one
/// filter may carry.
pub const MAX_FILTER_TERMS: usize = 128;

/// The maximum number of provenance records one edge may carry
/// (the bounded `parentEdgeKeys` chain of derived provenance).
pub const MAX_PROVENANCE_RECORDS: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.graph");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/graph/v1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_NODES, 100_000);
        assert_eq!(MAX_EDGES, 1_000_000);
        assert_eq!(MAX_DEPTH, 256);
        assert_eq!(MAX_RESULT_NODES, 50_000);
        assert_eq!(MAX_RESULT_EDGES, 250_000);
        assert_eq!(MAX_PATH_NODES, 256);
        assert_eq!(MAX_EXPORT_BYTES, 32 * 1024 * 1024);
        assert_eq!(MAX_FILTER_TERMS, 128);
        assert_eq!(MAX_PROVENANCE_RECORDS, 8);
    }
}
