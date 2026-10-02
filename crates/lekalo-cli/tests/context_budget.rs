//! Issue #75 CLI tests for the `lekalo context-budget` handoff: the
//! hermetic planner fixture, the three scopes, the exact budget
//! boundary, the over-budget advisory envelope, the named profile
//! document, the exit-code classes, the cross-subject comparison, and
//! byte determinism.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/context-budget/planner";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

/// The selection policy denies alias-spelled working directories
/// (`structure.selection-alias`) before any command logic runs; chdir
/// the child into the resolved spelling, stripped of the `\\?\` verbatim
/// prefix `canonicalize` produces on Windows.
fn alias_free_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("fixture path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        // `\\?\C:\...` -> `C:\...`; UNC (`\\?\UNC\...`) stays verbatim.
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

fn fixture_path() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(FIXTURE),
    )
}

fn document(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout_text(output)).expect("valid JSON envelope")
}

/// The symbol report is advisory: over budget stays `valid` at exit 0
/// with the exact over-by arithmetic and the registered warning.
#[test]
fn over_budget_is_advisory_valid_exit_zero_with_explainable_breakdown() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "200",
        ],
    );
    assert_eq!(exit_code(&output), 0, "advisory over-budget exits 0");
    let document = document(&output);
    assert_eq!(document["status"], "valid");
    let report = &document["contextBudget"];
    assert_eq!(
        report["schemaVersion"],
        "lekalo/context-budget-report/v0.6.3"
    );
    assert_eq!(report["identity"], "dev.lekalo.context-budget-report@0.6.3");
    let subject = &report["subjects"][0];
    assert_eq!(subject["assessment"], "over-budget");
    assert_eq!(subject["overByTokens"]["state"], "known");
    // The exact over-by remainder: required minus available.
    let required = subject["metrics"]["minimumRequiredSemanticTokens"]["value"]
        .as_u64()
        .expect("required known");
    let over_by = subject["overByTokens"]["value"]
        .as_u64()
        .expect("over known");
    assert_eq!(over_by, required - 200);
    // The advisory warning rides the valid envelope.
    let warnings = document["diagnostics"].as_array().expect("diagnostics");
    assert!(warnings
        .iter()
        .any(|diagnostic| diagnostic["id"] == "context.budget-exceeded"));
    // The explainable dependency breakdown is present and bounded.
    let breakdown = subject["breakdown"].as_array().expect("breakdown");
    assert!(!breakdown.is_empty());
    for row in breakdown {
        assert!(row["dependency"].is_string());
        assert!(row["exclusiveRequiredTokens"].is_u64());
    }
    // Ledger reconciliation: required facts sum to the required total.
    let ledger: u64 = subject["requiredFacts"]
        .as_array()
        .expect("facts")
        .iter()
        .map(|fact| fact["tokens"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(ledger, required, "the ledger sums to the required total");
}

/// The exact budget boundary: at exactly the required sum the
/// assessment is within-budget, one token below it is over.
#[test]
fn exact_budget_boundary_passes_one_below_is_over() {
    let project = fixture_path();
    let full = document(&lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1000000",
        ],
    ));
    let required = full["contextBudget"]["subjects"][0]["metrics"]["minimumRequiredSemanticTokens"]
        ["value"]
        .as_u64()
        .expect("required known");
    let boundary_run = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            &required.to_string(),
        ],
    );
    assert_eq!(exit_code(&boundary_run), 0);
    let boundary = document(&boundary_run);
    assert_eq!(
        boundary["contextBudget"]["subjects"][0]["assessment"],
        "within-budget"
    );
    let below_run = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            &(required - 1).to_string(),
        ],
    );
    assert_eq!(exit_code(&below_run), 0);
    let below = document(&below_run);
    assert_eq!(
        below["contextBudget"]["subjects"][0]["assessment"],
        "over-budget"
    );
    assert_eq!(
        below["contextBudget"]["subjects"][0]["overByTokens"]["value"],
        1
    );
}

/// Repeated runs are byte-identical (no timestamps, no run ids).
#[test]
fn repeated_runs_are_byte_identical() {
    let project = fixture_path();
    let args = &[
        "--json",
        "context-budget",
        "--symbol",
        "planner.focus_task",
        "--budget",
        "12000",
    ];
    let first = lekalo_in(&project, args);
    let second = lekalo_in(&project, args);
    assert_eq!(exit_code(&first), 0);
    assert_eq!(stdout_text(&first), stdout_text(&second));
    let text = stdout_text(&first);
    assert!(!text.contains("timestamp"));
    assert!(!text.contains("C:/"));
}

/// The module scope enumerates every owned definition and reconciles
/// the union against the per-subject sums.
#[test]
fn module_scope_reconciles_union() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--module",
            "planner",
            "--budget",
            "12000",
        ],
    );
    assert_eq!(exit_code(&output), 0);
    let document = document(&output);
    let report = &document["contextBudget"];
    assert_eq!(report["scope"]["kind"], "module");
    let subjects = report["subjects"].as_array().expect("subjects");
    assert!(subjects.len() > 1, "the module owns many definitions");
    assert_eq!(report["summary"]["subjects"], subjects.len() as u64);
    if let Some(union) = report["summary"]["unionRequiredTokens"]["value"].as_u64() {
        let per_subject: u64 = subjects
            .iter()
            .map(|subject| {
                subject["metrics"]["minimumRequiredSemanticTokens"]["value"]
                    .as_u64()
                    .unwrap_or(0)
            })
            .sum();
        assert!(
            union <= per_subject,
            "the union dedups shared facts instead of summing closures"
        );
    }
}

/// The malformed invocations are stable usage errors: no selector, both
/// budget handles, an unknown profile, and a missing profile document.
#[test]
fn malformed_invocations_refuse_closed() {
    let project = fixture_path();
    let cases: Vec<Vec<&str>> = vec![
        vec!["context-budget", "--symbol", "planner.focus_task"],
        vec!["context-budget", "--budget", "1000"],
        vec![
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1000",
            "--budget-profile",
            "local-12k",
            "--profiles",
            "profiles.json",
        ],
        vec![
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget-profile",
            "local-12k",
        ],
    ];
    for args in &cases {
        let output = lekalo_in(&project, args);
        assert_eq!(exit_code(&output), 1, "{args:?} is a usage error");
    }
    // An unknown selector is the registered graph.unknown-node refusal.
    // The invalid envelope writes stderr, so read it from there.
    let unknown = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "ghost.symbol",
            "--budget",
            "1000",
        ],
    );
    assert_eq!(exit_code(&unknown), 1);
    let stderr = String::from_utf8(unknown.stderr.clone()).expect("stderr utf8");
    let document: serde_json::Value = serde_json::from_str(&stderr).expect("invalid envelope");
    assert_eq!(document["status"], "invalid");
    assert_eq!(document["reasonCodes"][0], "graph.unknown-node");
}

/// The named profile document resolves, pins its digest, and refuses an
/// unknown profile id closed.
#[test]
fn named_profile_resolves_and_refuses_unknown() {
    let project = fixture_copy("profiles");
    let profiles = write_profiles(&project);
    let good_run = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget-profile",
            "local-12k",
            "--profiles",
            profiles.to_str().expect("utf8 path"),
        ],
    );
    assert_eq!(exit_code(&good_run), 0);
    let good = document(&good_run);
    let profile = &good["contextBudget"]["profile"];
    assert_eq!(profile["id"], "local-12k");
    assert_eq!(profile["availableContentTokens"], 12000);
    assert!(profile["digest"]
        .as_str()
        .expect("digest")
        .starts_with("sha256:"));
    let unknown = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget-profile",
            "other",
            "--profiles",
            profiles.to_str().expect("utf8 path"),
        ],
    );
    assert_eq!(exit_code(&unknown), 1);
}

/// An unsupported estimator identity in the profile document is the
/// closed `unsupported-version` exit 5 (no silent fallback).
#[test]
fn unsupported_estimator_is_unsupported_version() {
    let project = fixture_copy("estimator");
    let profiles = write_profiles_with_estimator(&project, "dev.lekalo.estimator.claude-3");
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget-profile",
            "local-12k",
            "--profiles",
            profiles.to_str().expect("utf8 path"),
        ],
    );
    assert_eq!(exit_code(&output), 5, "unsupported estimator exits 5");
    // The unsupported-version envelope writes stderr; read it from there.
    let stderr = String::from_utf8(output.stderr.clone()).expect("stderr utf8");
    let document: serde_json::Value = serde_json::from_str(&stderr).expect("envelope");
    assert_eq!(document["status"], "unsupported-version");
    assert_eq!(document["reasonCodes"][0], "context.profile-unsupported");
}

/// AC6: the planner reference symbol measures narrower than the module
/// that owns the whole flow under the identical profile.
#[test]
fn planner_reference_is_narrower_than_module() {
    let project = fixture_path();
    let symbol = document(&lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1000000",
        ],
    ));
    let module = document(&lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--module",
            "planner",
            "--budget",
            "1000000",
        ],
    ));
    let symbol_required = symbol["contextBudget"]["subjects"][0]["metrics"]
        ["minimumRequiredSemanticTokens"]["value"]
        .as_u64()
        .expect("symbol required");
    let module_required: u64 = module["contextBudget"]["subjects"]
        .as_array()
        .expect("module subjects")
        .iter()
        .map(|subject| {
            subject["metrics"]["minimumRequiredSemanticTokens"]["value"]
                .as_u64()
                .unwrap_or(0)
        })
        .sum();
    assert!(module_required >= symbol_required);
}

/// The simulation block exposes the missing required facts at a tiny
/// budget instead of claiming a sufficient capsule.
#[test]
fn tiny_budget_simulation_exposes_missing_required() {
    let project = fixture_path();
    let output = document(&lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1",
            "--simulate-capsule",
        ],
    ));
    let simulation = &output["contextBudget"]["subjects"][0]["simulation"];
    assert_eq!(simulation["requiredFits"], false);
    let missing = simulation["missingRequiredIds"]
        .as_array()
        .expect("missing");
    assert!(!missing.is_empty());
    assert_eq!(simulation["legacyFits"], false);
}

/// A private temp copy of the committed fixture project: generated
/// profile/policy/baseline documents land here, so the tracked fixture
/// tree stays byte-identical after any test run.
fn fixture_copy(tag: &str) -> PathBuf {
    let temp = tempfile::tempdir().expect("temp dir").keep();
    let target = temp.join("project");
    copy_dir(&fixture_path(), &target);
    let marker = target.join(format!(".{tag}-used"));
    std::fs::write(&marker, b"").expect("marker");
    // The temp dir is intentionally kept for the child process; the OS
    // temp sweep owns its cleanup (tests never write into the checkout).
    let mut marker = target.clone();
    marker.push(format!(".{tag}-used"));
    std::fs::write(&marker, b"").expect("marker");
    target
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("entry");
        let target_path = target.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            std::fs::copy(entry.path(), target_path).expect("copy file");
        }
    }
}

fn write_profiles(project: &Path) -> PathBuf {
    write_profiles_with_estimator(project, "dev.lekalo.estimator.chars-4@0.2.16")
}

fn write_profiles_with_estimator(project: &Path, estimator: &str) -> PathBuf {
    // Unique per estimator so concurrently running tests never share a
    // document; the file lives inside the temp project and is read by
    // its own invocation only.
    let stem = estimator
        .rsplit('@')
        .next()
        .unwrap_or("estimator")
        .replace('.', "-");
    let path = project.join(format!("context-budget-profiles-{stem}.json"));
    let digest = if estimator.ends_with("chars-4@0.2.16") {
        "sha256:602e648c2ff7c58cace92876f6c834c5c1735ce594e3ba565d7b012f752fb0be"
    } else {
        "sha256:0000000000000000000000000000000000000000000000000000000000000000"
    };
    let document = format!(
        r#"{{"schemaVersion":"lekalo/context-budget-profile/v0.6.3","identity":"dev.lekalo.context-budget-profile@0.6.3","profiles":[{{"id":"local-12k","version":"1","estimator":{{"id":"{estimator}","version":"0.2.16","specDigest":"{digest}"}},"budget":{{"contextWindowTokens":16384,"reservedOutputTokens":2048,"reservedSystemToolTokens":2336}},"limits":{{"maxNodes":50000,"maxEdges":250000,"maxFacts":50000,"maxSubjects":10000}}}}]}}"#
    );
    std::fs::write(&path, document).expect("write profiles");
    path
}

/// AC3: a mandatory policy denies with exit 3 over the exact profile
/// pin, and the report still rides the denied envelope; the passing
/// side keeps exit 0. `suggestions_never_write`: no file appears.
#[test]
fn mandatory_policy_denies_and_passes_on_the_pin() {
    let project = fixture_copy("policy");
    // Pin the policy to the effective profile of a passing report.
    let passing = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1000000",
        ],
    );
    assert_eq!(exit_code(&passing), 0);
    let report = document(&passing);
    let profile = &report["contextBudget"]["profile"];
    let policy = format!(
        r#"{{"schemaVersion":"lekalo/context-budget-policy/v0.6.3","identity":"dev.lekalo.context-budget-policy@0.6.3","mode":"mandatory","profileRef":{{"id":"{}","version":"{}","digest":"{}"}},"failOn":["over-budget"],"regressionLimits":[]}}"#,
        profile["id"].as_str().expect("id"),
        profile["version"].as_str().expect("version"),
        profile["digest"].as_str().expect("digest"),
    );
    let policy_path = project.join("context-budget-policy-pin.json");
    std::fs::write(&policy_path, policy).expect("write policy");
    let pass = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1000000",
            "--policy",
            policy_path.to_str().expect("utf8 policy"),
        ],
    );
    assert_eq!(exit_code(&pass), 0, "the passing side exits 0");
    let deny = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "200",
            "--policy",
            policy_path.to_str().expect("utf8 policy"),
        ],
    );
    assert_eq!(exit_code(&deny), 3, "the mandatory policy denies with 3");
    // The denial keeps its stdout envelope with the mirrored evidence:
    // denied writes stdout (exit 3), never stderr.
    let stdout = String::from_utf8(deny.stdout.clone()).expect("stdout utf8");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("denial envelope");
    assert_eq!(document["status"], "denied");
    assert_eq!(document["reasonCodes"][0], "context.policy-denied");
    let _ = std::fs::remove_file(&policy_path);
}

/// A policy pinned to a different profile digest denies before any
/// metric evaluation (`policy_cannot_be_weakened_by_override`).
#[test]
fn policy_pin_mismatch_denies() {
    let project = fixture_copy("pin");
    let policy = r#"{"schemaVersion":"lekalo/context-budget-policy/v0.6.3","identity":"dev.lekalo.context-budget-policy@0.6.3","mode":"mandatory","profileRef":{"id":"local-12k","version":"1","digest":"sha256:1111111111111111111111111111111111111111111111111111111111111111"},"failOn":["over-budget"],"regressionLimits":[]}"#;
    let policy_path = project.join("context-budget-policy-mismatch.json");
    std::fs::write(&policy_path, policy).expect("write policy");
    let denied = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1000000",
            "--policy",
            policy_path.to_str().expect("utf8 policy"),
        ],
    );
    assert_eq!(exit_code(&denied), 3);
    let _ = std::fs::remove_file(&policy_path);
}

/// AC5: a comparable baseline produces no regression warning; a
/// changed-profile baseline is an explicit incomparable row.
#[test]
fn baseline_comparison_records_verdicts() {
    let project = fixture_copy("baseline");
    let baseline_run = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1000000",
        ],
    );
    assert_eq!(exit_code(&baseline_run), 0);
    let baseline = document(&baseline_run);
    let baseline_path = project.join("context-budget-baseline.json");
    std::fs::write(
        &baseline_path,
        serde_json::to_string(&baseline["contextBudget"]).expect("serialize baseline"),
    )
    .expect("write baseline");
    // The identical rerun is comparable: no baseline warning appears.
    let same_run = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1000000",
            "--baseline",
            baseline_path.to_str().expect("utf8 baseline"),
        ],
    );
    assert_eq!(exit_code(&same_run), 0);
    let same = document(&same_run);
    let warnings = same["diagnostics"].as_array().cloned().unwrap_or_default();
    assert!(!warnings
        .iter()
        .any(|diagnostic| diagnostic["id"] == "context.baseline-incomparable"));
    // A changed budget changes the profile digest: incomparable.
    let changed = document(&lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "200",
            "--baseline",
            baseline_path.to_str().expect("utf8 baseline"),
        ],
    ));
    let changed_warnings = changed["diagnostics"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(changed_warnings
        .iter()
        .any(|diagnostic| diagnostic["id"] == "context.baseline-incomparable"));
    // A malformed baseline is invalid, never incomparable.
    std::fs::write(&baseline_path, b"{ broken").expect("corrupt baseline");
    let malformed = lekalo_in(
        &project,
        &[
            "--json",
            "context-budget",
            "--symbol",
            "planner.focus_task",
            "--budget",
            "1000000",
            "--baseline",
            baseline_path.to_str().expect("utf8 baseline"),
        ],
    );
    assert_eq!(exit_code(&malformed), 1);
    let _ = std::fs::remove_file(&baseline_path);
}

/// AC6: the paired fixture comparison through the CLI — the integration
/// workload spans more modules, more hops, and costs more than the
/// planner reference under the identical pinned profile (both sides
/// resolved from the same checkout).
#[test]
fn integration_workload_is_broader_than_planner_reference() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
    let planner = alias_free_path(&workspace.join("tests/fixtures/context-budget/planner"));
    let integration = alias_free_path(&workspace.join("tests/fixtures/context-budget/integration"));
    let selection = ["--json", "context-budget", "--budget", "1000000"];
    let planner_report = document(&lekalo_in(
        &planner,
        &[
            selection[0],
            selection[1],
            "--symbol",
            "planner.focus_task",
            selection[2],
            selection[3],
        ],
    ));
    let integration_report = document(&lekalo_in(
        &integration,
        &[
            selection[0],
            selection[1],
            "--symbol",
            "integration.sync_external_objects",
            selection[2],
            selection[3],
        ],
    ));
    let value = |report: &serde_json::Value, metric: &str| {
        report["contextBudget"]["subjects"][0]["metrics"][metric]["value"]
            .as_u64()
            .unwrap_or(0)
    };
    assert!(
        value(&integration_report, "minimumRequiredSemanticTokens")
            > value(&planner_report, "minimumRequiredSemanticTokens"),
        "the integration workload costs more"
    );
    assert!(
        value(&integration_report, "requiredModules") > value(&planner_report, "requiredModules"),
        "the integration workload spans more modules"
    );
    assert!(
        value(&integration_report, "maxCrossModuleHops")
            > value(&planner_report, "maxCrossModuleHops"),
        "the integration workload crosses boundaries"
    );
    // Identical pinned profile on both sides of the comparison.
    assert_eq!(
        planner_report["contextBudget"]["profile"]["digest"],
        integration_report["contextBudget"]["profile"]["digest"]
    );
}
