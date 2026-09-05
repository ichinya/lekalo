//! Issue #10 CLI tests for `lekalo update`: preview without writes,
//! plan-bound apply, compare-and-swap refusals, and the no-op guarantee.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .output()
        .expect("run the real lekalo binary")
}

/// GitHub's Windows runners export `%TEMP%` spelled with the 8.3 alias of
/// the profile directory (`C:\Users\RUNNER~1\AppData\Local\Temp`), and the
/// selection policy denies alias spellings (`structure.selection-alias`)
/// before any command logic runs. Chdir the child into the resolved,
/// alias-free spelling; `canonicalize` returns it under a `\\?\` verbatim
/// prefix that is stripped back to the plain drive form.
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

fn json(output: &Output) -> Value {
    let text = if output.stdout.is_empty() {
        stderr(output)
    } else {
        stdout(output)
    };
    serde_json::from_str(&text).expect("json envelope")
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
        "lekalo-update-cli-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp project dir");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/lockfile/project"),
        &dir,
    );
    dir
}

fn preview(dir: &Path) -> (Output, Value) {
    let output = lekalo_in(dir, &["--json", "update", "--dry-run"]);
    let document = json(&output);
    (output, document)
}

#[test]
fn plain_update_is_refused_until_a_preview_is_bound() {
    let dir = project_dir("plain");
    let output = lekalo_in(&dir, &["update"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("lock.preview-required"));
    assert!(!dir.join("lekalo.lock").exists(), "no writes");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn dry_run_and_apply_flags_are_mutually_exclusive() {
    let dir = project_dir("exclusive");
    let plan_id = "sha256:".to_owned() + &"a".repeat(64);
    let output = lekalo_in(&dir, &["update", "--dry-run", "--apply", &plan_id]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("cli.usage"));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn preview_is_pure_and_repeatable_then_apply_creates_the_lock() {
    let dir = project_dir("preview");
    let (output, document) = preview(&dir);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["operation"], "update");
    assert_eq!(document["mode"], "dry-run");
    assert_eq!(document["changed"], true);
    assert!(document["beforeDigest"].is_null());
    assert!(document["planId"]
        .as_str()
        .expect("plan id")
        .starts_with("sha256:"));
    // Pure: no .lekalo directory, no lock, no stage file.
    assert!(!dir.join(".lekalo").exists());
    assert!(!dir.join("lekalo.lock").exists());

    // Repeated identical previews are byte-identical.
    let (_, document_again) = preview(&dir);
    assert_eq!(document, document_again);

    // Apply the printed plan; the lock appears with the after digest.
    let plan_id = document["planId"].as_str().expect("plan id").to_owned();
    let after = document["afterDigest"].as_str().expect("after").to_owned();
    let output = lekalo_in(&dir, &["--json", "update", "--apply", &plan_id]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let receipt = json(&output);
    assert_eq!(receipt["mode"], "apply");
    assert_eq!(receipt["changed"], true);
    assert_eq!(receipt["afterDigest"], after);
    assert!(dir.join("lekalo.lock").exists());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn applying_a_stale_plan_id_after_drift_is_a_source_change() {
    let dir = project_dir("drift");
    let (output, document) = preview(&dir);
    assert_eq!(output.status.code(), Some(0));
    let plan_id = document["planId"].as_str().expect("plan id").to_owned();
    // Out-of-band drift: a valid but different lock appears between preview
    // and apply, so the fresh plan identity no longer matches.
    let golden = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/lockfile/valid/contract-only.lock.json"),
    )
    .expect("golden lock");
    std::fs::write(
        dir.join("lekalo.lock"),
        golden.replace("sha256:2cba65b0", "sha256:3cba65b0"),
    )
    .expect("drift");
    let output = lekalo_in(&dir, &["--json", "update", "--apply", &plan_id]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("lock.source-changed"));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn an_already_current_apply_writes_nothing_and_reports_unchanged() {
    let dir = project_dir("noop");
    let (output, document) = preview(&dir);
    assert_eq!(output.status.code(), Some(0));
    let plan_id = document["planId"].as_str().expect("plan id").to_owned();
    let output = lekalo_in(&dir, &["--json", "update", "--apply", &plan_id]);
    assert_eq!(output.status.code(), Some(0));
    let first = std::fs::read(dir.join("lekalo.lock")).expect("lock bytes");

    // Re-plan and re-apply: changed:false, identical bytes.
    let (output, document) = preview(&dir);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(document["changed"], false);
    let plan_id = document["planId"].as_str().expect("plan id").to_owned();
    let output = lekalo_in(&dir, &["--json", "update", "--apply", &plan_id]);
    assert_eq!(output.status.code(), Some(0));
    let receipt = json(&output);
    assert_eq!(receipt["changed"], false);
    assert_eq!(
        std::fs::read(dir.join("lekalo.lock")).expect("bytes"),
        first,
        "a no-op apply writes nothing"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_malformed_plan_spelling_is_cli_usage() {
    let dir = project_dir("bad-plan");
    let output = lekalo_in(&dir, &["update", "--apply", "not-a-plan"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("cli.usage"));

    let uppercase = format!("sha256:{}", "A".repeat(64));
    let output = lekalo_in(&dir, &["update", "--apply", &uppercase]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("cli.usage"));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
