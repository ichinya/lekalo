//! The issue #34 workflow-provider discovery contract: the typed
//! `lekalo provider describe` manifest consumed by AIFHub Extension
//! `/aif-*` lifecycle commands over the external CLI/process JSON
//! boundary. Pure metadata only: no filesystem, no environment, no
//! project, no adapter, and no lifecycle policy. Discovery supports no
//! `init`, `install`, `update`, `sync`, or cleanup operation.

pub mod manifest;
pub mod operations;
pub mod version;

pub use manifest::{Bounds, ContractPin, ProviderManifest};
pub use operations::{EffectClass, Operation, OPERATIONS};
pub use version::{
    DIGEST_ALGORITHM, DISCOVERY_COMMAND, FAMILY, IDENTITY, OPERATION_COUNT,
    RECOMMENDED_BUDGET_TOKENS, SCHEMA_VERSION, VERSION,
};
