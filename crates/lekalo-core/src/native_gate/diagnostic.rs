//! The `native-gate.*` diagnostic mapping (issue #48): every native
//! gate refusal onto its registered stable rule with typed bounded
//! data. The status — and therefore the exit class — lives here.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject};
use crate::result::Status;

/// Why the native gate host refused or could not complete the work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeGateFailure {
    /// The plan/policy document is undecodable or violates its shape.
    PlanInvalid { detail: &'static str },
    /// The recorded digest does not equal the recomputed one.
    PlanStale,
    /// No approval names this exact plan digest.
    ApprovalMissing,
    /// Trust/fixture custody does not satisfy the execution policy.
    TrustInsufficient,
    /// The manager/workspace layout has no implemented gate path.
    ManagerUnsupported,
    /// A required enforcement capability is unavailable.
    CapabilityMissing { detail: &'static str },
    /// A gate command finished with a failing outcome.
    RunFailed { outcome: &'static str },
    /// Infrastructure failed: spawn, deadline, flood, cleanup.
    RunInfrastructure,
    /// A confinement contract violation was observed.
    SecurityViolation,
    /// An environment name outside the approved recipe was refused.
    EnvDenied { name: String },
}

/// The registered rule id and status of one native gate failure.
pub fn native_gate_rule(failure: &NativeGateFailure) -> (&'static str, Status) {
    match failure {
        NativeGateFailure::PlanInvalid { .. } => ("native-gate.plan-invalid", Status::Invalid),
        NativeGateFailure::PlanStale => ("native-gate.plan-stale", Status::Invalid),
        NativeGateFailure::ApprovalMissing => ("native-gate.approval-missing", Status::Denied),
        NativeGateFailure::TrustInsufficient => ("native-gate.trust-insufficient", Status::Denied),
        NativeGateFailure::ManagerUnsupported => {
            ("native-gate.manager-unsupported", Status::Unsupported)
        }
        NativeGateFailure::CapabilityMissing { .. } => {
            ("native-gate.capability-missing", Status::Unsupported)
        }
        NativeGateFailure::RunFailed { .. } => ("native-gate.run-failed", Status::Invalid),
        NativeGateFailure::RunInfrastructure => {
            ("native-gate.run-infrastructure", Status::Unavailable)
        }
        NativeGateFailure::SecurityViolation => ("native-gate.security-violation", Status::Denied),
        NativeGateFailure::EnvDenied { .. } => ("native-gate.env-denied", Status::Denied),
    }
}

impl From<&NativeGateFailure> for crate::DomainResult {
    fn from(failure: &NativeGateFailure) -> Self {
        let (id, status) = native_gate_rule(failure);
        let mut data = DataObject::new();
        match failure {
            NativeGateFailure::PlanInvalid { detail }
            | NativeGateFailure::CapabilityMissing { detail } => {
                data.insert("detail".to_owned(), token_value(detail));
            }
            NativeGateFailure::RunFailed { outcome } => {
                data.insert("detail".to_owned(), token_value("gate-command"));
                data.insert("outcome".to_owned(), token_value(outcome));
            }
            NativeGateFailure::EnvDenied { name } => {
                data.insert("name".to_owned(), token_value(name));
            }
            _ => {
                data.insert("detail".to_owned(), token_value("native-gate"));
            }
        }
        let item =
            build(id, None, None, data).expect("native-gate rules are registered and active");
        let diagnostics = crate::diagnostics::DiagnosticSet::try_from_unsorted(vec![item], status)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"));
        match status {
            Status::Invalid => crate::DomainResult::Invalid { diagnostics },
            Status::Denied => crate::DomainResult::Denied { diagnostics },
            Status::Unsupported => crate::DomainResult::UnsupportedOperation { diagnostics },
            Status::Unavailable => crate::DomainResult::Unavailable { diagnostics },
            _ => crate::DomainResult::Invalid { diagnostics },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_native_gate_failure_maps_onto_a_registered_rule() {
        let failures = [
            NativeGateFailure::PlanInvalid { detail: "shape" },
            NativeGateFailure::PlanStale,
            NativeGateFailure::ApprovalMissing,
            NativeGateFailure::TrustInsufficient,
            NativeGateFailure::ManagerUnsupported,
            NativeGateFailure::CapabilityMissing { detail: "network" },
            NativeGateFailure::RunFailed { outcome: "failed" },
            NativeGateFailure::RunInfrastructure,
            NativeGateFailure::SecurityViolation,
            NativeGateFailure::EnvDenied {
                name: "PATH".into(),
            },
        ];
        for failure in &failures {
            let (id, _) = native_gate_rule(failure);
            let result = crate::DomainResult::from(failure);
            assert!(
                result.to_json_string().contains(id),
                "{id} must appear in the projected diagnostics"
            );
        }
    }

    #[test]
    fn distinct_diagnostic_families_distinguish_the_ac11_outcomes() {
        // AC11: missing/failed/blocked/unsupported differ.
        let missing = NativeGateFailure::RunFailed { outcome: "missing" };
        let failed = NativeGateFailure::RunFailed { outcome: "failed" };
        let blocked = NativeGateFailure::ApprovalMissing;
        let unsupported = NativeGateFailure::ManagerUnsupported;
        assert_eq!(native_gate_rule(&missing).0, "native-gate.run-failed");
        assert_eq!(native_gate_rule(&failed).0, "native-gate.run-failed");
        assert_eq!(native_gate_rule(&blocked).0, "native-gate.approval-missing");
        assert_eq!(
            native_gate_rule(&unsupported).0,
            "native-gate.manager-unsupported"
        );
        assert_eq!(native_gate_rule(&blocked).1, Status::Denied);
        assert_eq!(native_gate_rule(&unsupported).1, Status::Unsupported);
        assert_eq!(native_gate_rule(&failed).1, Status::Invalid);
    }
}
