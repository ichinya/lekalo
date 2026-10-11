//! Issue #93: the typed, offline semantic search service.
//!
//! One service, two entry points: `find` applies exact filters;
//! `search` combines those filters with deterministic lexical retrieval
//! (optionally bounded-labeled fuzzy). The index is a local, disposable
//! projection built from the canonical typed IR plus explicitly admitted
//! target/trace/effect evidence. Answers are bounded selection cards
//! with provenance, coverage, and explanations — never a repository
//! text scan, never an adapter launch, never a binding promotion, and
//! never an ambiguous candidate silently selected.
//!
//! Module composition:
//! - [`version`]: the closed contract identities, recipe, and bounds;
//! - [`input`]: the `search-inputs` manifest (typed producers only);
//! - [`projection`]: typed cards, relation witnesses, postings;
//! - [`index`]: the in-memory snapshot, its content digest, and the
//!   durable `.lekalo/cache/search/` generation store;
//! - [`query`]: request admission, exact filters, lexical tiering,
//!   pagination, and the result wire;
//! - [`rank`]: the frozen `semantic-search/1` normalization/ranking;
//! - [`cursor`]: the opaque generation-bound page cursor;
//! - [`wire`]: canonical bytes, duplicate-key rejection, digests;
//! - [`diagnostic`]: the registered `search.*` family emitters.

pub mod cursor;
pub mod diagnostic;
pub mod index;
pub mod input;
pub mod projection;
pub mod query;
pub mod rank;
pub mod version;
pub mod wire;

pub use cursor::{Cursor, CursorBinding, CursorError, LastTuple};
pub use index::{
    digest_of, pins_of, sha256_of, AdmissionConfig, IndexDocument, IndexStatus, InputPin,
    SearchIndex, SearchStore,
};
pub use input::{InputEntry, ProducerKind, SearchInputs};
pub use projection::{
    add_card, add_qualified_id, attach_effects, bounded_push, effect_posting_terms, post_term,
    project_ir, push_fact, Card, CardKind, Completeness, Confidence, Freshness, NativeBinding,
    Projection, RelationWitness, SignatureRef, Trust,
};
pub use query::{
    domain_needed, execute, CoverageRow, EffectScope, FilterSet, QueryRequest, SearchOutcome,
    SearchResult,
};
pub use version::{
    DEFAULT_LIMIT, INDEX_IDENTITY, INDEX_SCHEMA_VERSION, INPUTS_IDENTITY, INPUTS_SCHEMA_VERSION,
    MAX_INPUT_BYTES, MAX_POSTINGS, RECIPE,
};

/// `search.input-invalid` rejection with a bounded reason (and producer).
pub fn diagnostic_input_invalid(
    detail: &'static str,
    producer: Option<&str>,
) -> crate::diagnostics::DiagnosticSet {
    input::input_invalid(detail, producer)
}

/// Reject duplicate JSON keys anywhere in one JSON text before decode
/// (the closed-document convention of every admitted search byte source).
#[allow(clippy::result_unit_err)]
pub fn reject_duplicate_keys(text: &str) -> Result<(), ()> {
    wire::reject_duplicate_keys(text)
}

/// `search.query-invalid` rejection with a bounded member and reason.
pub fn diagnostic_query_invalid(
    member: &str,
    detail: &'static str,
) -> crate::diagnostics::DiagnosticSet {
    diagnostic::query_invalid(member, detail)
}

/// The `search.coverage-incomplete` warning for a needed domain whose
/// admitted state is not complete (missing/stale/partial/unsupported).
pub fn emit_coverage_incomplete(domain: &str, state: &str) -> crate::diagnostics::DiagnosticSet {
    diagnostic::coverage_incomplete(domain, state)
}

/// The `search.index-stale` warning for pins that no longer match.
pub fn emit_index_stale(pin: &str) -> crate::diagnostics::DiagnosticSet {
    diagnostic::index_stale(pin, false)
}

/// The `search.result-limit` warning counting producer rows the
/// projection could not attach (lost candidates are counted, never
/// silent).
pub fn emit_dropped_rows(count: u64) -> crate::diagnostics::DiagnosticSet {
    diagnostic::dropped_rows(count)
}

/// The exact `sha256:<64 lowercase hex>` spelling.
pub fn is_sha256_digest(text: &str) -> bool {
    version::is_sha256(text)
}
