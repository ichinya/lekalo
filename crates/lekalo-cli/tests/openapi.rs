//! Issue #46 CLI tests for the `lekalo openapi` handoff: the render
//! envelope over the hermetic planner fixture (digest, document,
//! findings-as-warnings), the declared 3.0 variant, the checked-mode
//! conformance and drift verdicts, the joined inspect surface, the
//! `lekalo validate` preflight, and the read-only failure classes.
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

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout json")
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

/// The scratch attachment bound to the copied project: the fixture
/// document with its Model digest repinned, written next to the
/// project (the suite runs the CLI inside the temp root).
fn prepared_attachment(dir: &Path) -> PathBuf {
    let mut value: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join("tests/fixtures/transport-http/valid/planner.transport.json"),
        )
        .unwrap(),
    )
    .unwrap();
    repin_model(dir, &mut value);
    let path = dir.join("planner.transport.json");
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    path
}

#[test]
fn render_produces_the_canonical_envelope() {
    let temp = scratch();
    let dir = temp.path();
    // The suite runs from a parent directory whose `project` subtree
    // is the fixture; the CLI resolves `--project` relative to cwd.
    let attachment = prepared_attachment(dir);
    let output = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "render",
            attachment.to_str().unwrap(),
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["openapi"]["projectId"], "planner");
    assert_eq!(envelope["openapi"]["openapiVersion"], "3.1.0");
    assert_eq!(envelope["openapi"]["mode"], "full");
    assert!(envelope["openapi"]["canonicalDigest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(
        envelope["openapi"]["document"]["paths"]["/tasks"]["get"]["operationId"],
        "listTasks"
    );
    // The command success without a declared output rides as a
    // projection-partial warning.
    let findings = envelope["diagnostics"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        findings
            .iter()
            .any(|finding| finding["id"] == "openapi.projection-partial"),
        "{findings:?}"
    );
}

#[test]
fn the_declared_30_variant_renders_nullable_siblings() {
    let temp = scratch();
    let dir = temp.path();
    let attachment = prepared_attachment(dir);
    let output = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "render",
            attachment.to_str().unwrap(),
            "--project",
            ".",
            "--version",
            "3.0",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["openapi"]["openapiVersion"], "3.0.0");
}

#[test]
fn render_refuses_the_unbound_attachment() {
    let temp = scratch();
    let dir = temp.path();
    let attachment = prepared_attachment(dir);
    // Corrupt the Model digest: custody refuses.
    let mut value: Value = serde_json::from_slice(&fs::read(&attachment).unwrap()).unwrap();
    value["modelRef"]["digest"] =
        json!("sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
    fs::write(&attachment, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let refused = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "render",
            attachment.to_str().unwrap(),
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&refused), 1);
    assert!(
        stderr_text(&refused).contains("transport.contract-invalid"),
        "{}",
        stderr_text(&refused)
    );
}

#[test]
fn check_accepts_the_generator_output_and_reports_drift() {
    let temp = scratch();
    let dir = temp.path();
    let attachment = prepared_attachment(dir);
    let render = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "render",
            attachment.to_str().unwrap(),
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&render), 0, "{}", stderr_text(&render));
    let document = stdout_json(&render)["openapi"]["document"].clone();
    let document_path = dir.join("openapi.json");
    fs::write(
        &document_path,
        serde_json::to_vec_pretty(&document).unwrap(),
    )
    .unwrap();

    // The generator output is conformant: every pointer binds cleanly.
    let conformant = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "check",
            document_path.to_str().unwrap(),
            "--transport",
            attachment.to_str().unwrap(),
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&conformant), 0, "{}", stderr_text(&conformant));
    let envelope = stdout_json(&conformant);
    assert_eq!(envelope["openapiCheck"]["conformant"], true);
    assert!(envelope["openapiCheck"]["boundClean"].as_u64().unwrap() >= 1);

    // A stale human edit under the bound operation drifts.
    let mut drifted = document.clone();
    drifted["paths"]["/tasks"]["get"]["summary"] = json!("stale human text");
    fs::write(&document_path, serde_json::to_vec_pretty(&drifted).unwrap()).unwrap();
    let refusal = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "check",
            document_path.to_str().unwrap(),
            "--transport",
            attachment.to_str().unwrap(),
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&refusal), 1);
    assert!(
        stderr_text(&refusal).contains("openapi.drift"),
        "{}",
        stderr_text(&refusal)
    );
}

#[test]
fn check_honors_the_declared_version_and_mode_flags() {
    // A maintained 3.0 document checks cleanly at --version 3.0 and
    // refuses under the 3.1 default; --mode is honored on the
    // recomputation (r1 cline F-6).
    let temp = scratch();
    let dir = temp.path();
    let attachment = prepared_attachment(dir);
    let render = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "render",
            "--version",
            "3.0",
            attachment.to_str().unwrap(),
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&render), 0, "{}", stderr_text(&render));
    let document = stdout_json(&render)["openapi"]["document"].clone();
    let document_path = dir.join("openapi-30.json");
    fs::write(
        &document_path,
        serde_json::to_vec_pretty(&document).unwrap(),
    )
    .unwrap();

    let declared30 = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "check",
            "--version",
            "3.0",
            "--mode",
            "full",
            document_path.to_str().unwrap(),
            "--transport",
            attachment.to_str().unwrap(),
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&declared30), 0, "{}", stderr_text(&declared30));
    assert_eq!(stdout_json(&declared30)["openapiCheck"]["conformant"], true);

    let default31 = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "check",
            document_path.to_str().unwrap(),
            "--transport",
            attachment.to_str().unwrap(),
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&default31), 1);
    assert!(
        stderr_text(&default31).contains("openapi.input-invalid"),
        "the version guard refuses: {}",
        stderr_text(&default31)
    );
}

#[test]
fn check_refuses_a_document_anchored_to_nothing() {
    let temp = scratch();
    let dir = temp.path();
    let attachment = prepared_attachment(dir);
    let document = json!({
        "openapi": "3.1.0",
        "info": {"title": "planner", "version": "0.4.0"},
        "paths": {
            "/legacy": {"get": {
                "operationId": "legacyRoute",
                "x-lekalo-endpoint": "planner.endpoint_removed"
            }}
        }
    });
    let document_path = dir.join("openapi.json");
    fs::write(
        &document_path,
        serde_json::to_vec_pretty(&document).unwrap(),
    )
    .unwrap();
    let refused = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "check",
            document_path.to_str().unwrap(),
            "--transport",
            attachment.to_str().unwrap(),
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&refused), 1);
    assert!(
        stderr_text(&refused).contains("openapi.binding-unresolved"),
        "{}",
        stderr_text(&refused)
    );
}

#[test]
fn inspect_returns_the_joined_operation() {
    let temp = scratch();
    let dir = temp.path();
    let attachment = prepared_attachment(dir);
    let output = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "inspect",
            attachment.to_str().unwrap(),
            "--endpoint",
            "planner.endpoint_list_tasks",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(
        envelope["endpoint"]["endpoint"],
        "planner.endpoint_list_tasks"
    );
    assert_eq!(
        envelope["endpoint"]["operation"]["operationId"],
        "listTasks"
    );
    assert_eq!(envelope["endpoint"]["pointer"], "/paths/~1tasks/get");

    let refused = lekalo_in(
        dir,
        &[
            "--json",
            "openapi",
            "inspect",
            attachment.to_str().unwrap(),
            "--endpoint",
            "planner.endpoint_missing",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&refused), 1);
    assert!(stderr_text(&refused).contains("openapi.binding-unresolved"));
}

#[test]
fn diff_reports_the_pointer_view_of_the_wire_classes() {
    let dir = fixture_dir();
    let base = dir.join("valid/planner.transport.json");
    let candidate = dir.join("diff/candidate-add-required-param.json");
    // The diff fixtures carry the placeholder Model pin; the command
    // resolves the project fixture beside them.
    let output = lekalo_in(
        dir.as_path(),
        &[
            "--json",
            "openapi",
            "diff",
            base.to_str().unwrap(),
            candidate.to_str().unwrap(),
            "--project",
            "project",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["openapiDiff"]["wireConsumerBlocked"], true);
    let paths = envelope["openapiDiff"]["paths"]
        .as_array()
        .cloned()
        .unwrap();
    assert!(!paths.is_empty());
    // Every changed path names the document locations it touches; the
    // parameter change points at the operation's parameter list.
    assert!(paths.iter().any(|path| path["pointers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|pointer| pointer.as_str().unwrap().ends_with("/parameters"))));
    // The non-breaking addition does not block the strict profile.
    let added = dir.join("diff/candidate-add-endpoint.json");
    let addition = lekalo_in(
        dir.as_path(),
        &[
            "--json",
            "openapi",
            "diff",
            base.to_str().unwrap(),
            added.to_str().unwrap(),
            "--project",
            "project",
        ],
    );
    assert_eq!(exit_code(&addition), 0, "{}", stderr_text(&addition));
    assert_eq!(
        stdout_json(&addition)["openapiDiff"]["wireConsumerBlocked"],
        false
    );
}

fn fixture_dir() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/transport-http"),
    )
}

#[test]
fn validate_preflights_the_openapi_projection_of_the_canonical_home() {
    let temp = scratch();
    let dir = temp.path();
    // The canonical home is the digest-repinned attachment at
    // lekalo/transport.yaml; `lekalo validate` renders it at the
    // declared defaults as a preflight.
    let mut value: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join("tests/fixtures/transport-http/valid/planner.transport.json"),
        )
        .unwrap(),
    )
    .unwrap();
    repin_model(dir, &mut value);
    fs::write(
        dir.join("lekalo").join("transport.yaml"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
    let output = lekalo_in(dir, &["--json", "validate", "--project", "."]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
}
