//! The CI report contract version identity (issue #103).
//!
//! The ci-report contract takes the product version of the commit that
//! introduced it per `docs/versioning.md`; the closed schema is
//! `contracts/ci-report.schema.v0.6.3.json`. The identity and
//! discriminator below are the single source the core, the CLI, and the
//! contract gate pin.

/// The exact wire discriminator of the CI report contract.
pub const SCHEMA_VERSION: &str = "lekalo/ci-report/v0.6.3";

/// The contract identity of the CI report wire.
pub const IDENTITY: &str = "dev.lekalo.ci-report@0.6.3";

/// The current CI report contract version (the product version of the
/// implementation commit).
pub const REPORT_VERSION: &str = "0.6.3";

/// The maximum diagnostics one report carries (the diagnostic set bound).
pub const MAX_DIAGNOSTICS: usize = 256;
