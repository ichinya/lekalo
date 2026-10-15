//! Adversarial regressions for r4 execution custody and authoritative effect joins.
use lekalo_core::{
    effects, extended_effects::ExtendedEffectsAttachment, ir, loader, task_links as tl,
    transaction_concurrency::TransactionConcurrencyAttachment,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn document(name: &str) -> Vec<u8> {
    std::fs::read(root().join("tests/fixtures/task-external-links").join(name)).unwrap()
}
static CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn fixture() -> (ir::Compilation, tl::Attachment) {
    let _guard = CWD.lock().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(root()).unwrap();
    let source = loader::normalize_model(&loader::LoadSelection {
        project: Some("tests/fixtures/task-external-links/project".to_owned()),
    });
    std::env::set_current_dir(original).unwrap();
    (
        ir::compile(&source.unwrap()).unwrap(),
        tl::parse(&document("contracts/integration.json")).unwrap(),
    )
}
fn profile(id: &str) -> tl::portability::ProfileRef {
    tl::portability::ProfileRef {
        profile_id: id.to_owned(),
        digest: format!("sha256:{}", "a".repeat(64)),
    }
}
fn repin(mut value: Value) -> tl::Attachment {
    value["attachmentRevision"] =
        Value::String(tl::pins::compute_revision(&serde_json::to_vec(&value).unwrap()).unwrap());
    tl::parse(&serde_json::to_vec(&value).unwrap()).unwrap()
}
fn owners() -> (TransactionConcurrencyAttachment, ExtendedEffectsAttachment) {
    (
        TransactionConcurrencyAttachment::from_value(
            &tl::json_parse(&document("contracts/transaction-concurrency.json")).unwrap(),
        )
        .unwrap(),
        ExtendedEffectsAttachment::from_value(
            &tl::json_parse(&document("contracts/extended-effects.json")).unwrap(),
        )
        .unwrap(),
    )
}

#[test]
fn producer_success_steps_and_native_booleans_never_authenticate_execution() {
    let (compilation, attachment) = fixture();
    let bytes = document("evidence/mock-run.json");
    let declared = tl::admit_evidence(&bytes, &attachment, &compilation, true).unwrap();
    assert!(declared.executed.is_empty());
    // Even a public caller filling every legacy execution/qualification field
    // cannot enter the sealed owner channel or promote a guarantee.
    let evidence = tl::portability::AdmittedEvidence {
        evidence_id: declared.evidence_id,
        origin: "native".to_owned(),
        scenario_id: declared.scenario_id,
        executed: BTreeSet::from([
            "tasklinks.request_action".to_owned(),
            "tasklinks.dispatch_outbox".to_owned(),
        ]),
    };
    let native = tl::portability::NativeEvidence {
        legs: BTreeSet::from(["synchronized_identity_race_binds_one_task".to_owned()]),
        receipt_digest: format!("sha256:{}", "0".repeat(64)),
        target_implementation: "php-laravel-maintained".to_owned(),
        target_profile: "cargo-test".to_owned(),
    };
    let report = tl::portability::report(
        &attachment,
        profile("php-laravel-maintained"),
        profile("php-laravel-maintained"),
        "permissive",
        &[evidence],
        &BTreeMap::new(),
        Some(&native),
    );
    assert!(report
        .core_semantics
        .iter()
        .all(|row| row.provided == tl::portability::Provided::Unknown));
    assert!(report
        .target_guarantees
        .iter()
        .filter(|row| row.fact != "delivery-uncertainty")
        .all(|row| row.provided == tl::portability::Provided::Unknown));
    assert!(
        report.execution_provenance.is_empty()
            && report.evidence_coverage.native_evidence_ids.is_empty()
    );
    let mut forged: Value = tl::json_parse(&bytes).unwrap();
    forged["origin"] = json!("native");
    forged["clock"]["policy"] = json!("wall-clock-native");
    assert!(tl::admit_evidence(
        &serde_json::to_vec(&forged).unwrap(),
        &attachment,
        &compilation,
        true
    )
    .is_err());
}

#[test]
fn sealed_execution_checks_all_ten_scenarios_and_binds_input_build_result() {
    let (compilation, attachment) = fixture();
    let first = tl::execution::run_mock(&attachment, &compilation).unwrap();
    let second = tl::execution::run_mock(&attachment, &compilation).unwrap();
    assert_eq!(first.report().result_digest, second.report().result_digest);
    assert_eq!(first.report().input_digest, second.report().input_digest);
    assert_eq!(first.report().scenarios.len(), 10);
    assert!(first
        .report()
        .assertions
        .contains(&"failed-only-retry-preserves-success-receipt".to_owned()));
    assert!(first
        .report()
        .steps
        .iter()
        .any(|step| step.outcome == "unknown_delivery"));
    let report = tl::portability::report_with_execution(
        &attachment,
        profile("node-typescript"),
        profile("php-laravel-maintained"),
        "permissive",
        &[],
        std::slice::from_ref(&first),
    );
    assert!(report
        .core_semantics
        .iter()
        .all(|row| row.provided == tl::portability::Provided::Supported
            && row.evidence_source == tl::portability::EvidenceSource::MockEvidence));
    // Mocks never supply a target-native guarantee.
    assert!(report
        .target_guarantees
        .iter()
        .filter(|row| row.fact != "delivery-uncertainty")
        .all(|row| row.provided == tl::portability::Provided::Unknown));
    assert_eq!(report.execution_provenance.len(), 1);
    assert_eq!(
        report.execution_provenance[0].source_digest,
        tl::pins::ir_digest(&compilation)
    );
    let mut changed = tl::json_parse(&document("contracts/integration.json")).unwrap();
    changed["providerContracts"][0]["upstreamPin"] = json!("github-rest/different-input");
    let changed = repin(changed);
    assert!(
        !first.witnesses(&changed, "actor-and-field-policy"),
        "a real receipt cannot be transferred to other inputs"
    );
    let changed_run = tl::execution::run_mock(&changed, &compilation).unwrap();
    assert_ne!(
        first.report().input_digest,
        changed_run.report().input_digest
    );
    // A serialized owner export cannot be read back as a fabricated evidence file.
    assert!(tl::admit_evidence(
        &serde_json::to_vec(first.report()).unwrap(),
        &attachment,
        &compilation,
        true
    )
    .is_err());
}

#[test]
fn executed_effects_require_exact_same_compilation_transaction_and_case_owners() {
    let (compilation, attachment) = fixture();
    let run = tl::execution::run_mock(&attachment, &compilation).unwrap();
    let graph = effects::build(&compilation.project).unwrap();
    let (transactions, extended) = owners();
    let joined = graph
        .with_execution(&run, &transactions, &extended)
        .unwrap();
    assert!(joined
        .detected()
        .iter()
        .all(|edge| edge.confidence() == effects::Confidence::Verified));
    assert!(joined
        .detected()
        .iter()
        .filter(|edge| edge.key().kind() == effects::EffectKind::ExternalCall)
        .all(|edge| edge.transaction_group().is_none()));
    let request = effects::OperationId::from_semantic("tasklinks.request_action").unwrap();
    assert_eq!(
        joined
            .declared()
            .iter()
            .filter(|edge| edge.key().operation() == &request && edge.transaction_group().is_some())
            .count(),
        4,
        "task, decision, outbox, audit canonical edges all join the transaction owner"
    );
    let payload: Value =
        serde_json::from_str(&effects::canonical::show_payload_bytes(&joined, &request).unwrap())
            .unwrap();
    assert_eq!(
        payload["transactions"][0]["basis"],
        "owner-admitted-execution"
    );
    let dispatch = effects::OperationId::from_semantic("tasklinks.dispatch_outbox").unwrap();
    let payload: Value =
        serde_json::from_str(&effects::canonical::show_payload_bytes(&joined, &dispatch).unwrap())
            .unwrap();
    assert_eq!(
        payload["partialFailures"][0]["basis"],
        "owner-admitted-execution"
    );
    assert_eq!(
        payload["partialFailures"][0]["retryEligible"],
        json!(["tasklinks.provider/mock-secondary"])
    );
    assert!(
        joined
            .with_detected_evidence(
                &effects::envelope::from_json(&document("contracts/detected-effects.json"))
                    .unwrap()
            )
            .is_err(),
        "untrusted envelopes cannot inherit an executed graph's custody"
    );
    for mutation in ["group-name", "omitted-audit", "stale-pin"] {
        let mut wire = tl::json_parse(&document("contracts/transaction-concurrency.json")).unwrap();
        match mutation {
            "group-name" => {
                wire["operations"][0]["atomicEffectGroups"][0]["groupId"] =
                    json!("tasklinks/invented-group")
            }
            "omitted-audit" => {
                let members = wire["operations"][0]["atomicEffectGroups"][0]["effectRefs"]
                    .as_array_mut()
                    .unwrap();
                let before = members.len();
                members
                    .retain(|member| !member.as_str().unwrap().contains("tasklinks.action_audit"));
                assert_eq!(
                    members.len(),
                    before - 1,
                    "the audit mutation must remove an actual canonical member"
                );
            }
            _ => wire["irRef"]["digest"] = json!(format!("sha256:{}", "0".repeat(64))),
        }
        let owner = TransactionConcurrencyAttachment::from_value(&wire).unwrap();
        assert!(
            graph.with_execution(&run, &owner, &extended).is_err(),
            "{mutation}"
        );
    }
    for mutation in [
        "case-name",
        "case-pin",
        "request-schema",
        "response-schema",
        "job-schema",
        "successful-failure",
    ] {
        let mut wire = tl::json_parse(&document("contracts/extended-effects.json")).unwrap();
        match mutation {
            "case-name" => {
                wire["partialFailureCases"][0]["caseId"] = json!("tasklinks.case/invented-case")
            }
            "case-pin" => {
                wire["partialFailureCases"][0]["scenarioRef"]["irDigest"] =
                    json!(format!("sha256:{}", "0".repeat(64)))
            }
            "request-schema" => {
                wire["calls"][0]["requestDigest"] = json!(format!("sha256:{}", "0".repeat(64)))
            }
            "response-schema" => {
                wire["calls"][0]["responseDigest"] = json!(format!("sha256:{}", "0".repeat(64)))
            }
            "job-schema" => {
                wire["jobs"][0]["payloadDigest"] = json!(format!("sha256:{}", "0".repeat(64)))
            }
            _ => {
                wire["partialFailureCases"][0]["expectedOutcomes"][1]["outcome"] = json!("success")
            }
        }
        match ExtendedEffectsAttachment::from_value(&wire) {
            Ok(owner) => assert!(
                graph.with_execution(&run, &transactions, &owner).is_err(),
                "{mutation}"
            ),
            Err(_) => assert_eq!(mutation, "successful-failure"),
        }
    }
}

#[test]
fn recomputable_envelope_digest_is_integrity_not_execution_provenance() {
    let (compilation, _) = fixture();
    let mut value = tl::json_parse(&document("contracts/detected-effects.json")).unwrap();
    value["entries"][0]["subject"] = json!("external-service:attacker/exfil-endpoint");
    assert!(effects::envelope::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut payload = value.clone();
    payload.as_object_mut().unwrap().remove("evidenceDigest");
    value["evidenceDigest"] =
        json!(tl::wire_digest(&serde_json::to_vec(&payload).unwrap()).as_str());
    let envelope = effects::envelope::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
    let graph = effects::build(&compilation.project)
        .unwrap()
        .with_detected_evidence(&envelope)
        .unwrap();
    assert!(graph
        .detected()
        .iter()
        .all(|edge| edge.confidence() != effects::Confidence::Verified));
    assert!(graph
        .with_execution(
            &tl::execution::run_mock(&fixture().1, &compilation).unwrap(),
            &owners().0,
            &owners().1
        )
        .is_err());
}

#[test]
fn owner_successor_canonicalization_preserves_negotiated_identities_and_refuses_aliases() {
    let (transactions, extended) = owners();
    for bytes in [
        transactions.canonical_bytes().unwrap(),
        extended.canonical_bytes().unwrap(),
    ] {
        let value: Value = serde_json::from_str(&bytes).unwrap();
        assert_eq!(value["modelRef"]["modelVersion"], "0.8.0");
        assert_eq!(value["irRef"]["identity"], "dev.lekalo.ir@0.8.0");
        assert!(value["identity"].as_str().unwrap().ends_with("@0.8.0"));
    }
    let mut wire = tl::json_parse(&document("contracts/extended-effects.json")).unwrap();
    wire["schemaVersion"] = json!("lekalo/extended-effects/v0.2.16");
    wire["identity"] = json!("dev.lekalo.extended-effects@0.2.16");
    assert!(ExtendedEffectsAttachment::from_value(&wire).is_err());
}
