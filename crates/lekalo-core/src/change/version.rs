//! Change contract identity constants (issue #73).
//!
//! Every artifact of the change family is independently versioned at the
//! product generation of its implementation commit (`docs/versioning.md`).
//! The identities below are the single source of truth for the embedded
//! catalog, the wire discriminators, and the byte pins the transaction
//! revalidates.

/// The exact wire discriminator of the change request contract.
pub const REQUEST_SCHEMA_VERSION: &str = "lekalo/change-request/v0.8.0";

/// The exact contract identity of the change request contract.
pub const REQUEST_IDENTITY: &str = "dev.lekalo.change-request@0.8.0";

/// The exact wire discriminator of the change catalog contract.
pub const CATALOG_SCHEMA_VERSION: &str = "lekalo/change-catalog/v0.8.0";

/// The exact contract identity of the change catalog contract.
pub const CATALOG_IDENTITY: &str = "dev.lekalo.change-catalog@0.8.0";

/// The exact wire discriminator of the change policy contract.
pub const POLICY_SCHEMA_VERSION: &str = "lekalo/change-policy/v0.8.0";

/// The exact contract identity of the change policy contract.
pub const POLICY_IDENTITY: &str = "dev.lekalo.change-policy@0.8.0";

/// The exact wire discriminator of the change plan contract.
pub const PLAN_SCHEMA_VERSION: &str = "lekalo/change-plan/v0.8.0";

/// The exact contract identity of the change plan contract.
pub const PLAN_IDENTITY: &str = "dev.lekalo.change-plan@0.8.0";

/// The exact wire discriminator of the change receipt contract.
pub const RECEIPT_SCHEMA_VERSION: &str = "lekalo/change-receipt/v0.8.0";

/// The exact contract identity of the change receipt contract.
pub const RECEIPT_IDENTITY: &str = "dev.lekalo.change-receipt@0.8.0";

/// The exact wire discriminator of the change history contract.
pub const HISTORY_SCHEMA_VERSION: &str = "lekalo/change-history/v0.8.0";

/// The exact contract identity of the change history contract.
pub const HISTORY_IDENTITY: &str = "dev.lekalo.change-history@0.8.0";

/// The logical home of the canonical per-project history sidecar.
pub const HISTORY_LOGICAL_PATH: &str = "lekalo/change-history.json";

/// The logical home of the per-transaction journal/recovery material.
pub const TRANSACTION_CACHE_ROOT: &str = ".lekalo/cache/change";

/// Maximum accepted request bytes before any decode runs.
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;

/// Maximum operations accepted in one request batch.
pub const MAX_OPERATIONS: usize = 100;

/// Maximum literal-tree depth accepted inside one operation payload.
pub const MAX_LITERAL_DEPTH: usize = 4;

/// Maximum per-side bytes of one file diff entry in a plan.
pub const MAX_FILE_DIFF_BYTES: usize = 16 * 1024;

/// Maximum patch lines of one file diff entry in a plan.
pub const MAX_PATCH_LINES: usize = 512;

/// Maximum pinned inputs in one plan export (the published
/// `change-plan.schema` `inputs.maxItems`); a larger valid project
/// degrades the plan to `complete: false` instead of emitting a document
/// its own contract refuses (r6, finding omp F1).
pub const MAX_PLAN_INPUTS: usize = 1000;

/// Maximum planned file replacements in one plan export (the published
/// `fileEdits`/`fileDiff` `maxItems`), same completion degradation.
pub const MAX_PLAN_FILES: usize = 1000;

/// Maximum serialized plan-export bytes: the research contract's
/// "bounded plan export" ceiling. A projection over the cap refuses the
/// whole preparation rather than emit a truncated plan presented as
/// complete.
pub const MAX_PLAN_EXPORT_BYTES: usize = 32 * 1024 * 1024;
