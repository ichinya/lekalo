//! Typed references of the Scenario IR (issue #23).
//!
//! A [`Ref`] is a closed typed union over everything a scenario step may
//! point at: semantic symbols, operations, entities, fields, events,
//! jobs, effects, errors, requirements, fixtures, actors, step outputs,
//! prior given values, clocks, and deterministic ID sources. Every
//! reference carries a qualified stable identifier in the inherited
//! grammar — never a short spelling, a source expression, a native path,
//! or a URL — plus an optional validated member path and an optional
//! expected type reference. Scenario-local kinds (`step-output`,
//! `given-value`, `clock`, `id-source`) carry step identifiers and are
//! resolved by the scenario data-flow pass; semantic resolution and
//! visibility belong to #12, error-union membership to #62.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::id::{SemanticId, StepId};

/// The closed reference kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefKind {
    /// A semantic symbol (command, query, entity, and so on).
    Symbol,
    /// A command or query operation.
    Operation,
    /// An entity.
    Entity,
    /// A field of an entity.
    Field,
    /// A declared event.
    Event,
    /// A declared job.
    Job,
    /// A declared effect.
    Effect,
    /// A declared error.
    Error,
    /// A requirement.
    Requirement,
    /// A reusable fixture.
    Fixture,
    /// An actor or identity.
    Actor,
    /// The output of a prior `when` step in this scenario.
    StepOutput,
    /// A value established by a prior `given` step in this scenario.
    GivenValue,
    /// A deterministic clock established by a prior `given` step.
    Clock,
    /// A deterministic ID source established by a prior `given` step.
    IdSource,
}

impl RefKind {
    /// The wire tag of this kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            RefKind::Symbol => "symbol",
            RefKind::Operation => "operation",
            RefKind::Entity => "entity",
            RefKind::Field => "field",
            RefKind::Event => "event",
            RefKind::Job => "job",
            RefKind::Effect => "effect",
            RefKind::Error => "error",
            RefKind::Requirement => "requirement",
            RefKind::Fixture => "fixture",
            RefKind::Actor => "actor",
            RefKind::StepOutput => "step-output",
            RefKind::GivenValue => "given-value",
            RefKind::Clock => "clock",
            RefKind::IdSource => "id-source",
        }
    }

    /// The kind for one wire tag.
    pub(crate) fn from_wire(text: &str) -> Option<RefKind> {
        match text {
            "symbol" => Some(RefKind::Symbol),
            "operation" => Some(RefKind::Operation),
            "entity" => Some(RefKind::Entity),
            "field" => Some(RefKind::Field),
            "event" => Some(RefKind::Event),
            "job" => Some(RefKind::Job),
            "effect" => Some(RefKind::Effect),
            "error" => Some(RefKind::Error),
            "requirement" => Some(RefKind::Requirement),
            "fixture" => Some(RefKind::Fixture),
            "actor" => Some(RefKind::Actor),
            "step-output" => Some(RefKind::StepOutput),
            "given-value" => Some(RefKind::GivenValue),
            "clock" => Some(RefKind::Clock),
            "id-source" => Some(RefKind::IdSource),
            _ => None,
        }
    }

    /// Whether the identifier of this kind is a scenario-local step.
    pub(crate) fn is_step_local(&self) -> bool {
        matches!(
            self,
            RefKind::StepOutput | RefKind::GivenValue | RefKind::Clock | RefKind::IdSource
        )
    }
}

/// One typed reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ref {
    /// The closed reference kind.
    pub kind: RefKind,
    /// The qualified stable identifier (or step identifier for the
    /// scenario-local kinds).
    pub id: RefTarget,
    /// The optional validated member path into the referenced value.
    pub path: Option<String>,
    /// The optional expected type reference.
    pub expected_type: Option<SemanticId>,
}

/// The validated identifier of one reference target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefTarget {
    /// A semantic identifier in the inherited #5 grammar.
    Semantic(SemanticId),
    /// A namespaced identifier of the effect-contract shape.
    Namespaced(super::id::NamespacedId),
    /// A scenario-local step identifier.
    Step(StepId),
}

impl Ref {
    /// Normalize one JSON value into a typed reference, or return the
    /// typed rejection set. `role` locates the reference.
    pub(crate) fn from_json(json: &Json, role: &str) -> Result<Ref, DiagnosticSet> {
        let object = match json.as_object() {
            Some(map) => map,
            None => return Err(diagnostic::input_invalid("ref-shape", Some(role))),
        };
        let keys: Vec<&str> = object.keys().map(String::as_str).collect();
        if !matches!(
            keys.as_slice(),
            ["$ref", "id"]
                | ["$ref", "id", "path"]
                | ["$ref", "id", "path", "type"]
                | ["$ref", "id", "type"]
        ) {
            return Err(diagnostic::input_invalid("ref-shape", Some(role)));
        }
        let Some(Json::String(kind_text)) = object.get("$ref") else {
            return Err(diagnostic::input_invalid("ref-kind", Some(role)));
        };
        let Some(kind) = RefKind::from_wire(kind_text) else {
            return Err(diagnostic::input_invalid("ref-kind", Some(role)));
        };
        let target = match object.get("id") {
            Some(Json::String(id_text)) => {
                if kind.is_step_local() {
                    RefTarget::Step(
                        StepId::parse(id_text)
                            .map_err(|_| diagnostic::input_invalid("ref-step-id", Some(role)))?,
                    )
                } else if let Ok(semantic) = SemanticId::parse(id_text) {
                    RefTarget::Semantic(semantic)
                } else {
                    RefTarget::Namespaced(
                        super::id::NamespacedId::parse(id_text)
                            .map_err(|_| diagnostic::input_invalid("ref-id", Some(role)))?,
                    )
                }
            }
            _ => return Err(diagnostic::input_invalid("ref-id", Some(role))),
        };
        let path = match object.get("path") {
            None | Some(Json::Null) => None,
            Some(Json::String(path_text)) => {
                if member_path_valid(path_text) {
                    Some(path_text.clone())
                } else {
                    return Err(diagnostic::input_invalid("ref-path", Some(role)));
                }
            }
            Some(_) => return Err(diagnostic::input_invalid("ref-path", Some(role))),
        };
        let expected_type = match object.get("type") {
            None | Some(Json::Null) => None,
            Some(Json::String(type_text)) => Some(
                SemanticId::parse(type_text)
                    .map_err(|_| diagnostic::input_invalid("ref-type", Some(role)))?,
            ),
            Some(_) => return Err(diagnostic::input_invalid("ref-type", Some(role))),
        };
        Ok(Ref {
            kind,
            id: target,
            path,
            expected_type,
        })
    }
}

/// The bounded member path grammar: one to sixteen dot-separated
/// segments of at most 64 identifier bytes each.
pub(crate) fn member_path_valid(text: &str) -> bool {
    if text.is_empty() || text.chars().count() > 1031 {
        return false;
    }
    let segments: Vec<&str> = text.split('.').collect();
    segments.len() <= 16
        && segments.iter().all(|segment| {
            let bytes = segment.as_bytes();
            !bytes.is_empty()
                && bytes.len() <= 64
                && bytes
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ok(json: Json) -> Ref {
        Ref::from_json(&json, "r").expect("valid ref")
    }

    fn err(json: Json) {
        assert!(Ref::from_json(&json, "r").is_err(), "must reject {json}");
    }

    #[test]
    fn refs_accept_semantic_namespaced_and_step_targets() {
        let semantic = ok(json!({"$ref": "operation", "id": "planner.command.focus_task"}));
        assert_eq!(
            semantic.id,
            RefTarget::Semantic(SemanticId::parse("planner.command.focus_task").unwrap())
        );
        let namespaced = ok(json!({"$ref": "fixture", "id": "core/planner-seed"}));
        assert!(matches!(namespaced.id, RefTarget::Namespaced(_)));
        let step = ok(json!({"$ref": "step-output", "id": "focus"}));
        assert!(matches!(step.id, RefTarget::Step(_)));
    }

    #[test]
    fn refs_carry_paths_and_expected_types() {
        let reference = ok(json!({
            "$ref": "step-output",
            "id": "focus",
            "path": "result.task_id",
            "type": "planner.type.task_id"
        }));
        assert_eq!(reference.path.as_deref(), Some("result.task_id"));
        assert_eq!(
            reference.expected_type.as_ref().map(SemanticId::as_str),
            Some("planner.type.task_id")
        );
    }

    #[test]
    fn refs_reject_unknown_kinds_short_ids_and_bad_paths() {
        err(json!({"$ref": "mystery", "id": "planner.task"}));
        err(json!({"$ref": "entity", "id": "task"}));
        err(json!({"$ref": "step-output", "id": "planner.task"}));
        err(json!({"$ref": "entity", "id": "planner.task", "path": "-bad"}));
        err(json!({"$ref": "entity", "id": "planner.task", "type": "short"}));
        err(json!({"$ref": "entity", "id": "planner.task", "extra": 1}));
    }

    #[test]
    fn member_paths_are_bounded() {
        assert!(member_path_valid("a"));
        assert!(member_path_valid("result.task_id"));
        assert!(!member_path_valid(""));
        assert!(!member_path_valid("-bad"));
        assert!(!member_path_valid("a..b"));
        let long = "a".repeat(65);
        assert!(!member_path_valid(&long));
    }
}
