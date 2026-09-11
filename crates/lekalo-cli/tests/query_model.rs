//! Issue #64 CLI tests for the `lekalo query-model` handoff: the
//! validate envelope over the hermetic planner fixture, the strict
//! tenant gate, the read-only kind enforcement, the model-dependent
//! refusals, the pure diff classification, and the custody probe.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PLACEHOLDER_DIGEST: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn copy_fixture(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == ".lekalo" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn scratch() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    copy_fixture(&fixture_path(), temp.path());
    temp
}

/// Copy the fixture project and one attachment document into it.
fn scratch_with_attachment(name: &str) -> tempfile::TempDir {
    let temp = scratch();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join("tests/fixtures/query-model")
        .join(name);
    fs::copy(&source, temp.path().join("queries.attachment.json")).unwrap();
    temp
}

fn digest(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        lekalo_core::versioning::plan::sha256_hex(bytes)
    )
}

/// Rebind the attachment to the exact canonical load envelope of the
/// copied fixture project.
fn repin_model(dir: &Path, value: &mut Value) {
    let load = lekalo_in(dir, &["--json", "load", "--project", "."]);
    assert_eq!(exit_code(&load), 0, "{}", stderr_text(&load));
    value["modelRef"]["digest"] = digest(load.stdout.strip_suffix(b"\n").unwrap()).into();
}

fn attachment_value(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join("queries.attachment.json")).unwrap()).unwrap()
}

fn save_attachment(dir: &Path, value: &Value) {
    fs::write(
        dir.join("queries.attachment.json"),
        serde_json::to_vec(value).unwrap(),
    )
    .unwrap();
}

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
        Some(stripped) => PathBuf::from(stripped),
        None => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn exit_code(output: &Output) -> u8 {
    output.status.code().expect("exit code") as u8
}

fn fixture_path() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/query-model/project"),
    )
}

fn validate_args(strict: bool) -> Vec<&'static str> {
    let mut args = vec![
        "--json",
        "query-model",
        "validate",
        "queries.attachment.json",
    ];
    if strict {
        args.push("--strict");
    }
    args.push("--project");
    args.push(".");
    args
}

/// Run a validation expected to succeed and return the JSON envelope.
fn validate(dir: &Path, strict: bool) -> Value {
    let output = lekalo_in(dir, &validate_args(strict));
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert!(stderr_text(&output).is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Run a validation expected to fail: stdout stays empty and the human
/// envelope on stderr names the registered rule.
fn validate_err(dir: &Path, strict: bool) -> String {
    let output = lekalo_in(dir, &validate_args(strict));
    assert_eq!(
        exit_code(&output),
        1,
        "expected refusal: {}",
        stderr_text(&output)
    );
    assert!(output.stdout.is_empty(), "no partial success on stdout");
    stderr_text(&output)
}

#[test]
fn validate_emits_the_plan_envelope_and_is_deterministic() {
    let temp = scratch_with_attachment("valid/planner.queries.json");
    let dir = temp.path();
    let mut value = attachment_value(dir);
    assert_eq!(value["modelRef"]["digest"], PLACEHOLDER_DIGEST);
    repin_model(dir, &mut value);
    save_attachment(dir, &value);

    let first = validate(dir, false);
    assert_eq!(first["status"], "valid");
    assert_eq!(first["queryModel"]["projectId"], "planner");
    assert_eq!(first["queryModel"]["queryCount"], 6);
    assert_eq!(first["queryModel"]["foreignQueries"], 1);
    assert_eq!(first["queryModel"]["tenancyScopes"], 2);
    let plans = first["plans"].as_array().expect("plans array");
    assert_eq!(plans.len(), 6);

    // The foreign escape blocks managed generation: no steps.
    let foreign = plans
        .iter()
        .find(|plan| plan["query"] == "planner.focus_search")
        .expect("foreign plan");
    assert_eq!(foreign["foreign"], true);
    assert_eq!(foreign["steps"].as_array().unwrap().len(), 0);

    // The managed page plan is the closed ordered contract.
    let page = plans
        .iter()
        .find(|plan| plan["query"] == "planner.tasks_today")
        .expect("page plan");
    assert_eq!(page["foreign"], false);
    let kinds: Vec<&str> = page["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["filter", "sort", "cursor", "limit", "project"]);

    // The carry-over read keeps the closed vocabulary and order.
    let carry = plans
        .iter()
        .find(|plan| plan["query"] == "planner.tasks_carry_over")
        .expect("carry-over plan");
    let carry_kinds: Vec<&str> = carry["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["kind"].as_str().unwrap())
        .collect();
    assert_eq!(carry_kinds, vec!["filter", "sort", "project"]);

    // Reruns are byte-identical.
    let second = validate(dir, false);
    assert_eq!(first, second);
}

#[test]
fn strict_accepts_the_tenant_filtered_attachment() {
    let temp = scratch_with_attachment("valid/planner.queries.strict-safe.json");
    let dir = temp.path();
    let mut value = attachment_value(dir);
    repin_model(dir, &mut value);
    save_attachment(dir, &value);
    let envelope = validate(dir, true);
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["queryModel"]["queryCount"], 2);
}

#[test]
fn strict_refuses_the_tenant_filter_omission() {
    let temp = scratch_with_attachment("invalid/tenant-filter-missing.json");
    let dir = temp.path();
    let mut value = attachment_value(dir);
    repin_model(dir, &mut value);
    save_attachment(dir, &value);

    // The strict profile diagnoses the omission...
    let strict = validate_err(dir, true);
    assert!(strict.contains("query.tenant-filter-missing"), "{strict}");

    // ...while the default profile stays silent.
    let envelope = validate(dir, false);
    assert_eq!(envelope["status"], "valid");
}

#[test]
fn strict_refuses_tenant_scoped_includes_without_a_path_filter() {
    let temp = scratch_with_attachment("valid/planner.queries.includes.json");
    let dir = temp.path();
    let mut value = attachment_value(dir);
    repin_model(dir, &mut value);
    save_attachment(dir, &value);

    let default = validate(dir, false);
    assert_eq!(default["status"], "valid");
    let include_plan = default["plans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|plan| plan["query"] == "planner.tasks_by_project")
        .expect("include plan");
    let kinds: Vec<&str> = include_plan["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["filter", "project", "include"]);
    let include_step = include_plan["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["kind"] == "include")
        .expect("include step");
    assert_eq!(include_step["path"].as_array().unwrap().len(), 1);

    let strict = validate_err(dir, true);
    assert!(strict.contains("query.tenant-filter-missing"), "{strict}");
    assert!(strict.contains("include-tenant-unconstrained"), "{strict}");
}

#[test]
fn write_surfaces_never_back_a_query() {
    // An effect in the source slot is a write surface...
    let temp = scratch_with_attachment("invalid/write-source.json");
    let dir = temp.path();
    let mut value = attachment_value(dir);
    repin_model(dir, &mut value);
    save_attachment(dir, &value);
    let refusal = validate_err(dir, false);
    assert!(refusal.contains("query.source-invalid"), "{refusal}");

    // ...and a command in the query slot is refused the same way.
    let temp = scratch_with_attachment("invalid/write-query-symbol.json");
    let dir = temp.path();
    let mut value = attachment_value(dir);
    repin_model(dir, &mut value);
    save_attachment(dir, &value);
    let refusal = validate_err(dir, false);
    assert!(refusal.contains("query.source-invalid"), "{refusal}");
    assert!(refusal.contains("write-surface"), "{refusal}");
}

#[test]
fn model_dependent_refusals_stay_typed() {
    let cases: &[(&str, &str)] = &[
        ("invalid/sort-nondeterministic.json", "query.sort-invalid"),
        ("invalid/filter-unknown-member.json", "query.filter-invalid"),
        ("invalid/filter-type-mismatch.json", "query.filter-invalid"),
        (
            "invalid/selection-visibility.json",
            "query.visibility-boundary",
        ),
        ("invalid/source-not-read.json", "query.source-invalid"),
        (
            "invalid/include-segment-type.json",
            "query.reference-invalid",
        ),
    ];
    for (name, rule) in cases {
        let temp = scratch_with_attachment(name);
        let dir = temp.path();
        let mut value = attachment_value(dir);
        repin_model(dir, &mut value);
        save_attachment(dir, &value);
        let refusal = validate_err(dir, false);
        assert!(refusal.contains(rule), "{name}: {refusal}");
    }
}

#[test]
fn wire_level_refusals_fail_closed() {
    let temp = scratch_with_attachment("invalid/wrong-schema-version.json");
    let dir = temp.path();
    let refusal = validate_err(dir, false);
    assert!(refusal.contains("query.input-invalid"), "{refusal}");
}

#[test]
fn custody_refuses_a_foreign_or_drifted_model() {
    let temp = scratch_with_attachment("valid/planner.queries.json");
    let dir = temp.path();
    let mut value = attachment_value(dir);
    repin_model(dir, &mut value);
    save_attachment(dir, &value);

    // The project id must match the loaded project.
    let mut foreign_project = value.clone();
    foreign_project["projectId"] = json!("other");
    save_attachment(dir, &foreign_project);
    let refusal = validate_err(dir, false);
    assert!(refusal.contains("query.input-invalid"), "{refusal}");
    save_attachment(dir, &value);

    // A drifted model document breaks the digest custody.
    let entities = dir.join("lekalo/modules/planner/entities.yaml");
    let drifted = fs::read_to_string(&entities)
        .unwrap()
        .replace("A focusable task", "A focusable task (drifted)");
    fs::write(&entities, drifted).unwrap();
    let refusal = validate_err(dir, false);
    assert!(refusal.contains("query.input-invalid"), "{refusal}");
    assert!(refusal.contains("model-digest"), "{refusal}");
}

#[test]
fn diff_classifies_removal_breaking_and_filter_policy_change() {
    let temp = scratch();
    let dir = temp.path();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join("tests/fixtures/query-model/diff");
    fs::copy(fixtures.join("base.json"), dir.join("base.json")).unwrap();
    fs::copy(
        fixtures.join("candidate-weaker.json"),
        dir.join("candidate.json"),
    )
    .unwrap();

    let output = lekalo_in(
        dir,
        &[
            "--json",
            "query-model",
            "diff",
            "base.json",
            "candidate.json",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let diff: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(diff["status"], "valid");
    assert_eq!(diff["queryModelDiff"]["equal"], false);
    assert_eq!(diff["queryModelDiff"]["breaking"], 1);
    assert_eq!(diff["queryModelDiff"]["nonBreaking"], 0);
    assert_eq!(diff["queryModelDiff"]["policyChange"], 1);
    let paths = diff["queryModelDiff"]["paths"].as_array().unwrap();
    assert!(paths.iter().any(|path| {
        path["path"] == "queries/planner.count_focused" && path["class"] == "breaking"
    }));
    assert!(paths.iter().any(|path| {
        path["path"] == "queries/planner.tasks_today/filter" && path["class"] == "policy-change"
    }));

    // The verdict stays data: identical attachments compare equal and
    // the exit stays 0.
    let output = lekalo_in(
        dir,
        &["--json", "query-model", "diff", "base.json", "base.json"],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let diff: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(diff["queryModelDiff"]["equal"], true);
}

#[test]
fn mixed_revisions_refuse_the_comparison() {
    let temp = scratch();
    let dir = temp.path();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join("tests/fixtures/query-model/diff");
    fs::copy(fixtures.join("base.json"), dir.join("base.json")).unwrap();
    let mut candidate: Value =
        serde_json::from_slice(&fs::read(fixtures.join("candidate-weaker.json")).unwrap()).unwrap();
    candidate["attachmentRevision"] = json!("1.0.1");
    fs::write(
        dir.join("candidate.json"),
        serde_json::to_vec(&candidate).unwrap(),
    )
    .unwrap();
    let output = lekalo_in(
        dir,
        &[
            "--json",
            "query-model",
            "diff",
            "base.json",
            "candidate.json",
        ],
    );
    assert_eq!(exit_code(&output), 1, "{}", stderr_text(&output));
    assert!(output.stdout.is_empty(), "no partial success on stdout");
    let refusal = stderr_text(&output);
    assert!(refusal.contains("query.input-invalid"), "{refusal}");
}
