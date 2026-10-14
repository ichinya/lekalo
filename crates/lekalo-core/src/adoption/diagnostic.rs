//! Adoption diagnostics routed through the accepted #11 contract.
//!
//! Every adoption failure or finding is one registered `adoption.*` rule
//! (the reviewed 0.8.0 registry union successor) assembled through the
//! shared registry-backed constructor. Echoed tokens are bounded after
//! grammar validation; raw input never scales or shapes the envelope.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject, DataValue, NestedValue, Scalar};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

/// One bounded `{ name: token }` record for `records`-typed fields.
fn named_record(name: &str, value: &str) -> std::collections::BTreeMap<String, NestedValue> {
    let mut record = std::collections::BTreeMap::new();
    record.insert(
        name.to_owned(),
        NestedValue::Scalar(Scalar::Token(value.to_owned())),
    );
    record
}

/// The shared cross-service writer serialization refusal (r6, findings
/// devin F2 + omp F2): another service's writer holds the project-wide
/// mutation lock, or a sibling's interrupted transaction blocks mutation
/// under it. The infrastructure in-progress rule is not an adoption rule,
/// so it rides the closed passthrough set at the family's denied class.
pub(crate) fn writer_busy_set() -> DiagnosticSet {
    match build(
        "versioning.migration-in-progress",
        None,
        None,
        DataObject::new(),
    ) {
        Ok(diagnostic) => match DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Invalid)
        {
            Ok(set) => set,
            Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
        },
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(id: &str, status: Status, data: DataObject) -> DiagnosticSet {
    match build(id, None, None, data) {
        Ok(diagnostic) => match DiagnosticSet::try_from_unsorted(vec![diagnostic], status) {
            Ok(set) => set,
            Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
        },
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The registered `adoption.input-invalid` / LEK-ADOPT-001: a closed input is malformed,
/// duplicate, foreign-scoped, or over limit.
pub(crate) fn input_invalid_set(field: &str, reason: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("field".to_owned(), token_value(field));
    data.insert("reason".to_owned(), token_value(reason));
    one("adoption.input-invalid", Status::Invalid, data)
}

/// `adoption.version-unsupported` / LEK-ADOPT-002.
pub(crate) fn version_unsupported_set(version: &str, supported: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("version".to_owned(), token_value(version));
    data.insert("supported".to_owned(), token_value(supported));
    one(
        "adoption.version-unsupported",
        Status::UnsupportedVersion,
        data,
    )
}

/// `adoption.scope-refused` / LEK-ADOPT-003.
pub(crate) fn scope_refused_set(scope: &str, reason: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("scope".to_owned(), token_value(scope));
    data.insert("reason".to_owned(), token_value(reason));
    one("adoption.scope-refused", Status::Denied, data)
}

/// `adoption.plan-stale` / LEK-ADOPT-004: a pinned input changed. The
/// named refs are the changed members, never their private contents.
pub(crate) fn plan_stale_set(plan: &str, changed: &[String]) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("plan".to_owned(), token_value(plan));
    data.insert(
        "inputs".to_owned(),
        DataValue::Records(
            changed
                .iter()
                .map(|name| named_record("input", name))
                .collect(),
        ),
    );
    one("adoption.plan-stale", Status::Denied, data)
}

/// `adoption.confirmation-required` / LEK-ADOPT-005.
pub(crate) fn confirmation_required_set(plan: &str, reason: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("plan".to_owned(), token_value(plan));
    data.insert("reason".to_owned(), token_value(reason));
    one("adoption.confirmation-required", Status::Denied, data)
}

/// One non-blocking `adoption.evidence-incomplete` / LEK-ADOPT-006
/// diagnostic; the caller decides the envelope status it rides.
pub(crate) fn evidence_incomplete(
    check: &str,
    reason: &str,
) -> Option<crate::diagnostics::Diagnostic> {
    let mut data = DataObject::new();
    data.insert("check".to_owned(), token_value(check));
    data.insert("reason".to_owned(), token_value(reason));
    build("adoption.evidence-incomplete", None, None, data).ok()
}

/// The valid envelope carrying only evidence-incomplete findings.
pub(crate) fn evidence_incomplete_set(check: &str, reason: &str) -> DiagnosticSet {
    match evidence_incomplete(check, reason) {
        Some(diagnostic) => DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Valid)
            .unwrap_or_else(|_| crate::result::fallback_set()),
        None => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// `adoption.verification-failed` / LEK-ADOPT-007.
pub(crate) fn verification_failed_set(check: &str, receipt: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("check".to_owned(), token_value(check));
    data.insert("receipt".to_owned(), token_value(receipt));
    one("adoption.verification-failed", Status::Invalid, data)
}

/// `adoption.recovery-required` / LEK-ADOPT-008.
pub(crate) fn recovery_required_set(plan: &str, phase: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("plan".to_owned(), token_value(plan));
    data.insert("phase".to_owned(), token_value(phase));
    one("adoption.recovery-required", Status::Invalid, data)
}

/// `adoption.rollback-conflict` / LEK-ADOPT-009.
pub(crate) fn rollback_conflict_set(receipt: &str, paths: &[String]) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("receipt".to_owned(), token_value(receipt));
    data.insert(
        "paths".to_owned(),
        DataValue::Records(
            paths
                .iter()
                .map(|path| named_record("path", path))
                .collect(),
        ),
    );
    one("adoption.rollback-conflict", Status::Denied, data)
}
