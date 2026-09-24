//! Issue #87 data-flow report contract identity and hard denial limits.
//!
//! The data-flow report is its own derived-artifact family: independent
//! of the product release, of the Model/IR/effect-graph contract
//! versions, and of the diagnostic registry. The report is bound to the
//! exact canonical digests of its classification and policy inputs; it
//! is derived, read-only, and never an input to itself.

/// The data-flow report family identifier.
pub const FAMILY: &str = "dev.lekalo.data-flow-report";

/// The exact data-flow report contract version.
pub const VERSION: &str = "0.4.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.data-flow-report@0.4.0";

/// The exact wire discriminator of the data-flow report contract.
pub const SCHEMA_VERSION: &str = "lekalo/data-flow-report/v0.4.0";

/// The exact IR identity the report binds (`irRef`).
pub const IR_IDENTITY: &str = "dev.lekalo.ir@0.2.16";

/// The exact Model version the report binds (`modelRef`).
pub const MODEL_VERSION: &str = "0.2.16";

/// The maximum number of flows one report may carry.
pub const MAX_FLOWS: usize = 8192;

/// The maximum number of findings one report may carry.
pub const MAX_FINDINGS: usize = 1024;

/// The maximum number of unknowns one report may carry.
pub const MAX_UNKNOWNS: usize = 1024;

/// The maximum number of path hops one flow may carry.
pub const MAX_PATH_HOPS: usize = 64;

/// The maximum number of open questions one report may carry.
pub const MAX_OPEN_QUESTIONS: usize = 256;

/// The maximum size in bytes of one canonical report payload.
pub const MAX_CANONICAL_BYTES: usize = 8 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_parts_are_consistent() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert!(SCHEMA_VERSION.ends_with(VERSION));
        assert!(IR_IDENTITY.starts_with("dev.lekalo.ir@"));
    }

    #[test]
    fn bounds_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_FLOWS, 8192);
        assert_eq!(MAX_FINDINGS, 1024);
        assert_eq!(MAX_UNKNOWNS, 1024);
        assert_eq!(MAX_PATH_HOPS, 64);
        assert_eq!(MAX_OPEN_QUESTIONS, 256);
        assert_eq!(MAX_CANONICAL_BYTES, 8 * 1024 * 1024);
    }
}
