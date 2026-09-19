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
fn project_emits_the_byte_pinned_route_surface() {
    let temp = scratch_with_attachment();
    let dir = temp.path();
    for namespace in ["go", "laravel", "node", "rust"] {
        let output = lekalo_in(
            dir,
            &[
                "--json",
                "transport",
                "project",
                "transport.attachment.json",
                "--namespace",
                namespace,
                "--project",
                ".",
                "--query-model",
                "query-model.json",
            ],
        );
        assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(envelope["status"], "valid");
        let golden_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(format!(
                "tests/fixtures/transport-http/projected/{namespace}/{namespace}.expect.json",
            ));
        let golden: Value =
            serde_json::from_slice(&fs::read(alias_free_path(&golden_path)).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_string(&envelope["surface"]).unwrap(),
            serde_json::to_string(&golden).unwrap(),
            "{namespace}: the CLI surface is the committed golden",
        );
    }
}

#[test]
fn the_canonical_home_joins_lekalo_validate() {
    let temp = scratch_with_attachment();
    let dir = temp.path();
    // The canonical home is the digest-repinned attachment at
    // lekalo/transport.yaml; lekalo validate includes its semantic
    // pass (family-internal plus Model-bound checks).
    let mut value: Value =
        serde_json::from_slice(&fs::read(dir.join("transport.attachment.json")).unwrap()).unwrap();
    repin_model(dir, &mut value);
    fs::write(
        dir.join("lekalo").join("transport.yaml"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
    let output = lekalo_in(dir, &["--json", "validate", "--project", "."]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));

    // An invalid home refuses the whole validate run.
    value["endpoints"][0]["endpoint"] = json!("planner.not_an_endpoint");
    fs::write(
        dir.join("lekalo").join("transport.yaml"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
    let refused = lekalo_in(dir, &["--json", "validate", "--project", "."]);
    assert_eq!(exit_code(&refused), 1);
    assert!(
        stderr_text(&refused).contains("transport.endpoint-unresolved"),
        "{}",
        stderr_text(&refused)
    );
}

#[test]
fn generate_writes_the_transport_evidence() {
    // The orchestration fixture project carries the lock and the
    // endpoint symbol planner.api_focus; its transport home joins the
    // generate pipeline exactly like an operator project.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join("tests/fixtures/orchestration/project");
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    copy_fixture(&root, &project);
    let load = lekalo_in(&project, &["--json", "load", "--project", "."]);
    assert_eq!(exit_code(&load), 0);
    let model_digest = digest(
        load.stdout
            .strip_suffix(
                b"
",
            )
            .unwrap(),
    );
    let home = json!({
        "schemaVersion": "lekalo/transport-http/v0.4.0",
        "identity": "dev.lekalo.transport-http@0.4.0",
        "attachmentRevision": "0.4.0",
        "projectId": "planner",
        "modelRef": {
            "modelVersion": "0.2.16",
            "digest": model_digest,
        },
        "irRef": {
            "identity": "dev.lekalo.ir@0.2.16",
            "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        },
        "wire": {"dialect": "lekalo-http-wire/v1", "contentType": "application/json"},
        "defaults": {
            "errorEnvelope": "canonical-v1",
            "idempotencyHeader": "Idempotency-Key",
            "correlationHeaders": ["X-Request-Id"],
        },
        "securitySchemes": [],
        "endpoints": [
            {
                "endpoint": "planner.api_focus",
                "params": [
                    {"name": "task_id", "in": "path", "field": "input.task_id", "required": true, "style": "simple"},
                ],
                "success": {"status": 202},
                "errorDefaults": {
                    "validation": 400, "auth": 403, "conflict": 409,
                    "not-found": 404, "domain": 422, "infrastructure": 500,
                },
                "auth": {"actor": "identity.user", "schemes": ["user_bearer"]},
            },
        ],
    });
    let schemes = json!([{"id": "user_bearer", "kind": "bearer", "format": "jwt"}]);
    let mut home = home;
    home["securitySchemes"] = schemes;
    fs::write(
        project.join("lekalo").join("transport.yaml"),
        serde_json::to_vec_pretty(&home).unwrap(),
    )
    .unwrap();
    // The pipeline requires the lock; bind the reference adapter.
    let lock = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "--json",
            "lock",
            "--",
            "node",
            "adapters/node-typescript/node-adapter.mjs",
        ])
        .current_dir(alias_free_path(&project))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the lock");
    assert_eq!(
        exit_code(&lock),
        0,
        "{}",
        String::from_utf8_lossy(&lock.stderr)
    );
    // A dry-run generate through the reference adapter reaches the
    // preflight stage; the evidence file must exist afterwards.
    let output = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "--json",
            "generate",
            "--target",
            "node-typescript",
            "--dry-run",
            "--",
            "node",
            "adapters/node-typescript/node-adapter.mjs",
        ])
        .current_dir(alias_free_path(&project))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary");
    assert_eq!(
        exit_code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let evidence = project
        .join(".lekalo")
        .join("cache")
        .join("transport")
        .join("planner.json");
    let bytes = fs::read(&evidence).unwrap_or_else(|_| {
        panic!(
            "the transport evidence is written: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let evidence: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(evidence["schemaVersion"], "lekalo/transport-http/v0.4.0",);
    assert_eq!(evidence["endpoints"][0]["endpoint"], "planner.api_focus");
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
