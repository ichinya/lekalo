//! Issue #10 CLI tests for `lekalo lock`: create/check behavior, the
//! committed golden bytes, refusal classification on the accepted
//! 0/1/3/4/5 envelope, and byte-identical determinism across two projects.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GOLDEN: &str = include_str!("../../../tests/fixtures/lockfile/valid/contract-only.lock.json");
const GOLDEN_DIGEST: &str =
    "sha256:12427804a273f3a3b5d9d258f3b811b9d1b7c48f3cf1c032d25274f0917aaf7e";
const REFERENCE_PROJECT: &str = "tests/fixtures/lockfile/project";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run the real lekalo binary")
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

fn copy_reference_project(root: &Path) {
    copy_dir(&workspace_path(REFERENCE_PROJECT), root);
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

fn project_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lekalo-lock-cli-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp project dir");
    copy_reference_project(&dir);
    dir
}

#[test]
fn lock_create_then_check_produces_the_committed_golden_bytes() {
    let dir = project_dir("golden");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let bytes = std::fs::read(dir.join("lekalo.lock")).expect("created lock");
    assert_eq!(bytes, GOLDEN.as_bytes(), "created bytes match the golden");

    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(0));
    assert_lf_only(&stdout(&output));
    let output = lekalo_in(&dir, &["--json", "lock"]);
    assert_eq!(output.status.code(), Some(0));
    let document: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("json envelope");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["operation"], "lock");
    assert_eq!(document["mode"], "check");
    assert_eq!(document["lockDigest"], GOLDEN_DIGEST);
    assert_eq!(document["resolverVersion"], "1.0.0");
    assert_eq!(document["counts"]["adapters"], 0);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn lock_create_human_line_is_stable_and_deterministic_across_projects() {
    let dir_a = project_dir("det-a");
    let dir_b = project_dir("det-b");
    let output_a = lekalo_in(&dir_a, &["lock"]);
    let output_b = lekalo_in(&dir_b, &["lock"]);
    assert_eq!(output_a.status.code(), Some(0));
    assert_eq!(output_a.status.code(), output_b.status.code());
    assert_eq!(stdout(&output_a), stdout(&output_b), "byte-identical runs");
    assert!(
        stdout(&output_a).starts_with("lock created sha256:"),
        "stable human projection: {}",
        stdout(&output_a)
    );
    std::fs::remove_dir_all(&dir_a).expect("cleanup");
    std::fs::remove_dir_all(&dir_b).expect("cleanup");
}

#[test]
fn lock_check_refuses_a_missing_lock_without_creating_it() {
    let dir = project_dir("missing");
    let output = lekalo_in(&dir, &["lock", "--check"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        stderr(&output).contains("lock.missing"),
        "typed refusal: {}",
        stderr(&output)
    );
    assert!(!dir.join("lekalo.lock").exists(), "zero writes on refusal");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn digest_mismatch_is_a_stdout_denial_distinct_from_validation_failure() {
    let dir = project_dir("tamper");
    std::fs::write(
        dir.join("lekalo.lock"),
        GOLDEN.replace("sha256:2cba65b0", "sha256:3cba65b0"),
    )
    .expect("tampered lock");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(stdout(&output).contains("lock.digest-mismatch"));
    assert!(stderr(&output).is_empty(), "denials stay on stdout");

    // Validation failure stays on stderr with exit 1.
    let broken = "{ not json }";
    std::fs::write(dir.join("lekalo.lock"), broken).expect("broken lock");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("lock.schema-invalid"));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn noncanonical_locks_are_refused_and_never_rewritten() {
    let dir = project_dir("noncanonical");
    let payload = GOLDEN.trim_end_matches('\n');
    std::fs::write(dir.join("lekalo.lock"), format!("{payload}\n\n")).expect("noncanonical");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("lock.noncanonical"));
    // --locked never silently rewrites: the bytes are untouched.
    let bytes = std::fs::read(dir.join("lekalo.lock")).expect("bytes");
    assert_eq!(bytes, format!("{payload}\n\n").as_bytes());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_lock_with_adapters_under_the_unpublished_protocol_is_refused() {
    let dir = project_dir("protocol");
    let multi = std::fs::read_to_string(workspace_path(
        "tests/fixtures/lockfile/valid/multi-adapter.lock.json",
    ))
    .expect("multi-adapter fixture");
    let value: serde_json::Value =
        serde_json::from_str(multi.trim_end_matches('\n')).expect("fixture json");
    std::fs::write(
        dir.join("lekalo.lock"),
        serde_json::to_string(&value).expect("wire"),
    )
    .expect("write lock");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    assert!(stderr(&output).contains("versioning.protocol-unpublished"));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn flag_exclusivity_and_unknown_flags_map_to_cli_usage() {
    let dir = project_dir("usage");
    // `--offline` composes with create and check.
    let output = lekalo_in(&dir, &["lock", "--offline"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let output = lekalo_in(&dir, &["lock", "--offline", "--check"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    // An unknown flag maps to the stable usage failure (JSON with --json).
    let output = lekalo_in(&dir, &["--json", "lock", "--bogus"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "{\n  \"status\": \"invalid\",\n  \"reasonCodes\": [\n    \"cli.usage\"\n  ]\n}\n"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
