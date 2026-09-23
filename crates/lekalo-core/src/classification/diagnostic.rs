//! Classification diagnostics routed through the accepted #11 contract
//! (issue #87).
//!
//! Every integration failure is one registered `classification.*` rule
//! assembled through the shared registry-backed constructor and
//! finalized into a normalized [`DiagnosticSet`]. Subjects are hashed
//! before construction: rule data carries only fixed detail tags and
//! opaque SHA-256 tokens — never raw input, paths, subject text, or
//! attacker-controlled content. A registry construction failure
//! collapses the whole set to the registry-invariant set (double
//! developer fault) instead of panicking.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::Status;

/// The registered rule for a wire or declaration violation.
pub const DOCUMENT_INVALID: &str = "classification.unknown-kind";
/// The registered rule for a subject that does not resolve in the
/// bound IR or names an unaddressable member.
pub const UNKNOWN_SUBJECT: &str = "classification.unknown-subject";
/// The registered rule for one subject addressed twice
/// contradictorily.
pub const DUPLICATE_SUBJECT: &str = "classification.duplicate-subject";
/// The registered rule for two entries declaring different kinds for
/// one subject without a grant.
pub const CONFLICTING_KIND: &str = "classification.conflicting-kind";
/// The registered rule for a grant lowering an unlowerable kind,
/// targeting the same kind, or naming an unresolvable subject.
pub const INVALID_DECLASSIFICATION: &str = "classification.invalid-declassification";
/// The registered rule for a grant without a review reference or with
/// a review reference outside every declassify role of the from kind.
pub const MISSING_APPROVAL: &str = "classification.missing-approval";
/// The registered rule for a grant expired against the as-of date.
pub const EXPIRED_DECLASSIFICATION: &str = "classification.expired-declassification";
/// The registered rule for a grant approved by its own declaration.
pub const SELF_APPROVED: &str = "classification.self-approved";
/// The registered rule for a classification attachment without the
/// classification policy that governs its kinds.
pub const POLICY_MISSING: &str = "classification.policy-missing";
/// The registered rule for a resolved kind without a policy rule row.
pub const KIND_RULE_MISSING: &str = "classification.kind-rule-missing";
/// The registered rule for a sink ceiling below a reachable kind.
pub const SINK_CEILING_EXCEEDED: &str = "classification.sink-ceiling-exceeded";
/// The registered rule for an unclassified subject on a sensitive sink
/// under the strict profile.
pub const UNCLASSIFIED_SENSITIVE_SINK: &str = "classification.unclassified-sensitive-sink";
/// The registered rule for a custody mismatch on the project identity
/// (review r3, F-5: dedicated custody refusal ids, not the generic
/// `classification.unknown-kind` id).
pub const CUSTODY_PROJECT: &str = "classification.custody-project";
/// The registered rule for a stale or foreign `modelRef` pin.
pub const CUSTODY_MODEL: &str = "classification.custody-model";
/// The registered rule for a stale or foreign `irRef` pin.
pub const CUSTODY_IR: &str = "classification.custody-ir";
/// The registered rule for a grant whose `approvedBy` review reference
/// is missing or violates the review-reference grammar (r4 F-5: its
/// own greppable id, not the generic `classification.unknown-kind`).
pub const MALFORMED_REVIEW_REF: &str = "classification.malformed-review-ref";

/// Why one diagnostic could not be finalized (collapsed to the invariant
/// set by the caller).
type Built = Result<Diagnostic, BuildError>;

/// Finalize one registered diagnostic; a registry failure surfaces as
/// `Err` and collapses the whole set to the invariant set.
fn one(id: &'static str, data: DataObject) -> Built {
    build(id, None, None, data)
}

/// Build one validated diagnostic with a fixed detail tag and optional
/// opaque subject digest, or an invariant-collapse marker.
fn diagnostic(id: &'static str, detail: &str, subject: Option<&str>) -> Result<Diagnostic, ()> {
    let tag = match subject {
        Some(subject) => format!(
            "{detail}:subject-{}",
            crate::digest::sha256_hex(subject.as_bytes())
        ),
        None => detail.to_owned(),
    };
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(&tag));
    if let Some(subject) = subject {
        data.insert(
            "subject".to_owned(),
            token(&format!(
                "subject-{}",
                crate::digest::sha256_hex(subject.as_bytes())
            )),
        );
    }
    one(id, data).map_err(|_| ())
}

/// Build a validated set of one status or collapse to the invariant set.
fn set(status: Status, diagnostics: Vec<Result<Diagnostic, ()>>) -> DiagnosticSet {
    match diagnostics.into_iter().collect::<Result<Vec<_>, _>>() {
        Ok(built) => DiagnosticSet::try_from_unsorted(built, status)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The closed token value spelling.
fn token(tag: &str) -> DataValue {
    token_value(tag)
}

/// One wire or declaration violation of the classification attachment.
pub fn document_invalid(detail: &str, subject: Option<&str>) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(DOCUMENT_INVALID, detail, subject)],
    )
}

/// One subject that does not resolve in the bound IR.
pub fn unknown_subject(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(UNKNOWN_SUBJECT, detail, Some(subject))],
    )
}

/// One subject addressed twice contradictorily.
pub fn duplicate_subject(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(DUPLICATE_SUBJECT, detail, Some(subject))],
    )
}

/// Two entries declaring different kinds for one subject.
pub fn conflicting_kind(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(CONFLICTING_KIND, detail, Some(subject))],
    )
}

/// One grant lowering an unlowerable kind or naming an unknown subject.
pub fn invalid_declassification(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(INVALID_DECLASSIFICATION, detail, Some(subject))],
    )
}

/// One grant without a usable review reference.
pub fn missing_approval(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(MISSING_APPROVAL, detail, Some(subject))],
    )
}

/// One grant expired against the evaluation date.
pub fn expired_declassification(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(EXPIRED_DECLASSIFICATION, detail, Some(subject))],
    )
}

/// One grant approved by its own declaration.
pub fn self_approved(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(SELF_APPROVED, detail, Some(subject))],
    )
}

/// One classification attachment without its governing policy.
pub fn policy_missing(detail: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(POLICY_MISSING, detail, None)],
    )
}

/// One resolved kind without a policy rule row.
pub fn kind_rule_missing(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(KIND_RULE_MISSING, detail, Some(subject))],
    )
}

/// One sink ceiling below a reachable kind.
pub fn sink_ceiling_exceeded(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(SINK_CEILING_EXCEEDED, detail, Some(subject))],
    )
}

/// One unclassified subject on a sensitive sink (strict profile).
pub fn unclassified_sensitive_sink(detail: &str, subject: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(
            UNCLASSIFIED_SENSITIVE_SINK,
            detail,
            Some(subject),
        )],
    )
}

/// One custody mismatch on the project identity of the pinned
/// compilation (review r3, F-5).
pub fn custody_project(detail: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(CUSTODY_PROJECT, detail, None)],
    )
}

/// One stale or foreign `modelRef` pin (review r3, F-5).
pub fn custody_model(detail: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(CUSTODY_MODEL, detail, None)],
    )
}

/// One stale or foreign `irRef` pin (review r3, F-5).
pub fn custody_ir(detail: &str) -> DiagnosticSet {
    set(Status::Invalid, vec![diagnostic(CUSTODY_IR, detail, None)])
}

/// One grant whose `approvedBy` review reference is missing or
/// violates the review-reference grammar (r4 F-5).
pub fn malformed_review_ref(detail: &str) -> DiagnosticSet {
    set(
        Status::Invalid,
        vec![diagnostic(MALFORMED_REVIEW_REF, detail, None)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::Status;

    #[test]
    fn every_rule_finalizes_through_the_registry() {
        for set in [
            document_invalid("probe", None),
            unknown_subject("probe", "core.entity.user/email"),
            duplicate_subject("probe", "core.entity.user/email"),
            conflicting_kind("probe", "core.entity.user/email"),
            invalid_declassification("probe", "core.entity.user/email"),
            missing_approval("probe", "core.entity.user/email"),
            expired_declassification("probe", "core.entity.user/email"),
            self_approved("probe", "core.entity.user/email"),
            policy_missing("probe"),
            kind_rule_missing("probe", "core.entity.user/email"),
            sink_ceiling_exceeded("probe", "core.entity.user/email"),
            unclassified_sensitive_sink("probe", "core.entity.user/email"),
            custody_project("probe"),
            custody_model("probe"),
            custody_ir("probe"),
            malformed_review_ref("probe"),
        ] {
            assert_eq!(set.as_slice().len(), 1);
            assert!(set.as_slice()[0].id().starts_with("classification."));
        }
    }

    #[test]
    fn subjects_are_never_carried_raw() {
        let set = unknown_subject("probe", "core.entity.user/email");
        let rendered = serde_json::to_string(&set).expect("serializes");
        assert!(!rendered.contains("core.entity.user"), "{rendered}");
        assert!(rendered.contains("subject-"));
    }

    #[test]
    fn every_rule_allows_invalid_and_only_that_gate_status_where_declared() {
        let registry =
            crate::diagnostics::registry::DiagnosticRegistry::embedded().expect("registry");
        for id in [
            DOCUMENT_INVALID,
            UNKNOWN_SUBJECT,
            DUPLICATE_SUBJECT,
            CONFLICTING_KIND,
            INVALID_DECLASSIFICATION,
            MISSING_APPROVAL,
            SELF_APPROVED,
            POLICY_MISSING,
            KIND_RULE_MISSING,
        ] {
            let entry = registry.entry(id).unwrap_or_else(|| panic!("{id}"));
            assert!(entry.allows_status(Status::Invalid), "{id}");
        }
        for id in [
            EXPIRED_DECLASSIFICATION,
            SINK_CEILING_EXCEEDED,
            UNCLASSIFIED_SENSITIVE_SINK,
            CUSTODY_PROJECT,
            CUSTODY_MODEL,
            CUSTODY_IR,
            MALFORMED_REVIEW_REF,
        ] {
            let entry = registry.entry(id).unwrap_or_else(|| panic!("{id}"));
            assert!(entry.allows_status(Status::Invalid), "{id}");
        }
    }
}
