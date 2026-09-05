//! Issue #23 Scenario IR contract identity and hard denial limits.
//!
//! The Scenario IR is its own contract family: independent of the product
//! release, of the source Model versions, of the accepted #8 IR contract,
//! of the diagnostic registry, and of every execution backend. The limits
//! below are the owner-approved v1 constants (ADR-0014): every
//! normalization, validation, and canonical export rejects with an
//! explicit registered diagnostic instead of truncating by arrival order.

/// The Scenario IR contract family identifier.
pub const FAMILY: &str = "dev.lekalo.scenario-ir";

/// The exact Scenario IR contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.scenario-ir@1.0.0";

/// The exact wire discriminator of the Scenario IR contract.
pub const SCHEMA_VERSION: &str = "lekalo/scenario-ir/v1.0.0";

/// The exact identity of the source-map sidecar family the IR references.
pub const SOURCE_MAP_IDENTITY: &str = "dev.lekalo.scenario-sourcemap@1.0.0";

/// The exact identities of the accepted #8 IR contract the IR references.
pub const IR_IDENTITY: &str = crate::ir::IDENTITY;

/// The maximum number of `given` steps one scenario may carry.
pub const MAX_GIVEN_STEPS: usize = 256;

/// The maximum number of `when` steps one scenario may carry.
pub const MAX_WHEN_STEPS: usize = 256;

/// The maximum number of `then` steps one scenario may carry.
pub const MAX_THEN_STEPS: usize = 512;

/// The maximum total number of steps (`given` plus `when` plus `then`).
pub const MAX_TOTAL_STEPS: usize = 1024;

/// The maximum number of typed values or references one step may carry
/// across its selector, fields, input, payload, and assertion members.
pub const MAX_REFS_PER_STEP: usize = 128;

/// The maximum nesting depth of one typed value or reference.
pub const MAX_TYPED_DEPTH: usize = 32;

/// The maximum number of items one list or object typed value may hold.
pub const MAX_TYPED_ITEMS: usize = 4096;

/// The maximum number of Unicode code points one scalar string may hold.
pub const MAX_SCALAR_CODEPOINTS: usize = 4096;

/// The maximum number of field predicates one state selector may carry.
pub const MAX_SELECTOR_TERMS: usize = 64;

/// The maximum number of field entries one state, input, or payload map
/// may carry.
pub const MAX_FIELD_ENTRIES: usize = 64;

/// The maximum number of tags one scenario may carry.
pub const MAX_TAGS: usize = 64;

/// The maximum byte length of one tag.
pub const MAX_TAG_BYTES: usize = 64;

/// The maximum number of namespaced metadata entries one scenario may
/// carry.
pub const MAX_METADATA_ENTRIES: usize = 64;

/// The maximum canonical size in bytes of the namespaced metadata map.
pub const MAX_METADATA_BYTES: usize = 64 * 1024;

/// The maximum number of backend bindings one scenario may carry.
pub const MAX_BINDINGS: usize = 32;

/// The maximum number of capabilities one fixture or backend binding may
/// declare.
pub const MAX_CAPABILITIES: usize = 64;

/// The maximum number of projection paths one `contract_match` assertion
/// may carry.
pub const MAX_PROJECTION_PATHS: usize = 32;

/// The maximum number of entries one referenced source-map sidecar may
/// carry (the sidecar itself is a separate digest-bound artifact).
pub const MAX_SOURCE_MAP_ENTRIES: usize = 8192;

/// The maximum size in bytes of one canonical Scenario IR payload.
pub const MAX_CANONICAL_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.scenario-ir");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/scenario-ir/v1.0.0");
        assert_eq!(SOURCE_MAP_IDENTITY, "dev.lekalo.scenario-sourcemap@1.0.0");
        assert_eq!(IR_IDENTITY, "dev.lekalo.ir@0.1.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_GIVEN_STEPS, 256);
        assert_eq!(MAX_WHEN_STEPS, 256);
        assert_eq!(MAX_THEN_STEPS, 512);
        assert_eq!(MAX_TOTAL_STEPS, 1024);
        assert_eq!(MAX_REFS_PER_STEP, 128);
        assert_eq!(MAX_TYPED_DEPTH, 32);
        assert_eq!(MAX_TYPED_ITEMS, 4096);
        assert_eq!(MAX_SCALAR_CODEPOINTS, 4096);
        assert_eq!(MAX_SELECTOR_TERMS, 64);
        assert_eq!(MAX_FIELD_ENTRIES, 64);
        assert_eq!(MAX_TAGS, 64);
        assert_eq!(MAX_TAG_BYTES, 64);
        assert_eq!(MAX_METADATA_ENTRIES, 64);
        assert_eq!(MAX_METADATA_BYTES, 64 * 1024);
        assert_eq!(MAX_BINDINGS, 32);
        assert_eq!(MAX_CAPABILITIES, 64);
        assert_eq!(MAX_PROJECTION_PATHS, 32);
        assert_eq!(MAX_SOURCE_MAP_ENTRIES, 8192);
        assert_eq!(MAX_CANONICAL_BYTES, 1024 * 1024);
    }
}
