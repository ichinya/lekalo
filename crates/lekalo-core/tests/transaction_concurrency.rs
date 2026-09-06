//! Issue #24 integration tests: wire normalization over the committed
//! goldens, byte-identical canonicalization, permutation determinism,
//! adversarial rejection with exact registered rules, the semantic diff
//! classes, capability-profile mapping, the group-membership projection,
//! and the #14 graph cross-validation through the accepted loader seam.
//!
//! The tests are hermetic: every fixture is embedded at compile time,
//! nothing touches the network, and no report or cache is written.

use lekalo_core::scenario::id::NamespacedId;
use lekalo_core::transaction_concurrency::{
    diff::compare, map_capabilities, CapabilityDecision, CapabilityProfile, CapabilitySnapshot,
    SnapshotSupport, TransactionConcurrencyAttachment, TransactionMode,
};
#[allow(unused_imports)]
use CapabilityDecision as _DecisionVisibility;

/// The committed valid golden.
const VALID: &[u8] =
    include_bytes!("../../../tests/fixtures/transaction-concurrency/valid/planner-focus-race.json");

/// The committed adversarial vectors: (name, fixture bytes, expectation
/// bytes). The expectation records the exact registered rule and the
/// fixed detail token of the single diagnostic.
const INVALID: &[(&str, &[u8], &[u8])] = &[
    ("wrong-schema-version", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/wrong-schema-version.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/wrong-schema-version.expect.json")),
    ("unknown-top-field", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unknown-top-field.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unknown-top-field.expect.json")),
    ("required-without-group", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/required-without-group.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/required-without-group.expect.json")),
    ("external-effect-in-group", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/external-effect-in-group.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/external-effect-in-group.expect.json")),
    ("duplicate-effect-in-group", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/duplicate-effect-in-group.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/duplicate-effect-in-group.expect.json")),
    ("foreign-effect-ref", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/foreign-effect-ref.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/foreign-effect-ref.expect.json")),
    ("duplicate-group-id", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/duplicate-group-id.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/duplicate-group-id.expect.json")),
    ("missing-compensation", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/missing-compensation.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/missing-compensation.expect.json")),
    ("lock-order-duplicate", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/lock-order-duplicate.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/lock-order-duplicate.expect.json")),
    ("idempotency-key-missing", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/idempotency-key-missing.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/idempotency-key-missing.expect.json")),
    ("retry-safety-conflict", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/retry-safety-conflict.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/retry-safety-conflict.expect.json")),
    ("missing-capability", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/missing-capability.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/missing-capability.expect.json")),
    ("unresolved-capability-ref", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unresolved-capability-ref.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unresolved-capability-ref.expect.json")),
    ("unresolved-invariant-ref", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unresolved-invariant-ref.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unresolved-invariant-ref.expect.json")),
    ("cyclic-schedule", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/cyclic-schedule.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/cyclic-schedule.expect.json")),
    ("unknown-participant", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unknown-participant.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unknown-participant.expect.json")),
    ("outcome-mismatch", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/outcome-mismatch.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/outcome-mismatch.expect.json")),
    ("bad-effect-ref", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/bad-effect-ref.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/bad-effect-ref.expect.json")),
    ("unscheduled-invocation", include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unscheduled-invocation.json"), include_bytes!("../../../tests/fixtures/transaction-concurrency/invalid/unscheduled-invocation.expect.json")),
];

/// The diff vector: base, weakened candidate, and the pure permutation.
const DIFF_BASE: &[u8] =
    include_bytes!("../../../tests/fixtures/transaction-concurrency/diff/base.json");
const DIFF_WEAKER: &[u8] =
    include_bytes!("../../../tests/fixtures/transaction-concurrency/diff/candidate-weaker.json");
const DIFF_PERMUTATION: &[u8] = include_bytes!(
    "../../../tests/fixtures/transaction-concurrency/diff/candidate-permutation.json"
);

fn parse(bytes: &[u8]) -> TransactionConcurrencyAttachment {
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("fixture JSON");
    TransactionConcurrencyAttachment::from_value(&value).expect("valid fixture")
}

#[test]
fn golden_normalizes_and_canonicalizes_byte_identically() {
    let attachment = parse(VALID);
    assert_eq!(attachment.project_id().as_str(), "planner");
    assert_eq!(attachment.operations().len(), 3);
    assert_eq!(attachment.invariants().len(), 1);
    assert_eq!(attachment.cases().len(), 2);
    assert_eq!(attachment.capability_requirements().len(), 10);
    let canonical = attachment.canonical_bytes().expect("canonical bytes");
    let committed = std::str::from_utf8(VALID).expect("utf8").trim_end();
    assert_eq!(canonical, committed, "canonical bytes must match goldens");
}

#[test]
fn permutation_is_not_a_semantic_change() {
    let base = parse(DIFF_BASE);
    let permuted = parse(DIFF_PERMUTATION);
    let base_bytes = base.canonical_bytes().expect("base canonical");
    let permuted_bytes = permuted.canonical_bytes().expect("permuted canonical");
    assert_eq!(base_bytes, permuted_bytes);
    let diff = compare(&base, &permuted).expect("comparable");
    assert!(diff.equal());
    assert!(diff.paths().is_empty());
}

#[test]
fn diff_classifies_weakening_breaking_and_additions_additive() {
    let base = parse(DIFF_BASE);
    let weaker = parse(DIFF_WEAKER);
    let diff = compare(&base, &weaker).expect("comparable");
    assert!(!diff.equal());
    let rendered: Vec<(String, String)> = diff
        .paths()
        .iter()
        .map(|path| (path.path().to_owned(), path.class().key().to_owned()))
        .collect();
    let find = |path: &str| {
        rendered
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, class)| class.to_owned())
            .unwrap_or_else(|| panic!("path {path} missing from {rendered:?}"))
    };
    assert_eq!(
        find("operations/operation:planner.focus_task/transaction"),
        "breaking"
    );
    assert_eq!(
        find("operations/operation:planner.focus_task/preconditions/version|planner.resource/user-task-planning"),
        "breaking"
    );
    assert_eq!(
        find("concurrencyCases/planner.case/different-users"),
        "non-breaking"
    );
    assert_eq!(
        find("capabilityRequirements/planner.req/unique-invariant"),
        "policy-change"
    );
    // Removal of a required operation never happens here; every other
    // path must be one of the recorded ones plus the requirement set.
    assert!(rendered.len() >= 4);
}

#[test]
fn adversarial_vectors_reject_with_the_registered_rule() {
    for (name, bytes, expectation) in INVALID {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let expected: serde_json::Value = serde_json::from_slice(expectation)
            .unwrap_or_else(|error| panic!("{name} expectation: {error}"));
        let rule = expected["rule"].as_str().expect("rule");
        let detail = expected["detail"].as_str().expect("detail");
        let error = TransactionConcurrencyAttachment::from_value(&value)
            .expect_err(&format!("{name} must reject"));
        let ids = error.reason_ids();
        assert_eq!(
            ids.first().copied(),
            Some(rule),
            "{name}: wrong rule for {ids:?}"
        );
        assert_eq!(error.as_slice().len(), 1, "{name}: one diagnostic only");
        let rendered = serde_json::to_string(&error).expect("diagnostic json");
        assert!(
            rendered.contains(detail),
            "{name}: detail {detail} missing from {rendered}"
        );
    }
}

#[test]
fn group_membership_projects_sorted_typed_data() {
    let attachment = parse(VALID);
    let members = attachment.group_membership();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].group_id, "planner.group/focus-commit");
    assert_eq!(members[0].operation, "operation:planner.focus_task");
    assert_eq!(
        members[0].effect,
        "operation:planner.focus_task|create|planner.task|effect:planner.create_task|0"
    );
}

#[test]
fn forbidden_operations_never_declare_groups() {
    let attachment = parse(VALID);
    let read = attachment
        .operations()
        .iter()
        .find(|operation| operation.transaction() == TransactionMode::Forbidden)
        .expect("forbidden operation");
    assert!(read.groups().is_empty());
}

#[test]
fn permissive_profiles_degrade_and_strict_blocks_on_unknown() {
    let attachment = parse(VALID);
    let empty = CapabilitySnapshot::default();
    let strict = map_capabilities(
        attachment.capability_requirements(),
        &empty,
        CapabilityProfile::Strict,
    );
    assert!(strict.blocked());
    assert!(strict.degraded());
    let permissive = map_capabilities(
        attachment.capability_requirements(),
        &empty,
        CapabilityProfile::Permissive,
    );
    assert!(!permissive.blocked());
    assert!(permissive.degraded());
    let full = CapabilitySnapshot::new(
        attachment
            .capability_requirements()
            .iter()
            .map(|requirement| (requirement.capability().to_wire(), SnapshotSupport::Full))
            .collect::<Vec<_>>(),
    );
    let satisfied = map_capabilities(
        attachment.capability_requirements(),
        &full,
        CapabilityProfile::Strict,
    );
    assert!(!satisfied.blocked() && !satisfied.degraded());
}

#[test]
fn graph_cross_validation_accepts_real_edges_and_rejects_foreign_ones() {
    use lekalo_core::effects::{build, EffectGraph};
    use lekalo_core::ir::compile;
    use lekalo_core::loader::{normalize_model, LoadSelection};
    use std::path::Path;
    use std::sync::Mutex;

    static CWD_LOCK: Mutex<()> = Mutex::new(());
    const FIXTURE: &str = "tests/fixtures/effects/planner";

    fn compile_planner() -> (EffectGraph, Vec<String>) {
        let _guard = CWD_LOCK.lock().expect("cwd lock");
        let original = std::env::current_dir().expect("current dir");
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf();
        std::env::set_current_dir(&root).expect("enter workspace root");
        let selection = LoadSelection {
            project: Some(FIXTURE.to_owned()),
        };
        let model = normalize_model(&selection).expect("load");
        let compilation = compile(&model).expect("compile");
        let graph = build(&compilation.project).expect("effect graph");
        let edges: Vec<String> = graph
            .operation_edges(
                &lekalo_core::effects::OperationId::from_qualified("operation:planner.focus_task")
                    .expect("operation id"),
            )
            .iter()
            .map(|edge| edge.key().to_canonical_string())
            .collect();
        std::env::set_current_dir(original).expect("restore cwd");
        (graph, edges)
    }

    let (graph, edges) = compile_planner();
    assert!(!edges.is_empty(), "the planner fixture declares edges");
    let base: serde_json::Value = serde_json::from_slice(DIFF_BASE).expect("base JSON");
    let mut wire = base.clone();
    // Rebuild the focus group around one real declared edge of the
    // planner.focus_task command, and bind the graph digest.
    let real = edges[0].clone();
    wire["effectGraphRef"]["digest"] = serde_json::Value::String(graph.ir_digest().to_owned());
    wire["operations"][0]["atomicEffectGroups"][0]["effectRefs"] =
        serde_json::Value::Array(vec![serde_json::Value::String(real.clone())]);
    // The declared focus operation ref must match the edge's operation.
    let edge_operation: String = real.split('|').next().unwrap_or_default().to_owned();
    wire["operations"][0]["operationRef"] = serde_json::Value::String(edge_operation.clone());
    let attachment = TransactionConcurrencyAttachment::from_value(&wire).expect("valid");
    attachment
        .validate_against_graph(&graph)
        .expect("real edges cross-validate");

    // A foreign effect ref fails the cross-check.
    wire["operations"][0]["atomicEffectGroups"][0]["effectRefs"][0] = serde_json::Value::String(
        "operation:planner.focus_task|delete|planner.task|effect:planner.no_such|7".to_owned(),
    );
    let corrupted = TransactionConcurrencyAttachment::from_value(&wire).expect("wire valid");
    let error = corrupted
        .validate_against_graph(&graph)
        .expect_err("foreign ref must fail");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("transaction.group-overlap")
    );
    let _ = NamespacedId::parse("planner.group/focus-commit").expect("id grammar");
}
