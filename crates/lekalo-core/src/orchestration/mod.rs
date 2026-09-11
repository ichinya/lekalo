//! The generate and verify orchestration commands (issue #91).
//!
//! The unified core pipelines behind `lekalo generate` and
//! `lekalo verify`: they validate the Model, resolve the lock, profile,
//! and adapter capabilities, compute the affected scope, request adapter
//! write plans through the published `lekalo.target/v1` protocol,
//! validate paths and ownership, execute only on an explicit command,
//! verify actual writes against the plan, and maintain the artifact
//! manifest and evidence atomically. No target logic lives here: the
//! adapter owns every target decision, and this layer aggregates results
//! from one model so the human and JSON renderers never diverge.
//!
//! Diagnostics reuse the accepted closed registry (no new rules); the
//! receipts are typed result data published as
//! `contracts/orchestration-report.schema.v1.0.0.json`
//! (`dev.lekalo.orchestration-report@1.0.0`).

mod catalog;
mod diagnostic;
mod generate;
mod receipt;
mod verify;
mod version;

pub use catalog::{candidate_supply, AdapterSupply};
pub use diagnostic::Failure;
pub use generate::{generate, GenerateRequest};
pub use receipt::{
    ComponentReceipt, ComponentState, GenerateReceipt, InputsReceipt, IrEvidenceReceipt,
    ScenarioCoverage, ScopeReceipt, SeverityCounts, TargetCounts, TargetReceipt, TargetState,
    TraceSummary, Verdict, VerdictCountsReceipt, VerifyReceipt, WriteReceipt, IDENTITY,
    SCHEMA_VERSION,
};
pub use verify::{verify, VerifyRequest};
pub use version::{DEFAULT_TIMEOUT_MS, IR_EVIDENCE_DIR, MAX_TARGETS};

use crate::loader::LoadSelection;

/// The validated physical project root of one selection; the terminal
/// loader result passes through untouched.
pub fn project_root(
    selection: &LoadSelection,
) -> Result<std::path::PathBuf, crate::result::DomainResult> {
    crate::loader::root_for_selection(selection)
}

/// The `--locked` preflight of the drift gate: prepare the accepted
/// pipeline, then demand the full locked inventory before any read.
pub fn locked_check(selection: &LoadSelection) -> Result<(), crate::result::DomainResult> {
    let prepared = crate::artifacts::check::Prepared::prepare(selection)
        .map_err(|failure| crate::result::DomainResult::from(&Failure::Artifact(failure)))?;
    generate::locked_preflight(&prepared)
}
