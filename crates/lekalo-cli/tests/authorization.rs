//! Issue #25 CLI tests for the authorization review handoff: strict
//! coverage blocks, malformed documents, profile separation, output
//! determinism, and the version custody probe.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURES: &str = "tests/fixtures/authorization";

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

fn workspace_root() -> PathBuf {
    alias_free_path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../"))
}

fn run_validate(fixture: &str, strict: bool) -> Output {
    let root = workspace_root();
    let project = format!("{FIXTURES}/{fixture}");
    let mut args = vec!["validate", "--json", "--project", project.as_str()];
    if strict {
        args.push("--strict");
    }
    lekalo_in(&root, &args)
}

/// The version custody probe: the binary is exactly the prospective
/// product candidate.
#[test]
fn the_version_is_the_prospective_product_version() {
    let root = workspace_root();
    let human = lekalo_in(&root, &["--version"]);
    assert_eq!(exit_code(&human), 0);
    assert_eq!(stdout_text(&human).trim(), "lekalo 0.1.29");
    let json = lekalo_in(&root, &["--json", "--version"]);
    assert_eq!(exit_code(&json), 0);
    assert_eq!(
        stdout_text(&json).trim(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.1.29\"\n}"
    );
}

/// The fully covered tenant project passes strict review with exit 0,
/// byte-identical across reruns.
#[test]
fn covered_project_passes_strict_and_is_byte_identical_across_reruns() {
    let first = run_validate("planner", true);
    assert_eq!(exit_code(&first), 0, "stdout: {}", stdout_text(&first));
    assert!(stderr_text(&first).is_empty());
    let second = run_validate("planner", true);
    assert_eq!(stdout_text(&first), stdout_text(&second), "determinism");
}

/// Strict blocks a project with protected effects but no authorization
/// document at all (exit 3, denied).
#[test]
fn strict_blocks_protected_effects_without_authorization_document() {
    let output = run_validate("noauth", true);
    assert_eq!(exit_code(&output), 3, "stdout: {}", stdout_text(&output));
    let text = stdout_text(&output);
    assert!(
        text.contains("\"status\": \"denied\""),
        "denied envelope: {text}"
    );
    assert!(
        text.contains("authorization.effect-unprotected"),
        "rule id: {text}"
    );
    let again = run_validate("noauth", true);
    assert_eq!(stdout_text(&output), stdout_text(&again), "determinism");
}

/// Strict blocks partial adapter mapping evidence (exit 3).
#[test]
fn strict_blocks_partial_mapping_evidence() {
    let output = run_validate("mapping-partial", true);
    assert_eq!(exit_code(&output), 3, "stdout: {}", stdout_text(&output));
    let text = stdout_text(&output);
    assert!(
        text.contains("authorization.mapping-stale"),
        "rule id: {text}"
    );
}

/// Strict blocks a stale model pin (exit 3).
#[test]
fn strict_blocks_stale_model_reference() {
    let output = run_validate("stale", true);
    assert_eq!(exit_code(&output), 3, "stdout: {}", stdout_text(&output));
    assert!(stdout_text(&output).contains("authorization.mapping-stale"));
}

/// Strict blocks an uncovered project (exit 3); the same project passes
/// the default profile — the documented profile separation.
#[test]
fn profile_separation_blocks_only_under_strict() {
    let strict = run_validate("uncovered", true);
    assert_eq!(
        exit_code(&strict),
        3,
        "strict blocks: {}",
        stdout_text(&strict)
    );
    let default = run_validate("uncovered", false);
    assert_eq!(
        exit_code(&default),
        0,
        "default advisory: {}",
        stdout_text(&default)
    );
}

/// A malformed document is invalid (exit 1) in both profiles, and the
/// failure is deterministic.
#[test]
fn malformed_authorization_document_is_invalid_in_both_profiles() {
    for strict in [true, false] {
        let output = run_validate("malformed", strict);
        assert_eq!(exit_code(&output), 1, "strict={strict}");
        let text = stderr_text(&output);
        assert!(
            text.contains("authorization.document-invalid"),
            "rule id: {text}"
        );
        assert!(
            text.contains("authorization.actor-invalid"),
            "actor rule id: {text}"
        );
        let again = run_validate("malformed", strict);
        assert_eq!(stderr_text(&output), stderr_text(&again), "determinism");
    }
}

/// A hostile non-object `ownership` entry fails closed as the registered
/// `authorization.policy-invalid` diagnostic (LEK-AUTH-004) with exit 1
/// in both profiles — never a process panic (exit 101). Regression for
/// the parser branch that re-evaluated the `Option` it had just found
/// empty and crashed on any non-mapping ownership item.
#[test]
fn hostile_non_object_ownership_entry_is_invalid_not_a_panic() {
    for strict in [true, false] {
        let output = run_validate("ownership-shape", strict);
        assert_eq!(exit_code(&output), 1, "strict={strict}");
        let text = stderr_text(&output);
        assert!(!text.contains("panicked"), "no panic allowed: {text}");
        assert!(
            text.contains("authorization.policy-invalid"),
            "rule id: {text}"
        );
        assert!(text.contains("LEK-AUTH-004"), "code: {text}");
        assert!(
            text.contains("ownership-shape"),
            "bounded detail tag: {text}"
        );
        assert!(
            !text.contains("not-an-object"),
            "zero attacker-controlled echo: {text}"
        );
        let again = run_validate("ownership-shape", strict);
        assert_eq!(stderr_text(&output), stderr_text(&again), "determinism");
    }
}
