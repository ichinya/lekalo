//! Issue #31 CLI tests for `lekalo adapter test`: the thin handoff to
//! the conformance engine — exit codes, report documents, and usage
//! failures — against the committed reference adapter.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn adapter_script() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/target-protocol/fake-adapter.mjs")
        .display()
        .to_string()
}

fn run_adapter(args: &[&str]) -> Output {
    let mut argv: Vec<&str> = vec!["adapter", "test"];
    argv.extend(args);
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(&argv)
        .output()
        .expect("run the real lekalo binary")
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

/// The resolved spelling of the adapter script: never a Windows 8.3
/// alias spelling.
fn alias_free(path: &str) -> String {
    let canonical = PathBuf::from(path).canonicalize().expect("script exists");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => return rest.to_owned(),
        _ => {}
    }
    canonical.display().to_string()
}

#[test]
fn adapter_test_without_a_program_is_the_usage_failure() {
    let output = run_adapter(&[]);
    assert_eq!(exit_code(&output), 1);
    let stderr = stderr_text(&output);
    assert!(stderr.contains("cli.usage"), "usage failure: {stderr}");
}

#[test]
fn adapter_test_rejects_an_unknown_profile_token() {
    let script = alias_free(&adapter_script());
    let args: Vec<String> = vec!["--profile".into(), "stricter".into(), "node".into(), script];
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = run_adapter(&refs);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("cli.usage"));
}

#[test]
fn the_reference_adapter_passes_with_a_human_verdict() {
    let script = alias_free(&adapter_script());
    let args: Vec<String> = vec!["--timeout-ms".into(), "30000".into(), "node".into(), script];
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = run_adapter(&refs);
    assert_eq!(exit_code(&output), 0);
    let stdout = stdout_text(&output);
    assert!(stdout.contains("verdict pass"), "{stdout}");
    assert!(stdout.contains("badge verified protocol=0.3.2 ir=0.2.16"));
}

#[test]
fn the_json_report_is_a_valid_envelope_with_an_issued_badge() {
    let script = alias_free(&adapter_script());
    let args: Vec<String> = vec![
        "--report".into(),
        "json".into(),
        "--timeout-ms".into(),
        "30000".into(),
        "node".into(),
        script,
    ];
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = run_adapter(&refs);
    assert_eq!(exit_code(&output), 0);
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("json report");
    assert_eq!(document["status"], "valid");
    assert_eq!(
        document["report"]["schema_version"],
        "lekalo/adapter-conformance/v0.2.16"
    );
    assert_eq!(document["report"]["badge"]["issued"], true);
    assert_eq!(document["report"]["badge"]["protocol"], "0.3.2");
}

#[test]
fn the_junit_report_prints_for_failing_runs_too() {
    let script = alias_free(&adapter_script());
    let args: Vec<String> = vec![
        "--report".into(),
        "junit".into(),
        "--timeout-ms".into(),
        "10000".into(),
        "--repeats".into(),
        "2".into(),
        "node".into(),
        script,
        "--lekalo-fault".into(),
        "crash".into(),
    ];
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = run_adapter(&refs);
    assert_eq!(exit_code(&output), 4);
    let stdout = stdout_text(&output);
    assert!(stdout.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(stdout.contains("<failure"));
    assert!(stdout.contains("<skipped"));
}

#[test]
fn the_battery_checks_generic_evidence_from_the_synthetic_adapter() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/adapter-conformance/evidence-adapter.cjs");
    let script = alias_free(path.to_str().unwrap());
    for (mode, expected_exit, expected_state) in [
        ("valid", 0, "pass"),
        ("targetless", 0, "pass"),
        ("bad-digest", 4, "fail"),
        ("bad-subject", 4, "fail"),
        ("empty", 0, "skipped"),
    ] {
        let output = run_adapter(&[
            "--report",
            "json",
            "--timeout-ms",
            "30000",
            "--repeats",
            "2",
            "node",
            &script,
            mode,
        ]);
        let document: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|failure| panic!("{mode}: {failure}: {}", stderr_text(&output)));
        assert_eq!(exit_code(&output), expected_exit, "{mode}: {document}");
        let report = &document["report"];
        assert_eq!(
            report["schema_version"],
            "lekalo/adapter-conformance/v1.3.0"
        );
        assert_eq!(report["session"]["protocol"], "1.4.0");
        let check = report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "evidence.envelope")
            .expect("evidence check in report");
        assert_eq!(check["state"], expected_state, "{mode}: {check}");
        if expected_state != "pass" {
            assert_eq!(report["badge"]["issued"], false);
        }
    }
}
