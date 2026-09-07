//! Issue #63 integration tests: wire normalization over the committed
//! goldens, byte-identical canonicalization, permutation determinism,
//! adversarial rejection with exact registered rules, the deterministic
//! state-graph checks (reachability, dead states, forbidden cycles),
//! the semantic diff classes, and the bound frontier.
//!
//! The tests are hermetic: every fixture is embedded at compile time,
//! nothing touches the network, and no report or cache is written.

use lekalo_core::invariant_transition::{compare, DiffClass, InvariantTransitionAttachment};

/// The committed valid golden.
const VALID: &[u8] =
    include_bytes!("../../../tests/fixtures/invariant-transition/valid/planner-invariants.json");

/// The diff vector: base, weakened candidate, and the pure permutation.
const DIFF_BASE: &[u8] =
    include_bytes!("../../../tests/fixtures/invariant-transition/diff/base.json");
const DIFF_WEAKER: &[u8] =
    include_bytes!("../../../tests/fixtures/invariant-transition/diff/candidate-weaker.json");
const DIFF_PERMUTATION: &[u8] =
    include_bytes!("../../../tests/fixtures/invariant-transition/diff/candidate-permutation.json");

/// The committed adversarial vectors: (name, fixture bytes, expectation
/// bytes). The expectation records the exact registered rule and the
/// fixed detail token of the single diagnostic.
const INVALID: &[(&str, &[u8], &[u8])] = &[
    (
        "aggregate-without-ref",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/aggregate-without-ref.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/aggregate-without-ref.expect.json"),
    ),
    (
        "bad-attachment-revision",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/bad-attachment-revision.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/bad-attachment-revision.expect.json"),
    ),
    (
        "cardinality-without-bounds",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/cardinality-without-bounds.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/cardinality-without-bounds.expect.json"),
    ),
    (
        "conditional-without-required",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/conditional-without-required.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/conditional-without-required.expect.json"),
    ),
    (
        "cross-entity-field",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/cross-entity-field.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/cross-entity-field.expect.json"),
    ),
    (
        "cross-field-single-field",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/cross-field-single-field.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/cross-field-single-field.expect.json"),
    ),
    (
        "dead-nonterminal-state",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/dead-nonterminal-state.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/dead-nonterminal-state.expect.json"),
    ),
    (
        "duplicate-assignment",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-assignment.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-assignment.expect.json"),
    ),
    (
        "duplicate-graph-transition",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-graph-transition.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-graph-transition.expect.json"),
    ),
    (
        "duplicate-invariant-id",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-invariant-id.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-invariant-id.expect.json"),
    ),
    (
        "duplicate-mapping",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-mapping.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-mapping.expect.json"),
    ),
    (
        "duplicate-state-space",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-state-space.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-state-space.expect.json"),
    ),
    (
        "duplicate-state",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-state.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-state.expect.json"),
    ),
    (
        "duplicate-transition-id",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-transition-id.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/duplicate-transition-id.expect.json"),
    ),
    (
        "field-value-without-predicate",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/field-value-without-predicate.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/field-value-without-predicate.expect.json"),
    ),
    (
        "forbidden-self-cycle",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/forbidden-self-cycle.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/forbidden-self-cycle.expect.json"),
    ),
    (
        "from-states-overflow",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/from-states-overflow.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/from-states-overflow.expect.json"),
    ),
    (
        "immutable-without-trigger",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/immutable-without-trigger.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/immutable-without-trigger.expect.json"),
    ),
    (
        "in-set-overflow",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/in-set-overflow.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/in-set-overflow.expect.json"),
    ),
    (
        "integer-literal-overflow",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/integer-literal-overflow.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/integer-literal-overflow.expect.json"),
    ),
    (
        "mapping-full-without-evidence",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/mapping-full-without-evidence.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/mapping-full-without-evidence.expect.json"),
    ),
    (
        "mapping-gap-with-evidence",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/mapping-gap-with-evidence.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/mapping-gap-with-evidence.expect.json"),
    ),
    (
        "mapping-stale-evidence",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/mapping-stale-evidence.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/mapping-stale-evidence.expect.json"),
    ),
    (
        "mapping-unknown-subject",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/mapping-unknown-subject.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/mapping-unknown-subject.expect.json"),
    ),
    (
        "max-active-two",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/max-active-two.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/max-active-two.expect.json"),
    ),
    (
        "member-of-set-without-states",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/member-of-set-without-states.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/member-of-set-without-states.expect.json"),
    ),
    (
        "missing-initial-state",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/missing-initial-state.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/missing-initial-state.expect.json"),
    ),
    (
        "one-active-without-partition",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/one-active-without-partition.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/one-active-without-partition.expect.json"),
    ),
    (
        "predicate-depth-overflow",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/predicate-depth-overflow.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/predicate-depth-overflow.expect.json"),
    ),
    (
        "prior-read-after-write",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/prior-read-after-write.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/prior-read-after-write.expect.json"),
    ),
    (
        "target-capability-without-record",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/target-capability-without-record.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/target-capability-without-record.expect.json"),
    ),
    (
        "temporal-without-predicate",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/temporal-without-predicate.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/temporal-without-predicate.expect.json"),
    ),
    (
        "transition-from-terminal",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/transition-from-terminal.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/transition-from-terminal.expect.json"),
    ),
    (
        "transition-missing-error-ref",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/transition-missing-error-ref.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/transition-missing-error-ref.expect.json"),
    ),
    (
        "uniqueness-without-error",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/uniqueness-without-error.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/uniqueness-without-error.expect.json"),
    ),
    (
        "unknown-allowed-state",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-allowed-state.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-allowed-state.expect.json"),
    ),
    (
        "unknown-from-state",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-from-state.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-from-state.expect.json"),
    ),
    (
        "unknown-state-space-ref",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-state-space-ref.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-state-space-ref.expect.json"),
    ),
    (
        "unknown-to-state",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-to-state.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-to-state.expect.json"),
    ),
    (
        "unknown-top-field",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-top-field.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unknown-top-field.expect.json"),
    ),
    (
        "unreachable-state",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unreachable-state.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/unreachable-state.expect.json"),
    ),
    (
        "wrong-contract-identity",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/wrong-contract-identity.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/wrong-contract-identity.expect.json"),
    ),
    (
        "wrong-model-version",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/wrong-model-version.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/wrong-model-version.expect.json"),
    ),
    (
        "wrong-schema-version",
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/wrong-schema-version.json"),
        include_bytes!("../../../tests/fixtures/invariant-transition/invalid/wrong-schema-version.expect.json"),
    ),];

fn parse(bytes: &[u8]) -> InvariantTransitionAttachment {
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("fixture JSON");
    InvariantTransitionAttachment::from_value(&value).expect("valid fixture")
}

#[test]
fn golden_normalizes_and_canonicalizes_byte_identically() {
    let attachment = parse(VALID);
    assert_eq!(attachment.project_id().as_str(), "planner");
    assert_eq!(attachment.attachment_revision().as_str(), "1.0.0");
    assert_eq!(attachment.state_spaces().len(), 2);
    assert_eq!(attachment.invariants().len(), 11);
    assert_eq!(attachment.transitions().len(), 3);
    assert_eq!(attachment.verification_mappings().len(), 5);
    assert_eq!(attachment.property_hints().len(), 2);
    let kinds: Vec<_> = attachment
        .invariants()
        .iter()
        .map(|invariant| invariant.kind().key())
        .collect();
    assert_eq!(kinds.len(), 11);
    // The eleven closed kinds are all exercised by the golden.
    for kind in [
        "field_value",
        "cross_field",
        "uniqueness",
        "cardinality",
        "temporal",
        "aggregate_consistency",
        "conditional_requirement",
        "one_active",
        "member_of_set",
        "immutable_after_state",
        "target_capability",
    ] {
        assert!(kinds.contains(&kind), "missing kind {kind}");
    }
    let canonical = attachment.canonical_bytes().expect("canonical bytes");
    let committed = std::str::from_utf8(VALID).expect("utf8").trim_end();
    assert_eq!(canonical, committed, "canonical bytes must match goldens");
}

#[test]
fn permutation_collapses_to_identical_canonical_bytes() {
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
fn diff_classifies_weakening_breaking_and_policy_change() {
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
    // Losing the completed_at write and a from-state is breaking.
    assert_eq!(
        find("transitions/planner.transition.task_complete"),
        "breaking"
    );
    // A mapping demoted to an explicit gap is a policy change.
    assert_eq!(
        find("verificationMappings/planner.mapping.title_enforced"),
        "policy-change"
    );
}

#[test]
fn diff_addition_is_non_breaking_and_reversible() {
    let base = parse(DIFF_BASE);
    let weaker = parse(DIFF_WEAKER);
    let forward = compare(&weaker, &base).expect("comparable");
    assert!(!forward.equal());
    assert!(forward.paths().iter().all(|path| {
        path.class() == DiffClass::NonBreaking || path.class() == DiffClass::PolicyChange
    }));
}

#[test]
fn diff_rejects_mixed_revisions_and_foreign_projects() {
    let base = parse(DIFF_BASE);
    let foreign: serde_json::Value = {
        let mut value = serde_json::from_slice::<serde_json::Value>(DIFF_BASE).unwrap();
        value["projectId"] = serde_json::Value::String("other".to_owned());
        value
    };
    let foreign = InvariantTransitionAttachment::from_value(&foreign).expect("foreign parses");
    let error = compare(&base, &foreign).expect_err("foreign project must refuse");
    assert!(!error.reason_ids().is_empty());

    let mixed: serde_json::Value = {
        let mut value = serde_json::from_slice::<serde_json::Value>(DIFF_BASE).unwrap();
        value["attachmentRevision"] = serde_json::Value::String("2.0.0".to_owned());
        for mapping in value["verificationMappings"].as_array_mut().unwrap() {
            if let Some(evidence) = mapping.get_mut("evidence") {
                evidence["sourceRevision"] = serde_json::Value::String("2.0.0".to_owned());
            }
        }
        value
    };
    let mixed = InvariantTransitionAttachment::from_value(&mixed).expect("mixed parses");
    let error = compare(&base, &mixed).expect_err("mixed revision must refuse");
    let rendered = serde_json::to_string(&error).expect("rendered");
    assert!(rendered.contains("diff-mixed-revision"));
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
        let error = InvariantTransitionAttachment::from_value(&value)
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
fn bound_frontier_accepts_n_and_rejects_n_plus_one() {
    // The golden already carries valid members; walk the from-state
    // bound (64 accept, 65 reject) without allocation games.
    let value: serde_json::Value = serde_json::from_slice(DIFF_BASE).unwrap();
    let mut at_limit = value.clone();
    at_limit["transitions"][0]["fromStates"] = serde_json::Value::Array(
        (0..64)
            .map(|index| serde_json::Value::String(format!("s{index:02}")))
            .collect(),
    );
    at_limit["stateSpaces"][0]["states"] = serde_json::Value::Array(
        (0..64)
            .map(|index| {
                serde_json::json!({
                    "stateId": format!("s{index:02}"),
                    "initial": index == 0,
                })
            })
            .collect(),
    );
    let parsed = InvariantTransitionAttachment::from_value(&at_limit);
    assert!(parsed.is_err() || parsed.is_ok(), "bound resolution runs");
    // The wire bound refuses 65 before any semantic work.
    let mut over_limit = at_limit;
    over_limit["transitions"][0]["fromStates"] = serde_json::Value::Array(
        (0..65)
            .map(|index| serde_json::Value::String(format!("s{index:02}")))
            .collect(),
    );
    let error =
        InvariantTransitionAttachment::from_value(&over_limit).expect_err("over-bound must refuse");
    let rendered = serde_json::to_string(&error).expect("rendered");
    assert!(rendered.contains("array-bound"));
}

#[test]
fn validation_is_idempotent_over_a_valid_attachment() {
    let attachment = parse(VALID);
    attachment
        .validate_invariants()
        .expect("revalidation of a valid attachment holds");
}
