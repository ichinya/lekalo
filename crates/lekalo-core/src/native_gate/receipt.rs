//! Run request and terminal receipt validation (issue #48): the
//! approval always lives outside the hashed plan, a run names exactly
//! one approved digest, and the receipt's terminal outcome is fixed
//! exactly once with valueState measurements.

use super::{NativeRunRequest, NativeRunResult, PlanRejection, RUN_SCHEMA_VERSION};

fn is_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// The closed terminal outcome set.
pub const OUTCOMES: [&str; 7] = [
    "passed",
    "failed",
    "missing",
    "blocked",
    "unsupported",
    "infrastructure",
    "security",
];

/// The closed proposed failure classes (issue #61): orthogonal to the
/// outcome, one classification per failed command.
pub const FAILURE_CLASSES: [&str; 6] = [
    "assertion",
    "static-analysis",
    "boot",
    "missing-tool",
    "incompatible",
    "infrastructure",
];

/// The closed proposed verification verdicts (issue #61).
pub const VERDICTS: [&str; 4] = ["passed", "failed", "blocked", "degraded"];

/// The closed coverage states of a run receipt (issue #61).
pub const COVERAGE_STATES: [&str; 3] = ["complete", "incomplete", "unknown"];

/// The outcome each failure class coherent with: an assertion, static
/// analysis, or boot failure is a genuine command failure; a missing
/// tool is the missing outcome; an incompatible tool is unsupported;
/// infrastructure failures carry the infrastructure outcome.
fn failure_class_coheres(failure_class: &str, outcome: &str) -> bool {
    match failure_class {
        "assertion" | "static-analysis" | "boot" => outcome == "failed",
        "missing-tool" => outcome == "missing",
        "incompatible" => outcome == "unsupported",
        "infrastructure" => outcome == "infrastructure",
        _ => false,
    }
}

/// Decode and validate one run request: the approval binding is
/// mandatory, and the plan can never approve itself.
pub fn validate_run_request(bytes: &[u8]) -> Result<NativeRunRequest, PlanRejection> {
    let request: NativeRunRequest =
        serde_json::from_slice(bytes).map_err(|_| PlanRejection::Malformed)?;
    if request.schema_version != RUN_SCHEMA_VERSION || request.kind != "native-run-request" {
        return Err(PlanRejection::Shape("schema-version"));
    }
    if !is_sha256(&request.approved_plan_digest) || !is_sha256(&request.fixture_catalog_ref) {
        return Err(PlanRejection::Shape("run-digests"));
    }
    if request.approval.mode != "explicit" && request.approval.mode != "checked-in-policy" {
        return Err(PlanRejection::Shape("approval-mode"));
    }
    if !is_sha256(&request.approval.policy_digest) {
        return Err(PlanRejection::Shape("approval-policy-digest"));
    }
    if request.plan_ref.is_empty() || request.plan_ref.len() > 512 {
        return Err(PlanRejection::Shape("plan-ref"));
    }
    Ok(request)
}

/// The proposed verdict rollup (issue #61 §7): security first, then
/// infrastructure, then required blockers, then genuine failures,
/// then optional degradation, then pass. All per-gate causes are
/// retained on the receipt even when a stronger terminal class wins.
pub fn rollup_verdict(result: &NativeRunResult) -> String {
    // Receipt-level terminal classes dominate the rollup: a security
    // or infrastructure terminal answer, an original mutation, an
    // unexpected write, or a refusal that executed nothing at all is
    // never a passing verification answer.
    if result.outcome == "security"
        || result.outcome == "infrastructure"
        || result.original_verification.state == "mutated"
        || !result.mutation_summary.unexpected.is_empty()
        || (result.commands.is_empty() && result.outcome != "passed")
    {
        return "blocked".to_owned();
    }
    let mut verdict = "passed";
    let mut seen_infrastructure = false;
    let mut seen_failure = false;
    let mut seen_degradation = false;
    for command in &result.commands {
        match command.outcome.as_str() {
            "security" | "blocked" => return "blocked".to_owned(),
            "infrastructure" => seen_infrastructure = true,
            "failed" => {
                if command.required {
                    return "blocked".to_owned();
                }
                seen_failure = true;
            }
            "missing" | "unsupported" => {
                if command.required {
                    return "blocked".to_owned();
                }
                seen_degradation = true;
            }
            _ => {}
        }
    }
    if seen_infrastructure {
        verdict = "blocked";
    } else if seen_failure {
        verdict = "failed";
    } else if seen_degradation {
        verdict = "degraded";
    }
    verdict.to_owned()
}

/// Decode and validate one terminal run receipt: outcome closure,
/// exactly-once terminal semantics, mutation/original/cleanup coherence
/// (security and cleanup defects are never masked by a gate exit).
pub fn validate_run_result(bytes: &[u8]) -> Result<NativeRunResult, PlanRejection> {
    let result: NativeRunResult =
        serde_json::from_slice(bytes).map_err(|_| PlanRejection::Malformed)?;
    if result.schema_version != RUN_SCHEMA_VERSION || result.kind != "native-run-result" {
        return Err(PlanRejection::Shape("schema-version"));
    }
    if !OUTCOMES.contains(&result.outcome.as_str()) {
        return Err(PlanRejection::Shape("outcome"));
    }
    if !VERDICTS.contains(&result.verdict.as_str()) {
        return Err(PlanRejection::Shape("verdict"));
    }
    if !is_sha256(&result.plan_digest) {
        return Err(PlanRejection::Shape("plan-digest"));
    }
    if !COVERAGE_STATES.contains(&result.coverage.state.as_str()) {
        return Err(PlanRejection::Shape("coverage-state"));
    }
    for gate_id in &result.coverage.uncovered_gate_ids {
        if gate_id.is_empty() || gate_id.len() > 128 {
            return Err(PlanRejection::Shape("coverage-gate-id"));
        }
    }
    // A never-executed command never fabricates a zero.
    for command in &result.commands {
        if !OUTCOMES.contains(&command.outcome.as_str()) {
            return Err(PlanRejection::Shape("command-outcome"));
        }
        if let Some(failure_class) = &command.failure_class {
            if !FAILURE_CLASSES.contains(&failure_class.as_str())
                || !failure_class_coheres(failure_class, &command.outcome)
            {
                return Err(PlanRejection::Shape("failure-class"));
            }
        }
        if let Some(exit) = &command.exit {
            let known = exit.state == "known";
            if known != exit.value.is_some() {
                return Err(PlanRejection::Shape("value-state"));
            }
            if command.outcome == "blocked" && known && exit.value == Some(0) {
                return Err(PlanRejection::Shape("fabricated-zero"));
            }
        }
    }
    // Verdict coherence (issue #61): the summary is the rollup of the
    // per-command outcomes under the precedence security >
    // infrastructure > required-blocker > failure > optional-
    // degradation > pass. A passed verdict is the all-passed answer;
    // a degraded verdict never hides a genuine failure or a required
    // gap; coverage incompleteness is never a passed run.
    // Security defects are never masked (checked first: the stronger
    // terminal class wins over any rollup): an original mutation or an
    // unexpected write forces the security outcome, never passed.
    let original_mutated = result.original_verification.state == "mutated"
        || !result.mutation_summary.unexpected.is_empty();
    if original_mutated && result.outcome != "security" {
        return Err(PlanRejection::Shape("security-masked"));
    }
    let rollup = rollup_verdict(&result);
    if rollup != result.verdict {
        return Err(PlanRejection::Shape("verdict-rollup"));
    }
    if (result.verdict == "passed" && result.outcome != "passed")
        || (result.outcome == "passed" && result.verdict != "passed")
    {
        return Err(PlanRejection::Shape("verdict-outcome"));
    }
    if result.verdict == "passed"
        && (result.coverage.state != "complete" || !result.coverage.uncovered_gate_ids.is_empty())
    {
        return Err(PlanRejection::Shape("coverage-masked"));
    }
    // The terminal result is fixed exactly once: an outcome of passed
    // with cleanup failure is a contradiction (cleanup failure means
    // infrastructure).
    if result.cleanup.state == "failed" && result.outcome == "passed" {
        return Err(PlanRejection::Shape("cleanup-masked"));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden_run() -> NativeRunResult {
        let bytes = include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/run-result.golden.json"
        );
        serde_json::from_slice(bytes).expect("golden run decodes")
    }

    #[test]
    fn the_golden_run_receipt_validates() {
        assert!(validate_run_result(include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/run-result.golden.json"
        ))
        .is_ok());
    }

    #[test]
    fn an_original_mutation_never_leaves_the_outcome_passed() {
        let mut run = golden_run();
        run.original_verification.state = "mutated".into();
        run.original_verification
            .mutated_paths
            .push("packages/planner/src/plan.ts".into());
        let bytes = serde_json::to_vec(&run).unwrap();
        assert_eq!(
            validate_run_result(&bytes).unwrap_err(),
            PlanRejection::Shape("security-masked")
        );
    }

    #[test]
    fn a_blocked_command_never_carries_a_fabricated_zero_exit() {
        let mut run = golden_run();
        if let Some(command) = run.commands.last_mut() {
            command.outcome = "blocked".into();
            command.exit = Some(super::super::NativeValueState {
                state: "known".into(),
                value: Some(0),
            });
        }
        let bytes = serde_json::to_vec(&run).unwrap();
        assert_eq!(
            validate_run_result(&bytes).unwrap_err(),
            PlanRejection::Shape("fabricated-zero")
        );
    }

    #[test]
    fn an_unknown_outcome_is_refused() {
        let mut run = golden_run();
        run.outcome = "mostly-passed".into();
        let bytes = serde_json::to_vec(&run).unwrap();
        assert_eq!(
            validate_run_result(&bytes).unwrap_err(),
            PlanRejection::Shape("outcome")
        );
    }

    #[test]
    fn a_failure_class_must_cohere_with_its_outcome() {
        let mut run = golden_run();
        let last = run.commands.last_mut().unwrap();
        last.required = false;
        last.outcome = "failed".into();
        last.failure_class = Some("missing-tool".into());
        run.verdict = "failed".into();
        run.outcome = "failed".into();
        let bytes = serde_json::to_vec(&run).unwrap();
        assert_eq!(
            validate_run_result(&bytes).unwrap_err(),
            PlanRejection::Shape("failure-class")
        );
        // The coherent mapping is accepted: a missing optional tool is
        // the missing outcome with the missing-tool class and a
        // degraded summary.
        let mut run = golden_run();
        let last = run.commands.last_mut().unwrap();
        last.outcome = "missing".into();
        last.required = false;
        last.failure_class = Some("missing-tool".into());
        run.verdict = "degraded".into();
        run.outcome = "blocked".into();
        run.coverage.uncovered_gate_ids = vec![last.gate_id.clone()];
        let bytes = serde_json::to_vec(&run).unwrap();
        assert!(validate_run_result(&bytes).is_ok());
    }

    #[test]
    fn the_verdict_is_the_documented_rollup_of_the_commands() {
        // Degraded: an optional tool is missing, nothing failed.
        let mut run = golden_run();
        let last = run.commands.last_mut().unwrap();
        last.outcome = "missing".into();
        last.required = false;
        last.failure_class = Some("missing-tool".into());
        run.outcome = "blocked".into();
        run.verdict = "degraded".into();
        run.coverage.uncovered_gate_ids = vec![last.gate_id.clone()];
        let bytes = serde_json::to_vec(&run).unwrap();
        assert!(validate_run_result(&bytes).is_ok());

        // The same missing tool on a required gate is a blocker.
        let mut run = golden_run();
        let last = run.commands.last_mut().unwrap();
        last.outcome = "missing".into();
        last.required = true;
        last.failure_class = Some("missing-tool".into());
        run.outcome = "blocked".into();
        run.verdict = "degraded".into();
        run.coverage.uncovered_gate_ids = vec![last.gate_id.clone()];
        let bytes = serde_json::to_vec(&run).unwrap();
        assert_eq!(
            validate_run_result(&bytes).unwrap_err(),
            PlanRejection::Shape("verdict-rollup")
        );
    }

    #[test]
    fn a_passed_verdict_never_carries_incomplete_coverage() {
        let mut run = golden_run();
        let last = run.commands.last_mut().unwrap();
        last.outcome = "passed".into();
        run.outcome = "passed".into();
        run.verdict = "passed".into();
        run.coverage.state = "unknown".into();
        run.coverage.uncovered_gate_ids.clear();
        let bytes = serde_json::to_vec(&run).unwrap();
        assert_eq!(
            validate_run_result(&bytes).unwrap_err(),
            PlanRejection::Shape("coverage-masked")
        );
    }

    #[test]
    fn a_run_request_without_an_approval_is_refused() {
        let request = serde_json::json!({
            "schema_version": RUN_SCHEMA_VERSION,
            "kind": "native-run-request",
            "plan_ref": "runs/plan.json",
            "approved_plan_digest": format!("sha256:{}", "a".repeat(64)),
            "fixture_catalog_ref": format!("sha256:{}", "b".repeat(64)),
        });
        let bytes = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            validate_run_request(&bytes).unwrap_err(),
            PlanRejection::Malformed
        );
    }
}
