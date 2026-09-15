//! Issue #31 conformance suite regression tests: the reference adapter
//! passes, every fixture class is detected with its exact class, the
//! badge names exact versions only, and both report projections are
//! deterministic and redacted.

use lekalo_core::adapter_conformance::{run, SuiteOptions};
use lekalo_core::target_protocol::transport::AdapterCommand;
use std::path::{Path, PathBuf};

fn adapter_command(extra: &[&str]) -> AdapterCommand {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/target-protocol/fake-adapter.mjs")
        .display()
        .to_string();
    let mut args = vec![script];
    let mut extra = extra.iter().map(|s| s.to_string()).collect();
    args.append(&mut extra);
    AdapterCommand {
        program: PathBuf::from("node"),
        args,
    }
}

fn run_suite(
    command: &AdapterCommand,
    profile: &str,
) -> lekalo_core::adapter_conformance::SuiteOutcome {
    let options = SuiteOptions {
        profile: lekalo_core::adapter_conformance::Profile::parse(profile).expect("profile parses"),
        ..SuiteOptions::default()
    };
    run(command, &options).expect("the suite completes")
}

fn check(
    outcome: &lekalo_core::adapter_conformance::SuiteOutcome,
    id: &str,
) -> (&'static str, Option<&'static str>) {
    let row = outcome
        .report
        .checks
        .iter()
        .find(|row| row.id == id)
        .expect("catalog row present");
    (row.state, row.detail)
}

const SUITE_TIMEOUT_MS: u64 = 30_000;

fn timed_options(profile: &str) -> SuiteOptions {
    SuiteOptions {
        profile: lekalo_core::adapter_conformance::Profile::parse(profile).expect("profile parses"),
        repeats: 2,
        timeout_ms: SUITE_TIMEOUT_MS,
    }
}

fn run_fault(fault: &str) -> lekalo_core::adapter_conformance::SuiteOutcome {
    let command = adapter_command(&["--lekalo-fault", fault]);
    run(&command, &timed_options("default")).expect("the suite completes")
}

#[test]
fn options_are_clamped_into_the_closed_bounds() {
    let clamped = SuiteOptions {
        repeats: 200,
        timeout_ms: 1,
        ..SuiteOptions::default()
    }
    .clamped();
    assert_eq!(
        clamped.repeats,
        lekalo_core::adapter_conformance::MAX_REPEATS
    );
    assert_eq!(
        clamped.timeout_ms,
        lekalo_core::adapter_conformance::MIN_TIMEOUT_MS
    );
}

#[test]
fn the_reference_adapter_passes_the_default_battery_with_an_exact_badge() {
    let outcome = run_suite(&adapter_command(&[]), "default");
    assert_eq!(outcome.status, lekalo_core::result::Status::Valid);
    assert_eq!(outcome.report.verdict, "pass");
    assert_eq!(
        outcome.report.badge,
        lekalo_core::adapter_conformance::Badge {
            issued: true,
            protocol: Some("1.0.0"),
            ir: Some("0.1.0"),
        }
    );
    // The legacy session skips exactly the rows that do not apply.
    let (ir_state, _) = check(&outcome, "capability.ir-declaration");
    assert_eq!(ir_state, "skipped");
    let (surface_state, _) = check(&outcome, "capability.surface");
    assert_eq!(surface_state, "skipped");
    // Every catalog row is present in fixed order.
    let ids: Vec<&str> = outcome.report.checks.iter().map(|row| row.id).collect();
    let expected: Vec<&str> = lekalo_core::adapter_conformance::CATALOG
        .iter()
        .map(|id| id.as_str())
        .collect();
    assert_eq!(ids, expected);
}

#[test]
fn the_fluent_adapter_passes_strict_with_a_1_2_0_badge() {
    let command = adapter_command(&["--lekalo-adapter-variant", "fluent"]);
    let outcome = run(&command, &timed_options("strict")).expect("the suite completes");
    assert_eq!(outcome.status, lekalo_core::result::Status::Valid);
    assert_eq!(outcome.report.session.protocol, "1.2.0");
    assert_eq!(outcome.report.badge.protocol, Some("1.2.0"));
    let (surface_state, _) = check(&outcome, "capability.surface");
    assert_eq!(surface_state, "pass");
    let (ir_state, _) = check(&outcome, "capability.ir-declaration");
    assert_eq!(ir_state, "pass");
}

#[test]
fn report_bytes_stay_deterministic_and_redacted() {
    let first = run_suite(&adapter_command(&[]), "default");
    let second = run_suite(&adapter_command(&[]), "default");
    assert_eq!(first, second);
    let envelope = first.envelope_json();
    let junit = first.junit();
    for document in [&envelope, &junit] {
        // No host state may leak into durable evidence. The fixture
        // root is per-run, so byte-identical runs already prove no root
        // leak; adapter artifact paths never enter the report either.
        assert!(!document.contains("generated/"), "no artifact leak");
        assert!(!document.contains("model.ts"), "no artifact leak");
        assert!(!document.contains('\\'), "no host path separators");
    }
    assert!(junit.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(junit.contains("<testcase name=\"describe.handshake\""));
    assert!(!junit.contains("<failure"), "a passing run has no failures");
}

#[test]
fn a_foreign_implementation_passes_with_its_own_identity() {
    let command = adapter_command(&["--lekalo-adapter-identity", "php-laravel"]);
    let outcome = run_suite(&command, "default");
    assert_eq!(outcome.report.verdict, "pass");
    assert_eq!(outcome.report.adapter.id, "php-laravel");
    assert!(outcome.report.badge.issued);
}

#[test]
fn a_reduced_surface_passes_default_and_fails_strict() {
    let command = adapter_command(&["--lekalo-adapter-ops", "describe,scan,bind"]);
    let default_run = run_suite(&command, "default");
    assert_eq!(default_run.report.verdict, "pass");
    let (surface_state, surface_detail) = check(&default_run, "capability.surface");
    assert_eq!(surface_state, "skipped");
    assert_eq!(surface_detail, Some("default-profile"));

    let strict_run = run(&command, &timed_options("strict")).expect("the suite completes");
    assert_eq!(strict_run.status, lekalo_core::result::Status::Unsupported);
    assert_eq!(strict_run.report.verdict, "protocol");
    let (surface_state, _) = check(&strict_run, "capability.surface");
    assert_eq!(surface_state, "fail");
    assert!(!strict_run.report.badge.issued);
}

#[test]
fn a_protocol_mismatch_is_an_uncompensated_unsupported_verdict() {
    let outcome = run_fault("wrong-token");
    assert_eq!(outcome.status, lekalo_core::result::Status::Unsupported);
    assert_eq!(outcome.report.verdict, "protocol");
    let (handshake, detail) = check(&outcome, "describe.handshake");
    assert_eq!(handshake, "fail");
    assert!(detail.is_some());
    assert!(!outcome.report.badge.issued);
}

#[test]
fn a_crashing_adapter_is_a_process_failure() {
    let outcome = run_fault("crash");
    assert_eq!(outcome.status, lekalo_core::result::Status::Unavailable);
    assert_eq!(outcome.report.verdict, "process");
}

#[test]
fn a_hanging_adapter_hits_the_deadline_as_a_process_failure() {
    let outcome = run_fault("hang");
    assert_eq!(outcome.status, lekalo_core::result::Status::Unavailable);
    assert_eq!(outcome.report.verdict, "process");
    let (_, detail) = check(&outcome, "describe.handshake");
    assert_eq!(detail, Some("deadline"));
}

#[test]
fn an_operation_error_envelope_is_a_feature_failure() {
    let outcome = run_fault("boom");
    assert_eq!(outcome.status, lekalo_core::result::Status::Invalid);
    assert_eq!(outcome.report.verdict, "feature");
    // The battery aborts at the failed handshake, so the remaining
    // rows are explicitly not-run rather than silently absent.
    let (structured_state, structured_detail) = check(&outcome, "diagnostics.structured");
    assert_eq!(structured_state, "skipped");
    assert_eq!(structured_detail, Some("not-run"));
    assert!(!outcome.report.badge.issued);
}

#[test]
fn nondeterministic_output_is_detected_by_repeated_runs() {
    let outcome = run_fault("nondeterministic");
    assert_ne!(outcome.report.verdict, "pass");
    let (state, detail) = check(&outcome, "determinism.repeats");
    assert_eq!(state, "fail");
    assert_eq!(detail, Some("generate-plan"));
    assert!(!outcome.report.badge.issued);
}

#[test]
fn a_hostile_mutation_attempt_never_passes() {
    // On Windows the denied dry-run write crashes the child (process
    // class); on confined Unix it is refused as a dry-run mutation
    // (security class). Either way the run must fail without a badge
    // and never as a plain pass.
    let outcome = run_fault("mutate-dry");
    assert_ne!(outcome.status, lekalo_core::result::Status::Valid);
    assert!(!outcome.report.badge.issued);
}

#[test]
fn an_undeclared_staged_write_never_passes() {
    let outcome = run_fault("extra-write");
    assert_ne!(outcome.status, lekalo_core::result::Status::Valid);
    assert!(!outcome.report.badge.issued);
}

#[test]
fn every_failed_check_projects_a_registered_diagnostic() {
    let outcome = run_fault("boom");
    assert_eq!(
        outcome.diagnostics.reason_ids(),
        vec!["adapter.check-failed".to_owned()]
    );
    assert_eq!(outcome.domain_result().exit_code(), 1);
}

#[test]
fn a_process_failure_projects_the_registered_process_failure_rule() {
    let outcome = run_fault("crash");
    assert_eq!(outcome.status, lekalo_core::result::Status::Unavailable);
    assert_eq!(
        outcome.diagnostics.reason_ids(),
        vec!["adapter.process-failure".to_owned()]
    );
    let diagnostic = outcome
        .diagnostics
        .as_slice()
        .first()
        .expect("the process failure carries its diagnostic");
    assert_eq!(diagnostic.code(), "LEK-ADP-003");
    assert_eq!(
        diagnostic.data().get("check"),
        Some(&lekalo_core::diagnostics::DataValue::Token(
            "describe.handshake".to_owned()
        ))
    );
}

#[test]
fn infrastructure_failure_projects_the_registered_rule_not_a_panic() {
    let error = lekalo_core::adapter_conformance::SuiteError {
        detail: "fixture-root",
    };
    let domain = lekalo_core::adapter_conformance::infrastructure_result(error);
    assert_eq!(domain.status(), lekalo_core::result::Status::Unavailable);
    assert_eq!(domain.exit_code(), 4);
    assert_eq!(
        domain
            .reason_codes()
            .iter()
            .map(|code| code.as_str())
            .collect::<Vec<_>>(),
        vec!["adapter.process-failure"]
    );
    let diagnostic = domain.diagnostics().first().expect("one diagnostic");
    assert_eq!(diagnostic.code(), "LEK-ADP-003");
    assert_eq!(
        diagnostic.data().get("detail"),
        Some(&lekalo_core::diagnostics::DataValue::Token(
            "fixture-root".to_owned()
        ))
    );
}
