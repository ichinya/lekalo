//! The issue #49 observed-mode Node consumer pilot: one end-to-end run
//! of the accepted flow over the synthetic `taskhub` pnpm-workspace
//! fixture — adopt, scan, bind/confirm, inspect, impact, promotion,
//! capsule, native gates, and the saved baseline — with every
//! acceptance criterion asserted as a named step. Every stage runs the
//! real binary over a sandbox copy of the committed fixture; temp roots
//! stay under `target/` addressed with relative selectors — the Windows
//! 8.3-alias lesson — and binary invocations go through
//! `alias_free_path`.
//!
//! Privacy boundary of the assertions: `taskhub` is the fixture's own
//! fictional slug (every committed name is synthetic), so semantic ids
//! and receipts may carry it; what must never appear in any emitted
//! byte is the host's absolute path and the committed fake `.env`
//! secrets, and `.env` itself is never read (fingerprint-proven) and
//! never referenced by the observed artifacts.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const TASKHUB: &str = "tests/fixtures/pilot/taskhub";
const SCAN: &str = "tests/fixtures/pilot/taskhub-scans/taskhub.scan.v0_2_16.json";
const NATIVE_PLAN: &str = "tests/fixtures/pilot/taskhub-gates/plan.golden.json";

/// The committed fake `.env` secrets: no emitted byte may ever carry
/// them, and the file itself is never read by any stage.
const ENV_SECRETS: [&str; 2] = [
    "sync-secret-0001-not-a-real-credential",
    "calendar-token-0001-not-a-real-credential",
];

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

/// Every project-relative file path with its exact byte length, walked
/// read-only through the sandbox tree.
fn snapshot(root: &Path) -> BTreeMap<String, u64> {
    let mut files = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("walk entry") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if entry.file_type().expect("entry type").is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("inside root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let length = entry.metadata().expect("metadata").len();
                files.insert(relative, length);
            }
        }
    }
    files
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let unique = NEXT_SANDBOX.fetch_add(1, Ordering::SeqCst);
        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let root = target.join(format!("pilot-cli-{tag}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("sandbox parent");
        let reference = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(TASKHUB);
        copy_dir(&reference, &root.join("proj"));
        // The pinned scan and native plan documents are invocation-
        // relative inputs, never project files.
        std::fs::create_dir_all(root.join("scans")).expect("sandbox scans");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join(SCAN),
            root.join("scans/taskhub.scan.v0_2_16.json"),
        )
        .expect("copy pinned scan");
        std::fs::create_dir_all(root.join("plans")).expect("sandbox plans");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join(NATIVE_PLAN),
            root.join("plans/plan.golden.json"),
        )
        .expect("copy pinned native plan");
        Self {
            root: alias_free_path(&root),
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        lekalo_in(&self.root, args)
    }

    /// Run one stage, assert success, and feed both streams to the
    /// privacy transcript.
    fn stage(&self, transcript: &mut String, args: &[&str]) -> Value {
        let output = self.run(args);
        transcript.push_str(&stdout_text(&output));
        transcript.push_str(&stderr_text(&output));
        assert_eq!(exit_code(&output), 0, "{args:?}: {}", stderr_text(&output));
        serde_json::from_str(stdout_text(&output).trim()).expect("json envelope")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The whole pilot: every acceptance criterion in flow order.
#[test]
fn the_taskhub_pilot_runs_end_to_end_without_touching_the_consumer() {
    let sandbox = Sandbox::new("e2e");
    let mut transcript = String::new();

    // -------------------------------------------------------------
    // Criterion: the consumer tree is never rewritten. Snapshot every
    // pre-adopt file length; `.env` gets a byte-exact fingerprint.
    // -------------------------------------------------------------
    let before = snapshot(&sandbox.root.join("proj"));
    assert!(before.contains_key(".env"), "the fixture carries .env");
    let env_before = {
        let bytes = std::fs::read(sandbox.root.join("proj/.env")).expect(".env bytes");
        lekalo_core::digest::sha256_hex(&bytes)
    };

    // -------------------------------------------------------------
    // Criterion: `lekalo init --adopt` creates only Lekalo config and
    // model files, and the detection is honest about the workspace.
    // -------------------------------------------------------------
    let adopt = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "init",
            "--adopt",
            "--project-id",
            "taskhub",
            "--project",
            "proj",
        ],
    );
    assert_eq!(adopt["status"], "valid");
    assert_eq!(adopt["gate"]["status"], "valid");

    // -------------------------------------------------------------
    // The observed modules the scan will populate; the model declares
    // them through the module seam, never by rewriting the consumer.
    // -------------------------------------------------------------
    for module in [
        "taskhub_tasks",
        "taskhub_events",
        "taskhub_integrations",
        "taskhub_sync",
        "taskhub_api",
    ] {
        sandbox.stage(
            &mut transcript,
            &["--json", "module", "new", module, "--project", "proj"],
        );
    }

    // -------------------------------------------------------------
    // Criterion: the pinned scan populates the observed index; every
    // source file and .env byte stays untouched.
    // -------------------------------------------------------------
    let update = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "update",
            "--scan",
            "scans/taskhub.scan.v0_2_16.json",
            "--project",
            "proj",
        ],
    );
    assert_eq!(update["status"], "valid");
    assert_eq!(update["symbols"], 35);
    assert_eq!(update["inferred"], 35);
    assert!(
        sandbox
            .root
            .join("proj/.lekalo/import/observed/index.json")
            .is_file(),
        "the index persists in the observed-model-draft home"
    );

    // -------------------------------------------------------------
    // Criteria: the Task, its lifecycle enum, the event envelope, and
    // the integration contract get user-owned facts — the Task and
    // TASK_STATE through confirmations, the envelope and the contract
    // through explicit binds (explicit/confirmed paths both used).
    // The tasks module domain surface confirms as a whole so the
    // module can promote.
    // -------------------------------------------------------------
    for symbol in [
        "taskhub_tasks.task",
        "taskhub_tasks.task_id",
        "taskhub_tasks.task_state",
        "taskhub_tasks.new_task_input",
        "taskhub_tasks.create_task",
        "taskhub_tasks.list_tasks",
        "taskhub_tasks.transition_task",
    ] {
        let confirmed = sandbox.stage(
            &mut transcript,
            &["--json", "observe", "confirm", symbol, "--project", "proj"],
        );
        assert_eq!(confirmed["binding"], "confirmed", "{symbol}");
    }
    let envelope = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "bind",
            "taskhub_events.task_event",
            "--path",
            "packages/events/src/index.ts",
            "--line",
            "11",
            "--project",
            "proj",
        ],
    );
    assert_eq!(envelope["binding"], "explicit");
    assert_eq!(envelope["state"], "current");
    let contract = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "bind",
            "taskhub_integrations.sync_payload",
            "--path",
            "packages/integrations/src/index.ts",
            "--line",
            "22",
            "--project",
            "proj",
        ],
    );
    assert_eq!(contract["binding"], "explicit");

    // Criterion: Task and event envelope are available through inspect.
    let task_card = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "inspect",
            "taskhub_tasks.task",
            "--project",
            "proj",
        ],
    );
    assert_eq!(task_card["binding"], "confirmed");
    assert_eq!(task_card["completeness"], "complete");
    assert_eq!(task_card["kind"], "entity");
    assert_eq!(task_card["location"]["path"], "packages/tasks/src/index.ts");
    let state_card = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "inspect",
            "taskhub_tasks.task_state",
            "--project",
            "proj",
        ],
    );
    assert_eq!(state_card["binding"], "confirmed");
    assert_eq!(
        state_card["evidence"]["values"].as_array().map(Vec::len),
        Some(3)
    );
    let envelope_card = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "inspect",
            "taskhub_events.task_event",
            "--project",
            "proj",
        ],
    );
    assert_eq!(envelope_card["binding"], "explicit");
    let contract_card = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "inspect",
            "taskhub_integrations.sync_payload",
            "--project",
            "proj",
        ],
    );
    assert_eq!(contract_card["binding"], "explicit");

    // -------------------------------------------------------------
    // Criterion: the observed impact reports API/worker/integration
    // links and states its own incompleteness.
    // -------------------------------------------------------------
    let task_impact = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "impact",
            "taskhub_tasks.task",
            "--project",
            "proj",
        ],
    );
    let dependents: Vec<String> = task_impact["dependents"]
        .as_array()
        .expect("dependents")
        .iter()
        .map(|row| row["from"].as_str().expect("dependent id").to_owned())
        .collect();
    assert!(
        dependents.iter().any(|id| id.starts_with("taskhub_api.")),
        "api surface links missing: {dependents:?}"
    );
    assert!(
        dependents.iter().any(|id| id.starts_with("taskhub_sync.")),
        "worker surface links missing: {dependents:?}"
    );
    assert_eq!(task_impact["completeness"], "incomplete");
    let contract_impact = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "impact",
            "taskhub_integrations.sync_payload",
            "--project",
            "proj",
        ],
    );
    let integration_links: Vec<String> = contract_impact["dependents"]
        .as_array()
        .expect("dependents")
        .iter()
        .map(|row| row["from"].as_str().expect("dependent id").to_owned())
        .collect();
    assert!(
        integration_links
            .iter()
            .any(|id| id.starts_with("taskhub_sync.")),
        "integration links missing: {integration_links:?}"
    );

    // The staleness gate stays green over the freshly recorded facts.
    let check = sandbox.stage(
        &mut transcript,
        &["--json", "observe", "check", "--project", "proj"],
    );
    assert_eq!(check["status"], "valid");

    // -------------------------------------------------------------
    // Criterion: the canonical impact of the promoted Task degrades
    // its completeness over the unpromoted neighbors
    // (`observed.incomplete-graph`).
    // -------------------------------------------------------------
    let plan = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "promote",
            "--module",
            "taskhub_tasks",
            "--dry-run",
            "--project",
            "proj",
        ],
    );
    let plan_id = plan["plan"].as_str().expect("promotion plan id").to_owned();
    let applied = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "promote",
            "--module",
            "taskhub_tasks",
            "--confirm",
            &plan_id,
            "--project",
            "proj",
        ],
    );
    assert_eq!(applied["symbols"].as_array().map(Vec::len), Some(7));
    let load = sandbox.stage(&mut transcript, &["--json", "load", "--project", "proj"]);
    assert_eq!(load["status"], "valid");
    let canonical_impact = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "impact",
            "taskhub_tasks.task",
            "--project",
            "proj",
        ],
    );
    assert!(canonical_impact
        .to_string()
        .contains("observed.incomplete-graph"));

    // -------------------------------------------------------------
    // Criterion: the capsule for the Task symbol is materially smaller
    // than the consumer source corpus and carries the bound contracts.
    // -------------------------------------------------------------
    let capsule = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "context",
            "taskhub_tasks.task",
            "--budget",
            "4000",
            "--project",
            "proj",
        ],
    );
    assert_eq!(capsule["status"], "valid");
    let capsule_text = serde_json::to_string(&capsule).expect("capsule bytes");
    assert!(
        capsule_text.contains("taskhub_tasks.task"),
        "the bound contract is in the capsule"
    );
    let corpus: u64 = snapshot(&sandbox.root.join("proj")).values().sum();
    assert!(
        capsule_text.len() as u64 * 4 < corpus,
        "capsule {} bytes must be materially smaller than the {}-byte corpus",
        capsule_text.len(),
        corpus
    );

    // -------------------------------------------------------------
    // Criterion: the native gate plan over the workspace carries the
    // confirmed build/typecheck/test command surface, and the
    // production `native run` answers with the typed refusal — the
    // plan is never executed.
    // -------------------------------------------------------------
    let committed_plan: Value = serde_json::from_str(
        &std::fs::read_to_string(sandbox.root.join("plans/plan.golden.json"))
            .expect("committed plan bytes"),
    )
    .expect("committed plan json");
    let gates: Vec<&str> = committed_plan["commands"]
        .as_array()
        .expect("plan commands")
        .iter()
        .map(|command| command["gate"].as_str().expect("gate kind"))
        .collect();
    assert!(gates.contains(&"build"), "{gates:?}");
    assert!(gates.contains(&"typecheck"), "{gates:?}");
    assert!(gates.contains(&"test"), "{gates:?}");
    let run = sandbox.stage(
        &mut transcript,
        &["--json", "native", "run", "plans/plan.golden.json"],
    );
    assert_eq!(run["kind"], "native-run-result");
    assert_eq!(run["outcome"], "unsupported");
    assert_eq!(
        run["reason_codes"],
        serde_json::json!(["fixture-runner-not-shipped"])
    );
    assert_eq!(run["plan_digest"], committed_plan["plan_digest"]);
    assert_eq!(
        run["commands"].as_array().map(Vec::len),
        Some(0),
        "the production surface never launches a command"
    );

    // -------------------------------------------------------------
    // Criterion: the baseline metrics are saved, and an identical run
    // writes byte-identical documents.
    // -------------------------------------------------------------
    let baseline_one = sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "baseline",
            "--native-plan",
            "plans/plan.golden.json",
            "--project",
            "proj",
        ],
    );
    assert_eq!(baseline_one["status"], "valid");
    assert_eq!(baseline_one["counts"]["symbols"], 35);
    assert_eq!(baseline_one["counts"]["explicit"], 2);
    assert_eq!(baseline_one["counts"]["confirmed"], 7);
    assert_eq!(baseline_one["counts"]["inferred"], 26);
    assert_eq!(baseline_one["counts"]["promoted"], 7);
    assert_eq!(
        baseline_one["native_plan"]["plan_digest"],
        committed_plan["plan_digest"]
    );
    let baseline_path = sandbox
        .root
        .join("proj/.lekalo/import/observed/baseline.json");
    let first_bytes = std::fs::read(&baseline_path).expect("baseline bytes");
    sandbox.stage(
        &mut transcript,
        &[
            "--json",
            "observe",
            "baseline",
            "--native-plan",
            "plans/plan.golden.json",
            "--project",
            "proj",
        ],
    );
    let second_bytes = std::fs::read(&baseline_path).expect("baseline bytes");
    assert_eq!(first_bytes, second_bytes, "the baseline is deterministic");
    let baseline: Value = serde_json::from_slice(&first_bytes).expect("baseline json");
    assert_eq!(
        baseline["schema_version"],
        "lekalo/observed-baseline/v0.2.16"
    );
    assert_eq!(baseline["project"], "taskhub");

    // -------------------------------------------------------------
    // Criterion: existing code/files are not rewritten, and only the
    // Lekalo homes grew. Every non-Lekalo path keeps its exact byte
    // length, and `.env` keeps its byte-exact fingerprint.
    // -------------------------------------------------------------
    let after = snapshot(&sandbox.root.join("proj"));
    for (path, length) in &before {
        assert_eq!(
            after.get(path),
            Some(length),
            "{path} changed outside the Lekalo homes"
        );
    }
    for path in after.keys() {
        if before.contains_key(path) {
            continue;
        }
        assert!(
            path.starts_with("lekalo/") || path.starts_with(".lekalo/"),
            "unexpected new file outside the Lekalo homes: {path}"
        );
    }
    let env_after = {
        let bytes = std::fs::read(sandbox.root.join("proj/.env")).expect(".env bytes");
        lekalo_core::digest::sha256_hex(&bytes)
    };
    assert_eq!(env_before, env_after, ".env is never touched");

    // -------------------------------------------------------------
    // Criterion: the consumer repository identity floor — no emitted
    // byte carries the fake secrets, a `.env` reference, or the host's
    // absolute path. (`taskhub` itself is the fixture's fictional slug
    // and is allowed on the wire; the secret floor is what policy
    // guards.)
    // -------------------------------------------------------------
    for secret in ENV_SECRETS {
        assert!(!transcript.contains(secret), "a secret leaked: {secret}");
        let index_bytes =
            std::fs::read_to_string(sandbox.root.join("proj/.lekalo/import/observed/index.json"))
                .expect("index bytes");
        assert!(
            !index_bytes.contains(secret),
            "a secret leaked into the index"
        );
        assert!(
            !String::from_utf8_lossy(&first_bytes).contains(secret),
            "a secret leaked into the baseline"
        );
    }
    assert!(
        !transcript.contains("\".env\""),
        "the sensitive-config file is never referenced: {transcript}"
    );
    let index_bytes =
        std::fs::read_to_string(sandbox.root.join("proj/.lekalo/import/observed/index.json"))
            .expect("index bytes");
    assert!(
        !index_bytes.contains("\".env\""),
        "the sensitive-config file is never referenced"
    );
    let host_root = sandbox.root.to_string_lossy().to_string();
    let host_root_escaped = host_root.replace('\\', "\\\\");
    assert!(
        !transcript.contains(host_root.as_str()),
        "the absolute host path leaked"
    );
    assert!(
        !transcript.contains(host_root_escaped.as_str()),
        "the escaped absolute host path leaked"
    );
    assert!(!index_bytes.contains(host_root.as_str()));
    assert!(!String::from_utf8_lossy(&first_bytes).contains(host_root.as_str()));
}
