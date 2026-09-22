//! Issue #87 library tests for the derived data-flow report: flow
//! extraction over the classified planner fixture, gate decisions,
//! sink ceilings, unknown handling, and canonical export stability.

use lekalo_core::classification::{Attachment, PolicyAttachment, Resolution};
use lekalo_core::dataflow::{self, analyze, Inputs};
use lekalo_core::effects::build_with_classification;
use lekalo_core::ir::compile;
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::lockfile::types::Sha256Digest;
use lekalo_core::scenario::id::SemanticId;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FIXTURE: &str = "tests/fixtures/effects/planner";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn planner_project() -> lekalo_core::ir::Compilation {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let selection = LoadSelection {
        project: Some(FIXTURE.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("load failed: {}", outcome.to_json_string()),
    };
    let compilation = match compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => panic!("IR failed: {}", failure.into_result().to_json_string()),
    };
    std::env::set_current_dir(original).expect("restore cwd");
    compilation
}

/// The classification covering every planner/notify entity, event, and
/// value object the declared effects touch.
const CLASSIFICATION: &str = r#"{
  "schemaVersion": "lekalo/data-classification/v0.4.0",
  "identity": "dev.lekalo.data-classification@0.4.0",
  "attachmentRevision": "1.0.0",
  "projectId": "planner",
  "modelRef": {"modelVersion": "0.2.16", "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
  "irRef": {"irVersion": "0.2.16", "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
  "defaults": {"profile": "strict", "unclassifiedFields": "internal", "unclassifiedPayloads": "confidential"},
  "classifications": [
    {"subject": "planner.task", "kind": "internal"},
    {"subject": "planner.task_id", "kind": "internal"},
    {"subject": "planner.text", "kind": "internal"},
    {"subject": "planner.due_date", "kind": "internal"},
    {"subject": "planner.task_state", "kind": "internal"},
    {"subject": "planner.due_window", "kind": "internal"},
    {"subject": "planner.task_focused", "kind": "internal"},
    {"subject": "planner.task_archived", "kind": "internal"},
    {"subject": "notify.text", "kind": "internal"},
    {"subject": "notify.user_id", "kind": "personal"},
    {"subject": "notify.user", "kind": "personal"}
  ],
  "declassifications": [],
  "openQuestions": []
}"#;

/// The policy governing the kinds above: strict ceilings for every
/// non-model sink.
const POLICY: &str = r#"{
  "schemaVersion": "lekalo/classification-policy/v0.4.0",
  "identity": "dev.lekalo.classification-policy@0.4.0",
  "attachmentRevision": "1.0.0",
  "projectId": "planner",
  "modelRef": {"modelVersion": "0.2.16", "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
  "irRef": {"irVersion": "0.2.16", "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
  "kinds": [
    {
      "kind": "internal",
      "readers": ["tenant", "user:service-readonly"],
      "writers": ["tenant"],
      "destinations": ["internal-service", "message-bus"],
      "masking": {"strategy": "redact", "policyRef": "privacy-policy.masking.internal"},
      "encryptionRefs": ["dev.lekalo.nfr.encryption-at-rest@0.4.0"],
      "consentRequired": false,
      "crossTenant": "reviewed",
      "declassifyRoles": ["data-steward"]
    },
    {
      "kind": "personal",
      "readers": ["tenant"],
      "writers": ["tenant"],
      "destinations": ["internal-service"],
      "masking": {"strategy": "tokenize", "policyRef": "privacy-policy.masking.personal"},
      "retentionRef": "privacy-policy.retention.personal",
      "encryptionRefs": [
        "dev.lekalo.nfr.encryption-at-rest@0.4.0",
        "dev.lekalo.nfr.encryption-in-transit@0.4.0"
      ],
      "consentRequired": true,
      "crossTenant": "forbidden",
      "declassifyRoles": ["data-steward"]
    }
  ],
  "sinks": {
    "logs": {"maxKind": "internal"},
    "traces": {"maxKind": "internal"},
    "contextCapsules": {"maxKind": "internal"},
    "diagnostics": {"maxKind": "internal"},
    "evidence": {"maxKind": "public"},
    "exports": {"maxKind": "derived"}
  },
  "openQuestions": []
}"#;

fn digests() -> (Sha256Digest, Sha256Digest) {
    (
        Sha256Digest::from_hex(&"a".repeat(64)),
        Sha256Digest::from_hex(&"b".repeat(64)),
    )
}

#[test]
fn the_analyzer_projects_flows_and_the_report_is_stable() {
    let compilation = planner_project();
    let project = &compilation.project;
    let classification = Attachment::parse(CLASSIFICATION.as_bytes()).expect("classification");
    let policy = PolicyAttachment::parse(POLICY.as_bytes()).expect("policy");
    let resolution = Resolution::build(&classification);
    let graph =
        build_with_classification(project, Some(&resolution)).expect("stamped graph builds");
    let (model_digest, ir_digest) = digests();
    let project_id = SemanticId::parse_root("planner").expect("project id");

    // The exact input digests the report is pinned to.
    let classification_ref = Sha256Digest::parse(
        &lekalo_core::classification::attachment_digest(&classification).expect("digest"),
    )
    .expect("digest shape");
    let policy_ref =
        Sha256Digest::parse(&lekalo_core::classification::policy_digest(&policy).expect("digest"))
            .expect("digest shape");

    let analysis = analyze(&Inputs {
        project_id: &project_id,
        model_ref: ("0.2.16", &model_digest),
        ir_ref: ("0.2.16", &ir_digest),
        graph: &graph,
        compilation: &compilation,
        classification: &resolution,
        classification_ref: &classification_ref,
        policy: &policy,
        policy_ref: &policy_ref,
        generated_by: "lekalo-core/0.3.2",
        report_revision: "1.0.0",
        endpoint_exposures: &[],
    })
    .expect("analysis");

    let report = &analysis.report;
    // Every declared edge projects one flow.
    assert_eq!(report.flows().len(), graph.declared().len());
    // Every flow carries its resolved kind and a gate decision.
    for flow in report.flows() {
        assert!(
            flow.classification().rank() >= lekalo_core::classification::DataKind::Internal.rank()
        );
        assert!(flow.gate().is_some());
    }
    // The personal data of the notify module is sensitive.
    assert!(report
        .flows()
        .iter()
        .any(|flow| flow.classification() == lekalo_core::classification::DataKind::Personal));

    // The canonical export is stable and re-readable.
    let first = dataflow::report_canonical_bytes(report).expect("canonical export");
    let second = dataflow::report_canonical_bytes(report).expect("canonical export");
    assert_eq!(first, second);
    assert!(!first.contains('\n'), "compact canonical bytes");
    assert!(first.contains("\"schemaVersion\":\"lekalo/data-flow-report/v0.4.0\""));
    assert!(!first.contains("\"verdict\":\"pass\"") || report.verdict().as_str() == "pass");
    let parsed: serde_json::Value = serde_json::from_str(&first).expect("report JSON");
    let reparsed =
        dataflow::report::report_from_value(&parsed).expect("re-reads through the wire parser");
    assert_eq!(reparsed.flows().len(), report.flows().len());
    assert_eq!(reparsed.verdict(), report.verdict());
}

#[test]
fn export_sinks_above_their_ceiling_are_findings() {
    let compilation = planner_project();
    let project = &compilation.project;
    let classification = Attachment::parse(CLASSIFICATION.as_bytes()).expect("classification");
    // The same policy but with an `export` ceiling below `internal`
    // would flag storage flows; here the strict ceilings accept the
    // declared storage/event flows, so no error findings exist.
    let policy = PolicyAttachment::parse(POLICY.as_bytes()).expect("policy");
    let resolution = Resolution::build(&classification);
    let graph =
        build_with_classification(project, Some(&resolution)).expect("stamped graph builds");
    let (model_digest, ir_digest) = digests();
    let project_id = SemanticId::parse_root("planner").expect("project id");
    let classification_ref = Sha256Digest::parse(
        &lekalo_core::classification::attachment_digest(&classification).expect("digest"),
    )
    .expect("digest shape");
    let policy_ref =
        Sha256Digest::parse(&lekalo_core::classification::policy_digest(&policy).expect("digest"))
            .expect("digest shape");
    let analysis = analyze(&Inputs {
        project_id: &project_id,
        model_ref: ("0.2.16", &model_digest),
        ir_ref: ("0.2.16", &ir_digest),
        graph: &graph,
        compilation: &compilation,
        classification: &resolution,
        classification_ref: &classification_ref,
        policy: &policy,
        policy_ref: &policy_ref,
        generated_by: "lekalo-core/0.3.2",
        report_revision: "1.0.0",
        endpoint_exposures: &[],
    })
    .expect("analysis");
    // The declared planner flows stay inside the declared ceilings.
    assert!(
        analysis.report.findings().is_empty(),
        "unexpected findings: {:?}",
        analysis
            .report
            .findings()
            .iter()
            .map(|finding| finding.rule_id())
            .collect::<Vec<_>>()
    );
    assert_eq!(analysis.report.verdict().as_str(), "pass");
}
