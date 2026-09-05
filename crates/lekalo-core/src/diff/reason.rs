//! The closed reason vocabulary of the semantic diff (issue #18).
//!
//! Every reason is a direct fact about one change, one history state, one
//! profile rule, or one adapter-evidence state — never an inference from a
//! line count, a filename, a display name, a graph path, or a transitive
//! consumer. Reasons render as stable dotted identifiers with bounded
//! machine parameters only.

/// The exact reason identifier of the change kind itself (`field.removed`).
pub(crate) fn kind_reason(kind_key: &str) -> String {
    kind_key.to_owned()
}

/// The candidate definition was renamed: a matching `renamed_from` plus a
/// direct same-identity history edge prove one semantic rename.
pub const RENAME_HISTORY: &str = "symbol.rename-history";

/// The rename resolved through a bounded multi-hop history walk.
pub const RENAME_HISTORY_MULTIHOP: &str = "symbol.rename-history-multihop";

/// The old id carries a `replaced` tombstone whose replacement is live.
pub const REPLACEMENT_TOMBSTONE: &str = "symbol.replacement-tombstone";

/// The candidate reuses an id an earlier revision tombstoned; old ids are
/// never reusable, so the evidence is classified unknown.
pub const TOMBSTONE_REUSE: &str = "symbol.tombstone-reuse";

/// The history names a replacement or rename target that does not exist.
pub const HISTORY_DANGLING: &str = "history.dangling";

/// More than one live candidate claims the same predecessor.
pub const HISTORY_AMBIGUOUS: &str = "history.ambiguous";

/// The history walk revisited a node: the chain cannot order renames.
pub const HISTORY_CYCLIC: &str = "history.cyclic";

/// A `renamed_from` claim and the registry edge contradict each other.
pub const HISTORY_CONFLICTING: &str = "history.conflicting";

/// The history entry's recorded definition version does not match the
/// definition it describes.
pub const HISTORY_VERSION_INVALID: &str = "history.version-invalid";

/// The history walk exceeded its hop bound without resolving.
pub const HISTORY_TRUNCATED: &str = "history.truncated";

/// The storage profile found no storage projection evidence.
pub const STORAGE_EVIDENCE_ABSENT: &str = "profile.storage-evidence-absent";

/// The target profile found no target capability evidence.
pub const TARGET_EVIDENCE_ABSENT: &str = "profile.target-evidence-absent";

/// Adapter evidence was captured against an older input revision.
pub const ADAPTER_STALE: &str = "adapter.stale";

/// Two adapter contributions contradict each other.
pub const ADAPTER_CONFLICTING: &str = "adapter.conflicting";

/// The adapter declares a capability this comparison cannot consume.
pub const ADAPTER_UNSUPPORTED: &str = "adapter.unsupported";

/// The adapter contribution is unverifiable.
pub const ADAPTER_UNKNOWN: &str = "adapter.unknown";

/// Every reason identifier that classifies evidence as not current.
pub const EVIDENCE_REASONS: [&str; 9] = [
    HISTORY_DANGLING,
    HISTORY_AMBIGUOUS,
    HISTORY_CYCLIC,
    HISTORY_CONFLICTING,
    HISTORY_VERSION_INVALID,
    HISTORY_TRUNCATED,
    TOMBSTONE_REUSE,
    ADAPTER_STALE,
    ADAPTER_CONFLICTING,
];

/// Whether one reason marks the change evidence as degraded or unknown.
pub(crate) fn is_evidence_reason(reason: &str) -> bool {
    EVIDENCE_REASONS.contains(&reason)
}
