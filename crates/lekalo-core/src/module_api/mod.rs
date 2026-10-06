//! Versioned module public API and dependency boundaries (issue #113).
//!
//! The closed pure surface of the family: [`resolve`] decodes and binds the
//! source attachment, [`check`] runs the boundary validation with typed
//! findings and cycle witnesses, [`export_public`] materializes the
//! standalone public bundle, [`consumer_snapshot`] joins declared and
//! resolved consumer uses, [`compare_public`] classifies public changes and
//! exact affected consumers, [`resolve_lock`] builds and checks consumer
//! locks, and [`plan_consumer_context`] plans a bounded capsule from a
//! public bundle alone. The core owns semantic closure and policy only —
//! never the filesystem, subprocesses, caches, or networks.
//!
//! Status mapping follows `result.rs`: malformed inputs are invalid (1),
//! unknown contract versions unsupported-version (5), strict semantic
//! violations invalid (1), strict policy failures denied (3), and required
//! missing evidence unavailable (4). Advisory reports stay valid (0).

pub mod consumers;
pub mod context;
pub mod diagnostic;
pub mod diff;
pub mod effects;
pub mod graph;
pub mod lock;
pub mod public;
pub mod resolve;
pub mod validate;
pub mod version;
pub mod wire;

pub use context::{estimate, plan_consumer_context};
pub use consumers::{consumer_snapshot, parse_snapshot};
pub use diff::compare_public;
pub use lock::{bundle_lock_evidence, check_lock, parse_lock, resolve_lock};
pub use public::{export_public, parse_bundle};
pub use resolve::resolve;
pub use validate::{check, FindingClass, Verdict};
pub use version::{ApiVersion, ConsumerPolicy, RequiredBump, VersionInterval};
pub use wire::{
    Attachment, ConsumerLock, ConsumerSnapshot, ContextCapsule, DiffReport, PublicBundle, Report,
    VersionPolicyWire,
};
