//! Issue #22 neutral trace-manifest contract identity and hard denial limits.
//!
//! The trace contract is its own family: independent of the product
//! release, of the Model/IR/graph/effects contract versions, of the
//! adapter/protocol/profile versions, and of the diagnostic registry. The
//! limits below are the owner-approved v1 constants (ADR-0014): every
//! construction, query, and export rejects with an explicit registered
//! diagnostic instead of truncating by arrival order.

/// The trace contract family identifier.
pub const FAMILY: &str = "dev.lekalo.trace-manifest";

/// The exact trace contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.trace-manifest@1.0.0";

/// The exact wire discriminator of the trace contract.
pub const SCHEMA_VERSION: &str = "lekalo/trace-manifest/v1.0.0";

/// The maximum number of nodes one manifest may hold (fatal beyond it).
pub const MAX_NODES: usize = 100_000;

/// The maximum number of relations one manifest may hold (fatal beyond it).
pub const MAX_RELATIONS: usize = 250_000;

/// The maximum number of gaps one manifest may carry (fatal beyond it).
pub const MAX_GAPS: usize = 10_000;

/// The maximum number of external references one node may carry.
pub const MAX_EXTERNAL_REFS: usize = 16;

/// The maximum number of evidence references one relation may carry.
pub const MAX_EVIDENCE_REFS: usize = 16;

/// The maximum size in bytes of one manifest document accepted for
/// validation (fatal beyond it).
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024 * 1024;

/// The maximum size in bytes of one canonical export.
pub const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

/// The maximum number of rows one query may return.
pub const MAX_RESULT_ROWS: usize = 50_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.trace-manifest");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/trace-manifest/v1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_NODES, 100_000);
        assert_eq!(MAX_RELATIONS, 250_000);
        assert_eq!(MAX_GAPS, 10_000);
        assert_eq!(MAX_EXTERNAL_REFS, 16);
        assert_eq!(MAX_EVIDENCE_REFS, 16);
        assert_eq!(MAX_MANIFEST_BYTES, 64 * 1024 * 1024);
        assert_eq!(MAX_EXPORT_BYTES, 32 * 1024 * 1024);
        assert_eq!(MAX_RESULT_ROWS, 50_000);
    }
}
