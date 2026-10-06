//! The exact portability contract identities (issue #19).
//!
//! The four new artifact families take the product version of their
//! commit per `docs/versioning.md`; the frozen predecessor contracts are
//! never touched. The reviewed recipe instance is embedded: the report
//! binds its digest, and the assessment refuses evidence that names any
//! other recipe.

/// The exact wire discriminator of the portability report contract.
pub const REPORT_SCHEMA_VERSION: &str = "lekalo/portability-report/v0.8.0";

/// The exact report identity.
pub const REPORT_IDENTITY: &str = "dev.lekalo.portability-report@0.8.0";

/// The exact wire discriminator of the target-binding configuration.
pub const TARGETS_SCHEMA_VERSION: &str = "lekalo/portability-targets/v0.8.0";

/// The exact target-configuration identity.
pub const TARGETS_IDENTITY: &str = "dev.lekalo.portability-targets@0.8.0";

/// The exact wire discriminator of the evidence manifest.
pub const EVIDENCE_SCHEMA_VERSION: &str = "lekalo/portability-evidence/v0.8.0";

/// The exact evidence identity.
pub const EVIDENCE_IDENTITY: &str = "dev.lekalo.portability-evidence@0.8.0";

/// The exact wire discriminator of the reviewed recipe contract.
pub const RECIPE_SCHEMA_VERSION: &str = "lekalo/portability-recipe/v0.8.0";

/// The exact recipe identity.
pub const RECIPE_IDENTITY: &str = "dev.lekalo.portability-recipe@0.8.0";

/// The embedded reviewed recipe instance: the extraction, aggregation,
/// and mapping definitions the report binds to.
pub const RECIPE_BYTES: &[u8] =
    include_bytes!("../../../../contracts/portability-recipe.v0.8.0.json");

/// The bounded input byte limit for configuration and evidence
/// documents, enforced before any parse.
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
