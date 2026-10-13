//! The worklog-changelog pilot composition contract (issue #51).
//!
//! One closed, independent, versioned composition document —
//! `lekalo/worklog-changelog-pilot/v0.8.0`, identity
//! `dev.lekalo.worklog-changelog-pilot@0.8.0` — pins the neutral pilot
//! Model/IR state and declares the twelve pilot operation roles with
//! their explicit effect declarations, commit boundaries, independence
//! rules, correlations, approval and snapshot field bindings, the
//! public field allowlist, replay scopes, and capability requirements.
//!
//! The core invariant the composition carries (issue #51): worklog,
//! provider comment, and public changelog are different entities and
//! effects; no action enables another implicitly. Every declared
//! effect is cross-checked against the pinned compilation's canonical
//! effect projection, and every composition role names a declared
//! command or query of the same Model — the composition can never
//! couple effects the Model itself keeps independent.
//!
//! Family layout, following the accepted house convention:
//!
//! - [`version`] pins the independent contract identity and the
//!   closed owner-approved bounds;
//! - [`types`] is the typed attachment value;
//! - [`wire`] is the bounded parse seam from exact JSON bytes with
//!   duplicate-key rejection;
//! - [`validate`] joins the parsed composition to the pinned
//!   compilation: role resolution, effect-graph agreement, allowlist
//!   privacy, approval binding, snapshot immutability, and replay
//!   scope checks;
//! - [`canonical`] renders the deterministic canonical bytes and
//!   digest;
//! - [`diagnostic`] routes every failure through the accepted #11
//!   registry with the `worklog-changelog.*` rules of the 0.8.0
//!   successor and bounded tokens only.

pub mod canonical;
pub mod diagnostic;
pub mod evidence;
pub mod types;
pub mod validate;
pub mod version;
pub mod wire;

pub use evidence::{
    admit, io_failure, AdmissionProfile, AdmissionReceipt, BackendProvenance, ExpectedConsumer,
    NativeExecutionReceipt, TargetVerdict,
};
pub use types::Composition;
pub use wire::parse;

use crate::result::DomainResult;

/// Decode one consumed-attachment document with member uniqueness
/// enforced before any map insertion. The shell hashes the exact bytes
/// and hands them here so the same duplicate-key refusal the
/// composition and evidence documents enforce closes the last wire
/// seam; the core stays the single owner of the closed decode.
pub fn parse_unique_consumed(
    text: &str,
) -> Result<serde_json::Value, crate::diagnostics::DiagnosticSet> {
    canonical::canonical_parse_unique(text)
}

/// Validate one composition document against its pinned compilation:
/// the full pure pipeline from exact bytes to the resolved attachment.
/// Load and compile failures pass through unchanged; every
/// composition violation rejects with an explicit registered
/// diagnostic and no partial result.
pub fn validate_document(
    bytes: &[u8],
    compilation: &crate::ir::Compilation,
) -> Result<Composition, DomainResult> {
    let composition = parse(bytes).map_err(DomainResult::invalid)?;
    composition
        .validate(&compilation.project)
        .map_err(DomainResult::invalid)?;
    Ok(composition)
}
