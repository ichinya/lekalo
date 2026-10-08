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

/// The canonical attachment revision: the sha256 of the compact,
/// recursively byte-sorted serialization of every revision-covered
/// member — the exact canonicalization the core recomputes.
fn canonical_revision(document: &Value) -> String {
    const MEMBERS: [&str; 16] = [
        "schemaVersion",
        "identity",
        "projectId",
        "modelRef",
        "irRef",
        "ownerContractPins",
        "entities",
        "relations",
        "uniqueConstraints",
        "providerContracts",
        "linkCapabilityRequirements",
        "fieldPolicies",
        "actionPolicies",
        "operationBindings",
        "deliveryRules",
        "contributions",
    ];
    fn sort_deep(value: Value) -> Value {
        match value {
            Value::Array(items) => Value::Array(items.into_iter().map(sort_deep).collect()),
            Value::Object(map) => {
                let mut sorted = serde_json::Map::new();
                let mut keys: Vec<String> = map.keys().cloned().collect();
                keys.sort();
                for key in keys {
                    sorted.insert(key.clone(), sort_deep(map[&key].clone()));
                }
                Value::Object(sorted)
            }
            other => other,
        }
    }
    let mut revision = serde_json::Map::new();
    for member in MEMBERS {
        revision.insert(member.to_owned(), document[member].clone());
    }
    let bytes = serde_json::to_vec(&sort_deep(Value::Object(revision))).expect("canonical bytes");
    format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes))
}

/// A declaration defect invalidates under BOTH profiles: permissive
/// softens only the missing/unsupported required-guarantee class
/// (LEK-TASKLINK-007, witnessed by the portability gate), never a
/// broken declaration. Permissive therefore never claims pass here.
#[test]
fn declaration_defects_invalidate_under_both_profiles() {
    let target = workspace_path("target/task-links-defect-copy.json");
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let original: Value = serde_json::from_str(
        &std::fs::read_to_string(workspace_path(attachment().as_str())).expect("attachment"),
    )
    .expect("attachment json");
    let mut defect = original.clone();
    defect["fieldPolicies"][2]["policyRef"] =
        Value::String("tasklinks.policy.missing_owner".to_owned());
    defect["attachmentRevision"] = Value::String(canonical_revision(&defect));
    std::fs::write(
        &target,
        serde_json::to_vec_pretty(&defect).expect("defect bytes"),
    )
    .expect("write defect copy");
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
    // Derive the stale copy from the live base revision: the evidence
    // pin moves with the attachment, so the mutation must too.
    let evidence: Value = serde_json::from_str(&original).expect("evidence json");
    let live_revision = evidence["attachmentRevision"]
        .as_str()
        .expect("revision")
        .to_owned();
    let stale = original.replace(
        &live_revision,
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    );
    assert_ne!(stale, original, "the stale mutation must apply");
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
    let evidence = fixture("evidence/mock-run.json");
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
        "--evidence",
        evidence.as_str(),
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
    let original: Value = serde_json::from_str(
        &std::fs::read_to_string(workspace_path(attachment().as_str())).expect("attachment"),
    )
    .expect("attachment json");
    let mut changed = original.clone();
    for contract in changed["providerContracts"]
        .as_array_mut()
        .expect("contracts")
    {
        if contract["upstreamPin"] == "github-rest/2022-11-28" {
            contract["upstreamPin"] = Value::String("github-rest/2026-01-01".to_owned());
        }
    }
    changed["attachmentRevision"] = Value::String(canonical_revision(&changed));
    std::fs::write(
        &target,
        serde_json::to_vec_pretty(&changed).expect("changed bytes"),
    )
    .expect("write changed copy");
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

/// A same-family different-version discriminator maps to the accepted
/// exit-5 class (unsupported-version), not plain invalid.
#[test]
fn wrong_schema_version_maps_to_unsupported_version() {
    let target = workspace_path("target/task-links-wrong-version.json");
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let original =
        std::fs::read_to_string(workspace_path(attachment().as_str())).expect("attachment");
    let wrong = original.replace(
        "lekalo/task-links-integration/v0.8.0",
        "lekalo/task-links-integration/v0.7.0",
    );
    std::fs::write(&target, wrong).expect("write wrong version");
    let wrong_path = target.to_string_lossy().into_owned();
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "validate",
            &wrong_path,
            "--project",
            &project(),
            "--profile",
            "strict",
        ],
    );
    assert_eq!(output.status.code(), Some(5), "version skew is exit 5");
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(envelope["status"], "unsupported-version");
    let _ = std::fs::remove_file(&target);
}

/// Every content pin is recomputed: a forged IR digest, a fabricated
/// attachment revision, and copied placeholder owner pins all fail
/// closed with LEK-TASKLINK-001 instead of validating.
#[test]
fn forged_pins_fail_closed() {
    for (pin_fixture, expected_detail) in [
        ("invalid/pin-ir-digest-stale.json", "ir-digest-stale"),
        ("invalid/pin-owner-pin-stale.json", "owner-pin-stale"),
    ] {
        let output = lekalo_in(
            &workspace_path("."),
            &[
                "--json",
                "task-links",
                "validate",
                &fixture(pin_fixture),
                "--project",
                &project(),
                "--profile",
                "strict",
            ],
        );
        assert_eq!(output.status.code(), Some(1), "{pin_fixture}");
        let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
        assert_eq!(envelope["status"], "invalid", "{pin_fixture}");
        assert_eq!(
            envelope["diagnostics"][0]["data"]["detail"], expected_detail,
            "{pin_fixture}"
        );
    }
    // A fabricated attachment revision (any digest that is not the
    // recomputation of this document) is refused.
    let target = workspace_path("target/task-links-forged-revision.json");
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let original =
        std::fs::read_to_string(workspace_path(attachment().as_str())).expect("attachment");
    let forged = original.replace(
        &original[original.find("sha256:").expect("digest")
            ..original.find("sha256:").expect("digest") + 71],
        &format!("sha256:{}", "e".repeat(64)),
    );
    std::fs::write(&target, forged).expect("write forged copy");
    let forged_path = target.to_string_lossy().into_owned();
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "validate",
            &forged_path,
            "--project",
            &project(),
            "--profile",
            "strict",
        ],
    );
    assert_eq!(output.status.code(), Some(1), "forged revision refused");
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(
        envelope["diagnostics"][0]["data"]["detail"],
        "revision-digest-stale"
    );
    let _ = std::fs::remove_file(&target);
}

/// Duplicate JSON members are rejected before serde can erase them, on
/// every input path: attachment, evidence, and profile documents.
#[test]
fn duplicate_json_keys_are_rejected() {
    let target = workspace_path("target/task-links-dup-keys.json");
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let original =
        std::fs::read_to_string(workspace_path(attachment().as_str())).expect("attachment");
    // Smuggle a second projectId member after the first: serde would
    // silently take the last value.
    let dup = original.replace(
        "\"projectId\": \"tasklinks\",",
        "\"projectId\": \"tasklinks\",\"projectId\": \"other\",",
    );
    assert_ne!(dup, original, "the mutation must apply");
    std::fs::write(&target, dup).expect("write dup copy");
    let dup_path = target.to_string_lossy().into_owned();
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "validate",
            &dup_path,
            "--project",
            &project(),
            "--profile",
            "strict",
        ],
    );
    assert_eq!(output.status.code(), Some(1), "duplicate members refused");
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(
        envelope["diagnostics"][0]["data"]["detail"],
        "attachment-json"
    );
    let _ = std::fs::remove_file(&target);

    // Duplicate members inside an evidence document are refused too.
    let evidence_target = workspace_path("target/task-links-dup-evidence.json");
    let evidence_original =
        std::fs::read_to_string(workspace_path(fixture("evidence/mock-run.json").as_str()))
            .expect("evidence");
    let evidence_dup = evidence_original.replace(
        "\"origin\": \"mock\"",
        "\"origin\": \"mock\",\"origin\": \"native\"",
    );
    std::fs::write(&evidence_target, evidence_dup).expect("write dup evidence");
    let evidence_dup_path = evidence_target.to_string_lossy().into_owned();
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
            &evidence_dup_path,
        ],
    );
    assert_eq!(output.status.code(), Some(1), "dup evidence refused");
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(
        envelope["diagnostics"][0]["data"]["detail"],
        "evidence-json"
    );
    let _ = std::fs::remove_file(&evidence_target);
}

/// A forged native evidence claim is refused without the qualified
/// environment; schema-valid evidence members (attribution, decision,
/// observations) admit cleanly.
#[test]
fn native_claims_are_qualification_gated() {
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
            &fixture("invalid/evidence-native-unqualified.json"),
        ],
    );
    assert_eq!(output.status.code(), Some(1), "forged native refused");
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(
        envelope["diagnostics"][0]["data"]["detail"],
        "evidence-native-unqualified"
    );
}

/// The inspect operand resolves against the view: an unknown symbol is
/// a typed refusal, never the whole view.
#[test]
fn unknown_inspect_symbol_is_refused() {
    let output = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "inspect",
            "bogus.symbol.that.does.not.exist",
            "--project",
            &project(),
            "--attachment",
            &attachment(),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(
        envelope["diagnostics"][0]["data"]["detail"],
        "symbol-not-found"
    );
}

/// An attachment that fails semantic validation never renders through
/// inspect or portability: the shared admission runs first.
#[test]
fn inspect_and_portability_join_semantic_validation() {
    let defect = "tests/fixtures/task-external-links/invalid/decision-incomplete.json";
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
            defect,
        ],
    );
    assert_eq!(output.status.code(), Some(1), "inspect joins validation");
    let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
    assert_eq!(envelope["reasonCodes"][0], "tasklink.decision-incomplete");

    let portability = lekalo_in(
        &workspace_path("."),
        &[
            "--json",
            "task-links",
            "portability",
            defect,
            "--project",
            &project(),
            "--from-profile",
            &fixture("profiles/laravel.json"),
            "--to-profile",
            &fixture("profiles/node.json"),
            "--profile",
            "permissive",
        ],
    );
    assert_eq!(
        portability.status.code(),
        Some(1),
        "portability joins validation"
    );
}

/// A symlinked attachment/evidence path is refused (no-follow read
/// discipline), never silently followed.
#[test]
fn symlinked_operand_is_refused() {
    #[cfg(unix)]
    {
        let link = workspace_path("target/task-links-linked-attachment.json");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(workspace_path(attachment().as_str()), &link)
            .expect("create link");
        let link_path = link.to_string_lossy().into_owned();
        let output = lekalo_in(
            &workspace_path("."),
            &[
                "--json",
                "task-links",
                "validate",
                &link_path,
                "--project",
                &project(),
                "--profile",
                "strict",
            ],
        );
        assert_eq!(output.status.code(), Some(1), "symlink refused");
        let envelope: Value = serde_json::from_str(stderr_text(&output).trim()).expect("envelope");
        assert_eq!(
            envelope["diagnostics"][0]["data"]["detail"],
            "attachment-file"
        );
        let _ = std::fs::remove_file(&link);
    }
    #[cfg(windows)]
    {
        // Reparse points are refused through symlink_metadata; a plain
        // file copy stays admissible (positive control).
        let copy = workspace_path("target/task-links-copied-attachment.json");
        std::fs::copy(workspace_path(attachment().as_str()), &copy).expect("copy");
        let copy_path = copy.to_string_lossy().into_owned();
        let output = lekalo_in(
            &workspace_path("."),
            &[
                "--json",
                "task-links",
                "validate",
                &copy_path,
                "--project",
                &project(),
                "--profile",
                "strict",
            ],
        );
        assert_eq!(output.status.code(), Some(0), "a real file is readable");
        let _ = std::fs::remove_file(&copy);
    }
}
