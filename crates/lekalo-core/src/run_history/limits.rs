//! The closed allocation bounds of the run history (issue #121).
//!
//! Every bound is checked before allocation or persistence; an
//! oversized record is refused with `history.retention-limit`, an
//! oversized observation with `history.input-invalid`. The input bound
//! covers the raw harness bytes so a hostile stream cannot grow memory
//! before validation.

/// The maximum accepted raw observation size from `--input -`.
pub(crate) const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// The maximum canonical run-record size.
pub(crate) const MAX_RECORD_BYTES: usize = 1024 * 1024;
/// The maximum canonical assertion-set size.
pub(crate) const MAX_ASSERTION_BYTES: usize = 1024 * 1024;
/// The default retention window in days.
pub(crate) const DEFAULT_MAX_AGE_DAYS: u32 = 30;
/// The default retention record bound per tenant scope.
pub(crate) const DEFAULT_MAX_RECORDS: u32 = 10_000;
/// The default retention logical-payload bound per tenant scope (64 MiB).
pub(crate) const DEFAULT_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// The maximum list page size.
pub(crate) const MAX_LIST_LIMIT: usize = 200;
/// The bounded SQLite writer wait before `history.busy`.
pub(crate) const LOCK_WAIT_MILLIS: u64 = 5_000;
