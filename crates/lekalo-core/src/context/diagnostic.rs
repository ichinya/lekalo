//! Context-capsule diagnostics routed through the accepted #11 contract
//! (issue #17).
//!
//! The context contract adds no registry rules in v1 — the registry file is
//! unchanged by design, exactly as in ADR-0013. Fatal input violations and
//! unknown subjects reuse the graph-family rules `graph.input-invalid`,
//! `graph.traversal-limit`, and `graph.unknown-node` through the shared
//! registry-backed constructor with bounded tokens on every echo. Capsule
//! truncation is not a diagnostic at all: it is closed in-band metadata
//! (`budget.fits`, the manifest exclusions, and the gap vocabulary), so a
//! small budget is a valid, explainable result and never an error.

use crate::diagnostics::types::{DataObject, DataValue};
use crate::diagnostics::DiagnosticSet;

/// The `invalid` set for a fatal capsule input violation: a budget outside
/// the recorded v1 bounds, or a malformed changed-scope list.
pub(crate) fn input_invalid_set(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), crate::graph::diagnostic::token(detail));
    data.insert(
        "role".to_owned(),
        DataValue::Token("context-plan".to_owned()),
    );
    data.insert(
        "target".to_owned(),
        DataValue::Token("context-capsule".to_owned()),
    );
    match crate::graph::diagnostic::one(crate::graph::diagnostic::INPUT_INVALID, None, data) {
        Ok(diagnostic) => crate::graph::diagnostic::invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_invalid_carries_a_bounded_detail_token() {
        let set = input_invalid_set(&"x".repeat(4_096));
        let rendered = serde_json::to_string(&set).expect("set serializes");
        assert!(rendered.contains("graph.input-invalid"));
        // The bounded token never echoes the whole hostile input.
        assert!(rendered.len() < 4_096);
    }

    #[test]
    fn unknown_roots_reuse_the_graph_rule() {
        let set = crate::graph::diagnostic::unknown_node_set(&"y".repeat(4_096));
        let rendered = serde_json::to_string(&set).expect("set serializes");
        assert!(rendered.contains("graph.unknown-node"));
        assert!(rendered.len() < 4_096);
    }
}
