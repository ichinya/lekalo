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
    digest_of, pins_of, sha256_of, IndexDocument, IndexStatus, SearchIndex, SearchStore,
};
pub use input::{InputEntry, ProducerKind, SearchInputs};
pub use projection::{
    add_card, attach_effects, bounded_push, project_ir, Card, CardKind, Completeness, Confidence,
    Freshness, NativeBinding, Projection, RelationWitness, SignatureRef, Trust,
};
pub use query::{
    execute, CoverageRow, EffectScope, FilterSet, QueryRequest, SearchOutcome, SearchResult,
};
pub use version::{DEFAULT_LIMIT, INDEX_IDENTITY, INDEX_SCHEMA_VERSION, MAX_POSTINGS, RECIPE};

/// `search.input-invalid` rejection with a bounded reason (and producer).
pub fn diagnostic_input_invalid(
    detail: &'static str,
    producer: Option<&str>,
) -> crate::diagnostics::DiagnosticSet {
    input::input_invalid(detail, producer)
}

/// `search.query-invalid` rejection with a bounded member and reason.
pub fn diagnostic_query_invalid(
    member: &str,
    detail: &'static str,
) -> crate::diagnostics::DiagnosticSet {
    diagnostic::query_invalid(member, detail)
}

/// The exact `sha256:<64 lowercase hex>` spelling.
pub fn is_sha256_digest(text: &str) -> bool {
    version::is_sha256(text)
}
