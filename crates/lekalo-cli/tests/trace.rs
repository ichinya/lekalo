//! Issue #22 CLI tests for `lekalo trace`: validate/export/query on the
//! accepted 0/1 envelope, byte-identical canonical export, the pinned
//! digest, and typed refusals — including the Windows 8.3-alias lesson
//! (children run from the resolved, alias-free spelling).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GOLDEN: &str = include_str!("../../../tests/fixtures/trace/golden/planner.trace.json");
const GOLDEN_DIGEST: &str =
    "sha256:b0fabdc6f2bc55f392e912989579ce8b56b41547d7a4782f06acfda41a3a5031";
const FULL: &str = "tests/fixtures/trace/full.trace.json";
const PARTIAL: &str = "tests/fixtures/trace/partial.trace.json";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .output()
        .expect("run the real lekalo binary")
}

/// GitHub's Windows runners export `%TEMP%` spelled with the 8.3 alias of
/// the profile directory (`C:\Users\RUNNER~1\AppData\Local\Temp`), and the
/// selection policy denies alias spellings before any command logic runs.
/// Chdir the child into the resolved, alias-free spelling; `canonicalize`
/// returns it under a `\\?\` verbatim prefix that is stripped back to the
/// plain drive form.
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

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn assert_lf_only(text: &str) {
    assert!(text.ends_with('\n'), "one final LF");
    assert!(!text.contains("\r\n"), "no CRLF");
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
}

#[test]
fn trace_validate_accepts_the_golden_contract_on_stdout() {
    let dir = workspace_path(".");
    let output = lekalo_in(&dir, &["trace", "validate", FULL]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let human = stdout(&output);
    assert_lf_only(&human);
    assert!(human.contains("planner-trace-full"), "{human}");
    assert!(
        human.contains("completeness full (requirement-to-gate)"),
        "{human}"
    );
    assert!(human.contains("nodes 9; relations 9; gaps 0"), "{human}");

    let json_output = lekalo_in(&dir, &["--json", "trace", "validate", FULL]);
    assert_eq!(json_output.status.code(), Some(0));
    let document: serde_json::Value =
        serde_json::from_str(&stdout(&json_output)).expect("json envelope");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["trace"]["manifestId"], "planner-trace-full");
    assert_eq!(document["trace"]["completeness"], "full");
    assert_eq!(document["trace"]["nodeCount"], 9);
    assert_eq!(document["trace"]["relationCount"], 9);
    assert_eq!(document["trace"]["gapCount"], 0);
    assert_eq!(document["trace"]["uncoveredSinkCount"], 0);
}

#[test]
fn trace_validate_reports_partial_manifests_with_explicit_gaps() {
    let dir = workspace_path(".");
    let output = lekalo_in(&dir, &["trace", "validate", PARTIAL]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("gaps 2; uncovered sinks 1"),
        "{}",
        stdout(&output)
    );

    let json_output = lekalo_in(&dir, &["--json", "trace", "validate", PARTIAL]);
    let document: serde_json::Value =
        serde_json::from_str(&stdout(&json_output)).expect("json envelope");
    assert_eq!(document["trace"]["completeness"], "partial");
    assert_eq!(document["trace"]["gapCount"], 2);
    assert_eq!(document["trace"]["uncoveredSinkCount"], 1);
    assert_eq!(
        document["trace"]["uncoveredSinks"][0],
        "requirement:PLANNER-REQ-003"
    );
}

#[test]
fn trace_validate_rejects_invalid_manifests_on_stderr() {
    let dir = workspace_path(".");
    let output = lekalo_in(
        &dir,
        &[
            "trace",
            "validate",
            "tests/fixtures/trace/invalid/dangling-endpoint.json",
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    let text = stderr(&output);
    assert_lf_only(&text);
    assert!(text.contains("LEK-GRAPH-003"), "{text}");
    assert!(text.contains("graph.input-invalid"), "{text}");

    let json_output = lekalo_in(
        &dir,
        &[
            "--json",
            "trace",
            "validate",
            "tests/fixtures/trace/invalid/dangling-endpoint.json",
        ],
    );
    assert_eq!(json_output.status.code(), Some(1));
    let document: serde_json::Value =
        serde_json::from_str(&stderr(&json_output)).expect("json envelope");
    assert_eq!(document["status"], "invalid");
    assert_eq!(
        document["diagnostics"][0]["data"]["detail"],
        "dangling-endpoint"
    );
}

#[test]
fn trace_validate_refuses_a_missing_file_through_the_registered_io_rule() {
    let dir = workspace_path(".");
    let output = lekalo_in(
        &dir,
        &["trace", "validate", "tests/fixtures/trace/absent.json"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("loader.io"), "{}", stderr(&output));
}

#[test]
fn trace_export_emits_the_committed_canonical_bytes_and_digest() {
    let dir = workspace_path(".");
    let output = lekalo_in(&dir, &["trace", "export", FULL]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let human = stdout(&output);
    assert_eq!(
        human, GOLDEN,
        "human export is exactly the canonical bytes + LF"
    );

    let json_output = lekalo_in(&dir, &["--json", "trace", "export", FULL]);
    assert_eq!(json_output.status.code(), Some(0));
    let document: serde_json::Value =
        serde_json::from_str(&stdout(&json_output)).expect("json envelope");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["manifestDigest"], GOLDEN_DIGEST);
}

#[test]
fn trace_queries_answer_from_the_committed_fixture() {
    let dir = workspace_path(".");
    let cases: &[(&str, &str, &str)] = &[
        (
            "requirements-for:planner.focus_task",
            "requirement PLANNER-REQ-001 implements",
            "\"target\":\"planner.focus_task\"",
        ),
        (
            "symbols-for:PLANNER-REQ-001",
            "symbol planner.switch_focus implements",
            "\"rows\":[{\"id\":\"planner.focus_task\"",
        ),
        (
            "artifacts-for:planner.focus_task",
            "artifact apps-api-focus-task binds",
            "\"occurrence\":\"bindings.focus_task\"",
        ),
        (
            "tests-for:planner.focus_task",
            "native_test node.focus-task-switch verifies",
            "\"relation\":\"verifies\"",
        ),
        (
            "gates-for:node.focus-task-switch",
            "gate hlv.gate.focus evidences",
            "\"confidence\":\"exact\"",
        ),
        (
            "diagnostics-for:hlv.gate.focus",
            "diagnostic hlv.diag.focus references",
            "\"status\":\"confirmed\"",
        ),
    ];
    for (selector, human_needle, json_needle) in cases {
        let output = lekalo_in(&dir, &["trace", "query", FULL, selector]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{selector}: {}",
            stderr(&output)
        );
        assert!(
            stdout(&output).contains(human_needle),
            "{selector}: {}",
            stdout(&output)
        );
        assert_lf_only(&stdout(&output));

        let json_output = lekalo_in(&dir, &["--json", "trace", "query", FULL, selector]);
        assert_eq!(json_output.status.code(), Some(0));
        assert!(
            stdout(&json_output).contains(json_needle),
            "{selector}: {}",
            stdout(&json_output)
        );
    }
}

#[test]
fn trace_gaps_projection_lists_every_gap_in_canonical_order() {
    let dir = workspace_path(".");
    let output = lekalo_in(&dir, &["trace", "query", PARTIAL, "gaps"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let human = stdout(&output);
    assert!(human.contains("gap missing-gate candidate"), "{human}");
    assert!(human.contains("expected=hlv.gate.archive"), "{human}");
    let missing_gate = human.find("gap missing-gate").expect("missing-gate row");
    let stale = human.find("gap stale-revision").expect("stale row");
    assert!(missing_gate < stale, "canonical gap order");

    let empty = lekalo_in(&dir, &["trace", "query", FULL, "gaps"]);
    assert_eq!(empty.status.code(), Some(0));
    assert!(stdout(&empty).contains("gaps: none"), "{}", stdout(&empty));
}

#[test]
fn trace_query_refusals_are_typed_never_empty_successes() {
    let dir = workspace_path(".");
    // Unknown subject.
    let output = lekalo_in(
        &dir,
        &["trace", "query", FULL, "requirements-for:planner.ghost"],
    );
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stderr(&output).contains("LEK-GRAPH-007"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("graph.unknown-node"),
        "{}",
        stderr(&output)
    );

    // Malformed selector.
    let output = lekalo_in(&dir, &["trace", "query", FULL, "everything:all"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("loader.io"), "{}", stderr(&output));
}

#[test]
fn trace_export_is_byte_identical_across_projects_and_runners() {
    // Re-export from a canonicalized temp copy: the bytes never depend on
    // the invocation path or the input spelling.
    let dir = std::env::temp_dir().join(format!(
        "lekalo-trace-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::copy(workspace_path(FULL), dir.join("trace.json")).expect("copy fixture");
    let output = lekalo_in(&dir, &["trace", "export", "trace.json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), GOLDEN, "byte-identical export");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
