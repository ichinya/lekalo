//! The semantic self-check of the query-model attachment (issue #64).
//!
//! Everything checkable without the bound Model: declaration
//! coherence across cardinality, sort, pagination, consistency, and
//! the foreign escape. Model-dependent checks (references, field
//! types, identity tie-breakers, tenant filters, visibility) live in
//! [`resolve`](super::resolve), which owns them.

use super::diagnostic;
use super::query::{Cardinality, Consistency, PaginationStrategy};
use super::QueryModelAttachment;
use crate::diagnostics::DiagnosticSet;

/// Run the closed semantic self-check over one attachment. Pure and
/// read-only.
pub(crate) fn semantic_self_check(attachment: &QueryModelAttachment) -> Result<(), DiagnosticSet> {
    for decl in attachment.queries() {
        // Defense in depth: the wire decoder enforces the same bounds
        // incrementally; the typed revalidation refuses anything that
        // reached this level through another constructor.
        if !decl.within_bounds() {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "declaration-bound",
                Some(decl.query.as_str()),
            ));
        }
        check_staleness(decl)?;
        check_cardinality(decl)?;
        check_cursor_parameter(decl)?;
    }
    Ok(())
}

/// The staleness bound exists exactly for the `bounded` profile.
fn check_staleness(decl: &super::query::QueryDecl) -> Result<(), DiagnosticSet> {
    match (decl.consistency, decl.max_staleness) {
        (Consistency::Bounded, Some(_))
        | (Consistency::Strong, None)
        | (Consistency::StaleOk, None) => Ok(()),
        (Consistency::Bounded, None) => Err(diagnostic::rule_invalid(
            diagnostic::CONTRACT_INVALID,
            "staleness-missing",
            Some(decl.query.as_str()),
        )),
        (_, Some(_)) => Err(diagnostic::rule_invalid(
            diagnostic::CONTRACT_INVALID,
            "staleness-forbidden",
            Some(decl.query.as_str()),
        )),
    }
}

/// Cardinality admits exactly the right sort/pagination combination:
/// `one`/`optional` carry neither, `list` carries no pagination, and
/// `page` requires both while `stream` requires the total order.
fn check_cardinality(decl: &super::query::QueryDecl) -> Result<(), DiagnosticSet> {
    let card = decl.cardinality;
    if !card.admits_pagination() && decl.pagination.is_some() {
        return Err(diagnostic::rule_invalid(
            diagnostic::PAGINATION_INVALID,
            "cardinality-forbids-pagination",
            Some(decl.query.as_str()),
        ));
    }
    if card.requires_total_order() && decl.sort.is_none() {
        return Err(diagnostic::rule_invalid(
            diagnostic::SORT_INVALID,
            "total-order-required",
            Some(decl.query.as_str()),
        ));
    }
    if matches!(card, Cardinality::One | Cardinality::Optional) && decl.sort.is_some() {
        return Err(diagnostic::rule_invalid(
            diagnostic::SORT_INVALID,
            "single-result-forbids-sort",
            Some(decl.query.as_str()),
        ));
    }
    if card == Cardinality::Page && decl.pagination.is_none() {
        return Err(diagnostic::rule_invalid(
            diagnostic::PAGINATION_INVALID,
            "page-requires-pagination",
            Some(decl.query.as_str()),
        ));
    }
    Ok(())
}

/// The cursor strategy must name one declared parameter as its key.
fn check_cursor_parameter(decl: &super::query::QueryDecl) -> Result<(), DiagnosticSet> {
    let pagination = match &decl.pagination {
        Some(pagination) => pagination,
        None => return Ok(()),
    };
    if pagination.strategy != PaginationStrategy::Cursor {
        return Ok(());
    }
    let key = match &pagination.key_parameter {
        Some(key) => key,
        None => {
            return Err(diagnostic::rule_invalid(
                diagnostic::PAGINATION_INVALID,
                "cursor-key-missing",
                Some(decl.query.as_str()),
            ))
        }
    };
    if !decl.parameters.iter().any(|param| &param.name == key) {
        return Err(diagnostic::rule_invalid(
            diagnostic::PAGINATION_INVALID,
            "cursor-key-undeclared",
            Some(decl.query.as_str()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::version;
    use super::*;

    /// Build an attachment from a JSON body.
    fn attachment(body: serde_json::Value) -> QueryModelAttachment {
        QueryModelAttachment::from_value(&body).expect("attachment")
    }

    fn queries(value: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": version::SCHEMA_VERSION,
            "identity": version::IDENTITY,
            "attachmentRevision": "1.0.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "tenancy": [],
            "queries": value
        })
    }

    fn sort_keys() -> serde_json::Value {
        serde_json::json!([
            {"field": "task_id", "direction": "asc"}
        ])
    }

    #[test]
    fn bounded_requires_staleness_and_others_forbid_it() {
        let ok = attachment(queries(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "list",
            "consistency": "bounded",
            "maxStaleness": 30
        }])));
        assert!(ok.validate_queries().is_ok());

        let missing = queries(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "list",
            "consistency": "bounded"
        }]));
        assert!(QueryModelAttachment::from_value(&missing).is_err());

        let forbidden = queries(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "list",
            "consistency": "strong",
            "maxStaleness": 30
        }]));
        assert!(QueryModelAttachment::from_value(&forbidden).is_err());
    }

    #[test]
    fn page_requires_sort_and_pagination() {
        let bad = queries(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "page",
            "consistency": "strong"
        }]));
        assert!(QueryModelAttachment::from_value(&bad).is_err());

        let good = attachment(queries(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "page",
            "consistency": "strong",
            "sort": sort_keys(),
            "pagination": {"strategy": "offset", "limit": 20}
        }])));
        assert!(good.validate_queries().is_ok());
    }

    #[test]
    fn one_forbids_sort_and_pagination() {
        let bad_sort = queries(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "one",
            "consistency": "strong",
            "sort": sort_keys()
        }]));
        assert!(QueryModelAttachment::from_value(&bad_sort).is_err());

        let bad_page = queries(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "optional",
            "consistency": "strong",
            "pagination": {"strategy": "offset", "limit": 20}
        }]));
        assert!(QueryModelAttachment::from_value(&bad_page).is_err());
    }

    #[test]
    fn cursor_must_name_a_declared_parameter() {
        let bad = queries(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "page",
            "consistency": "strong",
            "sort": sort_keys(),
            "pagination": {"strategy": "cursor", "limit": 20, "keyParameter": "cursor_key"}
        }]));
        assert!(QueryModelAttachment::from_value(&bad).is_err());

        let good = attachment(queries(serde_json::json!([{
            "query": "planner.q",
            "source": "planner.task",
            "cardinality": "page",
            "consistency": "strong",
            "parameters": [{"name": "cursor_key", "type": "planner.task_id"}],
            "sort": sort_keys(),
            "pagination": {"strategy": "cursor", "limit": 20, "keyParameter": "cursor_key"}
        }])));
        assert!(good.validate_queries().is_ok());
    }
}
