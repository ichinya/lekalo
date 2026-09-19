//! The embedded PostgreSQL engine profile (issue #69).
//!
//! The per-engine half of the storage-engine family: the
//! owner-published version matrix, the policy-gated type table, the
//! deterministic identifier quoting, and the #24 capability-snapshot
//! builder. The module is engine data and pure functions — no
//! connection, no execution, no runtime. A sibling `mysql/` profile
//! (#117) reuses the same shape with its own matrix, type table, and
//! answers.

pub mod quoting;
pub mod snapshot;
pub mod types;
pub mod version_matrix;
