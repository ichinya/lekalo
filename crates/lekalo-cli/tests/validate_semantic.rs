//! Issue #12 CLI tests for `lekalo validate`: the per-rule fixture matrix,
//! the valid/golden and profile-difference behavior, module scoping, the
//! 0/1 exit classes, and byte-identical determinism.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURES: &str = "tests/fixtures/validation";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    // The pre-cache suites pin the published --no-cache path; cache-on
    // behavior is exercised by tests/cache.rs.
    let mut args = args.to_vec();
    args.insert(0, "--no-cache");
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run the real lekalo binary")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
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

/// Every invalid fixture fails with exactly its registered rule, on the
/// invalid envelope (exit 1, stderr), carrying one located diagnostic.
#[test]
fn every_invalid_fixture_fires_exactly_its_registered_rule() {
    let invalid = workspace_path("tests/fixtures/validation/invalid");
    let mut names: Vec<String> = std::fs::read_dir(&invalid)
        .expect("invalid fixtures")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "fixture matrix is present");

    for name in names {
        let project = format!("{FIXTURES}/invalid/{name}");
        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(invalid.join(&name).join("expect.json")).expect("expect.json"),
        )
        .expect("expectation parses");
        let rule = expected["reasonCodes"][0].as_str().expect("reason code");

        let output = lekalo_in(
            &workspace_path("."),
            &["validate", "--json", "--project", &project],
        );
        assert_eq!(output.status.code(), Some(1), "{name}: exit");
        assert!(output.stdout.is_empty(), "{name}: stdout");
        let envelope: serde_json::Value =
            serde_json::from_str(stderr_text(&output).trim()).expect("{name}: envelope");
        assert_eq!(envelope["status"], "invalid", "{name}");
        let diagnostics = envelope["diagnostics"].as_array().expect("diagnostics");
        assert_eq!(diagnostics.len(), 1, "{name}: exactly one diagnostic");
        assert_eq!(diagnostics[0]["id"], rule, "{name}: rule id");
        assert_eq!(envelope["reasonCodes"], serde_json::json!([rule]), "{name}");
        // Every source-derived rule carries its exact span and subject.
        assert!(
            diagnostics[0]["source"].is_object() || diagnostics[0]["symbol"] == "planner",
            "{name}: located or symbol-bound"
        );
    }
}

/// The clean base project validates with exit 0 and byte-identical reruns.
#[test]
fn valid_base_is_exit_zero_and_byte_identical_across_reruns() {
    let project = format!("{FIXTURES}/valid/base");
    let first = lekalo_in(
        &workspace_path("."),
        &["validate", "--strict", "--json", "--project", &project],
    );
    assert_eq!(first.status.code(), Some(0));
    let second = lekalo_in(
        &workspace_path("."),
        &["validate", "--strict", "--json", "--project", &project],
    );
    assert_eq!(first.stdout, second.stdout, "determinism");
    assert!(second.stderr.is_empty());

    let golden = std::fs::read_to_string(workspace_path(
        "tests/fixtures/validation/golden/valid-base-strict-envelope.json",
    ))
    .expect("golden envelope");
    assert_eq!(stdout_text(&first), golden, "golden strict envelope bytes");

    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&first).trim()).expect("envelope");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["modelVersion"], "0.1.0");
    assert_eq!(document["validation"]["profile"], "strict");
    assert_eq!(document["validation"]["registryVersion"], "1.21.0");
    assert_eq!(document["validation"]["counts"]["error"], 0);
}

/// The default human projection is the stable one-line summary.
#[test]
fn default_human_summary_matches_the_golden_line() {
    let project = format!("{FIXTURES}/valid/base");
    let output = lekalo_in(&workspace_path("."), &["validate", "--project", &project]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let golden = std::fs::read_to_string(workspace_path(
        "tests/fixtures/validation/golden/valid-base-default-human.txt",
    ))
    .expect("golden human");
    assert_eq!(stdout_text(&output), golden);
}

/// The two built-in profiles differ exactly in the recorded portability
/// downgrade: default reports it at info, strict keeps the warning.
#[test]
fn strict_keeps_the_warning_the_default_profile_downgrades_to_info() {
    let project = format!("{FIXTURES}/warning/portable-target-reference");

    let default = lekalo_in(
        &workspace_path("."),
        &["validate", "--json", "--project", &project],
    );
    assert_eq!(default.status.code(), Some(0), "warnings never fail");
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&default).trim()).expect("envelope");
    assert_eq!(document["validation"]["profile"], "default");
    assert_eq!(document["validation"]["counts"]["info"], 1);
    assert_eq!(document["validation"]["counts"]["warning"], 0);
    assert_eq!(document["diagnostics"][0]["severity"], "info");
    assert_eq!(
        document["diagnostics"][0]["id"],
        "semantic.portable-target-reference"
    );

    let strict = lekalo_in(
        &workspace_path("."),
        &["validate", "--strict", "--json", "--project", &project],
    );
    assert_eq!(strict.status.code(), Some(0));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&strict).trim()).expect("envelope");
    assert_eq!(document["validation"]["profile"], "strict");
    assert_eq!(document["validation"]["counts"]["warning"], 1);
    assert_eq!(document["diagnostics"][0]["severity"], "warning");
}

/// An unknown `--module` scope fails closed before any rule runs.
#[test]
fn unknown_module_scope_is_a_stable_failure() {
    let project = format!("{FIXTURES}/valid/base");
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "validate",
            "--json",
            "--module",
            "ghost",
            "--project",
            &project,
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let envelope: serde_json::Value =
        serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(envelope["status"], "invalid");
    assert_eq!(
        envelope["diagnostics"][0]["id"],
        "validate.module-unresolved"
    );
    assert_eq!(envelope["diagnostics"][0]["data"]["module"], "ghost");
    assert_eq!(
        envelope["reasonCodes"],
        serde_json::json!(["validate.module-unresolved"])
    );
}

/// Module scoping keeps every error: a planner-owned error survives an
/// `--module audit` run, so mandatory cross-module failures are not hidden.
#[test]
fn module_scope_does_not_hide_cross_module_errors() {
    let project = format!("{FIXTURES}/invalid/visibility-boundary");
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "validate",
            "--json",
            "--module",
            "audit",
            "--project",
            &project,
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let envelope: serde_json::Value =
        serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    let ids: Vec<&str> = envelope["reasonCodes"]
        .as_array()
        .expect("reasons")
        .iter()
        .map(|value| value.as_str().expect("id"))
        .collect();
    assert!(ids.contains(&"semantic.visibility-boundary-violation"));
}

/// The validator never writes: the project tree is byte-identical after a
/// validating run.
#[test]
fn validation_never_mutates_the_project() {
    let source = workspace_path(FIXTURES).join("valid/base");
    let dir = workspace_path("target").join(format!(
        "lekalo-validate-immutable-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    copy_dir(&source, &dir);
    let selector = format!(
        "target/{}",
        dir.file_name().expect("name").to_string_lossy()
    );

    fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
            for entry in std::fs::read_dir(dir).expect("read dir") {
                let entry = entry.expect("entry");
                let path = entry.path();
                let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
                if path.is_dir() {
                    walk(&path, &name, out);
                } else {
                    out.push((name, std::fs::read(&path).expect("read file")));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, "", &mut out);
        out.sort();
        out
    }

    let before = snapshot(&dir);
    let output = lekalo_in(
        &workspace_path("."),
        &["validate", "--json", "--project", &selector],
    );
    assert_eq!(output.status.code(), Some(0));
    let after = snapshot(&dir);
    assert_eq!(before, after, "no file appeared, disappeared, or changed");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
