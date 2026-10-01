//! The CI report surface (issue #103).
//!
//! One closed, versioned report document per headless CI run, plus pure
//! projections to the formats CI consumers read: native JSON (the
//! canonical report), JUnit XML (scenario/gate suites), SARIF 2.1.0
//! (source diagnostics with repository-relative safe paths), and a
//! concise Markdown job summary. Rendering is pure and deterministic:
//! no timestamps, durations, absolute paths, host or user identity, or
//! raw child output ever enters a projection.

pub mod build;
pub mod junit;
pub mod markdown;
pub mod model;
pub mod provenance;
pub mod sarif;
#[cfg(test)]
mod tests;
pub mod version;

pub use build::{
    apply_case_policy, apply_check_policy, build, with_diagnostics, CaseDraft, CheckDraft,
    CiPolicy, CommandOutcome, SuiteDraft,
};
pub use model::{
    AdapterProvenance, CaseRow, CheckRow, CiReport, CommandName, Coverage, EffectiveOutcome,
    Evaluation, FailureClass, GitProvenance, InputProvenance, Invocation, KnownValue, Outcome,
    ProfileProvenance, Provenance, Publication, PublicationDecision, SerializableDiagnostic,
    SourceOutcome, Suite, SuiteKind, UnknownReason, ValueState, Verdict,
};
