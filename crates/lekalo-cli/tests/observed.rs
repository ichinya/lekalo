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
        lekalo_core::digest::sha256_hex(&bytes)
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

fn evidence_package(project: &Path, id: &str, schema: Value) -> Value {
    evidence_package_with_binding(project, id, schema, None)
}

fn evidence_package_with_binding(
    project: &Path,
    id: &str,
    schema: Value,
    binding: Option<Value>,
) -> Value {
    let directory = project.join(id);
    std::fs::create_dir_all(&directory).unwrap();
    let mut files = vec!["payload.schema.json"];
    if binding.is_some() {
        files.push("bindings.json");
    }
    let manifest = serde_json::to_vec(&serde_json::json!({
        "schema":"lekalo/package/v1", "id":id, "version":"1.0.0",
        "kind":"evidence", "files":files
    }))
    .unwrap();
    let schema = serde_json::to_vec(&schema).unwrap();
    std::fs::write(directory.join("lekalo.package.json"), &manifest).unwrap();
    std::fs::write(directory.join("payload.schema.json"), &schema).unwrap();
    let schema_digest = lekalo_core::runtime::digest(&schema);
    let mut inventory = serde_json::json!({
        "lekalo.package.json":lekalo_core::runtime::digest(&manifest),
        "payload.schema.json":schema_digest,
    });
    if let Some(binding) = binding {
        let bytes = serde_json::to_vec(&binding).unwrap();
        std::fs::write(directory.join("bindings.json"), &bytes).unwrap();
        inventory["bindings.json"] = serde_json::json!(lekalo_core::runtime::digest(&bytes));
    }
    serde_json::json!({
        "source":format!("path:{id}"), "schema_digest":schema_digest,
        "package_digest":lekalo_core::runtime::digest(&serde_json::to_vec(&inventory).unwrap()),
    })
}

#[test]
fn validate_checks_every_evidence_package_without_an_adapter() {
    use serde_json::json;
    let sandbox = Sandbox::new("validate-evidence");
    let project = sandbox.root.join("proj");
    let config_path = project.join("lekalo.evidence.json");
    let validate = || sandbox.run(&["--json", "validate", "--strict", "--project", "proj"]);
    let baseline = validate();
    assert_eq!(exit_code(&baseline), 0, "{}", stderr_text(&baseline));

    let pin = evidence_package(&project, "example-provider", json!({"type":"string"}));
    let digest = lekalo_core::runtime::digest(b"no adapter needed for validate");
    let selection = |profile, pin: &Value| {
        json!({
            "target":"example", "profile":profile,
            "producer":{"id":"not-installed","version":"1.0.0","digest":digest},
            "entry_digest":digest, "program_digest":digest, "families":{"example":pin},
        })
    };
    let mut config = json!({
        "schema":"lekalo/evidence-config/v1", "scans":[selection("primary", &pin), selection("secondary", &pin)],
    });
    let write_config = |config: &Value| {
        std::fs::write(&config_path, serde_json::to_vec(config).unwrap()).unwrap();
    };
    write_config(&config);
    let valid = validate();
    assert_eq!(exit_code(&valid), 0, "{}", stderr_text(&valid));
    assert_eq!(stdout_text(&valid), stdout_text(&baseline));

    config["scans"][1]["families"]["example"]["schema_digest"] = json!(digest);
    write_config(&config);
    let stale = validate();
    assert_eq!(exit_code(&stale), 1, "{}", stderr_text(&stale));
    assert!(stderr_text(&stale).contains("evidence-package-invalid"));

    let invalid = evidence_package(
        &project,
        "invalid-provider",
        json!({"$ref":"https://example.invalid/schema"}),
    );
    config["scans"][1] = selection("secondary", &invalid);
    write_config(&config);
    let malformed = validate();
    assert_eq!(exit_code(&malformed), 1, "{}", stderr_text(&malformed));
    assert!(stderr_text(&malformed).contains("evidence-schema-invalid"));
}

#[test]
fn validate_checks_pinned_evidence_binding_declarations() {
    use serde_json::json;

    let sandbox = Sandbox::new("validate-evidence-bindings");
    let project = sandbox.root.join("proj");
    let schema = json!({
        "type":"object", "required":["id"], "properties":{"id":{"type":"string"}}
    });
    let rule = json!({
        "schema":"lekalo/evidence-binding-rules/v1.3.1",
        "kind":"command", "identity":["id"], "match":{"id":"id"},
        "source_roles":["subject","dependency"]
    });
    let digest = lekalo_core::runtime::digest(b"fixture");
    let validate = |rule: &Value| {
        let pin = evidence_package_with_binding(
            &project,
            "binding-provider",
            schema.clone(),
            Some(rule.clone()),
        );
        let config = json!({
            "schema":"lekalo/evidence-config/v1",
            "scans":[{
                "target":"example",
                "producer":{"id":"example","version":"1.0.0","digest":digest},
                "entry_digest":digest, "program_digest":digest,
                "families":{"example":pin}
            }]
        });
        std::fs::write(
            project.join("lekalo.evidence.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        sandbox.run(&["--json", "validate", "--strict", "--project", "proj"])
    };
    let valid = validate(&rule);
    assert_eq!(exit_code(&valid), 0, "{}", stderr_text(&valid));

    std::fs::write(project.join("binding-provider/bindings.json"), b"{}").unwrap();
    let unpinned = sandbox.run(&["--json", "validate", "--project", "proj"]);
    assert_eq!(exit_code(&unpinned), 1);
    assert!(stderr_text(&unpinned).contains("evidence-package-invalid"));

    let mut invalid = rule.clone();
    invalid["identity"] = json!(["unknown"]);
    let result = validate(&invalid);
    assert_eq!(exit_code(&result), 1);
    assert!(stderr_text(&result).contains("evidence-binding-payload-field"));

    invalid = rule;
    invalid["kind"] = json!("unsupported-kind");
    let result = validate(&invalid);
    assert_eq!(exit_code(&result), 1);
    assert!(stderr_text(&result).contains("evidence-binding-rule-invalid"));
}

#[test]
fn generic_scan_uses_pinned_provider_schemas_through_the_public_cli() {
    use serde_json::json;
    let sandbox = Sandbox::new("generic-evidence");
    let project = sandbox.root.join("proj");
    let config_path = project.join("lekalo.evidence.json");
    let index_path = project.join(".lekalo/import/observed/index.json");
    let node = std::env::split_paths(&std::env::var_os("PATH").expect("PATH"))
        .map(|path| path.join(if cfg!(windows) { "node.exe" } else { "node" }))
        .find(|path| path.is_file())
        .expect("Node.js available for the protocol fixture");
    let adapter_path = sandbox.root.join("evidence-adapter.cjs");
    let adapter = include_bytes!("../../../tests/fixtures/target-protocol/evidence-adapter.cjs");
    std::fs::write(&adapter_path, adapter).unwrap();
    let producer = json!({"id":"evidence-demo","version":"1.0.0",
        "digest":lekalo_core::runtime::digest(b"demo producer")});
    let entry_digest = lekalo_core::runtime::digest(adapter);
    let program_digest = lekalo_core::runtime::digest(&std::fs::read(&node).unwrap());
    let strings = evidence_package(&project, "labels-provider", json!({"type":"string"}));
    let numbers = evidence_package(&project, "counts-provider", json!({"type":"integer"}));
    let mut config = json!({"schema":"lekalo/evidence-config/v1","scans":[{
        "target":"evidence-demo", "producer":producer, "entry_digest":entry_digest, "program_digest":program_digest,
        "families":{"labels":strings, "counts":numbers}
    }]});
    let write_config = |config: &Value| {
        std::fs::write(&config_path, serde_json::to_vec(config).unwrap()).unwrap();
    };
    let scan = |version: &str, groups: &Value| {
        let wire = json!({"producer":producer, "groups":groups}).to_string();
        sandbox.run(&[
            "--json",
            "scan",
            "--target",
            "evidence-demo",
            "--project",
            "proj",
            node.to_str().unwrap(),
            alias_free_path(&adapter_path).to_str().unwrap(),
            &wire,
            version,
        ])
    };
    let accepted = |version, groups: &Value| {
        let out = scan(version, groups);
        assert_eq!(
            exit_code(&out),
            0,
            "{}",
            stdout_text(&out) + &stderr_text(&out)
        );
        std::fs::read(&index_path).unwrap()
    };
    let source = |path: &str| json!({"path":path,"source_digest":lekalo_core::runtime::digest(&std::fs::read(project.join(path)).unwrap())});
    let group = |family, pin: &Value, payload| {
        json!({
            "family":family, "coverage":"complete", "package_digest":pin["package_digest"],
            "schema_digest":pin["schema_digest"], "facts":[{
                "subject":"root", "key":"example", "references":{"dependency":"dependency"},
                "sources":[source("src/tasks.ts"), source("src/ids.ts")], "payload":payload,
            }]
        })
    };
    let labels = group("labels", &strings, json!("ready"));
    let counts = group("counts", &numbers, json!(7));
    let groups = json!([labels, counts]);

    let native = accepted("1.1.0", &json!([]));
    assert!(String::from_utf8_lossy(&native).contains("observed-index/v1.1.0"));
    write_config(&config);
    let observed = accepted("1.4.0", &groups);
    let index: Value = serde_json::from_slice(&observed).unwrap();
    let rows = &index["evidence_groups"];
    assert_eq!(rows[0]["facts"][0]["fact"]["payload"], 7);
    assert_eq!(rows[1]["facts"][0]["fact"]["payload"], "ready");
    let label_id = rows[1]["facts"][0]["id"].clone();
    assert_eq!(accepted("1.4.0", &json!([counts, labels])), observed);
    assert!(lekalo_core::observed::parse_index(&observed).is_ok());

    let refused = |version, groups: &Value, detail| {
        assert_scan_refusal(&scan(version, groups), detail);
        assert_eq!(std::fs::read(&index_path).unwrap(), observed);
    };
    let mut invalid = labels.clone();
    invalid["family"] = json!("unknown");
    refused("1.4.0", &json!([invalid]), "evidence-family-disabled");
    let mut invalid = labels.clone();
    invalid["facts"][0]["payload"] = json!(7);
    refused("1.4.0", &json!([invalid]), "evidence-payload");
    let mut invalid = labels.clone();
    invalid["facts"][0]["references"]["dependency"] = json!("unknown");
    refused("1.4.0", &json!([invalid]), "evidence-source-custody");
    let mut invalid = labels.clone();
    invalid["facts"][0]["sources"][1]["source_digest"] = json!(entry_digest);
    refused("1.4.0", &json!([invalid]), "evidence-source-custody");
    let mut invalid = labels.clone();
    invalid["schema_digest"] = numbers["schema_digest"].clone();
    refused("1.4.0", &json!([invalid]), "evidence-package-pin");
    std::fs::write(
        project.join("labels-provider/payload.schema.json"),
        br#"{"type":"array"}"#,
    )
    .unwrap();
    refused("1.4.0", &groups, "evidence-package-invalid");
    assert_eq!(
        evidence_package(&project, "labels-provider", json!({"type":"string"})),
        strings,
    );
    config["scans"][0]["entry_digest"] = json!(producer["digest"]);
    write_config(&config);
    refused("1.4.0", &groups, "evidence-producer");
    config["scans"][0]["entry_digest"] = json!(entry_digest);
    write_config(&config);
    config["scans"][0]["producer"]["id"] = json!("another-scanner");
    write_config(&config);
    refused("1.4.0", &groups, "evidence-producer");
    config["scans"][0]["producer"] = producer.clone();
    write_config(&config);
    refused("1.1.0", &json!([]), "evidence-protocol-required");

    std::fs::write(sandbox.root.join("generic.json"), &observed).unwrap();
    assert_scan_refusal(
        &sandbox.run(&[
            "--json",
            "observe",
            "update",
            "--scan",
            "generic.json",
            "--project",
            "proj",
        ]),
        "evidence-requires-target-scan",
    );
    assert_scan_refusal(
        &sandbox.run(&[
            "--json",
            "observe",
            "update",
            "--scan",
            INITIAL_SCAN,
            "--project",
            "proj",
        ]),
        "evidence-inventory-downgrade",
    );
    assert_eq!(std::fs::read(&index_path).unwrap(), observed);

    let mut partial_counts = counts.clone();
    partial_counts["coverage"] = json!("partial");
    partial_counts["facts"] = json!([]);
    let partial: Value =
        serde_json::from_slice(&accepted("1.4.0", &json!([labels, partial_counts]))).unwrap();
    assert_eq!(partial["evidence_groups"][0]["coverage"], "partial");
    assert_eq!(
        partial["evidence_groups"][0]["facts"][0]["state"],
        "unassessed"
    );

    let replacement = evidence_package(
        &project,
        "labels-provider",
        json!({"type":"string","minLength":1}),
    );
    config["scans"][0]["families"]["labels"] = replacement.clone();
    write_config(&config);
    let migrated = group("labels", &replacement, json!("updated"));
    let next: Value = serde_json::from_slice(&accepted("1.4.0", &json!([migrated]))).unwrap();
    assert_eq!(next["evidence_groups"][1]["facts"][0]["id"], label_id);
    assert_eq!(next["evidence_groups"][0]["coverage"], "unassessed");
    assert_eq!(
        next["evidence_groups"][0]["facts"][0]["state"],
        "unassessed"
    );
    std::fs::write(project.join("src/ids.ts"), "// dependency changed\n").unwrap();
    assert_scan_refusal(
        &sandbox.run(&["--json", "observe", "check", "--project", "proj"]),
        "evidence-source-stale",
    );
    let empty_labels = json!({"family":"labels", "coverage":"complete",
        "package_digest":replacement["package_digest"], "schema_digest":replacement["schema_digest"],
        "facts":[]});
    let next: Value = serde_json::from_slice(&accepted("1.4.0", &json!([empty_labels]))).unwrap();
    assert_eq!(next["evidence_groups"][1]["facts"], json!([]));
    assert_eq!(next["evidence_groups"][0]["facts"][0]["state"], "stale");

    let with_binding = evidence_package_with_binding(
        &project,
        "labels-provider",
        json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}),
        Some(json!({
            "schema":"lekalo/evidence-binding-rules/v1.3.1",
            "kind":"command", "identity":["id"], "match":{"id":"id"},
            "source_roles":["subject","dependency"]
        })),
    );
    config["scans"][0]["families"]["labels"] = with_binding.clone();
    write_config(&config);
    let mut with_sources = group("labels", &with_binding, json!({"id":"taskboard.task"}));
    let checked = accepted("1.4.0", &json!([with_sources]));
    with_sources["facts"][0]["references"] = json!({});
    assert_scan_refusal(
        &scan("1.4.0", &json!([with_sources])),
        "evidence-binding-source-role",
    );
    assert_eq!(std::fs::read(&index_path).unwrap(), checked);

    // The root declaration remains byte-for-byte unchanged. Only the
    // source of the fact's named dependency changes after the proposal.
    let proposals = sandbox.json(&["bindings", "propose", "--project", "proj"]);
    let root_proposal = proposals["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|proposal| proposal["symbol"] == "taskboard.task")
        .unwrap();
    let proposal_id = root_proposal["proposal"].as_str().unwrap();
    let batch = sandbox.json(&[
        "bindings",
        "confirm",
        "--batch",
        "--preview",
        "--project",
        "proj",
    ]);
    let plan_id = batch["plan"].as_str().unwrap();
    let dependency = project.join("src/ids.ts");
    let original = std::fs::read(&dependency).unwrap();
    std::fs::write(&dependency, "// dependency edited after the proposal\n").unwrap();
    assert_scan_refusal(
        &sandbox.run(&[
            "--json",
            "bindings",
            "confirm",
            proposal_id,
            "--project",
            "proj",
        ]),
        "candidate-source-drift",
    );
    assert_scan_refusal(
        &sandbox.run(&[
            "--json",
            "bindings",
            "confirm",
            "--batch",
            "--confirm",
            plan_id,
            "--project",
            "proj",
        ]),
        "candidate-source-drift",
    );
    assert_scan_refusal(
        &sandbox.run(&[
            "--json",
            "observe",
            "confirm",
            "taskboard.task",
            "--project",
            "proj",
        ]),
        "candidate-source-drift",
    );
    let registry = sandbox.json(&["bindings", "list", "--project", "proj"]);
    let root_binding = registry["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|binding| binding["semantic"] == "taskboard.task")
        .unwrap();
    assert_eq!(root_binding["state"], "stale");
    assert_eq!(registry["counts"]["stale"], 1);
    let audit = sandbox.run(&["--json", "bindings", "audit", "--project", "proj"]);
    assert_scan_refusal(&audit, "fingerprint-mismatch");
    let audit: Value = serde_json::from_slice(&audit.stderr).unwrap();
    assert!(audit["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["id"] == "observed.stale-binding"
                && diagnostic["data"]["symbol"] == "taskboard.task"
        }));
    assert_eq!(std::fs::read(&index_path).unwrap(), checked);
    std::fs::write(&dependency, original).unwrap();
    let confirmed = sandbox.json(&["bindings", "confirm", proposal_id, "--project", "proj"]);
    assert_eq!(confirmed["binding"], "confirmed");

    std::fs::remove_file(&config_path).unwrap();
    std::fs::remove_file(&index_path).unwrap();
    let http = accepted("1.2.0", &json!([]));
    assert!(String::from_utf8_lossy(&http).contains("observed-index/v1.2.0"));
    assert_scan_refusal(
        &scan("1.4.0", &json!([])),
        "evidence-http-migration-required",
    );
    assert_eq!(std::fs::read(&index_path).unwrap(), http);
}

fn assert_scan_refusal(output: &Output, detail: &str) {
    assert_ne!(exit_code(output), 0);
    let rendered = stdout_text(output) + &stderr_text(output);
    assert!(rendered.contains(detail), "{detail}: {rendered}");
}
