//! Issue #52 CLI battery: the read-only task-links surface over the
//! committed pilot fixtures. Every run is `--no-cache` (the published
//! read-only path) and asserts the exact DomainResult envelope, exit
//! code, and reason codes.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURES: &str = "tests/fixtures/task-external-links";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    let mut args = args.to_vec();
    args.insert(0, "--no-cache");
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run the real lekalo binary")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
}

fn fixture(relative: &str) -> String {
    format!("{FIXTURES}/{relative}")
}

fn project() -> String {
    fixture("project")
}

fn attachment() -> String {
    fixture("contracts/integration.json")
}

/// The valid attachment validates strict with exit 0 and the exact
/// counts block.
#[test]
fn valid_attachment_validates_strict_with_exact_counts() {
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "validate",
            &attachment(),
            "--project",
            &project(),
            "--profile",
            "strict",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_text(&output)
    );
    assert!(stderr_text(&output).is_empty());
    let document: Value = serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
    assert_eq!(document["status"], "valid");
    let report = &document["taskLinks"];
    assert_eq!(report["schemaVersion"], "lekalo/task-links-report/v0.8.0");
    assert_eq!(report["identity"], "dev.lekalo.task-links-report@0.8.0");
    assert_eq!(report["profile"], "strict");
    assert_eq!(report["status"], "valid");
    assert_eq!(report["projectId"], "tasklinks");
    assert_eq!(report["modelRef"]["schemaVersion"], "0.2.16");
    assert_eq!(report["checks"]["relationsChecked"], 2);
    assert_eq!(report["checks"]["uniqueConstraintsChecked"], 1);
    assert_eq!(report["checks"]["providerContractsChecked"], 2);
    assert_eq!(report["checks"]["findings"], 0);
    assert_eq!(report["findings"], serde_json::json!([]));
}

/// Repeated strict runs are byte-identical (determinism).
#[test]
fn strict_validate_is_byte_identical_across_reruns() {
    let args = [
        "--json",
        "task-links",
        "validate",
        attachment().leak(),
        "--project",
        project().leak(),
        "--profile",
        "strict",
    ];
    let first = lekalo_in(&workspace_path("."), &args);
    let second = lekalo_in(&workspace_path("."), &args);
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(stdout_text(&first), stdout_text(&second));
}

/// A declaration defect invalidates under BOTH profiles: permissive
/// softens only the missing/unsupported required-guarantee class
/// (LEK-TASKLINK-007, witnessed by the portability gate), never a
/// broken declaration. Permissive therefore never claims pass here.
#[test]
fn declaration_defects_invalidate_under_both_profiles() {
    let target = workspace_path("target/task-links-defect-copy.json");
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let original =
        std::fs::read_to_string(workspace_path(attachment().as_str())).expect("attachment");
    let defect = original.replace(
        "\"policyRef\": \"tasklinks.policy.request_action_member\"",
        "\"policyRef\": \"tasklinks.policy.missing_owner\"",
    );
    std::fs::write(&target, defect).expect("write defect copy");
    let defect_path = target.to_string_lossy().into_owned();
    for profile in ["strict", "permissive"] {
        let output = lekalo_in(
            &workspace_path("."),
            &[
                "--json",
                "task-links",
                "validate",
                &defect_path,
                "--project",
                &project(),
                "--profile",
                profile,
            ],
        );
        assert_eq!(output.status.code(), Some(1), "{profile} invalidates");
        let envelope: Value =
            serde_json::from_str(stderr_text(&output).trim()).expect("{profile} envelope");
        assert_eq!(envelope["status"], "invalid", "{profile}");
        assert_eq!(
            envelope["reasonCodes"][0], "tasklink.decision-incomplete",
            "{profile}"
        );
    }
    let _ = std::fs::remove_file(&target);
}

/// An unknown project id fails closed before any finding is computed.
#[test]
fn project_mismatch_fails_closed() {
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "validate",
            &attachment(),
            "--project",
            "tests/fixtures/authorization/planner",
            "--profile",
            "strict",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(envelope["status"], "invalid");
    assert_eq!(
        envelope["reasonCodes"][0],
        "tasklink.relation-identity-invalid"
    );
}

/// The typed inspect view shows relations, constraints, decisions, and
/// unavailable evidence; with admitted evidence the pins match.
#[test]
fn inspect_projects_the_typed_view_and_admits_evidence() {
    let without = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "inspect",
            "tasklinks.task_external_link",
            "--project",
            &project(),
            "--attachment",
            &attachment(),
        ],
    );
    assert_eq!(without.status.code(), Some(0));
    let view: Value = serde_json::from_str(stdout_text(&without).trim()).expect("view envelope");
    assert_eq!(view["status"], "valid");
    let payload = &view["taskLinks"];
    assert_eq!(
        payload["identity"],
        "dev.lekalo.task-links-integration@0.8.0"
    );
    let relations = payload["relations"].as_array().expect("relations");
    assert_eq!(relations.len(), 2);
    assert!(relations
        .iter()
        .any(|row| row["id"] == "tasklinks.rel.task_links"
            && row["cardinality"] == "0..N"
            && row["on_detach"] == "retain-history"));
    let constraints = payload["unique_constraints"]
        .as_array()
        .expect("constraints");
    assert_eq!(constraints[0]["id"], "tasklinks.uq_external_identity");
    assert_eq!(
        constraints[0]["fields"],
        serde_json::json!([
            "workspace_id",
            "installation_id",
            "provider_id",
            "object_type",
            "external_id"
        ])
    );
    assert_eq!(
        payload["evidence"],
        Value::Null,
        "missing evidence stays unavailable"
    );

    let evidence = fixture("evidence/mock-run.json");
    let with = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "inspect",
            "tasklinks.task_external_link",
            "--project",
            &project(),
            "--attachment",
            &attachment(),
            "--evidence",
            &evidence,
        ],
    );
    assert_eq!(with.status.code(), Some(0));
    let admitted: Value =
        serde_json::from_str(stdout_text(&with).trim()).expect("admitted envelope");
    let summary = &admitted["taskLinks"]["evidence"];
    assert_eq!(summary["origin"], "mock");
    assert_eq!(summary["pins_match"], true);
    assert_eq!(summary["steps"], 3);
}

/// Stale evidence pins fail closed with LEK-TASKLINK-008.
#[test]
fn stale_evidence_fails_closed() {
    let target = workspace_path("target/task-links-stale-evidence.json");
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let original =
        std::fs::read_to_string(workspace_path(fixture("evidence/mock-run.json").as_str()))
            .expect("evidence");
    let stale = original.replace(
        "sha256:52524a4f1c8e0e5e05ee1e51d3dbb986a2a1e5d51b3a3b2f8e6f5f3b3f6b6a01",
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    );
    std::fs::write(&target, stale).expect("write stale copy");
    let stale_path = target.to_string_lossy().into_owned();
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "inspect",
            "tasklinks.task_external_link",
            "--project",
            &project(),
            "--attachment",
            &attachment(),
            "--evidence",
            &stale_path,
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(envelope["status"], "invalid");
    assert_eq!(envelope["reasonCodes"][0], "tasklink.evidence-invalid");
    let _ = std::fs::remove_file(&target);
}

/// Portability separates the core rows from provider and target rows;
/// strict refuses the un-evidenced target guarantees.
#[test]
fn portability_separates_sections_and_strict_refuses_unknown_required() {
    let from = fixture("profiles/laravel.json");
    let to = fixture("profiles/node.json");
    let base = [
        "--json",
        "task-links",
        "portability",
        attachment().leak(),
        "--project",
        project().leak(),
        "--from-profile",
        from.as_str(),
        "--to-profile",
        to.as_str(),
    ];
    let permissive = lekalo_in(
        &workspace_path("."),
        &[base.as_slice(), &["--profile", "permissive"]].concat(),
    );
    assert_eq!(permissive.status.code(), Some(0));
    let document: Value =
        serde_json::from_str(stdout_text(&permissive).trim()).expect("portability envelope");
    let report = &document["taskLinks"];
    assert_eq!(
        report["schemaVersion"],
        "lekalo/task-links-portability/v0.8.0"
    );
    assert_eq!(report["fromProfile"]["profileId"], "php-laravel-maintained");
    assert_eq!(report["toProfile"]["profileId"], "node-typescript");
    let core = report["coreSemantics"].as_array().expect("core rows");
    assert_eq!(core.len(), 6);
    assert!(core
        .iter()
        .all(|row| row["provided"] == "supported" && row["requirement"] == "required"));
    let provider_rows = report["providerAdapters"]
        .as_array()
        .expect("provider rows");
    assert!(provider_rows.len() >= 6);
    assert!(report["targetGuarantees"]
        .as_array()
        .expect("target rows")
        .iter()
        .any(|row| row["fact"] == "durable-outbox" && row["provided"] == "unknown"));
    assert_eq!(report["evidenceCoverage"]["unknownRequired"], 3);
    // Permissive keeps the envelope valid while naming every gap.
    let gap_rules: Vec<&str> = document["reasonCodes"]
        .as_array()
        .map(|codes| codes.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    assert!(gap_rules.contains(&"tasklink.required-guarantee-unsupported"));

    let strict = lekalo_in(
        &workspace_path("."),
        &[base.as_slice(), &["--profile", "strict"]].concat(),
    );
    assert_eq!(
        strict.status.code(),
        Some(1),
        "strict denies unknown required"
    );
    let envelope: Value = serde_json::from_str(stderr_text(&strict).trim()).expect("strict");
    assert_eq!(envelope["status"], "invalid");
    assert!(envelope["reasonCodes"]
        .as_array()
        .expect("codes")
        .iter()
        .any(|code| code == "tasklink.required-guarantee-unsupported"));
}

/// A provider-only revision change preserves every core row byte.
#[test]
fn provider_only_change_preserves_core_rows() {
    let target = workspace_path("target/task-links-provider-changed.json");
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let original =
        std::fs::read_to_string(workspace_path(attachment().as_str())).expect("attachment");
    let changed = original.replace(
        "\"upstreamPin\": \"github-rest/2022-11-28\"",
        "\"upstreamPin\": \"github-rest/2026-01-01\"",
    );
    std::fs::write(&target, changed).expect("write changed copy");
    let changed_path = target.to_string_lossy().into_owned();
    let from = fixture("profiles/laravel.json");
    let to = fixture("profiles/node.json");
    let run = |path: &str| {
        let output = lekalo_in(
            &workspace_path("."),
            &[
                "--json",
                "task-links",
                "portability",
                path,
                "--project",
                &project(),
                "--from-profile",
                &from,
                "--to-profile",
                &to,
                "--profile",
                "permissive",
            ],
        );
        assert_eq!(output.status.code(), Some(0));
        let document: Value = serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
        document["taskLinks"].clone()
    };
    let before = run(&attachment());
    let after = run(&changed_path);
    assert_eq!(
        before["coreSemantics"], after["coreSemantics"],
        "core rows are byte-identical under a provider-only change"
    );
    assert_ne!(
        before["providerAdapters"], after["providerAdapters"],
        "adapter rows move with the provider contract"
    );
    let _ = std::fs::remove_file(&target);
}

/// A wrong profile value is a usage-shape invalid, not a panic.
#[test]
fn unknown_profile_is_rejected() {
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "validate",
            &attachment(),
            "--project",
            &project(),
            "--profile",
            "yolo",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(envelope["status"], "invalid");
    assert_eq!(
        envelope["reasonCodes"][0],
        "tasklink.relation-identity-invalid"
    );
}

/// A missing attachment file fails closed with the bounded input rule.
#[test]
fn missing_attachment_fails_closed() {
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "validate",
            "tests/fixtures/task-external-links/contracts/does-not-exist.json",
            "--project",
            &project(),
            "--profile",
            "strict",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(
        envelope["reasonCodes"][0],
        "tasklink.relation-identity-invalid"
    );
}
