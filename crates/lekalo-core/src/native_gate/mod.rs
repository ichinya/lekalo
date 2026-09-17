//! The native gate host API (issue #48).
//!
//! This module is the pure, independently validating core side of the
//! native gate lifecycle: strict plan validation, checked execution
//! policies, digest recomputation over the pinned domain, plan-only
//! execution refusals, normalized run receipts, and the composite
//! observed view projection. There is deliberately no process launch
//! here: the only execution path is the test-only fixture runner under
//! `#[cfg(test)]` (see `fixture_tests.rs`), which reuses the audited
//! confinement backends. The production `native run` surface ends
//! before any fixture execution with a typed refusal.

mod diagnostic;
#[cfg(test)]
mod fixture_tests;
mod policy;
mod receipt;
mod types;
mod view;
mod wire;

pub use diagnostic::{native_gate_rule, NativeGateFailure};
pub use policy::validate_policy;
pub use receipt::{validate_run_request, validate_run_result};
pub use types::*;
pub use view::{build_view, validate_view};

use crate::digest::sha256_hex;

/// The digest domain of the native plan contract: `plan_digest` is
/// sha256 over `DOMAIN || canonical(plan without plan_digest)` where
/// the canonical form is compact UTF-8 JSON with recursively bytewise
/// key-sorted members (serde_json BTreeMap ordering). Shared Node/Rust
/// golden vectors pin the exact bytes.
pub const PLAN_DIGEST_DOMAIN: &str = "lekalo.native-plan.v0.3.2";

/// The schema discriminator of the native gate plan family.
pub const PLAN_SCHEMA_VERSION: &str = "lekalo/native-gate-plan/v0.3.2";
/// The schema discriminator of the native gate policy family.
pub const POLICY_SCHEMA_VERSION: &str = "lekalo/native-gate-policy/v0.3.2";
/// The schema discriminator of the native gate run family.
pub const RUN_SCHEMA_VERSION: &str = "lekalo/native-gate-run/v0.3.2";
/// The schema discriminator of the native gate view family.
pub const VIEW_SCHEMA_VERSION: &str = "lekalo/native-gate-view/v0.3.2";

/// The independent plan digest recomputation.
pub fn plan_digest(plan: &NativePlan) -> String {
    let mut value = serde_json::to_value(plan).expect("plan serializes");
    value
        .as_object_mut()
        .expect("plan is an object")
        .remove("plan_digest");
    // serde_json::Map sorts keys bytewise (BTreeMap), giving the pinned
    // canonical form: compact, recursively key-sorted JSON.
    let bytes = serde_json::to_vec(&value).expect("canonical value serializes");
    let mut joined = PLAN_DIGEST_DOMAIN.as_bytes().to_vec();
    joined.extend_from_slice(&bytes);
    format!("sha256:{}", sha256_hex(&joined))
}

/// The production run receipt: the terminal answer of `lekalo native
/// run` without any command launch. It is the closed run-result
/// document with outcome "blocked" (private/untrusted, plan-only
/// until #89) or "unsupported" (trusted synthetic fixture plan: the
/// runner ships only in the test harness).
pub fn production_run(plan_bytes: &[u8]) -> Result<NativeRunResult, NativeGateFailure> {
    let plan =
        wire::decode_plan(plan_bytes).map_err(|rejection| NativeGateFailure::PlanInvalid {
            detail: rejection.detail(),
        })?;
    let decision = decision_for(&plan);
    let outcome = match decision {
        ExecutionDecision::ConfinementRequired => "blocked",
        ExecutionDecision::FixtureRunnerNotShipped => "unsupported",
    };
    let reason = plan_only_reason(decision);
    Ok(NativeRunResult {
        schema_version: RUN_SCHEMA_VERSION.to_owned(),
        kind: "native-run-result".to_owned(),
        plan_digest: plan.plan_digest.clone(),
        execution_policy_ref: plan.execution_policy_ref.clone(),
        authority_ref: plan.authority_ref.clone(),
        policy_ref: plan.policy_ref.clone(),
        outcome: outcome.to_owned(),
        reason_codes: vec![reason.to_owned()],
        commands: Vec::new(),
        mutation_summary: Default::default(),
        original_verification: NativeOriginalVerification {
            state: "unverifiable".to_owned(),
            mutated_paths: Vec::new(),
        },
        cleanup: NativeCleanup {
            state: "unknown".to_owned(),
            detail: None,
        },
        capability_evidence: NativeCapabilityEvidence {
            network_denial: "unknown".to_owned(),
            process_containment: "unknown".to_owned(),
            backend: None,
            receipt_digest: None,
        },
        provenance: None,
    })
}

/// The closed execution decision of the production `native run` seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionDecision {
    /// Private/untrusted repositories are plan-only until #89.
    ConfinementRequired,
    /// Trusted synthetic plans: the fixture runner is not shipped in
    /// the production build.
    FixtureRunnerNotShipped,
}

/// The trust-based decision for one validated plan.
pub fn decision_for(plan: &NativePlan) -> ExecutionDecision {
    match plan.trust.mode.as_str() {
        "public-fixture" => ExecutionDecision::FixtureRunnerNotShipped,
        _ => ExecutionDecision::ConfinementRequired,
    }
}

/// The stable reason code of one execution decision.
pub fn plan_only_reason(decision: ExecutionDecision) -> &'static str {
    match decision {
        ExecutionDecision::ConfinementRequired => "confinement-required",
        ExecutionDecision::FixtureRunnerNotShipped => "fixture-runner-not-shipped",
    }
}

/// Why one native gate plan was refused before anything could use it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanRejection {
    /// The document is not decodable JSON of the closed shape.
    Malformed,
    /// A member violates its bound, enum, or grammar.
    Shape(&'static str),
    /// The recorded plan digest does not equal the recomputed one.
    DigestMismatch,
    /// The workspace inventory is ambiguous (duplicate packages, cycles
    /// without policy, unresolved mandatory edges).
    Workspace(&'static str),
    /// The trust/authority refs do not name the accepted generations.
    TrustRef,
}

impl PlanRejection {
    /// The bounded wire token carried in diagnostic data.
    pub fn detail(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::Shape(detail) => detail,
            Self::DigestMismatch => "digest-mismatch",
            Self::Workspace(detail) => detail,
            Self::TrustRef => "trust-ref",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_versions_are_pinned() {
        assert_eq!(PLAN_SCHEMA_VERSION, "lekalo/native-gate-plan/v0.3.2");
        assert_eq!(POLICY_SCHEMA_VERSION, "lekalo/native-gate-policy/v0.3.2");
        assert_eq!(RUN_SCHEMA_VERSION, "lekalo/native-gate-run/v0.3.2");
        assert_eq!(VIEW_SCHEMA_VERSION, "lekalo/native-gate-view/v0.3.2");
        assert_eq!(PLAN_DIGEST_DOMAIN, "lekalo.native-plan.v0.3.2");
    }
}
