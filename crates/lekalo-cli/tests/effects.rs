//! Issue #14 CLI tests for the `lekalo effects` handoff: the hermetic
//! planner fixture, the three thin subcommands on the accepted 0/1
//! envelope, the writer/reader selector grammar, the explicit conflict
//! handoff, and the version custody probe.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/effects/planner";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    // The pre-cache suites pin the published --no-cache path; cache-on
    // behavior is exercised by tests/cache.rs.
    let mut args = args.to_vec();
    args.insert(0, "--no-cache");
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

/// `effects show` answers on the valid envelope with the operation card,
/// its edges, and explicit completeness.
#[test]
fn show_reports_declared_effects_of_one_operation() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "show",
            "planner.focus_task",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    assert_eq!(document["status"], "valid");
    let effects = &document["effects"];
    assert_eq!(effects["identity"], "dev.lekalo.effects@1.0.0");
    assert_eq!(effects["operation"], "operation:planner.focus_task");
    assert_eq!(effects["complete"], true);
    let edges = effects["edges"].as_array().expect("edges");
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0]["kind"], "create");
    assert_eq!(
        edges[0]["resource"],
        serde_json::json!({"id": "planner.task", "kind": "canonical"})
    );
    assert_eq!(edges[0]["confidence"], "canonical");
    assert_eq!(edges[1]["kind"], "emit-event");
    assert_eq!(edges[1]["resource"]["id"], "planner.task_focused");

    // An operation with no effects is a known definition and an empty
    // success.
    let empty = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "show",
            "notify.purge_cache_cmd",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&empty), 0, "{}", stderr_text(&empty));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&empty).trim()).expect("envelope json");
    assert_eq!(
        document["effects"]["edges"]
            .as_array()
            .expect("edges")
            .len(),
        0
    );

    // An endpoint is not an operation: an explicit failure.
    let endpoint = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "show",
            "planner.api_focus",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&endpoint), 1);
    assert!(stderr_text(&endpoint).contains("graph.unknown-node"));

    let unknown = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "show",
            "planner.nonesuch",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&unknown), 1);
    let text = stderr_text(&unknown);
    assert!(text.contains("graph.unknown-node"));
    assert!(text.contains("LEK-GRAPH-007"));
}

/// `effects writers` answers from the reverse index; the selector
/// grammar accepts the entity, the exact field, and typed references.
#[test]
fn writers_report_the_reverse_view() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "writers",
            "planner.task",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    assert_eq!(document["effects"]["scope"], "entity");
    let writers = document["effects"]["edges"].as_array().expect("writers");
    assert_eq!(writers.len(), 3);
    let kinds: Vec<&str> = writers
        .iter()
        .map(|edge| edge["kind"].as_str().expect("kind"))
        .collect();
    assert_eq!(kinds, vec!["delete", "update", "create"]);

    // Readers of the same resource: exactly the three queries.
    let readers = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "writers",
            "planner.task",
            "--readers",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&readers), 0, "{}", stderr_text(&readers));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&readers).trim()).expect("envelope json");
    let readers = document["effects"]["edges"].as_array().expect("readers");
    assert_eq!(readers.len(), 3);
    for edge in readers {
        assert_eq!(edge["kind"], "read");
    }

    // A field-scoped selector without an exact-field effect is empty;
    // with --readers the entity-wide reads do not match either.
    let field = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "writers",
            "planner.task.title",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&field), 0, "{}", stderr_text(&field));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&field).trim()).expect("envelope json");
    assert_eq!(document["effects"]["scope"], "field");
    assert_eq!(
        document["effects"]["edges"]
            .as_array()
            .expect("writers")
            .len(),
        0
    );

    // An unknown selector is an explicit failure, never an empty answer.
    let unknown = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "writers",
            "planner.nonesuch",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&unknown), 1);
    assert!(stderr_text(&unknown).contains("graph.unknown-node"));
}

/// `effects conflicts --changed` classifies the explicit change set;
/// the handoff never infers changed symbols and usage errors stay typed.
#[test]
fn conflicts_classify_the_explicit_change_set() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "conflicts",
            "--changed",
            "planner.edit_task_cmd,planner.archive_task_cmd",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    let effects = &document["effects"];
    assert_eq!(effects["complete"], true);
    let conflicts = effects["conflicts"].as_array().expect("conflicts");
    assert!(conflicts.len() >= 3, "{}", stdout_text(&output));
    let classifications: Vec<&str> = conflicts
        .iter()
        .map(|item| item["classification"].as_str().expect("classification"))
        .collect();
    assert!(classifications.contains(&"delete-overlap"));
    assert!(classifications.contains(&"potential-read-write"));
    assert!(classifications.contains(&"definite-write-write"));

    // Unknown changed operations are explicit failures.
    let unknown = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "conflicts",
            "--changed",
            "planner.nonesuch",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&unknown), 1);
    assert!(stderr_text(&unknown).contains("graph.unknown-node"));

    // An empty changed list is a usage failure, never an empty success.
    let usage = lekalo_in(
        &project,
        &[
            "--json",
            "effects",
            "conflicts",
            "--changed",
            "",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&usage), 1);
    assert!(stderr_text(&usage).contains("cli.usage"));
}

/// The human and JSON projections carry the same result.
#[test]
fn human_projection_matches_the_json_envelope() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &["effects", "writers", "planner.task", "--project", "."],
    );
    assert_eq!(exit_code(&output), 0);
    let stdout = stdout_text(&output);
    assert!(
        stdout.starts_with("writers of canonical:planner.task : 3 operations"),
        "human summary: {stdout}"
    );
}

/// The version custody probe: the binary reports the prospective product
/// version in both projections.
#[test]
fn version_reports_the_prospective_product_version() {
    let project = fixture_path();
    let human = lekalo_in(&project, &["--version"]);
    assert_eq!(exit_code(&human), 0);
    assert_eq!(stdout_text(&human).trim(), "lekalo 0.1.27");
    let json = lekalo_in(&project, &["--json", "--version"]);
    assert_eq!(exit_code(&json), 0);
    assert_eq!(
        stdout_text(&json).trim(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.1.27\"\n}"
    );
}
