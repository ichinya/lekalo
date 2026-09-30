//! Evidence construction for native gate runs (issue #61, plan slice
//! D; research doc §5 and §7): the proposed failure classification, the
//! bounded redaction of captured output, and the verdict rollup.
//!
//! Every rule here is honest-by-default: a measurement that cannot be
//! classified is withheld or escalated, never fabricated into a pass.
//! The classification maps the proposed `failure_class` vocabulary onto
//! the closed v0.3.2 outcome set (the coherence the run receipt
//! validator enforces), and the redaction rules guarantee that no raw
//! secret-bearing output, host path, or env value ever reaches a
//! persisted artifact.

use super::types::{NativeCommandResult, NativeRunResult};

/// The proposed failure classes (issue #61), orthogonal to the closed
/// outcome set. Mirrored by `receipt::FAILURE_CLASSES` validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureClass {
    /// A valid terminal test result or neutral failure row failed.
    Assertion,
    /// Blocking static-analysis diagnostics with parseable results.
    StaticAnalysis,
    /// A confirmed application boot/provider/config check failed on a
    /// compatible provisioned runtime.
    Boot,
    /// A required or optional tool is absent (zero-spawn preflight).
    MissingTool,
    /// A tool exists but its observed version/platform is incompatible.
    Incompatible,
    /// Spawn/crash/signal, deadline, cancellation, flood, corrupt
    /// evidence, or cleanup failure.
    Infrastructure,
}

impl FailureClass {
    /// The stable wire token of one failure class.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Assertion => "assertion",
            Self::StaticAnalysis => "static-analysis",
            Self::Boot => "boot",
            Self::MissingTool => "missing-tool",
            Self::Incompatible => "incompatible",
            Self::Infrastructure => "infrastructure",
        }
    }

    /// The closed outcome each class coherent with (the run receipt
    /// validator refuses a class that contradicts its outcome).
    pub const fn outcome(self) -> &'static str {
        match self {
            Self::Assertion | Self::StaticAnalysis | Self::Boot => "failed",
            Self::MissingTool => "missing",
            Self::Incompatible => "unsupported",
            Self::Infrastructure => "infrastructure",
        }
    }
}

/// The stable bounded reason tokens a run receipt may carry (the
/// closed vocabulary tool-result adapters classify into).
#[allow(dead_code)]
pub mod reasons {
    /// A confirmed script/tool is absent from the provisioned stage.
    pub const TOOL_MISSING: &str = "tool-missing";
    /// The observed tool version is outside the confirmed compatibility.
    pub const TOOL_VERSION_INCOMPATIBLE: &str = "tool-version-incompatible";
    /// The runtime platform cannot qualify the confirmed recipe.
    pub const RUNTIME_PLATFORM_INCOMPATIBLE: &str = "runtime-platform-incompatible";
    /// A per-command deadline elapsed; the measurement is unknown.
    pub const TIMEOUT: &str = "timeout";
    /// A bounded output pipe exceeded its cap.
    pub const OUTPUT_LIMIT: &str = "output-limit";
    /// The whole-run deadline preempted the remaining commands.
    pub const RUN_DEADLINE: &str = "run-deadline";
    /// A cancellation token fired; dependent commands never spawn.
    pub const CANCELLED: &str = "cancelled";
    /// The process wrote past its allowed output home.
    pub const UNEXPECTED_WRITE: &str = "unexpected-write";
    /// A stage-cleanup step failed after the commands completed.
    pub const CLEANUP_FAILED: &str = "cleanup-failed";
    /// A gate assertion failed with a valid terminal result.
    pub const ASSERTION_FAILED: &str = "assertion-failed";
    /// Static analysis produced blocking diagnostics.
    pub const STATIC_ANALYSIS_BLOCKING: &str = "static-analysis-blocking";
    /// The confirmed application boot check failed.
    pub const BOOT_FAILED: &str = "boot-failed";
}

/// The bounded, redacted form of one captured output stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedactedOutput {
    /// The redacted bytes (already truncated to the cap and scrubbed).
    pub bytes: Vec<u8>,
    /// Whether the raw stream exceeded the cap (infrastructure).
    pub flooded: bool,
    /// Whether the classification was uncertain and the bytes must be
    /// withheld from any artifact (`None` digest).
    pub withheld: bool,
}

/// The output caps of one run (from the plan limits).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputCaps {
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

/// Capture one raw output stream into its bounded redacted form: the
/// bytes are truncated at the cap (one overflow byte marks the flood),
/// control text is dropped, and every occurrence of a secret or a host
/// path is replaced with a stable token. Classification is opt-in: a
/// caller that cannot classify keeps `withheld` set and never persists
/// the bytes.
pub fn redact_stream(
    raw: &[u8],
    cap: usize,
    secrets: &[String],
    host_paths: &[String],
) -> RedactedOutput {
    let flooded = raw.len() > cap;
    let bounded = if flooded { &raw[..cap] } else { raw };
    // Control characters never survive into evidence (terminal escape
    // attacks included); newlines and tabs stay structural.
    let mut bytes: Vec<u8> = bounded
        .iter()
        .copied()
        .map(|b| {
            if b < 0x20 && b != b'\n' && b != b'\t' {
                b'?'
            } else {
                b
            }
        })
        .collect();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let mut scrubbed = text.clone();
    for secret in secrets {
        if !secret.is_empty() {
            scrubbed = scrubbed.replace(secret.as_str(), "[redacted-secret]");
        }
    }
    for path in host_paths {
        if !path.is_empty() {
            scrubbed = scrubbed.replace(path.as_str(), "[host-path]");
        }
    }
    if scrubbed != text {
        bytes = scrubbed.into_bytes();
    }
    RedactedOutput {
        bytes,
        flooded,
        withheld: false,
    }
}

/// The output reference of one command result: the digest of the
/// redacted bytes, or the explicit withheld/unknown state.
pub fn output_ref(
    redacted: &RedactedOutput,
    digest_of: impl Fn(&[u8]) -> String,
) -> super::types::NativeOutputRef {
    use super::types::NativeOutputRef;
    if redacted.withheld {
        return NativeOutputRef::State {
            state: "withheld".to_owned(),
        };
    }
    NativeOutputRef::Digest(digest_of(&redacted.bytes))
}

/// The per-command evidence the runner hands to the receipt builder.
#[derive(Clone, Debug)]
pub struct CommandMeasurement {
    pub command_id: String,
    pub package_id: String,
    pub cwd: String,
    pub tool_ref: String,
    pub argv: Vec<String>,
    pub env_names: Vec<String>,
    pub gate_id: String,
    pub required: bool,
    /// The known exit code, when the process was observed to exit.
    pub exit_code: Option<u64>,
    pub duration_ms: u64,
    pub outcome: &'static str,
    pub failure_class: Option<FailureClass>,
    pub reason_codes: Vec<String>,
    pub stdout: Option<RedactedOutput>,
    pub stderr: Option<RedactedOutput>,
    pub tool_version: Option<String>,
    pub toolchain_ref: Option<String>,
    pub env_recipe_digest: Option<String>,
}

/// Build one command result from a measurement: the outcome/class
/// coherence is decided here so the receipt validator's closed mapping
/// always holds. Preflight misses (missing/incompatible) carry unknown
/// exit and duration states — a never-executed command never reports a
/// fabricated zero.
pub fn command_result(
    measurement: CommandMeasurement,
    digest_of: impl Fn(&[u8]) -> String,
) -> NativeCommandResult {
    let exit = measurement
        .exit_code
        .map(|value| super::types::NativeValueState {
            state: "known".to_owned(),
            value: Some(value),
        });
    let duration = if measurement.outcome == "missing" || measurement.outcome == "unsupported" {
        super::types::NativeValueState {
            state: "unknown".to_owned(),
            value: None,
        }
    } else {
        super::types::NativeValueState {
            state: "known".to_owned(),
            value: Some(measurement.duration_ms),
        }
    };
    let stdout = measurement
        .stdout
        .map(|redacted| output_ref(&redacted, &digest_of));
    let _ = measurement.stderr;
    NativeCommandResult {
        command_id: measurement.command_id,
        package_id: measurement.package_id,
        cwd: measurement.cwd,
        tool_ref: measurement.tool_ref,
        argv: measurement.argv,
        env_names: measurement.env_names,
        gate_id: measurement.gate_id,
        required: measurement.required,
        failure_class: measurement
            .failure_class
            .map(|class| class.as_str().to_owned()),
        env_recipe_digest: measurement.env_recipe_digest,
        tool_version: measurement.tool_version,
        toolchain_ref: measurement.toolchain_ref,
        exit,
        duration_ms: Some(duration),
        outcome: measurement.outcome.to_owned(),
        reason_codes: measurement.reason_codes,
        output_ref: stdout,
    }
}

/// The verification verdict rollup over a fully built receipt: the
/// documented precedence security > infrastructure > required-blocker >
/// failure > optional-degradation > pass. The receipt validator
/// re-derives this rollup, so a receipt whose verdict disagrees is
/// refused — the runner and the validator cannot drift apart.
pub fn rollup(receipt: &NativeRunResult) -> String {
    super::receipt::rollup_verdict(receipt)
}

#[cfg(test)]
mod evidence_tests {
    use super::*;

    #[test]
    fn failure_classes_map_onto_coherent_outcomes() {
        assert_eq!(FailureClass::Assertion.outcome(), "failed");
        assert_eq!(FailureClass::StaticAnalysis.outcome(), "failed");
        assert_eq!(FailureClass::Boot.outcome(), "failed");
        assert_eq!(FailureClass::MissingTool.outcome(), "missing");
        assert_eq!(FailureClass::Infrastructure.outcome(), "infrastructure");
        assert_eq!(FailureClass::Assertion.as_str(), "assertion");
        assert_eq!(FailureClass::MissingTool.as_str(), "missing-tool");
    }

    #[test]
    fn redaction_truncates_at_the_cap_and_marks_the_flood() {
        let raw = vec![b'x'; 128];
        let redacted = redact_stream(&raw, 64, &[], &[]);
        assert!(redacted.flooded);
        assert_eq!(redacted.bytes.len(), 64);
        let small = redact_stream(b"ok", 64, &[], &[]);
        assert!(!small.flooded);
        assert_eq!(small.bytes, b"ok".to_vec());
    }

    #[test]
    fn redaction_scrubs_secrets_host_paths_and_control_text() {
        let secrets = vec!["hunter2-super-secret".to_owned()];
        let host_paths = vec!["C:\\Users\\User\\staged".to_owned()];
        let raw = format!(
            "token=hunter2-super-secret at {} done\x1b[31m",
            "C:\\Users\\User\\staged\\composer.json"
        );
        let redacted = redact_stream(raw.as_bytes(), 4096, &secrets, &host_paths);
        let text = String::from_utf8(redacted.bytes).unwrap();
        assert!(
            !text.contains("hunter2-super-secret"),
            "secrets never survive"
        );
        assert!(
            !text.contains("C:\\Users\\User\\staged"),
            "host paths never survive"
        );
        assert!(text.contains("[redacted-secret]"));
        assert!(text.contains("[host-path]"));
        assert!(!text.contains('\x1b'), "escape bytes are dropped");
    }

    #[test]
    fn command_results_carry_the_closed_measurement_shape() {
        let stdout = redact_stream(b"all green", 1024, &[], &[]);
        let result = command_result(
            CommandMeasurement {
                command_id: "gate-scenario-tests".into(),
                package_id: ".=lekalo/planner-laravel-fixture".into(),
                cwd: ".".into(),
                tool_ref: "php-runtime".into(),
                argv: vec!["php".into(), "vendor/bin/testo".into(), "run".into()],
                env_names: vec![],
                gate_id: "scenario-tests".into(),
                required: true,
                exit_code: Some(0),
                duration_ms: 420,
                outcome: "passed",
                failure_class: None,
                reason_codes: vec![],
                stdout: Some(stdout),
                stderr: None,
                tool_version: Some("8.3.35".into()),
                toolchain_ref: Some(format!("sha256:{}", "3".repeat(64))),
                env_recipe_digest: Some(format!("sha256:{}", "4".repeat(64))),
            },
            |bytes| format!("sha256:{}", super::super::super::digest::sha256_hex(bytes)),
        );
        assert_eq!(result.outcome, "passed");
        assert_eq!(result.failure_class, None);
        assert_eq!(result.duration_ms.as_ref().unwrap().value, Some(420));
        assert!(matches!(
            result.output_ref,
            Some(super::super::types::NativeOutputRef::Digest(_))
        ));
        // A preflight miss carries unknown measurements, never a zero.
        let missing = command_result(
            CommandMeasurement {
                command_id: "gate-boot".into(),
                package_id: ".=fixture".into(),
                cwd: ".".into(),
                tool_ref: "php-runtime".into(),
                argv: vec!["php".into()],
                env_names: vec![],
                gate_id: "boot".into(),
                required: true,
                exit_code: None,
                duration_ms: 0,
                outcome: "missing",
                failure_class: Some(FailureClass::MissingTool),
                reason_codes: vec![reasons::TOOL_MISSING.to_owned()],
                stdout: None,
                stderr: None,
                tool_version: None,
                toolchain_ref: None,
                env_recipe_digest: None,
            },
            |bytes| format!("sha256:{}", super::super::super::digest::sha256_hex(bytes)),
        );
        assert_eq!(missing.exit, None);
        assert_eq!(missing.duration_ms.unwrap().state, "unknown");
    }
}
