//! Issue #113 CLI tests for the `module api` family and the `validate
//! --module-api` seam: the status-owned streams and exit classes, the
//! certified export, the stale lock denial, the exact diff partition, the
//! public-only context bounds, and the hostile-input refusals.
//!
//! Every child runs inside an alias-free scratch copy with relative
//! selectors only (the GitHub Windows runners export `%TEMP%` with the
//! 8.3 profile alias and the selection policy denies alias and absolute
//! spellings), and every mutation happens inside the scratch.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const WORKSPACE_FIXTURES: &str = "tests/fixtures/module-api";

fn alias_free_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = canonical.to_string_lossy().into_owned();
    // The selection policy denies `\\?\` spellings too.
    PathBuf::from(text.trim_start_matches(r"\\?\").to_owned())
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == ".lekalo" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The workspace fixture family, alias-free.
fn workspace_fixture(rel: &str) -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(WORKSPACE_FIXTURES)
            .join(rel),
    )
}

/// One fixture project copied to the root of an alias-free scratch: the
/// command surface is `--project .` with relative attachment paths.
fn scratch_project(rel: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    copy_tree(&workspace_fixture(rel), &alias_free_path(temp.path()));
    temp
}

/// The whole module-api family copied under the scratch root.
fn scratch_family() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    copy_tree(&workspace_fixture(""), &alias_free_path(temp.path()));
    temp
}

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .arg("--json")
        .arg("--no-cache")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("lekalo runs")
}

fn envelope(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("valid envelope on stdout")
}

/// Parse whichever stream owns the run: invalid/unsupported-version ride
/// stderr; every other status rides stdout.
fn failure_envelope(output: &Output) -> Value {
    serde_json::from_slice(&output.stderr).expect("valid envelope on stderr")
}

fn codes(output: &Output) -> Vec<String> {
    let document = if output
        .status
        .code()
        .is_some_and(|code| code == 1 || code == 5)
    {
        failure_envelope(output)
    } else {
        envelope(output)
    };
    document["diagnostics"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["code"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn check_reference_is_advisory_valid_and_deterministic() {
    let temp = scratch_project("reference/tasks");
    let dir = alias_free_path(temp.path());
    let args = [
        "module",
        "api",
        "check",
        "--project",
        ".",
        "--api",
        "module-api.json",
    ];
    let first = lekalo_in(&dir, &args);
    assert_eq!(first.status.code(), Some(0));
    let document = envelope(&first);
    assert_eq!(document["status"], "valid");
    assert_eq!(document["moduleApiReport"]["assessment"], "pass");
    assert_eq!(document["moduleApiReport"]["complete"], true);
    assert_eq!(
        document["moduleApiReport"]["pins"]["registryVersion"],
        "0.8.0"
    );
    // Byte-deterministic.
    let second = lekalo_in(&dir, &args);
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn visibility_strict_invalidates_and_advisory_stays_valid() {
    let temp = scratch_project("visibility");
    let dir = alias_free_path(temp.path());
    let strict = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "module-api.json",
            "--strict",
        ],
    );
    assert_eq!(strict.status.code(), Some(1));
    let codes = codes(&strict);
    assert!(codes.contains(&"LEK-MODULE-003".to_owned()));
    assert!(codes.contains(&"LEK-MODULE-005".to_owned()));
    assert!(strict.stdout.is_empty(), "invalid rides stderr");
    // Advisory: exit 0 with the full evidence retained.
    let advisory = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "module-api.json",
        ],
    );
    assert_eq!(advisory.status.code(), Some(0));
    let document = envelope(&advisory);
    assert_eq!(document["status"], "valid");
    assert_eq!(document["moduleApiReport"]["assessment"], "fail");
}

#[test]
fn export_is_certified_and_the_digest_matches_the_committed_golden() {
    let golden: Value = serde_json::from_slice(
        &fs::read(workspace_fixture("standalone/golden-bundle.json")).unwrap(),
    )
    .unwrap();
    // The certified refusal: the portability closure never exports.
    let portability = scratch_project("portability");
    let portability_dir = alias_free_path(portability.path());
    let refused = lekalo_in(
        &portability_dir,
        &[
            "module",
            "api",
            "export",
            "tasks",
            "--project",
            ".",
            "--api",
            "module-api.json",
        ],
    );
    assert_eq!(refused.status.code(), Some(1));
    assert!(codes(&refused).contains(&"LEK-MODULE-006".to_owned()));
    // The reference bundle is certified and byte-stable across runs.
    let reference = scratch_project("reference/tasks");
    let reference_dir = alias_free_path(reference.path());
    let args = [
        "module",
        "api",
        "export",
        "tasks",
        "--project",
        ".",
        "--api",
        "module-api.json",
    ];
    let first = lekalo_in(&reference_dir, &args);
    assert_eq!(first.status.code(), Some(0));
    let document = envelope(&first);
    assert_eq!(document["status"], "valid");
    assert_eq!(document["modulePublicContract"]["digest"], golden["digest"]);
    assert_eq!(document["modulePublicContract"]["apiVersion"], "1.2.0");
    let second = lekalo_in(&reference_dir, &args);
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn lock_candidates_print_and_stale_locks_deny() {
    let lock = scratch_project("lock");
    let dir = alias_free_path(lock.path());
    // Candidate on stdout only: no lock file is ever created.
    let candidate = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "lock",
            "--project",
            ".",
            "--api",
            "module-api.json",
            "--module",
            "tasks",
            "--consumer",
            "planner",
        ],
    );
    assert_eq!(candidate.status.code(), Some(0));
    let document = envelope(&candidate);
    assert_eq!(document["moduleApiLock"]["consumer"], "planner");
    assert!(
        !dir.join("module-api.lock").exists(),
        "the candidate never writes a file"
    );
    // Fresh check passes.
    let fresh = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "lock",
            "--project",
            ".",
            "--api",
            "module-api.json",
            "--module",
            "tasks",
            "--consumer",
            "planner",
            "--check",
            "--lock",
            "golden-lock.json",
        ],
    );
    assert_eq!(fresh.status.code(), Some(0));
    // Stale: a changed apiVersion changes the public digest.
    let stale: Value =
        serde_json::from_slice(&fs::read(dir.join("module-api.json")).unwrap()).unwrap();
    let mut stale = stale;
    for module in stale["modules"].as_array_mut().unwrap().iter_mut() {
        if module["moduleId"] == "tasks" {
            module["apiVersion"] = "1.3.0".into();
        }
    }
    fs::write(
        dir.join("stale-attachment.json"),
        serde_json::to_vec(&stale).unwrap(),
    )
    .unwrap();
    let denied = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "lock",
            "--project",
            ".",
            "--api",
            "stale-attachment.json",
            "--module",
            "tasks",
            "--consumer",
            "planner",
            "--check",
            "--lock",
            "golden-lock.json",
        ],
    );
    assert_eq!(denied.status.code(), Some(3));
    let codes = codes(&denied);
    assert!(codes.contains(&"LEK-MODULE-013".to_owned()));
    assert!(codes.contains(&"LEK-MODULE-014".to_owned()));
    assert!(
        envelope(&denied)["payload"]["moduleApiLock"].is_object(),
        "denial retains the evidence report"
    );
    // --lock without --check is usage-invalid.
    let misuse = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "lock",
            "--project",
            ".",
            "--api",
            "module-api.json",
            "--module",
            "tasks",
            "--consumer",
            "planner",
            "--lock",
            "golden-lock.json",
        ],
    );
    assert_eq!(misuse.status.code(), Some(1));
}

#[test]
fn diff_identifies_exact_consumers_and_strict_policy_denies() {
    let family = scratch_family();
    let dir = alias_free_path(family.path()).join("diff");
    let args = [
        "module",
        "api",
        "diff",
        "golden-bundle-before.json",
        "golden-bundle-after.json",
        "--consumers-before",
        "golden-consumers-before.json",
        "--consumers-after",
        "golden-consumers-after.json",
    ];
    let advisory = lekalo_in(&dir, &args);
    assert_eq!(advisory.status.code(), Some(0));
    let document = envelope(&advisory);
    let report = &document["moduleApiDiff"];
    // The planner's declared range accepts the candidate resolution, so
    // the verdict is the honest source break; strict denies through the
    // missing major bump, never through a phantom consumer policy
    // incompatibility (findings 5/9/10).
    assert_eq!(report["verdict"], "breaking");
    let removed = report["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["reason"] == "field-removed")
        .unwrap();
    assert_eq!(removed["export"], "tasks.task_view");
    assert_eq!(removed["member"], "state");
    assert_eq!(removed["class"], "source-breaking");
    let consumers = report["consumers"].as_array().unwrap();
    let planner = consumers
        .iter()
        .find(|c| c["consumer"] == "planner")
        .unwrap();
    assert_eq!(planner["status"], "affected");
    assert_eq!(planner["reason"], "policy-accepted");
    let provider = consumers
        .iter()
        .find(|c| c["consumer"] == "provider")
        .unwrap();
    assert_eq!(provider["status"], "unaffected");
    assert_eq!(report["coverage"]["known"], 1);
    // Strict denies the missing major bump (version-policy violation) with
    // the terminal registry error; the advisory report rides the evidence
    // payload.
    let strict = lekalo_in(&dir, &append(&args, "--strict"));
    assert_eq!(strict.status.code(), Some(3));
    assert!(codes(&strict).contains(&"LEK-MODULE-014".to_owned()));
    let strict_document = envelope(&strict);
    assert_eq!(strict_document["status"], "denied");
    assert!(strict_document["payload"]["moduleApiDiff"].is_object());
}

fn append<'a>(args: &[&'a str], extra: &'a str) -> Vec<&'a str> {
    let mut extended = args.to_vec();
    extended.push(extra);
    extended
}

#[test]
fn context_capsules_need_only_the_public_bundle() {
    let standalone = scratch_project("standalone");
    let dir = alias_free_path(standalone.path());
    let args = [
        "module",
        "api",
        "context",
        "golden-bundle.json",
        "--consumer",
        "planner",
        "--uses",
        "tasks.task_view,tasks.get_task",
        "--budget",
        "4000",
    ];
    let capsule = lekalo_in(&dir, &args);
    assert_eq!(capsule.status.code(), Some(0));
    let document = envelope(&capsule);
    assert_eq!(document["moduleConsumerContext"]["fits"], true);
    assert_eq!(
        document["moduleConsumerContext"]["estimator"]["recipe"],
        "public-facts-chars4/1"
    );
    assert!(document["moduleConsumerContext"]["exports"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["export"] == "tasks.task_view"));
    // A tiny budget keeps the required facts and never claims fit.
    let tiny = lekalo_in(&dir, &append(&args[..args.len() - 1], "16"));
    assert_eq!(tiny.status.code(), Some(0));
    let tiny_document = envelope(&tiny);
    assert_eq!(tiny_document["moduleConsumerContext"]["fits"], false);
    assert_eq!(
        tiny_document["moduleConsumerContext"]["gaps"],
        serde_json::json!(["budget-exceeded"])
    );
    // Exact selectors only: no short-name guessing, no duplicates.
    let refused = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "context",
            "golden-bundle.json",
            "--consumer",
            "planner",
            "--uses",
            "task_view",
            "--budget",
            "4000",
        ],
    );
    assert_eq!(refused.status.code(), Some(1));
    let duplicated = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "context",
            "golden-bundle.json",
            "--consumer",
            "planner",
            "--uses",
            "tasks.task_view,tasks.task_view",
            "--budget",
            "4000",
        ],
    );
    assert_eq!(duplicated.status.code(), Some(1));
}

#[test]
fn validate_seam_gates_the_boundary_and_the_lock() {
    let family = scratch_family();
    let dir = alias_free_path(family.path());
    let seam = lekalo_in(
        &dir,
        &[
            "validate",
            "--project",
            "reference/tasks",
            "--module-api",
            "reference/tasks/module-api.json",
        ],
    );
    assert_eq!(seam.status.code(), Some(0));
    let document = envelope(&seam);
    assert_eq!(document["status"], "valid");
    // The strict boundary violation invalidates (exit 1) with the exact
    // rule, over any base verdict.
    let rejected = lekalo_in(
        &dir,
        &[
            "validate",
            "--project",
            "visibility",
            "--strict",
            "--module-api",
            "visibility/module-api.json",
        ],
    );
    assert_eq!(rejected.status.code(), Some(1));
    assert!(codes(&rejected).contains(&"LEK-MODULE-003".to_owned()));
    // The stale checked lock denies with full evidence.
    let stale: Value =
        serde_json::from_slice(&fs::read(dir.join("lock/module-api.json")).unwrap()).unwrap();
    let mut stale = stale;
    for module in stale["modules"].as_array_mut().unwrap().iter_mut() {
        if module["moduleId"] == "tasks" {
            module["apiVersion"] = "1.3.0".into();
        }
    }
    fs::write(
        dir.join("lock/stale-attachment.json"),
        serde_json::to_vec(&stale).unwrap(),
    )
    .unwrap();
    let denied = lekalo_in(
        &dir,
        &[
            "validate",
            "--project",
            "lock",
            "--module-api",
            "lock/stale-attachment.json",
            "--module-api-lock",
            "lock/golden-lock.json",
        ],
    );
    assert_eq!(denied.status.code(), Some(3));
    assert!(codes(&denied).contains(&"LEK-MODULE-013".to_owned()));
}

#[test]
fn hostile_inputs_follow_the_status_contract() {
    let reference = scratch_project("reference/tasks");
    let dir = alias_free_path(reference.path());
    let text = fs::read_to_string(dir.join("module-api.json")).unwrap();
    // Duplicate JSON keys are refused before typed decoding.
    let duplicated = text.replace(
        "\"schemaVersion\":",
        "\"schemaVersion\": \"lekalo/module-api/v0.8.0\", \"schemaVersion\":",
    );
    fs::write(dir.join("duplicate.json"), duplicated).unwrap();
    let refused = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "duplicate.json",
        ],
    );
    assert_eq!(refused.status.code(), Some(1));
    assert!(codes(&refused).contains(&"LEK-MODULE-001".to_owned()));
    // An unknown family version is unsupported-version (exit 5).
    let future = text.replace("v0.8.0", "v9.9.9");
    fs::write(dir.join("future.json"), future).unwrap();
    let unsupported = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "future.json",
        ],
    );
    assert_eq!(unsupported.status.code(), Some(5));
    assert!(codes(&unsupported).contains(&"LEK-MODULE-002".to_owned()));
}
