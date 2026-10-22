//! Issue #52 task-links contract identity and hard refusal limits.
//!
//! The task-links integration family is its own closed contract family:
//! independent of the product release, of the source Model and IR
//! versions, and of every owner contract it pins (authorization,
//! transaction-concurrency, extended-effects, invariant-transition,
//! storage-projection) without widening any of them. All five task-links
//! artifacts take the product version of their implementation commit per
//! `docs/versioning.md`; the frozen owner contracts keep their accepted
//! versions.

/// The task-links integration family identifier.
pub const FAMILY: &str = "dev.lekalo.task-links-integration";

/// The exact task-links attachment contract version.
pub const VERSION: &str = "0.8.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.task-links-integration@0.8.0";

/// The exact wire discriminator of the task-links attachment.
pub const SCHEMA_VERSION: &str = "lekalo/task-links-integration/v0.8.0";

/// The exact Model/IR pin the family binds: the Model/IR successor
/// that owns the typed relations and secondary uniqueness (no alias
/// under the 0.2.16 Model exists).
pub const MODEL_VERSION: &str = "0.8.0";
pub const SUCCESSOR_IR_IDENTITY: &str = "dev.lekalo.ir@0.8.0";

/// The exact wire discriminator of the task-links report.
pub const REPORT_SCHEMA_VERSION: &str = "lekalo/task-links-report/v0.8.0";

/// The exact contract identity of the task-links report.
pub const REPORT_IDENTITY: &str = "dev.lekalo.task-links-report@0.8.0";

/// The exact wire discriminator of the task-links evidence.
pub const EVIDENCE_SCHEMA_VERSION: &str = "lekalo/task-links-evidence/v0.8.0";

/// The exact contract identity of the task-links evidence.
pub const EVIDENCE_IDENTITY: &str = "dev.lekalo.task-links-evidence@0.8.0";

/// The exact wire discriminator of the task-links portability projection.
pub const PORTABILITY_SCHEMA_VERSION: &str = "lekalo/task-links-portability/v0.8.0";

/// The exact contract identity of the task-links portability projection.
pub const PORTABILITY_IDENTITY: &str = "dev.lekalo.task-links-portability@0.8.0";

/// The maximum size in bytes of one attachment or evidence payload.
pub const MAX_BYTES: usize = 1_048_576;

/// The maximum number of relation declarations one attachment carries.
pub const MAX_RELATIONS: usize = 64;

/// The maximum number of unique-constraint declarations one attachment
/// carries.
pub const MAX_UNIQUE_CONSTRAINTS: usize = 16;

/// The maximum number of steps one evidence document carries.
pub const MAX_STEPS: usize = 128;

/// The maximum number of observations one evidence document carries.
pub const MAX_OBSERVATIONS: usize = 256;

/// The exact external identity tuple width: workspace, installation,
/// provider, object type, external id.
pub const EXTERNAL_IDENTITY_FIELDS: usize = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(SCHEMA_VERSION, "lekalo/task-links-integration/v0.8.0");
        assert_eq!(REPORT_IDENTITY, "dev.lekalo.task-links-report@0.8.0");
        assert_eq!(EVIDENCE_IDENTITY, "dev.lekalo.task-links-evidence@0.8.0");
        assert_eq!(
            PORTABILITY_IDENTITY,
            "dev.lekalo.task-links-portability@0.8.0"
        );
    }
}
