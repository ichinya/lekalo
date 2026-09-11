//! Closed identity, wire, and bound constants of the orchestration
//! surface (issue #91).

/// The runtime home of the canonical IR evidence one generate run binds
/// and one verify run consumes. The evidence file is derived runtime
/// cache under the reserved `.lekalo/cache/` area — never source, never
/// a manifest artifact, never the only copy of a semantic decision.
pub const IR_EVIDENCE_DIR: &str = ".lekalo/cache/ir";

/// The default adapter-operation deadline, in milliseconds. The value
/// matches the accepted protocol default; callers may lower it.
pub const DEFAULT_TIMEOUT_MS: u64 = 600_000;

/// Maximum number of targets one generate or verify invocation aggregates.
/// The bound keeps receipts bounded and forces explicit selection on
/// multi-target projects.
pub const MAX_TARGETS: usize = 8;
