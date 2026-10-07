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
    // command: the report carries exactly that subject with its
    // cross-boundary witnesses, and no other subject row.
    let envelope = document(&output);
    let report = &envelope["portability"];
    assert_eq!(report["scope"]["kind"], "symbol");
    assert_eq!(report["scope"]["selector"], "planner.schedule_calculate");
    let subjects = report["subjects"].as_array().unwrap();
    assert_eq!(subjects.len(), 1);
    assert_eq!(subjects[0]["subject"], "command:planner.schedule_calculate");
    assert!(!subjects[0]["requirements"].as_array().unwrap().is_empty());
    // An unknown symbol is refused with the input diagnostic.
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
#[cfg(windows)]
fn a_junction_inside_the_project_never_escapes_the_root() {
    // The review probe: `config/` replaced by a directory junction to a
    // directory outside the project must not feed the assessment. The
    // confined read path rejects the reparse point instead of opening
    // through it.
    let project = fixture_path();
    let outside = std::env::temp_dir().join("lekalo-portability-junction-outside");
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).expect("create outside dir");
    std::fs::write(
        outside.join("portability-targets.json"),
        "{\"smuggled\":true}\n",
    )
    .expect("write outside file");
    let sandbox = std::env::temp_dir().join("lekalo-portability-junction-run");
    let _ = std::fs::remove_dir_all(&sandbox);
    copy_dir(&project, &sandbox);
    let junction = sandbox.join("config");
    std::fs::remove_dir_all(&junction).expect("remove real config");
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside)
        .status()
        .expect("create junction");
    assert!(status.success(), "junction creation must succeed");
    let output = lekalo_in(&sandbox, &["--json", "portability"]);
    // The default targets read must refuse (junction classified, no
    // smuggled config bound) and the run must never report a pinned
    // configuration digest of outside bytes.
    assert_ne!(exit_code(&output), 0, "junction read must not succeed");
    let _ = std::fs::remove_dir_all(&sandbox);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn an_empty_default_destination_list_is_refused_not_reinterpreted() {
    // `defaultDestinations: []` is schema-invalid (minItems 1); the
    // decoder must refuse it rather than silently assessing nothing.
    let mut config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture_path().join("config/portability-targets.json"))
            .expect("config"),
    )
    .unwrap();
    config["defaultDestinations"] = serde_json::Value::Array(vec![]);
    let empty_dir = std::env::temp_dir().join("lekalo-portability-empty-destinations");
    let _ = std::fs::remove_dir_all(&empty_dir);
    copy_dir(&fixture_path(), &empty_dir);
    std::fs::write(
        empty_dir.join("config/portability-targets.json"),
        serde_json::to_vec_pretty(&config).unwrap(),
    )
    .unwrap();
    let output = lekalo_in(&empty_dir, &["--json", "portability"]);
    assert_eq!(exit_code(&output), 1, "empty destinations are invalid");
    assert!(stderr_text(&output).contains("portability.input-invalid"));
    std::fs::remove_dir_all(&empty_dir).ok();
}

#[test]
fn a_mapping_for_another_destination_never_credits_this_row() {
    // The review probe: a mapping scoped to php-laravel (its capability
    // declared only partially there) must keep the go-service hook
    // blocked even when the go row would otherwise look mappable.
    let evidence =
        std::fs::read_to_string(fixture_path().join("reports/portability-evidence.json"))
            .expect("evidence");
    let mut manifest: serde_json::Value = serde_json::from_str(&evidence).unwrap();
    for mapping in manifest["mappings"].as_array_mut().unwrap().iter_mut() {
        if mapping["id"] == "map.schedule-php" {
            mapping["capability"] = "storage.returning".into();
            mapping["definitionPins"] = serde_json::json!([
                {"id": "storage.returning", "version": "0.2.16"}
            ]);
        }
    }
    let scoped_dir = std::env::temp_dir().join("lekalo-portability-scoped-mapping");
    let _ = std::fs::remove_dir_all(&scoped_dir);
    copy_dir(&fixture_path(), &scoped_dir);
    std::fs::write(
        scoped_dir.join("reports/portability-evidence.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let output = lekalo_in(
        &scoped_dir,
        &[
            "--json",
            "portability",
            "--evidence",
            "reports/portability-evidence.json",
        ],
    );
    assert_eq!(exit_code(&output), 0);
    let envelope = document(&output);
    let report = &envelope["portability"];
    let schedule = report["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["subject"] == "command:planner.schedule_calculate")
        .expect("schedule subject");
    let go_row = schedule["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["destination"] == "go-service" && r["dimension"] == "behavioral")
        .expect("go behavioral row");
    assert_ne!(
        go_row["classification"], "supported-with-mapping",
        "a php-scoped mapping must never green the go row"
    );
    assert_eq!(go_row["classification"], "unsupported");
    std::fs::remove_dir_all(&scoped_dir).ok();
}

#[test]
fn running_from_the_lekalo_subdirectory_still_reads_the_project_root() {
    // The loader resolves the project root through the ancestor walk;
    // artifact reads must bind to that same resolved root.
    let project = fixture_path();
    let subdir = project.join("lekalo/modules/planner");
    let output = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(["--json", "portability"])
        .current_dir(alias_free_path(&subdir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary");
    assert_eq!(
        exit_code(&output),
        0,
        "ancestor selection resolves the root"
    );
    let envelope = document(&output);
    let report = &envelope["portability"];
    assert_eq!(report["projectRef"]["projectId"], "planner");
    assert_eq!(
        report["inputs"]["targetsConfig"]["path"],
        "config/portability-targets.json"
    );
    assert_eq!(report["inputs"]["targetsConfig"]["state"], "pinned");
}

#[test]
fn a_stale_profile_pin_is_refused_before_any_assessment() {
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
