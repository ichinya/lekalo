//! Issue #107: the in-memory reference evaluator for the basic
//! Lekalo semantics and the Scenario IR.
//!
//! One closed, deterministic, versioned reference evaluation of one
//! validated Scenario IR against the exact pinned compiled IR and the
//! exact pinned invariant-transition attachment: `given` steps
//! materialize typed entity rows, actors, deterministic clocks, and
//! deterministic ID sources in memory; `when` invokes execute as
//! staged all-or-nothing transactions (entry preconditions, ordered
//! assignments, post-write invariant enforcement, rollback within the
//! in-memory state on every failure class, declared event-intent
//! capture, durable-key idempotent replay); `then` assertions observe
//! step outputs, the effect log, and the final state and receive
//! closed verdicts (`pass` / `fail` / `unsupported`).
//!
//! ## Purpose and limits
//!
//! The evaluator is a conformance oracle and golden-behavior
//! definition for adapters — it validates semantics before target
//! generation and lets Node/PHP/Go backends compare their normalized
//! execution against the reference trace. It is not a production
//! runtime, not a database, and it proves nothing about production
//! equivalence; no such claim is made anywhere.
//!
//! ## Determinism
//!
//! Every decision is pure: no filesystem, network, process,
//! wall-clock, randomness, locale, or host access. The clock is the
//! scenario's declared clock (explicit step reference, else the first
//! declared clock, else the documented epoch fallback); IDs derive
//! from SHA-256 of the declared seed. Canonical trace bytes are
//! compact UTF-8 JSON with byte-sorted object keys; the same pinned
//! scenario, IR, and attachment produce byte-identical traces on
//! every host.
//!
//! ## Unsupported is explicit
//!
//! Only the explicitly supported deterministic subset executes:
//! authorization decisions (#25 owns policy semantics), opaque
//! fixture materialization, #66 typed-expression assignments, the
//! #64 filter/sort/pagination grammar beyond exact-match reads,
//! contract-match and fixture-digest verification, and #24
//! concurrency-race schedules each return the fixed `unsupported`
//! outcome with a stable reason token instead of guessed behavior.
//! The closed capability sets below are the exact vocabulary the
//! `unsupported` assertion verifies.
//!
//! ## Boundaries
//!
//! Reference behavior is versioned with the Model/IR: every trace
//! records the exact scenario digest, Model version, IR digest, and
//! attachment revision it interpreted, plus the separate
//! reference-semantics identity. Purely declarative validation (#12)
//! never invokes this module — the core stays usable without an
//! evaluator. Diagnostics reuse the accepted #11 infrastructure
//! rules (`graph.export-limit`, `graph.traversal-limit`); the
//! registry file is unchanged. No CLI command exists in v1: traces
//! run through the library surface and the contract gates.

pub mod canonical;
mod diagnostic;
pub mod execute;
pub mod semantics;
pub mod state;
pub mod trace;
pub mod version;

pub use execute::ReferenceEvaluation;
pub use trace::{
    AssertionRecord, EffectKind, EffectRecord, ErrorToken, GivenRecord, Outcome, ReferenceTrace,
    RowSnapshot, Status, Verdict, WhenRecord,
};
pub use version::{FAMILY, IDENTITY, SCHEMA_VERSION, SEMANTICS_IDENTITY, VERSION};

use crate::scenario::precondition::IdAlgorithm;
use crate::scenario::value::TypedValue;

/// The closed set of capabilities the reference backend supports.
/// Assertions of kind `unsupported` fail against these.
pub const SUPPORTED_CAPABILITIES: &[&str] = &[
    "reference.core/command-transition",
    "reference.core/effect-log",
    "reference.core/given-state",
    "reference.core/id-source",
    "reference.core/idempotency",
    "reference.core/invariants",
    "reference.core/query-read",
    "reference.core/rollback",
];

/// The closed set of capabilities the reference backend knows it does
/// not support. Assertions of kind `unsupported` pass against these.
pub const ABSENT_CAPABILITIES: &[&str] = &[
    "reference.core/authorization",
    "reference.core/contract-match",
    "reference.core/expression",
    "reference.core/fixture",
    "reference.core/fixture-digest",
    "reference.core/job-execution",
    "reference.core/pagination",
    "reference.core/query-filter",
    "reference.core/race-schedule",
];

/// Derive one deterministic ID of a #23 ID source: sequence sources
/// derive `{seed}-{index}`; UUIDv4 sources derive the lowercase
/// hyphenated v4 spelling of SHA-256(`seed:index`) with the RFC 4122
/// version and variant bits pinned. Pure and stable across hosts.
pub fn derived_id(seed: &str, algorithm: IdAlgorithm, index: u64) -> TypedValue {
    semantics::derive_id(seed, algorithm, index)
}

/// The canonical trace bytes of one finished evaluation, or the typed
/// refusal beyond the payload bound.
pub fn trace_bytes(trace: &ReferenceTrace) -> Result<String, crate::diagnostics::DiagnosticSet> {
    canonical::trace_bytes(trace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_sets_are_disjoint_and_sorted() {
        assert!(
            super::SUPPORTED_CAPABILITIES
                .windows(2)
                .all(|pair| pair[0] < pair[1]),
            "supported capabilities are byte-sorted"
        );
        assert!(
            super::ABSENT_CAPABILITIES
                .windows(2)
                .all(|pair| pair[0] < pair[1]),
            "absent capabilities are byte-sorted"
        );
        for capability in super::SUPPORTED_CAPABILITIES {
            assert!(
                !super::ABSENT_CAPABILITIES.contains(capability),
                "{capability} appears in both capability sets"
            );
        }
    }

    #[test]
    fn derived_ids_are_public_and_deterministic() {
        let one = derived_id("seed", IdAlgorithm::Sequence, 2);
        let two = derived_id("seed", IdAlgorithm::Sequence, 2);
        assert_eq!(one, two);
        assert_eq!(one, TypedValue::String("seed-2".to_owned()));
    }
}
