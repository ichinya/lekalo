//! Issue #17: the bounded context capsule for one symbol or one typed
//! changed scope.
//!
//! A capsule is the minimal-but-sufficient context an AI agent needs to
//! work on one symbol or change set, selected deterministically over the
//! accepted #12/#13/#14 surfaces (typed inspect facts, the dependency
//! graph, the effect graph) — never over raw source. Protected semantic
//! facts (the root contract, its policies, effects, direct dependencies,
//! scenarios, public impact, and bindings) are emitted as typed records
//! and never collapsed into ambiguous prose; supporting context (type
//! cards and the bounded closure) is ranked and may be excluded. Every
//! exclusion is one explainable manifest row, truncation is always
//! explicit metadata, and the fixed offline estimator pins its identity,
//! version, and digest into the wire.
//!
//! Raw source is excluded by default: the capsule never reads files and
//! never carries file bytes, secrets, `.env` content, or absolute paths.
//! The only source evidence is the opt-in declaration-span sidecar
//! (`--spans`), resolved through the #8 source map as logical
//! project-relative paths.
//!
//! Boundaries: this module owns selection, estimation, and projection. It
//! never validates semantics (#12), never builds graphs itself beyond the
//! accepted builders, never detects effects (#14/#27), never derives
//! changed inputs (#16), and never persists anything — the neutral
//! in-memory projection is the whole product; persisted
//! `.ai-factory/context/**` artifacts are owned by AI Factory.
//!
//! Determinism: the same compilation, scope, budget, and spans flag always
//! produce byte-identical JSON and Markdown. Canonical bytes are compact
//! UTF-8 JSON with byte-sorted object keys; sections appear in protection
//! order and the manifest in selection-walk order.

mod canonical;
mod diagnostic;
pub mod estimate;
mod model;
mod render;
mod select;
pub mod version;

use crate::diagnostics::DiagnosticSet;
use crate::ir::Compilation;

pub use select::CapsuleScope;

/// One finished capsule: the single normalized product both projections
/// render. Every accessor is side-effect free.
#[derive(Clone, Debug)]
pub struct Capsule {
    inner: select::Selected,
}

/// Plan one capsule over a compiled project.
///
/// Load, IR, graph, and effect failures never reach this seam: callers
/// compile through the accepted surfaces first, and this entry builds the
/// accepted graph and effect projections itself. A fatal input (budget
/// out of range, unknown root, empty scope, over-bound manifest) is an
/// `invalid` diagnostic set; every other outcome is a valid capsule.
pub fn plan(
    scope: &CapsuleScope,
    budget: u64,
    spans: bool,
    compilation: &Compilation,
) -> Result<Capsule, DiagnosticSet> {
    let project = &compilation.project;
    let graph = crate::graph::build(project)?;
    let effects = crate::effects::build(project)?;
    let inner = select::plan(
        scope,
        budget,
        spans,
        project,
        compilation.source_map.entries(),
        &graph,
        &effects,
    )?;
    Ok(Capsule { inner })
}

impl Capsule {
    /// The canonical JSON payload bytes (without the envelope wrapper and
    /// without a trailing newline).
    ///
    /// The caller wraps these bytes as
    /// `{"status":"valid","context":{...}}` — the payload stays
    /// producer-owned exactly as in the accepted #7/#13/#14 seams.
    pub fn to_canonical_json(&self) -> String {
        render::json(&self.inner)
    }

    /// The deterministic Markdown document.
    pub fn to_markdown(&self) -> String {
        render::markdown(&self.inner)
    }

    /// Whether every candidate fact fit the budget.
    pub fn fits(&self) -> bool {
        self.inner.fits
    }

    /// The estimated tokens of the emitted capsule.
    pub fn estimated_tokens(&self) -> u64 {
        self.inner.estimated
    }

    /// The exact minimum budget that fits every candidate fact.
    pub fn minimum_required_tokens(&self) -> u64 {
        self.inner.minimum_required
    }

    /// The requested budget limit.
    pub fn budget(&self) -> u64 {
        self.inner.budget
    }

    /// The number of candidate facts (included plus excluded).
    pub fn candidate_count(&self) -> usize {
        self.inner.manifest.len()
    }

    /// The number of included facts.
    pub fn included_count(&self) -> usize {
        self.inner
            .manifest
            .iter()
            .filter(|row| row.reason.is_none())
            .count()
    }

    /// The number of excluded facts.
    pub fn excluded_count(&self) -> usize {
        self.inner
            .manifest
            .iter()
            .filter(|row| row.reason.is_some())
            .count()
    }

    /// The qualified root identities in canonical order.
    pub fn roots(&self) -> &[String] {
        &self.inner.roots
    }

    /// The capsule mode wire spelling (`symbol` or `changed`).
    pub fn mode(&self) -> &'static str {
        self.inner.mode
    }
}
