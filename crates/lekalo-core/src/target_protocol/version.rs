//! The identity, wire, and bound constants of the target protocol (issue #27).
//!
//! The wire token `lekalo.target/v1` names the protocol line; the exact
//! contract version `1.0.0` is what the version registry publishes and what
//! request and response envelopes negotiate. The identity follows the house
//! `dev.lekalo.<topic>@<version>` spelling. Every bound here has a matching
//! JSON Schema constraint; the paired test pins them together.

/// The stable wire token of the target protocol line.
pub const PROTOCOL_TOKEN: &str = "lekalo.target/v1";

/// The exact published protocol contract version (`dev.lekalo.protocol@1.0.0`).
pub const VERSION: &str = "1.0.0";

/// The identity of the schema artifact for this protocol version.
pub const IDENTITY: &str = "dev.lekalo.target-protocol@1.0.0";

/// The schema identity of the wire contract (`lekalo/target-protocol/v1.0.0`).
pub const SCHEMA_VERSION: &str = "lekalo/target-protocol/v1.0.0";

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_the_published_contract_version() {
        assert_eq!(PROTOCOL_TOKEN, "lekalo.target/v1");
        assert_eq!(VERSION, "1.0.0");
        assert_eq!(IDENTITY, "dev.lekalo.target-protocol@1.0.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/target-protocol/v1.0.0");
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
    }
}
