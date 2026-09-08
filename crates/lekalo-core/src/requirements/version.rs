//! Issue #36 requirements-traceability contract identity and hard denial
//! limits.
//!
//! The requirements contract is its own family: independent of the product
//! release, of the Model/IR/trace/graph contract versions, of the diagnostic
//! registry version, and of the OpenSpec CLI (which is never invoked). The
//! limits below are the owner-approved v1 constants (ADR-0026): every
//! construction, resolution, and export rejects with an explicit registered
//! diagnostic instead of truncating by arrival order.

/// The requirements attachment contract family identifier.
pub const FAMILY: &str = "dev.lekalo.requirements";

/// The exact requirements attachment contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.requirements@1.0.0";

/// The exact wire discriminator of the requirements attachment contract.
pub const SCHEMA_VERSION: &str = "lekalo/requirements/v1.0.0";

/// The requirements report contract family identifier.
pub const REPORT_FAMILY: &str = "dev.lekalo.requirements-report";

/// The exact requirements report contract version.
pub const REPORT_VERSION: &str = "1.0.0";

/// The exact report contract identity.
pub const REPORT_IDENTITY: &str = "dev.lekalo.requirements-report@1.0.0";

/// The exact wire discriminator of the requirements report contract.
pub const REPORT_SCHEMA_VERSION: &str = "lekalo/requirements-report/v1.0.0";

/// The maximum number of declared requirement providers (fatal beyond it).
pub const MAX_PROVIDERS: usize = 8;

/// The maximum number of declared requirement references (fatal beyond it).
pub const MAX_REFERENCES: usize = 4096;

/// The maximum number of capabilities one provider may carry (fatal beyond
/// it).
pub const MAX_CAPABILITIES: usize = 256;

/// The maximum distinct requirement ids encountered in one provider and
/// the aggregate catalog, coverage or conflict rows across all providers.
pub const MAX_REQUIREMENTS: usize = 10_000;

/// The maximum number of active change directories one provider may apply
/// (fatal beyond it); the archive subtree is not read.
pub const MAX_CHANGES: usize = 256;

/// The maximum requirement title length in Unicode scalars.
pub const MAX_TITLE_CHARS: usize = 128;

/// The maximum size in bytes of one spec or delta document accepted for
/// reading (fatal beyond it).
pub const MAX_SPEC_BYTES: usize = 1024 * 1024;

/// The maximum size in bytes of one attachment document accepted for
/// validation (fatal beyond it).
pub const MAX_DOC_BYTES: usize = 8 * 1024 * 1024;

/// The maximum size in bytes of one canonical export (fatal beyond it).
pub const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.requirements");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/requirements/v1.0.0");
        assert_eq!(REPORT_IDENTITY, format!("{REPORT_FAMILY}@{REPORT_VERSION}"));
        assert_eq!(REPORT_FAMILY, "dev.lekalo.requirements-report");
        assert_eq!(REPORT_VERSION, "1.0.0");
        assert_eq!(REPORT_SCHEMA_VERSION, "lekalo/requirements-report/v1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_PROVIDERS, 8);
        assert_eq!(MAX_REFERENCES, 4096);
        assert_eq!(MAX_CAPABILITIES, 256);
        assert_eq!(MAX_REQUIREMENTS, 10_000);
        assert_eq!(MAX_CHANGES, 256);
        assert_eq!(MAX_TITLE_CHARS, 128);
        assert_eq!(MAX_SPEC_BYTES, 1024 * 1024);
        assert_eq!(MAX_DOC_BYTES, 8 * 1024 * 1024);
        assert_eq!(MAX_EXPORT_BYTES, 32 * 1024 * 1024);
    }
}
