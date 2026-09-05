//! Issue #15 inspect contract identity and recorded v1 limits.
//!
//! The inspect contract is its own family: independent of the product
//! release, of the source Model versions, of the IR/graph/effect
//! contracts, and of the diagnostic registry. The limits below are the
//! owner-approved v1 constants (ADR-0014): every bound degrades the
//! result visibly with returned/omitted counts and a reason, and only
//! the whole-payload bound fails the invocation outright.

/// The inspect contract family identifier.
pub const FAMILY: &str = "dev.lekalo.inspect";

/// The exact inspect contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.inspect@1.0.0";

/// The exact wire discriminator of the inspect contract.
pub const SCHEMA_VERSION: &str = "lekalo/inspect/v1.0.0";

/// The maximum number of items one bounded section may return.
pub const MAX_SECTION_ITEMS: usize = 256;

/// The maximum number of candidate ids one ambiguous short-name
/// diagnostic may carry.
pub const MAX_CANDIDATES: usize = 32;

/// The maximum byte length of one echoed semantic id.
pub const MAX_SYMBOL_ID_BYTES: usize = 192;

/// The maximum byte length of one description or message value.
pub const MAX_TEXT_BYTES: usize = 4096;

/// The maximum number of provenance references one item may carry.
pub const MAX_PROVENANCE_REFS: usize = 8;

/// The maximum size in bytes of one canonical inspect payload; beyond
/// it the invocation fails with `inspect.output-limit` instead of
/// emitting a silently partial answer.
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.inspect");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/inspect/v1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_SECTION_ITEMS, 256);
        assert_eq!(MAX_CANDIDATES, 32);
        assert_eq!(MAX_SYMBOL_ID_BYTES, 192);
        assert_eq!(MAX_TEXT_BYTES, 4096);
        assert_eq!(MAX_PROVENANCE_REFS, 8);
        assert_eq!(MAX_OUTPUT_BYTES, 1024 * 1024);
    }
}
