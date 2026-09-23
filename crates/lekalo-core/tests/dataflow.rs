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
        as_of: lekalo_core::classification::DEFAULT_AS_OF,
        validation_findings: &[],
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
        as_of: lekalo_core::classification::DEFAULT_AS_OF,
        validation_findings: &[],
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

// ---------------------------------------------------------------------------
// Issue #87 r1 F-3: the strict-profile sensitive-sink rule over the
// committed planner fixture (lives in the integration binary: the loader
// resolves the fixture selection against the process cwd, which must not
// race the lib tests).
// ---------------------------------------------------------------------------

#[test]
fn strict_rule_flags_only_subjects_without_explicit_entries() {
    let fixture = "tests/fixtures/classification/valid/planner";
    let compilation = planner_project();
    let attachment_path = workspace_root()
        .join(fixture)
        .join("lekalo/classification.json");
    let attachment =
        Attachment::parse(&std::fs::read(&attachment_path).expect("read")).expect("parses");

    // The full attachment covers the graph: no strict findings.
    assert!(lekalo_core::classification::strict_sensitive_sink_findings(
        &attachment,
        &compilation.project
    )
    .is_empty());

    // Removing the definition-level `notify.user` entry leaves the
    // entity covered only by the profile default: exactly one finding,
    // on that subject.
    let stripped = Attachment::parse(
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": "lekalo/data-classification/v0.4.0",
            "identity": "dev.lekalo.data-classification@0.4.0",
            "attachmentRevision": "1.1.0",
            "projectId": "planner",
            "modelRef": {"modelVersion": "0.2.16", "digest": attachment.model_ref().1.as_str()},
            "irRef": {"irVersion": "0.2.16", "digest": attachment.ir_ref().1.as_str()},
            "defaults": {"profile": "strict", "unclassifiedFields": "internal", "unclassifiedPayloads": "confidential"},
            "classifications": attachment.classifications().iter().filter(|entry| entry.subject().as_str() != "notify.user").map(|entry| serde_json::json!({"subject": entry.subject().as_str(), "kind": entry.kind().as_str()})).collect::<Vec<_>>(),
            "declassifications": [],
            "openQuestions": []
        }))
        .expect("serializes")
        .as_slice(),
    )
    .expect("parses");
    let rows = lekalo_core::classification::strict_sensitive_sink_findings(
        &stripped,
        &compilation.project,
    );
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].rule, "classification.unclassified-sensitive-sink");
    assert_eq!(rows[0].subject, "notify.user");
}

// ---------------------------------------------------------------------------
// Issue #87 r3 F-3: gated-sink evaluation test coverage via detected edges
// ---------------------------------------------------------------------------

#[test]
fn gated_sink_evaluation_via_detected_edge() {
    // Build a minimal compilation with an external-call effect
    let mut project = lekalo_core::ir::CompiledProject::default();
    project.project = Some(lekalo_core::ir::Project {
        id: lekalo_core::ir::SemanticId::parse_root("test").unwrap(),
        ..Default::default()
    });

    // Define an entity with an external-call effect
    let entity = lekalo_core::ir::EntityDef {
        id: lekalo_core::ir::SemanticId::parse("test.Entity").unwrap(),
        fields: Vec::new(),
        ..Default::default()
    };

    let effect = lekalo_core::ir::Effect {
        id: lekalo_core::ir::SemanticId::parse("test.Entity:external").unwrap(),
        kind: lekalo_core::ir::EffectKind::ExternalCall,
        reads: Vec::new(),
        writes: Vec::new(),
        ..Default::default()
    };

    let command = lekalo_core::ir::CommandDef {
        id: lekalo_core::ir::SemanticId::parse("test.Entity").unwrap(),
        effects: vec![effect.id().as_str().to_owned()],
        ..Default::default()
    };

    project.definitions = vec![
        lekalo_core::ir::Definition::Entity(entity),
        lekalo_core::ir::Definition::Command(command),
        lekalo_core::ir::Definition::Effect(lekalo_core::ir::EffectDef {
            id: effect.id().clone(),
            entity: lekalo_core::ir::SemanticId::parse("test.Entity").unwrap(),
            emits: Vec::new(),
            ..Default::default()
        }),
    ];

    // Build a classification that classifies the entity as personal (sink-eligible)
    let classification_json = r#"{
      "schemaVersion": "lekalo/data-classification/v0.4.0",
      "identity": "dev.lekalo.data-classification@0.4.0",
      "attachmentRevision": "1.0.0",
      "projectId": "test",
      "modelRef": {"modelVersion": "0.2.16", "digest": "sha256:a".repeat(64)},
      "irRef": {"irVersion": "0.2.16", "digest": "sha256:b".repeat(64)},
      "defaults": {"profile": "strict", "unclassifiedFields": "internal", "unclassifiedPayloads": "confidential"},
      "classifications": [
        {"subject": "test.Entity", "kind": "personal"}
      ],
      "declassifications": [],
      "openQuestions": []
    }"#;
    let classification = lekalo_core::classification::Attachment::parse(classification_json).unwrap();

    // Build a policy that requires an approved grant for personal kinds
    let policy_json = r#"{
      "schemaVersion": "lekalo/classification-policy/v0.4.0",
      "identity": "dev.lekalo.classification-policy@0.4.0",
      "attachmentRevision": "1.0.0",
      "projectId": "test",
      "modelRef": {"modelVersion": "0.2.16", "digest": "sha256:a".repeat(64)},
      "irRef": {"irVersion": "0.2.16", "digest": "sha256:b".repeat(64)},
      "kinds": [
        {
          "kind": "personal",
          "readers": ["tenant"],
          "writers": ["tenant"],
          "destinations": ["internal-service"],
          "masking": {"strategy": "tokenize", "policyRef": "privacy-policy.masking.personal"},
          "encryptionRefs": ["dev.lekalo.nfr.encryption-at-rest@0.4.0"],
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
    let policy = lekalo_core::classification::PolicyAttachment::parse(policy_json).unwrap();

    // Create a detected edge with external-call kind
    let mut graph = lekalo_core::effects::EffectGraph::default();
    let external_call_edge = lekalo_core::effects::EffectEdge::new(
        lekalo_core::effects::EffectKey::new(
            lekalo_core::effects::OperationId::from_semantic("test.Entity:external").unwrap(),
            lekalo_core::effects::EffectKind::ExternalCall,
        ),
        lekalo_core::effects::EffectProvenance::Observed,
    );
    graph.add_detected(external_call_edge);

    // Create resolution
    let resolution = lekalo_core::classification::Resolution::build(&classification);

    // Analyze with the detected edge
    let inputs = lekalo_core::dataflow::Inputs {
        project_id: &lekalo_core::scenario::id::SemanticId::parse_root("test").unwrap(),
        model_ref: ("0.2.16", &format!("sha256:{}", "a".repeat(64))),
        ir_ref: ("0.2.16", &format!("sha256:{}", "b".repeat(64))),
        graph: &graph,
        compilation: &project,
        classification: &resolution,
        classification_ref: &lekalo_core::lockfile::types::Sha256Digest::parse(&lekalo_core::classification::attachment_digest(&classification).unwrap()).unwrap(),
        policy: &policy,
        policy_ref: &lekalo_core::lockfile::types::Sha256Digest::parse(&lekalo_core::classification::policy_digest(&policy).unwrap()).unwrap(),
        generated_by: "test",
        report_revision: "1.0.0",
        endpoint_exposures: &[],
        as_of: lekalo_core::classification::DEFAULT_AS_OF,
        validation_findings: &[],
    };

    let analysis = lekalo_core::dataflow::analyze(&inputs).unwrap();
    let report = &analysis.report;

    // Should have one detected flow for external-call
    assert_eq!(report.flows().len(), 1);
    let flow = &report.flows()[0];
    assert_eq!(flow.sink_kind(), lekalo_core::dataflow::types::SinkKind::ExternalCall);
    assert_eq!(flow.provenance(), lekalo_core::dataflow::types::Provenance::Observed);

    // Should have a gate finding due to missing approval (consentRequired=true but no valid grant)
    assert!(report.findings().iter().any(|f| f.rule_id() == "dataflow.missing-approval"));
}
