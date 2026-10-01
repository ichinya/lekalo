//! Issue #34 workflow-provider contract identity and published bounds.
//!
//! The workflow-provider contract is its own family: it is the
//! AIFHub-consumable discovery handshake (`lekalo provider describe`),
//! independent of the product release semantics of every other family,
//! of the `lekalo.target/v1` adapter protocol (issue #27), of the
//! doctor family (issue #92), and of the diagnostic registry. Per
//! `docs/versioning.md` a changed or new contract takes the product
//! version of its implementation commit, so the constants below are
//! pinned to the workspace release that introduced them.

/// The workflow-provider contract family identifier.
pub const FAMILY: &str = "dev.lekalo.workflow-provider";

/// The exact workflow-provider contract version: the product version of
/// the implementation commit (issue #34).
pub const VERSION: &str = "0.6.3";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.workflow-provider@0.6.3";

/// The exact wire discriminator of the workflow-provider manifest.
pub const SCHEMA_VERSION: &str = "lekalo/workflow-provider/v0.6.3";

/// The discovery command that emits the manifest. It works outside any
/// project, reads nothing, launches nothing, and writes nothing.
pub const DISCOVERY_COMMAND: &str = "lekalo provider describe";

/// The digest algorithm of the manifest self-digest and of every
/// consumer binding. The digest domain is the canonical JSON (sorted
/// keys, no whitespace) of the manifest with the `manifestDigest` field
/// removed.
pub const DIGEST_ALGORITHM: &str = "sha256";

/// The recommended consumer budget for `lekalo context` inside a
/// workflow provider run. The core hard bound stays
/// [`crate::context::version::MAX_BUDGET_TOKENS`]; the workflow recommendation is
/// deliberately smaller so a capsule fits typical process limits.
pub const RECOMMENDED_BUDGET_TOKENS: u64 = 5_000;

/// The exact number of advertised workflow operations. The closed
/// vocabulary fails closed: discovery supports no `init`, `install`,
/// `update`, `sync`, or cleanup operation.
pub const OPERATION_COUNT: usize = 9;

/// The exact producing product version (workspace release 0.6.3),
/// restated as a literal so the manifest never depends on a CLI
/// environment variable.
pub const PRODUCT_VERSION: &str = "0.6.3";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.workflow-provider");
        assert_eq!(VERSION, "0.6.3");
        assert_eq!(SCHEMA_VERSION, "lekalo/workflow-provider/v0.6.3");
    }

    #[test]
    fn discovery_is_the_describe_command() {
        assert_eq!(DISCOVERY_COMMAND, "lekalo provider describe");
    }

    #[test]
    fn digest_and_bounds_match_the_published_contract() {
        assert_eq!(DIGEST_ALGORITHM, "sha256");
        assert_eq!(RECOMMENDED_BUDGET_TOKENS, 5_000);
        assert_eq!(OPERATION_COUNT, 9);
        // The workflow recommendation stays inside the core hard bound.
        const { assert!(RECOMMENDED_BUDGET_TOKENS <= crate::context::version::MAX_BUDGET_TOKENS) }
    }

    #[test]
    fn product_version_matches_the_workspace_release() {
        assert_eq!(PRODUCT_VERSION, "0.6.3");
    }
}
