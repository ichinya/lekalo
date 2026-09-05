//! Issue #18 semantic-diff contract identity and hard denial limits.
//!
//! The diff contract is its own family: independent of the product
//! release, of the source Model versions, of the IR/graph/effect
//! contracts, of the compatibility-profile vocabulary, and of the
//! diagnostic registry. The limits below are the owner-approved v1
//! constants (ADR-0019): every comparison and export rejects with an
//! explicit registered diagnostic instead of truncating by arrival order.

/// The semantic-diff contract family identifier.
pub const FAMILY: &str = "dev.lekalo.semantic-diff";

/// The exact semantic-diff contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.semantic-diff@1.0.0";

/// The exact wire discriminator of the semantic-diff contract.
pub const SCHEMA_VERSION: &str = "lekalo/semantic-diff/v1.0.0";

/// The exact wire discriminator of the built-in profile vocabulary.
pub const PROFILE_VERSION: &str = "lekalo/diff-profile/v1.0.0";

/// The exact policy revision the built-in profiles evaluate.
pub const POLICY_REVISION: &str = "diff-policy/v1";

/// The maximum number of semantic subjects one side may project
/// (fatal beyond it, before any comparison).
pub const MAX_COMPARE_SUBJECTS: usize = 100_000;

/// The maximum number of change records one comparison may report.
pub const MAX_CHANGES: usize = 250_000;

/// The maximum number of affected seeds one comparison may report.
pub const MAX_SEEDS: usize = 50_000;

/// The maximum size in bytes of one canonical diff result.
pub const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

/// The maximum number of requested profiles (the closed built-in set
/// is five; the bound still rejects pathological requests).
pub const MAX_PROFILE_TERMS: usize = 128;

/// The maximum number of adapter contributions one comparison accepts.
pub const MAX_ADAPTER_CONTRIBUTIONS: usize = 8;

/// The maximum number of change references one adapter contribution
/// may carry.
pub const MAX_ADAPTER_EFFECTS: usize = 50_000;

/// The maximum number of seed references one adapter contribution may
/// carry.
pub const MAX_ADAPTER_SEEDS: usize = 50_000;

/// The maximum hop count of one rename-history walk (multi-hop renames
/// resolve within this bound; deeper chains never resolve and stay with
/// the removal resolver — a plain removal, or removal plus addition
/// with `history.conflicting` — never a guessed alias).
pub const MAX_HISTORY_HOPS: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.semantic-diff");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/semantic-diff/v1.0.0");
        assert_eq!(PROFILE_VERSION, "lekalo/diff-profile/v1.0.0");
        assert_eq!(POLICY_REVISION, "diff-policy/v1");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_COMPARE_SUBJECTS, 100_000);
        assert_eq!(MAX_CHANGES, 250_000);
        assert_eq!(MAX_SEEDS, 50_000);
        assert_eq!(MAX_EXPORT_BYTES, 32 * 1024 * 1024);
        assert_eq!(MAX_PROFILE_TERMS, 128);
        assert_eq!(MAX_ADAPTER_CONTRIBUTIONS, 8);
        assert_eq!(MAX_ADAPTER_EFFECTS, 50_000);
        assert_eq!(MAX_ADAPTER_SEEDS, 50_000);
        assert_eq!(MAX_HISTORY_HOPS, 16);
    }
}
