//! Then/assertion shapes of the Scenario IR (issue #23).
//!
//! The closed assertion vocabulary of v1: `result`, `error`,
//! `entity_state`, `emitted`, `forbidden_effect`, `authorization`,
//! `idempotency`, `contract_match`, `deterministic_fixture`, and the
//! explicit `unsupported` expectation. Every assertion observes a
//! reachable action output or prior step and is separate from metrics
//! and telemetry — a metric or timing observation can never satisfy an
//! assertion. Error membership is validated by #62 once accepted;
//! effect meaning stays with #14; actor, scope, and policy semantics
//! stay with #25; transaction and concurrency guarantees stay with #24.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;

use super::action::typed_ref;
use super::diagnostic;
use super::id::{FieldName, NamespacedId, SemanticId};
use super::precondition::{has_duplicates, selector_field, semantic};
use super::reference::{Ref, RefKind};
use super::step::ValueOrRef;

/// One closed `then` assertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Assertion {
    /// Expected success output with an accepted output type reference.
    Result {
        /// The accepted output type reference.
        value_type: SemanticId,
        /// The optional expected output value or reference.
        value: Option<ValueOrRef>,
    },
    /// Expected typed domain error with optional public payload fields.
    Error {
        /// The typed error reference (#62 validates membership).
        error: SemanticId,
        /// The optional expected public payload fields.
        payload: Vec<(FieldName, ValueOrRef)>,
        /// The optional expected ErrorContract reference.
        contract: Option<NamespacedId>,
    },
    /// Expected entity state after the action.
    EntityState {
        /// The qualified entity.
        entity: SemanticId,
        /// The ordered row selector.
        selector: Vec<super::precondition::FieldPredicate>,
        /// The existence or count expectation.
        expect: EntityExpectation,
        /// The optional expected field states, sorted by field name.
        fields: Vec<(FieldName, FieldExpectation)>,
    },
    /// Expected event or job emission with bounded occurrence count.
    Emitted {
        /// The typed emission target.
        target: EmittedTarget,
        /// The optional bounded occurrence expectation.
        count: Option<OccurrenceCount>,
    },
    /// A declared effect that must not have happened; the reference is
    /// typed and its meaning stays with #14.
    ForbiddenEffect {
        /// The typed effect reference.
        effect: SemanticId,
        /// The closed scope of the prohibition.
        scope: EffectScope,
        /// The exact field, present exactly when the scope is field.
        field: Option<FieldName>,
    },
    /// Expected authorization outcome; actor, scope, and policy
    /// semantics stay with #25.
    Authorization {
        /// The typed actor reference.
        actor: Ref,
        /// The namespaced policy reference.
        policy: NamespacedId,
        /// The expected outcome.
        outcome: AuthOutcome,
    },
    /// Expected replay idempotency; effect proof stays with #14/#24.
    Idempotency {
        /// The replayed `when` step.
        replay: super::step::StepKey,
        /// The equivalence relation of the replay.
        equivalence: Equivalence,
    },
    /// Expected match against an accepted schema or contract.
    ContractMatch {
        /// The accepted contract reference.
        contract: NamespacedId,
        /// The optional bounded projection selector, sorted.
        projection: Vec<String>,
    },
    /// Expected deterministic fixture outcome pinned by digest.
    DeterministicFixture {
        /// The canonical digest expectation.
        digest: Sha256Digest,
        /// The optional clock control reference.
        clock: Option<Ref>,
        /// The optional ID source control reference.
        id_source: Option<Ref>,
    },
    /// An explicit expectation that a required capability is known
    /// absent — never an implicit pass.
    Unsupported {
        /// The namespaced capability that is known absent.
        capability: NamespacedId,
        /// The optional bounded note.
        note: Option<String>,
    },
}

/// The closed entity-state expectation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityExpectation {
    /// At least one row must exist.
    Exists,
    /// No row may exist.
    Missing,
    /// Exactly this many rows must exist.
    Count(u32),
}

/// One expected field state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldExpectation {
    /// An exact typed value or reference.
    Value(ValueOrRef),
    /// A closed typed matcher for unpinned literals.
    Match(MatchKind),
}

/// The closed matcher vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchKind {
    /// Any canonical datetime.
    Datetime,
    /// Any canonical UUID.
    Uuid,
    /// Any canonical URI.
    Uri,
    /// Any canonical decimal.
    Decimal,
    /// Any non-null value.
    NonNull,
}

/// The closed emission target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmittedTarget {
    /// The closed emission kind.
    pub kind: EmitKind,
    /// The qualified event or job identity.
    pub id: SemanticId,
}

/// The closed emission kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmitKind {
    /// A declared event.
    Event,
    /// A declared job.
    Job,
}

/// The bounded occurrence expectation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OccurrenceCount {
    /// At least this many occurrences.
    AtLeast(u32),
    /// Exactly this many occurrences.
    Exactly(u32),
}

/// The closed forbidden-effect scopes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectScope {
    /// The whole entity is protected.
    Entity,
    /// Exactly one field is protected.
    Field,
    /// The typed resource is protected.
    Resource,
}

/// The closed authorization outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthOutcome {
    /// The action must be allowed.
    Allowed,
    /// The action must be denied.
    Denied,
}

/// The closed replay equivalence relations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Equivalence {
    /// Byte-identical results.
    Identical,
    /// Semantically equivalent results.
    Equivalent,
}

impl Equivalence {
    /// The wire tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            Equivalence::Identical => "identical",
            Equivalence::Equivalent => "equivalent",
        }
    }
}

impl Assertion {
    /// The closed kind tag of this assertion.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            Assertion::Result { .. } => "result",
            Assertion::Error { .. } => "error",
            Assertion::EntityState { .. } => "entity_state",
            Assertion::Emitted { .. } => "emitted",
            Assertion::ForbiddenEffect { .. } => "forbidden_effect",
            Assertion::Authorization { .. } => "authorization",
            Assertion::Idempotency { .. } => "idempotency",
            Assertion::ContractMatch { .. } => "contract_match",
            Assertion::DeterministicFixture { .. } => "deterministic_fixture",
            Assertion::Unsupported { .. } => "unsupported",
        }
    }

    /// The typed error reference for the coverage vector, if any.
    pub fn error_ref(&self) -> Option<&SemanticId> {
        match self {
            Assertion::Error { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The typed effect reference for the coverage vector, if any.
    pub fn effect_ref(&self) -> Option<&SemanticId> {
        match self {
            Assertion::ForbiddenEffect { effect, .. } => Some(effect),
            _ => None,
        }
    }

    /// The policy reference for the coverage vector, if any.
    pub fn policy_ref(&self) -> Option<&NamespacedId> {
        match self {
            Assertion::Authorization { policy, .. } => Some(policy),
            _ => None,
        }
    }

    /// Normalize one wire assertion, or return the typed rejection set.
    pub(crate) fn from_json(json: &Json, role: &str) -> Result<Assertion, DiagnosticSet> {
        let object = json
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("assertion-shape", Some(role)))?;
        let kind = object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("assertion-kind", Some(role)))?;
        for (key, _value) in object {
            let known = match kind {
                "result" => matches!(key.as_str(), "kind" | "valueType" | "value"),
                "error" => matches!(key.as_str(), "kind" | "error" | "payload" | "contract"),
                "entity_state" => matches!(
                    key.as_str(),
                    "kind" | "entity" | "where" | "expect" | "fields"
                ),
                "emitted" => matches!(key.as_str(), "kind" | "target" | "count"),
                "forbidden_effect" => matches!(key.as_str(), "kind" | "effect" | "scope" | "field"),
                "authorization" => matches!(key.as_str(), "kind" | "actor" | "policy" | "outcome"),
                "idempotency" => matches!(
                    key.as_str(),
                    "kind" | "replay" | "equivalence" | "duplicates"
                ),
                "contract_match" => matches!(key.as_str(), "kind" | "contract" | "projection"),
                "deterministic_fixture" => {
                    matches!(key.as_str(), "kind" | "digest" | "clock" | "idSource")
                }
                "unsupported" => matches!(key.as_str(), "kind" | "capability" | "note"),
                _ => return Err(diagnostic::input_invalid("assertion-kind", Some(role))),
            };
            if !known {
                return Err(diagnostic::input_invalid("assertion-shape", Some(role)));
            }
        }
        match kind {
            "result" => {
                let value_type = semantic(object.get("valueType"), "assertion-type", role)?;
                let value = match object.get("value") {
                    None => None,
                    Some(value) => Some(ValueOrRef::from_json(value, role, 0)?),
                };
                Ok(Assertion::Result { value_type, value })
            }
            "error" => {
                let error = semantic(object.get("error"), "assertion-error", role)?;
                let payload = match object.get("payload") {
                    None => Vec::new(),
                    Some(_) => super::precondition::optional_field_map(
                        object.get("payload"),
                        "assertion-payload",
                        role,
                    )?,
                };
                let contract = match object.get("contract") {
                    None => None,
                    Some(_) => Some(super::precondition::namespaced(
                        object.get("contract"),
                        "assertion-contract",
                        role,
                    )?),
                };
                Ok(Assertion::Error {
                    error,
                    payload,
                    contract,
                })
            }
            "entity_state" => {
                let entity = semantic(object.get("entity"), "assertion-entity", role)?;
                let selector = selector_field(object.get("where"), role)?;
                let expect_json = object
                    .get("expect")
                    .ok_or_else(|| diagnostic::input_invalid("assertion-expect", Some(role)))?;
                let expect = match expect_json {
                    Json::Object(map)
                        if map
                            .keys()
                            .map(String::as_str)
                            .collect::<Vec<&str>>()
                            .as_slice()
                            == ["presence"] =>
                    {
                        match map.get("presence").and_then(Json::as_str) {
                            Some("exists") => EntityExpectation::Exists,
                            Some("missing") => EntityExpectation::Missing,
                            _ => {
                                return Err(diagnostic::input_invalid(
                                    "assertion-expect",
                                    Some(role),
                                ))
                            }
                        }
                    }
                    Json::Object(map)
                        if map
                            .keys()
                            .map(String::as_str)
                            .collect::<Vec<&str>>()
                            .as_slice()
                            == ["count"] =>
                    {
                        match map.get("count").and_then(Json::as_u64) {
                            Some(count)
                                if count <= 4096
                                    && !map.get("count").expect("checked").is_f64() =>
                            {
                                EntityExpectation::Count(count as u32)
                            }
                            _ => {
                                return Err(diagnostic::input_invalid(
                                    "assertion-expect",
                                    Some(role),
                                ))
                            }
                        }
                    }
                    _ => {
                        return Err(diagnostic::input_invalid("assertion-expect", Some(role)));
                    }
                };
                let fields = match object.get("fields") {
                    None => Vec::new(),
                    Some(map) => {
                        let raw = map.as_object().ok_or_else(|| {
                            diagnostic::input_invalid("assertion-fields", Some(role))
                        })?;
                        if raw.len() > super::version::MAX_FIELD_ENTRIES {
                            return Err(diagnostic::limit_set("field-entries"));
                        }
                        let mut parsed = Vec::with_capacity(raw.len());
                        for (key, value) in raw {
                            let field = FieldName::parse(key)
                                .map_err(|_| diagnostic::input_invalid("field-name", Some(role)))?;
                            let entry_role = format!("{role}.{key}");
                            let expectation = match value {
                                Json::Object(inner)
                                    if inner
                                        .keys()
                                        .map(String::as_str)
                                        .collect::<Vec<&str>>()
                                        .as_slice()
                                        == ["match"] =>
                                {
                                    let kind = match inner.get("match").and_then(Json::as_str) {
                                        Some("datetime") => MatchKind::Datetime,
                                        Some("uuid") => MatchKind::Uuid,
                                        Some("uri") => MatchKind::Uri,
                                        Some("decimal") => MatchKind::Decimal,
                                        Some("non-null") => MatchKind::NonNull,
                                        _ => {
                                            return Err(diagnostic::input_invalid(
                                                "assertion-fields",
                                                Some(role),
                                            ))
                                        }
                                    };
                                    FieldExpectation::Match(kind)
                                }
                                Json::Object(inner)
                                    if inner
                                        .keys()
                                        .map(String::as_str)
                                        .collect::<Vec<&str>>()
                                        .as_slice()
                                        == ["value"] =>
                                {
                                    FieldExpectation::Value(ValueOrRef::from_json(
                                        inner.get("value").expect("checked"),
                                        &entry_role,
                                        0,
                                    )?)
                                }
                                _ => {
                                    return Err(diagnostic::input_invalid(
                                        "assertion-fields",
                                        Some(role),
                                    ))
                                }
                            };
                            parsed.push((field, expectation));
                        }
                        parsed.sort_by(|left, right| left.0.cmp(&right.0));
                        parsed
                    }
                };
                Ok(Assertion::EntityState {
                    entity,
                    selector,
                    expect,
                    fields,
                })
            }
            "emitted" => {
                let target_object = object
                    .get("target")
                    .and_then(Json::as_object)
                    .ok_or_else(|| diagnostic::input_invalid("assertion-target", Some(role)))?;
                if target_object
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<&str>>()
                    .as_slice()
                    != ["id", "kind"]
                {
                    return Err(diagnostic::input_invalid("assertion-target", Some(role)));
                }
                let kind = match target_object.get("kind").and_then(Json::as_str) {
                    Some("event") => EmitKind::Event,
                    Some("job") => EmitKind::Job,
                    _ => {
                        return Err(diagnostic::input_invalid("assertion-target", Some(role)));
                    }
                };
                let id = semantic(target_object.get("id"), "assertion-target", role)?;
                let count = match object.get("count") {
                    None => None,
                    Some(count_json) => {
                        let map = count_json.as_object().ok_or_else(|| {
                            diagnostic::input_invalid("assertion-count", Some(role))
                        })?;
                        if map
                            .keys()
                            .map(String::as_str)
                            .collect::<Vec<&str>>()
                            .as_slice()
                            == ["atLeast"]
                        {
                            Some(OccurrenceCount::AtLeast(count_value(
                                map.get("atLeast"),
                                role,
                            )?))
                        } else if map
                            .keys()
                            .map(String::as_str)
                            .collect::<Vec<&str>>()
                            .as_slice()
                            == ["exactly"]
                        {
                            Some(OccurrenceCount::Exactly(count_value(
                                map.get("exactly"),
                                role,
                            )?))
                        } else {
                            return Err(diagnostic::input_invalid("assertion-count", Some(role)));
                        }
                    }
                };
                Ok(Assertion::Emitted {
                    target: EmittedTarget { kind, id },
                    count,
                })
            }
            "forbidden_effect" => {
                let effect = semantic(object.get("effect"), "assertion-effect", role)?;
                let scope = match object.get("scope").and_then(Json::as_str) {
                    Some("entity") => EffectScope::Entity,
                    Some("field") => EffectScope::Field,
                    Some("resource") => EffectScope::Resource,
                    _ => {
                        return Err(diagnostic::input_invalid("assertion-scope", Some(role)));
                    }
                };
                let field = match object.get("field") {
                    None => None,
                    Some(field_json) => Some(
                        FieldName::parse(field_json.as_str().ok_or_else(|| {
                            diagnostic::input_invalid("assertion-field", Some(role))
                        })?)
                        .map_err(|_| diagnostic::input_invalid("assertion-field", Some(role)))?,
                    ),
                };
                if (scope == EffectScope::Field) != field.is_some() {
                    return Err(diagnostic::input_invalid("assertion-scope", Some(role)));
                }
                Ok(Assertion::ForbiddenEffect {
                    effect,
                    scope,
                    field,
                })
            }
            "authorization" => {
                let actor =
                    typed_ref(object.get("actor"), RefKind::Actor, "assertion-actor", role)?
                        .ok_or_else(|| diagnostic::input_invalid("assertion-actor", Some(role)))?;
                let policy = super::precondition::namespaced(
                    object.get("policy"),
                    "assertion-policy",
                    role,
                )?;
                let outcome = match object.get("outcome").and_then(Json::as_str) {
                    Some("allowed") => AuthOutcome::Allowed,
                    Some("denied") => AuthOutcome::Denied,
                    _ => {
                        return Err(diagnostic::input_invalid("assertion-outcome", Some(role)));
                    }
                };
                Ok(Assertion::Authorization {
                    actor,
                    policy,
                    outcome,
                })
            }
            "idempotency" => {
                let replay = super::step::StepKey(
                    super::id::StepId::parse(
                        object.get("replay").and_then(Json::as_str).ok_or_else(|| {
                            diagnostic::input_invalid("assertion-replay", Some(role))
                        })?,
                    )
                    .map_err(|_| diagnostic::input_invalid("assertion-replay", Some(role)))?,
                );
                let equivalence = match object.get("equivalence").and_then(Json::as_str) {
                    Some("identical") => Equivalence::Identical,
                    Some("equivalent") => Equivalence::Equivalent,
                    _ => {
                        return Err(diagnostic::input_invalid(
                            "assertion-equivalence",
                            Some(role),
                        ))
                    }
                };
                if object.get("duplicates").and_then(Json::as_str) != Some("none") {
                    return Err(diagnostic::input_invalid(
                        "assertion-duplicates",
                        Some(role),
                    ));
                }
                Ok(Assertion::Idempotency {
                    replay,
                    equivalence,
                })
            }
            "contract_match" => {
                let contract = super::precondition::namespaced(
                    object.get("contract"),
                    "assertion-contract",
                    role,
                )?;
                let projection = match object.get("projection") {
                    None => Vec::new(),
                    Some(Json::Array(items)) => {
                        if items.len() > super::version::MAX_PROJECTION_PATHS {
                            return Err(diagnostic::limit_set("projection-paths"));
                        }
                        let mut paths = Vec::with_capacity(items.len());
                        for item in items {
                            let text = item.as_str().ok_or_else(|| {
                                diagnostic::input_invalid("assertion-projection", Some(role))
                            })?;
                            if !super::reference::member_path_valid(text) {
                                return Err(diagnostic::input_invalid(
                                    "assertion-projection",
                                    Some(role),
                                ));
                            }
                            paths.push(text.to_owned());
                        }
                        if has_duplicates(&paths) {
                            return Err(diagnostic::input_invalid(
                                "duplicate-projection",
                                Some(role),
                            ));
                        }
                        paths.sort();
                        paths
                    }
                    Some(_) => {
                        return Err(diagnostic::input_invalid(
                            "assertion-projection",
                            Some(role),
                        ));
                    }
                };
                Ok(Assertion::ContractMatch {
                    contract,
                    projection,
                })
            }
            "deterministic_fixture" => {
                let digest =
                    Sha256Digest::parse(object.get("digest").and_then(Json::as_str).ok_or_else(
                        || diagnostic::input_invalid("assertion-digest", Some(role)),
                    )?)
                    .map_err(|_| diagnostic::input_invalid("assertion-digest", Some(role)))?;
                let clock =
                    typed_ref(object.get("clock"), RefKind::Clock, "assertion-clock", role)?;
                let id_source = typed_ref(
                    object.get("idSource"),
                    RefKind::IdSource,
                    "assertion-id-source",
                    role,
                )?;
                if clock.is_none() && id_source.is_none() {
                    return Err(diagnostic::input_invalid("assertion-control", Some(role)));
                }
                Ok(Assertion::DeterministicFixture {
                    digest,
                    clock,
                    id_source,
                })
            }
            "unsupported" => {
                let capability = super::precondition::namespaced(
                    object.get("capability"),
                    "assertion-capability",
                    role,
                )?;
                let note = match object.get("note") {
                    None => None,
                    Some(Json::String(text)) => {
                        let characters = text.chars().count();
                        if characters == 0 || characters > 256 || text.chars().any(char::is_control)
                        {
                            return Err(diagnostic::limit_set("note-length"));
                        }
                        Some(text.clone())
                    }
                    Some(_) => {
                        return Err(diagnostic::input_invalid("assertion-note", Some(role)));
                    }
                };
                Ok(Assertion::Unsupported { capability, note })
            }
            _ => Err(diagnostic::input_invalid("assertion-kind", Some(role))),
        }
    }
}

/// Parse one bounded occurrence number.
fn count_value(json: Option<&Json>, role: &str) -> Result<u32, DiagnosticSet> {
    match json {
        Some(value) if !value.is_f64() => value
            .as_u64()
            .filter(|count| (1..=4096).contains(count))
            .map(|count| count as u32)
            .ok_or_else(|| diagnostic::input_invalid("assertion-count", Some(role))),
        _ => Err(diagnostic::input_invalid("assertion-count", Some(role))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_closed_kind_parses_minimally() {
        let cases = [
            json!({"kind": "result", "valueType": "planner.type.task"}),
            json!({"kind": "error", "error": "planner.error.not_found"}),
            json!({
                "kind": "entity_state",
                "entity": "planner.user_task_planning",
                "where": [{"field": "user_id", "equals": {"type": "string", "value": "u1"}}],
                "expect": {"presence": "exists"}
            }),
            json!({
                "kind": "emitted",
                "target": {"kind": "event", "id": "planner.event.focused"}
            }),
            json!({
                "kind": "forbidden_effect",
                "effect": "planner.effect.notify",
                "scope": "entity"
            }),
            json!({
                "kind": "authorization",
                "actor": {"$ref": "actor", "id": "core.actors/admin"},
                "policy": "core.policies/focus",
                "outcome": "denied"
            }),
            json!({
                "kind": "idempotency",
                "replay": "focus",
                "equivalence": "identical",
                "duplicates": "none"
            }),
            json!({"kind": "contract_match", "contract": "core.contracts/planner-output"}),
            json!({
                "kind": "deterministic_fixture",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "clock": {"$ref": "clock", "id": "clock_step"}
            }),
            json!({"kind": "unsupported", "capability": "core.capabilities/parallel"}),
        ];
        for case in cases {
            assert!(
                Assertion::from_json(&case, "t").is_ok(),
                "must accept {case}"
            );
        }
    }

    #[test]
    fn field_scope_requires_the_field_member() {
        let without = json!({
            "kind": "forbidden_effect",
            "effect": "planner.effect.notify",
            "scope": "field"
        });
        assert!(Assertion::from_json(&without, "t").is_err());
        let with = json!({
            "kind": "forbidden_effect",
            "effect": "planner.effect.notify",
            "scope": "field",
            "field": "focused_at"
        });
        assert!(Assertion::from_json(&with, "t").is_ok());
        let extra = json!({
            "kind": "forbidden_effect",
            "effect": "planner.effect.notify",
            "scope": "entity",
            "field": "focused_at"
        });
        assert!(Assertion::from_json(&extra, "t").is_err());
    }

    #[test]
    fn deterministic_fixtures_require_a_control_reference() {
        let bare = json!({
            "kind": "deterministic_fixture",
            "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        });
        assert!(Assertion::from_json(&bare, "t").is_err());
    }

    #[test]
    fn unknown_kinds_and_members_are_rejected() {
        assert!(
            Assertion::from_json(&json!({"kind": "latency_under_ms", "budget": 50}), "t").is_err()
        );
        assert!(Assertion::from_json(
            &json!({"kind": "result", "valueType": "planner.type.task", "sql": "x"}),
            "t"
        )
        .is_err());
    }
}
