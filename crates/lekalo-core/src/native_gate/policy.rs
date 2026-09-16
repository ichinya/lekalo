//! The checked execution policy (issue #48): confirmation semantics and
//! the release fallback rule. A confirmation names one exact
//! package+script+manifest-hash+tool recipe; a script merely named in a
//! manifest matches nothing. The policy is never execution authority by
//! itself — an approved exact plan digest is always required on top.

use serde::{Deserialize, Serialize};

use super::{PlanRejection, POLICY_SCHEMA_VERSION};

/// Maximum serialized policy bytes the host accepts (contract ceiling).
pub const MAX_POLICY_BYTES: usize = 1024 * 1024;

/// The closed confirmation entry of one execution policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfirmation {
    pub package_id: String,
    pub gate: String,
    pub script_name: String,
    pub manifest_digest: String,
    pub script_digest: String,
    pub argv: Vec<String>,
    pub tool_ref: PolicyToolRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tsconfig_ref: Option<String>,
    pub rule_version: String,
    pub rule_digest: String,
}

/// The tool reference of one confirmation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyToolRef {
    pub id: String,
    pub artifact_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// The checked execution policy document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicy {
    pub schema_version: String,
    pub kind: String,
    pub policy_digest: String,
    pub identity: PolicyIdentity,
    pub repository_role: String,
    pub trust: super::NativeTrust,
    pub authority_ref: super::NativeRef,
    pub policy_ref: super::NativeRef,
    pub classification_ref: super::NativeRef,
    pub allowed_gate_kinds: Vec<String>,
    pub confirmations: Vec<PolicyConfirmation>,
    pub fallback_rule: serde_json::Value,
    pub env_recipe: serde_json::Value,
    pub limits: super::NativeLimits,
    pub write_policy: super::NativeWritePolicy,
}

/// The policy identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyIdentity {
    pub id: String,
    pub version: String,
}

fn is_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Decode the checked-in execution policy document and validate it.
pub fn validate_policy(bytes: &[u8]) -> Result<ExecutionPolicy, PlanRejection> {
    if bytes.len() > MAX_POLICY_BYTES {
        return Err(PlanRejection::Shape("policy-oversize"));
    }
    let policy: ExecutionPolicy =
        serde_json::from_slice(bytes).map_err(|_| PlanRejection::Malformed)?;
    if policy.schema_version != POLICY_SCHEMA_VERSION || policy.kind != "native-gate-policy" {
        return Err(PlanRejection::Shape("schema-version"));
    }
    if !is_sha256(&policy.policy_digest) {
        return Err(PlanRejection::Shape("policy-digest"));
    }
    if policy.allowed_gate_kinds.is_empty() || policy.allowed_gate_kinds.len() > 4 {
        return Err(PlanRejection::Shape("gate-kinds"));
    }
    for kind in &policy.allowed_gate_kinds {
        if !["build", "typecheck", "lint", "test"].contains(&kind.as_str()) {
            return Err(PlanRejection::Shape("gate-kind"));
        }
    }
    if policy.confirmations.is_empty() || policy.confirmations.len() > 128 {
        return Err(PlanRejection::Shape("confirmations"));
    }
    for confirmation in &policy.confirmations {
        if !["build", "typecheck", "lint", "test"].contains(&confirmation.gate.as_str()) {
            return Err(PlanRejection::Shape("confirmation-gate"));
        }
        if !policy.allowed_gate_kinds.contains(&confirmation.gate) {
            return Err(PlanRejection::Shape("confirmation-gate-not-allowed"));
        }
        if !is_sha256(&confirmation.manifest_digest)
            || !is_sha256(&confirmation.script_digest)
            || !is_sha256(&confirmation.rule_digest)
        {
            return Err(PlanRejection::Shape("confirmation-digests"));
        }
        if confirmation.argv.is_empty() || confirmation.argv.len() > 64 {
            return Err(PlanRejection::Shape("confirmation-argv"));
        }
        if !is_sha256(&confirmation.tool_ref.artifact_digest) {
            return Err(PlanRejection::Shape("confirmation-tool-digest"));
        }
    }
    // The fallback rule is either absent (mode "none") or a complete
    // release-full rule with a recorded digest; anything else refuses.
    if policy.fallback_rule.is_object() {
        let mode = policy.fallback_rule.get("mode").and_then(|v| v.as_str());
        match mode {
            Some("none") => {}
            Some("release-full") => {
                if policy
                    .fallback_rule
                    .get("rule_digest")
                    .and_then(|v| v.as_str())
                    .is_some_and(|digest| !is_sha256(digest))
                {
                    return Err(PlanRejection::Shape("fallback-digest"));
                }
            }
            _ => return Err(PlanRejection::Shape("fallback-mode")),
        }
    } else {
        return Err(PlanRejection::Shape("fallback-rule"));
    }
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_gate::{decision_for, plan_only_reason, ExecutionDecision};

    fn golden_policy() -> ExecutionPolicy {
        let bytes = include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/policy.golden.json"
        );
        serde_json::from_slice(bytes).expect("golden policy decodes")
    }

    #[test]
    fn the_golden_policy_validates() {
        let policy = golden_policy();
        assert_eq!(policy.schema_version, POLICY_SCHEMA_VERSION);
        assert!(validate_policy(include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/policy.golden.json"
        ))
        .is_ok());
    }

    #[test]
    fn unknown_gate_kinds_are_refused_even_if_named_in_confirmations() {
        let mut policy = golden_policy();
        policy.allowed_gate_kinds = vec!["deploy".into()];
        let bytes = serde_json::to_vec(&policy).unwrap();
        assert_eq!(
            validate_policy(&bytes).unwrap_err(),
            PlanRejection::Shape("gate-kind")
        );
    }

    #[test]
    fn the_production_guard_never_authorizes_execution() {
        let plan_bytes = include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/plan.golden.json"
        );
        let plan = super::super::wire::decode_plan(plan_bytes).expect("golden plan");
        // A public-fixture plan is recognized as synthetic, and the
        // production answer is still the typed unsupported refusal.
        assert_eq!(
            decision_for(&plan),
            ExecutionDecision::FixtureRunnerNotShipped
        );
        assert_eq!(
            plan_only_reason(decision_for(&plan)),
            "fixture-runner-not-shipped"
        );

        // A private plan is plan-only with the confinement reason.
        let mut private = plan;
        private.trust.mode = "private".into();
        private.plan_digest = "sha256:".to_owned() + &"0".repeat(64);
        assert_eq!(
            decision_for(&private),
            ExecutionDecision::ConfinementRequired
        );
        assert_eq!(
            plan_only_reason(decision_for(&private)),
            "confinement-required"
        );
    }
}
