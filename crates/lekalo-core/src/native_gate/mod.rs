//! The native gate host API (issue #48, extended for #61).
//!
//! This module is the pure, independently validating core side of the
//! native gate lifecycle: strict plan validation, checked execution
//! policies, digest recomputation over the pinned domain, plan-only
//! execution refusals, normalized run receipts, the composite observed
//! view projection, and — since #61 — the qualified host runner
//! (`runner.rs`) with its evidence layer (`evidence.rs`). The runner is
//! reachable only through callers that present qualified runtime
//! capability evidence; every shipped production path still ends
//! before any launch with a typed refusal (`production_run`), and the
//! only execution paths exercised in this repository are the test
//! batteries (`fixture_tests.rs`, `runner_tests`).

mod diagnostic;
mod evidence;
#[cfg(test)]
mod fixture_tests;
mod policy;
mod receipt;
mod runner;
mod types;
mod view;
mod wire;

pub use diagnostic::{native_gate_rule, NativeGateFailure};
pub use evidence::{FailureClass, OutputCaps, RedactedOutput};
pub use policy::validate_policy;
pub use receipt::{validate_run_request, validate_run_result};
pub use runner::{
    run_confirmed_plan, CatalogEntry, ConfirmedRun, RunnerCatalog, RuntimeCapability,
};
pub use types::*;
pub use view::{build_view, validate_view};

use crate::digest::sha256_hex;

/// The digest domain of the native plan contract: `plan_digest` is
/// sha256 over `DOMAIN || canonical(plan without plan_digest)` where
/// the canonical form is compact UTF-8 JSON with recursively bytewise
/// key-sorted members (serde_json BTreeMap ordering). Shared Node/Rust
/// golden vectors pin the exact bytes.
pub const PLAN_DIGEST_DOMAIN: &str = "lekalo.native-plan.v0.4.0";

/// The schema discriminator of the native gate plan family.
pub const PLAN_SCHEMA_VERSION: &str = "lekalo/native-gate-plan/v0.4.0";
/// The schema discriminator of the native gate policy family.
pub const POLICY_SCHEMA_VERSION: &str = "lekalo/native-gate-policy/v0.4.0";
/// The schema discriminator of the native gate run family.
pub const RUN_SCHEMA_VERSION: &str = "lekalo/native-gate-run/v0.4.0";
/// The schema discriminator of the native gate view family.
pub const VIEW_SCHEMA_VERSION: &str = "lekalo/native-gate-view/v0.4.0";

/// The digest domain of the digest-addressed selection document: a
/// plan command's `selection_ref` is sha256 over this domain joined
/// with the canonical selection member, so an approved plan pins its
/// selection artifacts and they cannot be replaced after approval.
pub const SELECTION_DIGEST_DOMAIN: &str = "lekalo.native-selection.v0.4.0";

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

/// The digest-addressed selection document reference: sha256 over
/// `SELECTION_DIGEST_DOMAIN || canonical(selection)`. Every command of
/// the plan carries this digest so the approved plan pins its own
/// selection artifacts; a post-approval edit of the selection member
/// cannot validate without changing the plan digest too.
pub fn selection_digest(selection: &NativeSelection) -> String {
    // Through a serde_json Value so object keys are bytewise sorted,
    // matching the Node canonical form byte for byte.
    let value = serde_json::to_value(selection).expect("selection serializes");
    let bytes = serde_json::to_vec(&value).expect("canonical value serializes");
    let mut joined = SELECTION_DIGEST_DOMAIN.as_bytes().to_vec();
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
        verdict: "blocked".to_owned(),
        reason_codes: vec![reason.to_owned()],
        commands: Vec::new(),
        coverage: NativeCoverage {
            state: "unknown".to_owned(),
            uncovered_gate_ids: plan.selection.mandatory_gate_ids.to_vec(),
        },
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
        assert_eq!(PLAN_SCHEMA_VERSION, "lekalo/native-gate-plan/v0.4.0");
        assert_eq!(POLICY_SCHEMA_VERSION, "lekalo/native-gate-policy/v0.4.0");
        assert_eq!(RUN_SCHEMA_VERSION, "lekalo/native-gate-run/v0.4.0");
        assert_eq!(VIEW_SCHEMA_VERSION, "lekalo/native-gate-view/v0.4.0");
        assert_eq!(PLAN_DIGEST_DOMAIN, "lekalo.native-plan.v0.4.0");
        assert_eq!(SELECTION_DIGEST_DOMAIN, "lekalo.native-selection.v0.4.0");
    }

    #[test]
    fn the_selection_digest_is_the_pinned_domain_over_the_selection_member() {
        let plan_bytes = include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/plan.golden.json"
        );
        let plan: NativePlan = serde_json::from_slice(plan_bytes).expect("golden plan decodes");
        assert_eq!(
            selection_digest(&plan.selection),
            plan.commands[0].selection_ref
        );
        // The selection document cannot drift after approval: a changed
        // member changes every command's selection_ref and therefore
        // the approved plan digest.
        let mut drifted = plan.selection.clone();
        drifted.modules.push("other".into());
        assert_ne!(selection_digest(&drifted), plan.commands[0].selection_ref);
    }
}
