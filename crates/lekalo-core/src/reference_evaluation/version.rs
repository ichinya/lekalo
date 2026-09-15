//! Issue #107 reference-evaluation contract identity and hard denial
//! limits.
//!
//! The reference evaluation is its own contract family: independent of
//! the product release, of the source Model versions, of the Scenario
//! IR contract, of the invariant-transition attachment, and of every
//! execution backend. The behavior of the evaluator is pinned by the
//! separate reference-semantics identity, which is versioned with the
//! Model/IR pins every trace records: a trace names the exact scenario
//! digest, Model pin, IR digest, and attachment revision it
//! interpreted, so a native backend can compare its own normalized
//! execution against the exact reference inputs.
//!
//! The limits below are the owner-approved v1 constants (ADR-0041):
//! every construction, execution, and canonical export rejects with an
//! explicit registered diagnostic instead of truncating by arrival
//! order.

/// The reference-evaluation contract family identifier.
pub const FAMILY: &str = "dev.lekalo.reference-evaluation";

/// The exact reference-evaluation contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.reference-evaluation@1.0.0";

/// The exact wire discriminator of the reference-evaluation trace.
pub const SCHEMA_VERSION: &str = "lekalo/reference-evaluation/v1.0.0";

/// The identity of the executable reference semantics the evaluator
/// implements. It changes only through a reviewed successor of this
/// contract family, never silently.
pub const SEMANTICS_IDENTITY: &str = "dev.lekalo.reference-semantics@1.0.0";

/// The fallback evaluation clock applied when a `when` step carries no
/// clock reference and no `given` clock was established: the epoch
/// instant. It is reference behavior, never a host value.
pub const EPOCH_CLOCK: &str = "1970-01-01T00:00:00Z";

/// The maximum number of distinct entity rows one evaluation may hold.
pub const MAX_ROWS: usize = 4096;

/// The maximum number of effect-log entries one evaluation may record.
pub const MAX_EFFECTS: usize = 4096;

/// The maximum number of assertion records one evaluation may carry
/// (the Scenario IR already bounds `then` steps at 512).
pub const MAX_ASSERTIONS: usize = 512;

/// The maximum number of step records one evaluation may carry (the
/// Scenario IR already bounds `given` plus `when` at 512).
pub const MAX_STEP_RECORDS: usize = 512;

/// The maximum size in bytes of one canonical reference-evaluation
/// trace payload.
pub const MAX_CANONICAL_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_spellable_from_their_parts() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(SCHEMA_VERSION, "lekalo/reference-evaluation/v1.0.0");
        assert!(SEMANTICS_IDENTITY.starts_with("dev.lekalo.reference-semantics@"));
    }

    #[test]
    fn bounds_are_the_owner_approved_v1_constants() {
        assert_eq!(MAX_ROWS, 4096);
        assert_eq!(MAX_EFFECTS, 4096);
        assert_eq!(MAX_ASSERTIONS, 512);
        assert_eq!(MAX_STEP_RECORDS, 512);
        assert_eq!(MAX_CANONICAL_BYTES, 1024 * 1024);
    }
}
