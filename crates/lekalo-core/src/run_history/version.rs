//! The closed contract identities of the run-history family (issue #121).
//!
//! Every constant is the exact wire spelling embedded in the versioned
//! schemas under `contracts/`; unknown versions refuse with
//! `history.version-unsupported` and are never silently converted.

/// The accepted run-record schema discriminator.
pub const RUN_RECORD_SCHEMA_VERSION: &str = "lekalo/run-record/v0.4.0";
/// The accepted run-record contract identity.
pub const RUN_RECORD_IDENTITY: &str = "dev.lekalo.run-record@0.4.0";
/// The recorder-owned artifact-kind label of a run record. This is a
/// closed local label of the recorder schema; authority-matrix
/// admission of recorder kinds stays with the #120 successor procedure.
pub const RUN_RECORD_ARTIFACT_KIND: &str = "history.run-record";

/// The accepted assertion-set schema discriminator.
pub const RUN_ASSERTIONS_SCHEMA_VERSION: &str = "lekalo/run-assertions/v0.4.0";
/// The accepted assertion-set contract identity.
pub const RUN_ASSERTIONS_IDENTITY: &str = "dev.lekalo.run-assertions@0.4.0";
/// The recorder-owned artifact-kind label of an assertion set.
pub const RUN_ASSERTIONS_ARTIFACT_KIND: &str = "history.assertion-set";

/// The accepted store-metadata schema discriminator.
pub const STORE_SCHEMA_VERSION: &str = "lekalo/run-history-store/v0.4.0";
/// The accepted store-metadata contract identity.
pub const STORE_IDENTITY: &str = "dev.lekalo.run-history-store@0.4.0";

/// The accepted harness-observation schema discriminator.
pub const OBSERVATION_SCHEMA_VERSION: &str = "lekalo/run-observation/v0.4.0";
/// The accepted harness-observation contract identity.
pub const OBSERVATION_IDENTITY: &str = "dev.lekalo.run-observation@0.4.0";

/// The SQLite metadata key carrying the store schema discriminator.
pub(crate) const META_SCHEMA_VERSION: &str = "schema_version";
/// The SQLite metadata key carrying the store contract identity.
pub(crate) const META_IDENTITY: &str = "identity";
