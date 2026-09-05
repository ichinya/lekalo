//! Issue #15 CLI tests for `lekalo inspect`: the thin subcommand over
//! the hermetic planner fixture, the golden envelopes for the six
//! required kinds, the distinct unknown/ambiguous failures on the
//! accepted envelope, the include filters, the byte-identical reruns,
//! the version custody probe, and the no-write guarantee.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/inspect/planner";
const GOLDEN_DIR: &str = "tests/fixtures/inspect/golden";

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

fn workspace_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .canonicalize()
        .expect("workspace root")
}

/// `lekalo inspect <id>` answers on the valid envelope with the pinned
/// golden bytes: one call gives enough context to change the symbol.
#[test]
fn inspect_matches_the_pinned_golden_envelopes() {
    let project = fixture_path();
    for symbol in [
        "planner.task",
        "planner.focus_task",
        "planner.count_focused",
        "planner.deny_bulk_focus",
        "planner.task_focused",
        "planner.focus_flow",
    ] {
        let output = lekalo_in(&project, &["--json", "inspect", symbol, "--project", "."]);
        assert_eq!(exit_code(&output), 0, "{symbol}: {}", stderr_text(&output));
        let golden = std::fs::read_to_string(
            workspace_path()
                .join(GOLDEN_DIR)
                .join(format!("{symbol}.json")),
        )
        .expect("golden envelope");
        assert_eq!(stdout_text(&output), golden, "{symbol}: golden drift");
    }
}

/// The human view matches its pinned golden: fixed labels, no ANSI, no
/// locale.
#[test]
fn human_view_matches_the_pinned_golden() {
    let project = fixture_path();
    let output = lekalo_in(&project, &["inspect", "planner.task", "--project", "."]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let golden = std::fs::read_to_string(
        workspace_path()
            .join(GOLDEN_DIR)
            .join("planner.task.human.txt"),
    )
    .expect("human golden");
    assert_eq!(stdout_text(&output), golden);
}

/// Unknown and ambiguous selectors are distinct stable diagnostics, on
/// stderr with exit 1; malformed selectors are usage failures that echo
/// nothing.
#[test]
fn unknown_ambiguous_and_usage_failures_map_to_the_invalid_envelope() {
    let project = fixture_path();

    // Unknown full id: LEK-INS-001 on stderr.
    let unknown = lekalo_in(&project, &["inspect", "planner.nope", "--project", "."]);
    assert_eq!(exit_code(&unknown), 1);
    assert!(unknown.stdout.is_empty());
    assert!(stderr_text(&unknown).contains("inspect.symbol-unknown"));
    assert!(stderr_text(&unknown).contains("LEK-INS-001"));

    // Ambiguous short name: LEK-INS-003, a different diagnostic, with
    // the bounded sorted candidates and the exact match total.
    let ambiguous = lekalo_in(&project, &["--json", "inspect", "task", "--project", "."]);
    assert_eq!(exit_code(&ambiguous), 1);
    assert!(stdout_text(&ambiguous).is_empty());
    let document: serde_json::Value =
        serde_json::from_str(stderr_text(&ambiguous).trim()).expect("envelope json");
    assert_eq!(document["status"], "invalid");
    let candidates = document["diagnostics"][0]["data"]["candidates"]
        .as_array()
        .expect("candidates");
    assert_eq!(candidates.len(), 2);
    assert_eq!(document["diagnostics"][0]["data"]["matched"], 2);

    // Case-different spelling: malformed grammar, the usage failure —
    // never an unknown-symbol echo.
    let usage = lekalo_in(
        &project,
        &["--json", "inspect", "planner.TASK", "--project", "."],
    );
    assert_eq!(exit_code(&usage), 1);
    let document: serde_json::Value =
        serde_json::from_str(stderr_text(&usage).trim()).expect("envelope json");
    assert_eq!(document["diagnostics"][0]["id"], "cli.usage");
    assert!(document["diagnostics"][0].get("symbol").is_none());

    // Path-like and traversal spellings never reach the filesystem.
    for raw in ["../planner.task", "planner/task", "%2e%2e"] {
        let output = lekalo_in(&project, &["inspect", raw, "--project", "."]);
        assert_eq!(exit_code(&output), 1, "{raw}");
        assert!(stderr_text(&output).contains("cli.usage"), "{raw}");
    }
}

/// Safe short-name resolution and the alias registry resolve through
/// the CLI exactly as in the library.
#[test]
fn short_names_and_aliases_resolve() {
    let project = fixture_path();
    let short = lekalo_in(
        &project,
        &["--json", "inspect", "focus_task", "--project", "."],
    );
    assert_eq!(exit_code(&short), 0, "{}", stderr_text(&short));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&short).trim()).expect("envelope json");
    assert_eq!(document["inspect"]["selector"]["mode"], "short-name");
    assert_eq!(
        document["inspect"]["selector"]["resolvedId"],
        "planner.focus_task"
    );

    let alias = lekalo_in(
        &project,
        &["--json", "inspect", "audit.work_item", "--project", "."],
    );
    assert_eq!(exit_code(&alias), 0, "{}", stderr_text(&alias));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&alias).trim()).expect("envelope json");
    assert_eq!(document["inspect"]["selector"]["mode"], "alias");
    assert_eq!(document["inspect"]["symbol"]["id"], "audit.task");

    // A tombstoned id never resolves.
    let dead = lekalo_in(&project, &["inspect", "planner.old_task", "--project", "."]);
    assert_eq!(exit_code(&dead), 1);
    assert!(stderr_text(&dead).contains("inspect.symbol-unknown"));
}

/// `--include bindings,scenarios` projects the optional sections; an
/// unknown include value is a stable usage failure.
#[test]
fn include_filters_extend_the_projection() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "inspect",
            "planner.focus_task",
            "--include",
            "bindings,scenarios",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    assert_eq!(document["inspect"]["scenarios"]["state"], "available");
    assert_eq!(
        document["inspect"]["scenarios"]["items"][0]["id"],
        "planner.focus_flow"
    );
    assert_eq!(document["inspect"]["bindings"]["state"], "available");
    assert_eq!(
        document["inspect"]["bindings"]["items"][0]["targetId"],
        "node-typescript"
    );

    let bad = lekalo_in(
        &project,
        &[
            "inspect",
            "planner.focus_task",
            "--include",
            "ownership",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&bad), 1);
    assert!(stderr_text(&bad).contains("cli.usage"));
}

/// Repeated invocations are byte-identical, independent of the
/// invocation directory spelling.
#[test]
fn reruns_are_byte_identical() {
    let project = fixture_path();
    let first = lekalo_in(
        &project,
        &["--json", "inspect", "planner.task", "--project", "."],
    );
    let second = lekalo_in(
        &project,
        &["--json", "inspect", "planner.task", "--project", "."],
    );
    assert_eq!(exit_code(&first), 0);
    assert_eq!(stdout_text(&first), stdout_text(&second));
    let human_first = lekalo_in(&project, &["inspect", "planner.task", "--project", "."]);
    let human_second = lekalo_in(&project, &["inspect", "planner.task", "--project", "."]);
    assert_eq!(stdout_text(&human_first), stdout_text(&human_second));
}

/// Inspect writes nothing anywhere.
#[test]
fn inspect_performs_no_writes() {
    let project = fixture_path();
    let before = snapshot(&project);
    let output = lekalo_in(&project, &["inspect", "planner.task", "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    let after = snapshot(&project);
    assert_eq!(before, after, "inspect must not write");
}

fn snapshot(root: &Path) -> Vec<(String, u64)> {
    let mut entries = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            let meta = std::fs::metadata(&path).expect("metadata");
            if meta.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("relative")
                    .to_string_lossy()
                    .replace('\\', "/");
                entries.push((relative, meta.len()));
            }
        }
    }
    entries.sort();
    entries
}

/// Version custody: the binary reports the prospective 0.1.22.
#[test]
fn version_probe_reports_the_prospective_release() {
    let project = fixture_path();
    let human = lekalo_in(&project, &["--version"]);
    assert_eq!(exit_code(&human), 0);
    assert_eq!(stdout_text(&human).trim(), "lekalo 0.1.22");
    let json = lekalo_in(&project, &["--json", "--version"]);
    assert_eq!(exit_code(&json), 0);
    assert_eq!(
        stdout_text(&json).trim_end(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.1.22\"\n}"
    );
}
