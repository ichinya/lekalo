//! Stable result, diagnostic, reason-code, and process-exit contracts
//! (issues #3 and #11).
//!
//! [`DomainResult`] is the single envelope every command projects: it alone
//! owns the status, the protocol stream, and the process exit. Since #11 the
//! diagnostics array is authoritative and `reasonCodes` is derived from it
//! (the unique rule ids in normalized order). Severity and category never
//! compute an exit: valid 0/stdout, invalid 1/stderr, denied 3/stdout,
//! unsupported 4/stdout, unsupported-version 5/stderr.

use serde::Serialize;
use std::fmt;

use crate::diagnostics::normalize::build;
use crate::diagnostics::render::diagnostic_lines;
use crate::diagnostics::DiagnosticSet;

/// The stable diagnostic id emitted for malformed command-line syntax.
pub const CLI_USAGE: &str = "cli.usage";

/// The stable diagnostic id emitted by commands whose implementation is not
/// available yet.
pub const CAPABILITY_UNAVAILABLE: &str = "core.capability-unavailable";

/// The stable diagnostic id for an internal envelope invariant failure.
pub const REGISTRY_INVALID: &str = "diagnostics.registry-invalid";

/// The status classes shared by human and JSON output.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Valid,
    Invalid,
    Denied,
    Unsupported,
    /// A needed component is not available in this environment (exit 4).
    Unavailable,
    /// A contract version is outside the accepted registry (exit 5).
    UnsupportedVersion,
}

impl Status {
    /// Return the stable process exit code for this status.
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Valid => 0,
            Self::Invalid => 1,
            Self::Denied => 3,
            Self::Unsupported => 4,
            Self::Unavailable => 4,
            Self::UnsupportedVersion => 5,
        }
    }

    /// Return the stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Denied => "denied",
            Self::Unsupported => "unsupported",
            Self::Unavailable => "unavailable",
            Self::UnsupportedVersion => "unsupported-version",
        }
    }

    /// Failures of this status are written to stderr.
    pub const fn writes_stderr(self) -> bool {
        matches!(self, Self::Invalid | Self::UnsupportedVersion)
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

/// One derived, machine-readable reason code: the unique dotted diagnostic
/// ids in normalized order. Retained alongside `diagnostics` for the v1
/// compatibility window; removal requires a separately versioned envelope
/// migration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ReasonCode(String);

impl ReasonCode {
    pub(crate) fn new(code: impl Into<String>) -> Self {
        Self(code.into())
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

/// The accepted success payload of one command: its exact wire bytes plus
/// the stable human summary. Payload bytes stay producer-owned so the
/// published #7/#9/#10 success contracts remain byte-identical.
#[derive(Clone, Debug, PartialEq)]
pub enum SuccessPayload {
    /// `--version`: `{"status":"valid","version":...}` (pretty).
    Version { version: String },
    /// Loader success (compact envelope bytes built by the loader).
    Model { json: String, human: String },
    /// IR success (compact envelope bytes built by the CLI renderer).
    Ir { json: String, human: String },
    /// Validation success (compact envelope bytes built by the CLI
    /// renderer); may carry the non-blocking validation diagnostics.
    Validation { json: String, human: String },
    /// Graph success (compact envelope bytes built by the CLI renderer);
    /// may carry the non-blocking graph diagnostics.
    Graph { json: String, human: String },
    /// Impact success (compact envelope bytes built by the CLI renderer);
    /// may carry the non-blocking impact diagnostics.
    Impact { json: String, human: String },
    /// Semantic-diff success (compact envelope bytes built by the CLI
    /// renderer); may carry the non-blocking diff diagnostics.
    Diff { json: String, human: String },
    /// Receipt success (pretty two-space JSON without trailing newline).
    Receipt { json: String, human: String },
}

/// The single domain result projected by both CLI renderers.
///
/// JSON field order is normative: `status` first, then the payload or
/// `diagnostics`, then the derived `reasonCodes`.
#[derive(Clone, Debug, PartialEq)]
pub enum DomainResult {
    /// A successful result; may carry only info/warning diagnostics (v1
    /// producers attach none) and omits the fields while empty.
    Valid {
        payload: SuccessPayload,
        diagnostics: Vec<crate::diagnostics::Diagnostic>,
    },
    Invalid {
        diagnostics: DiagnosticSet,
    },
    Denied {
        diagnostics: DiagnosticSet,
    },
    Unavailable {
        diagnostics: DiagnosticSet,
    },
    Unsupported {
        capability: Capability,
        diagnostics: DiagnosticSet,
    },
    UnsupportedVersion {
        diagnostics: DiagnosticSet,
    },
}

impl DomainResult {
    /// The `--version` result.
    pub fn version(version: impl Into<String>) -> Self {
        Self::Valid {
            payload: SuccessPayload::Version {
                version: version.into(),
            },
            diagnostics: Vec::new(),
        }
    }

    /// A loader success with exact compact envelope bytes.
    pub fn model(json: String, human: String) -> Self {
        Self::Valid {
            payload: SuccessPayload::Model { json, human },
            diagnostics: Vec::new(),
        }
    }

    /// An IR success with exact compact envelope bytes.
    pub fn ir(json: String, human: String) -> Self {
        Self::Valid {
            payload: SuccessPayload::Ir { json, human },
            diagnostics: Vec::new(),
        }
    }

    /// A validation success with exact compact envelope bytes and the
    /// non-blocking diagnostics the profile produced (warning/info only).
    pub fn validation(
        json: String,
        human: String,
        diagnostics: Vec<crate::diagnostics::Diagnostic>,
    ) -> Self {
        Self::Valid {
            payload: SuccessPayload::Validation { json, human },
            diagnostics,
        }
    }

    /// A receipt success with pretty bytes (no trailing newline).
    pub fn receipt(json: String, human: String) -> Self {
        Self::Valid {
            payload: SuccessPayload::Receipt { json, human },
            diagnostics: Vec::new(),
        }
    }
    /// A graph success with exact compact envelope bytes and the
    /// non-blocking graph diagnostics (warning/info only).
    pub fn graph(
        json: String,
        human: String,
        diagnostics: Vec<crate::diagnostics::Diagnostic>,
    ) -> Self {
        Self::Valid {
            payload: SuccessPayload::Graph { json, human },
            diagnostics,
        }
    }

    /// An impact success with exact compact envelope bytes and the
    /// non-blocking impact diagnostics (warning/info only).
    pub fn impact(
        json: String,
        human: String,
        diagnostics: Vec<crate::diagnostics::Diagnostic>,
    ) -> Self {
        Self::Valid {
            payload: SuccessPayload::Impact { json, human },
            diagnostics,
        }
    }

    /// A semantic-diff success with exact compact envelope bytes and the
    /// non-blocking diff diagnostics (warning/info only).
    pub fn diff(
        json: String,
        human: String,
        diagnostics: Vec<crate::diagnostics::Diagnostic>,
    ) -> Self {
        Self::Valid {
            payload: SuccessPayload::Diff { json, human },
            diagnostics,
        }
    }

    /// Build a failure set from raw wire diagnostics, falling back to the
    /// single registry invariant diagnostic when the set itself is invalid.
    pub(crate) fn from_wire_set(
        status: Status,
        diagnostics: Vec<crate::diagnostics::Diagnostic>,
    ) -> DiagnosticSet {
        match DiagnosticSet::try_from_unsorted(diagnostics, status) {
            Ok(set) => set,
            Err(_) => fallback_set(),
        }
    }

    pub fn invalid(diagnostics: DiagnosticSet) -> Self {
        Self::Invalid { diagnostics }
    }

    pub fn usage_error() -> Self {
        Self::Invalid {
            diagnostics: singleton_set(CLI_USAGE),
        }
    }

    pub fn denied(diagnostics: DiagnosticSet) -> Self {
        Self::Denied { diagnostics }
    }

    pub fn unavailable(diagnostics: DiagnosticSet) -> Self {
        Self::Unavailable { diagnostics }
    }

    pub fn unsupported(capability: Capability) -> Self {
        Self::Unsupported {
            capability,
            diagnostics: from_ids(&[CAPABILITY_UNAVAILABLE], Status::Unsupported)
                .unwrap_or_else(|_| DiagnosticSet::empty()),
        }
    }

    pub fn unsupported_version(diagnostics: DiagnosticSet) -> Self {
        Self::UnsupportedVersion { diagnostics }
    }

    pub const fn status(&self) -> Status {
        match self {
            Self::Valid { .. } => Status::Valid,
            Self::Invalid { .. } => Status::Invalid,
            Self::Denied { .. } => Status::Denied,
            Self::Unavailable { .. } => Status::Unavailable,
            Self::Unsupported { .. } => Status::Unsupported,
            Self::UnsupportedVersion { .. } => Status::UnsupportedVersion,
        }
    }

    pub const fn exit_code(&self) -> u8 {
        self.status().exit_code()
    }

    /// Failures of this status render on stderr.
    pub const fn writes_stderr(&self) -> bool {
        self.status().writes_stderr()
    }

    pub fn capability(&self) -> Option<Capability> {
        match self {
            Self::Unsupported { capability, .. } => Some(*capability),
            _ => None,
        }
    }

    /// The authoritative diagnostics in normalized order.
    pub fn diagnostics(&self) -> &[crate::diagnostics::Diagnostic] {
        match self {
            Self::Valid { diagnostics, .. } => diagnostics,
            Self::Invalid { diagnostics }
            | Self::Denied { diagnostics }
            | Self::Unavailable { diagnostics }
            | Self::UnsupportedVersion { diagnostics } => diagnostics.as_slice(),
            Self::Unsupported { diagnostics, .. } => diagnostics.as_slice(),
        }
    }

    /// The derived unique ordered diagnostic ids.
    pub fn reason_codes(&self) -> Vec<ReasonCode> {
        let mut ids: Vec<ReasonCode> = Vec::new();
        for diagnostic in self.diagnostics() {
            let id = diagnostic.id();
            if ids.last().map(ReasonCode::as_str) != Some(id) {
                ids.push(ReasonCode::new(id));
            }
        }
        ids
    }

    /// Project the exact JSON envelope bytes (without trailing newline).
    pub fn to_json_string(&self) -> String {
        match self {
            Self::Valid {
                payload,
                diagnostics,
            } => {
                let mut json = match payload {
                    SuccessPayload::Version { version } => format!(
                        "{{\n  \"status\": \"valid\",\n  \"version\": {}\n}}",
                        serde_json::to_string(version).expect("version serializes")
                    ),
                    SuccessPayload::Model { json, .. }
                    | SuccessPayload::Ir { json, .. }
                    | SuccessPayload::Validation { json, .. }
                    | SuccessPayload::Graph { json, .. }
                    | SuccessPayload::Impact { json, .. }
                    | SuccessPayload::Diff { json, .. }
                    | SuccessPayload::Receipt { json, .. } => json.clone(),
                };
                if !diagnostics.is_empty() {
                    let items =
                        serde_json::to_string_pretty(diagnostics).expect("diagnostics serialize");
                    let indented = indent_nested(&items, 1);
                    let ids: Vec<&str> = diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.id())
                        .collect();
                    let insert = format!(
                        ",\n  \"diagnostics\": {},\n  \"reasonCodes\": {}",
                        indented,
                        serde_json::to_string(&ids).expect("ids serialize")
                    );
                    if let Some(position) = json.rfind('}') {
                        json.insert_str(position, &insert);
                    }
                }
                json
            }
            Self::Invalid { .. }
            | Self::Denied { .. }
            | Self::Unavailable { .. }
            | Self::Unsupported { .. }
            | Self::UnsupportedVersion { .. } => {
                #[derive(Serialize)]
                struct FailureEnvelope<'a> {
                    status: &'a str,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    capability: Option<Capability>,
                    #[serde(rename = "diagnostics")]
                    diagnostics: &'a [crate::diagnostics::Diagnostic],
                    #[serde(rename = "reasonCodes")]
                    reason_codes: Vec<&'a str>,
                }
                let reason_codes: Vec<&str> = self
                    .diagnostics()
                    .iter()
                    .map(|diagnostic| diagnostic.id())
                    .collect();
                let envelope = FailureEnvelope {
                    status: self.status().as_str(),
                    capability: self.capability(),
                    diagnostics: self.diagnostics(),
                    reason_codes,
                };
                serde_json::to_string_pretty(&envelope).expect("envelope serializes")
            }
        }
    }

    /// Project the exact human bytes (without trailing newline).
    pub fn to_human_string(&self, program_name: &str) -> String {
        match self {
            Self::Valid {
                payload,
                diagnostics,
            } => {
                let mut lines = match payload {
                    SuccessPayload::Version { version } => {
                        vec![format!("{program_name} {version}")]
                    }
                    SuccessPayload::Model { human, .. }
                    | SuccessPayload::Ir { human, .. }
                    | SuccessPayload::Validation { human, .. }
                    | SuccessPayload::Graph { human, .. }
                    | SuccessPayload::Impact { human, .. }
                    | SuccessPayload::Diff { human, .. }
                    | SuccessPayload::Receipt { human, .. } => human
                        .lines()
                        .map(|line| line.to_owned())
                        .collect::<Vec<_>>(),
                };
                for diagnostic in diagnostics {
                    lines.extend(diagnostic_lines("valid", diagnostic));
                }
                lines.join("\n")
            }
            _ => {
                let status = self.status().as_str();
                let lines = self
                    .diagnostics()
                    .iter()
                    .flat_map(|diagnostic| diagnostic_lines(status, diagnostic))
                    .collect::<Vec<_>>();
                lines.join("\n")
            }
        }
    }
}

/// Indent every line but the first of an embedded pretty JSON fragment so it
/// nests at the envelope's two-space depth.
fn indent_nested(fragment: &str, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    let mut lines = fragment.lines();
    let first = lines.next().unwrap_or_default().to_owned();
    let mut out = first;
    for line in lines {
        out.push('\n');
        out.push_str(&pad);
        out.push_str(line);
    }
    out
}

/// Build one registered singleton set; a registry failure collapses to an
/// empty set rather than panicking (double developer fault).
pub(crate) fn singleton_set(id: &str) -> DiagnosticSet {
    from_ids(&[id], Status::Invalid).unwrap_or_else(|_| DiagnosticSet::empty())
}

/// The last-resort set when even the fallback diagnostic cannot be built.
pub(crate) fn fallback_set() -> DiagnosticSet {
    DiagnosticSet::empty()
}

/// Build a validated set from rule ids with no data (test helper).
pub(crate) fn from_ids(
    ids: &[&str],
    status: Status,
) -> Result<DiagnosticSet, crate::diagnostics::SetError> {
    let mut diagnostics = Vec::with_capacity(ids.len());
    for id in ids {
        diagnostics.push(
            build(id, None, None, Default::default())
                .map_err(|_| crate::diagnostics::SetError::UnknownRule((*id).to_owned()))?,
        );
    }
    DiagnosticSet::try_from_unsorted(diagnostics, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_follow_status_not_severity() {
        assert_eq!(Status::Valid.exit_code(), 0);
        assert_eq!(Status::Invalid.exit_code(), 1);
        assert_eq!(Status::Denied.exit_code(), 3);
        assert_eq!(Status::Unsupported.exit_code(), 4);
        assert_eq!(Status::UnsupportedVersion.exit_code(), 5);
        assert!(!Status::Denied.writes_stderr());
        assert!(Status::UnsupportedVersion.writes_stderr());
    }

    #[test]
    fn version_payload_matches_the_published_bytes() {
        let result = DomainResult::version("0.2.12");
        assert_eq!(
            result.to_json_string(),
            "{\n  \"status\": \"valid\",\n  \"version\": \"0.2.12\"\n}"
        );
        assert_eq!(result.to_human_string("lekalo"), "lekalo 0.2.12");
    }

    #[test]
    fn usage_failure_carries_the_derived_reason_code() {
        let result = DomainResult::usage_error();
        assert_eq!(result.status(), Status::Invalid);
        assert_eq!(result.reason_codes(), vec![ReasonCode::new("cli.usage")]);
        let json = result.to_json_string();
        assert!(json.starts_with("{\n  \"status\": \"invalid\",\n  \"diagnostics\": ["));
        assert!(json.ends_with("  \"reasonCodes\": [\n    \"cli.usage\"\n  ]\n}"));
        assert_eq!(
            result.to_human_string("lekalo"),
            "invalid error [LEK-CLI-001] cli.usage: Malformed command-line syntax."
        );
    }
}
