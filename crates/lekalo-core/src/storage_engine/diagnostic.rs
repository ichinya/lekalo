//! Storage-engine diagnostics routed through the accepted #11
//! contract (issue #69).
//!
//! Every attachment failure is one registered `storage-engine.*` rule
//! assembled through the shared registry-backed constructor and
//! finalized into a normalized [`DiagnosticSet`]. Every echoed token
//! is bounded before construction: rule data carries only fixed detail
//! tags and declared limits — never raw input, paths, tokens, digests,
//! or attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal construction input violations.
pub(crate) const INPUT_INVALID: &str = "storage-engine.input-invalid";
/// The registered rule for an incoherent declared profile.
pub(crate) const PROFILE_INVALID: &str = "storage-engine.profile-invalid";
/// The registered rule for an unsupported engine version pin.
pub(crate) const VERSION_UNSUPPORTED: &str = "storage-engine.version-unsupported";
/// The registered rule for an unmappable domain declaration. Wired by
/// the DDL renderer (plan steps S4/S7).
#[allow(dead_code)]
pub(crate) const MAPPING_INVALID: &str = "storage-engine.mapping-invalid";
/// The registered rule for a fact with no deterministic rendering.
/// Wired by the DDL renderer (plan step S4).
#[allow(dead_code)]
pub(crate) const RENDER_UNSUPPORTED: &str = "storage-engine.render-unsupported";
/// The registered rule for refused introspection evidence. Wired by the
/// introspection normalizer (plan step S5).
#[allow(dead_code)]
pub(crate) const INTROSPECTION_INVALID: &str = "storage-engine.introspection-invalid";
/// The registered rule for an impossible drift comparison. Wired by the
/// drift comparison (plan step S5).
#[allow(dead_code)]
pub(crate) const DRIFT_INVALID: &str = "storage-engine.drift-invalid";
/// The registered rule for an invalid migration plan. Wired by the
/// migration planner (plan step S7).
#[allow(dead_code)]
pub(crate) const MIGRATION_INVALID: &str = "storage-engine.migration-invalid";
/// The registered rule for the destructive-step gate. Wired by the
/// migration planner (plan step S7).
#[allow(dead_code)]
pub(crate) const MIGRATION_GATED: &str = "storage-engine.migration-gated";
/// The registered rule for a missing declared capability. Wired by the
/// capability projection (plan step S4).
#[allow(dead_code)]
pub(crate) const CAPABILITY_MISSING: &str = "storage-engine.capability-missing";
/// The registered rule for an invalid test lifecycle.
pub(crate) const LIFECYCLE_INVALID: &str = "storage-engine.lifecycle-invalid";
/// The registered rule for an extension outside the allow-list. Wired
/// by the drift comparison (plan step S5).
#[allow(dead_code)]
pub(crate) const EXTENSION_UNSUPPORTED: &str = "storage-engine.extension-unsupported";
/// The registered rule for a failed conformance check. Wired by the
/// conformance battery (plan step S8).
#[allow(dead_code)]
pub(crate) const CONFORMANCE_FAILED: &str = "storage-engine.conformance-failed";
/// The registered rule for the canonical payload bound.
pub(crate) const EXPORT_LIMIT: &str = "storage-engine.export-limit";

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(id: &str, data: DataObject) -> Result<Diagnostic, BuildError> {
    build(id, None, None, data)
}

/// Build a validated `invalid` set or collapse to the invariant set.
pub(crate) fn invalid_set(diagnostics: Vec<Diagnostic>) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, Status::Invalid)
        .unwrap_or_else(|_| singleton_set("diagnostics.registry-invalid"))
}

/// One bounded token data value.
fn token(text: &str) -> DataValue {
    token_value(text)
}

/// The fatal set for one wire normalization violation. The detail tag
/// is a fixed classification token with no subject echo.
pub(crate) fn input_invalid(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(INPUT_INVALID, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one semantic record violation with a fixed detail
/// tag and an optional bounded subject echo. `rule` selects the
/// registered profile, mapping, lifecycle, or introspection rule.
pub(crate) fn rule_invalid(rule: &str, detail: &str, subject: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    if let Some(subject) = subject {
        data.insert("subject".to_owned(), token(&bounded(subject)));
    }
    match one(rule, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The typed refusal set for a CLI-read I/O failure. The detail tag
/// is a fixed classification token with no subject echo.
pub(crate) fn io_invalid(detail: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(INPUT_INVALID, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The typed unsupported set for one refusal rule (version, render,
/// capability). The rule must admit the unsupported status.
pub(crate) fn unsupported_version_set(detail: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(VERSION_UNSUPPORTED, data) {
        Ok(diagnostic) => {
            match DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Unsupported) {
                Ok(set) => set,
                Err(_) => singleton_set("diagnostics.registry-invalid"),
            }
        }
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for the over-bound canonical payload.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("canonical-bytes"));
    data.insert("limit".to_owned(), token(&format!("bytes={bytes}")));
    match one(EXPORT_LIMIT, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// Bound an echoed identifier to the diagnostic token bound.
pub(crate) fn bounded(text: &str) -> String {
    bound_token(text)
}
