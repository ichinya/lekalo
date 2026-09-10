//! Issue #18 CLI tests for the `lekalo diff` handoff: the equal
//! formatting case on the valid envelope, the golden fixture pairs, the
//! profile selection grammar, the usage failures, the mixed-model-version
//! rejection, and the version custody probe.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CASES: &str = "tests/fixtures/diff/cases";

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

fn run_diff(args: &[&str]) -> Output {
    let root = workspace_root();
    let mut full = vec!["--json"];
    full.extend_from_slice(args);
    lekalo_in(&root, &full)
}

/// The equal-formatting case answers `equal: true` with empty facts on
/// the valid envelope in both flag orders.
#[test]
fn formatting_only_cases_answer_equal_with_empty_facts() {
    let output = run_diff(&[
        "diff",
        "tests/fixtures/diff/cases/equal-formatting/base",
        "tests/fixtures/diff/cases/equal-formatting/candidate",
    ]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    assert_eq!(document["status"], "valid");
    let diff = &document["diff"];
    assert_eq!(diff["equal"], true);
    assert_eq!(diff["complete"], true);
    assert_eq!(diff["changes"].as_array().expect("changes").len(), 0);
    assert_eq!(diff["affectedSeeds"].as_array().expect("seeds").len(), 0);
    assert_eq!(diff["schemaVersion"], "lekalo/semantic-diff/v1.0.0");
    assert_eq!(diff["identity"], "dev.lekalo.semantic-diff@1.0.0");
    assert_eq!(diff["base"]["irIdentity"], "dev.lekalo.ir@0.1.0");
}

/// Every fixture case compares cleanly and emits the exact change count
/// its golden pins; the profile run stays on the valid envelope.
#[test]
fn fixture_cases_compare_with_and_without_profiles() {
    let cases = [
        ("behavior", 8),
        ("fields", 6),
        ("fields-tighten", 2),
        ("signature", 8),
        ("symbols", 7),
        ("history", 7),
    ];
    for (case, expected_changes) in cases {
        let base = format!("{CASES}/{case}/base");
        let candidate = format!("{CASES}/{case}/candidate");
        let output = run_diff(&["diff", &base, &candidate]);
        assert_eq!(exit_code(&output), 0, "{case}: {}", stderr_text(&output));
        let document: serde_json::Value =
            serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
        assert_eq!(document["diff"]["equal"], false, "{case}");
        assert_eq!(
            document["diff"]["metadata"]["changeCount"].as_u64(),
            Some(expected_changes),
            "{case}"
        );
        assert_eq!(document["diff"]["profiles"].as_array().unwrap().len(), 0);

        let output = run_diff(&[
            "diff",
            &base,
            &candidate,
            "--profiles",
            "source-consumer,wire-consumer,storage-consumer,target-consumer,advisory",
        ]);
        assert_eq!(exit_code(&output), 0, "{case}: {}", stderr_text(&output));
        let document: serde_json::Value =
            serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
        assert_eq!(
            document["diff"]["profiles"].as_array().unwrap().len(),
            5,
            "{case}"
        );
        let ids: Vec<&str> = document["diff"]["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|profile| profile["profileId"].as_str().expect("profile id"))
            .collect();
        assert_eq!(
            ids,
            vec![
                "advisory",
                "source-consumer",
                "storage-consumer",
                "target-consumer",
                "wire-consumer"
            ],
            "{case}: profiles in canonical order"
        );
    }
}

/// The rename fixture resolves through history, never as removal plus
/// guessed alias; the replacement tombstone stays visible.
#[test]
fn rename_and_tombstone_changes_carry_their_history_reasons() {
    let output = run_diff(&[
        "diff",
        "tests/fixtures/diff/cases/history/base",
        "tests/fixtures/diff/cases/history/candidate",
    ]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    let changes = document["diff"]["changes"].as_array().expect("changes");
    let renamed = changes
        .iter()
        .find(|change| change["kind"] == "symbol.renamed")
        .expect("rename record");
    assert_eq!(renamed["subject"]["id"], "shop.label");
    let reasons: Vec<&str> = renamed["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|reason| reason.as_str().expect("reason"))
        .collect();
    assert!(reasons.contains(&"symbol.rename-history"));
    let replaced = changes
        .iter()
        .find(|change| change["kind"] == "symbol.replaced")
        .expect("replacement record");
    assert_eq!(replaced["after"]["replacedBy"], "shop.status");
}

/// One positional only, or both `--base` and two positionals, are the
/// stable usage failure on exit 1.
#[test]
fn selector_arity_violations_are_usage_failures() {
    let one_positional = run_diff(&["diff", "tests/fixtures/diff/cases/fields/base"]);
    assert_eq!(exit_code(&one_positional), 1);
    let document: serde_json::Value =
        serde_json::from_str(stderr_text(&one_positional).trim()).expect("envelope json");
    assert_eq!(document["diagnostics"][0]["id"], "cli.usage");

    let over_specified = run_diff(&[
        "diff",
        "--base",
        "tests/fixtures/diff/cases/fields/base",
        "tests/fixtures/diff/cases/fields/base",
        "tests/fixtures/diff/cases/fields/candidate",
    ]);
    assert_eq!(exit_code(&over_specified), 1);

    // An unknown profile term rejects through the registered diff rule.
    let unknown_profile = run_diff(&[
        "diff",
        "tests/fixtures/diff/cases/fields/base",
        "tests/fixtures/diff/cases/fields/candidate",
        "--profiles",
        "warp-consumer",
    ]);
    assert_eq!(exit_code(&unknown_profile), 1);
    let document: serde_json::Value =
        serde_json::from_str(stderr_text(&unknown_profile).trim()).expect("envelope json");
    assert_eq!(document["diagnostics"][0]["id"], "diff.profile-invalid");
}

/// A base and a candidate from different Model contract families reject
/// through `diff.input-invalid` with no partial diff.
#[test]
fn mixed_model_versions_reject_without_partial_output() {
    let output = run_diff(&[
        "diff",
        "tests/fixtures/diff/cases/fields/base",
        "tests/fixtures/versioning/migration/golden-0.1.0",
    ]);
    assert_eq!(exit_code(&output), 1, "{}", stderr_text(&output));
    let text = stderr_text(&output);
    assert!(text.contains("diff.input-invalid"), "{text}");
    assert!(!text.contains("semantic-diff/v1.0.0"), "no partial result");
}

/// The version custody probe: the binary prints the prospective product
/// candidate in both flag orders.
#[test]
fn version_custody_prints_the_prospective_candidate() {
    let root = workspace_root();
    let human = lekalo_in(&root, &["--version"]);
    assert_eq!(exit_code(&human), 0);
    assert_eq!(stdout_text(&human).trim(), "lekalo 0.2.9");
    let json = lekalo_in(&root, &["--json", "--version"]);
    assert_eq!(exit_code(&json), 0);
    assert_eq!(
        stdout_text(&json).trim(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.2.9\"\n}"
    );
}
