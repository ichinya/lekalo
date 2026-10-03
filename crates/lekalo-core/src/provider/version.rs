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
//!
//! The two output-schema identities for the receipt-shaped operations
//! without an embedded wire discriminator (`validate` and
//! `generate --check`) are published as closed describing schemas in
//! this implementation series; their pins name the governing contract,
//! not an emitted field (see `docs/provider-contract.md`).

/// The workflow-provider contract family identifier.
pub const FAMILY: &str = "dev.lekalo.workflow-provider";

/// The exact workflow-provider contract version: the product version of
/// the implementation commit (issue #34).
pub const VERSION: &str = "0.6.4";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.workflow-provider@0.6.4";

/// The exact wire discriminator of the workflow-provider manifest.
pub const SCHEMA_VERSION: &str = "lekalo/workflow-provider/v0.6.4";

/// The discovery command that emits the manifest. It works outside a
/// project, reads nothing, launches nothing, and writes nothing.
pub const DISCOVERY_COMMAND: &str = "lekalo provider describe";

/// The digest algorithm of the manifest self-digest and of every
/// consumer binding. The digest domain is the canonical JSON (sorted
/// keys, no whitespace) of the manifest with the `manifestDigest` field
/// removed.
pub const DIGEST_ALGORITHM: &str = "sha256";

/// The recommended consumer budget for `lekalo context` inside a
/// workflow provider run. The core hard bound stays
/// [`crate::context::MAX_BUDGET_TOKENS`]; the workflow recommendation is
/// deliberately smaller so a capsule fits typical process limits.
pub const RECOMMENDED_BUDGET_TOKENS: u64 = 5_000;

/// The exact number of advertised workflow operations. The closed
/// vocabulary fails closed: discovery supports no `init`, `install`,
/// `update`, `sync`, or cleanup operation.
pub const OPERATION_COUNT: usize = 10;

/// The exact product version this contract was implemented at (issue
/// #34). Deliberately a frozen literal, not `CARGO_PKG_VERSION`: an
/// unchanged contract keeps its version across later product releases
/// (`docs/versioning.md`), so this value names the contract's product
/// generation and may trail a later binary's `--version`. Consumers
/// negotiate on `identity`/`schemaVersion`, never on this field.
pub const PRODUCT_VERSION: &str = "0.6.4";

/// The published closed output contract of `lekalo validate` success
/// receipts. The payload carries no embedded `schemaVersion` member;
/// this identity names the governing describing schema
/// (`contracts/validation-report.schema.v0.6.4.json`).
pub const VALIDATION_REPORT_SCHEMA: &str = "lekalo/validation-report/v0.6.4";

/// The published closed output contract of `lekalo generate --check`
/// receipts. The payload carries no embedded `schemaVersion` member;
/// this identity names the governing describing schema
/// (`contracts/generate-check-receipt.schema.v0.6.3.json`).
pub const GENERATE_CHECK_SCHEMA: &str = "lekalo/generate-check/v0.6.3";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.workflow-provider");
        assert_eq!(VERSION, "0.6.4");
        assert_eq!(SCHEMA_VERSION, "lekalo/workflow-provider/v0.6.4");
    }

    #[test]
    fn discovery_is_the_describe_command() {
        assert_eq!(DISCOVERY_COMMAND, "lekalo provider describe");
    }

    #[test]
    fn digest_and_bounds_match_the_published_contract() {
        assert_eq!(DIGEST_ALGORITHM, "sha256");
        assert_eq!(RECOMMENDED_BUDGET_TOKENS, 5_000);
        assert_eq!(OPERATION_COUNT, 10);
        // The workflow recommendation stays inside the core hard bound.
        const { assert!(RECOMMENDED_BUDGET_TOKENS <= crate::context::version::MAX_BUDGET_TOKENS) }
    }

    #[test]
    fn product_version_matches_the_workspace_release() {
        assert_eq!(PRODUCT_VERSION, "0.6.4");
    }

    #[test]
    fn receipt_output_identities_are_the_published_describing_schemas() {
        assert_eq!(VALIDATION_REPORT_SCHEMA, "lekalo/validation-report/v0.6.4");
        assert_eq!(GENERATE_CHECK_SCHEMA, "lekalo/generate-check/v0.6.3");
    }
}
