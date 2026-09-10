//! CLI integration tests for the issue #42 binding registry: the
//! adapter-driven `lekalo scan` with the reference node-typescript
//! scanner, `bindings list/propose/confirm/audit`, the ambiguity gate,
//! batch preview/apply, the set-once target, and the clean protection of
//! sensitive paths. Every test runs the real binary over a sandbox copy
//! of the committed fixture; temp roots stay under `target/` addressed
//! with relative selectors — the Windows 8.3-alias lesson — and binary
//! invocations go through `alias_free_path`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const APP: &str = "tests/fixtures/bindings/app";
const SCANNER: &str = "tests/fixtures/bindings/ts-scanner.mjs";

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
    workspace: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let unique = NEXT_SANDBOX.fetch_add(1, Ordering::SeqCst);
        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let root = target.join(format!(
            "bindings-cli-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("sandbox parent");
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join(APP),
            &root.join("proj"),
        );
        let workspace = alias_free_path(&root);
        Self { root, workspace }
    }

    fn run(&self, args: &[&str]) -> Output {
        lekalo_in(&self.workspace, args)
    }

    fn scan(&self, extra: &[&str]) -> Output {
        let scanner = self.scanner();
        let mut args = vec![
            "--json",
            "scan",
            "--target",
            "node-typescript",
            "--profile",
            "default",
            "--project",
            "proj",
            "node",
            scanner.as_str(),
        ];
        args.extend_from_slice(extra);
        self.run(&args)
    }
    fn scanner(&self) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(SCANNER)
            .to_string_lossy()
            .into_owned()
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
fn scan_records_the_registry_without_reading_sensitive_paths_or_writing_sources() {
    let sandbox = Sandbox::new("scan");
    let fingerprint = |name: &str| {
        let bytes = std::fs::read(sandbox.root.join(name)).expect("source");
        lekalo_core::versioning::plan::sha256_hex(&bytes)
    };
    let before_tasks = fingerprint("proj/src/tasks.ts");
    let before_env = fingerprint("proj/.env");

    let receipt = sandbox.scan(&[]);
    assert_eq!(exit_code(&receipt), 0, "{}", stderr_text(&receipt));
    let receipt: Value = serde_json::from_str(stdout_text(&receipt).trim()).expect("receipt");
    assert_eq!(receipt["status"], "valid");
    assert_eq!(receipt["symbols"], 6);
    assert_eq!(receipt["inferred"], 6);
    assert_eq!(receipt["test_bindings"], 3);
    assert_eq!(receipt["target"], "node-typescript");
    assert_eq!(receipt["profile"], "default");

    // No source file changed: the scan writes only the registry.
    assert_eq!(fingerprint("proj/src/tasks.ts"), before_tasks);
    assert_eq!(fingerprint("proj/.env"), before_env);
    assert!(
        sandbox
            .root
            .join("proj/.lekalo/import/observed/index.json")
            .is_file(),
        "the registry persists in the observed-model-draft home"
    );

    // Sensitive and excluded paths never enter the registry, and the
    // secret never leaks into any persisted byte.
    let index =
        std::fs::read_to_string(sandbox.root.join("proj/.lekalo/import/observed/index.json"))
            .expect("index bytes");
    assert!(!index.contains(".env"), "the secret file is not referenced");
    assert!(
        !index.contains("node_modules"),
        "excluded trees are skipped"
    );
    assert!(!index.contains("supersecret"), "the secret never leaks");
    assert_eq!(fingerprint("proj/.env"), before_env);

    // The scanner genuinely found the exported TypeScript constructs.
    let list = sandbox.json(&["bindings", "list", "--project", "proj"]);
    let rows: Vec<&Value> = list["bindings"]
        .as_array()
        .expect("rows")
        .iter()
        .filter(|row| row["semantic"] == "taskboard.create_task")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["relation"], "implements");
    assert_eq!(rows[0]["native"], "src/tasks.ts#createTask");
    assert_eq!(rows[0]["source"], "detected");
    assert_eq!(list["tests"].as_array().map(Vec::len), Some(3));
}

#[test]
fn scan_refuses_an_undeclared_target_through_the_protocol() {
    let sandbox = Sandbox::new("target");
    let output = sandbox.run(&[
        "--json",
        "scan",
        "--target",
        "php-laravel",
        "--project",
        "proj",
        "node",
        &sandbox.scanner(),
    ]);
    // The reference scanner declares only node-typescript, so the
    // capability refusal of the accepted protocol fires.
    assert_eq!(exit_code(&output), 4, "{}", stderr_text(&output));
    let refused: Value = serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
    assert_eq!(refused["status"], "unsupported");
    assert!(
        refused
            .to_string()
            .contains("target.capability-unsupported"),
        "{refused}"
    );
    assert!(
        !sandbox
            .root
            .join("proj/.lekalo/import/observed/index.json")
            .exists(),
        "a refused scan never writes the registry"
    );
}

#[test]
fn propose_confirm_and_audit_flow_with_batch_and_ambiguity() {
    let sandbox = Sandbox::new("flow");
    let output = sandbox.scan(&[]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));

    // Propose: six deterministic proposals, none ambiguous.
    let proposals = sandbox.json(&["bindings", "propose", "--project", "proj"]);
    assert_eq!(proposals["proposals"].as_array().map(Vec::len), Some(6));
    assert_eq!(proposals["ambiguous"], 0);
    let first = proposals["proposals"][0]["proposal"]
        .as_str()
        .expect("proposal id")
        .to_owned();
    let create_task = proposals["proposals"]
        .as_array()
        .expect("list")
        .iter()
        .find(|p| p["symbol"] == "taskboard.create_task")
        .expect("create task proposal")["proposal"]
        .as_str()
        .expect("id")
        .to_owned();

    // Single confirmation: exactly one candidate resolves it.
    let output = sandbox.run(&["bindings", "confirm", &create_task, "--project", "proj"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert!(stdout_text(&output).contains("taskboard.create_task"));
    assert!(stdout_text(&output).contains("src/tasks.ts#createTask"));

    // The stale proposal id refuses with the registered rule.
    let output = sandbox.run(&["--json", "bindings", "confirm", &first, "--project", "proj"]);
    if first != create_task {
        assert_eq!(exit_code(&output), 1);
        assert!(stderr_text(&output).contains("bindings.proposal-unknown"));
    }

    // Batch preview then apply; the apply refuses a foreign plan id.
    let preview = sandbox.json(&[
        "bindings",
        "confirm",
        "--batch",
        "--preview",
        "--project",
        "proj",
    ]);
    assert_eq!(preview["entries"].as_array().map(Vec::len), Some(5));
    let plan = preview["plan"].as_str().expect("plan id").to_owned();
    let output = sandbox.run(&[
        "--json",
        "bindings",
        "confirm",
        "--batch",
        "--confirm",
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("bindings.plan-mismatch"));
    let applied = sandbox.json(&[
        "bindings",
        "confirm",
        "--batch",
        "--confirm",
        &plan,
        "--project",
        "proj",
    ]);
    assert_eq!(applied["confirmed"].as_array().map(Vec::len), Some(5));

    // The confirmed bindings keep their provenance and survive scans.
    let drift = sandbox.root.join("proj/src/legacy.ts");
    std::fs::write(&drift, b"export function focusTask(dup: string): void {}\n")
        .expect("ambiguous duplicate");
    let output = sandbox.scan(&[]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let list = sandbox.json(&["bindings", "list", "--project", "proj"]);
    assert_eq!(list["counts"]["confirmed"], 6);
    let focus = list["bindings"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["semantic"] == "taskboard.focus_task")
        .expect("focus row");
    assert_eq!(focus["source"], "user-confirmed");
    assert!(focus["candidates"].is_null());

    // The staleness audit after a source change: drifting the recorded
    // source of the confirmed bindings fails the gate.
    let recorded = sandbox.root.join("proj/src/tasks.ts");
    std::fs::write(&recorded, b"export function drifted() {}\n").expect("drift");
    let output = sandbox.run(&["--json", "bindings", "audit", "--project", "proj"]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("observed.stale-binding"));
    assert!(stderr_text(&output).contains("fingerprint-mismatch"));
}

#[test]
fn ambiguous_proposals_refuse_until_a_candidate_is_named() {
    let sandbox = Sandbox::new("ambiguous");
    let output = sandbox.scan(&[]);
    assert_eq!(exit_code(&output), 0);
    // A duplicate export creates two candidates for one semantic id.
    std::fs::write(
        sandbox.root.join("proj/src/legacy.ts"),
        b"export function focusTask(dup: string): void {}\n",
    )
    .expect("duplicate");
    let output = sandbox.scan(&[]);
    assert_eq!(exit_code(&output), 0);

    let proposals = sandbox.json(&["bindings", "propose", "--project", "proj"]);
    assert_eq!(proposals["ambiguous"], 1);
    let proposal = proposals["proposals"]
        .as_array()
        .expect("list")
        .iter()
        .find(|p| p["symbol"] == "taskboard.focus_task")
        .expect("ambiguous proposal")
        .clone();
    assert_eq!(proposal["ambiguous"], true);
    assert_eq!(proposal["candidates"].as_array().map(Vec::len), Some(2));

    // Confirming without a candidate refuses; naming one resolves to
    // exactly that one — including the second row of the list.
    let id = proposal["proposal"].as_str().expect("id").to_owned();
    let output = sandbox.run(&["--json", "bindings", "confirm", &id, "--project", "proj"]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("bindings.ambiguous"));
    let output = sandbox.run(&[
        "bindings",
        "confirm",
        &id,
        "--candidate",
        "src/legacy.ts#focusTask",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert!(stdout_text(&output).contains("src/legacy.ts#focusTask"));
    let list = sandbox.json(&["bindings", "list", "--project", "proj"]);
    let focus = list["bindings"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["semantic"] == "taskboard.focus_task")
        .expect("focus row");
    assert_eq!(focus["native"], "src/legacy.ts#focusTask");
    assert_eq!(focus["source"], "user-confirmed");

    // A name outside the candidate set refuses as well.
    std::fs::write(
        sandbox.root.join("proj/src/second.ts"),
        b"export function listTasks(): void {}\n",
    )
    .expect("duplicate two");
    let output = sandbox.scan(&[]);
    assert_eq!(exit_code(&output), 0);
    let proposals = sandbox.json(&["bindings", "propose", "--project", "proj"]);
    let proposal = proposals["proposals"]
        .as_array()
        .expect("list")
        .iter()
        .find(|p| p["symbol"] == "taskboard.list_tasks")
        .expect("ambiguous list_tasks")
        .clone();
    let id = proposal["proposal"].as_str().expect("id").to_owned();
    let output = sandbox.run(&[
        "--json",
        "bindings",
        "confirm",
        &id,
        "--candidate",
        "src/nowhere.ts#listTasks",
        "--project",
        "proj",
    ]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("bindings.ambiguous"));
}
