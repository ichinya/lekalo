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
            "module-api.json",
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
            "module-api.json",
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
            "stale-attachment.json",
            "--module-api-lock",
            "golden-lock.json",
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

/// The r1 review probes at the CLI seam.

#[test]
fn private_closure_leaks_refuse_certified_export() {
    // Finding 1: the exported shape's private field is an exposure
    // finding; the bundle is refused in both modes.
    let leak = scratch_project("leak");
    let dir = alias_free_path(leak.path());
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
    assert!(codes(&strict).contains(&"LEK-MODULE-005".to_owned()));
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
    let refused = lekalo_in(
        &dir,
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
}

#[test]
fn unattached_producers_report_and_unknown_scopes_refuse() {
    // Finding 2: a producer absent from the attachment is a boundary
    // finding, never a silent pass. Finding 14: an unknown --module
    // scope is invalid, never a silent pass.
    let leak = scratch_project("leak");
    let dir = alias_free_path(leak.path());
    let mut attachment: Value =
        serde_json::from_slice(&fs::read(dir.join("module-api.json")).unwrap()).unwrap();
    attachment["modules"]
        .as_array_mut()
        .unwrap()
        .retain(|module| module["moduleId"] != "tasks");
    fs::write(
        dir.join("unattached.json"),
        serde_json::to_vec(&attachment).unwrap(),
    )
    .unwrap();
    let unattached = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "unattached.json",
            "--strict",
        ],
    );
    assert_eq!(unattached.status.code(), Some(1));
    assert!(codes(&unattached).contains(&"LEK-MODULE-003".to_owned()));
    let ghost = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "module-api.json",
            "--module",
            "ghost",
        ],
    );
    assert_eq!(ghost.status.code(), Some(1));
    assert!(codes(&ghost).contains(&"LEK-MODULE-001".to_owned()));
}

#[test]
fn kind_contradictions_and_unknown_import_uses_refuse() {
    // Finding 16: mislabeled kinds and unknown import uses are invalid.
    let reference = scratch_project("reference/tasks");
    let dir = alias_free_path(reference.path());
    let mut attachment: Value =
        serde_json::from_slice(&fs::read(dir.join("module-api.json")).unwrap()).unwrap();
    for module in attachment["modules"].as_array_mut().unwrap().iter_mut() {
        if module["moduleId"] == "tasks" {
            for entry in module["exports"].as_array_mut().unwrap().iter_mut() {
                if entry["export"] == "tasks.task_view" {
                    entry["kind"] = "entity".into();
                }
            }
        }
    }
    fs::write(
        dir.join("mislabeled.json"),
        serde_json::to_vec(&attachment).unwrap(),
    )
    .unwrap();
    let mislabeled = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "mislabeled.json",
        ],
    );
    assert_eq!(mislabeled.status.code(), Some(1));

    let mut attachment: Value =
        serde_json::from_slice(&fs::read(dir.join("module-api.json")).unwrap()).unwrap();
    for module in attachment["modules"].as_array_mut().unwrap().iter_mut() {
        if module["moduleId"] == "planner" {
            module["imports"] =
                serde_json::json!([{ "moduleId": "tasks", "uses": ["tasks.nonexistent"] }]);
        }
    }
    fs::write(
        dir.join("unknown-use.json"),
        serde_json::to_vec(&attachment).unwrap(),
    )
    .unwrap();
    let unknown_use = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "unknown-use.json",
        ],
    );
    assert_eq!(unknown_use.status.code(), Some(1));
}

#[test]
fn error_exports_carry_registry_facts_and_uses_stay_bounded() {
    // Finding 22: the typed #62 error export surface is reachable; the
    // per-consumer use bound is enforced.
    let errors = scratch_project("errors");
    let dir = alias_free_path(errors.path());
    let exported = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "export",
            "planner",
            "--project",
            ".",
            "--api",
            "module-api.json",
        ],
    );
    assert_eq!(exported.status.code(), Some(0));
    let document = envelope(&exported);
    let error_export = document["modulePublicContract"]["exports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "error")
        .unwrap()
        .clone();
    assert_eq!(error_export["code"], "LEK-ERR-005");
    assert_eq!(error_export["retryable"], false);

    let mut attachment: Value =
        serde_json::from_slice(&fs::read(dir.join("module-api.json")).unwrap()).unwrap();
    let flood: Vec<String> = (0..5000)
        .map(|index| format!("planner.symbol_{index}"))
        .collect();
    attachment["modules"][0]["consumers"] = serde_json::json!([
        { "consumer": "watcher", "uses": flood, "mode": "wire", "policy": null }
    ]);
    fs::write(
        dir.join("flooded.json"),
        serde_json::to_vec(&attachment).unwrap(),
    )
    .unwrap();
    let flooded = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "flooded.json",
        ],
    );
    assert_eq!(flooded.status.code(), Some(1));
    assert!(codes(&flooded).contains(&"LEK-MODULE-001".to_owned()));
}

#[test]
fn validate_seam_advisory_keeps_exit_zero() {
    // Finding 8: the advisory default keeps exit 0 and records boundary
    // findings as valid evidence; only --strict invalidates.
    let family = scratch_family();
    let dir = alias_free_path(family.path());
    let advisory = lekalo_in(
        &dir,
        &[
            "validate",
            "--project",
            "visibility",
            "--module-api",
            "module-api.json",
        ],
    );
    assert_eq!(advisory.status.code(), Some(0));
    let document = envelope(&advisory);
    assert_eq!(document["status"], "valid");
    assert!(codes(&advisory).contains(&"LEK-MODULE-003".to_owned()));
    let strict = lekalo_in(
        &dir,
        &[
            "validate",
            "--project",
            "visibility",
            "--strict",
            "--module-api",
            "module-api.json",
        ],
    );
    assert_eq!(strict.status.code(), Some(1));
}

/// The r3 review probes at the CLI seam.

#[test]
fn r3_supplied_check_lock_is_honored_or_refused() {
    // r3 findings 7/16: `check --lock PATH` without --check-lock is a
    // usage error, never a silently ignored lock; a nonexistent lock
    // under --check-lock refuses instead of looking checked.
    let reference = scratch_project("reference/tasks");
    let dir = alias_free_path(reference.path());
    let ignored = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "module-api.json",
            "--lock",
            "golden-lock.json",
        ],
    );
    assert_eq!(ignored.status.code(), Some(1));
    let refused = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "module-api.json",
            "--check-lock",
            "--lock",
            "nonexistent-lock.json",
        ],
    );
    assert_eq!(refused.status.code(), Some(1));
    assert!(codes(&refused).contains(&"LEK-MODULE-001".to_owned()));
}

#[test]
fn r3_strict_diff_requires_consumer_snapshots() {
    // r3 finding 7/codex: strict verdicts require all declared evidence;
    // a diff without the consumer inventory is unavailable (exit 4) with
    // typed evidence, never a silent approval.
    let family = scratch_family();
    let dir = alias_free_path(family.path()).join("diff");
    let strict = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "diff",
            "golden-bundle-before.json",
            "golden-bundle-after.json",
            "--strict",
        ],
    );
    assert_eq!(strict.status.code(), Some(4));
    assert!(codes(&strict).contains(&"LEK-MODULE-012".to_owned()));
    // Advisory keeps the honest unknown-coverage report at exit 0.
    let advisory = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "diff",
            "golden-bundle-before.json",
            "golden-bundle-after.json",
        ],
    );
    assert_eq!(advisory.status.code(), Some(0));
    let report = envelope(&advisory)["moduleApiDiff"].clone();
    assert!(report["coverage"]["unknownReasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "consumer-snapshots-missing"));
}

#[test]
fn r3_project_bound_inputs_reject_escape_paths() {
    // r3 finding 13/codex: the attachment selector resolves through the
    // classified project boundary — absolute and traversal spellings are
    // refused, never interpreted against the invocation directory.
    let reference = scratch_project("reference/tasks");
    let dir = alias_free_path(reference.path());
    let absolute = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            dir.join("module-api.json").to_str().unwrap(),
        ],
    );
    assert_eq!(absolute.status.code(), Some(1));
    let traversal = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "../module-api.json",
        ],
    );
    assert_eq!(traversal.status.code(), Some(1));
}

#[test]
fn r3_owner_evidence_gaps_exit_unavailable() {
    // r3 finding 14/codex: exported operations without declared owner
    // evidence make the strict check unavailable (exit 4) and refuse the
    // certified export in every mode.
    let reference = scratch_project("reference/tasks");
    let dir = alias_free_path(reference.path());
    let mut attachment: Value =
        serde_json::from_slice(&fs::read(dir.join("module-api.json")).unwrap()).unwrap();
    for module in attachment["modules"].as_array_mut().unwrap().iter_mut() {
        module.as_object_mut().unwrap().remove("operationErrors");
        module.as_object_mut().unwrap().remove("queryFacts");
    }
    fs::write(
        dir.join("no-owner-evidence.json"),
        serde_json::to_vec(&attachment).unwrap(),
    )
    .unwrap();
    let strict = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "check",
            "--project",
            ".",
            "--api",
            "no-owner-evidence.json",
            "--strict",
        ],
    );
    assert_eq!(strict.status.code(), Some(4));
    assert!(codes(&strict).contains(&"LEK-MODULE-012".to_owned()));
    let export = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "export",
            "tasks",
            "--project",
            ".",
            "--api",
            "no-owner-evidence.json",
        ],
    );
    assert_eq!(export.status.code(), Some(4));
    assert!(codes(&export).contains(&"LEK-MODULE-012".to_owned()));
}

#[test]
fn r4_validate_seam_requires_complete_evidence() {
    // r4 finding 1/codex: the strict validate seam composes required
    // evidence gaps as unavailable/4 with LEK-MODULE-012 — the same
    // complete-evidence decision as `module api check --strict` — never a
    // valid base with the gap demoted to a warning.
    let reference = scratch_project("reference/tasks");
    let dir = alias_free_path(reference.path());
    let mut attachment: Value =
        serde_json::from_slice(&fs::read(dir.join("module-api.json")).unwrap()).unwrap();
    for module in attachment["modules"].as_array_mut().unwrap().iter_mut() {
        module.as_object_mut().unwrap().remove("operationErrors");
        module.as_object_mut().unwrap().remove("queryFacts");
    }
    fs::write(
        dir.join("no-owner-evidence.json"),
        serde_json::to_vec(&attachment).unwrap(),
    )
    .unwrap();
    let control = lekalo_in(
        &dir,
        &[
            "validate",
            "--project",
            ".",
            "--module-api",
            "module-api.json",
        ],
    );
    assert_eq!(control.status.code(), Some(0), "intact control stays valid");
    let gap = lekalo_in(
        &dir,
        &[
            "validate",
            "--project",
            ".",
            "--strict",
            "--module-api",
            "no-owner-evidence.json",
        ],
    );
    assert_eq!(
        gap.status.code(),
        Some(4),
        "the gap is unavailable, not valid"
    );
    assert!(codes(&gap).contains(&"LEK-MODULE-012".to_owned()));
}

#[test]
fn r4_validate_seam_receipt_counts_track_appended_evidence() {
    // r4 finding 1/codex: the advisory receipt's severity counts report
    // the appended boundary evidence instead of warning=0.
    let reference = scratch_project("reference/tasks");
    let dir = alias_free_path(reference.path());
    let output = lekalo_in(
        &dir,
        &[
            "validate",
            "--project",
            ".",
            "--module-api",
            "module-api.json",
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    let document = envelope(&output);
    let counts = &document["validation"]["counts"];
    assert_eq!(counts["error"].as_u64(), Some(0));
    let warning = counts["warning"].as_u64().unwrap_or(0);
    let info = counts["info"].as_u64().unwrap_or(0);
    let reported = document["diagnostics"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    assert!(
        (warning + info) as usize >= reported.min(1),
        "the receipt counts the appended evidence (warning={warning} info={info})"
    );
}

#[test]
fn r4_context_rejects_invalid_consumer_selectors() {
    // r4 finding 9/codex: an invalid consumer selector refuses at the
    // pure seam — never a capsule the published schema rejects.
    let family = scratch_family();
    let dir = alias_free_path(family.path());
    let bundle = dir.join("standalone/golden-bundle.json");
    for bad in ["!", "Bad_ID", ""] {
        let output = lekalo_in(
            &dir,
            &[
                "module",
                "api",
                "context",
                bundle.to_str().unwrap(),
                "--consumer",
                bad,
                "--uses",
                "tasks.task_view",
                "--budget",
                "4000",
            ],
        );
        assert_eq!(output.status.code(), Some(1), "consumer {bad:?} refuses");
        assert!(codes(&output).contains(&"LEK-MODULE-001".to_owned()));
    }
}

#[test]
fn r4_mapping_conformance_proves_the_two_implementations() {
    // r4 finding 10/codex: the in-process and extracted-service mapping
    // evidence — independently authored documents against one public
    // digest — prove conformance through the real CLI.
    let family = scratch_family();
    let dir = alias_free_path(family.path());
    let args = [
        "module",
        "api",
        "mapping",
        "standalone/golden-bundle.json",
        "--in-process",
        "mapping/in-process/mapping-evidence.json",
        "--service",
        "mapping/service/mapping-evidence.json",
    ];
    let proven = lekalo_in(&dir, &args);
    assert_eq!(proven.status.code(), Some(0));
    let document = envelope(&proven);
    assert_eq!(document["moduleMappingConformance"]["conforms"], true);
    assert_eq!(document["moduleMappingConformance"]["observations"], 4);

    // A divergent observation (the service author saw a different error
    // union), correctly resealed, is an unproven conformance: exit 4
    // with the typed evidence gap, never a silent pass.
    let mut service: Value = serde_json::from_slice(
        &fs::read(dir.join("mapping/service/mapping-evidence.json")).unwrap(),
    )
    .unwrap();
    service["observations"][0]["errors"] = serde_json::json!(["tasks.public_error"]);
    seal_evidence(&mut service);
    fs::write(
        dir.join("mapping/service/divergent.json"),
        serde_json::to_vec(&service).unwrap(),
    )
    .unwrap();
    let divergent = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "mapping",
            "standalone/golden-bundle.json",
            "--in-process",
            "mapping/in-process/mapping-evidence.json",
            "--service",
            "mapping/service/divergent.json",
        ],
    );
    assert_eq!(divergent.status.code(), Some(4));
    assert!(codes(&divergent).contains(&"LEK-MODULE-012".to_owned()));

    // An implementation that cannot provide a required capability
    // diverges from the certified contract explicitly.
    let mut unsupported: Value = serde_json::from_slice(
        &fs::read(dir.join("mapping/service/mapping-evidence.json")).unwrap(),
    )
    .unwrap();
    unsupported["unsupported"] = serde_json::json!(["http"]);
    seal_evidence(&mut unsupported);
    fs::write(
        dir.join("mapping/service/unsupported.json"),
        serde_json::to_vec(&unsupported).unwrap(),
    )
    .unwrap();
    let refused = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "mapping",
            "standalone/golden-bundle.json",
            "--in-process",
            "mapping/in-process/mapping-evidence.json",
            "--service",
            "mapping/service/unsupported.json",
        ],
    );
    assert_eq!(refused.status.code(), Some(4));
    assert!(codes(&refused).contains(&"LEK-MODULE-012".to_owned()));

    // A malformed row (unknown implementation) is invalid input.
    let mut malformed: Value = serde_json::from_slice(
        &fs::read(dir.join("mapping/service/mapping-evidence.json")).unwrap(),
    )
    .unwrap();
    malformed["implementation"] = serde_json::json!("frankenstein");
    seal_evidence(&mut malformed);
    fs::write(
        dir.join("mapping/service/malformed.json"),
        serde_json::to_vec(&malformed).unwrap(),
    )
    .unwrap();
    let invalid = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "mapping",
            "standalone/golden-bundle.json",
            "--in-process",
            "mapping/in-process/mapping-evidence.json",
            "--service",
            "mapping/service/malformed.json",
        ],
    );
    assert_eq!(invalid.status.code(), Some(1));
    assert!(codes(&invalid).contains(&"LEK-MODULE-001".to_owned()));
}

/// Seal one mapping-evidence document over its canonical bytes (the exact
/// digest domain of the family).
fn seal_evidence(value: &mut Value) {
    let original = value.clone();
    value["digest"] = serde_json::Value::String(String::new());
    let canonical = lekalo_core::module_api::wire::canonical(value);
    let digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(
            format!("lekalo/module-mapping-evidence/1\n{canonical}").as_bytes()
        )
    );
    *value = original;
    value["digest"] = serde_json::Value::String(digest);
}

#[test]
fn r4_strict_diff_denies_required_capability_gap() {
    // r4 finding 7/codex: an added required capability has no operation
    // mapping in the bundle, so strict diff treats its consumer impact as
    // an explicit required evidence gap (exit 4), never an exa
    // unaffected claim.
    let family = scratch_family();
    let dir = alias_free_path(family.path());
    let mut after: Value =
        serde_json::from_slice(&fs::read(dir.join("diff/golden-bundle-after.json")).unwrap())
            .unwrap();
    after["capabilities"] = serde_json::json!([
        { "capability": "audit.sink", "obligation": "required", "owner": "compliance", "version": "1.0.0" },
        { "capability": "http", "obligation": "required", "owner": null, "version": null }
    ]);
    seal_bundle(&mut after);
    // Re-pin the after snapshot to the resealed bundle so the pair binds.
    let mut snapshot: Value =
        serde_json::from_slice(&fs::read(dir.join("diff/golden-consumers-after.json")).unwrap())
            .unwrap();
    snapshot["pins"]["apiDigest"] = after["digest"].clone();
    seal_snapshot(&mut snapshot);
    fs::write(
        dir.join("diff/required-capability-consumers.json"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("diff/required-capability.json"),
        serde_json::to_vec(&after).unwrap(),
    )
    .unwrap();
    let strict = lekalo_in(
        &dir,
        &[
            "module",
            "api",
            "diff",
            "diff/golden-bundle-before.json",
            "diff/required-capability.json",
            "--consumers-before",
            "diff/golden-consumers-before.json",
            "--consumers-after",
            "diff/required-capability-consumers.json",
            "--strict",
        ],
    );
    assert_eq!(strict.status.code(), Some(4));
    assert!(codes(&strict).contains(&"LEK-MODULE-012".to_owned()));
}

/// Seal one public bundle over its canonical projection (the exact digest
/// domain of the family: zeroed revisions, excluded pins).
fn seal_bundle(value: &mut Value) {
    let original_pins = value["pins"].clone();
    value["digest"] = serde_json::Value::String(String::new());
    value["pins"] = serde_json::json!({
        "modelVersion": "",
        "irDigest": "",
        "sourceRevision": { "state": "unknown" }
    });
    if let Some(exports) = value["exports"].as_array_mut() {
        for export in exports.iter_mut() {
            export["version"] = serde_json::Value::from(0);
        }
    }
    let canonical = lekalo_core::module_api::wire::canonical(value);
    let digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(
            format!("lekalo/module-public-contract/1\n{canonical}").as_bytes()
        )
    );
    value["digest"] = digest.into();
    value["pins"] = original_pins;
}

/// Seal one consumer snapshot over its canonical bytes.
fn seal_snapshot(value: &mut Value) {
    let original = value.clone();
    value["digest"] = serde_json::Value::String(String::new());
    let canonical = lekalo_core::module_api::wire::canonical(value);
    let digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(
            format!("lekalo/module-api-consumers/1\n{canonical}").as_bytes()
        )
    );
    *value = original;
    value["digest"] = serde_json::Value::String(digest);
}
