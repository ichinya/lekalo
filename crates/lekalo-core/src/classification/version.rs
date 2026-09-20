//! Issue #87 classification contract identity and hard denial limits.
//!
//! The data-classification family is its own contract family:
//! independent of the product release, of the source Model and IR
//! versions, of the authorization, extended-effects, transport, NFR,
//! and privacy-policy families, and of the diagnostic registry. The
//! M4 project line is 0.4.x, so the family's first generation is
//! 0.4.0. The limits below are the owner-approved v1 constants; every
//! normalization rejects with an explicit registered diagnostic
//! instead of truncating by arrival order.

/// The data-classification contract family identifier.
pub const FAMILY: &str = "dev.lekalo.data-classification";

/// The exact data-classification contract version.
pub const VERSION: &str = "0.4.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.data-classification@0.4.0";

/// The exact wire discriminator of the data-classification contract.
pub const SCHEMA_VERSION: &str = "lekalo/data-classification/v0.4.0";

/// The exact IR identity this attachment binds (`irRef`).
pub const IR_IDENTITY: &str = "dev.lekalo.ir@0.2.16";

/// The exact Model version this attachment binds (`modelRef`).
pub const MODEL_VERSION: &str = "0.2.16";

/// The maximum number of classification entries one attachment may carry.
pub const MAX_CLASSIFICATIONS: usize = 4096;

/// The maximum number of declassification grants one attachment may carry.
pub const MAX_DECLASSIFICATIONS: usize = 256;

/// The maximum number of open questions one attachment may carry.
pub const MAX_OPEN_QUESTIONS: usize = 256;

/// The maximum number of labels one classification entry may carry.
pub const MAX_LABELS: usize = 16;

/// The maximum number of condition tokens one grant may carry.
pub const MAX_CONDITIONS: usize = 8;

/// The maximum number of subject path segments (semantic id + fields).
pub const MAX_SUBJECT_SEGMENTS: usize = 3;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 1024 * 1024;

/// The maximum size in bytes of one accepted attachment document.
pub const MAX_DOC_BYTES: usize = 8 * 1024 * 1024;

/// The maximum size in bytes of one canonical export payload.
pub const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_parts_are_consistent() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert!(SCHEMA_VERSION.ends_with(VERSION));
        assert!(IR_IDENTITY.starts_with("dev.lekalo.ir@"));
    }

    #[test]
    fn bounds_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_CLASSIFICATIONS, 4096);
        assert_eq!(MAX_DECLASSIFICATIONS, 256);
        assert_eq!(MAX_OPEN_QUESTIONS, 256);
        assert_eq!(MAX_LABELS, 16);
        assert_eq!(MAX_CONDITIONS, 8);
        assert_eq!(MAX_SUBJECT_SEGMENTS, 3);
        assert_eq!(MAX_CANONICAL_BYTES, 1024 * 1024);
        assert_eq!(MAX_DOC_BYTES, 8 * 1024 * 1024);
        assert_eq!(MAX_EXPORT_BYTES, 32 * 1024 * 1024);
    }
}
