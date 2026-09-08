//! Issue #36 CLI tests for the `lekalo requirements` handoff: the gate
//! envelope on the hermetic planner fixture, the canonical report and
//! trace exports, the closed query selectors, the denied/unavailable
//! exit protocol, and the version custody probe.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

/// The selection policy denies alias-spelled working directories
/// (`structure.selection-alias`) before any command logic runs; chdir
/// the child into the resolved spelling, stripped of the `\\?\` verbatim
/// prefix `canonicalize` produces on Windows.
fn alias_free_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("fixture path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        // `\\?\C:\...` -> `C:\...`; UNC (`\\?\UNC\...`) stays verbatim.
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn exit_code(output: &Output) -> u8 {
    output.status.code().expect("exit code") as u8
}

fn fixture_path() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/requirements/planner"),
    )
}

#[test]
fn validate_gate_accepts_the_fresh_fixture() {
    let fixture = fixture_path();
    let human = lekalo_in(
        &fixture,
        &[
            "requirements",
            "validate",
            "requirements.attachment.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&human), 0, "{}", stderr_text(&human));
    assert!(
        human
            .stdout
            .windows(b"references 3".len())
            .any(|window| window == b"references 3"),
        "{}",
        stdout_text(&human)
    );
    let json = lekalo_in(
        &fixture,
        &[
            "--json",
            "requirements",
            "validate",
            "requirements.attachment.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&json), 0);
    let envelope: serde_json::Value =
        serde_json::from_str(stdout_text(&json).trim()).expect("envelope json");
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["requirements"]["fresh"], 3);
    assert_eq!(envelope["requirements"]["coverageGaps"], 0);
}

#[test]
fn report_and_trace_emit_schema_valid_canonical_bytes() {
    let fixture = fixture_path();
    let report = lekalo_in(
        &fixture,
        &[
            "requirements",
            "report",
            "requirements.attachment.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&report), 0, "{}", stderr_text(&report));
    let parsed: serde_json::Value =
        serde_json::from_str(stdout_text(&report).trim()).expect("report json");
    assert_eq!(parsed["identity"], "dev.lekalo.requirements-report@1.0.0");

    let trace = lekalo_in(
        &fixture,
        &[
            "requirements",
            "trace",
            "requirements.attachment.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&trace), 0, "{}", stderr_text(&trace));
    let manifest: serde_json::Value =
        serde_json::from_str(stdout_text(&trace).trim()).expect("trace json");
    assert_eq!(manifest["schemaVersion"], "lekalo/trace-manifest/v1.0.0");
    assert_eq!(manifest["completeness"], "partial");
    assert_eq!(manifest["manifestId"], "requirements-trace");
}

#[test]
fn query_answers_the_closed_selectors() {
    let fixture = fixture_path();
    let symbol = lekalo_in(
        &fixture,
        &[
            "requirements",
            "query",
            "requirements.attachment.json",
            "symbol:planner.focus_task",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&symbol), 0, "{}", stderr_text(&symbol));
    assert!(
        stdout_text(&symbol).contains("openspec:planner.REQ-focus-task derived_from fresh"),
        "{}",
        stdout_text(&symbol)
    );

    let unknown = lekalo_in(
        &fixture,
        &[
            "requirements",
            "query",
            "requirements.attachment.json",
            "symbol:planner.not_a_symbol",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&unknown), 1, "unknown subject is invalid");

    let selector = lekalo_in(
        &fixture,
        &[
            "requirements",
            "query",
            "requirements.attachment.json",
            "requirements-for",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&selector), 1, "unknown selector is usage");
}

#[test]
fn missing_attachment_is_invalid_not_a_crash() {
    let fixture = fixture_path();
    let output = lekalo_in(
        &fixture,
        &[
            "requirements",
            "validate",
            "does-not-exist.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 1);
    assert!(
        stderr_text(&output).contains("requirements.document-invalid"),
        "{}",
        stderr_text(&output)
    );
}

#[test]
fn version_custody_probe() {
    let output = lekalo_in(&fixture_path(), &["--version"]);
    assert_eq!(exit_code(&output), 0);
    assert_eq!(stdout_text(&output).trim(), "lekalo 0.2.1");
    let json = lekalo_in(&fixture_path(), &["--json", "--version"]);
    assert_eq!(exit_code(&json), 0);
    assert_eq!(
        stdout_text(&json).trim(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.2.1\"\n}"
    );
}
