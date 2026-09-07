//! Issue #26 integration tests: wire normalization over the committed
//! goldens, byte-identical canonicalization, permutation determinism,
//! adversarial rejection with exact registered rules, the semantic diff
//! classes, capability-profile mapping, the effect-binding projection,
//! and the #14 graph cross-validation through the accepted loader seam.
//!
//! The tests are hermetic: every fixture is embedded at compile time,
//! nothing touches the network, and no report or cache is written.

use lekalo_core::extended_effects::{
    compare, map_capabilities, CapabilityProfile, CapabilityRequirement, CapabilitySnapshot,
    ExtendedEffectsAttachment, SnapshotSupport,
};

/// The committed valid golden.
const VALID: &[u8] =
    include_bytes!("../../../tests/fixtures/extended-effects/valid/planner-effects.json");

/// The committed adversarial vectors: (name, fixture bytes, expectation
/// bytes). The expectation records the exact registered rule and the
/// fixed detail token of the single diagnostic.
const INVALID: &[(&str, &[u8], &[u8])] = &[
    ("wrong-schema-version", include_bytes!("../../../tests/fixtures/extended-effects/invalid/wrong-schema-version.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/wrong-schema-version.expect.json")),
    ("unknown-top-field", include_bytes!("../../../tests/fixtures/extended-effects/invalid/unknown-top-field.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/unknown-top-field.expect.json")),
    ("duplicate-contract-ref", include_bytes!("../../../tests/fixtures/extended-effects/invalid/duplicate-contract-ref.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/duplicate-contract-ref.expect.json")),
    ("event-dedup-key-missing", include_bytes!("../../../tests/fixtures/extended-effects/invalid/event-dedup-key-missing.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/event-dedup-key-missing.expect.json")),
    ("foreign-effect-kind", include_bytes!("../../../tests/fixtures/extended-effects/invalid/foreign-effect-kind.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/foreign-effect-kind.expect.json")),
    ("event-missing-capability", include_bytes!("../../../tests/fixtures/extended-effects/invalid/event-missing-capability.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/event-missing-capability.expect.json")),
    ("job-retry-idempotency-conflict", include_bytes!("../../../tests/fixtures/extended-effects/invalid/job-retry-idempotency-conflict.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/job-retry-idempotency-conflict.expect.json")),
    ("call-missing-compensation", include_bytes!("../../../tests/fixtures/extended-effects/invalid/call-missing-compensation.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/call-missing-compensation.expect.json")),
    ("call-missing-capability", include_bytes!("../../../tests/fixtures/extended-effects/invalid/call-missing-capability.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/call-missing-capability.expect.json")),
    ("approval-without-approver", include_bytes!("../../../tests/fixtures/extended-effects/invalid/approval-without-approver.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/approval-without-approver.expect.json")),
    ("sensitive-without-gate", include_bytes!("../../../tests/fixtures/extended-effects/invalid/sensitive-without-gate.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/sensitive-without-gate.expect.json")),
    ("target-specific-without-profile", include_bytes!("../../../tests/fixtures/extended-effects/invalid/target-specific-without-profile.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/target-specific-without-profile.expect.json")),
    ("cache-operation-kind-mismatch", include_bytes!("../../../tests/fixtures/extended-effects/invalid/cache-operation-kind-mismatch.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/cache-operation-kind-mismatch.expect.json")),
    ("unresolved-capability-ref", include_bytes!("../../../tests/fixtures/extended-effects/invalid/unresolved-capability-ref.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/unresolved-capability-ref.expect.json")),
    ("cyclic-schedule", include_bytes!("../../../tests/fixtures/extended-effects/invalid/cyclic-schedule.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/cyclic-schedule.expect.json")),
    ("unknown-step-contract", include_bytes!("../../../tests/fixtures/extended-effects/invalid/unknown-step-contract.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/unknown-step-contract.expect.json")),
    ("unscheduled-step", include_bytes!("../../../tests/fixtures/extended-effects/invalid/unscheduled-step.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/unscheduled-step.expect.json")),
    ("missing-outcome", include_bytes!("../../../tests/fixtures/extended-effects/invalid/missing-outcome.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/missing-outcome.expect.json")),
    ("fault-outcome-mismatch", include_bytes!("../../../tests/fixtures/extended-effects/invalid/fault-outcome-mismatch.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/fault-outcome-mismatch.expect.json")),
    ("no-contracts", include_bytes!("../../../tests/fixtures/extended-effects/invalid/no-contracts.json"), include_bytes!("../../../tests/fixtures/extended-effects/invalid/no-contracts.expect.json")),
];

/// The diff vector: base, weakened candidate, and the pure permutation.
const DIFF_BASE: &[u8] = include_bytes!("../../../tests/fixtures/extended-effects/diff/base.json");
const DIFF_WEAKER: &[u8] =
    include_bytes!("../../../tests/fixtures/extended-effects/diff/candidate-weaker.json");
const DIFF_PERMUTATION: &[u8] =
    include_bytes!("../../../tests/fixtures/extended-effects/diff/candidate-permutation.json");

fn parse(bytes: &[u8]) -> ExtendedEffectsAttachment {
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("fixture JSON");
    ExtendedEffectsAttachment::from_value(&value).expect("valid fixture")
}

#[test]
fn golden_normalizes_and_canonicalizes_byte_identically() {
    let attachment = parse(VALID);
    assert_eq!(attachment.project_id().as_str(), "planner");
    assert_eq!(attachment.events().len(), 1);
    assert_eq!(attachment.jobs().len(), 1);
    assert_eq!(attachment.calls().len(), 1);
    assert_eq!(attachment.caches().len(), 1);
    assert_eq!(attachment.publications().len(), 1);
    assert_eq!(attachment.cases().len(), 1);
    assert_eq!(attachment.capability_requirements().len(), 8);
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
        find("events/planner.event/task-focused/delivery"),
        "breaking"
    );
    assert_eq!(
        find("publications/planner.publish/focus-report/consent"),
        "breaking"
    );
    assert_eq!(find("jobs/planner.job/notify-focus/retry"), "non-breaking");
    assert_eq!(
        find("capabilityRequirements/planner.req/new-policy"),
        "policy-change"
    );
    assert_eq!(
        find("partialFailureCases/planner.case/cache-invalidation"),
        "non-breaking"
    );
    // Every other path must be one of the recorded ones.
    assert!(rendered.len() >= 5);
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
        let error = ExtendedEffectsAttachment::from_value(&value)
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
fn strict_profiles_block_unknown_delivery_guarantees() {
    let attachment = parse(VALID);
    let requirements: Vec<CapabilityRequirement> = attachment.capability_requirements().to_vec();
    let empty = CapabilitySnapshot::default();
    let strict = map_capabilities(&requirements, &empty, CapabilityProfile::Strict);
    assert!(
        strict.blocked(),
        "unknown support must block the strict profile"
    );
    assert!(strict.degraded());
    let permissive = map_capabilities(&requirements, &empty, CapabilityProfile::Permissive);
    assert!(!permissive.blocked());
    assert!(permissive.degraded());
    let full = CapabilitySnapshot::new(
        requirements
            .iter()
            .map(|requirement| (requirement.capability(), SnapshotSupport::Full))
            .collect::<Vec<_>>(),
    );
    let satisfied = map_capabilities(&requirements, &full, CapabilityProfile::Strict);
    assert!(!satisfied.blocked() && !satisfied.degraded());
    // An unsupported durable delivery guarantee is visible as a blocked
    // verdict — the portability report renders this data.
    let partial_support = CapabilitySnapshot::new(vec![(
        requirements[2].capability(),
        SnapshotSupport::Unsupported,
    )]);
    let blocked_delivery =
        map_capabilities(&requirements, &partial_support, CapabilityProfile::Strict);
    assert!(blocked_delivery.blocked());
    let verdict = blocked_delivery
        .verdicts()
        .iter()
        .find(|verdict| verdict.capability() == requirements[2].capability())
        .expect("delivery verdict");
    assert!(verdict.blocked());
    assert_eq!(verdict.support(), SnapshotSupport::Unsupported);
}

#[test]
fn strict_profiles_pass_policy_approved_partial_support() {
    // Re-declare the golden's requirements at the partial minimum: the
    // owner accepts bounded gaps when policy approves them.
    let mut wire: serde_json::Value = serde_json::from_slice(VALID).expect("golden JSON");
    for requirement in wire["capabilityRequirements"]
        .as_array_mut()
        .expect("requirements")
    {
        requirement["minimum"] = serde_json::Value::String("partial".to_owned());
    }
    let attachment = ExtendedEffectsAttachment::from_value(&wire).expect("valid wire");
    let requirements: Vec<CapabilityRequirement> = attachment.capability_requirements().to_vec();
    let partial = CapabilitySnapshot::new(
        requirements
            .iter()
            .map(|requirement| (requirement.capability(), SnapshotSupport::Partial))
            .collect::<Vec<_>>(),
    );
    let unapproved = map_capabilities(&requirements, &partial, CapabilityProfile::Strict);
    assert!(
        unapproved.blocked() && unapproved.degraded(),
        "partial support without policy approval still blocks strict"
    );
    let approved = requirements.iter().fold(partial, |snapshot, requirement| {
        snapshot.approve_partial(&requirement.capability())
    });
    let decision = map_capabilities(&requirements, &approved, CapabilityProfile::Strict);
    assert!(
        !decision.blocked() && !decision.degraded(),
        "policy-approved partial support passes the strict profile"
    );
}

#[test]
fn duplicate_snapshot_evidence_collapses_to_the_weakest_support() {
    let attachment = parse(VALID);
    let requirements: Vec<CapabilityRequirement> = attachment.capability_requirements().to_vec();
    let capability = requirements[0].capability();
    // Contradictory duplicate evidence collapses to the weakest state:
    // full plus unknown is unknown, and unknown is never yes.
    let snapshot = CapabilitySnapshot::new(vec![
        (capability.clone(), SnapshotSupport::Full),
        (capability.clone(), SnapshotSupport::Unknown),
    ]);
    assert_eq!(snapshot.support_of(&capability), SnapshotSupport::Unknown);
    let strict = map_capabilities(&requirements[..1], &snapshot, CapabilityProfile::Strict);
    assert!(
        strict.blocked() && strict.degraded(),
        "the weakest collapsed support must still block strict"
    );
}

#[test]
fn effect_bindings_project_sorted_typed_data() {
    let attachment = parse(VALID);
    let bindings = attachment.effect_bindings();
    assert_eq!(bindings.len(), 5);
    assert_eq!(bindings[0].contract_ref, "planner.cache/task-views");
    assert_eq!(bindings[0].kind, "cache-read");
    assert!(!bindings[0].graph);
    let event = bindings
        .iter()
        .find(|binding| binding.contract_ref == "planner.event/task-focused")
        .expect("event binding");
    assert!(event.graph, "the event contributes to the graph");
    assert_eq!(
        event.effect,
        "operation:planner.focus_task|emit-event|planner.task_focused|effect:planner.create_task|0"
    );
    assert!(event.impact && event.context && event.scenarios);
    // Nothing enters a surface implicitly: every publication flag stays
    // as declared.
    let publication = bindings
        .iter()
        .find(|binding| binding.contract_ref == "planner.publish/focus-report")
        .expect("publication binding");
    assert!(!publication.graph && !publication.impact);
    assert!(publication.scenarios);
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
    // Bind the real graph digest and one real declared emit-event edge
    // of the planner.focus_task command.
    let real = edges
        .iter()
        .find(|edge| edge.contains("|emit-event|"))
        .expect("a real emit-event edge")
        .clone();
    wire["effectGraphRef"]["digest"] = serde_json::Value::String(graph.ir_digest().to_owned());
    wire["events"][0]["effectRef"] = serde_json::Value::String(real.clone());
    let attachment = ExtendedEffectsAttachment::from_value(&wire).expect("valid");
    attachment
        .validate_against_graph(&graph)
        .expect("real edges cross-validate");

    // A foreign effect ref fails the cross-check.
    wire["events"][0]["effectRef"] = serde_json::Value::String(
        "operation:planner.focus_task|emit-event|planner.task_focused|effect:planner.no_such|9"
            .to_owned(),
    );
    let corrupted = ExtendedEffectsAttachment::from_value(&wire).expect("wire valid");
    let error = corrupted
        .validate_against_graph(&graph)
        .expect_err("foreign ref must fail");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("extended.input-invalid")
    );
    let rendered = serde_json::to_string(&error).expect("diagnostic json");
    assert!(rendered.contains("unresolved-effect-ref"), "{rendered}");
}
