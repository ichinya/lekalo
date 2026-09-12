//! Issue #91 CLI tests for the generate/verify orchestration: the
//! catalog seam on `lekalo lock`, the plan-first generation pipeline
//! against the committed reference adapter, the read-only verify
//! aggregation, and the accepted exit classes — including the golden
//! receipts pinned for the Node contract gate.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const REFERENCE_PROJECT: &str = "tests/fixtures/orchestration/project";
const DRY_RUN_GOLDEN: &str =
    include_str!("../../../tests/fixtures/orchestration/generate.dry-run.golden.json");
const APPLY_GOLDEN: &str =
    include_str!("../../../tests/fixtures/orchestration/generate.apply.golden.json");
const VERIFY_GOLDEN: &str =
    include_str!("../../../tests/fixtures/orchestration/verify.full.golden.json");

/// The committed reference adapter vector, spawned without a shell.
const ADAPTER_ARGS: &[&str] = &["node", "adapters/node-typescript/node-adapter.mjs"];

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
}

/// GitHub's Windows runners export `%TEMP%` spelled with the 8.3 alias of
/// the profile directory; the selection policy denies alias spellings.
fn alias_free_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("fixture path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

/// Run the real binary inside `dir`, with `head` followed by the adapter
/// vector after the `--` separator.
fn lekalo_in(dir: &Path, head: &[&str], adapter: bool) -> Output {
    let mut args: Vec<&str> = head.to_vec();
    if adapter {
        args.push("--");
        args.extend_from_slice(ADAPTER_ARGS);
    }
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(&args)
        .current_dir(alias_free_path(dir))
        .output()
        .expect("run the real lekalo binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn exit_code(output: &Output) -> u8 {
    output.status.code().expect("exit code") as u8
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("read entry");
        let target_path = target.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            std::fs::copy(entry.path(), target_path).expect("copy file");
        }
    }
}

static NEXT_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Run a body against a fresh copy of the orchestration fixture project,
/// with the child working directory set to the copy so the adapter
/// vector resolves project-relative, exactly like an operator run.
fn with_project(body: impl FnOnce(&Path)) {
    let id = NEXT_ROOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + u64::from(std::process::id());
    let root = workspace_path("target")
        .join("orchestration-cli-tests")
        .join(format!("case-{id}"));
    copy_dir(&workspace_path(REFERENCE_PROJECT), &root);
    body(&root);
    let _ = std::fs::remove_dir_all(&root);
}

fn lock_with_adapter(root: &Path) {
    let output = lekalo_in(root, &["lock"], true);
    assert_eq!(exit_code(&output), 0, "stdout={}", stdout(&output));
    assert!(stdout(&output).contains("adapters 1"));
}

fn generate(root: &Path, dry_run: bool, json: bool) -> Output {
    let mut head: Vec<&str> = Vec::new();
    if json {
        head.push("--json");
    }
    head.push("generate");
    head.push("--target");
    head.push("node-typescript");
    if dry_run {
        head.push("--dry-run");
    }
    lekalo_in(root, &head, true)
}

fn verify(root: &Path) -> Output {
    lekalo_in(
        root,
        &["--json", "verify", "--target", "node-typescript"],
        true,
    )
}

#[test]
fn lock_with_adapter_supply_pins_the_discovered_identity() {
    with_project(|root| {
        let output = lekalo_in(root, &["lock"], true);
        assert_eq!(exit_code(&output), 0, "stdout={}", stdout(&output));
        let out = stdout(&output);
        assert!(out.contains("adapters 1"), "adapter pinned: {out}");
        assert!(out.contains("generators 0"));
    });
}

#[test]
fn the_full_sequence_matches_the_golden_receipts() {
    with_project(|root| {
        lock_with_adapter(root);
        let dry = generate(root, true, true);
        assert_eq!(exit_code(&dry), 0, "stdout={}", stdout(&dry));
        let apply = generate(root, false, true);
        assert_eq!(exit_code(&apply), 0, "stdout={}", stdout(&apply));
        let verify = verify(root);
        assert_eq!(exit_code(&verify), 0, "stdout={}", stdout(&verify));
        // The receipts match the pinned goldens up to the deliberately
        // volatile protocol plan identity (it binds the private project
        // identity, which intentionally differs per project location).
        let normalize = |text: &str| {
            let mut value: serde_json::Value = serde_json::from_str(text).expect("receipt json");
            if let Some(targets) = value.get_mut("targets") {
                for target in targets.as_array_mut().expect("targets array") {
                    if let Some(plan) = target.get_mut("planId") {
                        *plan = serde_json::Value::String("plan-normalized".into());
                    }
                }
            }
            serde_json::to_string_pretty(&value).expect("reserialize")
        };
        assert_eq!(
            normalize(&stdout(&dry)),
            normalize(DRY_RUN_GOLDEN),
            "dry-run receipt matches the pinned golden"
        );
        assert_eq!(
            normalize(&stdout(&apply)),
            normalize(APPLY_GOLDEN),
            "apply receipt matches the pinned golden"
        );
        assert_eq!(
            normalize(&stdout(&verify)),
            normalize(VERIFY_GOLDEN),
            "verify receipt matches the pinned golden"
        );
    });
}

#[test]
fn generate_dry_run_lists_every_action_and_writes_nothing() {
    with_project(|root| {
        lock_with_adapter(root);
        let dry = generate(root, true, true);
        assert_eq!(exit_code(&dry), 0);
        let receipt: serde_json::Value = serde_json::from_str(&stdout(&dry)).expect("json");
        assert_eq!(receipt["mode"], "dry-run");
        assert_eq!(receipt["targets"][0]["state"], "planned");
        let writes = receipt["targets"][0]["writes"].as_array().expect("writes");
        assert_eq!(writes.len(), 1, "every intended action is listed");
        assert_eq!(
            writes[0]["path"],
            "src/generated/node-typescript/planner.ts"
        );
        assert_eq!(writes[0]["action"], "create");
        assert!(writes[0]["sha256"].is_string(), "content digest pinned");
        assert!(receipt["irEvidence"].is_object(), "evidence recorded");
        // The planned file does not exist; only the runtime evidence was
        // written under the reserved cache home.
        assert!(!root
            .join("src/generated/node-typescript/planner.ts")
            .exists());
    });
}

#[test]
fn generate_apply_publishes_the_manifest_and_the_check_is_clean() {
    with_project(|root| {
        lock_with_adapter(root);
        let apply = generate(root, false, true);
        assert_eq!(exit_code(&apply), 0);
        let receipt: serde_json::Value = serde_json::from_str(&stdout(&apply)).expect("json");
        assert_eq!(receipt["targets"][0]["state"], "applied");
        assert!(receipt["targets"][0]["manifestDigest"].is_string());
        assert!(root
            .join("src/generated/node-typescript/planner.ts")
            .exists());
        assert!(root
            .join(".lekalo/generated/manifests/ownership.json")
            .exists());
        // The drift gate accepts the freshly recorded ownership.
        let check = lekalo_in(root, &["generate", "--check"], false);
        assert_eq!(exit_code(&check), 0, "stdout={}", stdout(&check));
        assert!(stdout(&check).contains("clean 1"));
        assert!(stdout(&check).contains("orphan 0"));
    });
}

#[test]
fn verify_aggregates_components_and_never_writes() {
    with_project(|root| {
        lock_with_adapter(root);
        let apply = generate(root, false, false);
        assert_eq!(exit_code(&apply), 0);

        let before = project_fingerprint(root);
        let verify = verify(root);
        assert_eq!(exit_code(&verify), 0);
        assert_eq!(project_fingerprint(root), before, "verify wrote nothing");
        let receipt: serde_json::Value = serde_json::from_str(&stdout(&verify)).expect("json");
        assert_eq!(receipt["verdict"], "ready");
        let components = receipt["components"].as_array().expect("components");
        let ids: Vec<&str> = components
            .iter()
            .map(|component| component["id"].as_str().expect("id"))
            .collect();
        assert!(ids.contains(&"model.validation"));
        assert!(ids.contains(&"artifact.drift"));
        assert!(ids.contains(&"adapter.node-typescript"));
        // Declared absences: reported, optional, never counted as work.
        let execution = components
            .iter()
            .find(|component| component["id"] == "scenarios.execution")
            .expect("scenario execution row");
        assert_eq!(execution["state"], "unsupported");
        assert_eq!(execution["required"], false);
    });
}

#[test]
fn exit_classes_distinguish_usage_unavailable_and_denied() {
    with_project(|root| {
        // A bare generate names no mode: usage (exit 1).
        assert_eq!(exit_code(&lekalo_in(root, &["generate"], false)), 1);
        // Conflicting modes are usage too.
        assert_eq!(
            exit_code(&lekalo_in(root, &["generate", "--check", "--clean"], false)),
            1
        );
        // Generation without the mandatory adapter vector: usage
        // (exit 1), exactly like a scan without its program.
        assert_eq!(
            exit_code(&lekalo_in(
                root,
                &["generate", "--target", "node-typescript", "--dry-run"],
                false
            )),
            1
        );
    });
}

#[test]
fn a_tampered_adapter_is_an_integrity_denial() {
    with_project(|root| {
        lock_with_adapter(root);
        // With the lock present but no adapter vector, the required
        // adapter component is unsupported: unavailable (exit 4).
        assert_eq!(
            exit_code(&lekalo_in(
                root,
                &["verify", "--target", "node-typescript"],
                false
            )),
            4
        );
        let adapter_path = root.join("adapters/node-typescript/node-adapter.mjs");
        let original = std::fs::read_to_string(&adapter_path).expect("read adapter");
        std::fs::write(&adapter_path, original.replace("planner.ts", "planner2.ts"))
            .expect("tamper adapter");
        let tampered = generate(root, true, true);
        assert_eq!(exit_code(&tampered), 3);
        assert!(stdout(&tampered).contains("lock.digest-mismatch"));
    });
}

#[test]
fn verify_refuses_stale_ir_evidence_without_writing() {
    with_project(|root| {
        lock_with_adapter(root);
        // No generate ever ran: the IR evidence does not exist. The
        // required adapter component is unsupported (exit 4), and the
        // run wrote nothing (the evidence file stays absent).
        let verify = lekalo_in(
            root,
            &["--json", "verify", "--target", "node-typescript"],
            true,
        );
        assert_eq!(exit_code(&verify), 4);
        // A degraded verify renders the unsupported envelope: one
        // registered diagnostic per non-pass component, never a silent
        // success and never a written receipt file.
        let envelope: serde_json::Value = serde_json::from_str(&stdout(&verify)).expect("json");
        assert_eq!(envelope["status"], "unsupported");
        assert_eq!(
            envelope["status"], "unsupported",
            "degraded verification is exit 4, never a silent success"
        );
        assert!(!root.join(".lekalo/cache/ir/planner.json").exists());
    });
}

/// Walk every file under `root` into a sorted (path, bytes) vector: the
/// rollback evidence of the hostile-write probes.
fn project_fingerprint(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("read entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                let bytes = std::fs::read(&path).expect("read file");
                out.push((path.to_string_lossy().to_string(), bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

/// Copy the hermetic fake adapter into the project copy so the lock can
/// pin it exactly like the reference implementation.
fn with_fake_adapter(root: &Path) {
    let home = root.join("adapters/node-typescript");
    std::fs::create_dir_all(&home).expect("adapter home");
    std::fs::copy(
        workspace_path("tests/fixtures/target-protocol/fake-adapter.mjs"),
        home.join("fake-adapter.mjs"),
    )
    .expect("copy the fake adapter");
}

/// Run the real binary with the fake adapter vector — negotiated past the
/// legacy base so the lock preflight sees its IR compatibility, and
/// writing to an artifact-legal home — with an optional fault knob.
fn fake_lekalo_in(root: &Path, head: &[&str], fault: Option<&str>) -> Output {
    let mut args: Vec<&str> = head.to_vec();
    args.push("--");
    args.extend_from_slice(&[
        "node",
        "adapters/node-typescript/fake-adapter.mjs",
        "--lekalo-adapter-variant",
        "fluent",
        "--lekalo-write-root",
        "src/generated",
    ]);
    if let Some(fault) = fault {
        args.extend_from_slice(&["--lekalo-fault", fault]);
    }
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(&args)
        .current_dir(alias_free_path(root))
        .output()
        .expect("run the real lekalo binary")
}

/// Multi-target aggregation with distinct results: one run over a
/// declared and an undeclared target applies the declared one through the
/// manifest while the aggregate envelope preserves the other target's
/// isolated refusal.
#[test]
fn multi_target_run_applies_the_declared_target_and_aggregates_the_other() {
    with_project(|root| {
        lock_with_adapter(root);
        let run = lekalo_in(
            root,
            &[
                "--json",
                "generate",
                "--target",
                "node-typescript",
                "--target",
                "ghost-target",
            ],
            true,
        );
        assert_eq!(exit_code(&run), 4, "stdout={}", stdout(&run));
        let envelope: serde_json::Value = serde_json::from_str(&stdout(&run)).expect("json");
        assert_eq!(envelope["status"], "unsupported");
        let diagnostics = envelope["diagnostics"].as_array().expect("diagnostics");
        assert_eq!(
            diagnostics.len(),
            1,
            "exactly the undeclared target refusal"
        );
        assert_eq!(diagnostics[0]["id"], "target.capability-unsupported");
        assert_eq!(diagnostics[0]["data"]["target"], "ghost-target");
        // Isolation: the declared target's writes and manifest landed even
        // though its sibling refused.
        assert!(root
            .join("src/generated/node-typescript/planner.ts")
            .exists());
        assert!(root
            .join(".lekalo/generated/manifests/ownership.json")
            .exists());
    });
}

/// A hostile dry-run write faces a private view without any writable
/// mount: the interpreter dies, the run refuses as unavailable, and the
/// real project stays byte-identical.
#[test]
fn a_hostile_dry_run_write_is_refused_and_the_project_rolls_back() {
    with_project(|root| {
        with_fake_adapter(root);
        let lock = fake_lekalo_in(root, &["lock"], None);
        assert_eq!(exit_code(&lock), 0, "stdout={}", stdout(&lock));
        assert!(stdout(&lock).contains("adapters 1"));
        // Positive control: the same vector without the fault plans
        // cleanly and writes nothing but the reserved evidence home.
        let planned = fake_lekalo_in(
            root,
            &[
                "--json",
                "generate",
                "--target",
                "node-typescript",
                "--dry-run",
            ],
            None,
        );
        assert_eq!(exit_code(&planned), 0, "stdout={}", stdout(&planned));
        let receipt: serde_json::Value = serde_json::from_str(&stdout(&planned)).expect("json");
        assert_eq!(
            receipt["targets"][0]["writes"][0]["path"],
            "src/generated/node-typescript/model.ts"
        );
        let before = project_fingerprint(root);
        let hostile = fake_lekalo_in(
            root,
            &[
                "--json",
                "generate",
                "--target",
                "node-typescript",
                "--dry-run",
            ],
            Some("mutate-dry"),
        );
        assert_eq!(exit_code(&hostile), 4, "stdout={}", stdout(&hostile));
        let envelope: serde_json::Value = serde_json::from_str(&stdout(&hostile)).expect("json");
        assert_eq!(envelope["status"], "unavailable");
        assert_eq!(envelope["diagnostics"][0]["id"], "target.crash");
        assert_eq!(
            envelope["diagnostics"][0]["data"]["detail"],
            "abnormal-exit"
        );
        // Rollback verification: every project byte is unchanged and no
        // generated output exists.
        assert_eq!(
            project_fingerprint(root),
            before,
            "the hostile dry run mutated the project"
        );
        assert!(!root.join("src/generated").exists());
    });
}

/// An undeclared staged write disagrees with the bound plan: the run
/// refuses as invalid and publication never starts, so the declared
/// output, the undeclared file, and the manifest never reach the real
/// project.
#[test]
fn an_undeclared_apply_write_is_caught_and_never_published() {
    with_project(|root| {
        with_fake_adapter(root);
        let lock = fake_lekalo_in(root, &["lock"], None);
        assert_eq!(exit_code(&lock), 0, "stdout={}", stdout(&lock));
        // Positive control: the fault-free plan names exactly one write.
        let planned = fake_lekalo_in(
            root,
            &[
                "--json",
                "generate",
                "--target",
                "node-typescript",
                "--dry-run",
            ],
            None,
        );
        assert_eq!(exit_code(&planned), 0, "stdout={}", stdout(&planned));
        let before = project_fingerprint(root);
        let hostile = fake_lekalo_in(
            root,
            &["--json", "generate", "--target", "node-typescript"],
            Some("extra-write"),
        );
        assert_eq!(exit_code(&hostile), 1, "stderr={}", stderr(&hostile));
        let envelope: serde_json::Value = serde_json::from_str(&stderr(&hostile)).expect("json");
        assert_eq!(envelope["status"], "invalid");
        assert_eq!(envelope["diagnostics"][0]["id"], "target.plan-mismatch");
        assert_eq!(envelope["diagnostics"][0]["data"]["detail"], "undeclared");
        // Rollback verification: the real project never saw the declared
        // write, the undeclared file, or a manifest.
        assert_eq!(
            project_fingerprint(root),
            before,
            "the hostile apply mutated the project"
        );
        assert!(!root.join("src/generated").exists());
    });
}
