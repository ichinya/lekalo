//! Diagnostics routed through the accepted #11 contract (issue #51).
//!
//! Every composition failure is one registered `worklog-changelog.*`
//! rule of the 0.8.0 successor registry assembled through the shared
//! registry-backed constructor and finalized into a normalized
//! [`DiagnosticSet`]. Rule data carries only fixed detail tokens —
//! never raw input, paths, semantic ids beyond the closed tokens, or
//! attacker-controlled content. A registry construction failure
//! collapses the whole set to the registry-invariant set (double
//! developer fault) instead of panicking.

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{singleton_set, Status};

/// The registered rule for a malformed composition document.
pub const INPUT_INVALID: &str = "worklog-changelog.input-invalid";
/// The registered rule for merged or implicitly enabled effects.
pub const EFFECTS_COUPLED: &str = "worklog-changelog.effects-coupled";
/// The registered rule for an unbindable approval.
pub const APPROVAL_UNBOUND: &str = "worklog-changelog.approval-unbound";
/// The registered rule for a mutable published projection.
pub const SNAPSHOT_MUTABLE: &str = "worklog-changelog.snapshot-mutable";
/// The registered rule for an unsafe public projection.
pub const PUBLIC_DATA_UNSAFE: &str = "worklog-changelog.public-data-unsafe";
/// The registered rule for a colliding or unbound replay scope.
pub const REPLAY_SCOPE_INVALID: &str = "worklog-changelog.replay-scope-invalid";
/// The registered rule for incomplete admission evidence.
pub const EVIDENCE_INCOMPLETE: &str = "worklog-changelog.evidence-incomplete";

/// Why one diagnostic could not be finalized (collapsed to the
/// invariant set by the caller).
type Built = Result<Diagnostic, BuildError>;

/// Build one validated diagnostic with a fixed detail token, or an
/// invariant-collapse marker.
fn one(id: &str, detail: &str) -> Built {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    build(id, None, None, data)
}

/// One bounded token data value.
fn token(text: &str) -> DataValue {
    token_value(text)
}

/// Build a validated set of one status or collapse to the invariant
/// set.
fn set(status: Status, diagnostics: Vec<Built>) -> DiagnosticSet {
    match diagnostics.into_iter().collect::<Result<Vec<_>, _>>() {
        Ok(built) => DiagnosticSet::try_from_unsorted(built, status)
            .unwrap_or_else(|_| singleton_set("diagnostics.registry-invalid")),
        Err(_) => singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal `invalid` set for a refused composition document.
pub fn input_invalid(detail: &str) -> DiagnosticSet {
    set(Status::Invalid, vec![one(INPUT_INVALID, detail)])
}

/// The fatal `invalid` set for coupled or implicitly enabled effects.
pub fn effects_coupled(detail: &str) -> DiagnosticSet {
    set(Status::Invalid, vec![one(EFFECTS_COUPLED, detail)])
}

/// The fatal `invalid` set for an unbindable approval.
pub fn approval_unbound(detail: &str) -> DiagnosticSet {
    set(Status::Invalid, vec![one(APPROVAL_UNBOUND, detail)])
}

/// The fatal `invalid` set for a mutable published projection.
pub fn snapshot_mutable(detail: &str) -> DiagnosticSet {
    set(Status::Invalid, vec![one(SNAPSHOT_MUTABLE, detail)])
}

/// The fatal `invalid` set for an unsafe public projection.
pub fn public_data_unsafe(detail: &str) -> DiagnosticSet {
    set(Status::Invalid, vec![one(PUBLIC_DATA_UNSAFE, detail)])
}

/// The fatal `invalid` set for a colliding or unbound replay scope.
pub fn replay_scope_invalid(detail: &str) -> DiagnosticSet {
    set(Status::Invalid, vec![one(REPLAY_SCOPE_INVALID, detail)])
}

/// The `unsupported` set for missing admission evidence; the caller
/// keeps the per-target statuses instead of silently failing.
pub fn evidence_incomplete(detail: &str) -> DiagnosticSet {
    set(Status::Unsupported, vec![one(EVIDENCE_INCOMPLETE, detail)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_are_registered_in_the_successor() {
        for id in [
            INPUT_INVALID,
            EFFECTS_COUPLED,
            APPROVAL_UNBOUND,
            SNAPSHOT_MUTABLE,
            PUBLIC_DATA_UNSAFE,
            REPLAY_SCOPE_INVALID,
        ] {
            let set = set(Status::Invalid, vec![one(id, "probe")]);
            assert_eq!(set.as_slice().len(), 1, "{id}");
            assert_eq!(set.as_slice()[0].id(), id, "{id}");
            assert_eq!(set.as_slice()[0].registry_version(), "0.8.0", "{id}");
        }
        let unsupported = evidence_incomplete("probe");
        assert_eq!(unsupported.as_slice()[0].id(), EVIDENCE_INCOMPLETE);
        assert_eq!(unsupported.as_slice()[0].registry_version(), "0.8.0");
    }

    #[test]
    fn data_carries_only_the_closed_detail_token() {
        let rendered = serde_json::to_string(
            &set(Status::Invalid, vec![one(INPUT_INVALID, "schema-version")]).as_slice()[0],
        )
        .expect("diagnostic serializes");
        assert!(
            rendered.contains("\"detail\":\"schema-version\""),
            "{rendered}"
        );
        assert!(!rendered.contains("path"), "{rendered}");
    }
}
