//! CLI integration tests for the client-SDK evidence pipeline (issue
//! #72): `lekalo generate` derives the canonical client-SDK evidence
//! under `.lekalo/cache/client-sdk/<project>.json` from the validated
//! transport home, the compiled project, the embedded #62 registry,
//! and the bound #64 query-model home. The evidence wire contract is
//! schema-gated and the projection bytes are golden-pinned in the core
//! suite; these tests pin the pipeline behavior at the CLI seam.
//! Every test runs the real binary over a sandbox project; temp roots
//! stay under `target/` addressed with relative selectors — the
//! Windows 8.3-alias lesson.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// (the fixture project home is copied by scratch() through fixture_path())
const VALID_ATTACHMENT: &str = "../../tests/fixtures/transport-http/valid/planner.transport.json";
const QUERY_MODEL: &str = "../../tests/fixtures/transport-http/query-model.json";
const GOLDEN: &str = "../../tests/fixtures/client-sdk/golden/planner.expect.json";

fn alias_free_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("fixture path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

fn scratch() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    copy_fixture(&fixture_path(), temp.path());
    temp
}

fn fixture_path() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/transport-http/project"),
    )
}

fn copy_fixture(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout json")
}

fn exit_code(output: &Output) -> u8 {
    u8::try_from(output.status.code().expect("exit code")).expect("exit code fits u8")
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", lekalo_core::digest::sha256_hex(bytes))
}

/// Rebind the attachment to the exact canonical load envelope of the
/// copied fixture project.
fn repin_model(dir: &Path, value: &mut Value) {
    let load = lekalo_in(dir, &["--json", "load", "--project", "."]);
    assert_eq!(exit_code(&load), 0, "{}", stderr_text(&load));
    value["modelRef"]["digest"] = digest(load.stdout.strip_suffix(b"\n").unwrap()).into();
}

/// Install the canonical transport home (digest-repinned) and the
/// query-model home into the sandbox.
fn install_homes(dir: &Path) {
    let mut value: Value = serde_json::from_slice(
        &fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(VALID_ATTACHMENT)).unwrap(),
    )
    .unwrap();
    repin_model(dir, &mut value);
    fs::create_dir_all(dir.join("lekalo")).unwrap();
    fs::write(
        dir.join("lekalo").join("transport.yaml"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
    let query_model: Value = serde_json::from_slice(
        &fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(QUERY_MODEL)).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("lekalo").join("query-model.yaml"),
        serde_json::to_vec_pretty(&query_model).unwrap(),
    )
    .unwrap();
}

fn golden_value() -> Value {
    serde_json::from_slice(
        &fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN)).unwrap(),
    )
    .unwrap()
}

#[test]
fn the_committed_client_sdk_golden_covers_every_fixture_endpoint() {
    // The golden the pipeline pins (and the adapter consumers import)
    // covers the full planner surface: this is the CLI seam's pin of
    // the same bytes the core suite byte-compares.
    let golden = golden_value();
    assert_eq!(golden["projectId"], "planner");
    assert_eq!(golden["schemaVersion"], "lekalo/client-sdk/v0.4.0");
    assert_eq!(golden["operations"].as_array().map(Vec::len), Some(6));
    assert_eq!(golden["types"].as_array().map(Vec::len), Some(8));
}

#[test]
fn generate_without_a_transport_home_writes_no_client_sdk_evidence() {
    let temp = scratch();
    let dir = temp.path();
    // No transport home: `lekalo validate` passes (the transport pass
    // only runs when the home exists) and no SDK evidence derives.
    let output = lekalo_in(dir, &["--json", "validate", "--project", "."]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert!(
        !dir.join(".lekalo/cache/client-sdk").exists(),
        "no transport home, no SDK evidence"
    );
}

#[test]
fn a_valid_transport_home_validates_with_the_client_sdk_pipeline_context() {
    let temp = scratch();
    let dir = temp.path();
    install_homes(dir);

    // The transport pass runs and accepts the repinned home; the same
    // validated join feeds the SDK derivation.
    let output = lekalo_in(dir, &["--json", "validate", "--project", "."]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document = stdout_json(&output);
    assert_eq!(document["status"], "valid");
}
