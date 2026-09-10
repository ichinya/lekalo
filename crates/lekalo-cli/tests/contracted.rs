//! CLI integration tests for `lekalo contract` (issue #40): the
//! contracted-mode flow over the planner reference slice — declaration
//! merge, the conformance gate, native-test attachment, support-artifact
//! ownership, and every drift class the issue requires Lekalo to detect.
//! Every test runs the real binary over a sandbox copy of the fixture;
//! temp roots stay under `target/` addressed with relative selectors —
//! the Windows 8.3-alias lesson — and binary invocations go through
//! `alias_free_path`. The maintained TypeScript sources must be
//! byte-identical after every flow: the generator never overwrites
//! maintained implementation.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const SLICE: &str = "tests/fixtures/contracted/planner-slice";

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
            "contracted-cli-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("sandbox parent");
        let reference = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(SLICE);
        copy_dir(&reference, &root.join("proj"));
        Self { root }
    }

    fn project(&self) -> PathBuf {
        self.root.join("proj")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lekalo"))
            .args(args)
            .current_dir(alias_free_path(&self.root))
            .env_remove("LEKALO_PROJECT")
            .output()
            .expect("run the real lekalo binary")
    }

    fn update(&self, declaration: &str) -> Output {
        self.run(&[
            "contract",
            "update",
            "--declaration",
            declaration,
            "--project",
            "proj",
        ])
    }

    fn check(&self) -> Output {
        self.run(&["contract", "check", "--project", "proj"])
    }

    fn json(&self, args: &[&str]) -> Value {
        let mut with_json = vec!["--json"];
        with_json.extend_from_slice(args);
        let output = self.run(&with_json);
        assert_eq!(exit_code(&output), 0, "{args:?}: {}", stderr_text(&output));
        serde_json::from_str(stdout_text(&output).trim()).expect("valid receipt json")
    }

    /// The sha256 of one maintained source file in the sandbox.
    /// Run the gate with --json and parse the invalid envelope from
    /// stderr; returns (rule id, detail, symbol) rows.
    fn check_invalid(&self) -> Vec<(String, String, String)> {
        let output = self.run(&["--json", "contract", "check", "--project", "proj"]);
        assert_eq!(exit_code(&output), 1, "{}", stdout_text(&output));
        let envelope: Value =
            serde_json::from_str(stderr_text(&output).trim()).expect("invalid envelope json");
        envelope["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic["id"].as_str().expect("id").to_owned(),
                    diagnostic["data"]["detail"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    diagnostic["symbol"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect()
    }

    fn fingerprint(&self, name: &str) -> String {
        let bytes = std::fs::read(self.project().join(name)).expect("source");
        format!(
            "sha256:{}",
            lekalo_core::versioning::plan::sha256_hex(&bytes)
        )
    }
}

#[test]
fn the_first_contracted_slice_passes_conformance() {
    let sandbox = Sandbox::new("pass");
    let before_focus = sandbox.fingerprint("src/focus.ts");
    let before_queries = sandbox.fingerprint("src/queries.ts");

    let receipt = sandbox.json(&[
        "contract",
        "update",
        "--declaration",
        "proj/declarations/initial.json",
        "--project",
        "proj",
    ]);
    assert_eq!(receipt["status"], "valid");
    assert_eq!(receipt["symbols"], 3);
    assert_eq!(receipt["recorded"].as_array().expect("recorded").len(), 3);

    // Attach native coverage to both bound operations.
    let receipt = sandbox.json(&[
        "contract",
        "attach",
        "planner.focus_task",
        "--native-test",
        "npm test -- focusTask",
        "--project",
        "proj",
    ]);
    assert_eq!(receipt["native_tests"].as_array().expect("tests").len(), 1);

    let receipt = sandbox.json(&[
        "contract",
        "attach",
        "planner.list_tasks",
        "--native-test",
        "npm test -- listTasks",
        "--project",
        "proj",
    ]);
    assert_eq!(receipt["native_tests"].as_array().expect("tests").len(), 1);

    // Register the generated support artifact and materialize its exact
    // bytes inside the generated home.
    let digest = {
        let bytes =
            std::fs::read(sandbox.project().join("openapi-planner.json")).expect("artifact");
        format!(
            "sha256:{}",
            lekalo_core::versioning::plan::sha256_hex(&bytes)
        )
    };
    let digest = digest.leak() as &str;
    let receipt = sandbox.json(&[
        "contract",
        "support",
        "planner.focus_task",
        "--kind",
        "openapi",
        "--path",
        ".lekalo/generated/openapi/planner.json",
        "--digest",
        digest,
        "--project",
        "proj",
    ]);
    assert_eq!(receipt["kind"], "openapi");

    std::fs::create_dir_all(sandbox.project().join(".lekalo/generated/openapi"))
        .expect("create generated home");
    std::fs::copy(
        sandbox.project().join("openapi-planner.json"),
        sandbox
            .project()
            .join(".lekalo/generated/openapi/planner.json"),
    )
    .expect("materialize support artifact");

    let receipt = sandbox.json(&["contract", "check", "--project", "proj"]);
    assert_eq!(receipt["status"], "valid");
    assert_eq!(receipt["symbols"], 3);
    assert_eq!(receipt["conformant"], 3);
    assert_eq!(receipt["stale"], 0);
    assert_eq!(receipt["artifacts"], 1);
    assert_eq!(receipt["staleArtifacts"], 0);
    assert!(receipt["drifts"].as_array().expect("drifts").is_empty());

    // The maintained implementation is never overwritten.
    assert_eq!(sandbox.fingerprint("src/focus.ts"), before_focus);
    assert_eq!(sandbox.fingerprint("src/queries.ts"), before_queries);
}

#[test]
fn module_scope_requires_every_operation_implemented() {
    let sandbox = Sandbox::new("scope");
    sandbox.update("proj/declarations/initial.json");
    // Drop the query record: the module slice is no longer complete.
    let declaration_path = sandbox.project().join("declarations/initial.json");
    let mut declaration: Value = serde_json::from_str(
        &std::fs::read_to_string(&declaration_path).expect("read declaration"),
    )
    .expect("parse declaration");
    declaration["symbols"] = Value::Array(
        declaration["symbols"]
            .as_array()
            .expect("symbols")
            .iter()
            .filter(|symbol| symbol["id"] == "planner.focus_task")
            .cloned()
            .collect(),
    );
    std::fs::write(
        &declaration_path,
        serde_json::to_string_pretty(&declaration).expect("render") + "\n",
    )
    .expect("write declaration");
    sandbox.update("proj/declarations/initial.json");

    let output = sandbox.run(&[
        "--json",
        "contract",
        "check",
        "--module",
        "planner",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    let rows: Vec<(String, String)> = envelope["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| {
            (
                diagnostic["id"].as_str().expect("id").to_owned(),
                diagnostic["data"]["detail"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect();
    assert!(rows
        .iter()
        .any(|(id, detail)| { id == "contracted.binding-drift" && detail == "unimplemented" }));
}

#[test]
fn signature_drift_fails_the_gate() {
    let sandbox = Sandbox::new("signature");
    sandbox.update("proj/declarations/initial.json");
    sandbox.update("proj/declarations/drift-signature.json");

    let rows = sandbox.check_invalid();
    assert!(rows.iter().any(|(id, detail, symbol)| {
        id == "contracted.binding-drift" && detail == "signature" && symbol == "planner.focus_task"
    }));
}

#[test]
fn effect_drift_fails_the_gate() {
    let sandbox = Sandbox::new("effects");
    sandbox.update("proj/declarations/initial.json");
    sandbox.update("proj/declarations/drift-effects.json");

    let rows = sandbox.check_invalid();
    assert!(rows
        .iter()
        .any(|(id, detail, _)| { id == "contracted.binding-drift" && detail == "effects" }));
}

#[test]
fn source_and_artifact_changes_are_detected_as_stale() {
    let sandbox = Sandbox::new("stale");
    sandbox.update("proj/declarations/initial.json");
    sandbox.update("proj/declarations/drift-signature.json");
    sandbox.update("proj/declarations/drift-effects.json");
    // Neutralize the declaration drift: only source and artifact drift
    // remain.
    sandbox.update("proj/declarations/initial.json");

    // Attach coverage so the staleness finding is the only one.
    let _ = sandbox.run(&[
        "contract",
        "attach",
        "planner.focus_task",
        "--native-test",
        "npm test -- focusTask",
        "--project",
        "proj",
    ]);
    let _ = sandbox.run(&[
        "contract",
        "attach",
        "planner.list_tasks",
        "--native-test",
        "npm test -- listTasks",
        "--project",
        "proj",
    ]);

    // A maintained source edit invalidates the binding fingerprint.
    let focus = sandbox.project().join("src/focus.ts");
    let mut source = std::fs::read_to_string(&focus).expect("read source");
    source.push_str("\n// a late maintainer edit\n");
    std::fs::write(&focus, source).expect("write source");

    // The support artifact moved away from its recorded digest.
    std::fs::create_dir_all(sandbox.project().join(".lekalo/generated/openapi"))
        .expect("create generated home");
    std::fs::write(
        sandbox
            .project()
            .join(".lekalo/generated/openapi/planner.json"),
        "{\n  \"openapi\": \"3.0.1\"\n}\n",
    )
    .expect("write stale artifact");

    let rows = sandbox.check_invalid();
    assert!(rows.iter().any(|(id, detail, _)| {
        id == "contracted.binding-drift" && detail == "fingerprint-mismatch"
    }));
    assert!(rows
        .iter()
        .any(|(id, _, _)| id == "contracted.stale-artifact"));
}

#[test]
fn coverage_gaps_are_registered_findings() {
    let sandbox = Sandbox::new("coverage");
    sandbox.update("proj/declarations/initial.json");

    let rows = sandbox.check_invalid();
    assert!(rows
        .iter()
        .any(|(id, _, _)| id == "contracted.coverage-missing"));
}

#[test]
fn maintained_paths_refuse_support_claims() {
    let sandbox = Sandbox::new("refuse");
    sandbox.update("proj/declarations/initial.json");

    let output = sandbox.run(&[
        "--json",
        "contract",
        "support",
        "planner.focus_task",
        "--kind",
        "types",
        "--path",
        "src/focus.ts",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    let diagnostic = &envelope["diagnostics"][0];
    assert_eq!(diagnostic["id"], "contracted.declaration-invalid");
    assert_eq!(diagnostic["data"]["detail"], "support-path-refused");
    assert_eq!(diagnostic["data"]["target"], "src/focus.ts");
}

#[test]
fn unknown_symbols_and_selectors_fail_closed() {
    let sandbox = Sandbox::new("unknown");
    sandbox.update("proj/declarations/initial.json");

    let output = sandbox.run(&[
        "contract",
        "attach",
        "planner.ghost",
        "--native-test",
        "npm test -- ghost",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    assert!(
        stderr_text(&output).contains("contracted.unknown-symbol"),
        "{}",
        stderr_text(&output)
    );

    // A declaration naming an unknown canonical symbol refuses.
    let declaration_path = sandbox.project().join("declarations/initial.json");
    let mut declaration: Value = serde_json::from_str(
        &std::fs::read_to_string(&declaration_path).expect("read declaration"),
    )
    .expect("parse declaration");
    declaration["symbols"]
        .as_array_mut()
        .expect("symbols")
        .push(serde_json::json!({
            "id": "planner.ghost",
            "kind": "command",
            "source": {"path": "src/ghost.ts"},
            "signature": {"inputs": [], "output": null, "reads": []},
            "effects": []
        }));
    std::fs::write(
        &declaration_path,
        serde_json::to_string_pretty(&declaration).expect("render") + "\n",
    )
    .expect("write declaration");
    let output = sandbox.update("proj/declarations/initial.json");
    assert_eq!(exit_code(&output), 1);
    assert!(
        stderr_text(&output).contains("contracted.unknown-symbol"),
        "{}",
        stderr_text(&output)
    );
}

#[test]
fn missing_registry_refuses_the_gate() {
    let sandbox = Sandbox::new("missing");
    let output = sandbox.check();
    assert_eq!(exit_code(&output), 1);
    assert!(
        stderr_text(&output).contains("contracted.registry-io"),
        "{}",
        stderr_text(&output)
    );
}
