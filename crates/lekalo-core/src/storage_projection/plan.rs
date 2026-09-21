//! The migration-plan derivation over one storage diff (issue #117).
//!
//! [`migration_plan`] turns a [`DiffResult`] into an ordered,
//! non-executable plan document: one step per changed path, byte-sorted
//! deterministically, each step carrying its closed class, its visible
//! data risk, and the closed gate token — `explicit` whenever the risk
//! is destructive or a backfill obligation, `none` otherwise. The plan
//! identity is the sha256 over the canonical step bytes, so a consumer
//! can acknowledge exactly the plan it saw (`--confirm PLAN_ID`); the
//! plan document itself never executes: rendering and execution stay
//! with the runtime adapter the application picks separately.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::storage_projection::diff::DiffResult;
use crate::storage_projection::DataRisk;

/// The closed migration gate tokens.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Gate {
    /// The step applies only after a recorded plan-id confirmation.
    Explicit,
    /// The step carries no migration gate.
    None,
}

impl Gate {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::None => "none",
        }
    }
}

/// One ordered plan step.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct PlanStep {
    /// The canonical changed path.
    pub path: String,
    /// The closed compatibility class.
    pub class: String,
    /// The closed layer of the path.
    pub layer: String,
    /// The visible data risk, when the path carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    /// The closed gate token.
    pub gate: Gate,
}

/// The non-executable migration plan over one diff.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MigrationPlan {
    /// The plan identity: `sha256:<64 lowercase hex>` over the ordered
    /// canonical step bytes.
    #[serde(rename = "planId")]
    pub plan_id: String,
    /// Whether the diff was equal (an empty, gated-free plan).
    pub equal: bool,
    /// The number of steps requiring an explicit gate.
    pub gated: usize,
    /// Every step in byte-sorted canonical order.
    pub steps: Vec<PlanStep>,
}

/// Derive the migration plan from one comparison. Pure and read-only;
/// byte-identical for value-equal diffs.
pub fn migration_plan(diff: &DiffResult) -> MigrationPlan {
    let mut steps: Vec<PlanStep> = diff
        .paths()
        .iter()
        .map(|path| {
            let gated = matches!(
                path.risk(),
                Some(DataRisk::Destructive) | Some(DataRisk::BackfillRequired)
            );
            PlanStep {
                path: path.path().to_owned(),
                class: path.class().key().to_owned(),
                layer: path.layer().key().to_owned(),
                risk: path.risk().map(|risk| risk.key().to_owned()),
                gate: if gated { Gate::Explicit } else { Gate::None },
            }
        })
        .collect();
    steps.sort();
    let gated = steps
        .iter()
        .filter(|step| step.gate == Gate::Explicit)
        .count();
    MigrationPlan {
        plan_id: plan_id(&steps),
        equal: diff.equal(),
        gated,
        steps,
    }
}

/// The plan identity over the exact canonical step bytes: content-
/// bound, deterministic, and echo-safe.
fn plan_id(steps: &[PlanStep]) -> String {
    let mut hasher = Sha256::new();
    for step in steps {
        hasher.update(step.path.as_bytes());
        hasher.update([0]);
        hasher.update(step.class.as_bytes());
        hasher.update([0]);
        hasher.update(step.layer.as_bytes());
        hasher.update([0]);
        hasher.update(step.risk.as_deref().unwrap_or("").as_bytes());
        hasher.update([0]);
        hasher.update(step.gate.key().as_bytes());
        hasher.update([1]);
    }
    let digest = hasher.finalize();
    format!("sha256:{}", hex(&digest))
}

/// Lowercase hex over one digest.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_id_is_content_bound_and_deterministic() {
        let step = PlanStep {
            path: "storage/mysql/tables/task/table".to_owned(),
            class: "breaking".to_owned(),
            layer: "storage".to_owned(),
            risk: Some("destructive".to_owned()),
            gate: Gate::Explicit,
        };
        let first = plan_id(std::slice::from_ref(&step));
        let second = plan_id(std::slice::from_ref(&step));
        assert_eq!(first, second);
        let renamed = PlanStep {
            path: "storage/mysql/tables/task/primaryKey".to_owned(),
            ..step
        };
        assert_ne!(first, plan_id(std::slice::from_ref(&renamed)));
    }
}
