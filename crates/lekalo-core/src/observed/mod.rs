//! Observed mode for existing code (issues #39 and #42).
//!
//! Source code is primary. Lekalo records explicit semantic bindings and
//! evidence; it never generates or overwrites implementation, and an
//! inferred fact never becomes canonical without confirmation. The
//! observed index lives under the accepted `lekalo.observed-model-draft`
//! authority home (`.lekalo/import/**`), binds observed symbols to
//! source locations through adapter-provided scans, distinguishes
//! explicit/confirmed/inferred facts, detects stale bindings, feeds
//! inspect/impact projections with explicit incompleteness, and promotes
//! individual symbols or whole modules into the canonical (`contracted`)
//! model only through the planned and confirmed workflow in
//! [`promote`]. Issue #42 turns the index into the full binding
//! registry: adapter-driven [`scan_service`] runs, [`bindings`]
//! propose/confirm/audit/list, candidate sets for ambiguous mappings,
//! declared targets and profiles, and native test bindings.

pub mod bindings;
pub mod diagnostic;
pub mod index;
pub mod promote;
pub mod scan_service;
pub mod store;
pub mod types;
pub mod version;
pub mod view;
mod wire;

pub use diagnostic::{missing_index_set, scan_io_failure, scan_limit_set};
pub use index::{
    attach, bind_explicit, canonical_bytes, confirm_binding, index_digest, load_index, parse_index,
    staleness, update_index, AttachReceipt, BindReceipt, ConfirmReceipt, StalenessReceipt,
    UpdateReceipt,
};
pub use promote::{
    apply, plan, Ineligible, PlanEntry, PromotionApplyReceipt, PromotionPlanReceipt,
    PromotionSelection,
};
pub use types::{
    AdapterIdentity, BindingState, BindingStatus, Confidence, EndpointRecord, Evidence,
    FieldEvidence, HistoryEntry, HistoryEvent, ObservedIndex, Origin, Provenance,
    ReferenceEvidence, ReferenceRole, SchemaKind, SchemaRecord, SourceLocation, SymbolKind,
    SymbolRecord, ValueEvidence,
};
pub use version::{
    FAMILY, INDEX_DIR, INDEX_IDENTITY, INDEX_NAME, MAX_SCAN_BYTES, MODE, SCAN_SCHEMA_VERSIONS,
    SCHEMA_VERSION, VERSION,
};

/// The maximum source file accepted for fingerprinting.
pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;

/// The loaded project context every observed operation runs against:
/// the normalized canonical model plus the validated root. Loader and
/// structure failures surface here untouched as the terminal
/// [`crate::result::DomainResult`].
pub struct ObservedContext {
    pub model: crate::loader::NormalizedModel,
    pub root: std::path::PathBuf,
}

/// Load one project selection for the observed seam.
pub fn context(
    selection: &crate::loader::LoadSelection,
) -> Result<ObservedContext, crate::result::DomainResult> {
    crate::loader::load_with_snapshot(selection).map(|loaded| ObservedContext {
        model: loaded.model,
        root: loaded.root,
    })
}
