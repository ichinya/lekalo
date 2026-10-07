//! The managed receipt views projected by the CLI (issue #41).
//!
//! These types live in the core so the presentation crate stays a pure
//! selector/renderer; their canonical serialization is part of the stable
//! success payload bytes.

use super::wire::GenerationPlan;

/// The admission receipt of one managed module/target assignment.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentView {
    pub module: String,
    pub target: String,
    pub mode: String,
    pub generated_root: String,
    pub extensions: usize,
}

/// The `managed check` receipt.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReceipt {
    pub project: String,
    pub assignments: Vec<AssignmentView>,
    pub admitted: Vec<String>,
    pub coverage_digest: Option<String>,
}

/// The `managed authority` receipt: the exact generation-input pins of the
/// project plus the module's required ledger.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityReceipt {
    pub module: String,
    pub model: String,
    pub ir: String,
    pub lock: String,
    pub ledger: Vec<String>,
}

/// The `managed plan` receipt.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReceipt {
    pub action: String,
    pub module: String,
    pub target: String,
    pub plan: GenerationPlan,
}

/// One exact pin difference between two coverage documents.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeChange {
    pub field: String,
    pub before: String,
    pub after: String,
}

/// The `managed upgrade-plan` receipt.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeReceipt {
    pub module: String,
    pub target: String,
    pub comparable: bool,
    pub changes: Vec<UpgradeChange>,
    pub requirements_added: Vec<String>,
    pub requirements_removed: Vec<String>,
}
