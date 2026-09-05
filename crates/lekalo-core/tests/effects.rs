//! Issue #14 library tests for the effect graph: the declared projection
//! over the hermetic planner fixture through the accepted loader seam,
//! reverse readers/writers, typed evidence attachment and the
//! declared-versus-detected comparison, the conflict matrix over explicit
//! change sets, bounded summaries, and canonical export stability.
//!
//! The #8 IR newtypes are constructible only inside the crate, so these
//! integration tests drive the same loader seam every consumer uses.

use lekalo_core::effects::{
    attach_detected, build, ChangeSet, ComparisonSpec, ComparisonState, ConflictKind, EffectKind,
    EvidenceEntry, EvidenceEnvelope, FieldName, OperationId, ResourceId, ResourceKind, Subject,
    SubjectSelector, TrustState, WriteAction,
};
use lekalo_core::ir::{compile, CompiledProject};
use lekalo_core::loader::{normalize_model, LoadSelection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FIXTURE: &str = "tests/fixtures/effects/planner";
const GOLDEN: &str = "tests/fixtures/effects/golden/planner.effects.json";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn compile_fixture(name: &str) -> CompiledProject {
    let selection = LoadSelection {
        project: Some(name.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("{name}: load failed: {}", outcome.to_json_string()),
    };
    match compile(&model) {
        Ok(compilation) => compilation.project,
        Err(failure) => panic!(
            "{name}: IR failed: {}",
            failure
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn planner() -> CompiledProject {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let project = compile_fixture(FIXTURE);
    std::env::set_current_dir(original).expect("restore cwd");
    project
}

fn planner_graph() -> lekalo_core::effects::EffectGraph {
    build(&planner()).expect("effect graph builds")
}

fn operation(semantic: &str) -> OperationId {
    OperationId::from_semantic(semantic).expect("operation id")
}

fn canonical_resource(semantic: &str) -> ResourceId {
    ResourceId::new(ResourceKind::Canonical, semantic).expect("resource id")
}

fn edges(graph: &lekalo_core::effects::EffectGraph) -> Vec<String> {
    graph
        .declared()
        .iter()
        .map(|edge| edge.key().to_canonical_string())
        .collect()
}

#[test]
fn construction_is_deterministic_and_canonical() {
    let project = planner();
    let first = build(&project).expect("graph builds");
    let second = build(&project).expect("graph builds");
    assert_eq!(edges(&first), edges(&second));
    assert_eq!(first.ir_digest(), second.ir_digest());
    assert_eq!(first.identity(), "dev.lekalo.effects@1.0.0");
    assert_eq!(first.schema_version(), "lekalo/effects/v1.0.0");
    assert_eq!(first.project_id(), Some("planner"));
    assert_eq!(first.envelope_count(), 0);
}

#[test]
fn the_declared_projection_covers_every_model_fact_exactly() {
    let graph = planner_graph();
    assert_eq!(graph.declared().len(), 10);
    assert!(graph.detected().is_empty());

    // Query reads: two task readers, one cross-module user reader.
    let reads: Vec<String> = graph
        .declared()
        .iter()
        .filter(|edge| edge.key().kind() == EffectKind::Read)
        .map(|edge| edge.key().to_canonical_string())
        .collect();
    assert_eq!(
        reads,
        vec![
            "operation:planner.count_focused|read|planner.task|#0|0",
            "operation:planner.list_tasks|read|planner.task|#0|0",
            "operation:planner.user_names|read|notify.user|#0|0",
            "operation:planner.user_names|read|planner.task|#1|1",
        ]
    );

    // CRUD: create, update, delete are distinct kinds attributed to the
    // declaring command with the effect symbol as origin.
    let mutations: Vec<String> = graph
        .declared()
        .iter()
        .filter(|edge| {
            matches!(
                edge.key().kind(),
                EffectKind::Create | EffectKind::Update | EffectKind::Delete
            )
        })
        .map(|edge| edge.key().to_canonical_string())
        .collect();
    assert_eq!(
        mutations,
        vec![
            "operation:notify.rename_user_cmd|update|notify.user|effect:notify.rename_user|0",
            "operation:planner.archive_task_cmd|delete|planner.task|effect:planner.archive_task|0",
            "operation:planner.edit_task_cmd|update|planner.task|effect:planner.edit_task|0",
            "operation:planner.focus_task|create|planner.task|effect:planner.create_task|0",
        ]
    );

    // Emissions carry the declared event symbols.
    let emissions: Vec<String> = graph
        .declared()
        .iter()
        .filter(|edge| edge.key().kind() == EffectKind::EmitEvent)
        .map(|edge| edge.key().to_canonical_string())
        .collect();
    assert_eq!(
        emissions,
        vec![
            "operation:planner.archive_task_cmd|emit-event|planner.task_archived|effect:planner.archive_task|0",
            "operation:planner.focus_task|emit-event|planner.task_focused|effect:planner.create_task|0",
        ]
    );
}

#[test]
fn declared_confidence_and_provenance_are_canonical() {
    let graph = planner_graph();
    for edge in graph.declared() {
        assert_eq!(
            edge.confidence(),
            lekalo_core::effects::Confidence::Canonical
        );
        match edge.provenance() {
            lekalo_core::effects::EffectProvenance::CanonicalIr { ir_digest, .. } => {
                assert!(ir_digest.starts_with("sha256:"));
                assert_eq!(ir_digest.len(), "sha256:".len() + 64);
            }
            _ => panic!("declared edges carry canonical-ir provenance"),
        }
        assert!(edge.transaction_group().is_none());
        assert!(edge.sensitivity().is_none());
    }
}

#[test]
fn unknown_declared_references_fail_closed() {
    // A command effect referencing a non-effect definition is rejected
    // before any graph exists.
    let mut project = planner();
    project.definitions.retain(|definition| {
        !(definition.kind() == lekalo_core::ir::DefinitionKind::Effect
            && definition.id().as_str() == "planner.create_task")
    });
    let outcome = build(&project);
    let set = match outcome {
        Ok(_) => panic!("a dangling declared effect must fail the construction"),
        Err(set) => set,
    };
    assert_eq!(set.as_slice().len(), 1);
    assert_eq!(set.as_slice()[0].id(), "graph.input-invalid");
    let rendered = serde_json::to_string(&set.as_slice()[0]).expect("diagnostic serializes");
    assert!(rendered.contains("unresolved-declared"), "{rendered}");
}

#[test]
fn readers_and_writers_answer_from_the_index() {
    let graph = planner_graph();
    let task = canonical_resource("planner.task");

    let readers = graph
        .readers(&SubjectSelector::entity(task.clone()))
        .expect("readers");
    let reader_ids: Vec<&str> = readers
        .iter()
        .map(|edge| edge.key().operation().semantic_id())
        .collect();
    assert_eq!(
        reader_ids,
        vec![
            "planner.count_focused",
            "planner.list_tasks",
            "planner.user_names"
        ]
    );

    let writers = graph
        .writers(&SubjectSelector::entity(task.clone()))
        .expect("writers");
    let writer_ids: Vec<&str> = writers
        .iter()
        .map(|edge| edge.key().operation().semantic_id())
        .collect();
    assert_eq!(
        writer_ids,
        vec![
            "planner.archive_task_cmd",
            "planner.edit_task_cmd",
            "planner.focus_task"
        ]
    );

    // An unknown resource has no matching effect: an explicit empty
    // answer, not an error.
    let nowhere = canonical_resource("planner.nowhere");
    assert!(graph
        .writers(&SubjectSelector::entity(nowhere))
        .expect("writers")
        .is_empty());
}

#[test]
fn detected_effects_attach_without_mutating_the_canonical_model() {
    let graph = planner_graph();
    let declared_before = edges(&graph);

    let envelope = EvidenceEnvelope {
        adapter_id: "vendor.example/adapter".to_owned(),
        target_id: "vendor.example/target".to_owned(),
        protocol_version: "1.0.0".to_owned(),
        evidence_digest: format!("sha256:{}", "a".repeat(64)),
        source_revision: "planner".to_owned(),
        trust: TrustState::Extracted,
        entries: vec![
            EvidenceEntry {
                operation: operation("planner.edit_task_cmd"),
                kind: EffectKind::WriteField {
                    action: WriteAction::Set,
                },
                subject: Subject::with_field(
                    canonical_resource("planner.task"),
                    FieldName::new("title").expect("field"),
                ),
                occurrence: 0,
                trust: None,
                transaction_group: None,
                sensitivity: None,
            },
            EvidenceEntry {
                operation: operation("planner.focus_task"),
                kind: EffectKind::EmitEvent,
                subject: Subject::new(
                    ResourceId::new(ResourceKind::Event, "planner.task_archived")
                        .expect("event id"),
                ),
                occurrence: 1,
                trust: Some(TrustState::Stale),
                transaction_group: None,
                sensitivity: None,
            },
        ],
    };
    let attached = attach_detected(&graph, &envelope).expect("attach");
    assert_eq!(attached.declared(), graph.declared());
    assert_eq!(edges(&attached), declared_before);
    assert_eq!(attached.detected().len(), 2);
    assert_eq!(attached.envelope_count(), 1);
    // The original graph stays untouched.
    assert!(graph.detected().is_empty());

    // Degraded evidence keeps degraded confidence, never optimistic.
    let stale = attached
        .detected()
        .iter()
        .find(|edge| edge.key().operation().semantic_id() == "planner.focus_task")
        .expect("stale edge");
    assert_eq!(
        stale.confidence(),
        lekalo_core::effects::Confidence::Unknown
    );
}

#[test]
fn malformed_evidence_envelopes_fail_closed() {
    let graph = planner_graph();
    let base = EvidenceEnvelope {
        adapter_id: "vendor.example/adapter".to_owned(),
        target_id: "vendor.example/target".to_owned(),
        protocol_version: "1.0.0".to_owned(),
        evidence_digest: format!("sha256:{}", "a".repeat(64)),
        source_revision: "planner".to_owned(),
        trust: TrustState::Extracted,
        entries: Vec::new(),
    };
    let outcome = attach_detected(&graph, &base);
    assert!(outcome.is_ok(), "an empty valid envelope attaches");

    let bad_digest = EvidenceEnvelope {
        evidence_digest: "sha256:zzz".to_owned(),
        ..base.clone()
    };
    let set = match attach_detected(&graph, &bad_digest) {
        Err(set) => set,
        Ok(_) => panic!("a malformed digest must fail closed"),
    };
    assert_eq!(set.as_slice()[0].id(), "graph.input-invalid");

    let rejected = EvidenceEnvelope {
        trust: TrustState::Rejected,
        ..base.clone()
    };
    assert!(attach_detected(&graph, &rejected).is_err());

    // An illegal kind/subject pair inside an entry is a fatal input.
    let illegal = EvidenceEnvelope {
        entries: vec![EvidenceEntry {
            operation: operation("planner.focus_task"),
            kind: EffectKind::WriteField {
                action: WriteAction::Set,
            },
            subject: Subject::new(canonical_resource("planner.task")),
            occurrence: 0,
            trust: None,
            transaction_group: None,
            sensitivity: None,
        }],
        ..base.clone()
    };
    assert!(attach_detected(&graph, &illegal).is_err());
}

#[test]
fn the_comparison_classifies_declared_versus_detected() {
    let graph = planner_graph();
    let envelope = EvidenceEnvelope {
        adapter_id: "vendor.example/adapter".to_owned(),
        target_id: "vendor.example/target".to_owned(),
        protocol_version: "1.0.0".to_owned(),
        evidence_digest: format!("sha256:{}", "b".repeat(64)),
        source_revision: "planner".to_owned(),
        trust: TrustState::Verified,
        entries: vec![
            // Matches the declared create.
            EvidenceEntry {
                operation: operation("planner.focus_task"),
                kind: EffectKind::Create,
                subject: Subject::new(canonical_resource("planner.task")),
                occurrence: 0,
                trust: None,
                transaction_group: None,
                sensitivity: None,
            },
            // Same resource, different action: action-mismatch.
            EvidenceEntry {
                operation: operation("planner.edit_task_cmd"),
                kind: EffectKind::Delete,
                subject: Subject::new(canonical_resource("planner.task")),
                occurrence: 1,
                trust: None,
                transaction_group: None,
                sensitivity: None,
            },
            // A field-scoped update against the entity-wide declaration:
            // scope-mismatch.
            EvidenceEntry {
                operation: operation("planner.edit_task_cmd"),
                kind: EffectKind::Update,
                subject: Subject::with_field(
                    canonical_resource("planner.task"),
                    FieldName::new("title").expect("field"),
                ),
                occurrence: 2,
                trust: None,
                transaction_group: None,
                sensitivity: None,
            },
            // Never declared anywhere: detected-only.
            EvidenceEntry {
                operation: operation("planner.focus_task"),
                kind: EffectKind::ExternalCall,
                subject: Subject::new(
                    ResourceId::new(ResourceKind::ExternalService, "vendor.example/pay")
                        .expect("service id"),
                ),
                occurrence: 3,
                trust: None,
                transaction_group: None,
                sensitivity: None,
            },
        ],
    };
    let attached = attach_detected(&graph, &envelope).expect("attach");
    let comparison = attached
        .compare(&ComparisonSpec::new())
        .expect("comparison");

    let mut states: Vec<(String, ComparisonState)> = comparison
        .items()
        .iter()
        .map(|item| (item.key().to_owned(), item.state()))
        .collect();
    states.sort();
    let has = |key: &str, state: ComparisonState| {
        states
            .iter()
            .any(|(item_key, item_state)| item_key.contains(key) && *item_state == state)
    };
    assert!(
        has(
            "create|planner.task|effect:planner.create_task",
            ComparisonState::Matched
        ),
        "{states:?}"
    );
    assert!(
        has(
            "planner.edit_task_cmd|delete|planner.task|#1",
            ComparisonState::ActionMismatch
        ),
        "{states:?}"
    );
    assert!(
        has("planner.task#title|#2", ComparisonState::ScopeMismatch),
        "{states:?}"
    );
    assert!(
        has("external-call", ComparisonState::UnsupportedCapability),
        "{states:?}"
    );
    // Declared reads with no evidence stay declared-only.
    assert!(
        has("read|planner.task", ComparisonState::DeclaredOnly),
        "{states:?}"
    );
    assert!(comparison.complete());

    // Explanations are stable strings built from canonical data only.
    let matched = comparison
        .items()
        .iter()
        .find(|item| item.state() == ComparisonState::Matched)
        .expect("matched item");
    assert!(matched.explanation().contains("declared and detected"));
    assert!(matched
        .explanation()
        .starts_with("operation:planner.focus_task"));
}

#[test]
fn the_conflict_matrix_classifies_parallel_changes() {
    let graph = planner_graph();

    // The two task writers against each other and the readers: the
    // delete dominates its pairs, writer/writer is definite.
    let change_set = ChangeSet::new(vec![
        operation("planner.edit_task_cmd"),
        operation("planner.archive_task_cmd"),
    ])
    .expect("change set");
    let report = graph.conflicts(&change_set).expect("conflicts");
    let classifications: Vec<(String, String, ConflictKind)> = report
        .items()
        .iter()
        .map(|item| {
            (
                item.left().semantic_id().to_owned(),
                item.right().semantic_id().to_owned(),
                item.classification(),
            )
        })
        .collect();
    assert!(classifications.contains(&(
        "planner.archive_task_cmd".to_owned(),
        "planner.edit_task_cmd".to_owned(),
        ConflictKind::DeleteOverlap,
    )));
    // The changed writers vs the unchanged task readers.
    assert!(classifications.contains(&(
        "planner.archive_task_cmd".to_owned(),
        "planner.count_focused".to_owned(),
        ConflictKind::DeleteOverlap,
    )));
    assert!(classifications.contains(&(
        "planner.count_focused".to_owned(),
        "planner.edit_task_cmd".to_owned(),
        ConflictKind::PotentialReadWrite,
    )));
    // No conflict between the two independent reads.
    assert!(
        !classifications.iter().any(
            |(left, right, _)| left == "planner.count_focused" && right == "planner.list_tasks"
        ),
        "{classifications:?}"
    );
    assert!(report.complete());

    // Emit collisions need the same event scope; different events or
    // independent effects never collide.
    let emit_set = ChangeSet::new(vec![
        operation("planner.focus_task"),
        operation("planner.archive_task_cmd"),
    ])
    .expect("emit change set");
    let report = graph.conflicts(&emit_set).expect("conflicts");
    let collision = report
        .items()
        .iter()
        .any(|item| item.classification() == ConflictKind::EventJobCollision);
    assert!(!collision, "different events never collide");

    // Unknown operations are explicit failures, never empty answers.
    let unknown = ChangeSet::new(vec![operation("planner.nonesuch")]).expect("change set");
    let set = match graph.conflicts(&unknown) {
        Err(set) => set,
        Ok(_) => panic!("unknown changed operation must fail"),
    };
    assert_eq!(set.as_slice()[0].id(), "graph.unknown-node");
}

#[test]
fn the_summary_is_bounded_and_deterministic() {
    let graph = planner_graph();
    let first = graph
        .summary(&lekalo_core::effects::SummarySpec::new())
        .expect("summary");
    let second = graph
        .summary(&lekalo_core::effects::SummarySpec::new())
        .expect("summary");
    assert_eq!(first, second);
    assert_eq!(first.total(), 10);
    assert_eq!(first.rows().len(), 7);
    assert!(first.complete());
    let focus = first
        .rows()
        .iter()
        .find(|row| row.operation().semantic_id() == "planner.focus_task")
        .expect("focus row");
    assert_eq!(focus.reads(), 0);
    assert_eq!(focus.writes(), 1);
    assert_eq!(focus.emissions(), 1);
    assert_eq!(focus.declared(), 2);
    assert!(!focus.degraded());
}

#[test]
fn the_canonical_export_is_stable_and_matches_the_golden() {
    let graph = planner_graph();
    let first = graph.to_canonical_json().expect("export");
    let second = graph.to_canonical_json().expect("export");
    assert_eq!(first, second);
    assert!(!first.contains('\n'), "compact canonical bytes");
    assert!(first.starts_with("{\"effects\":["));

    let golden_path = workspace_root().join(GOLDEN);
    let golden = std::fs::read_to_string(&golden_path).expect("golden effect export");
    assert_eq!(first, golden.trim_end_matches('\n'), "pinned golden bytes");
}
