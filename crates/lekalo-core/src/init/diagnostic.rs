//! Adoption diagnostics routed through the accepted #11 contract.
//!
//! Every `lekalo init --adopt` failure is one registered `init.*` rule
//! (`LEK-INIT-NNN`, diagnostic registry 1.13.0) assembled through the
//! shared registry-backed constructor. Policy refusals (no-overwrite
//! conflicts, ambiguous roots, underivable project ids) are `denied`;
//! physical write and rollback failures are `invalid`. Severity never
//! computes the exit; [`DomainResult`] alone owns status, stream, and exit.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue, Scalar};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

/// A planned write path exists with different content.
pub const ADOPT_CONFLICT: &str = "init.adopt-conflict";
/// More than one candidate adoption root requires explicit resolution.
pub const ADOPT_AMBIGUOUS_ROOT: &str = "init.adopt-ambiguous-root";
/// No canonical project id is derivable and none was passed explicitly.
pub const ADOPT_ID_REQUIRED: &str = "init.adopt-id-required";
/// A planned write failed after earlier writes succeeded (rolled back).
pub const ADOPT_WRITE_FAILED: &str = "init.adopt-write-failed";
/// The rollback itself failed and created files remain.
pub const ADOPT_RECOVERY_REQUIRED: &str = "init.adopt-recovery-required";

/// The registry-invariant rule id used when the registry itself is broken.
const REGISTRY_INVALID: &str = "diagnostics.registry-invalid";

/// Finalize one registered diagnostic; a registry failure collapses to the
/// registry-invariant set (double developer fault) instead of panicking.
fn one(id: &str, data: DataObject) -> Result<crate::diagnostics::Diagnostic, DiagnosticSet> {
    build(id, None, None, data).map_err(|_| crate::result::singleton_set(REGISTRY_INVALID))
}

/// Sort, dedup, and finalize one set, collapsing on any validation error.
fn set_of(status: Status, diagnostics: Vec<crate::diagnostics::Diagnostic>) -> DiagnosticSet {
    let mut diagnostics = diagnostics;
    diagnostics.sort_by(|left, right| left.id().cmp(right.id()));
    diagnostics.dedup();
    crate::diagnostics::DiagnosticSet::try_from_unsorted(diagnostics, status)
        .unwrap_or_else(|_| crate::result::fallback_set())
}

/// The `denied` set for every no-overwrite conflict, one per path.
pub fn conflict_set(paths: &[String]) -> DiagnosticSet {
    let mut diagnostics = Vec::with_capacity(paths.len());
    for path in paths {
        let mut data = DataObject::new();
        data.insert("path".to_owned(), token_value(&bound_token(path)));
        match one(ADOPT_CONFLICT, data) {
            Ok(diagnostic) => diagnostics.push(diagnostic),
            Err(fallback) => return fallback,
        }
    }
    set_of(Status::Denied, diagnostics)
}

/// The `denied` set for an ambiguous adoption root.
pub fn ambiguous_root_set(candidates: &[String]) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert(
        "candidates".to_owned(),
        DataValue::List(
            candidates
                .iter()
                .map(|candidate| Scalar::Token(bound_token(candidate)))
                .collect(),
        ),
    );
    match one(ADOPT_AMBIGUOUS_ROOT, data) {
        Ok(diagnostic) => set_of(Status::Denied, vec![diagnostic]),
        Err(fallback) => fallback,
    }
}

/// The `denied` set for an underivable project id without an explicit one.
pub fn id_required_set(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    match one(ADOPT_ID_REQUIRED, data) {
        Ok(diagnostic) => set_of(Status::Denied, vec![diagnostic]),
        Err(fallback) => fallback,
    }
}

/// The `invalid` set for a failed planned write (rollback already done).
pub fn write_failed_set(path: &str, detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("path".to_owned(), token_value(&bound_token(path)));
    data.insert("detail".to_owned(), token_value(detail));
    match one(ADOPT_WRITE_FAILED, data) {
        Ok(diagnostic) => set_of(Status::Invalid, vec![diagnostic]),
        Err(fallback) => fallback,
    }
}

/// The `invalid` set for an incomplete rollback; lists what remains.
pub fn recovery_required_set(paths: &[String]) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert(
        "paths".to_owned(),
        DataValue::List(
            paths
                .iter()
                .map(|path| Scalar::Token(bound_token(path)))
                .collect(),
        ),
    );
    match one(ADOPT_RECOVERY_REQUIRED, data) {
        Ok(diagnostic) => set_of(Status::Invalid, vec![diagnostic]),
        Err(fallback) => fallback,
    }
}
