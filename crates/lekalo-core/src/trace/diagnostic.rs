//! Trace diagnostics routed through the accepted #11 contract (issue #22).
//!
//! Every trace failure is one registered rule assembled through the
//! shared registry-backed constructor and finalized into a normalized
//! [`DiagnosticSet`]. The trace contract adds no rules of its own in v1
//! (the registry file is unchanged by design): wire and semantics
//! violations reuse `graph.input-invalid`, unknown query subjects reuse
//! `graph.unknown-node`, over-bound queries reuse `graph.traversal-limit`,
//! over-bound exports reuse `graph.export-limit`, and unreadable manifest
//! files reuse `loader.io`. Every echoed token is bounded (the #11 review
//! invariant) and only after grammar validation, so attacker-controlled
//! input never scales or shapes the envelope.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

/// The registered rule reused for fatal wire/semantics violations.
pub(crate) const INPUT_INVALID: &str = "graph.input-invalid";
/// The registered rule reused for a query naming an unknown subject.
pub(crate) const UNKNOWN_NODE: &str = "graph.unknown-node";
/// The registered rule reused for an over-bound query.
pub(crate) const TRAVERSAL_LIMIT: &str = "graph.traversal-limit";
/// The registered rule reused for an unreadable manifest file.
pub const IO: &str = "loader.io";
pub(crate) const EXPORT_LIMIT: &str = "graph.export-limit";

/// A lexical logical-path violation carrying its stable `structure.*` code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathViolation(pub &'static str);

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(id: &str, symbol: Option<String>, data: DataObject) -> DiagnosticSet {
    match build(id, symbol, None, data) {
        Ok(diagnostic) => match DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Invalid)
        {
            Ok(set) => set,
            Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
        },
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one detail-tagged violation. `detail` is a fixed
/// classification tag owned by this module, never raw input; the optional
/// echo is bounded after grammar validation.
pub(crate) fn input_invalid_detail(detail: &'static str, token: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    if let Some(echo) = token {
        data.insert("target".to_owned(), token_value(echo));
    }
    one(INPUT_INVALID, None, data)
}

/// The fatal set for a lexical path violation with its stable code.
pub(crate) fn path_violation_set(violation: &PathViolation) -> DiagnosticSet {
    input_invalid_detail("path-invalid", Some(violation.0))
}

/// The `invalid` set for a query naming an unknown node id or external id.
pub(crate) fn unknown_subject_set(subject: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("unknown-subject"));
    one(UNKNOWN_NODE, Some(bound_token(subject)), data)
}

/// The `invalid` set for a query that crossed a recorded bound.
pub(crate) fn traversal_limit_set(limit: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(limit));
    one(TRAVERSAL_LIMIT, None, data)
}

/// The `invalid` set for an over-bound canonical export.
pub(crate) fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("bytes".to_owned(), DataValue::Count(bytes as u64));
    one(EXPORT_LIMIT, None, data)
}

/// The public `invalid` set for an unreadable manifest file: the CLI
/// reads the document bytes, so this failure is constructed there. The
/// detail is a fixed classification tag owned by the core.
pub fn io_failure(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    one(IO, None, data)
}
