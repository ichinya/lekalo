//! Issue #14 effect-graph contract identity and hard denial limits.
//!
//! The effect contract is its own family: independent of the product
//! release, of the source Model versions, of the IR and graph contracts,
//! and of the diagnostic registry. The limits below are the owner-approved
//! v1 constants (ADR-0013): every construction, query, comparison, and
//! export rejects with an explicit registered diagnostic instead of
//! truncating by arrival order.

/// The effect contract family identifier.
pub const FAMILY: &str = "dev.lekalo.effects";

/// The exact effect contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.effects@1.0.0";

/// The exact wire discriminator of the effect contract.
pub const SCHEMA_VERSION: &str = "lekalo/effects/v1.0.0";

/// The maximum number of effect edges one graph may hold (declared plus
/// detected together; fatal beyond it).
pub const MAX_EFFECTS: usize = 100_000;

/// The maximum depth of one bounded traversal reused from the #13 graph.
pub const MAX_DEPTH: usize = 256;

/// The maximum number of edges one query may return.
pub const MAX_RESULT_EDGES: usize = 250_000;

/// The maximum number of summary rows one summary may carry.
pub const MAX_RESULT_ROWS: usize = 50_000;

/// The maximum size in bytes of one canonical export or summary payload.
pub const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

/// The maximum number of filter terms (resource, operation, kind,
/// confidence selectors) one query spec may carry.
pub const MAX_FILTER_TERMS: usize = 128;

/// The maximum number of provenance/evidence references one edge may
/// carry forward into comparison explanations.
pub const MAX_PROVENANCE_RECORDS: usize = 8;

/// The maximum number of entries one detected-evidence envelope may carry.
pub const MAX_ENVELOPE_ENTRIES: usize = 100_000;

/// The maximum number of detected-evidence envelopes one graph may carry.
pub const MAX_ENVELOPES: usize = 8;

/// The maximum number of items one comparison may report.
pub const MAX_COMPARISON_ITEMS: usize = 50_000;

/// The maximum number of conflicts one conflict query may report.
pub const MAX_CONFLICT_ITEMS: usize = 50_000;

/// The maximum number of changed operations one explicit change set may
/// name (the typed `--changed` handoff; #16 owns its derivation).
pub const MAX_CHANGED_OPERATIONS: usize = 128;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.effects");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/effects/v1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_EFFECTS, 100_000);
        assert_eq!(MAX_DEPTH, 256);
        assert_eq!(MAX_RESULT_EDGES, 250_000);
        assert_eq!(MAX_RESULT_ROWS, 50_000);
        assert_eq!(MAX_EXPORT_BYTES, 32 * 1024 * 1024);
        assert_eq!(MAX_FILTER_TERMS, 128);
        assert_eq!(MAX_PROVENANCE_RECORDS, 8);
        assert_eq!(MAX_ENVELOPE_ENTRIES, 100_000);
        assert_eq!(MAX_ENVELOPES, 8);
        assert_eq!(MAX_COMPARISON_ITEMS, 50_000);
        assert_eq!(MAX_CONFLICT_ITEMS, 50_000);
        assert_eq!(MAX_CHANGED_OPERATIONS, 128);
    }
}
