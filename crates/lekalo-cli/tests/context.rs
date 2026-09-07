//! Issue #17 CLI tests for the `lekalo context` handoff: the hermetic
//! planner fixture, the single-symbol and changed-scope modes, the pinned
//! golden capsule in both projections, the exact budget truncation
//! metadata, the registered failure set, the opt-in span evidence path,
//! and the version custody probe.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/context/planner";
/// The pinned whole-envelope golden (`{"status":"valid","context":...}`).
const GOLDEN_ENVELOPE: &str =
    include_str!("../../../tests/fixtures/context/golden/planner.context.envelope.json");
/// The pinned Markdown golden (the human projection of the same capsule).
const GOLDEN_MARKDOWN: &str =
    include_str!("../../../tests/fixtures/context/golden/planner.context.md");

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
            .join(FIXTURE),
    )
}

/// The single-symbol capsule matches the pinned goldens byte for byte in
/// both projections, and repeated runs are byte-identical.
#[test]
fn context_matches_the_pinned_goldens_and_is_deterministic() {
    let project = fixture_path();
    let json = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "planner.focus_task",
            "--budget",
            "5000",
        ],
    );
    assert_eq!(exit_code(&json), 0, "{}", stderr_text(&json));
    assert_eq!(stdout_text(&json), GOLDEN_ENVELOPE);
    let repeat = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "planner.focus_task",
            "--budget",
            "5000",
        ],
    );
    assert_eq!(stdout_text(&repeat), GOLDEN_ENVELOPE);

    let markdown = lekalo_in(
        &project,
        &["context", "planner.focus_task", "--budget", "5000"],
    );
    assert_eq!(exit_code(&markdown), 0, "{}", stderr_text(&markdown));
    assert_eq!(stdout_text(&markdown), GOLDEN_MARKDOWN);
}

/// The JSON envelope carries the closed capsule shape: contract identity,
/// estimator pin, budget metadata, manifest, coverage, and the gaps.
#[test]
fn the_json_envelope_carries_the_closed_capsule_shape() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "planner.focus_task",
            "--budget",
            "5000",
        ],
    );
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    assert_eq!(document["status"], "valid");
    let capsule = &document["context"];
    assert_eq!(capsule["schemaVersion"], "lekalo/context/v1.0.0");
    assert_eq!(capsule["identity"], "dev.lekalo.context@1.0.0");
    assert_eq!(capsule["mode"], "symbol");
    assert_eq!(capsule["project"], "planner");
    assert_eq!(capsule["roots"][0], "operation:planner.focus_task");
    assert_eq!(capsule["budget"]["fits"], true);
    assert_eq!(
        capsule["budget"]["minimumRequired"],
        capsule["budget"]["estimated"]
    );
    assert_eq!(
        capsule["estimator"]["identity"],
        "dev.lekalo.estimator.chars-4@1.0.0"
    );
    assert_eq!(capsule["coverage"]["candidates"], 13);
    assert_eq!(capsule["coverage"]["included"], 13);
    assert_eq!(capsule["coverage"]["excluded"], 0);
    assert!(capsule["manifest"]["excluded"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        capsule["manifest"]["included"].as_array().unwrap().len(),
        13
    );
    assert!(capsule.get("spans").is_none(), "spans are opt-in");
}

/// An exhausted budget is a valid result with exact truncation metadata.
#[test]
fn an_exhausted_budget_reports_the_exact_minimum_requirement() {
    let project = fixture_path();
    // First learn the exact minimum requirement from the permissive run.
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "planner.focus_task",
            "--budget",
            "1000000",
        ],
    );
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    let required = document["context"]["budget"]["minimumRequired"]
        .as_u64()
        .expect("minimum required");
    let tight_budget = required - 1;
    let tight_budget_text = tight_budget.to_string();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "planner.focus_task",
            "--budget",
            &tight_budget_text,
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    let capsule = &document["context"];
    assert_eq!(capsule["budget"]["fits"], false);
    assert_eq!(capsule["budget"]["limit"], tight_budget);
    assert_eq!(capsule["budget"]["minimumRequired"], required);
    let included = capsule["manifest"]["included"].as_array().unwrap().len();
    let excluded = capsule["manifest"]["excluded"].as_array().unwrap().len();
    assert_eq!(included + excluded, 13);
    assert!(excluded >= 1);
    for row in capsule["manifest"]["excluded"].as_array().unwrap() {
        assert_eq!(row["reason"], "budget");
    }
    // The human projection carries the same truncation metadata.
    let human = lekalo_in(
        &project,
        &[
            "context",
            "planner.focus_task",
            "--budget",
            &tight_budget_text,
        ],
    );
    let markdown = stdout_text(&human);
    assert!(markdown.contains("fits false"));
    assert!(markdown.contains("## excluded"));
    assert!(markdown.contains("minimum required"));
}

/// The changed scope accepts a typed handoff, deduplicates, and sorts the
/// roots; every changed symbol's card is present.
#[test]
fn the_changed_scope_covers_the_typed_change_set() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "--changed",
            "planner.focus_task, planner.edit_task_cmd, planner.focus_task",
            "--budget",
            "1000000",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    let capsule = &document["context"];
    assert_eq!(capsule["mode"], "changed");
    let roots = capsule["roots"].as_array().unwrap();
    assert_eq!(roots.len(), 2, "duplicates collapse");
    assert_eq!(roots[0], "operation:planner.edit_task_cmd");
    assert_eq!(roots[1], "operation:planner.focus_task");
    let symbols = capsule["sections"]["symbol"].as_array().unwrap();
    assert_eq!(symbols.len(), 2);
}

/// Unknown symbols are explicit `graph.unknown-node` failures on the
/// accepted envelope — never empty successes.
#[test]
fn unknown_symbols_fail_through_the_registered_rule() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &["--json", "context", "no.such_symbol", "--budget", "100"],
    );
    assert_eq!(exit_code(&output), 1);
    let rendered = stderr_text(&output);
    assert!(rendered.contains("graph.unknown-node"), "{rendered}");
    assert!(rendered.contains("LEK-GRAPH-007"));
    assert!(rendered.contains("no.such_symbol"));

    // The same failure through the changed set.
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "--changed",
            "planner.focus_task,ghost.symbol",
            "--budget",
            "100",
        ],
    );
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("graph.unknown-node"));
}

/// A budget of zero or beyond the recorded bound is a fatal input
/// violation (`graph.input-invalid`, exit 1, stderr).
#[test]
fn out_of_range_budgets_are_fatal_input_violations() {
    let project = fixture_path();
    for budget in ["0", "1000001"] {
        let output = lekalo_in(
            &project,
            &[
                "--json",
                "context",
                "planner.focus_task",
                "--budget",
                budget,
            ],
        );
        assert_eq!(exit_code(&output), 1);
        let rendered = stderr_text(&output);
        assert!(rendered.contains("graph.input-invalid"), "{rendered}");
        assert!(rendered.contains("budget-out-of-range"));
    }
}
/// The scope argument grammar: exactly one of the positional symbol and
/// `--changed`; an empty changed entry is malformed usage. All violations
/// map to the stable `cli.usage` envelope (exit 1).
#[test]
fn scope_grammar_violations_map_to_the_usage_failure() {
    let project = fixture_path();
    // Both supplied.
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "planner.focus_task",
            "--changed",
            "planner.focus_task",
            "--budget",
            "100",
        ],
    );
    assert_eq!(exit_code(&output), 1);
    assert!(
        stderr_text(&output).contains("cli.usage"),
        "{}",
        stderr_text(&output)
    );
    // Neither supplied.
    let output = lekalo_in(&project, &["--json", "context", "--budget", "100"]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("cli.usage"));
    // An empty entry inside the changed list.
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "--changed",
            "planner.focus_task,",
            "--budget",
            "100",
        ],
    );
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("cli.usage"));
}

/// `--spans` attaches the declaration-span sidecar with logical
/// project-relative paths only; the default emits no spans at all.
#[test]
fn spans_are_opt_in_and_logical_only() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "planner.focus_task",
            "--budget",
            "5000",
            "--spans",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let rendered = stdout_text(&output);
    assert!(rendered.contains(r#""spans":"#));
    assert!(rendered.contains(r#""path":"lekalo/modules/planner/commands.yaml""#));
    // No absolute path spelling anywhere in the emitted bytes.
    assert!(!rendered.contains("C:\\") && !rendered.contains("C:/"));
    let document: serde_json::Value = serde_json::from_str(rendered.trim()).expect("envelope json");
    let spans = document["context"]["spans"].as_array().expect("spans");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0]["id"], "operation:planner.focus_task");
}

/// The capsule never carries raw source, secrets, or absolute paths, and
/// the failed-and-valid surfaces stay on their accepted streams.
#[test]
fn output_streams_and_privacy_stay_on_the_accepted_contract() {
    let project = fixture_path();
    let ok = lekalo_in(
        &project,
        &[
            "--json",
            "context",
            "planner.focus_task",
            "--budget",
            "5000",
        ],
    );
    assert!(ok.stderr.is_empty());
    assert!(ok.stdout.len() > 1_000);
    let rendered = stdout_text(&ok);
    assert!(!rendered.contains(".env"));
    assert!(!rendered.contains(r"\\?\"));
    // Failures render on stderr, successes on stdout.
    let failure = lekalo_in(
        &project,
        &["--json", "context", "ghost.symbol", "--budget", "5"],
    );
    assert!(failure.stdout.is_empty());
    assert!(!failure.stderr.is_empty());
}

/// The version custody probe: the binary is exactly 0.1.29.
#[test]
fn the_version_is_the_prospective_product_version() {
    let project = fixture_path();
    let human = lekalo_in(&project, &["--version"]);
    assert_eq!(stdout_text(&human).trim(), "lekalo 0.1.29");
    let json = lekalo_in(&project, &["--json", "--version"]);
    assert_eq!(
        stdout_text(&json).trim(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.1.29\"\n}"
    );
}
