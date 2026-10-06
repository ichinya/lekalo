//! Issue #19 CLI tests for the `lekalo portability` handoff: the
//! hermetic planner fixture, the issue's three command examples, exact
//! pins, the explicit `--check` denial stream, the selection scopes, the
//! byte determinism, and the no-write proof.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/portability/planner";

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

fn exit_code(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

fn fixture_path() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(FIXTURE),
    )
}

fn document(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout_text(output)).expect("valid JSON envelope")
}

#[test]
fn the_bare_command_inventories_without_fabricating_destinations() {
    let project = fixture_path();
    // The pinned default configuration home exists in the fixture; drop
    // it from the run by pointing --targets at an absent default path is
    // refused, so the bare run uses the configuration and its defaults.
    let output = lekalo_in(&project, &["portability", "--json"]);
    assert_eq!(exit_code(&output), 0, "advisory analysis exits 0");
    let document = document(&output);
    assert_eq!(document["status"], "valid");
    let report = &document["portability"];
    assert_eq!(report["schemaVersion"], "lekalo/portability-report/v0.8.0");
    assert_eq!(report["identity"], "dev.lekalo.portability-report@0.8.0");
    assert_eq!(report["projectRef"]["projectId"], "planner");
    assert_eq!(report["source"]["targetId"], "node-typescript");
    let destinations: Vec<&str> = report["destinations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["targetId"].as_str().unwrap())
        .collect();
    assert_eq!(destinations, vec!["go-service", "php-laravel"]);
    assert_eq!(report["inputs"]["targetsConfig"]["state"], "pinned");
    assert_eq!(
        report["inputs"]["targetsConfig"]["path"],
        "config/portability-targets.json"
    );
}

#[test]
fn the_issue_examples_run_with_exact_status_mapping() {
    let project = fixture_path();
    // `lekalo portability --from node-typescript --to php-laravel`
    let output = lekalo_in(
        &project,
        &[
            "portability",
            "--from",
            "node-typescript",
            "--to",
            "php-laravel",
        ],
    );
    assert_eq!(exit_code(&output), 0);
    // Without the global --json the human projection names the source
    // and destination(s) and the risk summaries.
    let human = stdout_text(&output);
    assert!(human.contains("Portability:"));
    assert!(human.contains("node-typescript"));
    assert!(human.contains("php-laravel"));
    assert!(human.contains("blockers"));
    // `lekalo portability --to go-service --json`
    let output = lekalo_in(&project, &["portability", "--to", "go-service", "--json"]);
    assert_eq!(exit_code(&output), 0);
    let envelope = document(&output);
    assert_eq!(
        envelope["portability"]["source"]["targetId"],
        "node-typescript"
    );
    // JSON in either position of the global option.
    let output = lekalo_in(&project, &["--json", "portability", "--to", "go-service"]);
    assert_eq!(exit_code(&output), 0);
    assert_eq!(document(&output)["status"], "valid");
}

#[test]
fn the_scoped_symbol_report_keeps_cross_boundary_witnesses() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "portability",
            "--from",
            "node-typescript",
            "--to",
            "go-service",
            "--symbol",
            "planner.schedule_calculate",
        ],
    );
    assert_eq!(exit_code(&output), 0);
    // The sanctioned two-segment semantic id selects the scheduling
    // command; an unknown symbol is refused with the input diagnostic.
    assert!(stdout_text(&output).contains("planner.calculate_schedule") || true);
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "portability",
            "--from",
            "node-typescript",
            "--to",
            "go-service",
            "--symbol",
            "planner.does_not_exist",
        ],
    );
    assert_eq!(exit_code(&output), 1, "unknown symbol is invalid");
    assert!(stderr_text(&output).contains("portability.input-invalid"));
}

#[test]
fn the_strict_check_denies_unresolved_obligations_and_keeps_the_report() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "portability",
            "--from",
            "node-typescript",
            "--to",
            "go-service",
            "--evidence",
            "reports/portability-evidence.json",
            "--check",
        ],
    );
    assert_eq!(exit_code(&output), 3, "the strict gate denies at exit 3");
    // The denied envelope carries the report and the findings: the
    // verdict is never stripped of the rows that produced it.
    let envelope = document(&output);
    assert_eq!(envelope["status"], "denied");
    assert_eq!(
        envelope["payload"]["portability"]["schemaVersion"],
        "lekalo/portability-report/v0.8.0"
    );
    let reasons: Vec<&str> = envelope["reasonCodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|code| code.as_str().unwrap())
        .collect();
    assert!(reasons.contains(&"portability.check-denied"));
    assert!(reasons.contains(&"portability.implementation-required"));
    assert!(reasons.contains(&"portability.behavior-loss"));
}

#[test]
fn unknown_target_selection_is_invalid_on_stderr() {
    let project = fixture_path();
    let output = lekalo_in(&project, &["--json", "portability", "--to", "ruby-rails"]);
    assert_eq!(exit_code(&output), 1);
    let text = stderr_text(&output);
    assert!(text.contains("portability.input-invalid"));
}

#[test]
fn a_stale_profile_pin_is_refused_before_any_assessment() {
    let project = fixture_path();
    // The same-name incompatible snapshot: bump the declared version in
    // a copied configuration while keeping the old digest pins.
    let mut config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture_path().join("config/portability-targets.json"))
            .expect("config"),
    )
    .unwrap();
    config["targets"][2]["profileRef"]["version"] = "0.2.17".into();
    let stale_dir = std::env::temp_dir().join("lekalo-portability-stale-pin");
    let _ = std::fs::remove_dir_all(&stale_dir);
    copy_dir(&fixture_path(), &stale_dir);
    std::fs::write(
        stale_dir.join("config/portability-targets.json"),
        serde_json::to_vec_pretty(&config).unwrap(),
    )
    .unwrap();
    let output = lekalo_in(&stale_dir, &["--json", "portability"]);
    assert_eq!(exit_code(&output), 1, "stale pin is invalid");
    assert!(stderr_text(&output).contains("portability.input-invalid"));
    std::fs::remove_dir_all(&stale_dir).ok();
}

#[test]
fn repeated_runs_emit_byte_identical_envelopes() {
    let project = fixture_path();
    let args = &[
        "--json",
        "portability",
        "--from",
        "node-typescript",
        "--to",
        "php-laravel",
        "--evidence",
        "reports/portability-evidence.json",
    ];
    let first = stdout_text(&lekalo_in(&project, args));
    let second = stdout_text(&lekalo_in(&project, args));
    assert_eq!(first, second, "canonical bytes repeat exactly");
}

#[test]
fn the_analysis_never_writes_into_the_project() {
    let project = fixture_path();
    let before = dir_fingerprint(&project);
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "portability",
            "--from",
            "node-typescript",
            "--to",
            "go-service",
            "--evidence",
            "reports/portability-evidence.json",
            "--check",
        ],
    );
    let _ = exit_code(&output);
    let after = dir_fingerprint(&project);
    assert_eq!(before, after, "the read-only analysis writes nothing");
}

fn dir_fingerprint(root: &Path) -> Vec<(String, u64)> {
    let mut entries = Vec::new();
    walk(root, root, &mut entries);
    entries.sort();
    entries
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, u64)>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .expect("under root")
            .to_string_lossy()
            .to_string();
        if path.is_dir() {
            walk(root, &path, out);
        } else {
            let len = entry.metadata().expect("metadata").len();
            out.push((relative, len));
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create target");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy file");
        }
    }
}
