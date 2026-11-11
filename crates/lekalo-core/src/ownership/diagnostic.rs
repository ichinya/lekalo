use crate::{
    diagnostics::{
        types::{token_value, DataObject},
        DiagnosticSet,
    },
    result::{DomainResult, Status},
};

pub(crate) fn identity(reason: &str) -> (&'static str, Status) {
    if reason.contains("version") {
        ("ownership.version-unsupported", Status::UnsupportedVersion)
    } else if reason.contains("writer-locked") {
        ("ownership.writer-locked", Status::Denied)
    } else if reason.contains("recovery-required") {
        ("ownership.recovery-required", Status::Denied)
    } else if reason.contains("rollback-conflict") || reason.contains("recovery-conflict") {
        ("ownership.rollback-conflict", Status::Denied)
    } else if reason.contains("equivalence-failed") {
        ("ownership.verification-failed", Status::Denied)
    } else if reason.contains("execution-")
        || reason.contains("tool-unavailable")
        || reason.contains("tool-unreadable")
    {
        ("ownership.execution-unavailable", Status::Invalid)
    } else if reason.contains("stale") || reason.contains("generated-drift") {
        ("ownership.plan-stale", Status::Denied)
    } else if reason.contains("unconfirmed") {
        ("ownership.binding-unconfirmed", Status::Denied)
    } else if reason.contains("scope") || reason.contains("shared-file") {
        ("ownership.scope-refused", Status::Denied)
    } else if reason.contains("conflict")
        || reason.contains("unclaimed")
        || reason.contains("transition-service")
    {
        ("ownership.ownership-conflict", Status::Denied)
    } else {
        ("ownership.input-invalid", Status::Invalid)
    }
}

pub(crate) fn set(reason: &str) -> DiagnosticSet {
    let (id, status) = identity(reason);
    let data: DataObject = [("reason".to_owned(), token_value(reason))]
        .into_iter()
        .collect();
    let diagnostic = crate::diagnostics::normalize::build(id, None, None, data)
        .expect("registered ownership diagnostic");
    DiagnosticSet::try_from_unsorted(vec![diagnostic], status).expect("ownership rule status")
}

pub(crate) fn failure(reason: &str) -> DomainResult {
    let (_, status) = identity(reason);
    match status {
        Status::Denied => DomainResult::denied(set(reason)),
        Status::UnsupportedVersion => DomainResult::unsupported_version(set(reason)),
        _ => DomainResult::invalid(set(reason)),
    }
}
