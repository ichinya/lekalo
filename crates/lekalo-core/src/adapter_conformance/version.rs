//! The adapter conformance suite version constants (issue #31).
//!
//! The suite is a qualification process over the already-published
//! `lekalo.target/v1` protocol and `dev.lekalo.ir` contract lines. It
//! publishes one report wire identity of its own; the identity is
//! independent of the product release and of every contract family.

use crate::target_protocol::version;

/// The report wire schema discriminator.
pub const SCHEMA_VERSION: &str = "lekalo/adapter-conformance/v1.0.0";

/// The published suite identity.
pub const IDENTITY: &str = "dev.lekalo.adapter-conformance@1.0.0";
/// Default per-exchange adapter deadline. The suite bounds every child
/// exchange far below the protocol's 600 s default so a hung adapter is
/// classified in seconds, not minutes.
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// The inclusive lower bound of the caller-supplied deadline.
pub const MIN_TIMEOUT_MS: u64 = 1_000;

/// The inclusive upper bound of the caller-supplied deadline; the
/// protocol-wide maximum.
pub const MAX_TIMEOUT_MS: u64 = version::DEFAULT_TIMEOUT_MS;

/// Default repetition count of every determinism probe.
pub const DEFAULT_REPEATS: u8 = 3;

/// The inclusive lower bound of repetitions: detecting nondeterminism
/// requires at least two runs.
pub const MIN_REPEATS: u8 = 2;

/// The inclusive upper bound of repetitions.
pub const MAX_REPEATS: u8 = 8;

/// The closed profile set of the suite.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    /// The default battery: every core check runs, optional operations
    /// that the adapter does not declare are skipped with a reason.
    Default,
    /// The strict battery adds the complete-surface requirement: a
    /// strict-conformant adapter declares every v1 operation.
    Strict,
}

impl Profile {
    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Strict => "strict",
        }
    }

    /// Parse the stable wire token.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "default" => Some(Self::Default),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }
}
