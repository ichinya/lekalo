//! Issue #70 CLI tests for the `lekalo transport` handoff: the
//! validate envelope over the hermetic planner fixture with the
//! embedded error registry and the bound query model, the strict
//! gates, the invalid-matrix refusals, the joined inspect surface,
//! and the read-only failure classes.
//!
//! Every child chdir runs through the alias-free temp spelling (see
//! the query-model suite for the Windows 8.3 rationale).

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn copy_fixture(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == ".lekalo" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn scratch() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    copy_fixture(&fixture_path(), temp.path());
    temp
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

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

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

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn exit_code(output: &Output) -> u8 {
    output.status.code().expect("exit code") as u8
}

fn fixture_path() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/transport-http/project"),
    )
}

fn fixture(relative: &str) -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/transport-http")
            .join(relative),
    )
}

/// Copy the fixture project plus the valid attachment (digest-repinned
/// to the copied project) plus the query model into a scratch dir.
fn scratch_with_attachment() -> tempfile::TempDir {
    let temp = scratch();
    let mut value: Value =
        serde_json::from_slice(&fs::read(fixture("valid/planner.transport.json")).unwrap())
            .unwrap();
    repin_model(temp.path(), &mut value);
    fs::write(
        temp.path().join("transport.attachment.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    fs::copy(
        fixture("query-model.json"),
        temp.path().join("query-model.json"),
    )
    .unwrap();
    temp
}

fn validate_args(strict: bool) -> Vec<&'static str> {
    let mut args = vec![
        "--json",
        "transport",
        "validate",
        "transport.attachment.json",
        "--project",
        ".",
        "--query-model",
        "query-model.json",
    ];
    if strict {
        args.push("--strict");
    }
    args
}

fn validate(dir: &Path, strict: bool) -> Value {
    let output = lekalo_in(dir, &validate_args(strict));
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert!(stderr_text(&output).is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn validate_err(dir: &Path, strict: bool) -> String {
    let output = lekalo_in(dir, &validate_args(strict));
    assert_eq!(
        exit_code(&output),
        1,
        "expected refusal: {}",
        stderr_text(&output)
    );
    assert!(output.stdout.is_empty(), "no partial success on stdout");
    stderr_text(&output)
}

#[test]
fn validate_emits_the_transport_envelope_and_is_deterministic() {
    let temp = scratch_with_attachment();
    let dir = temp.path();
    let first = validate(dir, false);
    assert_eq!(first["status"], "valid");
    assert_eq!(first["transport"]["projectId"], "planner");
    assert_eq!(first["transport"]["endpoints"], 6);
    assert_eq!(first["transport"]["schemes"], 2);
    assert!(first["transport"]["canonicalDigest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    let second = validate(dir, false);
    assert_eq!(first, second, "the envelope is deterministic");

    // The strict profile holds the same fixture: complete mappings
    // and the capability map are all satisfied.
    let strict = validate(dir, true);
    assert_eq!(strict["status"], "valid");
}

#[test]
fn an_unpinned_model_digest_refuses_custody() {
    let temp = scratch_with_attachment();
    let dir = temp.path();
    let mut value: Value =
        serde_json::from_slice(&fs::read(dir.join("transport.attachment.json")).unwrap()).unwrap();
    value["modelRef"]["digest"] =
        json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    fs::write(
        dir.join("transport.attachment.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    let stderr = validate_err(dir, false);
    assert!(stderr.contains("transport.contract-invalid"), "{stderr}");
    assert!(stderr.contains("model-digest"), "{stderr}");
}

#[test]
fn invalid_vectors_refuse_with_the_registered_rule() {
    let temp = scratch_with_attachment();
    let dir = temp.path();
    for name in [
        "path-param-unbound",
        "field-unresolved",
        "entry-outside-union",
        "scheme-unresolved",
        "public-with-scheme",
        "capability-unsatisfied",
        "endpoint-kind",
        "cursor-param-mismatch",
    ] {
        let mut value: Value =
            serde_json::from_slice(&fs::read(fixture(&format!("invalid/{name}.json"))).unwrap())
                .unwrap();
        repin_model(dir, &mut value);
        fs::write(
            dir.join("transport.attachment.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let stderr = validate_err(dir, false);
        let expectation: Value = serde_json::from_slice(
            &fs::read(fixture(&format!("invalid/{name}.expect.json"))).unwrap(),
        )
        .unwrap();
        assert!(
            stderr.contains(expectation["rule"].as_str().unwrap()),
            "{name}: {stderr}"
        );
    }
}

#[test]
fn strict_flags_the_unmapped_union_member() {
    let temp = scratch_with_attachment();
    let dir = temp.path();
    let mut value: Value =
        serde_json::from_slice(&fs::read(fixture("invalid/union-member-unmapped.json")).unwrap())
            .unwrap();
    repin_model(dir, &mut value);
    fs::write(
        dir.join("transport.attachment.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    let stderr = validate_err(dir, true);
    assert!(stderr.contains("transport.mapping-missing"), "{stderr}");
}

#[test]
fn inspect_joins_the_model_surface_and_params() {
    let temp = scratch_with_attachment();
    let dir = temp.path();
    let output = lekalo_in(
        dir,
        &[
            "--json",
            "transport",
            "inspect",
            "transport.attachment.json",
            "--endpoint",
            "planner.endpoint_focus_task_by_id",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["endpoint"]["method"], "POST");
    assert_eq!(envelope["endpoint"]["path"], "/tasks/{task_id}/focus");
    assert_eq!(envelope["endpoint"]["invokes"], "planner.focus_task");
    assert_eq!(
        envelope["endpoint"]["operationId"],
        "plannerEndpointFocusTaskById"
    );
    assert_eq!(envelope["endpoint"]["params"]["task_id"]["in"], "path");

    // An unknown endpoint symbol refuses with the family rule.
    let missing = lekalo_in(
        dir,
        &[
            "--json",
            "transport",
            "inspect",
            "transport.attachment.json",
            "--endpoint",
            "planner.not_an_endpoint",
        ],
    );
    assert_eq!(exit_code(&missing), 1);
    let stderr = stderr_text(&missing);
    assert!(stderr.contains("transport.endpoint-unresolved"), "{stderr}");
}

#[test]
fn missing_documents_refuse_read_only() {
    let temp = scratch_with_attachment();
    let dir = temp.path();
    let output = lekalo_in(dir, &["--json", "transport", "validate", "missing.json"]);
    assert_eq!(exit_code(&output), 1);
    assert!(output.stdout.is_empty());
    assert!(stderr_text(&output).contains("transport.input-invalid"));
}
