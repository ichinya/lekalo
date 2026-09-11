//! Issue #92 doctor/readiness contract identity and recorded v1 limits.
//!
//! The doctor contract is its own family: independent of the product
//! release, of the Model/IR/graph/effect/lock contracts, and of the
//! diagnostic registry. Doctor is a read-only diagnostic projection; the
//! limits below are the owner-approved v1 constants (ADR-0032): every
//! answer is a closed check state with a closed next-action recipe id and
//! at most a bounded echo of the preserved registry rule ids.

/// The doctor contract family identifier.
pub const FAMILY: &str = "dev.lekalo.doctor";

/// The exact doctor contract version.
pub const VERSION: &str = "1.0.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.doctor@1.0.0";

/// The exact wire discriminator of the doctor contract.
pub const SCHEMA_VERSION: &str = "lekalo/doctor/v1.0.0";

/// The maximum number of checks one report may carry (the closed v1
/// vocabulary is far below this; the bound fails closed).
pub const MAX_CHECKS: usize = 32;

/// The maximum number of preserved registry rule ids one check echoes.
pub const MAX_CHECK_DIAGNOSTICS: usize = 8;

/// The maximum number of trace manifests one invocation may supply.
pub const MAX_TRACES: usize = 16;

/// The maximum number of steps one safe-fix recipe may list.
pub const MAX_RECIPE_STEPS: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert_eq!(FAMILY, "dev.lekalo.doctor");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/doctor/v1.0.0");
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_CHECKS, 32);
        assert_eq!(MAX_CHECK_DIAGNOSTICS, 8);
        assert_eq!(MAX_TRACES, 16);
        assert_eq!(MAX_RECIPE_STEPS, 8);
    }
}
