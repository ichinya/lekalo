//! The cache contract identity (issue #20).
//!
//! `dev.lekalo.cache@0.2.16` is independent of the product release, of every
//! Model/IR/graph/effect/lock/protocol contract version, and of the
//! diagnostic registry. The product version is informational only and never
//! enters a key or a record.

/// The cache contract identity (`dev.lekalo.cache@0.2.16`).
pub(crate) const IDENTITY: &str = "dev.lekalo.cache@0.2.16";

/// The cache contract discriminator (`lekalo/cache/v0.2.16`).
pub(crate) const SCHEMA_VERSION: &str = "lekalo/cache/v0.2.16";

/// The cache-owned revision of the mirrored decode pipeline. Bumped
/// whenever the pipeline's parse/decode reuse semantics change; part of
/// every record binding so stale pipelines never produce trusted hits.
pub(crate) const PIPELINE_REVISION: u64 = 1;

/// The one storage backend of v1.
pub(crate) const BACKEND: &str = "sqlite";
