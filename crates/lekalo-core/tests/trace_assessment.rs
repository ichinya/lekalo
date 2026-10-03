//! Neutral consumer tests: no subprocess, HLV API, layout or filesystem IO.
use lekalo_core::trace::{assessment, TraceManifest};
use serde_json::{json, Value};

const TRACE: &[u8] = include_bytes!("../../../tests/fixtures/trace-assessment/trace.json");
const READY: &[u8] = include_bytes!("../../../tests/fixtures/trace-assessment/input/ready.json");

fn assess(input: &[u8]) -> assessment::AssessmentReport {
    assessment::assess(&TraceManifest::parse(TRACE).unwrap(), input).unwrap()
}

#[test]
fn strict_chain_requires_binding_branch_and_qualified_execution() {
    let report = assess(READY);
    assert_eq!(report.verdict, assessment::Verdict::Ready);
    assert_eq!(report.coverage, assessment::Coverage::Complete);
    assert_eq!(report.chains[0].execution, assessment::Execution::Passed);
    let missing = assess(include_bytes!(
        "../../../tests/fixtures/trace-assessment/input/missing-mapping.json"
    ));
    assert_eq!(missing.coverage, assessment::Coverage::Partial);
    assert!(missing
        .findings
        .iter()
        .any(|f| f.rule == "trace.bridge-mapping-missing"));
}

#[test]
fn failure_and_unavailability_preserve_independent_receipts() {
    let failed = assess(include_bytes!(
        "../../../tests/fixtures/trace-assessment/input/hlv-fail.json"
    ));
    let unavailable = assess(include_bytes!(
        "../../../tests/fixtures/trace-assessment/input/hlv-unavailable.json"
    ));
    assert_eq!(failed.verdict, assessment::Verdict::Blocked);
    assert_eq!(unavailable.verdict, assessment::Verdict::Blocked);
    assert_eq!(failed.chains[0].execution, assessment::Execution::Passed);
    assert_eq!(failed.evidence.len(), 3);
    let hlv = failed
        .evidence
        .iter()
        .find(|e| e.provider.as_str() == "hlv")
        .unwrap();
    assert_eq!(hlv.outcome, assessment::Outcome::Fail);
    assert_eq!(hlv.diagnostics[0].code, "CTR-030");
    assert_eq!(hlv.diagnostics[1].code, "GATE-005");
    let envelope: Value = serde_json::from_str(&failed.domain_result().to_json_string()).unwrap();
    assert_eq!(envelope["status"], "denied");
    assert_eq!(
        envelope["payload"]["assessment"]["evidence"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(unavailable
        .evidence
        .iter()
        .any(|e| e.outcome == assessment::Outcome::Unavailable));
}

#[test]
fn stale_model_cannot_reuse_a_passing_native_receipt() {
    let report = assess(include_bytes!(
        "../../../tests/fixtures/trace-assessment/input/stale-model.json"
    ));
    assert_eq!(
        report.chains[0].execution,
        assessment::Execution::Unverified
    );
    assert_eq!(report.verdict, assessment::Verdict::Blocked);
    assert!(report
        .findings
        .iter()
        .any(|f| f.rule == "trace.bridge-evidence-stale"));
}

#[test]
fn incompatible_protocol_is_preserved_and_refused() {
    let report = assess(include_bytes!(
        "../../../tests/fixtures/trace-assessment/input/unsupported-protocol.json"
    ));
    assert_eq!(report.verdict, assessment::Verdict::Blocked);
    let hlv = report
        .evidence
        .iter()
        .find(|e| e.provider.as_str() == "hlv")
        .unwrap();
    assert_eq!(hlv.protocol.version, "1.0.1");
    assert_eq!(hlv.outcome, assessment::Outcome::Pass);
    assert!(report
        .findings
        .iter()
        .any(|f| f.rule == "trace.bridge-provider-unsupported"));
}

#[test]
fn decoded_duplicate_keys_and_null_optionals_fail_closed() {
    let trace = TraceManifest::parse(TRACE).unwrap();
    for input in [
        include_bytes!("../../../tests/fixtures/trace-assessment/input/escaped-duplicate.json")
            .as_slice(),
        include_bytes!("../../../tests/fixtures/trace-assessment/input/null-test.json").as_slice(),
    ] {
        let diagnostics = assessment::assess(&trace, input).unwrap_err();
        assert_eq!(diagnostics.as_slice()[0].id(), "trace.bridge-input-invalid");
    }
}

#[test]
fn unreferenced_failure_and_duplicate_mappings_cannot_hide_behind_ready() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/trace-assessment/input/unreferenced-failure.json")
            .as_slice(),
        include_bytes!("../../../tests/fixtures/trace-assessment/input/duplicate-mapping.json")
            .as_slice(),
    ] {
        let report = assess(bytes);
        assert_eq!(report.coverage, assessment::Coverage::Conflicting);
        assert_eq!(report.verdict, assessment::Verdict::Blocked);
    }
}

#[test]
fn bounded_errors_never_echo_input_text() {
    let mut input: Value = serde_json::from_slice(READY).unwrap();
    input["secret"] = json!("PRIVATE_BODY /host/path $(command)");
    let trace = TraceManifest::parse(TRACE).unwrap();
    let diagnostics = assessment::assess(&trace, &serde_json::to_vec(&input).unwrap()).unwrap_err();
    let wire = serde_json::to_string(diagnostics.as_slice()).unwrap();
    assert!(!wire.contains("PRIVATE_BODY"));
    assert!(!wire.contains("/host/path"));
}
