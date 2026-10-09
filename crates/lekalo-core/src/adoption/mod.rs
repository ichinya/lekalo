//! Scoped brownfield adoption, selective refactoring, and hybrid mode
//! (issue #136).
//!
//! The adoption service composes the existing init/scan/binding/
//! promotion/conformance seams into one reviewable lifecycle: a
//! read-only analysis proposes a minimal starter plan for exactly one
//! selected scope; selective confirmation writes reviewed canonical
//! definitions and bindings through a journaled plan; refactor advice
//! becomes an exact, revision-bound plan; qualified verification really
//! runs the pinned checks in disposable copies; a verified apply
//! installs only the reviewed bytes with snapshots and compare-and-swap
//! rollback; and the coverage join keeps untouched observed, adapted
//! legacy, and new contracted code visible side by side.
//!
//! Evidence is never laundered into authority: proposal documents are
//! derived and local, canonical Model writes happen only through an
//! explicitly confirmed plan, and a failing or missing verification
//! refuses completion honestly.

pub mod analysis;
pub mod apply;
pub mod confirmation;
pub mod coverage;
pub mod diagnostic;
pub mod refactor;
pub mod transaction;
pub mod verification;
pub mod version;
pub mod wire;

/// The outcome type of every adoption service.
pub type Outcome<T> = Result<T, crate::result::DomainResult>;

pub use analysis::{analyze, context, AdoptionContext, AnalysisScope, UseCaseRecord};
pub use apply::{apply as apply_refactor, rollback, rollback_preview};
pub use confirmation::{apply as confirm_apply, plan as confirm_plan, ConfirmRequest};
pub use coverage::{
    coverage, coverage_digest, impact_report, load_coverage, status_report, trace_report,
};
pub use transaction::{
    load_journal, recover, recovery_view, RecoveryOutcome, RecoveryStep, RecoveryView,
};
/// The CLI-facing refusal constructor for unreadable input files.
pub fn diagnostic_input_invalid(field: &str, reason: &str) -> crate::diagnostics::DiagnosticSet {
    diagnostic::input_invalid_set(field, reason)
}
pub use refactor::{plan as refactor_plan, RefactorRequest};
pub use verification::execute as verify;
pub use version::{RECEIPTS_DIR, RECOVERY_DIR, USE_CASES_DIR, VERSION};
pub use wire::{
    AdoptionReceipt, AnalysisDocument, ConfirmationPlan, CoverageDocument, PatchDocument,
    RefactorPlan, VerificationDocument,
};
