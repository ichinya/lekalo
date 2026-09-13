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
    assert!(stdout.contains("badge verified protocol=1.0.0 ir=0.1.0"));
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
        "lekalo/adapter-conformance/v1.0.0"
    );
    assert_eq!(document["report"]["badge"]["issued"], true);
    assert_eq!(document["report"]["badge"]["protocol"], "1.0.0");
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
