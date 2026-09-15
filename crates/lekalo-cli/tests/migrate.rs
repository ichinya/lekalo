//! The development baseline has no historical migration steps.
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn current_baseline_dry_run_is_empty_and_deterministic() {
    let args = [
        "migrate",
        "--project",
        "tests/fixtures/model-v1/valid-minimal",
        "--to",
        "model/0.2.16",
        "--dry-run",
        "--json",
    ];
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_lekalo"))
            .args(args)
            .current_dir(root())
            .output()
            .unwrap()
    };
    let first = run();
    let second = run();
    assert!(first.status.success(), "{:?}", first);
    assert!(first.stderr.is_empty());
    assert_eq!(first.stdout, second.stdout);
    let result: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(result["changed"], false);
    assert_eq!(result["chain"], serde_json::json!([]));
    assert_eq!(result["files"], serde_json::json!([]));
    assert_eq!(result["from"], "0.2.16");
    assert_eq!(result["to"], "0.2.16");
    assert!(!root()
        .join("tests/fixtures/model-v1/valid-minimal/.lekalo/cache/migrations")
        .exists());
}

#[test]
fn historical_target_is_not_registered() {
    let output = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "migrate",
            "--project",
            "tests/fixtures/model-v1/valid-minimal",
            "--to",
            "model/1.0.0",
            "--dry-run",
            "--json",
        ])
        .current_dir(root())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    let result: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(result["status"], "unsupported-version");
}
