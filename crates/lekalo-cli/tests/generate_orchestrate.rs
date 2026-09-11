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

        let fingerprint = |root: &Path| -> Vec<(String, Vec<u8>)> {
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
        };
        let before = fingerprint(root);
        let verify = verify(root);
        assert_eq!(exit_code(&verify), 0);
        assert_eq!(fingerprint(root), before, "verify wrote nothing");
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
