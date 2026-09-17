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
    if !is_sha256(&result.plan_digest) {
        return Err(PlanRejection::Shape("plan-digest"));
    }
    // A never-executed command never fabricates a zero.
    for command in &result.commands {
        if !OUTCOMES.contains(&command.outcome.as_str()) {
            return Err(PlanRejection::Shape("command-outcome"));
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
    // Security defects are never masked: an original mutation or an
    // unexpected write forces the security outcome, never passed.
    let original_mutated = result.original_verification.state == "mutated"
        || !result.mutation_summary.unexpected.is_empty();
    if original_mutated && result.outcome != "security" {
        return Err(PlanRejection::Shape("security-masked"));
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
