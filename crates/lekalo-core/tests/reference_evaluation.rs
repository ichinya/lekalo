//! Issue #107 integration tests: the in-memory reference evaluator
//! over the committed board fixtures. Every acceptance-relevant
//! behavior is proven hermetically: planner happy / error /
//! idempotency scenarios execute in memory, transaction rollback and
//! invariant failures are deterministic, unsupported semantics are
//! explicit, and the same pinned inputs produce byte-identical
//! canonical traces — including under permutation of the independent
//! `given` state establishment and repeated durable-key replays.
//!
//! The tests are hermetic: every fixture is embedded at compile time
//! except the loader seam, which reads the committed model project
//! relative to the workspace root under one sequential test that pins
//! the process working directory and restores it.

use lekalo_core::invariant_transition::InvariantTransitionAttachment;
use lekalo_core::ir::CompiledProject;
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::reference_evaluation::{
    trace_bytes, Outcome, ReferenceEvaluation, Status, Verdict,
};
use lekalo_core::scenario::ScenarioIr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The committed board model project.
const MODEL: &str = "tests/fixtures/reference-evaluation/model";

/// The committed valid scenarios: (name, exact scenario bytes, exact
/// canonical golden trace bytes).
const SCENARIOS: &[(&str, &[u8], &[u8])] = &[
    (
        "focus-happy",
        include_bytes!("../../../tests/fixtures/reference-evaluation/scenarios/focus-happy.json"),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/focus-happy.json.trace.json"
        ),
    ),
    (
        "focus-conflict-rollback",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/focus-conflict-rollback.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/focus-conflict-rollback.json.trace.json"
        ),
    ),
    (
        "focus-idempotent",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/focus-idempotent.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/focus-idempotent.json.trace.json"
        ),
    ),
    (
        "query-read",
        include_bytes!("../../../tests/fixtures/reference-evaluation/scenarios/query-read.json"),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/query-read.json.trace.json"
        ),
    ),
    (
        "unsupported-authorization",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/unsupported-authorization.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/unsupported-authorization.json.trace.json"
        ),
    ),
];

/// The committed pinned attachment and #62 error registry.
const ATTACHMENT: &[u8] =
    include_bytes!("../../../tests/fixtures/reference-evaluation/invariants.json");
const REGISTRY: &[u8] =
    include_bytes!("../../../tests/fixtures/reference-evaluation/error-registry.json");

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// The compiled IR of the committed board model.
fn compile_board() -> CompiledProject {
    let selection = LoadSelection {
        project: Some(MODEL.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("board model failed to load: {}", outcome.to_json_string()),
    };
    match lekalo_core::ir::compile(&model) {
        Ok(compilation) => compilation.project,
        Err(failure) => panic!(
            "board IR failed: {}",
            failure
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

/// One bound evaluation over the committed pins.
fn board_evaluation<'a>(
    project: &'a CompiledProject,
    attachment: &'a InvariantTransitionAttachment,
    registry: &'a lekalo_core::error_contract::ErrorRegistry,
) -> ReferenceEvaluation<'a> {
    ReferenceEvaluation::new(project, attachment, registry)
}

/// Parse one committed scenario.
fn parse_scenario(name: &str, bytes: &[u8]) -> ScenarioIr {
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("scenario JSON");
    ScenarioIr::from_value(&value)
        .unwrap_or_else(|set| panic!("{name}: scenario rejected: {set:?}"))
}

#[test]
fn reference_suite_runs_from_the_workspace_root() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let result = std::panic::catch_unwind(|| {
        planner_scenarios_execute_in_memory_with_committed_goldens();
        repeated_executions_are_byte_identical();
        given_order_permutation_keeps_state_and_effects_stable();
        failing_command_rolls_back_to_the_pre_state();
        unknown_row_is_a_typed_declared_error();
        idempotent_replays_never_duplicate_effects();
        unsupported_semantics_are_explicit();
        mismatched_pins_refuse_the_whole_evaluation();
        deterministic_ids_are_seed_stable_and_shape_valid();
    });
    std::env::set_current_dir(original).expect("restore cwd");
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn planner_scenarios_execute_in_memory_with_committed_goldens() {
    let project = compile_board();
    let attachment_value: serde_json::Value =
        serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let evaluation = board_evaluation(&project, &attachment, &registry);
    for (name, scenario_bytes, golden) in SCENARIOS {
        let scenario = parse_scenario(name, scenario_bytes);
        let trace = evaluation.execute(&scenario).expect("trace");
        let canonical = trace_bytes(&trace).expect("canonical trace bytes");
        assert_eq!(
            canonical.as_bytes(),
            *golden,
            "{name}: canonical trace diverges from the committed golden"
        );
        match trace.status() {
            Status::Pass | Status::Unsupported => {}
            Status::Fail => panic!("{name}: unexpected fail status"),
        }
    }
    // The conflict scenario proves rollback; the authorization
    // scenario proves explicit unsupportedness; the rest pass.
    let statuses: Vec<(&str, Status)> = SCENARIOS
        .iter()
        .map(|(name, bytes, _)| {
            let scenario = parse_scenario(name, bytes);
            let trace = evaluation.execute(&scenario).expect("trace");
            (*name, trace.status())
        })
        .collect();
    assert_eq!(statuses[0].1, Status::Pass);
    assert_eq!(statuses[1].1, Status::Pass);
    assert_eq!(statuses[2].1, Status::Pass);
    assert_eq!(statuses[3].1, Status::Pass);
    assert_eq!(statuses[4].1, Status::Unsupported);
}

fn repeated_executions_are_byte_identical() {
    let project = compile_board();
    let attachment_value: serde_json::Value =
        serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let evaluation = board_evaluation(&project, &attachment, &registry);
    let (name, scenario_bytes, _) = &SCENARIOS[0];
    let scenario = parse_scenario(name, scenario_bytes);
    let first =
        trace_bytes(&evaluation.execute(&scenario).expect("trace")).expect("canonical bytes");
    for round in 1..8 {
        let again =
            trace_bytes(&evaluation.execute(&scenario).expect("trace")).expect("canonical bytes");
        assert_eq!(first, again, "round {round} diverged");
    }
}

fn given_order_permutation_keeps_state_and_effects_stable() {
    let project = compile_board();
    let attachment_value: serde_json::Value =
        serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let evaluation = board_evaluation(&project, &attachment, &registry);
    let (name, scenario_bytes, _) = &SCENARIOS[0];
    let scenario = parse_scenario(name, scenario_bytes);
    let baseline = evaluation.execute(&scenario).expect("trace");
    // Permute the independent state establishments and re-run: the
    // final state and the effect log must be byte-stable.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    let given = wire
        .get_mut("given")
        .and_then(|given| given.as_array_mut())
        .expect("given");
    given.reverse();
    let permuted = ScenarioIr::from_value(&wire).expect("permuted scenario parses");
    let trace = evaluation.execute(&permuted).expect("trace");
    assert_eq!(trace.state_digest(), baseline.state_digest());
    assert_eq!(trace.effect_digest(), baseline.effect_digest());
    for (baseline, permuted) in baseline.assertions().iter().zip(trace.assertions().iter()) {
        assert_eq!(baseline.verdict, permuted.verdict);
    }
}

fn failing_command_rolls_back_to_the_pre_state() {
    let project = compile_board();
    let attachment_value: serde_json::Value =
        serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let evaluation = board_evaluation(&project, &attachment, &registry);
    let (name, scenario_bytes, _) = &SCENARIOS[1];
    let scenario = parse_scenario(name, scenario_bytes);
    let trace = evaluation.execute(&scenario).expect("trace");
    let when = &trace.when()[0];
    match &when.outcome {
        Outcome::Error {
            token,
            violations,
            declared,
        } => {
            assert_eq!(token.as_str(), "invariant-violated");
            assert_eq!(
                violations,
                &vec!["board.invariant.one_focus_per_user".to_owned()]
            );
            assert!(declared.contains(&"errors.board/focus-conflict".to_owned()));
        }
        other => panic!("expected invariant-violated error, got {other:?}"),
    }
    // The staged write is gone: the unfocused row is exactly as
    // established and no effect was recorded.
    assert!(when.effects.is_empty());
    for record in trace.assertions() {
        assert_eq!(
            record.verdict,
            Verdict::Pass,
            "{} must hold",
            record.step_id
        );
    }
    // The conflict row's focus instant is the established value, not
    // the evaluation clock.
    let snapshot = trace
        .state()
        .iter()
        .find(|row| row.key.contains("task-1"))
        .expect("task-1 snapshot");
    let focused_at = snapshot
        .fields
        .iter()
        .find(|(field, _)| field == "focused_at")
        .expect("focused_at field");
    assert_eq!(
        focused_at.1,
        lekalo_core::scenario::TypedValue::Datetime("2026-09-05T10:00:00Z".to_owned())
    );
}

fn idempotent_replays_never_duplicate_effects() {
    let project = compile_board();
    let attachment_value: serde_json::Value =
        serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let evaluation = board_evaluation(&project, &attachment, &registry);
    let (name, scenario_bytes, _) = &SCENARIOS[2];
    let scenario = parse_scenario(name, scenario_bytes);
    let trace = evaluation.execute(&scenario).expect("trace");
    // The replay returned the recorded result with no new effects,
    // and exactly one focus event intent exists overall.
    let replay = &trace.when()[1];
    assert!(replay.replay_of.is_some());
    assert!(replay.effects.is_empty());
    let event_intents = trace
        .effects()
        .iter()
        .filter(|effect| effect.kind == lekalo_core::reference_evaluation::EffectKind::EventIntent)
        .count();
    assert_eq!(event_intents, 1);
    for record in trace.assertions() {
        assert_eq!(
            record.verdict,
            Verdict::Pass,
            "{} must hold",
            record.step_id
        );
    }
}

fn unsupported_semantics_are_explicit() {
    let project = compile_board();
    let attachment_value: serde_json::Value =
        serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let evaluation = board_evaluation(&project, &attachment, &registry);
    let (name, scenario_bytes, _) = &SCENARIOS[4];
    let scenario = parse_scenario(name, scenario_bytes);
    let trace = evaluation.execute(&scenario).expect("trace");
    let when = &trace.when()[0];
    match &when.outcome {
        Outcome::Unsupported { reason } => assert_eq!(*reason, "authorization"),
        other => panic!("expected unsupported authorization, got {other:?}"),
    }
    // The `unsupported` assertion passes because the capability is
    // declared known-absent, and the authorization assertion itself
    // stays explicitly undecided.
    assert!(matches!(
        trace.assertions()[0].verdict,
        Verdict::Unsupported("authorization")
    ));
    assert_eq!(trace.assertions()[1].verdict, Verdict::Pass);
    assert_eq!(trace.status(), Status::Unsupported);
}

fn mismatched_pins_refuse_the_whole_evaluation() {
    let project = compile_board();
    let attachment_value: serde_json::Value =
        serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let evaluation = board_evaluation(&project, &attachment, &registry);
    let (_, scenario_bytes, _) = &SCENARIOS[0];
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    let ir_ref = wire.get_mut("irRef").expect("irRef");
    ir_ref.as_object_mut().expect("irRef object").insert(
        "digest".to_owned(),
        serde_json::json!(
            "sha256:1111111111111111111111111111111111111111111111111111111111111111"
        ),
    );
    let scenario = ScenarioIr::from_value(&wire).expect("tampered scenario parses");
    let trace = evaluation.execute(&scenario).expect("trace");
    assert_eq!(trace.status(), Status::Unsupported);
    assert_eq!(trace.refusal(), Some("ir-pin-mismatch"));
    assert!(trace.when().is_empty() && trace.assertions().is_empty());
}

fn unknown_row_is_a_typed_declared_error() {
    let project = compile_board();
    let attachment_value: serde_json::Value =
        serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let evaluation = board_evaluation(&project, &attachment, &registry);
    let (_, scenario_bytes, _) = &SCENARIOS[0];
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    // Point the command at a task that was never established.
    let when = wire
        .get_mut("when")
        .and_then(|when| when.as_array_mut())
        .expect("when");
    let input = when[0]
        .get_mut("action")
        .expect("action")
        .get_mut("input")
        .expect("input");
    *input.get_mut("task_id").expect("task_id input") =
        serde_json::json!({"type": "string", "value": "task-404"});
    let scenario = ScenarioIr::from_value(&wire).expect("variant parses");
    let trace = evaluation.execute(&scenario).expect("trace");
    match &trace.when()[0].outcome {
        Outcome::Error {
            token,
            declared,
            violations,
        } => {
            assert_eq!(token.as_str(), "row-not-found");
            assert!(violations.is_empty());
            assert!(declared.contains(&"errors.board/task-not-found".to_owned()));
        }
        other => panic!("expected row-not-found, got {other:?}"),
    }
    // The rollback assertions hold and the command never succeeded,
    // so the reused scenario's event-intent assertion fails.
    assert_eq!(trace.assertions()[0].verdict, Verdict::Pass);
    assert_eq!(
        trace.assertions()[2].verdict,
        Verdict::Fail("emission-count")
    );
}

fn deterministic_ids_are_seed_stable_and_shape_valid() {
    use lekalo_core::reference_evaluation::derived_id;
    use lekalo_core::scenario::precondition::IdAlgorithm;
    use lekalo_core::scenario::TypedValue;
    // The same seed produces the byte-stable sequence; distinct
    // indexes stay distinct; the UUIDv4 spelling carries the pinned
    // version and variant nibbles.
    for seed in ["board", "user-1", "second seed"] {
        for algorithm in [IdAlgorithm::Sequence, IdAlgorithm::UuidV4] {
            let mut seen: Vec<String> = Vec::new();
            for index in 0..256u64 {
                let value = derived_id(seed, algorithm, index);
                assert_eq!(value, derived_id(seed, algorithm, index));
                seen.push(format!("{value:?}"));
            }
            let before = seen.len();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), before, "derivations must be distinct");
        }
    }
    let uuid = derived_id("board", IdAlgorithm::UuidV4, 7);
    match uuid {
        TypedValue::Uuid(text) => {
            assert_eq!(text.len(), 36);
            assert_eq!(text.as_bytes()[14], b'4');
            assert!(matches!(text.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        }
        other => panic!("expected uuid, got {other:?}"),
    }
    assert_eq!(
        derived_id("s", IdAlgorithm::Sequence, 3),
        TypedValue::String("s-3".to_owned())
    );
}
