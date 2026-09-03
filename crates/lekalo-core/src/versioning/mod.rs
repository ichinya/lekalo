//! Issue #9: versioning and migrations for the Model, IR, and protocol
//! contract families.
//!
//! Three contracts evolve independently of each other and of the product
//! release. This module owns:
//!
//! - strict canonical SemVer parsing ([`version`]) over sealed contract
//!   families ([`family`]);
//! - the embedded, validated version registry with exact-set support,
//!   deprecation windows, retirement, aliases, and migration edges
//!   ([`registry`]);
//! - the shared support gate every command passes through ([`support`]);
//! - the sealed Model migration catalog and its one-to-one registry
//!   binding ([`graph`], `model_v0_1_0_to_v1_0_0`);
//! - the pure planner with semantic diff and deterministic plan identity
//!   ([`plan`]);
//! - the capability-safe transactional writer with backup, journal,
//!   rollback, and recovery (`migration::transaction`);
//! - the adapter compatibility preflight ([`compatibility`]).
//!
//! Nothing here rewrites the accepted #6 dual-contract dispatch, the #7
//! loader pipeline, or the #8 typed IR: the planner consumes their typed
//! outputs, and an IR transition is always produced by rebuilding from
//! migrated Model source.

pub mod compatibility;
pub mod family;
pub mod graph;
pub mod migration;
pub mod plan;
pub mod registry;
pub mod support;
pub mod version;

mod model_v0_1_0_to_v1_0_0;

use crate::loader::error::Diagnostic;
use crate::loader::{LoadOutput, LoadStatus, ModelVersion};
use crate::project_fs::Fs;

pub use compatibility::{
    AdapterCompatibilityManifest, CompatibilityPreflight, CompatibilityVerdict,
};
pub use migration::{
    MigrationOutcome, MigrationReceipt, MigrationService, MigrationTransactionState, PlanFailure,
    PreparedMigration, VersioningFailure,
};
pub use plan::{SemanticDiff, VersionChange};
pub use registry::{
    AliasToken, ChangeClassification, FamilyRegistry, MigrationEdge, RegenerationImpact,
    RegistryError, VersionRecord, VersionRegistry, VersionState, REGISTRY_BYTES, REGISTRY_IDENTITY,
};
pub use support::{
    ensure_usable, ir_support, model_support, protocol_support, ModelTarget, SupportVerdict,
    TargetError, TargetMalformation, UnsupportedVersion,
};
pub use version::{ContractVersion, VersionParseError};

/// Stable `#9` reason codes. The list is closed until issue #11 widens the
/// diagnostic schema.
pub mod reasons {
    pub const INVALID_VERSION: &str = "versioning.invalid-version";
    pub const UNSUPPORTED_VERSION: &str = "versioning.unsupported-version";
    pub const MIXED_VERSIONS: &str = "versioning.mixed-versions";
    pub const REGISTRY_INVALID: &str = "versioning.registry-invalid";
    pub const NO_MIGRATION_PATH: &str = "versioning.no-migration-path";
    pub const NOT_FILE_MIGRATABLE: &str = "versioning.not-file-migratable";
    pub const MIGRATION_PRECONDITION: &str = "versioning.migration-precondition";
    pub const SOURCE_CHANGED: &str = "versioning.source-changed";
    pub const MIGRATION_IN_PROGRESS: &str = "versioning.migration-in-progress";
    pub const BACKUP_FAILED: &str = "versioning.backup-failed";
    pub const COMMIT_FAILED: &str = "versioning.commit-failed";
    pub const ROLLBACK_CONFLICT: &str = "versioning.rollback-conflict";
    pub const ROLLBACK_FAILED: &str = "versioning.rollback-failed";
    pub const RECOVERY_REQUIRED: &str = "versioning.recovery-required";
    pub const ADAPTER_MANIFEST_INVALID: &str = "versioning.adapter-manifest-invalid";
    pub const ADAPTER_INCOMPATIBLE: &str = "versioning.adapter-incompatible";
    pub const PROTOCOL_UNPUBLISHED: &str = "versioning.protocol-unpublished";
    pub const EXTENSION_INCOMPATIBLE: &str = "versioning.extension-incompatible";
}

/// The shared support gate behind load phase 6b: `Some` when the loaded
/// Model version must fail closed (retired or unregistered), `None` when it
/// is supported or deprecated (still fully usable).
pub(crate) fn gate_loaded_model_version(version: ModelVersion) -> Option<LoadOutput> {
    let registry = match VersionRegistry::embedded() {
        Ok(registry) => registry,
        Err(_) => {
            // A broken embedded registry is a developer fault, surfaced
            // honestly instead of silently loading without policy.
            return Some(LoadOutput::failure(
                LoadStatus::Invalid,
                vec![Diagnostic::new(reasons::REGISTRY_INVALID)],
            ));
        }
    };
    let verdict = support::model_support(registry, version);
    support::ensure_usable(&verdict).err().map(|unsupported| {
        let mut data = serde_json::json!({
            "version": unsupported.version,
            "state": unsupported.state,
        });
        if let Some(replacement) = &unsupported.replacement {
            data["replacement"] = serde_json::json!(replacement);
        }
        LoadOutput::failure(
            LoadStatus::UnsupportedVersion,
            vec![Diagnostic::new(reasons::UNSUPPORTED_VERSION).with_data(data)],
        )
    })
}

/// `Some("versioning.recovery-required")` while a durable migration journal
/// or runtime migration lock exists under `.lekalo/cache/migrations`; every
/// reader fails closed until the next explicit migrate operation recovers.
pub(crate) fn migration_recovery_code(fs: &Fs) -> Option<&'static str> {
    const HOME: &str = ".lekalo/cache/migrations";
    if let Ok(crate::project_fs::EntryType::File) = fs.entry_type(HOME, "active.lock") {
        return Some(reasons::RECOVERY_REQUIRED);
    }
    let entries = fs.entries(HOME).ok()?;
    for (name, entry_type) in entries {
        if entry_type != crate::project_fs::EntryType::Directory {
            continue;
        }
        if matches!(
            fs.entry_type(&format!("{HOME}/{name}"), "journal.json"),
            Ok(crate::project_fs::EntryType::File)
        ) {
            return Some(reasons::RECOVERY_REQUIRED);
        }
    }
    None
}
