//! Issue #73 CLI integration: the real binary over disposable fixture
//! copies. The gates pin the status-owned exits (valid 0, invalid 1,
//! denied 3, unsupported 4, unsupported-version 5), the preview
//! no-write guarantee, the durable apply/replay/receipt flow, and the
//! byte parity between CLI sugar and the JSON request surface.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const FIXTURE: &str = "tests/fixtures/change/field-vertical";

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create target");
    for entry in std::fs::read_dir(from).expect("read source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let unique = NEXT.fetch_add(1, Ordering::SeqCst);
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let root = workspace
            .join("target/change-cli")
            .join(format!("{tag}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join(FIXTURE),
            &root,
        );
        Self { root }
    }

    fn selection(&self) -> String {
        // The CLI resolves the selection against its invocation directory,
        // which these tests pin to the workspace root.
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        self.root
            .strip_prefix(workspace)
            .expect("relative to the workspace root")
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/")
    }

    fn run(&self, args: &[&str]) -> (u8, serde_json::Value, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_lekalo"))
            .args(args)
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .env_remove("LEKALO_PROJECT")
            .output()
            .expect("binary runs");
        let exit = output.status.code().expect("exit code") as u8;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let (stream, human) = if stdout.trim().is_empty() {
            (stderr.clone(), stderr)
        } else {
            (stdout.clone(), stdout)
        };
        let json: serde_json::Value =
            serde_json::from_str(stream.trim()).unwrap_or(serde_json::Value::Null);
        (exit, json, human)
    }

    fn entities_path(&self) -> PathBuf {
        self.root.join("lekalo/modules/tasks/entities.yaml")
    }
}

fn write_request(sandbox: &Sandbox, body: serde_json::Value) -> String {
    write_request_named(sandbox, "request.json", body)
}

fn write_request_named(sandbox: &Sandbox, name: &str, body: serde_json::Value) -> String {
    let path = sandbox.root.join(name);
    std::fs::write(&path, serde_json::to_vec_pretty(&body).expect("serializes"))
        .expect("write request");
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn add_field_body() -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [{
            "operation": "entity-field:add",
            "entity": "tasks.task",
            "field": { "name": "priority", "type": { "ref": "tasks.priority" } }
        }]
    })
}

#[test]
fn dry_run_reports_both_diffs_and_writes_nothing() {
    let sandbox = Sandbox::new("dry-run");
    let before = std::fs::read(sandbox.entities_path()).expect("bytes");
    let request = write_request(&sandbox, add_field_body());
    let (exit, json, _stream) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &sandbox.selection(),
        "--request",
        &request,
        "--dry-run",
        "--json",
    ]);
    assert_eq!(exit, 0);
    assert_eq!(json["status"], "valid");
    // The plan wire rides the domain envelope and carries both diffs and
    // the impact/generation projections.
    assert_eq!(json["schemaVersion"], "lekalo/change-plan/v0.8.0");
    assert!(json["fileDiff"].as_array().expect("fileDiff").len() == 1);
    assert!(json["semanticDiff"]["changes"].as_array().is_some());
    assert_eq!(json["generation"]["executed"], false);
    // Preview never writes: no history, no cache, no source change.
    assert!(!sandbox.root.join("lekalo/change-history.json").exists());
    assert!(!sandbox.root.join(".lekalo").exists());
    assert_eq!(
        std::fs::read(sandbox.entities_path()).expect("bytes"),
        before
    );
}

#[test]
fn sugar_and_json_request_lower_to_the_same_plan() {
    let sandbox = Sandbox::new("parity");
    let (exit, sugar, _) = sandbox.run(&[
        "change",
        "entity-field:add",
        "--project",
        &sandbox.selection(),
        "--entity",
        "tasks.task",
        "--name",
        "priority",
        "--type",
        "tasks.priority",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(exit, 0);
    let request = write_request(&sandbox, add_field_body());
    let (exit, file, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &sandbox.selection(),
        "--request",
        &request,
        "--dry-run",
        "--json",
    ]);
    assert_eq!(exit, 0);
    assert_eq!(
        sugar["planId"], file["planId"],
        "byte-identical plan identity"
    );
    assert_eq!(sugar["semanticDiff"], file["semanticDiff"]);
    assert_eq!(sugar["fileDiff"], file["fileDiff"]);
}

#[test]
fn apply_is_durable_and_replay_returns_the_original_receipt() {
    let sandbox = Sandbox::new("apply");
    let request = write_request(&sandbox, add_field_body());
    let selection = sandbox.selection();
    let (exit, first, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &selection,
        "--request",
        &request,
        "--apply",
        "--idempotency-key",
        "task-priority-v1",
        "--json",
    ]);
    assert_eq!(exit, 0);
    assert_eq!(first["state"], "committed");
    assert_eq!(first["replayed"], false);
    assert!(sandbox.entities_path().exists());
    let after = std::fs::read_to_string(sandbox.entities_path()).expect("utf8");
    assert!(after.contains("- name: \"priority\""));
    assert!(sandbox.root.join("lekalo/change-history.json").exists());
    // Repeating the same key replays without duplicating anything.
    let (exit, second, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &selection,
        "--request",
        &request,
        "--apply",
        "--idempotency-key",
        "task-priority-v1",
        "--json",
    ]);
    assert_eq!(exit, 0);
    assert_eq!(second["state"], "replayed");
    assert_eq!(second["planId"], first["planId"]);
    // The receipt surface finds the committed record by key.
    let (_, receipt, _) = sandbox.run(&[
        "change",
        "receipt",
        "--project",
        &selection,
        "--idempotency-key",
        "task-priority-v1",
        "--json",
    ]);
    assert_eq!(receipt["state"], "committed");
    assert_eq!(receipt["planId"], first["planId"]);
    // The project still loads and carries exactly one new field.
    let (_, model, _) = sandbox.run(&["load", "--project", &selection, "--json"]);
    assert_eq!(model["status"], "valid");
    let rendered = model["model"]["definitions"]
        .as_array()
        .expect("defs")
        .iter()
        .find(|definition| definition["id"] == "tasks.task")
        .expect("entity");
    assert_eq!(rendered["fields"].as_array().expect("fields").len(), 3);
}

#[test]
fn breaking_operation_denies_without_the_explicit_flag() {
    let sandbox = Sandbox::new("breaking");
    let removal = write_request(
        &sandbox,
        serde_json::json!({
            "schemaVersion": "lekalo/change-request/v0.8.0",
            "operations": [{
                "operation": "entity-field:remove",
                "entity": "tasks.task",
                "name": "done"
            }]
        }),
    );
    let selection = sandbox.selection();
    let (exit, denied, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &selection,
        "--request",
        &removal,
        "--apply",
        "--idempotency-key",
        "remove-done-v1",
        "--json",
    ]);
    assert_eq!(exit, 3, "denied exits 3");
    assert_eq!(denied["status"], "denied");
    let ids: Vec<&str> = denied["reasonCodes"]
        .as_array()
        .expect("reasons")
        .iter()
        .map(|value| value.as_str().expect("str"))
        .collect();
    assert!(ids.contains(&"change.compatibility-denied"), "{ids:?}");
    // The invalid candidate can never sneak through with --allow-breaking.
    // A separate request file: reusing the removal path would overwrite the
    // request the third run below re-applies.
    let malformed = write_request_named(
        &sandbox,
        "request-malformed.json",
        serde_json::json!({
            "schemaVersion": "lekalo/change-request/v0.8.0",
            "operations": [{
                "operation": "entity-field:remove",
                "entity": "tasks.nonexistent",
                "name": "done"
            }]
        }),
    );
    let (exit, _, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &selection,
        "--request",
        &malformed,
        "--apply",
        "--idempotency-key",
        "remove-done-v2",
        "--allow-breaking",
        "--json",
    ]);
    assert_eq!(exit, 1, "invalid subjects stay invalid under the flag");
    // A valid breaking operation commits with the flag and one key.
    let (exit, receipt, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &selection,
        "--request",
        &removal,
        "--apply",
        "--idempotency-key",
        "remove-done-v1",
        "--allow-breaking",
        "--json",
    ]);
    assert_eq!(exit, 0);
    assert_eq!(receipt["state"], "committed");
}

#[test]
fn invalid_and_unsupported_requests_own_their_exits() {
    let sandbox = Sandbox::new("exits");
    let selection = sandbox.selection();
    // Unknown operation discriminator: invalid (exit 1).
    let unknown = write_request(
        &sandbox,
        serde_json::json!({
            "schemaVersion": "lekalo/change-request/v0.8.0",
            "operations": [{ "operation": "entity-field:append", "entity": "tasks.task" }]
        }),
    );
    let (exit, invalid, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &selection,
        "--request",
        &unknown,
        "--dry-run",
        "--json",
    ]);
    assert_eq!(exit, 1);
    assert!(invalid["reasonCodes"]
        .as_array()
        .expect("reasons")
        .iter()
        .any(|value| value == "change.request-invalid"));
    // Cataloged but unimplemented operation: unsupported (exit 4).
    let unsupported = write_request(
        &sandbox,
        serde_json::json!({
            "schemaVersion": "lekalo/change-request/v0.8.0",
            "operations": [{
                "operation": "command-effect:add",
                "command": "tasks.complete",
                "emit": "tasks.event.completed"
            }]
        }),
    );
    let (exit, unsupported_json, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &selection,
        "--request",
        &unsupported,
        "--dry-run",
        "--json",
    ]);
    assert_eq!(exit, 4);
    assert_eq!(unsupported_json["status"], "unsupported");
    assert!(unsupported_json["reasonCodes"]
        .as_array()
        .expect("reasons")
        .iter()
        .any(|value| value == "change.operation-unsupported"));
    // Wrong request discriminator: unsupported-version (exit 5).
    let wrong_version = write_request(
        &sandbox,
        serde_json::json!({
            "schemaVersion": "lekalo/change-request/v0.6.4",
            "operations": [{ "operation": "entity-field:add", "entity": "tasks.task",
                "field": { "name": "x", "type": { "ref": "tasks.text" } } }]
        }),
    );
    let (exit, version, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &selection,
        "--request",
        &wrong_version,
        "--dry-run",
        "--json",
    ]);
    assert_eq!(exit, 5);
    assert_eq!(version["status"], "unsupported-version");
}

#[test]
fn declared_default_refuses_with_the_registered_unsupported_rule() {
    let sandbox = Sandbox::new("default");
    let request = write_request(
        &sandbox,
        serde_json::json!({
            "schemaVersion": "lekalo/change-request/v0.8.0",
            "operations": [{
                "operation": "entity-field:add",
                "entity": "tasks.task",
                "field": { "name": "priority", "type": { "ref": "tasks.priority" }, "default": "normal" }
            }]
        }),
    );
    let (exit, refused, _) = sandbox.run(&[
        "change",
        "request",
        "--project",
        &sandbox.selection(),
        "--request",
        &request,
        "--dry-run",
        "--json",
    ]);
    assert_eq!(exit, 4);
    assert!(refused["reasonCodes"]
        .as_array()
        .expect("reasons")
        .iter()
        .any(|value| value == "change.operation-unsupported"));
    assert!(!sandbox.root.join("lekalo/change-history.json").exists());
}

#[test]
fn recovery_blocks_the_project_until_recover_runs() {
    let sandbox = Sandbox::new("recovery");
    let journal = sandbox.root.join(".lekalo/cache/change/change-ab12cd34-1");
    std::fs::create_dir_all(&journal).expect("journal dir");
    std::fs::write(
        journal.join("journal.json"),
        br#"{"transaction":"change-ab12cd34-1","planId":"sha256:0000000000000000000000000000000000000000000000000000000000000000","files":[],"history":false}"#,
    )
    .expect("journal");
    let selection = sandbox.selection();
    // Any reader fails closed while the journal exists. The registered
    // rule is admitted for the denied class (LEK-CHANGE-010, denied 3).
    let (exit, blocked, _) = sandbox.run(&["load", "--project", &selection, "--json"]);
    assert_eq!(exit, 3, "the barrier denies the reader");
    assert!(blocked["reasonCodes"]
        .as_array()
        .expect("reasons")
        .iter()
        .any(|value| value == "change.recovery-required"));
    // Recovery clears the barrier. The rollback receipt is the flat
    // change-receipt document with state rolled-back (review finding 6).
    let (exit, recovered, _) = sandbox.run(&[
        "change",
        "recover",
        "--project",
        &selection,
        "--transaction",
        "change-ab12cd34-1",
        "--json",
    ]);
    assert_eq!(exit, 0);
    assert_eq!(recovered["status"], "valid");
    assert_eq!(recovered["state"], "rolled-back");
    assert_eq!(recovered["transaction"]["state"], "rolled-back");
    assert_eq!(recovered["schemaVersion"], "lekalo/change-receipt/v0.8.0");
    assert!(recovered.get("change").is_none(), "no wrapper envelope");
    let (exit, load, _) = sandbox.run(&["load", "--project", &selection, "--json"]);
    assert_eq!(exit, 0);
    assert_eq!(load["status"], "valid");
}

#[test]
fn dry_run_conflicts_with_apply_and_apply_requires_the_key() {
    let sandbox = Sandbox::new("flags");
    let request = write_request(&sandbox, add_field_body());
    let selection = sandbox.selection();
    let output = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "change",
            "request",
            "--project",
            &selection,
            "--request",
            &request,
            "--dry-run",
            "--apply",
            "--json",
        ])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .expect("binary runs");
    assert_ne!(
        output.status.code(),
        Some(0),
        "--dry-run conflicts with --apply"
    );
    let output = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "change",
            "request",
            "--project",
            &selection,
            "--request",
            &request,
            "--apply",
            "--json",
        ])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .expect("binary runs");
    assert_ne!(
        output.status.code(),
        Some(0),
        "--apply requires --idempotency-key"
    );
    assert!(!sandbox.root.join("lekalo/change-history.json").exists());
}
