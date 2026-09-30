//! The stable history diagnostic ids (issue #121).
//!
//! Every id is registered in `contracts/diagnostic-registry.v0.4.0.json`
//! before use; details are fixed tokens, never rejected values,
//! absolute paths, secrets, timestamps, or raw SQLite/Git errors.

/// The requested run or dependent belongs to another tenant scope.
pub(crate) const SCOPE_MISMATCH: &str = "history.scope-mismatch";
/// The observation violates the closed contract or its semantic rules.
pub(crate) const INPUT_INVALID: &str = "history.input-invalid";
/// The record references a policy outside the accepted custody family.
pub(crate) const POLICY_MISMATCH: &str = "history.policy-mismatch";
/// The observation carried content outside the safe field grammar.
pub(crate) const UNSAFE_FIELD: &str = "history.unsafe-field";
/// The history home refused an unsafe or out-of-home path.
pub(crate) const PATH_DENIED: &str = "history.path-denied";
/// The history home carries tracked content or its ignore protection
/// cannot be verified.
pub(crate) const TRACKED_STORE: &str = "history.tracked-store";
/// Another writer held the store within the bounded wait.
pub(crate) const BUSY: &str = "history.busy";
/// The store could not be read or written.
pub(crate) const IO: &str = "history.io";
/// The store failed its integrity or digest verification.
pub(crate) const CORRUPT: &str = "history.corrupt";
/// A contract version outside the accepted registry.
pub(crate) const VERSION_UNSUPPORTED: &str = "history.version-unsupported";
/// The run id already exists with different record bytes.
pub(crate) const RUN_CONFLICT: &str = "history.run-conflict";
/// A bound source record no longer resolves to live bytes.
pub(crate) const SOURCE_MISSING: &str = "history.source-missing";
/// The dependent reference was invalidated by a store mutation.
pub(crate) const DEPENDENT_INVALIDATED: &str = "history.dependent-invalidated";
/// The list cursor no longer matches the store generation or scope.
pub(crate) const CURSOR_STALE: &str = "history.cursor-stale";
/// The record exceeds a configured retention bound.
pub(crate) const RETENTION_LIMIT: &str = "history.retention-limit";
