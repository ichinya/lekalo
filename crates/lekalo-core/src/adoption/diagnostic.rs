//! Adoption diagnostics routed through the accepted #11 contract.
//!
//! Every adoption failure or finding is one registered `adoption.*` rule
//! (the reviewed 0.8.0 registry union successor) assembled through the
//! shared registry-backed constructor. Echoed tokens are bounded after
//! grammar validation; raw input never scales or shapes the envelope.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject, DataValue, NestedValue, Scalar};
use crate::diagnostics::DiagnosticSet;
use crate::result::{DomainResult, Status};

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
/// mutation lock. The r7 passthrough rode the closed `denied` envelope
/// with the infrastructure in-progress rule the registry admits for
/// `invalid` only — an envelope/rule combination no registry consumer
/// can expect (r7 review, devin F4 + omp F4) — so the family now owns a
/// dedicated denied-allowed busy rule, exactly like `format.locked`.
pub(crate) fn writer_busy_set() -> DiagnosticSet {
    match build("adoption.writer-locked", None, None, DataObject::new()) {
        Ok(diagnostic) => {
            match DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Denied) {
                Ok(set) => set,
                Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
            }
        }
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// A sibling service's interrupted transaction blocks mutation under the
/// shared lock (r8, devin F1/F3 + omp F3): the sibling family's own
/// recovery rule rides the closed passthrough set, emitted at the status
/// its registry entry admits (`versioning.recovery-required`,
/// `format.recovery-required`, and `adoption.recovery-required` are
/// invalid-only; `change.recovery-required` is denied-only), so the
/// envelope and the rule it carries never disagree.
pub(crate) fn sibling_recovery_set(code: &str) -> DiagnosticSet {
    let status = if code == "change.recovery-required" {
        Status::Denied
    } else {
        Status::Invalid
    };
    match build(code, None, None, DataObject::new()) {
        Ok(diagnostic) => match DiagnosticSet::try_from_unsorted(vec![diagnostic], status) {
            Ok(set) => set,
            Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
        },
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// Project one shared-writer refusal onto the adoption family's closed
/// rules (r8): a held lock denies through the dedicated busy rule, a
/// sibling's interrupted transaction refuses through that family's own
/// recovery rule at its admitted status, and the change family's
/// recovery rule keeps its bounded transaction token. Every mutating
/// adoption entry (apply, confirmation, rollback, recovery) refuses
/// through this one projection.
pub(crate) fn shared_writer_refusal(
    refusal: crate::project_writer::MutationRefusal,
) -> DomainResult {
    match refusal {
        crate::project_writer::MutationRefusal::ChangeRecovery { transaction } => {
            DomainResult::denied(
                crate::change::ChangeError::RecoveryRequired { transaction }.diagnostic_set(),
            )
        }
        crate::project_writer::MutationRefusal::LockHeld => DomainResult::denied(writer_busy_set()),
        crate::project_writer::MutationRefusal::SiblingJournal { code } => {
            if code == "change.recovery-required" {
                DomainResult::denied(sibling_recovery_set(code))
            } else {
                DomainResult::invalid(sibling_recovery_set(code))
            }
        }
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
