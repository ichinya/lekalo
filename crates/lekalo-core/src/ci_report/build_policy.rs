//! The shared CI policy table (issue #103 fix round 1).
//!
//! One closed vocabulary for the optional-absence decision, evaluated
//! identically for check rows and suite case rows so the JSON report,
//! the evaluation verdict, and the JUnit projection can never disagree.
//! The policy is a closed three-level built-in; a file-borne versioned
//! policy contract remains deferred follow-up work.

/// How an explicitly optional absence is treated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbsenceRule {
    /// Surface as a warning (exit stays 0, coverage incomplete).
    Warn,
    /// Promote to an error (exit 4).
    Error,
    /// Skip silently (exit 0).
    Skip,
}

/// The evaluated policy table of one run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CiPolicyTable {
    /// The optional-absence rule.
    pub optional_absence: AbsenceRule,
}

impl CiPolicyTable {
    /// The table of one closed [`super::build::CiPolicy`] level.
    pub const fn of(policy: super::build::CiPolicy) -> Self {
        match policy {
            super::build::CiPolicy::Default => Self {
                optional_absence: AbsenceRule::Warn,
            },
            super::build::CiPolicy::Strict => Self {
                optional_absence: AbsenceRule::Error,
            },
            super::build::CiPolicy::Lenient => Self {
                optional_absence: AbsenceRule::Skip,
            },
        }
    }
}
