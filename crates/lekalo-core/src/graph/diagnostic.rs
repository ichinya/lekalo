//! Graph diagnostics routed through the accepted #11 contract (issue #13).
//!
//! Every graph failure is one registered `graph.*` rule (`LEK-GRAPH-NNN`)
//! assembled through the shared registry-backed constructor and finalized
//! into a normalized [`DiagnosticSet`]. Categories are registry data:
//! forbidden cycles are `semantic`, malformed input and limit denials are
//! `infrastructure`; `security` stays reserved for policy owners. Graph
//! severity never computes an exit.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

/// A reference-target or node identity was resolved against the graph.
pub const INPUT_INVALID: &str = "graph.input-invalid";
/// A `requires`/`derived_from` strongly connected component or self-loop.
pub const CYCLE_FORBIDDEN: &str = "graph.cycle-forbidden";
/// A query named a node the graph does not contain.
pub const UNKNOWN_NODE: &str = "graph.unknown-node";
/// A filter or extension record named an unregistered relation.
pub const UNKNOWN_RELATION: &str = "graph.unknown-relation";
/// An edge carries incomplete or degraded provenance evidence.
pub const PROVENANCE_INCOMPLETE: &str = "graph.provenance-incomplete";
/// A traversal exceeded its recorded bound: rejected, never truncated.
pub const TRAVERSAL_LIMIT: &str = "graph.traversal-limit";
/// A shortest-path query has no connecting path.
pub const PATH_NOT_FOUND: &str = "graph.path-not-found";
/// A canonical export exceeded the recorded byte bound.
pub const EXPORT_LIMIT: &str = "graph.export-limit";

/// One fatal construction input violation, before any graph exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InputViolation {
    /// The same kind-qualified identity appeared twice.
    DuplicateNode { node: String },
    /// A from-definition node was absent at edge emission (internal).
    MissingNode { node: String },
    /// An edge endpoint named no node of the compiled project.
    UnresolvedTarget {
        from: String,
        role: String,
        target: String,
    },
    /// A node or edge count crossed the recorded construction cap.
    CapExceeded { cap: &'static str },
}

/// Finalize one registered diagnostic; a registry failure collapses to the
/// registry-invariant set (double developer fault) instead of panicking.
pub(crate) fn one(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
) -> Result<crate::diagnostics::Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, symbol, None, data)
}

/// Build a validated `invalid` set or collapse to the invariant set.
pub(crate) fn invalid_set(diagnostics: Vec<crate::diagnostics::Diagnostic>) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, Status::Invalid)
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

pub(crate) fn token(text: &str) -> DataValue {
    token_value(text)
}

/// The fatal set for construction input violations.
pub(crate) fn input_invalid_set(violations: Vec<InputViolation>) -> DiagnosticSet {
    let mut diagnostics = Vec::with_capacity(violations.len());
    for violation in violations {
        let mut data = DataObject::new();
        let (symbol, detail) = match violation {
            InputViolation::DuplicateNode { node } => (Some(node.clone()), "duplicate-node"),
            InputViolation::MissingNode { node } => (Some(node.clone()), "missing-node"),
            InputViolation::UnresolvedTarget { from, role, target } => {
                data.insert("target".to_owned(), token(&target));
                data.insert("role".to_owned(), token(&role));
                (Some(from), "unresolved-target")
            }
            InputViolation::CapExceeded { cap } => (None, cap),
        };
        data.insert("detail".to_owned(), token(detail));
        match one(INPUT_INVALID, symbol, data) {
            Ok(diagnostic) => diagnostics.push(diagnostic),
            Err(_) => return crate::result::singleton_set("diagnostics.registry-invalid"),
        }
    }
    if diagnostics.is_empty() {
        return input_invalid_detail("empty-input");
    }
    invalid_set(diagnostics)
}

/// The fatal set for a single detail-tagged input violation.
pub(crate) fn input_invalid_detail(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(INPUT_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The fatal set for one forbidden cycle discovery.
pub(crate) fn cycle_forbidden_set(discoveries: Vec<super::cycle::ForbiddenCycle>) -> DiagnosticSet {
    let mut diagnostics = Vec::with_capacity(discoveries.len());
    for discovery in discoveries {
        let mut data = DataObject::new();
        data.insert("relation".to_owned(), token(discovery.relation().key()));
        data.insert(
            "members".to_owned(),
            DataValue::List(
                discovery
                    .members()
                    .iter()
                    .map(|member| {
                        crate::diagnostics::types::Scalar::Token(
                            crate::diagnostics::types::bound_token(member.as_str()),
                        )
                    })
                    .collect(),
            ),
        );
        match one(CYCLE_FORBIDDEN, None, data) {
            Ok(diagnostic) => diagnostics.push(diagnostic),
            Err(_) => return crate::result::singleton_set("diagnostics.registry-invalid"),
        }
    }
    invalid_set(diagnostics)
}

/// The `invalid` set for a query naming an unknown node.
pub fn unknown_node_set(node: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("symbol".to_owned(), token(node));
    match one(UNKNOWN_NODE, Some(bounded_symbol(node)), data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a filter naming an unregistered relation.
pub fn unknown_relation_set(relation: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("relation".to_owned(), token(relation));
    match one(UNKNOWN_RELATION, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a bound-exhausted traversal.
pub fn traversal_limit_set(limit: &'static str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(limit));
    match one(TRAVERSAL_LIMIT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for a shortest-path query without a connecting path.
pub fn path_not_found_set(from: &str, to: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("from".to_owned(), token(from));
    data.insert("to".to_owned(), token(to));
    match one(PATH_NOT_FOUND, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` set for an over-bound canonical export.
pub fn export_limit_set(bytes: usize) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token("export-byte-limit"));
    if let Ok(count) = u64::try_from(bytes) {
        data.insert("bytes".to_owned(), DataValue::Count(count));
    }
    match one(EXPORT_LIMIT, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The warning set for degraded edge provenance (visible, never fatal).
pub fn provenance_incomplete_set(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token(detail));
    match one(PROVENANCE_INCOMPLETE, None, data) {
        Ok(diagnostic) => DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Valid)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The bounded `symbol` echo the #11 wire carries for located rules.
fn bounded_symbol(symbol: &str) -> String {
    crate::diagnostics::types::bound_token(symbol)
}
