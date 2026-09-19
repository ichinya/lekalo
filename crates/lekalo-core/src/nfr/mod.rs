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

pub mod version;

pub use version::{
    EVIDENCE_FAMILY, EVIDENCE_IDENTITY, EVIDENCE_SCHEMA_VERSION, EVIDENCE_VERSION, FAMILY,
    IDENTITY, MAX_CAPABILITIES, MAX_CONSTRAINTS, MAX_DOC_BYTES, MAX_ENVIRONMENTS, MAX_EXPORT_BYTES,
    MAX_MEASUREMENTS, MAX_OPEN_QUESTIONS, MAX_RESULTS, REPORT_FAMILY, REPORT_IDENTITY,
    REPORT_SCHEMA_VERSION, REPORT_VERSION, SCHEMA_VERSION, VERSION,
};
