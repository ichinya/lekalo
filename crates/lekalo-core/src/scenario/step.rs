//! Step shapes of the Scenario IR (issue #23).
//!
//! A scenario is an ordered triple of step arrays: `given` establishes
//! data and controls, `when` performs one explicit action, and `then`
//! asserts deterministic outcomes. Behavioral order is preserved exactly
//! — arrays are never sorted. Identity is `scenario_id + step_id`; the
//! array position and the step text are never identity, and the
//! occurrence ordinal exists only for source/reference disambiguation.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;

use super::action::InvokeAction;
use super::assertion::Assertion;
use super::diagnostic;
use super::precondition::Precondition;
use super::reference::Ref;
use super::value::TypedValue;

/// One typed value or one typed reference, the leaf of every data
/// position in a step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueOrRef {
    /// A closed typed value.
    Value(TypedValue),
    /// A closed typed reference.
    Reference(Ref),
}

impl ValueOrRef {
    /// Normalize one JSON value into its typed leaf, or return the typed
    /// rejection set.
    pub(crate) fn from_json(
        json: &Json,
        role: &str,
        depth: usize,
    ) -> Result<ValueOrRef, DiagnosticSet> {
        if json.get("$ref").is_some() {
            Ok(ValueOrRef::Reference(Ref::from_json(json, role)?))
        } else {
            Ok(ValueOrRef::Value(TypedValue::from_json(json, role, depth)?))
        }
    }

    /// The number of typed values reachable from this leaf, inclusive.
    pub(crate) fn count(&self) -> usize {
        match self {
            ValueOrRef::Value(value) => value.count(),
            ValueOrRef::Reference(_) => 1,
        }
    }
}

/// One `given` step: a precondition that establishes data or controls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GivenStep {
    /// The stable scenario-local step identity.
    pub step_id: StepKey,
    /// The closed precondition payload.
    pub precondition: Precondition,
}

/// One `when` step: one explicit action plus optional bounded replay
/// metadata for idempotency verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhenStep {
    /// The stable scenario-local step identity.
    pub step_id: StepKey,
    /// The closed action payload.
    pub action: InvokeAction,
    /// Optional replay metadata pointing at a prior `when` step.
    pub replay: Option<ReplayMeta>,
}

/// One `then` step: one assertion observing a reachable action output or
/// prior step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThenStep {
    /// The stable scenario-local step identity.
    pub step_id: StepKey,
    /// The step whose output or established value this assertion
    /// observes.
    pub observes: StepKey,
    /// The closed assertion payload.
    pub assertion: Assertion,
}

/// A validated step identifier (newtype over the id grammar).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StepKey(pub(super) super::id::StepId);

impl StepKey {
    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Bounded replay metadata for idempotency verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayMeta {
    /// The prior `when` step this call replays.
    pub of: StepKey,
    /// The equivalence expectation of the replay.
    pub expect: ReplayExpect,
}

/// The closed replay expectation vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayExpect {
    /// The replay must return the same result.
    SameResult,
    /// The replay must not create a duplicate.
    NoDuplicate,
}

impl ReplayExpect {
    /// The wire tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            ReplayExpect::SameResult => "same-result",
            ReplayExpect::NoDuplicate => "no-duplicate",
        }
    }
}

/// Parse one wire step array member into its typed shape. `role`
/// locates the step in diagnostics.
pub(crate) fn given_from_json(json: &Json, role: &str) -> Result<GivenStep, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("step-shape", Some(role)))?;
    if object
        .keys()
        .map(String::as_str)
        .collect::<Vec<&str>>()
        .as_slice()
        != ["precondition", "stepId"]
    {
        return Err(diagnostic::input_invalid("step-shape", Some(role)));
    }
    let step_id = step_id_field(object, role)?;
    let precondition = Precondition::from_json(object.get("precondition").expect("checked"), role)?;
    Ok(GivenStep {
        step_id,
        precondition,
    })
}

/// Parse one wire `when` member.
pub(crate) fn when_from_json(json: &Json, role: &str) -> Result<WhenStep, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("step-shape", Some(role)))?;
    let keys: Vec<&str> = object.keys().map(String::as_str).collect();
    if !matches!(
        keys.as_slice(),
        ["action", "stepId"] | ["action", "replay", "stepId"]
    ) {
        return Err(diagnostic::input_invalid("step-shape", Some(role)));
    }
    let step_id = step_id_field(object, role)?;
    let action = InvokeAction::from_json(object.get("action").expect("checked"), role)?;
    let replay = match object.get("replay") {
        None => None,
        Some(replay) => {
            let replay_object = replay
                .as_object()
                .ok_or_else(|| diagnostic::input_invalid("replay-shape", Some(role)))?;
            if replay_object
                .keys()
                .map(String::as_str)
                .collect::<Vec<&str>>()
                .as_slice()
                != ["expect", "of"]
            {
                return Err(diagnostic::input_invalid("replay-shape", Some(role)));
            }
            let expect = match replay_object.get("expect") {
                Some(Json::String(text)) => match text.as_str() {
                    "same-result" => ReplayExpect::SameResult,
                    "no-duplicate" => ReplayExpect::NoDuplicate,
                    _ => return Err(diagnostic::input_invalid("replay-expect", Some(role))),
                },
                _ => return Err(diagnostic::input_invalid("replay-expect", Some(role))),
            };
            let of = StepKey(
                super::id::StepId::parse(
                    replay_object
                        .get("of")
                        .and_then(Json::as_str)
                        .ok_or_else(|| diagnostic::input_invalid("replay-of", Some(role)))?,
                )
                .map_err(|_| diagnostic::input_invalid("replay-of", Some(role)))?,
            );
            Some(ReplayMeta { of, expect })
        }
    };
    Ok(WhenStep {
        step_id,
        action,
        replay,
    })
}

/// Parse one wire `then` member.
pub(crate) fn then_from_json(json: &Json, role: &str) -> Result<ThenStep, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("step-shape", Some(role)))?;
    if object
        .keys()
        .map(String::as_str)
        .collect::<Vec<&str>>()
        .as_slice()
        != ["assertion", "observes", "stepId"]
    {
        return Err(diagnostic::input_invalid("step-shape", Some(role)));
    }
    let step_id = step_id_field(object, role)?;
    let observes = StepKey(
        super::id::StepId::parse(
            object
                .get("observes")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("observes", Some(role)))?,
        )
        .map_err(|_| diagnostic::input_invalid("observes", Some(role)))?,
    );
    let assertion = Assertion::from_json(object.get("assertion").expect("checked"), role)?;
    Ok(ThenStep {
        step_id,
        observes,
        assertion,
    })
}

/// Extract and validate the `stepId` member of one step object.
fn step_id_field(
    object: &serde_json::Map<String, Json>,
    role: &str,
) -> Result<StepKey, DiagnosticSet> {
    Ok(StepKey(
        super::id::StepId::parse(
            object
                .get("stepId")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("step-id", Some(role)))?,
        )
        .map_err(|_| diagnostic::input_invalid("step-id", Some(role)))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn value_or_ref_dispatches_on_the_discriminator() {
        let value = ValueOrRef::from_json(&json!({"type": "integer", "value": 3}), "v", 0)
            .expect("value leaf");
        assert!(matches!(value, ValueOrRef::Value(TypedValue::Integer(3))));
        let reference =
            ValueOrRef::from_json(&json!({"$ref": "step-output", "id": "focus"}), "v", 0)
                .expect("reference leaf");
        assert!(matches!(reference, ValueOrRef::Reference(_)));
    }

    #[test]
    fn when_steps_accept_with_and_without_replay_metadata() {
        let invoke = json!({
            "kind": "invoke",
            "operation": "planner.command.focus_task",
            "input": {}
        });
        let step =
            when_from_json(&json!({"stepId": "focus", "action": invoke}), "w").expect("when step");
        assert_eq!(step.step_id.as_str(), "focus");
        assert!(step.replay.is_none());
        let replayed = when_from_json(
            &json!({
                "stepId": "replay_focus",
                "action": invoke,
                "replay": {"of": "focus", "expect": "no-duplicate"}
            }),
            "w",
        )
        .expect("replayed when step");
        assert_eq!(
            replayed.replay.as_ref().expect("replay").of.as_str(),
            "focus"
        );
    }

    #[test]
    fn malformed_step_shapes_are_rejected() {
        let invoke = json!({
            "kind": "invoke",
            "operation": "planner.command.focus_task",
            "input": {}
        });
        assert!(when_from_json(&json!({"stepId": "focus"}), "w").is_err());
        assert!(when_from_json(
            &json!({"stepId": "focus", "action": invoke, "extra": 1}),
            "w"
        )
        .is_err());
        assert!(then_from_json(&json!({"stepId": "t", "observes": "focus"}), "t").is_err());
        assert!(given_from_json(&json!({"stepId": "g"}), "g").is_err());
    }
}
