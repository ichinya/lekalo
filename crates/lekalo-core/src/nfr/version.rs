//! Issue #85 NFR constraint contract identity and hard denial limits.
//!
//! The NFR contract is its own three-family set: the constraint
//! attachment (`dev.lekalo.nfr`), the volatile measured-evidence set
//! (`dev.lekalo.nfr-evidence`), and the derived read-only resolution
//! report (`dev.lekalo.nfr-report`). All three are independent of the
//! product release, of the Model/IR/trace/graph contract versions, and
//! of the diagnostic registry version. The limits below are the v1
//! constants: every construction, resolution, and export rejects with
//! an explicit registered diagnostic instead of truncating by arrival
//! order.

/// The NFR constraint attachment contract family identifier.
pub const FAMILY: &str = "dev.lekalo.nfr";

/// The exact NFR attachment contract version.
pub const VERSION: &str = "0.4.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.nfr@0.4.0";

/// The exact wire discriminator of the NFR attachment contract.
pub const SCHEMA_VERSION: &str = "lekalo/nfr/v0.4.0";

/// The NFR evidence set contract family identifier.
pub const EVIDENCE_FAMILY: &str = "dev.lekalo.nfr-evidence";

/// The exact NFR evidence contract version.
pub const EVIDENCE_VERSION: &str = "0.4.0";

/// The exact evidence contract identity.
pub const EVIDENCE_IDENTITY: &str = "dev.lekalo.nfr-evidence@0.4.0";

/// The exact wire discriminator of the NFR evidence contract.
pub const EVIDENCE_SCHEMA_VERSION: &str = "lekalo/nfr-evidence/v0.4.0";

/// The NFR report contract family identifier.
pub const REPORT_FAMILY: &str = "dev.lekalo.nfr-report";

/// The exact NFR report contract version.
pub const REPORT_VERSION: &str = "0.4.0";

/// The exact report contract identity.
pub const REPORT_IDENTITY: &str = "dev.lekalo.nfr-report@0.4.0";

/// The exact wire discriminator of the NFR report contract.
pub const REPORT_SCHEMA_VERSION: &str = "lekalo/nfr-report/v0.4.0";

/// The maximum number of declared constraints (fatal beyond it).
pub const MAX_CONSTRAINTS: usize = 1024;

/// The maximum number of registered open questions (fatal beyond it).
pub const MAX_OPEN_QUESTIONS: usize = 256;

/// The maximum accepted-environment declarations per constraint (fatal
/// beyond it).
pub const MAX_ENVIRONMENTS: usize = 16;

/// The maximum required-capability entries per constraint (fatal
/// beyond it).
pub const MAX_CAPABILITIES: usize = 32;

/// The maximum evidence results per evidence document (fatal beyond
/// it).
pub const MAX_RESULTS: usize = 4096;

/// The maximum measurements per evidence result (fatal beyond it).
pub const MAX_MEASUREMENTS: usize = 64;

/// The maximum size in bytes of one attachment or evidence document
/// accepted for validation (fatal beyond it).
pub const MAX_DOC_BYTES: usize = 8 * 1024 * 1024;

/// The maximum size in bytes of one canonical export (fatal beyond it).
pub const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.nfr");
        assert_eq!(VERSION, "0.4.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/nfr/v0.4.0");
        assert_eq!(
            EVIDENCE_IDENTITY,
            format!("{EVIDENCE_FAMILY}@{EVIDENCE_VERSION}")
        );
        assert_eq!(EVIDENCE_FAMILY, "dev.lekalo.nfr-evidence");
        assert_eq!(EVIDENCE_VERSION, "0.4.0");
        assert_eq!(EVIDENCE_SCHEMA_VERSION, "lekalo/nfr-evidence/v0.4.0");
        assert_eq!(REPORT_IDENTITY, format!("{REPORT_FAMILY}@{REPORT_VERSION}"));
        assert_eq!(REPORT_FAMILY, "dev.lekalo.nfr-report");
        assert_eq!(REPORT_VERSION, "0.4.0");
        assert_eq!(REPORT_SCHEMA_VERSION, "lekalo/nfr-report/v0.4.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_CONSTRAINTS, 1024);
        assert_eq!(MAX_OPEN_QUESTIONS, 256);
        assert_eq!(MAX_ENVIRONMENTS, 16);
        assert_eq!(MAX_CAPABILITIES, 32);
        assert_eq!(MAX_RESULTS, 4096);
        assert_eq!(MAX_MEASUREMENTS, 64);
        assert_eq!(MAX_DOC_BYTES, 8 * 1024 * 1024);
        assert_eq!(MAX_EXPORT_BYTES, 32 * 1024 * 1024);
    }
}
