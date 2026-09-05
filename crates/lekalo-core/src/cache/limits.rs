//! Bounded limits, checked before any allocation (issue #20).

/// Maximum entries accepted in one store.
pub(crate) const MAX_ENTRIES: usize = 1_000_000;

/// Maximum total payload bytes accepted in one store (256 MiB).
pub(crate) const MAX_TOTAL_PAYLOAD_BYTES: usize = 256 * 1024 * 1024;

/// Maximum bytes accepted for one record payload (8 MiB, matching the
/// loader's per-document bound).
pub(crate) const MAX_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;

/// Maximum quarantined stores kept before the oldest corrupt bytes are
/// dropped (quarantine never overwrites an existing item).
pub(crate) const MAX_QUARANTINE_ITEMS: usize = 8;

/// The bounded SQLite busy timeout for the single writer (milliseconds).
pub(crate) const LOCK_WAIT_MILLIS: u64 = 5_000;
