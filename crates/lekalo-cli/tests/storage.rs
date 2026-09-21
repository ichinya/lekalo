//! Issue #117 CLI tests: every new `storage`/`storage-profile`
//! subcommand's envelope on the accepted 0/1/3/4/5 exit surface, the
//! data-only verdicts, and the hermetic read-only behavior — no
//! database connection flag exists anywhere on the surface.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PROJECTION: &str = "tests/fixtures/storage-projection/valid/planner-storage.json";
const MYSQL_PROFILE: &str = "tests/fixtures/storage-engine-profile/valid/mysql-8.0.json";
const MARIADB_PROFILE: &str = "tests/fixtures/storage-engine-profile/valid/mariadb-10.11.json";
const EVIDENCE: &str = "tests/fixtures/storage-introspection/valid/mysql-8.0-planner.json";

fn lekalo(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .output()
        .expect("run the real lekalo binary")
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

#[test]
fn storage_validate_emits_the_summary_envelope() {
    let path = workspace_path(PROJECTION);
    let output = lekalo(&[
        "--json",
        "storage",
        "validate",
        path.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("\"projectId\":\"planner\""));
    assert!(stdout.contains("\"namespaces\":[\"laravel\",\"mariadb\",\"mysql\",\"postgres\"]"));
}

#[test]
fn storage_project_emits_the_derived_canonical_bytes() {
    let path = workspace_path(PROJECTION);
    let output = lekalo(&[
        "--json",
        "storage",
        "project",
        path.to_str().expect("path"),
        "--namespace",
        "mysql",
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let stdout = stdout(&output);
    // The canonical derived bytes are the committed golden's payload.
    let golden = std::fs::read_to_string(workspace_path(
        "tests/fixtures/storage-projection/derived/mysql.json",
    ))
    .expect("golden");
    assert_eq!(stdout.trim_end(), golden.trim_end());
}

#[test]
fn storage_diff_classifies_and_stays_data() {
    let base = workspace_path("tests/fixtures/storage-projection/diff/base.json");
    let candidate = workspace_path("tests/fixtures/storage-projection/diff/candidate-storage.json");
    let output = lekalo(&[
        "--json",
        "storage",
        "diff",
        base.to_str().expect("base"),
        candidate.to_str().expect("candidate"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("\"equal\":false"));
    assert!(stdout.contains("\"breaking\":1"));
    // The data-risk evidence rides the path record.
    assert!(stdout.contains("\"risk\":\"destructive\""));
}

#[test]
fn storage_introspect_check_reports_drift_as_data() {
    let projection = workspace_path(PROJECTION);
    let evidence = workspace_path(EVIDENCE);
    let output = lekalo(&[
        "--json",
        "storage",
        "introspect-check",
        "--projection",
        projection.to_str().expect("projection"),
        "--evidence",
        evidence.to_str().expect("evidence"),
        "--namespace",
        "mysql",
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let stdout = stdout(&output);
    // The agreeing facts of the fixture evidence hold; the drift
    // entries stay closed kinds, never repairs.
    assert!(stdout.contains("\"status\":\"valid\""));
    assert!(stdout.contains("\"introspectCheck\""));
    assert!(stdout.contains("\"kind\":\"missing-table\""));
}

#[test]
fn storage_profile_validate_and_capabilities_emit_summaries() {
    let path = workspace_path(MYSQL_PROFILE);
    let output = lekalo(&[
        "--json",
        "storage-profile",
        "validate",
        path.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let validate_stdout = stdout(&output);
    assert!(validate_stdout.contains("\"engine\":\"mysql\""));
    assert!(validate_stdout.contains("\"engineVersion\":\"8.0.36\""));

    let output = lekalo(&[
        "--json",
        "storage-profile",
        "capabilities",
        path.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let capabilities_stdout = stdout(&output);
    assert!(capabilities_stdout
        .contains("\"capability\":\"isolation.repeatable_read\",\"support\":\"full\""));
    assert!(capabilities_stdout
        .contains("\"capability\":\"isolation.snapshot\",\"support\":\"unsupported\""));
    assert!(capabilities_stdout.contains("\"capability\":\"lock.range\",\"support\":\"partial\""));
}

#[test]
fn storage_profile_portability_names_the_deltas() {
    let base = workspace_path(MYSQL_PROFILE);
    let target = workspace_path(MARIADB_PROFILE);
    let output = lekalo(&[
        "--json",
        "storage-profile",
        "portability",
        base.to_str().expect("base"),
        target.to_str().expect("target"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let portability_stdout = stdout(&output);
    assert!(portability_stdout.contains("\"source\":\"mysql\""));
    assert!(portability_stdout.contains("\"target\":\"mariadb\""));
    assert!(portability_stdout.contains("\"storage.sequences\""));
    // The named PostgreSQL divergence block is opt-in.
    assert!(!portability_stdout.contains("postgresDivergences"));
    let output = lekalo(&[
        "--json",
        "storage-profile",
        "portability",
        base.to_str().expect("base"),
        target.to_str().expect("target"),
        "--postgres-divergences",
    ]);
    assert_eq!(output.status.code(), Some(0));
    let divergences_stdout = stdout(&output);
    assert!(divergences_stdout.contains("postgresDivergences"));
    assert!(divergences_stdout.contains("deferrableConstraints"));
}

#[test]
fn storage_profile_diff_classifies_profile_changes() {
    let base = workspace_path(MYSQL_PROFILE);
    let target = workspace_path(MARIADB_PROFILE);
    let output = lekalo(&[
        "--json",
        "storage-profile",
        "diff",
        base.to_str().expect("base"),
        target.to_str().expect("target"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("\"equal\":false"));
    assert!(
        stdout.contains("\"breaking\":2"),
        "capability narrowings are breaking"
    );
}

#[test]
fn storage_plan_gates_destructive_steps_and_confirms_by_identity() {
    let base = workspace_path("tests/fixtures/storage-projection/diff/base.json");
    let candidate = workspace_path("tests/fixtures/storage-projection/diff/candidate-storage.json");
    let output = lekalo(&[
        "--json",
        "storage",
        "plan",
        base.to_str().expect("base"),
        candidate.to_str().expect("candidate"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let plan_stdout = stdout(&output);
    assert!(plan_stdout.contains("\"acknowledged\":false"));
    assert!(plan_stdout.contains("\"gate\":\"explicit\""));
    assert!(plan_stdout.contains("\"risk\":\"destructive\""));
    // The plan identity is content-bound: extract and re-confirm.
    let plan: serde_json::Value = serde_json::from_str(plan_stdout.trim()).expect("json");
    let plan_id = plan["plan"]["planId"].as_str().expect("plan id");
    let output = lekalo(&[
        "--json",
        "storage",
        "plan",
        base.to_str().expect("base"),
        candidate.to_str().expect("candidate"),
        "--confirm",
        plan_id,
    ]);
    assert_eq!(output.status.code(), Some(0), "{:?}", stderr(&output));
    let confirmed_stdout = stdout(&output);
    assert!(confirmed_stdout.contains("\"acknowledged\":true"));
    // A wrong identity refuses as stale, never acknowledges.
    let output = lekalo(&[
        "--json",
        "storage",
        "plan",
        base.to_str().expect("base"),
        candidate.to_str().expect("candidate"),
        "--confirm",
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    ]);
    assert_eq!(output.status.code(), Some(1), "{:?}", stderr(&output));
    assert!(stderr(&output).contains("plan-changed"));
}

#[test]
fn missing_documents_are_typed_invalid_never_panics() {
    let output = lekalo(&["storage", "validate", "missing-file.json"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("storage.input-invalid"));
    let output = lekalo(&["storage-profile", "validate", "missing-file.json"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}
