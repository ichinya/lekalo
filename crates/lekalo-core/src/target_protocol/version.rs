//! The identity, wire, and bound constants of the target protocol
//! (issues #27 and #28).
//!
//! The wire token `lekalo.target/v1` names the protocol line. The line
//! carries two exact contract versions: the base `1.0.0` envelope every
//! v1-line adapter accepts, and the current `1.1.0` whose describe
//! response additively extends the negotiated capabilities. The client
//! probes at the base version and upgrades only to a version the adapter
//! explicitly declared, so the session always runs on a version both
//! sides named. The identity follows the house
//! `dev.lekalo.<topic>@<version>` spelling. Every bound here has a
//! matching JSON Schema constraint; the paired test pins them together.

/// The stable wire token of the target protocol line.
pub const PROTOCOL_TOKEN: &str = "lekalo.target/v1";

/// The base wire version (`1.0.0`): the describe probe version every
/// v1-line adapter accepts, and the frozen published contract document.
pub const BASE_VERSION: &str = "1.0.0";

/// The current protocol contract version (`dev.lekalo.protocol@1.1.0`):
/// the additive describe-response extension negotiated by issue #28.
pub const VERSION: &str = "1.1.0";

/// The closed, ascending set of protocol versions this core decodes and
/// negotiates. The registry may only publish versions from this set;
/// anything else is a registry/decoder drift refused as a developer
/// fault before any adapter is launched.
pub const SUPPORTED_VERSIONS: [&str; 2] = ["1.0.0", "1.1.0"];

/// The identity of the schema artifact for the current protocol version.
pub const IDENTITY: &str = "dev.lekalo.target-protocol@1.1.0";

/// The schema identity of the current wire contract
/// (`lekalo/target-protocol/v1.1.0`).
pub const SCHEMA_VERSION: &str = "lekalo/target-protocol/v1.1.0";

/// Whether one exact spelling is in the supported negotiation set.
pub fn is_supported_version(value: &str) -> bool {
    SUPPORTED_VERSIONS.contains(&value)
}

/// The highest supported spelling present in a declared set, in the
/// ascending order of [`SUPPORTED_VERSIONS`].
pub fn negotiate(declared: &[String]) -> Option<&'static str> {
    SUPPORTED_VERSIONS
        .iter()
        .rev()
        .find(|candidate| declared.iter().any(|version| version == *candidate))
        .copied()
}

/// Prefix of every deterministic request identifier.
pub const REQUEST_ID_PREFIX: &str = "req-";

/// Prefix of every deterministic plan identifier.
pub const PLAN_ID_PREFIX: &str = "plan-";

/// Maximum serialized request the client will build or hand to a child.
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;

/// Default operation deadline in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 600_000;

/// Default cap for the response envelope read from stdout.
pub const DEFAULT_MAX_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;

/// Cap for the captured stderr diagnostics stream.
pub const MAX_STDERR_BYTES: usize = 64 * 1024;

/// Maximum number of entries in one declared output plan or write report.
pub const MAX_PLAN_ENTRIES: usize = 10_000;

/// Maximum number of files one write-scope snapshot may observe before the
/// scope is refused as unverifiable.
pub const MAX_SCOPED_FILES: usize = 4_096;

/// Maximum byte size of one file read for digest verification.
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;

/// Maximum number of declared scopes per side (read/write).
pub const MAX_SCOPES: usize = 64;

/// Maximum number of declared IR contract versions in one describe.
pub const MAX_IR_VERSIONS: usize = 8;

/// Maximum number of named capability support states in one describe.
pub const MAX_DECLARED_CAPABILITIES: usize = 64;

/// Maximum number of declared optional constraints in one describe.
pub const MAX_CONSTRAINTS: usize = 8;

/// Inclusive upper bound of one declared optional constraint value.
pub const MAX_CONSTRAINT_VALUE: u64 = 1_073_741_824;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_the_published_contract_version() {
        assert_eq!(PROTOCOL_TOKEN, "lekalo.target/v1");
        assert_eq!(BASE_VERSION, "1.0.0");
        assert_eq!(VERSION, "1.1.0");
        assert_eq!(IDENTITY, "dev.lekalo.target-protocol@1.1.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/target-protocol/v1.1.0");
    }

    #[test]
    fn negotiation_prefers_the_highest_declared_supported_version() {
        let declared = |values: &[&str]| -> Option<String> {
            negotiate(
                &values
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>(),
            )
            .map(str::to_owned)
        };
        assert_eq!(negotiate(&[]), None);
        assert_eq!(
            declared(&["0.9.0", "2.0.0"]).as_deref(),
            None,
            "unsupported spellings never negotiate"
        );
        assert_eq!(declared(&["1.0.0"]).as_deref(), Some("1.0.0"));
        assert_eq!(declared(&["1.1.0"]).as_deref(), Some("1.1.0"));
        assert_eq!(
            declared(&["1.1.0", "1.0.0"]).as_deref(),
            Some("1.1.0"),
            "declaration order never decides the negotiated version"
        );
        assert!(is_supported_version("1.0.0") && is_supported_version("1.1.0"));
        assert!(!is_supported_version("1.0.1") && !is_supported_version("0.9.0"));
    }

    #[test]
    fn limits_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_REQUEST_BYTES, 1_048_576);
        assert_eq!(DEFAULT_TIMEOUT_MS, 600_000);
        assert_eq!(DEFAULT_MAX_OUTPUT_BYTES, 8_388_608);
        assert_eq!(MAX_STDERR_BYTES, 65_536);
        assert_eq!(MAX_PLAN_ENTRIES, 10_000);
        assert_eq!(MAX_SCOPED_FILES, 4_096);
        assert_eq!(MAX_FILE_BYTES, 4 * 1024 * 1024);
        assert_eq!(MAX_SCOPES, 64);
        assert_eq!(MAX_IR_VERSIONS, 8);
        assert_eq!(MAX_DECLARED_CAPABILITIES, 64);
        assert_eq!(MAX_CONSTRAINTS, 8);
        assert_eq!(MAX_CONSTRAINT_VALUE, 1_073_741_824);
    }
}
