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
    let project = fixture_path();
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
    let project = fixture_path();
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
