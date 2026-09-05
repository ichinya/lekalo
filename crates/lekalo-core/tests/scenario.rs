//! Issue #23 Scenario IR integration tests: wire normalization over the
//! committed goldens, byte-identical canonicalization, independent
//! canonical-form proofs, adversarial rejection with exact registered
//! details, denial bounds, permutation determinism, and the coverage
//! vector.
//!
//! The tests are hermetic: every fixture is embedded at compile time,
//! nothing touches the network, no process is spawned, and no path
//! outside the crate is read.

use lekalo_core::scenario::{Backend, OutcomeKind, ScenarioIr};

/// The committed valid goldens: (name, exact file bytes).
const VALID: &[(&str, &[u8])] = &[
    (
        "minimal-invoke-result",
        include_bytes!("../../../tests/fixtures/scenario/valid/minimal-invoke-result.json"),
    ),
    (
        "planner-switch-focus",
        include_bytes!("../../../tests/fixtures/scenario/valid/planner-switch-focus.json"),
    ),
    (
        "reachability-flow",
        include_bytes!("../../../tests/fixtures/scenario/valid/reachability-flow.json"),
    ),
    (
        "idempotency-replay",
        include_bytes!("../../../tests/fixtures/scenario/valid/idempotency-replay.json"),
    ),
    (
        "error-effect-auth",
        include_bytes!("../../../tests/fixtures/scenario/valid/error-effect-auth.json"),
    ),
    (
        "contract-and-bindings",
        include_bytes!("../../../tests/fixtures/scenario/valid/contract-and-bindings.json"),
    ),
];

/// The committed adversarial vectors: (name, fixture bytes, expectation
/// bytes). The expectation records the exact registered rule and the
/// fixed detail token of the single diagnostic.
const INVALID: &[(&str, &[u8], &[u8])] = &[
    (
        "wrong-schema-version",
        include_bytes!("../../../tests/fixtures/scenario/invalid/wrong-schema-version.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/wrong-schema-version.expect.json"),
    ),
    (
        "wrong-identity",
        include_bytes!("../../../tests/fixtures/scenario/invalid/wrong-identity.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/wrong-identity.expect.json"),
    ),
    (
        "unknown-top-field",
        include_bytes!("../../../tests/fixtures/scenario/invalid/unknown-top-field.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/unknown-top-field.expect.json"),
    ),
    (
        "missing-when",
        include_bytes!("../../../tests/fixtures/scenario/invalid/missing-when.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/missing-when.expect.json"),
    ),
    (
        "bad-scenario-id",
        include_bytes!("../../../tests/fixtures/scenario/invalid/bad-scenario-id.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/bad-scenario-id.expect.json"),
    ),
    (
        "wrong-ir-identity",
        include_bytes!("../../../tests/fixtures/scenario/invalid/wrong-ir-identity.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/wrong-ir-identity.expect.json"),
    ),
    (
        "bad-digest",
        include_bytes!("../../../tests/fixtures/scenario/invalid/bad-digest.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/bad-digest.expect.json"),
    ),
    (
        "wrong-model-version",
        include_bytes!("../../../tests/fixtures/scenario/invalid/wrong-model-version.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/wrong-model-version.expect.json"),
    ),
    (
        "empty-scenario",
        include_bytes!("../../../tests/fixtures/scenario/invalid/empty-scenario.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/empty-scenario.expect.json"),
    ),
    (
        "duplicate-step-id",
        include_bytes!("../../../tests/fixtures/scenario/invalid/duplicate-step-id.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/duplicate-step-id.expect.json"),
    ),
    (
        "unknown-typed-value-kind",
        include_bytes!("../../../tests/fixtures/scenario/invalid/unknown-typed-value-kind.json"),
        include_bytes!(
            "../../../tests/fixtures/scenario/invalid/unknown-typed-value-kind.expect.json"
        ),
    ),
    (
        "noncanonical-decimal",
        include_bytes!("../../../tests/fixtures/scenario/invalid/noncanonical-decimal.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/noncanonical-decimal.expect.json"),
    ),
    (
        "forward-step-output",
        include_bytes!("../../../tests/fixtures/scenario/invalid/forward-step-output.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/forward-step-output.expect.json"),
    ),
    (
        "dangling-given-value",
        include_bytes!("../../../tests/fixtures/scenario/invalid/dangling-given-value.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/dangling-given-value.expect.json"),
    ),
    (
        "then-unreachable",
        include_bytes!("../../../tests/fixtures/scenario/invalid/then-unreachable.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/then-unreachable.expect.json"),
    ),
    (
        "conflicting-setup",
        include_bytes!("../../../tests/fixtures/scenario/invalid/conflicting-setup.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/conflicting-setup.expect.json"),
    ),
    (
        "idempotency-without-control",
        include_bytes!("../../../tests/fixtures/scenario/invalid/idempotency-without-control.json"),
        include_bytes!(
            "../../../tests/fixtures/scenario/invalid/idempotency-without-control.expect.json"
        ),
    ),
    (
        "undeclared-actor",
        include_bytes!("../../../tests/fixtures/scenario/invalid/undeclared-actor.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/undeclared-actor.expect.json"),
    ),
    (
        "control-kind",
        include_bytes!("../../../tests/fixtures/scenario/invalid/control-kind.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/control-kind.expect.json"),
    ),
    (
        "forbidden-effect-scope-mismatch",
        include_bytes!(
            "../../../tests/fixtures/scenario/invalid/forbidden-effect-scope-mismatch.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/scenario/invalid/forbidden-effect-scope-mismatch.expect.json"
        ),
    ),
    (
        "unknown-assertion-kind",
        include_bytes!("../../../tests/fixtures/scenario/invalid/unknown-assertion-kind.json"),
        include_bytes!(
            "../../../tests/fixtures/scenario/invalid/unknown-assertion-kind.expect.json"
        ),
    ),
    (
        "over-limit-tags",
        include_bytes!("../../../tests/fixtures/scenario/invalid/over-limit-tags.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/over-limit-tags.expect.json"),
    ),
    (
        "replay-not-prior",
        include_bytes!("../../../tests/fixtures/scenario/invalid/replay-not-prior.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/replay-not-prior.expect.json"),
    ),
    (
        "bad-metadata-key",
        include_bytes!("../../../tests/fixtures/scenario/invalid/bad-metadata-key.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/bad-metadata-key.expect.json"),
    ),
    (
        "control-dangling",
        include_bytes!("../../../tests/fixtures/scenario/invalid/control-dangling.json"),
        include_bytes!("../../../tests/fixtures/scenario/invalid/control-dangling.expect.json"),
    ),
];

/// Parse one embedded fixture payload (exact file bytes minus the one
/// trailing LF) into JSON.
fn payload(name: &str, bytes: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
    let payload = text
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("{name}: exactly one trailing LF expected"));
    serde_json::from_str(payload).unwrap_or_else(|error| panic!("{name}: {error}"))
}

#[test]
fn valid_goldens_normalize_and_canonicalize_byte_identically() {
    for (name, bytes) in VALID {
        let value = payload(name, bytes);
        let ir = ScenarioIr::from_value(&value)
            .unwrap_or_else(|set| panic!("{name}: rejected: {set:?}"));
        let text = std::str::from_utf8(bytes).expect("utf8");
        let payload_text = text.strip_suffix('\n').expect("one trailing LF");
        let canonical = ir
            .canonical_bytes()
            .unwrap_or_else(|set| panic!("{name}: export refused: {set:?}"));
        assert_eq!(canonical, payload_text, "{name}: canonical bytes diverge");
    }
}

#[test]
fn goldens_survive_an_independent_canonical_form_check() {
    // serde_json objects are byte-sorted maps: re-serializing the parsed
    // payload must reproduce the committed bytes exactly. This proves
    // the canonical form independently of the Rust writer.
    for (name, bytes) in VALID {
        let value = payload(name, bytes);
        let text = std::str::from_utf8(bytes).expect("utf8");
        let payload_text = text.strip_suffix('\n').expect("one trailing LF");
        let reserialized = serde_json::to_string(&value).expect("serialize");
        assert_eq!(reserialized, payload_text, "{name}: not canonical JSON");
    }
}

#[test]
fn adversarial_fixtures_fail_with_the_recorded_single_diagnostic() {
    for (name, fixture, expectation) in INVALID {
        let value = payload(name, fixture);
        let expect: serde_json::Value = payload(name, expectation);
        let detail = expect["detail"].as_str().expect("detail recorded");
        let rule = expect["rule"].as_str().expect("rule recorded");
        let Err(set) = ScenarioIr::from_value(&value) else {
            panic!("{name}: must be rejected")
        };
        assert_eq!(set.as_slice().len(), 1, "{name}: one diagnostic only");
        assert_eq!(set.as_slice()[0].id(), rule, "{name}: registered rule");
        let rendered = serde_json::to_string(&set.as_slice()[0]).expect("diagnostic serializes");
        assert!(
            rendered.contains(&format!("\"{detail}\"")),
            "{name}: detail token {detail} missing from {rendered}"
        );
    }
}

#[test]
fn set_like_permutations_preserve_the_canonical_bytes() {
    let name = "planner-switch-focus";
    let bytes = &VALID[1].1;
    let mut value = payload(name, bytes);
    let baseline = ScenarioIr::from_value(&value)
        .expect("golden normalizes")
        .canonical_bytes()
        .expect("canonical bytes");
    // Reversing the set-like tag order must not change the bytes.
    value["tags"].as_array_mut().expect("tags array").reverse();
    let reordered = ScenarioIr::from_value(&value)
        .expect("permutation normalizes")
        .canonical_bytes()
        .expect("canonical bytes");
    assert_eq!(baseline, reordered, "tags are a sorted set");
}

#[test]
fn binding_order_never_changes_the_canonical_bytes() {
    let name = "contract-and-bindings";
    let bytes = &VALID[5].1;
    let mut value = payload(name, bytes);
    let baseline = ScenarioIr::from_value(&value)
        .expect("golden normalizes")
        .canonical_bytes()
        .expect("canonical bytes");
    value["bindings"]
        .as_array_mut()
        .expect("bindings array")
        .reverse();
    let reordered = ScenarioIr::from_value(&value)
        .expect("permutation normalizes")
        .canonical_bytes()
        .expect("canonical bytes");
    assert_eq!(baseline, reordered, "bindings are canonically sorted");
}

#[test]
fn behavioral_order_is_never_sorted_away() {
    let name = "planner-switch-focus";
    let bytes = &VALID[1].1;
    let mut value = payload(name, bytes);
    let baseline = ScenarioIr::from_value(&value)
        .expect("golden normalizes")
        .canonical_bytes()
        .expect("canonical bytes");
    // Reversing the two given steps swaps behavior; the bytes must
    // change because order is behavior.
    value["given"]
        .as_array_mut()
        .expect("given array")
        .reverse();
    let swapped = ScenarioIr::from_value(&value)
        .expect("swapped scenario is still valid")
        .canonical_bytes()
        .expect("canonical bytes");
    assert_ne!(baseline, swapped, "given order is behavior");
}

#[test]
fn coverage_vectors_record_operations_and_expected_outcomes() {
    let bytes = VALID[1].1;
    let ir =
        ScenarioIr::from_value(&payload("planner-switch-focus", bytes)).expect("golden normalizes");
    let coverage = ir.coverage();
    assert_eq!(
        coverage.scenario_id.as_str(),
        "planner.scenario.switch_focus"
    );
    assert_eq!(
        coverage
            .operations
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<&str>>(),
        vec!["planner.command.focus_task"]
    );
    assert_eq!(coverage.entries.len(), 2);
    for entry in &coverage.entries {
        assert_eq!(entry.assertion_kind, "entity_state");
        assert_eq!(entry.outcome, OutcomeKind::Entity);
        assert_eq!(entry.observes.as_str(), "focus");
    }
}

#[test]
fn bindings_and_source_map_references_survive_the_golden() {
    let bytes = VALID[5].1;
    let ir = ScenarioIr::from_value(&payload("contract-and-bindings", bytes))
        .expect("golden normalizes");
    assert_eq!(ir.bindings().len(), 2);
    assert_eq!(ir.bindings()[0].backend, Backend::FakeReference);
    assert_eq!(ir.bindings()[1].backend, Backend::Native);
    let source_map = ir.source_map().expect("source map reference");
    assert_eq!(source_map.entries, 9);
    assert_eq!(
        ir.tags()
            .iter()
            .map(|tag| tag.as_str())
            .collect::<Vec<&str>>(),
        {
            let mut tags = vec!["p0", "planner", "smoke"];
            tags.sort_unstable();
            tags
        }
    );
}

#[test]
fn role_bounds_and_total_bounds_are_enforced_with_typed_limits() {
    let step = serde_json::json!({
        "stepId": "step",
        "action": {
            "kind": "invoke",
            "operation": "planner.command.focus_task",
            "input": {}
        }
    });
    let given_step = serde_json::json!({
        "stepId": "step",
        "precondition": {
            "kind": "state",
            "entity": "planner.user_task_planning",
            "selector": [{"field": "task_id", "equals": {"type": "string", "value": "t1"}}],
            "fields": {"user_id": {"type": "string", "value": "u1"}}
        }
    });
    let assertion = serde_json::json!({
        "stepId": "check",
        "observes": "step",
        "assertion": {"kind": "result", "valueType": "planner.type.focus_result"}
    });
    let scenario = |given: usize, when: usize, then: usize| {
        let mut base = serde_json::json!({
            "schemaVersion": "lekalo/scenario-ir/v1.0.0",
            "identity": "dev.lekalo.scenario-ir@1.0.0",
            "projectId": "planner",
            "scenarioId": "planner.scenario.bounds",
            "scenarioVersion": "1.0.0",
            "summary": "bounds",
            "irRef": {
                "identity": "dev.lekalo.ir@0.1.0",
                "digest": format!("sha256:{}", "0".repeat(64))
            },
            "modelRef": {"modelVersion": "1.0.0", "digest": format!("sha256:{}", "0".repeat(64))},
            "given": [],
            "when": [],
            "then": [],
            "bindings": [],
            "tags": [],
            "metadata": {}
        });
        for index in 0..given {
            let mut one = given_step.clone();
            one["stepId"] = serde_json::json!(format!("given_{index}"));
            base["given"].as_array_mut().expect("array").push(one);
        }
        for index in 0..when {
            let mut one = step.clone();
            one["stepId"] = serde_json::json!(format!("when_{index}"));
            base["when"].as_array_mut().expect("array").push(one);
        }
        for index in 0..then {
            let mut one = assertion.clone();
            one["stepId"] = serde_json::json!(format!("then_{index}"));
            one["observes"] =
                serde_json::json!(format!("when_{}", index.min(when.saturating_sub(1))));
            base["then"].as_array_mut().expect("array").push(one);
        }
        base
    };
    let over_role = scenario(257, 1, 1);
    assert_scenario_limit(&over_role, "steps-per-role");
    // The per-role caps fire first: 256 + 256 + 512 = 1024 exactly meets
    // the total cap, so the total bound is defense in depth.
    let over_then_role = scenario(1, 1, 513);
    assert_scenario_limit(&over_then_role, "steps-per-role");
}

/// Assert one scenario document fails normalization with the given
/// fixed limit token.
fn assert_scenario_limit(value: &serde_json::Value, detail: &str) {
    let error = ScenarioIr::from_value(value)
        .map(|_| ())
        .expect_err("over-limit scenario must fail");
    assert_eq!(error.as_slice().len(), 1);
    assert_eq!(
        error.as_slice()[0].id(),
        "graph.traversal-limit",
        "{:#?}",
        error.as_slice()[0]
    );
    let rendered = serde_json::to_string(&error.as_slice()[0]).expect("diagnostic serializes");
    assert!(rendered.contains(&format!("\"{detail}\"")), "{rendered}");
}
