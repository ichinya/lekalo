//! Issue #75 context-budget report contract identity and hard limits.
//!
//! The context-budget report is its own derived-report family:
//! independent of the product release, of the Model/IR/graph/effect
//! contract versions, and of the diagnostic registry. One report is a
//! pure function of the pinned compilation, graph, effect graph, profile,
//! optional evidence, selector, and metric version; the identity below is
//! stamped into every report and the version participates in every
//! baseline-comparability decision.

/// The context-budget report family identifier.
pub const FAMILY: &str = "dev.lekalo.context-budget-report";

/// The exact context-budget report contract version.
pub const VERSION: &str = "0.6.3";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.context-budget-report@0.6.3";

/// The exact wire discriminator of the context-budget report contract.
pub const SCHEMA_VERSION: &str = "lekalo/context-budget-report/v0.6.3";

/// The exact metric-semantics version stamped into every report.
pub const METRIC_VERSION: &str = "context-budget-semantics/1";

/// The maximum token budget one report request may carry.
pub const MAX_BUDGET_TOKENS: u64 = 1_000_000;

/// The maximum number of analysis subjects one request may carry.
pub const MAX_SUBJECTS: usize = 10_000;

/// The maximum number of nodes one closure walk may visit.
pub const MAX_CLOSURE_NODES: usize = 50_000;

/// The maximum number of edges one closure walk may collect.
pub const MAX_CLOSURE_EDGES: usize = 250_000;

/// The maximum number of facts one ledger may carry.
pub const MAX_FACTS: usize = 50_000;

/// The maximum number of dependency-breakdown rows one subject carries.
pub const MAX_BREAKDOWN_ROWS: usize = 2_000;

/// The maximum number of suggested extraction boundaries per subject.
pub const MAX_SUGGESTIONS: usize = 16;

/// The maximum number of baseline comparison rows one comparison carries.
pub const MAX_COMPARISON_ROWS: usize = 20_000;

/// The maximum number of diagnostic data gap rows one subject carries.
pub const MAX_GAPS: usize = 256;

/// The maximum size in bytes of one canonical report payload.
pub const MAX_REPORT_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_parts_are_consistent() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert!(SCHEMA_VERSION.ends_with(VERSION));
    }

    #[test]
    fn bounds_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_BUDGET_TOKENS, 1_000_000);
        assert_eq!(MAX_SUBJECTS, 10_000);
        assert_eq!(MAX_CLOSURE_NODES, 50_000);
        assert_eq!(MAX_CLOSURE_EDGES, 250_000);
        assert_eq!(MAX_FACTS, 50_000);
        assert_eq!(MAX_BREAKDOWN_ROWS, 2_000);
        assert_eq!(MAX_SUGGESTIONS, 16);
        assert_eq!(MAX_COMPARISON_ROWS, 20_000);
        assert_eq!(MAX_GAPS, 256);
        assert_eq!(MAX_REPORT_BYTES, 32 * 1024 * 1024);
    }
}
