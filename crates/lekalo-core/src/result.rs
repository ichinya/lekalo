//! Stable result, reason-code, and process-exit contracts.

use serde::Serialize;
use std::fmt;

/// The stable reason emitted for malformed command-line syntax.
pub const CLI_USAGE: &str = "cli.usage";

/// The stable reason emitted by commands whose implementation is not available yet.
pub const CAPABILITY_UNAVAILABLE: &str = "core.capability-unavailable";

/// The status classes shared by human and JSON output.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Valid,
    Invalid,
    Denied,
    Unsupported,
}

impl Status {
    /// Return the stable process exit code for this status.
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Valid => 0,
            Self::Invalid => 1,
            Self::Denied => 3,
            Self::Unsupported => 4,
        }
    }

    /// Return the stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Denied => "denied",
            Self::Unsupported => "unsupported",
        }
    }
}

/// A CLI capability represented without target- or provider-specific state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Validate,
    Inspect,
    Impact,
    Context,
}

impl Capability {
    /// Return the stable wire spelling of the capability.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::Inspect => "inspect",
            Self::Impact => "impact",
            Self::Context => "context",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One stable, machine-readable reason code.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ReasonCode(String);

impl ReasonCode {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn cli_usage() -> Self {
        Self::new(CLI_USAGE)
    }

    pub fn capability_unavailable() -> Self {
        Self::new(CAPABILITY_UNAVAILABLE)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ReasonCode {
    fn from(code: &str) -> Self {
        Self::new(code)
    }
}

/// A single domain result projected by both CLI renderers.
///
/// The enum representation deliberately fixes JSON field order: `status`
/// first, then variant fields in declaration order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum DomainResult {
    Valid {
        version: String,
    },
    Invalid {
        #[serde(rename = "reasonCodes")]
        reason_codes: Vec<ReasonCode>,
    },
    Denied {
        #[serde(rename = "reasonCodes")]
        reason_codes: Vec<ReasonCode>,
    },
    Unsupported {
        capability: Capability,
        #[serde(rename = "reasonCodes")]
        reason_codes: Vec<ReasonCode>,
    },
}

impl DomainResult {
    pub fn version(version: impl Into<String>) -> Self {
        Self::Valid {
            version: version.into(),
        }
    }

    pub fn invalid(reason_codes: Vec<ReasonCode>) -> Self {
        Self::Invalid { reason_codes }
    }

    pub fn usage_error() -> Self {
        Self::invalid(vec![ReasonCode::cli_usage()])
    }

    pub fn denied(reason_codes: Vec<ReasonCode>) -> Self {
        Self::Denied { reason_codes }
    }

    pub fn unsupported(capability: Capability) -> Self {
        Self::Unsupported {
            capability,
            reason_codes: vec![ReasonCode::capability_unavailable()],
        }
    }

    pub const fn status(&self) -> Status {
        match self {
            Self::Valid { .. } => Status::Valid,
            Self::Invalid { .. } => Status::Invalid,
            Self::Denied { .. } => Status::Denied,
            Self::Unsupported { .. } => Status::Unsupported,
        }
    }

    pub const fn exit_code(&self) -> u8 {
        self.status().exit_code()
    }

    pub fn reason_codes(&self) -> &[ReasonCode] {
        match self {
            Self::Valid { .. } => &[],
            Self::Invalid { reason_codes }
            | Self::Denied { reason_codes }
            | Self::Unsupported { reason_codes, .. } => reason_codes,
        }
    }

    pub const fn capability(&self) -> Option<Capability> {
        match self {
            Self::Unsupported { capability, .. } => Some(*capability),
            _ => None,
        }
    }

    /// Project the domain result into one deterministic human-readable line.
    pub fn human_line(&self, program_name: &str) -> String {
        match self {
            Self::Valid { version } => format!("{program_name} {version}"),
            Self::Invalid { reason_codes } => status_with_reasons("invalid", reason_codes),
            Self::Denied { reason_codes } => status_with_reasons("denied", reason_codes),
            Self::Unsupported {
                capability,
                reason_codes,
            } => {
                let status = status_with_reasons("unsupported", reason_codes);
                format!("{status} {capability}")
            }
        }
    }
}

fn status_with_reasons(status: &str, reason_codes: &[ReasonCode]) -> String {
    if reason_codes.is_empty() {
        status.to_owned()
    } else {
        let reasons = reason_codes
            .iter()
            .map(ReasonCode::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{status}: {reasons}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_exit_mapping_is_exhaustive() {
        let cases = [
            (Status::Valid, 0),
            (Status::Invalid, 1),
            (Status::Denied, 3),
            (Status::Unsupported, 4),
        ];

        for (status, exit_code) in cases {
            assert_eq!(status.exit_code(), exit_code);
        }
    }

    #[test]
    fn every_result_variant_has_an_exact_json_snapshot() {
        let cases = [
            (
                DomainResult::version("0.1.3"),
                "{\n  \"status\": \"valid\",\n  \"version\": \"0.1.3\"\n}",
            ),
            (
                DomainResult::usage_error(),
                "{\n  \"status\": \"invalid\",\n  \"reasonCodes\": [\n    \"cli.usage\"\n  ]\n}",
            ),
            (
                DomainResult::denied(vec![ReasonCode::new("policy.denied")]),
                "{\n  \"status\": \"denied\",\n  \"reasonCodes\": [\n    \"policy.denied\"\n  ]\n}",
            ),
            (
                DomainResult::unsupported(Capability::Inspect),
                "{\n  \"status\": \"unsupported\",\n  \"capability\": \"inspect\",\n  \"reasonCodes\": [\n    \"core.capability-unavailable\"\n  ]\n}",
            ),
        ];

        for (result, expected) in cases {
            assert_eq!(serde_json::to_string_pretty(&result).unwrap(), expected);
        }
    }

    #[test]
    fn every_result_variant_has_an_exact_human_projection() {
        let cases = [
            (DomainResult::version("0.1.3"), "lekalo 0.1.3"),
            (DomainResult::usage_error(), "invalid: cli.usage"),
            (
                DomainResult::denied(vec![ReasonCode::new("policy.denied")]),
                "denied: policy.denied",
            ),
            (
                DomainResult::unsupported(Capability::Inspect),
                "unsupported: core.capability-unavailable inspect",
            ),
        ];

        for (result, expected) in cases {
            assert_eq!(result.human_line("lekalo"), expected);
        }
    }
}
