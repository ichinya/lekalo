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
    trace_bytes, ErrorToken, Outcome, ReferenceEvaluation, ReferenceTrace, Status, Verdict,
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

/// The issue #107 correction round 1 fixtures: the exact public-API
/// reproductions of the four reviewed scenario-level defects, each
/// bound to its exact canonical golden bytes.
const CORRECTION_SCENARIOS: &[(&str, &[u8], &[u8])] = &[
    (
        "early-unsupported-then-replay",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/early-unsupported-then-replay.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/early-unsupported-then-replay.json.trace.json"
        ),
    ),
    (
        "early-unsupported-no-replay",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/early-unsupported-no-replay.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/early-unsupported-no-replay.json.trace.json"
        ),
    ),
    (
        "clock-declaration-order",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/clock-declaration-order.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/clock-declaration-order.json.trace.json"
        ),
    ),
    (
        "given-missing-reference",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/given-missing-reference.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/given-missing-reference.json.trace.json"
        ),
    ),
];

/// The issue #107 correction round 2 fixtures: the exact public-API
/// reproductions of the two reviewed semantic defects, each bound to
/// its exact scenario, attachment-variant, and canonical golden trace
/// bytes.
type Correction2Fixture = (&'static str, &'static [u8], &'static [u8], &'static [u8]);

const CORRECTION_2_FIXTURES: &[Correction2Fixture] = &[
    (
        "fraction-equal-before",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/fraction-equal-before.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/invariants-fraction-equal-before.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/fraction-equal-before.json.trace.json"
        ),
    ),
    (
        "all-empty-false",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/all-empty-false.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/invariants-all-empty-false.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/all-empty-false.json.trace.json"
        ),
    ),
];

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
        correction_goldens_match_committed_bytes();
        early_unsupported_step_keeps_input_log_aligned();
        unresolved_given_reference_is_typed_with_no_partial_state();
        early_unsupported_clocks_are_schema_valid();
        default_clock_is_the_first_declared_given_clock();
        fractional_datetime_literals_execute();
        correction_2_goldens_match_committed_bytes();
        equal_instant_fractions_stay_strictly_chronological();
        all_quantifier_resolves_the_collection();
        correction_3_goldens_match_committed_bytes();
        within_stays_numeric_at_year_boundaries();
        out_of_range_literal_rejects_the_round_trip();
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

/// The compiled board pins of one correction regression run.
fn correction_pins() -> (
    CompiledProject,
    InvariantTransitionAttachment,
    lekalo_core::error_contract::ErrorRegistry,
) {
    let selection = LoadSelection {
        project: Some(MODEL.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("board model failed to load: {}", outcome.to_json_string()),
    };
    let project = match lekalo_core::ir::compile(&model) {
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
    };
    let attachment_value: serde_json::Value =
        serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    (project, attachment, registry)
}

/// The schema clock pattern of the published trace contract:
/// `^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?Z$`.
fn is_utc_clock(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || !bytes.ends_with(b"Z") || bytes[10] != b'T' {
        return false;
    }
    let digits = |slice: &[u8]| slice.iter().all(|byte| byte.is_ascii_digit());
    if !(digits(&bytes[0..4])
        && bytes[4] == b'-'
        && digits(&bytes[5..7])
        && bytes[7] == b'-'
        && digits(&bytes[8..10]))
    {
        return false;
    }
    let clock = &text[11..text.len() - 1];
    let (clock, fraction) = match clock.split_once('.') {
        Some((clock, fraction)) => (clock, Some(fraction)),
        None => (clock, None),
    };
    let parts: Vec<&str> = clock.split(':').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| part.len() == 2 && digits(part.as_bytes()))
        && fraction.map_or(true, |fraction| {
            !fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// Every correction fixture matches its committed canonical golden
/// byte for byte, and the exact repro outcomes hold.
fn correction_goldens_match_committed_bytes() {
    let (project, attachment, registry) = correction_pins();
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    for (index, (name, scenario_bytes, golden)) in CORRECTION_SCENARIOS.iter().enumerate() {
        let scenario = parse_scenario(name, scenario_bytes);
        let trace = evaluation.execute(&scenario).expect("trace");
        let canonical = trace_bytes(&trace).expect("canonical trace bytes");
        assert_eq!(
            canonical.as_bytes(),
            *golden,
            "{name}: canonical trace diverges from the committed golden"
        );
        let expected = match index {
            2 => Status::Pass,
            _ => Status::Unsupported,
        };
        assert_eq!(trace.status(), expected, "{name}: overall status");
    }
}

/// An early-unsupported `when` step stays typed trace data: the
/// input bookkeeping stays aligned, so the following durable-key
/// execution and its explicit replay execute and replay without any
/// indexing panic (correction 1, review A1/B1).
fn early_unsupported_step_keeps_input_log_aligned() {
    let (project, attachment, registry) = correction_pins();
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    let (name, scenario_bytes, _) = &CORRECTION_SCENARIOS[0];
    let scenario = parse_scenario(name, scenario_bytes);
    let trace = evaluation
        .execute(&scenario)
        .expect("mixed unsupported/success/replay runs to a typed result");
    assert_eq!(trace.status(), Status::Unsupported);
    let when = trace.when();
    assert_eq!(when.len(), 3);
    match &when[0].outcome {
        Outcome::Unsupported { reason } => assert_eq!(*reason, "member-path"),
        other => panic!("expected unsupported member-path, got {other:?}"),
    }
    assert!(when[1].replay_of.is_none());
    assert_eq!(when[2].replay_of.as_deref(), Some("focus"));
    assert!(when[2].effects.is_empty());
    assert_eq!(trace.assertions()[1].verdict, Verdict::Pass);

    // Adjacent: an explicit replay whose input mismatches after an
    // early-unsupported step is a typed replay-mismatch, never a
    // panic.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    wire["when"][2]["action"]["input"]["user_id"] =
        serde_json::json!({"type": "string", "value": "user-2"});
    let variant = ScenarioIr::from_value(&wire).expect("variant parses");
    let trace = evaluation.execute(&variant).expect("variant trace");
    match &trace.when()[2].outcome {
        Outcome::Unsupported { reason } => assert_eq!(*reason, "replay-mismatch"),
        other => panic!("expected replay-mismatch, got {other:?}"),
    }

    // Adjacent: a durable re-invocation (no explicit replay) after an
    // early-unsupported step still replays the recorded result.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    wire["when"][2]
        .as_object_mut()
        .expect("replay step object")
        .remove("replay");
    let variant = ScenarioIr::from_value(&wire).expect("variant parses");
    let trace = evaluation.execute(&variant).expect("variant trace");
    assert_eq!(trace.when()[2].replay_of.as_deref(), Some("focus"));
    assert!(trace.when()[2].effects.is_empty());
}

/// An unresolved `given` selector or field leaf is a typed
/// unsupported establishment carrying the exact resolution reason,
/// with no partial row — never a silently dropped field under a
/// `pass` (correction 1, review A2/B2).
fn unresolved_given_reference_is_typed_with_no_partial_state() {
    let (project, attachment, registry) = correction_pins();
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    let (name, scenario_bytes, _) = &CORRECTION_SCENARIOS[3];
    let scenario = parse_scenario(name, scenario_bytes);
    let trace = evaluation.execute(&scenario).expect("trace");
    assert_eq!(trace.status(), Status::Unsupported);
    let given = trace.given();
    match &given[1].status {
        Err(reason) => assert_eq!(*reason, "member-path"),
        Ok(()) => panic!("unresolved establishment must not report established"),
    }
    assert!(given[1].row.is_none());
    // Only the fully resolved row materialized: no partial row with a
    // dropped field exists in the final state.
    assert!(trace.state().iter().all(|row| !row.key.contains("task-2")));
    // The result assertions still decide, over the actually
    // established state.
    assert_eq!(trace.assertions()[0].verdict, Verdict::Pass);
    assert_eq!(trace.assertions()[1].verdict, Verdict::Pass);

    // Adjacent: an unresolved selector leaf is equally typed and
    // merges nothing.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    wire["given"][0]["precondition"]["selector"][0]["equals"] = serde_json::json!({
        "$ref": "given-value", "id": "setup_row_two", "path": "task_id"
    });
    let variant = ScenarioIr::from_value(&wire).expect("variant parses");
    let trace = evaluation.execute(&variant).expect("variant trace");
    match &trace.given()[0].status {
        Err(reason) => assert_eq!(*reason, "given-value-unavailable"),
        Ok(()) => panic!("unresolved selector must not report established"),
    }
    assert!(trace.state().iter().all(|row| !row.key.contains("task-1")));
}

/// Every reachable early-unsupported `when` outcome serializes a
/// schema-valid deterministic clock: the declared evaluation clock,
/// else the documented epoch fallback (correction 1, review A3/B3).
fn early_unsupported_clocks_are_schema_valid() {
    let (project, attachment, registry) = correction_pins();
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    let (_, scenario_bytes, _) = &CORRECTION_SCENARIOS[1];
    let declared = "2026-09-08T12:00:00Z";
    let epoch = "1970-01-01T00:00:00Z";

    // The reachable early reasons (review A3), each driven by one
    // wire-valid variant: (variant wire text, reason, expected
    // clock). The clock-unresolved and step-ref-shape tokens stay
    // unreachable through the public constructors.
    let mut variants: Vec<(String, &'static str, &'static str)> = Vec::new();
    variants.push((
        String::from_utf8_lossy(scenario_bytes).into_owned(),
        "member-path",
        declared,
    ));

    // step-output-unavailable: a prior step that produced no output.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    wire["when"]
        .as_array_mut()
        .expect("when")
        .push(serde_json::json!({
            "action": {
                "input": {
                    "task_id": {"$ref": "step-output", "id": "bad_first", "path": "missing"},
                    "user_id": {"type": "string", "value": "user-1"}
                },
                "kind": "invoke",
                "operation": "board.command.focus_task"
            },
            "stepId": "uses_bad"
        }));
    variants.push((
        serde_json::to_string(&wire).expect("wire"),
        "step-output-unavailable",
        declared,
    ));

    // given-value-unavailable: a given-value ref to a control step
    // that yields no value.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    wire["given"]
        .as_array_mut()
        .expect("given")
        .push(serde_json::json!({
            "precondition": {"algorithm": "uuidv4", "kind": "id_source", "seed": "board-seed"},
            "stepId": "ids_row"
        }));
    wire["when"][0]["action"]["input"]["task_id"] =
        serde_json::json!({"$ref": "given-value", "id": "ids_row"});
    variants.push((
        serde_json::to_string(&wire).expect("wire"),
        "given-value-unavailable",
        declared,
    ));

    // fixture: an opaque fixture reference cannot execute.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    wire["when"][0]["action"]["input"]["task_id"] =
        serde_json::json!({"$ref": "fixture", "id": "core/board-seed"});
    variants.push((
        serde_json::to_string(&wire).expect("wire"),
        "fixture",
        declared,
    ));

    // semantic-ref-value: semantic references carry no runtime value.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    wire["when"][0]["action"]["input"]["task_id"] =
        serde_json::json!({"$ref": "entity", "id": "board.user_task_planning"});
    variants.push((
        serde_json::to_string(&wire).expect("wire"),
        "semantic-ref-value",
        declared,
    ));

    // actor-ref-unavailable: an actor reference resolves only in the
    // actor position.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    wire["when"][0]["action"]["input"]["task_id"] =
        serde_json::json!({"$ref": "actor", "id": "board.user"});
    variants.push((
        serde_json::to_string(&wire).expect("wire"),
        "actor-ref-unavailable",
        declared,
    ));

    // replay-key-missing: an explicit replay with no recorded key.
    let idempotent_bytes = &SCENARIOS[2].1;
    let mut wire: serde_json::Value = serde_json::from_slice(idempotent_bytes).expect("JSON");
    wire["when"][1]["action"]["idempotencyKey"] =
        serde_json::json!({"type": "string", "value": "user-9"});
    variants.push((
        serde_json::to_string(&wire).expect("wire"),
        "replay-key-missing",
        declared,
    ));

    // replay-mismatch: an explicit replay whose input diverges.
    let mut wire: serde_json::Value = serde_json::from_slice(idempotent_bytes).expect("JSON");
    wire["when"][1]["action"]["input"]["user_id"] =
        serde_json::json!({"type": "string", "value": "user-2"});
    variants.push((
        serde_json::to_string(&wire).expect("wire"),
        "replay-mismatch",
        declared,
    ));

    // The epoch fallback: no declared clock at all.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    wire["given"]
        .as_array_mut()
        .expect("given")
        .retain(|step| step["precondition"]["kind"] != "clock");
    variants.push((
        serde_json::to_string(&wire).expect("wire"),
        "member-path",
        epoch,
    ));

    for (index, (wire_text, reason, clock)) in variants.iter().enumerate() {
        let value: serde_json::Value = serde_json::from_str(wire_text).expect("variant JSON");
        let variant = ScenarioIr::from_value(&value)
            .unwrap_or_else(|set| panic!("variant {index} ({reason}) must parse: {set:?}"));
        let trace = evaluation.execute(&variant).expect("variant trace");
        let record = trace
            .when()
            .iter()
            .rev()
            .find(|record| matches!(record.outcome, Outcome::Unsupported { .. }))
            .unwrap_or_else(|| panic!("variant {index} ({reason}) must stay unsupported"));
        match &record.outcome {
            Outcome::Unsupported {
                reason: actual_reason,
            } => assert_eq!(*actual_reason, *reason, "variant {index}"),
            other => panic!("variant {index}: expected unsupported, got {other:?}"),
        }
        assert_eq!(record.clock, *clock, "variant {index}: clock");
        assert!(
            is_utc_clock(&record.clock),
            "variant {index}: clock must match the schema pattern"
        );
        // The exported trace must carry the same valid bytes.
        trace_bytes(&trace).expect("variant trace exports");
    }
}

/// The default clock is the first declared `given` clock in scenario
/// order, independent of step-id lexical order (correction 1, review
/// A4/B4).
fn default_clock_is_the_first_declared_given_clock() {
    let (project, attachment, registry) = correction_pins();
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    // The committed golden: `aaa_later` (2030) is declared after
    // `at_noon` (2026) and sorts lexicographically first, yet the
    // command runs at the first declared 2026 clock.
    let (name, scenario_bytes, _) = &CORRECTION_SCENARIOS[2];
    let scenario = parse_scenario(name, scenario_bytes);
    let trace = evaluation.execute(&scenario).expect("trace");
    assert_eq!(trace.when()[0].clock, "2026-09-08T12:00:00Z");
    assert_eq!(trace.status(), Status::Pass);

    // Adjacent: reversing the declaration order reverses the default.
    let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
    let given = wire
        .get_mut("given")
        .and_then(|given| given.as_array_mut())
        .expect("given");
    given.swap(1, 2);
    let variant = ScenarioIr::from_value(&wire).expect("variant parses");
    let trace = evaluation.execute(&variant).expect("variant trace");
    assert_eq!(trace.when()[0].clock, "2030-01-01T00:00:00Z");
    assert_eq!(
        trace.assertions()[1].verdict,
        Verdict::Fail("field-mismatch")
    );
}

/// Grammar-valid fractional-second datetime literals execute, with
/// the exact fraction preserved after offset normalization, in UTC
/// and offset spellings (correction 1, review A5/B5).
fn fractional_datetime_literals_execute() {
    let (project, _attachment, registry) = correction_pins();
    for (literal, normalized) in [
        ("2026-09-08T12:00:00.123Z", "2026-09-08T12:00:00.123Z"),
        ("2026-09-08T15:30:00.456+03:00", "2026-09-08T12:30:00.456Z"),
    ] {
        let mut attachment_wire: serde_json::Value =
            serde_json::from_slice(ATTACHMENT).expect("attachment JSON");
        attachment_wire["transitions"][0]["assignments"][0]["value"] = serde_json::json!({
            "kind": "literal",
            "value": {"kind": "datetime", "value": literal}
        });
        let attachment =
            InvariantTransitionAttachment::from_value(&attachment_wire).expect("attachment parses");
        let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
        let (_, scenario_bytes, _) = &SCENARIOS[2];
        let mut wire: serde_json::Value = serde_json::from_slice(scenario_bytes).expect("JSON");
        wire["then"][1]["assertion"]["fields"]["focused_at"] = serde_json::json!({
            "value": {"type": "datetime", "value": normalized}
        });
        let scenario = ScenarioIr::from_value(&wire).expect("scenario parses");
        let trace = evaluation.execute(&scenario).expect("trace");
        match &trace.when()[0].outcome {
            Outcome::Ok { .. } => {}
            other => panic!("fractional literal {literal} must execute, got {other:?}"),
        }
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
            lekalo_core::scenario::TypedValue::Datetime(normalized.to_owned())
        );
        assert_eq!(trace.assertions()[1].verdict, Verdict::Pass);
        assert_eq!(trace.status(), Status::Pass);
    }
}

/// Evaluates one correction-2 fixture: the exact scenario against the
/// exact attachment variant over the committed board model.
fn correction_2_execute(index: usize) -> ReferenceTrace {
    let project = compile_board();
    let (_, scenario_bytes, attachment_bytes, _) = &CORRECTION_2_FIXTURES[index];
    let attachment_value: serde_json::Value =
        serde_json::from_slice(attachment_bytes).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let scenario_wire: serde_json::Value =
        serde_json::from_slice(scenario_bytes).expect("scenario JSON");
    let scenario = ScenarioIr::from_value(&scenario_wire).expect("scenario parses");
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    evaluation.execute(&scenario).expect("trace")
}

/// Rebuilds one correction-2 evaluation with a mutated attachment
/// precondition (and optional scenario mutation), as a public wire
/// variant.
fn correction_2_variant(
    index: usize,
    precondition: serde_json::Value,
    scenario_mutation: impl FnOnce(&mut serde_json::Value),
) -> ReferenceTrace {
    let project = compile_board();
    let (_, scenario_bytes, attachment_bytes, _) = &CORRECTION_2_FIXTURES[index];
    let mut attachment_wire: serde_json::Value =
        serde_json::from_slice(attachment_bytes).expect("attachment JSON");
    attachment_wire["transitions"][0]["preconditions"] = serde_json::json!([precondition]);
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_wire).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let mut scenario_wire: serde_json::Value =
        serde_json::from_slice(scenario_bytes).expect("scenario JSON");
    scenario_mutation(&mut scenario_wire);
    let scenario = ScenarioIr::from_value(&scenario_wire).expect("scenario parses");
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    evaluation.execute(&scenario).expect("trace")
}

/// The declared errors of the focus transition, byte-sorted.
fn focus_declared() -> Vec<String> {
    vec![
        String::from("errors.board/focus-conflict"),
        String::from("errors.board/task-not-found"),
    ]
}

/// Every correction-2 fixture matches its committed canonical golden
/// byte for byte, and the exact repro outcomes hold: equal-instant
/// fraction spellings refuse `before` with no effects, and `all`
/// over an empty collection executes vacuously (correction 2,
/// review B2.1/B2.2).
fn correction_2_goldens_match_committed_bytes() {
    let fraction_trace = correction_2_execute(0);
    let canonical = trace_bytes(&fraction_trace).expect("canonical trace bytes");
    assert_eq!(
        canonical.as_bytes(),
        CORRECTION_2_FIXTURES[0].3,
        "fraction-equal-before: canonical trace diverges from the committed golden"
    );
    assert_eq!(fraction_trace.status(), Status::Fail);
    for record in fraction_trace.when() {
        match &record.outcome {
            Outcome::Error {
                token,
                declared,
                violations,
            } => {
                assert_eq!(*token, ErrorToken::PreconditionFailed);
                assert_eq!(*declared, focus_declared());
                assert!(violations.is_empty());
            }
            other => panic!("expected precondition-failed error, got {other:?}"),
        }
    }
    assert_eq!(fraction_trace.when()[1].replay_of.as_deref(), Some("focus"));
    assert!(fraction_trace.when()[1].effects.is_empty());
    assert!(fraction_trace.effects().is_empty());
    assert_eq!(
        fraction_trace.assertions()[0].verdict,
        Verdict::Fail("replay-error")
    );
    // The final state keeps the given row untouched: focused_at is
    // still null and no event intent was emitted.
    let snapshot = &fraction_trace.state()[0];
    let focused_at = snapshot
        .fields
        .iter()
        .find(|(field, _)| field == "focused_at")
        .expect("focused_at field");
    assert_eq!(focused_at.1, lekalo_core::scenario::TypedValue::Null);

    let all_trace = correction_2_execute(1);
    let canonical = trace_bytes(&all_trace).expect("canonical trace bytes");
    assert_eq!(
        canonical.as_bytes(),
        CORRECTION_2_FIXTURES[1].3,
        "all-empty-false: canonical trace diverges from the committed golden"
    );
    assert_eq!(all_trace.status(), Status::Pass);
    assert!(matches!(all_trace.when()[0].outcome, Outcome::Ok { .. }));
    assert_eq!(all_trace.when()[1].replay_of.as_deref(), Some("focus"));
    // The replay adds no effects; the focus step wrote the row and
    // emitted the declared event intent.
    assert!(all_trace.when()[1].effects.is_empty());
    assert_eq!(all_trace.effects().len(), 2);
    for assertion in all_trace.assertions() {
        assert_eq!(assertion.verdict, Verdict::Pass);
    }
}

/// Chronological datetime ordering compares fraction digits
/// numerically: `.5`, `.50`, an absent fraction, and `.0` denote one
/// instant, so strict `before`/`after` over them refuses the command
/// with `precondition-failed` and no effects, `within` accepts the
/// boundary, and unequal fractions keep executing (correction 2,
/// review B2.1/A-D1).
fn equal_instant_fractions_stay_strictly_chronological() {
    // after(.1230Z, .123Z): the symmetric strict probe refuses too.
    let trace = correction_2_variant(
        0,
        serde_json::json!({
            "op": "after",
            "left": {"kind": "datetime", "value": "2026-09-08T12:00:00.1230Z"},
            "right": {"kind": "datetime", "value": "2026-09-08T12:00:00.123Z"}
        }),
        |_| {},
    );
    assert_eq!(trace.status(), Status::Fail);
    match &trace.when()[0].outcome {
        Outcome::Error { token, .. } => assert_eq!(*token, ErrorToken::PreconditionFailed),
        other => panic!("expected precondition-failed, got {other:?}"),
    }
    assert!(trace.effects().is_empty());

    // before(absent fraction, .0): one instant, so the strict probe
    // refuses as well.
    let trace = correction_2_variant(
        0,
        serde_json::json!({
            "op": "before",
            "left": {"kind": "datetime", "value": "2026-09-08T12:00:00Z"},
            "right": {"kind": "datetime", "value": "2026-09-08T12:00:00.0Z"}
        }),
        |_| {},
    );
    match &trace.when()[0].outcome {
        Outcome::Error { token, .. } => assert_eq!(*token, ErrorToken::PreconditionFailed),
        other => panic!("expected precondition-failed, got {other:?}"),
    }

    // before(same spelling): the control still refuses.
    let trace = correction_2_variant(
        0,
        serde_json::json!({
            "op": "before",
            "left": {"kind": "datetime", "value": "2026-09-08T12:00:00.5Z"},
            "right": {"kind": "datetime", "value": "2026-09-08T12:00:00.5Z"}
        }),
        |_| {},
    );
    match &trace.when()[0].outcome {
        Outcome::Error { token, .. } => assert_eq!(*token, ErrorToken::PreconditionFailed),
        other => panic!("expected precondition-failed, got {other:?}"),
    }

    // before(.1235Z, .123Z): unequal instants still order strictly
    // and execute.
    let trace = correction_2_variant(
        0,
        serde_json::json!({
            "op": "before",
            "left": {"kind": "datetime", "value": "2026-09-08T12:00:00.123Z"},
            "right": {"kind": "datetime", "value": "2026-09-08T12:00:00.1235Z"}
        }),
        |_| {},
    );
    assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
    assert_eq!(trace.status(), Status::Pass);
    assert_eq!(trace.effects().len(), 2);

    // within(prior.focused_at, 1s) at the upper boundary: the row
    // was given `12:00:01.00Z` against the `12:00:00Z` clock, which
    // is the same instant as the span's upper bound, so it lies
    // within and executes.
    let trace = correction_2_variant(
        0,
        serde_json::json!({
            "op": "within",
            "left": {"kind": "field", "field": "focused_at"},
            "duration": {"unit": "seconds", "amount": 1}
        }),
        |wire| {
            wire["given"][0]["precondition"]["fields"]["focused_at"] = serde_json::json!({
                "type": "datetime",
                "value": "2026-09-08T12:00:01.00Z"
            });
        },
    );
    assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
    assert_eq!(trace.status(), Status::Pass);
}

/// `all` resolves and validates its collection operand through the
/// common member helper: an empty collection is vacuously true
/// without evaluating the predicate, a scalar operand is a typed
/// `incompatible-kind`, and a nonempty collection takes the member
/// predicate decision in the same context under the documented
/// no-member-binding interpretation (correction 2, review
/// B2.2).
fn all_quantifier_resolves_the_collection() {
    // Empty collection with a predicate that reads an unset input:
    // vacuously true without evaluating it.
    let trace = correction_2_variant(
        1,
        serde_json::json!({
            "op": "all",
            "from": {"kind": "list", "items": []},
            "predicate": {"op": "not_null", "operand": {"kind": "input", "field": "missing"}}
        }),
        |_| {},
    );
    assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
    assert_eq!(trace.status(), Status::Pass);

    // A null operand is the empty collection: vacuously true.
    let trace = correction_2_variant(
        1,
        serde_json::json!({
            "op": "all",
            "from": {"kind": "null"},
            "predicate": {"op": "is_null", "operand": {"kind": "input", "field": "task_id"}}
        }),
        |_| {},
    );
    assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
    assert_eq!(trace.status(), Status::Pass);

    // Nonempty collection with a true member decision: executes.
    let trace = correction_2_variant(
        1,
        serde_json::json!({
            "op": "all",
            "from": {"kind": "list", "items": [
                {"kind": "string", "value": "a"},
                {"kind": "string", "value": "b"}
            ]},
            "predicate": {"op": "not_null", "operand": {"kind": "input", "field": "task_id"}}
        }),
        |_| {},
    );
    assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
    assert_eq!(trace.status(), Status::Pass);
    assert_eq!(trace.effects().len(), 2);

    // Nonempty collection with a false member decision: refuses
    // with precondition-failed and no effects.
    let trace = correction_2_variant(
        1,
        serde_json::json!({
            "op": "all",
            "from": {"kind": "list", "items": [{"kind": "string", "value": "a"}]},
            "predicate": {"op": "is_null", "operand": {"kind": "input", "field": "task_id"}}
        }),
        |_| {},
    );
    match &trace.when()[0].outcome {
        Outcome::Error { token, .. } => assert_eq!(*token, ErrorToken::PreconditionFailed),
        other => panic!("expected precondition-failed, got {other:?}"),
    }
    assert!(trace.effects().is_empty());
    assert_eq!(trace.status(), Status::Fail);

    // A scalar operand is not a collection: typed incompatible-kind,
    // with no writes and no event intents. The scenario assertions
    // are adjusted to expect exactly that reality (row untouched, no
    // emission), so the overall status isolates the unsupported step.
    let trace = correction_2_variant(
        1,
        serde_json::json!({
            "op": "all",
            "from": {"kind": "integer", "value": 7},
            "predicate": {"op": "not_null", "operand": {"kind": "input", "field": "task_id"}}
        }),
        |wire| {
            wire["then"][1]["assertion"]["fields"]["focused_at"] = serde_json::json!({
                "value": {"type": "null", "value": null}
            });
            wire["then"]
                .as_array_mut()
                .expect("then array")
                .pop()
                .expect("emitted assertion");
        },
    );
    assert_eq!(trace.status(), Status::Unsupported);
    match &trace.when()[0].outcome {
        Outcome::Unsupported { reason } => assert_eq!(*reason, "incompatible-kind"),
        other => panic!("expected unsupported incompatible-kind, got {other:?}"),
    }
    assert_eq!(
        trace.assertions()[0].verdict,
        Verdict::Unsupported("incompatible-kind")
    );
    assert_eq!(trace.assertions()[1].verdict, Verdict::Pass);
    assert_eq!(trace.assertions().len(), 2);
    assert!(trace.when()[1].effects.is_empty());
    assert!(trace.effects().is_empty());
    let snapshot = &trace.state()[0];
    let focused_at = snapshot
        .fields
        .iter()
        .find(|(field, _)| field == "focused_at")
        .expect("focused_at field");
    assert_eq!(focused_at.1, lekalo_core::scenario::TypedValue::Null);

    // Adjacent control: `any` over an empty collection stays false,
    // so the same strict command refuses.
    let trace = correction_2_variant(
        1,
        serde_json::json!({
            "op": "any",
            "from": {"kind": "list", "items": []},
            "predicate": {"op": "not_null", "operand": {"kind": "input", "field": "task_id"}}
        }),
        |_| {},
    );
    match &trace.when()[0].outcome {
        Outcome::Error { token, .. } => assert_eq!(*token, ErrorToken::PreconditionFailed),
        other => panic!("expected precondition-failed, got {other:?}"),
    }
}

/// The issue #107 correction round 3 fixtures: the exact public-API
/// reproductions of the two reviewed datetime boundary defects, each
/// bound to its exact scenario, attachment-variant, and canonical
/// golden trace bytes.
type Correction3Fixture = (&'static str, &'static [u8], &'static [u8], &'static [u8]);

const CORRECTION_3_FIXTURES: &[Correction3Fixture] = &[
    // B3.1 exact upper edge: the offset literal normalizes to year
    // 10000, so the assignment refuses before any write or event.
    (
        "offset-range-upper",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/offset-range-upper.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/invariants-offset-range-upper.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/offset-range-upper.json.trace.json"
        ),
    ),
    // B3.1 exact lower edge: the offset literal normalizes to year 0.
    (
        "offset-range-lower",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/offset-range-lower.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/invariants-offset-range-lower.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/offset-range-lower.json.trace.json"
        ),
    ),
    // B3.1 in-range control: the ordinary year rollover still
    // executes and writes the exact normalized UTC spelling.
    (
        "offset-range-control",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/offset-range-control.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/invariants-offset-range-control.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/offset-range-control.json.trace.json"
        ),
    ),
    // B3.2 exact upper boundary: `within(now, 1s)` at
    // `9999-12-31T23:59:59Z` (whose serialized span would leave the
    // four-digit range) executes.
    (
        "within-year-upper",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/within-year-upper.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/invariants-within-year.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/within-year-upper.json.trace.json"
        ),
    ),
    // B3.2 lower boundary control: the same predicate at
    // `0001-01-01T00:00:00Z` keeps executing.
    (
        "within-year-lower",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/within-year-lower.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/invariants-within-year.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/within-year-lower.json.trace.json"
        ),
    ),
    // B3.2 maximum accepted duration: `within(now, 31536000 days)`
    // (whose serialized bounds would also leave the four-digit
    // range) executes at the ordinary clock.
    (
        "within-max-duration",
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/scenarios/within-max-duration.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/invariants-within-max-duration.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/reference-evaluation/golden/within-max-duration.json.trace.json"
        ),
    ),
];

/// Evaluates one correction-3 fixture: the exact scenario against the
/// exact attachment variant over the committed board model.
fn correction_3_execute(index: usize) -> ReferenceTrace {
    let project = compile_board();
    let (_, scenario_bytes, attachment_bytes, _) = &CORRECTION_3_FIXTURES[index];
    let attachment_value: serde_json::Value =
        serde_json::from_slice(attachment_bytes).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_value).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let scenario_wire: serde_json::Value =
        serde_json::from_slice(scenario_bytes).expect("scenario JSON");
    let scenario = ScenarioIr::from_value(&scenario_wire).expect("scenario parses");
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    evaluation.execute(&scenario).expect("trace")
}

/// Rebuilds one correction-3 evaluation over a within-year fixture
/// with a mutated `within` precondition reading the given row, a
/// mutated given row value, and an optional mutated clock, as a
/// public wire variant.
fn correction_3_within_variant(
    duration: serde_json::Value,
    clock: &str,
    given_focused_at: Option<&str>,
) -> ReferenceTrace {
    let project = compile_board();
    let (_, scenario_bytes, attachment_bytes, _) = &CORRECTION_3_FIXTURES[3];
    let mut attachment_wire: serde_json::Value =
        serde_json::from_slice(attachment_bytes).expect("attachment JSON");
    attachment_wire["transitions"][0]["preconditions"] = serde_json::json!([
        {
            "op": "within",
            "left": {"kind": "field", "field": "focused_at"},
            "duration": duration
        }
    ]);
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_wire).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let mut scenario_wire: serde_json::Value =
        serde_json::from_slice(scenario_bytes).expect("scenario JSON");
    scenario_wire["given"][1]["precondition"]["at"]["value"] = serde_json::json!(clock);
    scenario_wire["then"][1]["assertion"]["fields"]["focused_at"]["value"]["value"] =
        serde_json::json!(clock);
    match given_focused_at {
        Some(value) => {
            scenario_wire["given"][0]["precondition"]["fields"]["focused_at"] =
                serde_json::json!({ "type": "datetime", "value": value });
        }
        None => {
            scenario_wire["given"][0]["precondition"]["fields"]["focused_at"] =
                serde_json::json!({ "type": "null", "value": null });
        }
    }
    // The command writes `now`, so the fixture's entity-state
    // assertion (the clock spelling) stays untouched.
    let scenario = ScenarioIr::from_value(&scenario_wire).expect("scenario parses");
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    evaluation.execute(&scenario).expect("trace")
}

/// Every correction-3 fixture matches its committed canonical golden
/// byte for byte, and the exact repro outcomes hold: an offset
/// literal whose normalization leaves the four-digit runtime range is
/// a typed unsupported outcome with zero effects, the in-range
/// rollover control still writes the exact normalized spelling, and
/// `within` executes at both year boundaries and at the maximum
/// accepted duration (correction 3, review B3.1/B3.2).
fn correction_3_goldens_match_committed_bytes() {
    // --- B3.1: both range edges refuse with zero effects. ---
    for index in [0usize, 1] {
        let trace = correction_3_execute(index);
        let canonical = trace_bytes(&trace).expect("canonical trace bytes");
        assert_eq!(
            canonical.as_bytes(),
            CORRECTION_3_FIXTURES[index].3,
            "{}: canonical trace diverges from the committed golden",
            CORRECTION_3_FIXTURES[index].0
        );
        assert_eq!(trace.status(), Status::Unsupported);
        for record in trace.when() {
            match &record.outcome {
                Outcome::Unsupported { reason } => {
                    assert_eq!(*reason, "datetime-out-of-range");
                }
                other => panic!("expected unsupported datetime-out-of-range, got {other:?}"),
            }
        }
        assert_eq!(trace.when()[1].replay_of.as_deref(), Some("focus"));
        assert!(trace.when()[1].effects.is_empty());
        assert!(trace.effects().is_empty());
        assert_eq!(
            trace.assertions()[0].verdict,
            Verdict::Unsupported("datetime-out-of-range")
        );
        assert_eq!(trace.assertions()[1].verdict, Verdict::Pass);
        assert_eq!(trace.assertions().len(), 2);
        let snapshot = &trace.state()[0];
        let focused_at = snapshot
            .fields
            .iter()
            .find(|(field, _)| field == "focused_at")
            .expect("focused_at field");
        assert_eq!(focused_at.1, lekalo_core::scenario::TypedValue::Null);
    }

    // --- B3.1: the in-range rollover control executes. ---
    let trace = correction_3_execute(2);
    let canonical = trace_bytes(&trace).expect("canonical trace bytes");
    assert_eq!(
        canonical.as_bytes(),
        CORRECTION_3_FIXTURES[2].3,
        "offset-range-control: canonical trace diverges from the committed golden"
    );
    assert_eq!(trace.status(), Status::Pass);
    assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
    assert_eq!(trace.when()[1].replay_of.as_deref(), Some("focus"));
    assert!(trace.when()[1].effects.is_empty());
    assert_eq!(trace.effects().len(), 2);
    let snapshot = &trace.state()[0];
    let focused_at = snapshot
        .fields
        .iter()
        .find(|(field, _)| field == "focused_at")
        .expect("focused_at field");
    assert_eq!(
        focused_at.1,
        lekalo_core::scenario::TypedValue::Datetime("2025-12-31T23:00:00.1Z".to_owned())
    );
    for assertion in trace.assertions() {
        assert_eq!(assertion.verdict, Verdict::Pass);
    }

    // --- B3.2: both year boundaries and the maximum accepted
    // duration execute with the clock written (`now` lies within its
    // own span at zero distance). ---
    let written = [
        "9999-12-31T23:59:59Z",
        "0001-01-01T00:00:00Z",
        "2026-09-08T12:00:00Z",
    ];
    for index in [3usize, 4, 5] {
        let trace = correction_3_execute(index);
        let canonical = trace_bytes(&trace).expect("canonical trace bytes");
        assert_eq!(
            canonical.as_bytes(),
            CORRECTION_3_FIXTURES[index].3,
            "{}: canonical trace diverges from the committed golden",
            CORRECTION_3_FIXTURES[index].0
        );
        assert_eq!(trace.status(), Status::Pass);
        assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
        assert!(trace.when()[1].effects.is_empty());
        assert_eq!(trace.effects().len(), 2);
        for assertion in trace.assertions() {
            assert_eq!(assertion.verdict, Verdict::Pass);
        }
        let snapshot = &trace.state()[0];
        let focused_at = snapshot
            .fields
            .iter()
            .find(|(field, _)| field == "focused_at")
            .expect("focused_at field");
        assert_eq!(
            focused_at.1,
            lekalo_core::scenario::TypedValue::Datetime(written[index - 3].to_owned())
        );
    }
}

/// `within` compares the span numerically against the elapsed
/// distance, so it stays exact at nonzero distances on the upper
/// year boundary: the inclusive endpoints accept an operand exactly
/// one span away (including a fractional operand at exact
/// whole-second distance), and one step beyond the span refuses with
/// no effects (correction 3, review B3.2).
fn within_stays_numeric_at_year_boundaries() {
    // Upper boundary clock 9999-12-31T23:59:59Z with the row exactly
    // one span below: inclusive lower endpoint accepts.
    let trace = correction_3_within_variant(
        serde_json::json!({ "unit": "seconds", "amount": 1 }),
        "9999-12-31T23:59:59Z",
        Some("9999-12-31T23:59:58Z"),
    );
    assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
    assert_eq!(trace.status(), Status::Pass);

    // A fractional operand at exact whole-second distance orders
    // numerically: `.5` is after the absent fraction of the lower
    // bound, so it lies within.
    let trace = correction_3_within_variant(
        serde_json::json!({ "unit": "seconds", "amount": 1 }),
        "9999-12-31T23:59:59Z",
        Some("9999-12-31T23:59:58.5Z"),
    );
    assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
    assert_eq!(trace.status(), Status::Pass);

    // One step beyond the span refuses with no effects.
    let trace = correction_3_within_variant(
        serde_json::json!({ "unit": "seconds", "amount": 1 }),
        "9999-12-31T23:59:59Z",
        Some("9999-12-31T23:59:57.9Z"),
    );
    match &trace.when()[0].outcome {
        Outcome::Error { token, .. } => assert_eq!(*token, ErrorToken::PreconditionFailed),
        other => panic!("expected precondition-failed, got {other:?}"),
    }
    assert!(trace.effects().is_empty());
    assert_eq!(trace.status(), Status::Fail);

    // Lower boundary clock 0001-01-01T00:00:00Z with the row exactly
    // one span above: inclusive upper endpoint accepts, both at the
    // one-second span and at the maximum accepted duration.
    for duration in [
        serde_json::json!({ "unit": "seconds", "amount": 1 }),
        serde_json::json!({ "unit": "days", "amount": 31_536_000 }),
    ] {
        let trace = correction_3_within_variant(
            duration,
            "0001-01-01T00:00:00Z",
            Some("0001-01-01T00:00:01Z"),
        );
        assert!(matches!(trace.when()[0].outcome, Outcome::Ok { .. }));
        assert_eq!(trace.status(), Status::Pass);
    }
}

/// An unrepresentable normalized literal is outside the runtime
/// typed-value contract: feeding each refused normalization result
/// back through the public scenario constructor rejects it
/// (`graph.input-invalid`), proving the evaluator refuses exactly
/// what the contract cannot represent, while the assignment literal
/// itself stays legal attachment wire data — the refusal belongs at
/// normalization, not at parse (correction 3, review B3.1).
fn out_of_range_literal_rejects_the_round_trip() {
    let scenario_wire: serde_json::Value =
        serde_json::from_slice(CORRECTION_3_FIXTURES[0].1).expect("scenario JSON");
    for committed in ["10000-01-01T01:00:00.1Z", "0000-12-31T23:00:00.1Z"] {
        let mut wire = scenario_wire.clone();
        wire["given"][0]["precondition"]["fields"]["focused_at"] =
            serde_json::json!({ "type": "datetime", "value": committed });
        let outcome = ScenarioIr::from_value(&wire);
        assert!(outcome.is_err(), "{committed}: round-trip must reject");
        let detail = format!("{:?}", outcome.err());
        assert!(detail.contains("input-invalid"), "{committed}: {detail}");
    }

    // The refusal fixtures still execute to the typed unsupported
    // outcome through the public constructor path (assignment value
    // shape is legal wire data; the literal is what normalization
    // refuses).
    let project = compile_board();
    let attachment_wire: serde_json::Value =
        serde_json::from_slice(CORRECTION_3_FIXTURES[0].2).expect("attachment JSON");
    let attachment =
        InvariantTransitionAttachment::from_value(&attachment_wire).expect("attachment parses");
    let registry =
        lekalo_core::error_contract::ErrorRegistry::from_bytes(REGISTRY).expect("registry parses");
    let evaluation = ReferenceEvaluation::new(&project, &attachment, &registry);
    let scenario = ScenarioIr::from_value(&scenario_wire).expect("baseline scenario parses");
    let trace = evaluation.execute(&scenario).expect("trace");
    assert_eq!(trace.status(), Status::Unsupported);
    match &trace.when()[0].outcome {
        Outcome::Unsupported { reason } => assert_eq!(*reason, "datetime-out-of-range"),
        other => panic!("expected unsupported datetime-out-of-range, got {other:?}"),
    }
}
