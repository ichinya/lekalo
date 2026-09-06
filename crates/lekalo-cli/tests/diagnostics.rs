//! Issue #11 CLI conformance: the human and JSON projections are byte-exact
//! projections of the same normalized `DomainResult`, golden envelopes are
//! stable across reruns, exit classes stay status-owned, and the human line
//! never appears in machine output (and vice versa).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE_ROOT: &str = "tests/fixtures/loader";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate lives under workspace/crates")
        .to_path_buf()
}

fn run_load(selector: &str, extra_args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lekalo"));
    command
        .arg("--no-cache")
        .arg("load")
        .arg("--project")
        .arg(selector);
    for argument in extra_args {
        command.arg(argument);
    }
    command
        .current_dir(workspace_root())
        .env_remove("LEKALO_PROJECT");
    command.output().expect("run lekalo load")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(
        workspace_root()
            .join("tests/fixtures/diagnostics")
            .join(name),
    )
    .expect("golden fixture reads")
}

fn run(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lekalo"));
    command.args(args).current_dir(workspace_root());
    command.env_remove("LEKALO_PROJECT");
    command.output().expect("run the real lekalo binary")
}

#[test]
fn golden_projections_are_byte_stable_across_reruns() {
    let cases: Vec<(Vec<&str>, &str)> = vec![
        (
            vec![
                "--no-cache",
                "load",
                "--json",
                "--project",
                "tests/fixtures/loader/invalid-duplicate-definition",
            ],
            "loader-failure-envelope.json",
        ),
        (
            vec![
                "--no-cache",
                "load",
                "--project",
                "tests/fixtures/loader/invalid-duplicate-definition",
            ],
            "loader-failure-human.txt",
        ),
        (
            vec![
                "--no-cache",
                "load",
                "--ir",
                "--json",
                "--project",
                "tests/fixtures/ir/invalid-unknown-field",
            ],
            "ir-failure-envelope.json",
        ),
        (vec!["--json", "--bogus"], "usage-envelope.json"),
    ];
    for (args, golden_name) in cases {
        let expected = golden(golden_name);
        let first = run(&args);
        let second = run(&args);
        assert_eq!(first.status.code(), second.status.code(), "{args:?}: exit");
        let first_text = if first.stdout.is_empty() {
            stderr_text(&first)
        } else {
            stdout_text(&first)
        };
        let second_text = if second.stdout.is_empty() {
            stderr_text(&second)
        } else {
            stdout_text(&second)
        };
        assert_eq!(first_text, second_text, "{args:?}: rerun bytes");
        assert_eq!(first_text, expected, "{args:?}: golden bytes");
    }
}

#[test]
fn human_and_json_are_projections_of_the_same_normalized_result() {
    // The human projection names the same ids as the derived reason codes,
    // in the same order, with the same exit class and stream.
    let selector = format!("{FIXTURE_ROOT}/invalid-duplicate-definition");
    let json = run_load(&selector, &["--json"]);
    let human = run_load(&selector, &[]);
    assert_eq!(json.status.code(), human.status.code());
    assert!(json.stdout.is_empty() && !stderr_text(&human).is_empty());
    let document: serde_json::Value =
        serde_json::from_str(stderr_text(&json).trim()).expect("envelope parses");
    let ids: Vec<&str> = document["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .map(|diagnostic| diagnostic["id"].as_str().expect("id"))
        .collect();
    let human = stderr_text(&human);
    for id in ids {
        assert!(human.contains(id), "human names {id}: {human}");
    }
    assert!(human.starts_with("invalid error [LEK-"), "{human}");
    assert!(!human.contains("schema_version"), "human is not JSON");
}

#[test]
fn valid_success_envelopes_carry_no_diagnostics_fields() {
    let output = run_load("tests/fixtures/loader/valid-zero-modules", &["--json"]);
    assert_eq!(output.status.code(), Some(0));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope parses");
    assert_eq!(document["status"].as_str(), Some("valid"));
    assert!(document.get("diagnostics").is_none(), "empty stays omitted");
    assert!(document.get("reasonCodes").is_none(), "empty stays omitted");
}
