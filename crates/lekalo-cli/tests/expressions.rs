//! Issue #66 CLI tests for the `lekalo expressions` handoff: the
//! hermetic validate envelope with the capability summary and
//! canonical digest, the deterministic reference evaluator over the
//! shared cross-target vectors with the injected clock, the
//! generated program render for every target, the managed-mode
//! built-in block, and the pure diff classification.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.
//! Failure envelopes stream to stderr with exit 1; success envelopes
//! stream to stdout.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn expressions_root() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/expressions"),
    )
}

/// The selection policy denies alias-spelled working directories;
/// chdir the child into the resolved spelling.
fn alias_free_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("fixture path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => canonical,
    }
    #[cfg(not(windows))]
    {
        canonical
    }
}

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
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

fn json_of(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout_text(output)).expect("json envelope")
}

fn json_err_of(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stderr_text(output)).expect("json envelope")
}

#[test]
fn validate_emits_the_capability_summary_and_digest() {
    let root = expressions_root();
    let output = lekalo_in(
        &root,
        &["--json", "expressions", "validate", "valid/planner.json"],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let document = json_of(&output);
    assert_eq!(document["status"], "valid");
    assert_eq!(document["expressions"]["projectId"], "planner");
    assert_eq!(document["expressions"]["expressionCount"], 36);
    assert_eq!(document["expressions"]["conditions"], 16);
    assert_eq!(document["expressions"]["assignments"], 20);
    assert_eq!(document["expressions"]["builtinSemantics"], "1.0.0");
    assert_eq!(
        document["expressions"]["requiredCapabilities"]
            .as_array()
            .map(Vec::len),
        Some(16)
    );
    let digest = document["expressions"]["digest"]
        .as_str()
        .expect("digest")
        .to_owned();
    assert!(digest.starts_with("sha256:"));
    assert_eq!(digest.len(), "sha256:".len() + 64);
    // Deterministic: the same attachment validates to the same digest.
    let again = lekalo_in(
        &root,
        &["--json", "expressions", "validate", "valid/planner.json"],
    );
    assert_eq!(json_of(&again)["expressions"]["digest"], digest);
}

#[test]
fn invalid_attachments_fail_with_the_registered_rule() {
    let root = expressions_root();
    let output = lekalo_in(
        &root,
        &[
            "--json",
            "expressions",
            "validate",
            "invalid/eq-type-mismatch.json",
        ],
    );
    assert_eq!(exit_code(&output), 1);
    assert!(
        stdout_text(&output).is_empty(),
        "no partial success on stdout"
    );
    let document = json_err_of(&output);
    assert_eq!(document["status"], "invalid");
    assert_eq!(document["reasonCodes"][0], "expression.type-invalid");
    // The declared span explains the rejection at its source.
    let diagnostic = &document["diagnostics"][0];
    assert_eq!(
        diagnostic["source"]["path"],
        "modules/planner/expressions.yaml"
    );
    assert_eq!(diagnostic["source"]["range"]["start"]["line"], 12);
}

#[test]
fn eval_runs_the_shared_vectors_with_the_injected_clock() {
    let root = expressions_root();
    let output = lekalo_in(
        &root,
        &[
            "--json",
            "expressions",
            "eval",
            "valid/planner.json",
            "--vectors",
            "vectors.json",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let document = json_of(&output);
    assert_eq!(document["status"], "valid");
    assert_eq!(document["expressionEval"]["vectors"], 87);
    assert_eq!(document["expressionEval"]["failures"], 0);
    let results = document["expressionEval"]["results"]
        .as_array()
        .expect("results");
    // The deterministic clock drove the overdue verdict.
    let overdue = results
        .iter()
        .find(|row| row["id"] == "overdue-past-open")
        .expect("overdue vector");
    assert_eq!(overdue["value"], true);
    let review = results
        .iter()
        .find(|row| row["id"] == "review-null-uses-now")
        .expect("review vector");
    assert_eq!(review["value"], "2026-09-12T00:00:00Z");
    // Domain errors carry the closed tokens as data.
    let divide = results
        .iter()
        .find(|row| row["id"] == "spread-divide-by-zero")
        .expect("divide vector");
    assert_eq!(divide["error"], "divide-by-zero");
}

#[test]
fn render_emits_complete_programs_for_every_target() {
    let root = expressions_root();
    for (target, marker) in [
        ("node", "const LEK = {};"),
        ("php", "<?php"),
        ("go", "package main"),
    ] {
        let output = lekalo_in(
            &root,
            &[
                "expressions",
                "render",
                "valid/planner.json",
                "--target",
                target,
            ],
        );
        assert_eq!(exit_code(&output), 0, "{target}: {}", stdout_text(&output));
        let program = stdout_text(&output);
        assert!(program.contains(marker), "{target} missing {marker}");
        assert!(program.contains("expr.planner/overdue-check"), "{target}");
        // The JSON envelope carries the same program.
        let json_output = lekalo_in(
            &root,
            &[
                "--json",
                "expressions",
                "render",
                "valid/planner.json",
                "--target",
                target,
            ],
        );
        assert_eq!(exit_code(&json_output), 0);
        let document = json_of(&json_output);
        assert_eq!(document["expressionRender"]["target"], target);
        assert_eq!(
            document["expressionRender"]["program"]
                .as_str()
                .map(str::len),
            Some(program.len())
        );
    }
}

#[test]
fn managed_mode_blocks_builtins_absent_from_the_snapshot() {
    let root = expressions_root();
    let ok = lekalo_in(
        &root,
        &[
            "--json",
            "expressions",
            "validate",
            "valid/planner.json",
            "--builtin-support",
            "builtin-support/full.json",
        ],
    );
    assert_eq!(exit_code(&ok), 0, "{}", stdout_text(&ok));
    let blocked = lekalo_in(
        &root,
        &[
            "--json",
            "expressions",
            "validate",
            "valid/planner.json",
            "--builtin-support",
            "builtin-support/core-only.json",
        ],
    );
    assert_eq!(exit_code(&blocked), 1);
    assert!(stdout_text(&blocked).is_empty());
    let document = json_err_of(&blocked);
    assert_eq!(document["reasonCodes"][0], "expression.builtin-unsupported");
}

#[test]
fn diff_classifies_every_changed_path() {
    let root = expressions_root();
    let output = lekalo_in(
        &root,
        &[
            "--json",
            "expressions",
            "diff",
            "diff/base.json",
            "diff/candidate.json",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let document = json_of(&output);
    assert_eq!(document["status"], "valid");
    assert_eq!(document["expressionsDiff"]["equal"], false);
    assert_eq!(document["expressionsDiff"]["breaking"], 2);
    assert_eq!(document["expressionsDiff"]["nonBreaking"], 1);
    assert_eq!(document["expressionsDiff"]["policyChange"], 2);
    let paths = document["expressionsDiff"]["paths"]
        .as_array()
        .expect("paths");
    assert!(paths
        .iter()
        .any(|path| path["path"] == "expr.planner/title-open" && path["class"] == "breaking"));
    assert!(paths
        .iter()
        .any(|path| path["path"] == "expr.planner/search-match/body"
            && path["class"] == "policy-change"));
    assert!(paths
        .iter()
        .any(|path| path["path"] == "expr.planner/audit-tag" && path["class"] == "non-breaking"));
}

#[test]
fn missing_documents_fail_closed() {
    let root = expressions_root();
    let output = lekalo_in(
        &root,
        &["--json", "expressions", "validate", "valid/absent.json"],
    );
    assert_eq!(exit_code(&output), 1);
    assert!(stdout_text(&output).is_empty());
    let document = json_err_of(&output);
    assert_eq!(document["reasonCodes"][0], "expression.input-invalid");
}
