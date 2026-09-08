//! Issue #16 CLI tests for the `lekalo impact` handoff: the symbol mode
//! over the hermetic planner fixture, the typed `--changed` Git adapter in
//! committed and worktree modes, the selector grammar, the strict-profile
//! denial, canonical byte stability, and the 0.2.1 version custody probe.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings (and absolute selectors) before
//! any command logic runs, so each test copies the fixture into a temp
//! directory and chdirs the child there. The Git fixtures are hermetic
//! local repositories (no network, no fetch); Git runs only through the
//! test's own read-only setup commands.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/impact/planner";
const RENAME_FIXTURE: &str = "tests/fixtures/impact/rename";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

fn git_in(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(alias_free_path(dir))
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .canonicalize()
        .expect("workspace root")
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("entry");
        let target_path = target.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            std::fs::copy(entry.path(), target_path).expect("copy file");
        }
    }
}

/// One alias-free temp directory holding a fresh fixture copy.
fn fixture_copy(tag: &str, fixture: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lekalo-impact-cli-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    copy_dir(&workspace_root().join(fixture), &dir);
    dir
}

/// A hermetic local Git repository holding one committed copy of the
/// planner fixture, with a deterministic identity and no network.
fn git_repo(tag: &str) -> PathBuf {
    let project = fixture_copy(tag, FIXTURE);
    git_in(&project, &["init", "-q", "--initial-branch=main", "."]);
    git_in(&project, &["add", "-A"]);
    git_in(&project, &["commit", "-qm", "base"]);
    project
}

/// One deterministic, valid model edit on the entity description.
fn touch_entity(project: &Path) {
    let entities = project.join("lekalo/modules/planner/entities.yaml");
    let source = std::fs::read_to_string(&entities).expect("entities source");
    let updated = source.replace("A focusable task", "A focusable task v2");
    std::fs::write(&entities, updated).expect("entities update");
}

#[test]
fn symbol_mode_answers_on_the_valid_envelope() {
    let dir = fixture_copy("symbol", FIXTURE);
    let human = lekalo_in(&dir, &["impact", "planner.task"]);
    assert_eq!(exit_code(&human), 0, "{}", stderr_text(&human));
    let stdout = stdout_text(&human);
    assert!(stdout.starts_with("impact symbol (entity:planner.task)"));
    assert!(stdout.contains("direct 6"));
    assert!(stdout.contains("transitive 6"));
    assert_lf_only(&stdout);

    let json = lekalo_in(&dir, &["--json", "impact", "planner.task"]);
    assert_eq!(exit_code(&json), 0);
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&json).trim_end()).expect("json envelope");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["impact"]["schemaVersion"], "lekalo/impact/v1.0.0");
    assert_eq!(document["impact"]["identity"], "dev.lekalo.impact@1.0.0");
    assert_eq!(document["impact"]["roots"][0], "entity:planner.task");
    assert_eq!(document["impact"]["direct"]["returned"], 6);
    assert_eq!(document["impact"]["transitive"]["returned"], 6);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn unknown_symbol_is_invalid_exit_one_on_stderr() {
    let dir = fixture_copy("unknown", FIXTURE);
    let output = lekalo_in(&dir, &["impact", "planner.nonexistent"]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("impact.symbol-unknown"));
    assert!(stdout_text(&output).is_empty());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn changed_worktree_mode_resolves_the_changed_entity() {
    let project = git_repo("worktree");
    touch_entity(&project);

    let output = lekalo_in(&project, &["--json", "impact", "--changed", "--worktree"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim_end()).expect("json envelope");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["impact"]["input"]["mode"], "worktree");
    assert!(document["impact"]["input"]["changedInputDigest"]
        .as_str()
        .expect("digest")
        .starts_with("sha256:"));
    assert!(
        document["impact"]["roots"]
            .as_array()
            .expect("roots")
            .iter()
            .any(|root| root == "entity:planner.task"),
        "the changed entity resolves as a root"
    );
    assert!(document["impact"]["input"]["baseRevisionRef"].is_null());
    std::fs::remove_dir_all(&project).expect("cleanup");
}

#[test]
fn changed_committed_mode_resolves_base_and_candidate() {
    let project = git_repo("committed");
    touch_entity(&project);
    git_in(&project, &["add", "-A"]);
    git_in(&project, &["commit", "-qm", "change"]);

    let output = lekalo_in(
        &project,
        &["--json", "impact", "--changed", "--base", "HEAD~1"],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim_end()).expect("json envelope");
    assert_eq!(document["impact"]["input"]["mode"], "committed");
    let base = document["impact"]["input"]["baseRevisionRef"]
        .as_str()
        .expect("base revision");
    let candidate = document["impact"]["input"]["candidateRevisionRef"]
        .as_str()
        .expect("candidate revision");
    assert_eq!(base.len(), 40, "resolved base commit identity");
    assert_eq!(candidate.len(), 40, "resolved candidate commit identity");
    assert_ne!(base, candidate);
    assert!(document["impact"]["roots"]
        .as_array()
        .expect("roots")
        .iter()
        .any(|root| root == "entity:planner.task"));
    std::fs::remove_dir_all(&project).expect("cleanup");
}

#[test]
fn changed_mode_without_changes_is_an_explicit_fault() {
    let project = git_repo("clean");
    let output = lekalo_in(&project, &["impact", "--changed", "--worktree"]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("impact.changed-input-invalid"));
    std::fs::remove_dir_all(&project).expect("cleanup");
}

#[test]
fn strict_profile_denial_uses_the_denied_envelope() {
    let dir = fixture_copy("strict", RENAME_FIXTURE);
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "impact",
            "planner.focus_task",
            "--profile",
            "strict",
        ],
    );
    assert_eq!(exit_code(&output), 3, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim_end()).expect("json envelope");
    assert_eq!(document["status"], "denied");
    assert_eq!(document["reasonCodes"][0], "impact.gate-blocked");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn canonical_bytes_are_byte_identical_across_repeat_runs() {
    let dir = fixture_copy("repeat", FIXTURE);
    let first = lekalo_in(&dir, &["--json", "impact", "planner.task"]);
    let second = lekalo_in(&dir, &["--json", "impact", "planner.task"]);
    assert_eq!(exit_code(&first), 0);
    assert_eq!(
        stdout_text(&first),
        stdout_text(&second),
        "byte-identical reruns"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn version_probe_prints_the_prospective_product() {
    let dir = std::env::temp_dir();
    let output = lekalo_in(&dir, &["--version"]);
    assert_eq!(exit_code(&output), 0);
    assert_eq!(stdout_text(&output).trim_end(), "lekalo 0.2.1");
    let json = lekalo_in(&dir, &["--json", "--version"]);
    assert_eq!(exit_code(&json), 0);
    assert_eq!(
        stdout_text(&json).trim_end(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.2.1\"\n}"
    );
}

fn assert_lf_only(text: &str) {
    assert!(text.ends_with('\n'), "one final LF");
    assert!(!text.contains("\r\n"), "no CRLF");
}
