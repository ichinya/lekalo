//! Issue #85: non-functional requirements as versioned semantic
//! constraints bound to measurable evidence.
//!
//! Three independent, closed, versioned contracts form the family: the
//! constraint attachment — `lekalo/nfr/v0.4.0`, identity
//! `dev.lekalo.nfr@0.4.0` — declares constraints over semantic scopes
//! with closed kind vocabularies partitioned by dimension (`runtime`
//! versus `ai-budget`); the volatile measured-evidence set —
//! `lekalo/nfr-evidence/v0.4.0` — carries results pinned to the exact
//! constraint id and revision, keyed by exact environment identity;
//! the derived read-only report — `lekalo/nfr-report/v0.4.0` —
//! resolves every constraint into per-environment rows, first-class
//! statuses (`unverified` is computed, never declared), and a gate
//! verdict.
//!
//! Authority and boundaries: NFRs are Lekalo-owned semantic data,
//! strictly separated from functional invariants (the
//! invariant-transition family) and from scenario assertions (a
//! measurement can never satisfy an assertion). Evidence arrives from
//! the outside; this module never runs a benchmark, never invents a
//! value, and never merges results across different environments. A
//! declaration without measurement is reported unverified, never
//! silently proven.

pub mod constraint;
pub mod diagnostic;
pub mod evidence;
pub mod id;
pub mod validate;
pub mod version;

mod canonical;
pub mod environment;
mod json;
mod wire;

pub use constraint::{
    AiBudgetKind, CapabilityRequirement, Comparator, Constraint, Dimension, Enforcement, Kind,
    Measurement, Method, Mode, OpenQuestion, Percentile, Requirement, RequirementValue, Resource,
    RuntimeKind, Scope, ScopeKind, Support, Unit, Validity, WindowUnit,
};
pub use environment::{Environment, OwnerRef, Token};
pub use evidence::{EvidenceResult, EvidenceSet, MeasuredValue, ResultStatus, ScenarioRef};
pub use id::{ConstraintId, Decimal, IsoDate};
pub use version::{
    EVIDENCE_FAMILY, EVIDENCE_IDENTITY, EVIDENCE_SCHEMA_VERSION, EVIDENCE_VERSION, FAMILY,
    IDENTITY, MAX_CAPABILITIES, MAX_CONSTRAINTS, MAX_DOC_BYTES, MAX_ENVIRONMENTS, MAX_EXPORT_BYTES,
    MAX_MEASUREMENTS, MAX_OPEN_QUESTIONS, MAX_RESULTS, REPORT_FAMILY, REPORT_IDENTITY,
    REPORT_SCHEMA_VERSION, REPORT_VERSION, SCHEMA_VERSION, VERSION,
};

use crate::diagnostics::DiagnosticSet;

pub use wire::NfrAttachment;

/// The canonical export of one attachment: compact JSON with
/// byte-sorted keys, canonical collections, and no trailing LF, or the
/// export-limit refusal.
pub fn attachment_canonical_bytes(attachment: &NfrAttachment) -> Result<String, DiagnosticSet> {
    let bytes = canonical::canonical_value_bytes(&attachment.wire());
    canonical::check_export_bound(&bytes)?;
    Ok(bytes)
}

/// The canonical export of one evidence set.
pub fn evidence_canonical_bytes(evidence: &EvidenceSet) -> Result<String, DiagnosticSet> {
    let bytes = canonical::canonical_value_bytes(&evidence.evidence_wire());
    canonical::check_export_bound(&bytes)?;
    Ok(bytes)
}

/// The exact `sha256:<64 lowercase hex>` digest of the canonical
/// attachment bytes.
pub fn attachment_digest(attachment: &NfrAttachment) -> Result<String, DiagnosticSet> {
    Ok(canonical::sha256_hex(
        attachment_canonical_bytes(attachment)?.as_bytes(),
    ))
}
