//! Task-links diagnostics routed through the accepted #11 contract.
//!
//! Every attachment or evidence failure is one registered
//! `tasklink.*` rule assembled through the shared registry-backed
//! constructor and finalized into a normalized [`DiagnosticSet`].
//! Every echoed token is bounded before construction: rule data carries
//! only fixed detail tags and bounded subject symbols — never raw
//! input, paths, provider tokens, digests, or attacker-controlled text.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{bound_token, token_value, DataObject};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for fatal attachment/input violations.
pub(crate) const INPUT_INVALID: &str = "tasklink.relation-identity-invalid";
/// The registered rule for an incomplete or unbound decision.
pub(crate) const DECISION_INCOMPLETE: &str = "tasklink.decision-incomplete";
/// The registered rule for an outbound effect without an admitted decision.
pub(crate) const FANOUT_NOT_ADMITTED: &str = "tasklink.fanout-not-admitted";
/// The registered rule for a broken delivery/outbox declaration.
pub(crate) const DELIVERY_CONTRACT_INVALID: &str = "tasklink.delivery-contract-invalid";
/// The registered rule for an invalid conflict resolution declaration.
#[allow(dead_code)]
pub(crate) const CONFLICT_RESOLUTION_INVALID: &str = "tasklink.conflict-resolution-invalid";
/// The registered rule for history/ownership violations.
pub(crate) const HISTORY_OR_OWNERSHIP_VIOLATION: &str = "tasklink.history-or-ownership-violation";
/// The registered rule for a required guarantee the selected adapter
/// cannot supply.
pub(crate) const REQUIRED_GUARANTEE_UNSUPPORTED: &str = "tasklink.required-guarantee-unsupported";
/// The registered rule for stale, fabricated, or unadmitted evidence.
pub(crate) const EVIDENCE_INVALID: &str = "tasklink.evidence-invalid";

/// Why one task-links operation failed: the registered rule plus a
/// fixed detail tag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Failure {
    /// The registered rule id.
    pub rule: &'static str,
    /// The fixed classification detail token (no subject echo).
    pub detail: &'static str,
    /// The optional bounded subject echo.
    pub subject: Option<String>,
}

impl Failure {
    /// Map a declaration failure to its stable envelope status.
    ///
    /// Required-guarantee and unknown/unsupported capability gaps stay
    /// `invalid` under the task-links owner: they invalidate the
    /// attachment admission instead of widening the envelope. A
    /// schema-version identity mismatch is the accepted exit-5 class:
    /// the document is well-formed but its contract version is outside
    /// the accepted registry.
    pub fn status(&self) -> Status {
        match (self.rule, self.detail) {
            (REQUIRED_GUARANTEE_UNSUPPORTED, _) => Status::Unsupported,
            (INPUT_INVALID, "schema-version-unsupported") => Status::UnsupportedVersion,
            _ => Status::Invalid,
        }
    }
}

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(id: &str, data: DataObject) -> Result<Diagnostic, BuildError> {
    build(id, None, None, data)
}

/// Build a validated `invalid` set or collapse to the invariant set.
pub(crate) fn set_for(failure: &Failure, status: Status) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(failure.detail));
    if let Some(subject) = &failure.subject {
        data.insert("subject".to_owned(), token_value(subject));
    }
    match one(failure.rule, data) {
        Ok(diagnostic) => DiagnosticSet::try_from_unsorted(vec![diagnostic], status)
            .unwrap_or_else(|_| singleton_set("diagnostics.registry-invalid")),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one wire normalization violation. The detail tag
/// is a fixed classification token with no subject echo.
pub(crate) fn input_invalid(detail: &'static str) -> Failure {
    Failure {
        rule: INPUT_INVALID,
        detail,
        subject: None,
    }
}

/// The typed failure for one semantic rule with a fixed detail tag and
/// an optional bounded subject echo.
pub(crate) fn rule_invalid(
    rule: &'static str,
    detail: &'static str,
    subject: Option<&str>,
) -> Failure {
    Failure {
        rule,
        detail,
        subject: subject.map(bound_token),
    }
}

/// Project a failure into the stable [`DomainResult`], mapping a
/// version-identity mismatch onto the accepted exit-5 class.
pub fn into_result(failure: &Failure) -> crate::result::DomainResult {
    let status = failure.status();
    let set = set_for(failure, status);
    match status {
        Status::UnsupportedVersion => {
            crate::result::DomainResult::UnsupportedVersion { diagnostics: set }
        }
        Status::Unsupported => {
            crate::result::DomainResult::UnsupportedOperation { diagnostics: set }
        }
        _ => crate::result::DomainResult::invalid(set),
    }
}
