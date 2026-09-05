//! When/action shapes of the Scenario IR (issue #23).
//!
//! The action vocabulary of v1 is exactly one closed shape, `invoke`: a
//! qualified command or query reference, typed input fields, an optional
//! actor reference, optional deterministic clock control reference, and
//! an explicit idempotency key where applicable. Writes, effects,
//! authorization, retries, and error categories are never inferred from
//! operation names; command/query existence, operation kind, input/output
//! types, and visibility are #12 checks, while scenario data flow and
//! action shape are #23 checks.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::id::{FieldName, SemanticId};
use super::precondition::{optional_field_map, semantic};
use super::reference::{Ref, RefKind};
use super::step::ValueOrRef;

/// The one closed v1 action: invoke a qualified operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvokeAction {
    /// The qualified operation reference (command or query).
    pub operation: SemanticId,
    /// The typed input fields, sorted by field name.
    pub input: Vec<(FieldName, ValueOrRef)>,
    /// The optional actor reference (kind `actor`).
    pub actor: Option<Ref>,
    /// The optional deterministic clock reference (kind `clock`,
    /// pointing at a prior `given` clock step).
    pub clock: Option<Ref>,
    /// The explicit idempotency key where the operation is idempotent.
    pub idempotency_key: Option<ValueOrRef>,
}

impl InvokeAction {
    /// Normalize one wire action, or return the typed rejection set.
    pub(crate) fn from_json(json: &Json, role: &str) -> Result<InvokeAction, DiagnosticSet> {
        let object = json
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("action-shape", Some(role)))?;
        if object.get("kind").and_then(Json::as_str) != Some("invoke") {
            return Err(diagnostic::input_invalid("action-kind", Some(role)));
        }
        let mut operation = None;
        let mut input = None;
        let mut actor = None;
        let mut clock = None;
        let mut idempotency_key = None;
        for (key, value) in object {
            match key.as_str() {
                "kind" => {}
                "operation" => operation = Some(semantic(Some(value), "action-operation", role)?),
                "input" => input = Some(optional_field_map(Some(value), "action-input", role)?),
                "actor" => actor = typed_ref(Some(value), RefKind::Actor, "action-actor", role)?,
                "clock" => clock = typed_ref(Some(value), RefKind::Clock, "action-clock", role)?,
                "idempotencyKey" => idempotency_key = Some(ValueOrRef::from_json(value, role, 0)?),
                _ => return Err(diagnostic::input_invalid("action-shape", Some(role))),
            }
        }
        let (Some(operation), Some(input)) = (operation, input) else {
            return Err(diagnostic::input_invalid("action-shape", Some(role)));
        };
        Ok(InvokeAction {
            operation,
            input,
            actor,
            clock,
            idempotency_key,
        })
    }
}

/// Parse one optional reference that must carry the exact kind.
pub(crate) fn typed_ref(
    json: Option<&Json>,
    kind: RefKind,
    detail: &'static str,
    role: &str,
) -> Result<Option<Ref>, DiagnosticSet> {
    let Some(value) = json else {
        return Ok(None);
    };
    let reference = Ref::from_json(value, role)?;
    if reference.kind != kind {
        return Err(diagnostic::input_invalid(detail, Some(role)));
    }
    Ok(Some(reference))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn invoke() -> Json {
        json!({
            "kind": "invoke",
            "operation": "planner.command.focus_task",
            "input": {"user_id": {"type": "string", "value": "user-1"}}
        })
    }

    #[test]
    fn invoke_actions_parse_their_members() {
        let action = InvokeAction::from_json(&invoke(), "w").expect("invoke action");
        assert_eq!(action.operation.as_str(), "planner.command.focus_task");
        assert_eq!(action.input.len(), 1);
        assert!(action.actor.is_none() && action.clock.is_none());
    }

    #[test]
    fn control_references_are_kind_checked() {
        let with_actor = json!({
            "kind": "invoke",
            "operation": "planner.command.focus_task",
            "input": {},
            "actor": {"$ref": "clock", "id": "clock_step"}
        });
        assert!(InvokeAction::from_json(&with_actor, "w").is_err());
        let with_clock = json!({
            "kind": "invoke",
            "operation": "planner.command.focus_task",
            "input": {},
            "clock": {"$ref": "clock", "id": "clock_step"}
        });
        let action = InvokeAction::from_json(&with_clock, "w").expect("clocked action");
        assert!(action.clock.is_some());
    }

    #[test]
    fn unknown_action_members_and_kinds_are_rejected() {
        let extra = json!({
            "kind": "invoke",
            "operation": "planner.command.focus_task",
            "input": {},
            "sql": "delete from users"
        });
        assert!(InvokeAction::from_json(&extra, "w").is_err());
        let kind = json!({
            "kind": "sql",
            "operation": "planner.command.focus_task",
            "input": {}
        });
        assert!(InvokeAction::from_json(&kind, "w").is_err());
    }
}
