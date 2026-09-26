//! The operation binding key of the checked mode (issue #46).
//!
//! A maintained document's operation binds to a generated endpoint
//! through its `x-lekalo-endpoint` anchor first and its `operationId`
//! second (plan §4.3). One closed key keeps the bind/drift decision
//! shared by `merge` and `check` — the same rule both verify paths
//! apply, never two.

use serde_json::Value as Json;

/// The binding identity of one OpenAPI operation object.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct BindingKey {
    /// The `x-lekalo-endpoint` anchor, when present.
    pub endpoint: Option<String>,
    /// The `operationId`, when present.
    pub operation_id: Option<String>,
}

impl BindingKey {
    /// Extract one operation's binding key.
    pub fn of(operation: &Json) -> Option<Self> {
        let object = operation.as_object()?;
        let endpoint = object
            .get("x-lekalo-endpoint")
            .and_then(Json::as_str)
            .map(str::to_owned);
        let operation_id = object
            .get("operationId")
            .and_then(Json::as_str)
            .map(str::to_owned);
        if endpoint.is_none() && operation_id.is_none() {
            return None;
        }
        Some(Self {
            endpoint,
            operation_id,
        })
    }

    /// Whether this key binds one generated endpoint: the endpoint
    /// anchor matches first; without an anchor, the operation id
    /// matches the generated operation id. (An operation id that
    /// belongs to a *different* generated endpoint never binds: the
    /// comparison is against this endpoint's id only.)
    pub fn binds(&self, endpoint: &str, operation_id: &str) -> bool {
        if let Some(anchor) = &self.endpoint {
            return anchor == endpoint;
        }
        self.operation_id.as_deref() == Some(operation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_endpoint_anchor_binds_first() {
        let key = BindingKey::of(&json!({
            "x-lekalo-endpoint": "planner.endpoint_focus_task",
            "operationId": "someoneElsesId"
        }))
        .expect("bound");
        assert_eq!(key.endpoint.as_deref(), Some("planner.endpoint_focus_task"));
        assert!(key.binds("planner.endpoint_focus_task", "plannerEndpointFocusTask"));
        assert!(!key.binds("planner.endpoint_other", "plannerEndpointOther"));
    }

    #[test]
    fn the_operation_id_binds_second() {
        let key =
            BindingKey::of(&json!({ "operationId": "plannerEndpointFocusTask" })).expect("bound");
        assert!(key.binds("planner.endpoint_focus_task", "plannerEndpointFocusTask"));
        assert!(!key.binds("planner.endpoint_other", "plannerEndpointOther"));
    }

    #[test]
    fn an_operation_without_identifiers_is_unbound() {
        assert!(BindingKey::of(&json!({ "responses": {} })).is_none());
    }
}
