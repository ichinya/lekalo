//! CLI integration tests for `lekalo observe` (issue #39): the observed
//! connect flow, explicit bindings and confirmation, the staleness gate,
//! stable-key move survival, the incompleteness report of the canonical
//! impact, promotion with its plan/confirm workflow, and the clean
//! protection of observed files. Every test runs the real binary over a
//! sandbox copy of the task-domain fixture; temp roots stay under
//! `target/` addressed with relative selectors — the Windows 8.3-alias
//! lesson — and binary invocations go through `alias_free_path`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const TASK_DOMAIN: &str = "tests/fixtures/observed/task-domain";
const INITIAL_SCAN: &str = "scans/initial.json";

fn alias_free_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("canonicalize path");
    #[cfg(windows)]
    {
        let text = canonical.to_string_lossy().to_string();
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            if stripped.as_bytes().get(1) == Some(&b':') {
                return PathBuf::from(stripped);
            }
        }
    }
    canonical
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf8 stderr")
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

static NEXT_SANDBOX: AtomicU32 = AtomicU32::new(0);

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create sandbox dir");
    for entry in std::fs::read_dir(from).expect("read reference") {
        let entry = entry.expect("reference entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy reference file");
        }
    }
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let unique = NEXT_SANDBOX.fetch_add(1, Ordering::SeqCst);
        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let root = target.join(format!(
            "observed-cli-{tag}-{}-{unique}",
            std::process::id()
        ));
        let project = root.join("proj");
        let _ = &project;
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("sandbox parent");
        let reference = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(TASK_DOMAIN);
        copy_dir(&reference, &root.join("proj"));
        let scans = root.join("scans");
        std::fs::create_dir_all(&scans).expect("sandbox scans");
        copy_dir(&reference.join("scans"), &scans);
        Self { root }
    }

    fn run(&self, args: &[&str]) -> Output {
        lekalo_in(&self.root, args)
    }

    fn update(&self, scan: &str) -> Output {
        self.run(&["observe", "update", "--scan", scan, "--project", "proj"])
    }

    fn json(&self, args: &[&str]) -> Value {
        let mut with_json = vec!["--json"];
        with_json.extend_from_slice(args);
        let output = self.run(&with_json);
        assert_eq!(exit_code(&output), 0, "{args:?}: {}", stderr_text(&output));
        serde_json::from_str(stdout_text(&output).trim()).expect("json envelope")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn connect_records_the_index_without_touching_source_files() {
    let sandbox = Sandbox::new("connect");
    let fingerprint = |name: &str| {
        let bytes = std::fs::read(sandbox.root.join("proj").join(name)).expect("source");
        lekalo_core::versioning::plan::sha256_hex(&bytes)
    };
    let before_tasks = fingerprint("src/tasks.ts");
    let before_http = fingerprint("src/http.ts");

    let receipt = sandbox.json(&[
        "observe",
        "update",
        "--scan",
        "scans/initial.json",
        "--project",
        "proj",
    ]);
    assert_eq!(receipt["status"], "valid");
    assert_eq!(receipt["symbols"], 7);
    assert_eq!(receipt["inferred"], 7);

    assert_eq!(fingerprint("src/tasks.ts"), before_tasks);
    assert_eq!(fingerprint("src/http.ts"), before_http);
    assert!(
        sandbox
            .root
            .join("proj/.lekalo/import/observed/index.json")
            .is_file(),
        "the index persists in the observed-model-draft home"
    );
}

#[test]
fn inferred_and_confirmed_bindings_differ_on_the_wire() {
    let sandbox = Sandbox::new("statuses");
    sandbox.update(INITIAL_SCAN);

    let card = sandbox.json(&[
        "observe",
        "inspect",
        "taskboard.create_task",
        "--project",
        "proj",
    ]);
    assert_eq!(card["binding"], "inferred");
    assert_eq!(card["completeness"], "incomplete");

    let output = sandbox.run(&[
        "observe",
        "confirm",
        "taskboard.create_task",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));

    let card = sandbox.json(&[
        "observe",
        "inspect",
        "taskboard.create_task",
        "--project",
        "proj",
    ]);
    assert_eq!(card["binding"], "confirmed");
    // A confirmed binding with current fingerprint evidence is complete:
    // that is exactly what separates it from the inferred card above.
    assert_eq!(card["completeness"], "complete");

    // A second confirmation refuses with the registered rule.
    let output = sandbox.run(&[
        "observe",
        "confirm",
        "taskboard.create_task",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("observed.confirm-refused"));
}

#[test]
fn explicit_binding_survives_a_file_move_or_the_gate_reports_it() {
    let sandbox = Sandbox::new("move");
    sandbox.update(INITIAL_SCAN);

    // The source files move; the moved scan re-resolves the same stable
    // keys under the new paths.
    std::fs::create_dir_all(sandbox.root.join("proj/src/todo")).expect("move target");
    std::fs::rename(
        sandbox.root.join("proj/src/tasks.ts"),
        sandbox.root.join("proj/src/todo/tasks.ts"),
    )
    .expect("move tasks");
    std::fs::rename(
        sandbox.root.join("proj/src/ids.ts"),
        sandbox.root.join("proj/src/todo/ids.ts"),
    )
    .expect("move ids");

    let receipt = sandbox.json(&[
        "observe",
        "update",
        "--scan",
        "scans/moved.json",
        "--project",
        "proj",
    ]);
    let moved = receipt["moved"].as_array().expect("moved list");
    assert!(moved.iter().any(|symbol| symbol == "taskboard.create_task"));
    assert_eq!(receipt["staled"].as_array().expect("staled").len(), 1);

    let card = sandbox.json(&[
        "observe",
        "inspect",
        "taskboard.create_task",
        "--project",
        "proj",
    ]);
    assert_eq!(card["location"]["path"], "src/todo/tasks.ts");
    assert_eq!(card["state"], "current");

    // The gate: the dropped binding (source gone with no replacement
    // record) fails with the missing-source detail. The JSON envelope
    // carries one registered diagnostic per stale binding.
    let output = sandbox.run(&["--json", "observe", "check", "--project", "proj"]);
    assert_eq!(exit_code(&output), 1);
    let stderr = stderr_text(&output);
    assert!(stderr.contains("observed.stale-binding"), "{stderr}");
    assert!(stderr.contains("source-missing"), "{stderr}");

    // Drifting a moved file surfaces as a fingerprint mismatch.
    std::fs::write(sandbox.root.join("proj/src/todo/tasks.ts"), "// drifted\n")
        .expect("drift source");
    let output = sandbox.run(&["--json", "observe", "check", "--project", "proj"]);
    assert_eq!(exit_code(&output), 1);
    let stderr = stderr_text(&output);
    assert!(stderr.contains("fingerprint-mismatch"), "{stderr}");
}

#[test]
fn impact_reports_the_incompleteness_of_the_observed_graph() {
    let sandbox = Sandbox::new("impact");
    sandbox.update(INITIAL_SCAN);
    // Confirm the entity and its field types only; the operation surface
    // stays observed (inferred), so recorded code references the
    // canonical graph without being part of it.
    for symbol in [
        "taskboard.task_id",
        "taskboard.task_state",
        "taskboard.task",
    ] {
        let output = sandbox.run(&["observe", "confirm", symbol, "--project", "proj"]);
        assert_eq!(exit_code(&output), 0, "{symbol}");
    }

    // Promote the module with the planned plan id.
    let plan = sandbox.json(&[
        "observe",
        "promote",
        "--module",
        "taskboard",
        "--dry-run",
        "--project",
        "proj",
    ]);
    let plan_id = plan["plan"].as_str().expect("plan id").to_owned();
    let output = sandbox.run(&[
        "observe",
        "promote",
        "--module",
        "taskboard",
        "--confirm",
        &plan_id,
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));

    // The canonical model loads, and the canonical impact of the promoted
    // entity sees the recorded (still observed) operation references and
    // degrades its completeness explicitly.
    let impact = sandbox.json(&["impact", "taskboard.task", "--project", "proj"]);
    let reason_codes = impact["reasonCodes"].as_array().expect("reason codes");
    assert!(
        reason_codes
            .iter()
            .any(|code| code == "observed.incomplete-graph"),
        "{reason_codes:?}"
    );
}

#[test]
fn promotion_is_planned_and_confirmed_never_silent() {
    let sandbox = Sandbox::new("promote");
    sandbox.update(INITIAL_SCAN);
    let output = sandbox.run(&[
        "observe",
        "confirm",
        "taskboard.task_id",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 0);

    // An inferred symbol refuses promotion.
    let output = sandbox.run(&[
        "observe",
        "promote",
        "--symbol",
        "taskboard.task_state",
        "--dry-run",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("observed.promotion-refused"));

    let plan = sandbox.json(&[
        "observe",
        "promote",
        "--symbol",
        "taskboard.task_id",
        "--dry-run",
        "--project",
        "proj",
    ]);
    let plan_id = plan["plan"].as_str().expect("plan id").to_owned();

    // A mismatched plan id never writes.
    let wrong = format!("sha256:{}", "a".repeat(64));
    let output = sandbox.run(&[
        "observe",
        "promote",
        "--symbol",
        "taskboard.task_id",
        "--confirm",
        &wrong,
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("observed.promotion-plan-mismatch"));

    let output = sandbox.run(&[
        "observe",
        "promote",
        "--symbol",
        "taskboard.task_id",
        "--confirm",
        &plan_id,
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));

    // The promoted canonical model loads; the index carries the receipt.
    let output = sandbox.run(&["load", "--project", "proj"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let card = sandbox.json(&[
        "observe",
        "inspect",
        "taskboard.task_id",
        "--project",
        "proj",
    ]);
    assert_eq!(card["promoted"], true);
    assert!(card["promotion"].is_object());
}

/// A symbol whose canonical definition cannot be rendered (an entity
/// field references a type that is neither canonical nor in the plan)
/// never promotes: the module plan refuses with its ineligible reason,
/// the confirmed apply of any partial plan id writes nothing, and the
/// index keeps every symbol unpromoted without an adoption receipt.
#[test]
fn promotion_refuses_when_rendering_fails_instead_of_promoting_silently() {
    let sandbox = Sandbox::new("promote-unresolved");
    sandbox.update(INITIAL_SCAN);
    // Confirm the entity and one of its field types; the other field
    // type (task_state) stays inferred, so the entity renders only when
    // the plan contains the type — and a scalar-only plan does not.
    for symbol in ["taskboard.task_id", "taskboard.task"] {
        let output = sandbox.run(&["observe", "confirm", symbol, "--project", "proj"]);
        assert_eq!(exit_code(&output), 0, "{symbol}");
    }

    // Single-symbol plan: the confirmed entity is eligible, but its
    // `state` field references the unconfirmed, unplanned enum, so the
    // plan refuses with the registered diagnostic and the failure
    // reason instead of advertising an empty-entries plan.
    let output = sandbox.run(&[
        "--json",
        "observe",
        "promote",
        "--symbol",
        "taskboard.task",
        "--dry-run",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    let refused: Value =
        serde_json::from_str(stderr_text(&output).trim()).expect("refusal envelope");
    assert_eq!(refused["status"], "invalid");
    assert_eq!(refused["reasonCodes"][0], "observed.promotion-refused");

    // Module plan: the confirmed scalar plans and renders normally;
    // the unrenderable entity is excluded and reported only as
    // ineligible with its reason — never advertised as planned.
    let plan = sandbox.json(&[
        "observe",
        "promote",
        "--module",
        "taskboard",
        "--dry-run",
        "--project",
        "proj",
    ]);
    assert_eq!(plan["symbols"], serde_json::json!(["taskboard.task_id"]));
    assert_eq!(plan["entries"].as_array().map(Vec::len), Some(1));
    let ineligible: Vec<&Value> = plan["ineligible"]
        .as_array()
        .expect("ineligible list")
        .iter()
        .filter(|item| item["symbol"] == "taskboard.task")
        .collect();
    assert_eq!(ineligible.len(), 1);
    assert_eq!(ineligible[0]["detail"], "unresolved-reference");
    // The confirmed path refuses a plan whose id does not match a
    // currently-valid plan, so nothing can be resurrected after the
    // fact and nothing is marked promoted.
    let output = sandbox.run(&[
        "observe",
        "promote",
        "--symbol",
        "taskboard.task",
        "--confirm",
        &format!("sha256:{}", "b".repeat(64)),
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("observed.promotion-refused"));

    // No canonical definition document was written (the fixture ships
    // only the module skeleton) and the index carries no promotion
    // receipt for the refused symbol.
    assert!(!sandbox
        .root
        .join("proj/lekalo/modules/taskboard/entities.yaml")
        .exists());
    let card = sandbox.json(&["observe", "inspect", "taskboard.task", "--project", "proj"]);
    assert_eq!(card["promoted"], false);
    assert!(card["promotion"].is_null());
}

#[test]
fn clean_never_deletes_observed_files() {
    let sandbox = Sandbox::new("clean");
    sandbox.update(INITIAL_SCAN);

    // One orphan inside the managed root, plus the observed evidence.
    // The generate seam requires the committed lock.
    let output = sandbox.run(&["lock", "--project", "proj"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let generated = sandbox.root.join("proj/.lekalo/generated");
    std::fs::create_dir_all(&generated).expect("managed root");
    std::fs::write(generated.join("orphan.ts"), "orphan bytes\n").expect("orphan");

    let output = sandbox.run(&[
        "generate",
        "--clean",
        "--dry-run",
        "--project",
        "proj",
        "--json",
    ]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let plan: Value = serde_json::from_str(stdout_text(&output).trim()).expect("plan");
    let files = plan["files"].as_array().expect("clean plan files");
    assert_eq!(files.len(), 1, "only the generated orphan is planned");
    assert_eq!(files[0]["path"], ".lekalo/generated/orphan.ts");

    // The staleness gate and the observed index remain untouched by clean.
    let output = sandbox.run(&["observe", "check", "--project", "proj"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert!(sandbox
        .root
        .join("proj/.lekalo/import/observed/index.json")
        .is_file());
}
