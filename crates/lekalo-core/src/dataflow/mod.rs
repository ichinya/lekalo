//! Issue #87: the derived, read-only data-flow report.
//!
//! One independent contract family projecting every classified flow of
//! one project — source subject, ordered path, sink, provenance,
//! confidence, tenant relation, resolved kind, and the sensitive-effect
//! gate decision — plus the first-class unknown list and the aggregated
//! gate verdict. The report is derived output bound to the exact
//! canonical digests of its classification and policy inputs; it is
//! never an input to itself and never feeds validation as a
//! declaration.
//!
//! Hard rules: unknown is never safe. A flow that rests on observed,
//! partial, or declared-only evidence can never satisfy a gate; an
//! unresolvable hop degrades the flow and blocks its gates; an
//! incomplete observed graph blocks gate satisfaction project-wide.
//! The report is metadata-only: findings reference subjects by path
//! and carry fixed bounded detail tokens — never values, source text,
//! physical paths, runtime principals, tokens, secrets, or provider
//! transcripts.
//!
//! Boundaries: gate *decisions* are computed here; runtime redaction
//! and export enforcement stay with #119; policy semantics stay with
//! #120; classification declaration and resolution stay with the
//! classification family.

pub mod analyze;
pub mod diagnostic;
pub mod report;
pub mod types;
pub mod version;

pub use analyze::{analyze, Analysis, Inputs};
pub use report::{Report, ReportWire, Verdict};
pub use report::report_canonical_bytes;
pub use types::{
    BoundedText, Confidence, DataKind, Finding, Flow, Gate, GateReason, GateState, QuestionId,
    Severity, SinkKind, SubjectPath, TenantRelation, UnknownFlow, UnknownReason,
};
pub use version::{
    FAMILY, IDENTITY, IR_IDENTITY, MAX_CANONICAL_BYTES, MAX_FINDINGS, MAX_FLOWS, MAX_OPEN_QUESTIONS,
    MAX_PATH_HOPS, MAX_UNKNOWNS, MODEL_VERSION, SCHEMA_VERSION, VERSION,
};

