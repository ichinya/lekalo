//! Given/precondition shapes of the Scenario IR (issue #23).
//!
//! A precondition establishes data and controls only: typed entity state
//! (selectors plus field values), an opaque fixture reference with its
//! own version and capabilities, a typed actor identity, a deterministic
//! clock, or a deterministic ID source. No arbitrary expressions,
//! executable code, SQL, network fixtures, or target framework names
//! exist on the wire. Entity-level selectors and field-level values are
//! explicit everywhere; an empty selector or field map is invalid, and
//! bare field names (never dotted paths) make entity and field scope
//! confusion structurally impossible.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::id::{FieldName, NamespacedId, SemanticId};
use super::step::ValueOrRef;
use super::value;

/// One closed `given` precondition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Precondition {
    /// Typed entity state: a selector plus explicit field values.
    State {
        /// The qualified entity the state belongs to.
        entity: SemanticId,
        /// The ordered row selector; every term pins one field.
        selector: Vec<FieldPredicate>,
        /// The field values to establish, sorted by field name.
        fields: Vec<(FieldName, ValueOrRef)>,
    },
    /// A reusable fixture, fully visible by reference.
    Fixture {
        /// The namespaced fixture identity.
        fixture: NamespacedId,
        /// The exact fixture contract version.
        version: crate::lockfile::types::SemVer,
        /// The sorted capability set the fixture requires.
        capabilities: Vec<NamespacedId>,
    },
    /// A typed actor identity with an optional scope.
    Actor {
        /// The namespaced actor identity.
        actor: NamespacedId,
        /// The optional namespaced scope the actor acts in.
        scope: Option<NamespacedId>,
    },
    /// One deterministic clock value.
    Clock {
        /// The canonical UTC datetime the clock is fixed at.
        at: String,
    },
    /// One deterministic ID source.
    IdSource {
        /// The bounded seed text.
        seed: String,
        /// The closed derivation algorithm.
        algorithm: IdAlgorithm,
    },
}

/// One selector term: a bare field name pinned to one typed value or
/// reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldPredicate {
    /// The bare field name (same entity as the precondition).
    pub field: FieldName,
    /// The pinned value.
    pub equals: ValueOrRef,
}

/// The closed deterministic ID derivation vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdAlgorithm {
    /// Deterministic UUIDv4 derivation from the seed.
    UuidV4,
    /// Deterministic sequence numbering from the seed.
    Sequence,
}

impl IdAlgorithm {
    /// The wire tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            IdAlgorithm::UuidV4 => "uuidv4",
            IdAlgorithm::Sequence => "sequence",
        }
    }
}

impl Precondition {
    /// Normalize one wire precondition, or return the typed rejection
    /// set. `role` locates the precondition.
    pub(crate) fn from_json(json: &Json, role: &str) -> Result<Precondition, DiagnosticSet> {
        let object = json
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("precondition-shape", Some(role)))?;
        let kind = object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("precondition-kind", Some(role)))?;
        match kind {
            "state" => {
                if object
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<&str>>()
                    .as_slice()
                    != ["entity", "fields", "kind", "selector"]
                {
                    return Err(diagnostic::input_invalid("precondition-shape", Some(role)));
                }
                let entity = semantic(object.get("entity"), "precondition-entity", role)?;
                let selector = selector_field(object.get("selector"), role)?;
                let fields = field_map(object.get("fields"), "precondition-fields", role)?;
                Ok(Precondition::State {
                    entity,
                    selector,
                    fields,
                })
            }
            "fixture" => {
                if !matches!(
                    object
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<&str>>()
                        .as_slice(),
                    ["fixture", "kind", "version"] | ["capabilities", "fixture", "kind", "version"]
                ) {
                    return Err(diagnostic::input_invalid("precondition-shape", Some(role)));
                }
                let fixture = namespaced(object.get("fixture"), "precondition-fixture", role)?;
                let version = crate::lockfile::types::SemVer::parse(
                    object
                        .get("version")
                        .and_then(Json::as_str)
                        .ok_or_else(|| {
                            diagnostic::input_invalid("precondition-version", Some(role))
                        })?,
                )
                .map_err(|_| diagnostic::input_invalid("precondition-version", Some(role)))?;
                let capabilities = match object.get("capabilities") {
                    None => Vec::new(),
                    Some(Json::Array(items)) => {
                        if items.len() > super::version::MAX_CAPABILITIES {
                            return Err(diagnostic::limit_set("capabilities"));
                        }
                        let mut parsed = Vec::with_capacity(items.len());
                        for item in items {
                            parsed.push(namespaced(Some(item), "precondition-capability", role)?);
                        }
                        if has_duplicates(&parsed) {
                            return Err(diagnostic::input_invalid(
                                "duplicate-capability",
                                Some(role),
                            ));
                        }
                        parsed.sort();
                        parsed
                    }
                    Some(_) => {
                        return Err(diagnostic::input_invalid(
                            "precondition-capability",
                            Some(role),
                        ))
                    }
                };
                Ok(Precondition::Fixture {
                    fixture,
                    version,
                    capabilities,
                })
            }
            "actor" => {
                if !matches!(
                    object
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<&str>>()
                        .as_slice(),
                    ["actor", "kind"] | ["actor", "kind", "scope"]
                ) {
                    return Err(diagnostic::input_invalid("precondition-shape", Some(role)));
                }
                let actor = namespaced(object.get("actor"), "precondition-actor", role)?;
                let scope = match object.get("scope") {
                    None => None,
                    Some(_) => Some(namespaced(object.get("scope"), "precondition-scope", role)?),
                };
                Ok(Precondition::Actor { actor, scope })
            }
            "clock" => {
                if object
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<&str>>()
                    .as_slice()
                    != ["at", "kind"]
                {
                    return Err(diagnostic::input_invalid("precondition-shape", Some(role)));
                }
                let at = object
                    .get("at")
                    .and_then(|at| at.get("value"))
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid("precondition-clock", Some(role)))?;
                if object
                    .get("at")
                    .and_then(|at| at.get("type"))
                    .and_then(Json::as_str)
                    != Some("datetime")
                {
                    return Err(diagnostic::input_invalid("precondition-clock", Some(role)));
                }
                if !value::canonical_datetime(at) {
                    return Err(diagnostic::input_invalid("precondition-clock", Some(role)));
                }
                Ok(Precondition::Clock { at: at.to_owned() })
            }
            "id_source" => {
                if object
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<&str>>()
                    .as_slice()
                    != ["algorithm", "kind", "seed"]
                {
                    return Err(diagnostic::input_invalid("precondition-shape", Some(role)));
                }
                let seed = object
                    .get("seed")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid("precondition-seed", Some(role)))?;
                if seed.is_empty()
                    || seed.chars().count() > 128
                    || seed.chars().any(char::is_control)
                {
                    return Err(diagnostic::limit_set("seed-length"));
                }
                let algorithm = match object.get("algorithm").and_then(Json::as_str) {
                    Some("uuidv4") => IdAlgorithm::UuidV4,
                    Some("sequence") => IdAlgorithm::Sequence,
                    _ => {
                        return Err(diagnostic::input_invalid(
                            "precondition-algorithm",
                            Some(role),
                        ))
                    }
                };
                Ok(Precondition::IdSource {
                    seed: seed.to_owned(),
                    algorithm,
                })
            }
            _ => Err(diagnostic::input_invalid("precondition-kind", Some(role))),
        }
    }

    /// The actor identity if this is an actor precondition.
    pub fn actor_id(&self) -> Option<&NamespacedId> {
        match self {
            Precondition::Actor { actor, .. } => Some(actor),
            _ => None,
        }
    }

    /// Whether this is a clock precondition.
    pub fn is_clock(&self) -> bool {
        matches!(self, Precondition::Clock { .. })
    }

    /// Whether this is an ID source precondition.
    pub fn is_id_source(&self) -> bool {
        matches!(self, Precondition::IdSource { .. })
    }

    /// The conflict identity of a state precondition: the entity plus
    /// its selector terms with their exact pinned values (the row the
    /// setup claims) and the field names it establishes.
    pub(crate) fn state_setup(&self) -> Option<(&SemanticId, Vec<String>, Vec<&str>)> {
        match self {
            Precondition::State {
                entity,
                selector,
                fields,
            } => {
                let mut terms: Vec<String> = selector
                    .iter()
                    .map(|term| {
                        format!(
                            "{}={}",
                            term.field.as_str(),
                            super::canonical::leaf_bytes(&term.equals)
                        )
                    })
                    .collect();
                terms.sort_unstable();
                let mut names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
                names.sort_unstable();
                Some((entity, terms, names))
            }
            _ => None,
        }
    }
}

/// Parse one selector array of field predicates.
pub(crate) fn selector_field(
    json: Option<&Json>,
    role: &str,
) -> Result<Vec<FieldPredicate>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("selector-shape", Some(role)))?;
    if items.is_empty() || items.len() > super::version::MAX_SELECTOR_TERMS {
        return Err(diagnostic::limit_set("selector-terms"));
    }
    let mut selector = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("selector-shape", Some(role)))?;
        if object
            .keys()
            .map(String::as_str)
            .collect::<Vec<&str>>()
            .as_slice()
            != ["equals", "field"]
        {
            return Err(diagnostic::input_invalid("selector-shape", Some(role)));
        }
        let field = FieldName::parse(
            object
                .get("field")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("selector-field", Some(role)))?,
        )
        .map_err(|_| diagnostic::input_invalid("selector-field", Some(role)))?;
        let equals = ValueOrRef::from_json(object.get("equals").expect("checked"), role, 0)?;
        selector.push(FieldPredicate { field, equals });
    }
    Ok(selector)
}

/// Parse one explicit field map (at least one entry, bounded).
pub(crate) fn field_map(
    json: Option<&Json>,
    detail: &'static str,
    role: &str,
) -> Result<Vec<(FieldName, ValueOrRef)>, DiagnosticSet> {
    let map = json
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid(detail, Some(role)))?;
    if map.is_empty() || map.len() > super::version::MAX_FIELD_ENTRIES {
        return Err(diagnostic::limit_set("field-entries"));
    }
    let mut entries = Vec::with_capacity(map.len());
    for (key, value) in map {
        let field = FieldName::parse(key)
            .map_err(|_| diagnostic::input_invalid("field-name", Some(role)))?;
        let entry_role = format!("{role}.{key}");
        entries.push((field, ValueOrRef::from_json(value, &entry_role, 0)?));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries)
}

/// Parse one optional field map (possibly empty, bounded).
pub(crate) fn optional_field_map(
    json: Option<&Json>,
    detail: &'static str,
    role: &str,
) -> Result<Vec<(FieldName, ValueOrRef)>, DiagnosticSet> {
    match json {
        Some(Json::Object(map)) if map.is_empty() => Ok(Vec::new()),
        other => field_map(other, detail, role),
    }
}

/// Parse one required semantic identifier member.
pub(crate) fn semantic(
    json: Option<&Json>,
    detail: &'static str,
    role: &str,
) -> Result<SemanticId, DiagnosticSet> {
    SemanticId::parse(
        json.and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid(detail, Some(role)))?,
    )
    .map_err(|_| diagnostic::input_invalid(detail, Some(role)))
}

/// Parse one required namespaced identifier member.
pub(crate) fn namespaced(
    json: Option<&Json>,
    detail: &'static str,
    role: &str,
) -> Result<NamespacedId, DiagnosticSet> {
    NamespacedId::parse(
        json.and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid(detail, Some(role)))?,
    )
    .map_err(|_| diagnostic::input_invalid(detail, Some(role)))
}

/// Whether a sorted-then-compared list holds equal members.
pub(crate) fn has_duplicates<T: Ord>(items: &[T]) -> bool {
    let mut sorted: Vec<&T> = items.iter().collect();
    sorted.sort();
    sorted.windows(2).any(|pair| pair[0] == pair[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn state_preconditions_parse_and_sort_fields() {
        let precondition = Precondition::from_json(
            &json!({
                "kind": "state",
                "entity": "planner.user_task_planning",
                "selector": [{"field": "user_id", "equals": {"type": "string", "value": "user-1"}}],
                "fields": {
                    "focused_at": {"type": "null", "value": null},
                    "task_id": {"type": "string", "value": "task-1"}
                }
            }),
            "g",
        )
        .expect("state precondition");
        match precondition {
            Precondition::State { entity, fields, .. } => {
                assert_eq!(entity.as_str(), "planner.user_task_planning");
                assert_eq!(fields[0].0.as_str(), "focused_at");
                assert_eq!(fields[1].0.as_str(), "task_id");
            }
            _ => panic!("state expected"),
        }
    }

    #[test]
    fn empty_selectors_or_fields_and_dotted_fields_are_rejected() {
        let base = json!({
            "kind": "state",
            "entity": "planner.user_task_planning",
            "selector": [{"field": "user_id", "equals": {"type": "string", "value": "u"}}],
            "fields": {"focused_at": {"type": "null", "value": null}}
        });
        assert!(Precondition::from_json(&base, "g").is_ok());
        let empty_fields = json!({
            "kind": "state",
            "entity": "planner.user_task_planning",
            "selector": [{"field": "user_id", "equals": {"type": "string", "value": "u"}}],
            "fields": {}
        });
        assert!(Precondition::from_json(&empty_fields, "g").is_err());
        let dotted = json!({
            "kind": "state",
            "entity": "planner.user_task_planning",
            "selector": [{"field": "other.entity.field", "equals": {"type": "string", "value": "u"}}],
            "fields": {"focused_at": {"type": "null", "value": null}}
        });
        assert!(Precondition::from_json(&dotted, "g").is_err());
    }

    #[test]
    fn clock_preconditions_require_canonical_utc() {
        let good =
            json!({"kind": "clock", "at": {"type": "datetime", "value": "2026-01-02T03:04:05Z"}});
        assert!(Precondition::from_json(&good, "g").is_ok());
        let offset = json!({"kind": "clock", "at": {"type": "datetime", "value": "2026-01-02T03:04:05+01:00"}});
        assert!(Precondition::from_json(&offset, "g").is_err());
    }

    #[test]
    fn id_sources_bound_their_seed_and_algorithm() {
        let good = json!({"kind": "id_source", "seed": "planner-1", "algorithm": "sequence"});
        assert!(Precondition::from_json(&good, "g").is_ok());
        let bad_algorithm =
            json!({"kind": "id_source", "seed": "planner-1", "algorithm": "random"});
        assert!(Precondition::from_json(&bad_algorithm, "g").is_err());
        let long_seed = format!("s{}", "e".repeat(200));
        let long = json!({"kind": "id_source", "seed": long_seed, "algorithm": "uuidv4"});
        assert!(Precondition::from_json(&long, "g").is_err());
    }
}
