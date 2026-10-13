//! Issue #95 watch contract version identity.
//!
//! The watch event stream and per-run result are independent contract
//! families: they take the product version of the commit that introduces
//! them (0.8.0 here) and never drift a Model/IR/graph/effect/lock or
//! diagnostic-item pin. The successor registry that carries the
//! `watch.*` rule family is the merged 0.8.0 union registry (format.*
//! and adoption.* families already merged); predecessor producers keep
//! their exact pins.

/// The exact wire discriminator of the watch event stream contract.
pub const EVENT_SCHEMA_VERSION: &str = "lekalo/watch-event/v0.8.0";

/// The exact identity of the watch event stream contract.
pub const EVENT_IDENTITY: &str = "dev.lekalo.watch-event@0.8.0";

/// The exact wire discriminator of the per-run watch result contract.
pub const RESULT_SCHEMA_VERSION: &str = "lekalo/watch-result/v0.8.0";

/// The exact identity of the per-run watch result contract.
pub const RESULT_IDENTITY: &str = "dev.lekalo.watch-result@0.8.0";

/// The successor registry the watch service emits through: the merged
/// 0.8.0 union (format.* + adoption.* + watch.*) over the 0.6.5 chain.
/// Identical family file as [`crate::diagnostics::registry::FORMAT_REGISTRY_BYTES`];
/// watch only appends its family to the union, it never clobbers a
/// predecessor rule.
pub const WATCH_REGISTRY_VERSION: &str = "0.8.0";
