//! Canonical serialization of the Scenario IR (issue #23).
//!
//! Compact UTF-8 JSON with no insignificant whitespace. Every object's
//! members are written in unsigned UTF-8 byte order of their keys;
//! set-like collections (tags, binding capabilities, contract-match
//! projections, bindings) are written sorted; behaviorally ordered
//! collections (`given`, `when`, `then`, selectors) are written in their
//! exact scenario order — canonicalization never sorts away behavior.
//! The bytes are path-independent: no physical root, raw source,
//! timestamp, host, locale, runtime value, or adapter transcript ever
//! enters them, and the writer appends no trailing LF.

use super::action::InvokeAction;
use super::assertion::{Assertion, FieldExpectation};
use super::binding::{Backend, Binding, Mode};
use super::coverage::CoverageVector;
use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::precondition::{FieldPredicate, Precondition};
use super::reference::{Ref, RefTarget};
use super::source_map::SourceMapRef;
use super::step::{GivenStep, ReplayMeta, ThenStep, ValueOrRef, WhenStep};
use super::value::TypedValue;
use super::{MetadataValue, ScenarioIr};

/// Serialize one whole scenario to canonical bytes, or refuse beyond the
/// payload bound.
pub fn scenario_bytes(ir: &ScenarioIr) -> Result<String, DiagnosticSet> {
    let bytes = scenario_payload(ir);
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The canonical bytes of the coverage vector.
pub fn coverage_bytes(coverage: &CoverageVector) -> String {
    let entries: Vec<String> = coverage
        .entries
        .iter()
        .map(|entry| {
            object(vec![
                ("assertionKind", string(entry.assertion_kind)),
                (
                    "effect",
                    optional(entry.effect.as_ref().map(|id| string(id.as_str()))),
                ),
                (
                    "error",
                    optional(entry.error.as_ref().map(|id| string(id.as_str()))),
                ),
                ("observes", string(entry.observes.as_str())),
                (
                    "operation",
                    optional(entry.operation.as_ref().map(|id| string(id.as_str()))),
                ),
                ("outcome", string(entry.outcome.as_str())),
                (
                    "policy",
                    optional(entry.policy.as_ref().map(|id| string(id.as_str()))),
                ),
                ("stepId", string(entry.step_id.as_str())),
            ])
        })
        .collect();
    object(vec![
        ("entries", array(&entries)),
        (
            "operations",
            array(
                &coverage
                    .operations
                    .iter()
                    .map(|id| string(id.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        ("scenarioId", string(coverage.scenario_id.as_str())),
    ])
}

/// The canonical payload of one scenario.
fn scenario_payload(ir: &ScenarioIr) -> String {
    let bindings: Vec<String> = ir.bindings.iter().map(binding_bytes).collect();
    let tags: Vec<String> = ir.tags.iter().map(|tag| string(tag.as_str())).collect();
    let metadata: Vec<String> = ir
        .metadata
        .iter()
        .map(|(key, value)| format!("{}:{}", string(key), metadata_value_bytes(value)))
        .collect();
    object(vec![
        ("bindings", array(&bindings)),
        (
            "given",
            array(&ir.given.iter().map(given_bytes).collect::<Vec<String>>()),
        ),
        ("identity", string(super::version::IDENTITY)),
        (
            "irRef",
            object(vec![
                ("digest", string(ir.ir_ref.digest.as_str())),
                ("identity", string(super::version::IR_IDENTITY)),
            ]),
        ),
        ("metadata", raw(&format!("{{{}}}", metadata.join(",")))),
        (
            "modelRef",
            object(vec![
                ("digest", string(ir.model_ref.digest.as_str())),
                ("modelVersion", string(ir.model_ref.version_text())),
            ]),
        ),
        ("projectId", string(ir.project_id.as_str())),
        ("scenarioId", string(ir.scenario_id.as_str())),
        ("scenarioVersion", string(ir.scenario_version.as_str())),
        ("schemaVersion", string(super::version::SCHEMA_VERSION)),
        (
            "sourceMapRef",
            optional(ir.source_map.as_ref().map(source_map_bytes)),
        ),
        ("summary", string(&ir.summary)),
        ("tags", array(&tags)),
        (
            "then",
            array(&ir.then.iter().map(then_bytes).collect::<Vec<String>>()),
        ),
        (
            "when",
            array(&ir.when.iter().map(when_bytes).collect::<Vec<String>>()),
        ),
    ])
}

/// The canonical bytes of one `given` step.
fn given_bytes(step: &GivenStep) -> String {
    object(vec![
        ("precondition", precondition_bytes(&step.precondition)),
        ("stepId", string(step.step_id.as_str())),
    ])
}

/// The canonical bytes of one precondition.
fn precondition_bytes(precondition: &Precondition) -> String {
    match precondition {
        Precondition::State {
            entity,
            selector,
            fields,
        } => object(vec![
            ("entity", string(entity.as_str())),
            ("fields", field_map_bytes(fields)),
            ("kind", string("state")),
            (
                "selector",
                array(
                    &selector
                        .iter()
                        .map(predicate_bytes)
                        .collect::<Vec<String>>(),
                ),
            ),
        ]),
        Precondition::Fixture {
            fixture,
            version,
            capabilities,
        } => object(vec![
            (
                "capabilities",
                array(
                    &capabilities
                        .iter()
                        .map(|id| string(id.as_str()))
                        .collect::<Vec<String>>(),
                ),
            ),
            ("fixture", string(fixture.as_str())),
            ("kind", string("fixture")),
            ("version", string(version.as_str())),
        ]),
        Precondition::Actor { actor, scope } => object(vec![
            ("actor", string(actor.as_str())),
            ("kind", string("actor")),
            (
                "scope",
                optional(scope.as_ref().map(|id| string(id.as_str()))),
            ),
        ]),
        Precondition::Clock { at } => object(vec![
            (
                "at",
                object(vec![("type", string("datetime")), ("value", string(at))]),
            ),
            ("kind", string("clock")),
        ]),
        Precondition::IdSource { seed, algorithm } => object(vec![
            ("algorithm", string(algorithm.as_str())),
            ("kind", string("id_source")),
            ("seed", string(seed)),
        ]),
    }
}

/// The canonical bytes of one selector predicate.
fn predicate_bytes(term: &FieldPredicate) -> String {
    object(vec![
        ("equals", leaf_bytes(&term.equals)),
        ("field", string(term.field.as_str())),
    ])
}

/// The canonical bytes of one `when` step.
fn when_bytes(step: &WhenStep) -> String {
    object(vec![
        ("action", invoke_bytes(&step.action)),
        ("replay", optional(step.replay.as_ref().map(replay_bytes))),
        ("stepId", string(step.step_id.as_str())),
    ])
}

/// The canonical bytes of one replay meta.
fn replay_bytes(replay: &ReplayMeta) -> String {
    object(vec![
        ("expect", string(replay.expect.as_str())),
        ("of", string(replay.of.as_str())),
    ])
}

/// The canonical bytes of one invoke action.
fn invoke_bytes(action: &InvokeAction) -> String {
    object(vec![
        ("actor", optional(action.actor.as_ref().map(ref_bytes))),
        ("clock", optional(action.clock.as_ref().map(ref_bytes))),
        (
            "idempotencyKey",
            optional(action.idempotency_key.as_ref().map(leaf_bytes)),
        ),
        ("input", field_map_bytes(&action.input)),
        ("kind", string("invoke")),
        ("operation", string(action.operation.as_str())),
    ])
}

/// The canonical bytes of one `then` step.
fn then_bytes(step: &ThenStep) -> String {
    object(vec![
        ("assertion", assertion_bytes(&step.assertion)),
        ("observes", string(step.observes.as_str())),
        ("stepId", string(step.step_id.as_str())),
    ])
}

/// The canonical bytes of one assertion.
fn assertion_bytes(assertion: &Assertion) -> String {
    match assertion {
        Assertion::Result { value_type, value } => object(vec![
            ("kind", string("result")),
            ("value", optional(value.as_ref().map(leaf_bytes))),
            ("valueType", string(value_type.as_str())),
        ]),
        Assertion::Error {
            error,
            payload,
            contract,
        } => object(vec![
            (
                "contract",
                optional(contract.as_ref().map(|id| string(id.as_str()))),
            ),
            ("error", string(error.as_str())),
            ("kind", string("error")),
            ("payload", field_map_bytes(payload)),
        ]),
        Assertion::EntityState {
            entity,
            selector,
            expect,
            fields,
        } => {
            let expectation = match expect {
                super::assertion::EntityExpectation::Exists => {
                    object(vec![("presence", string("exists"))])
                }
                super::assertion::EntityExpectation::Missing => {
                    object(vec![("presence", string("missing"))])
                }
                super::assertion::EntityExpectation::Count(count) => {
                    object(vec![("count", number(*count as i64))])
                }
            };
            let field_bytes: Vec<String> = fields
                .iter()
                .map(|(field, expectation)| {
                    let leaf = match expectation {
                        FieldExpectation::Value(leaf) => object(vec![("value", leaf_bytes(leaf))]),
                        FieldExpectation::Match(kind) => object(vec![(
                            "match",
                            string(match kind {
                                super::assertion::MatchKind::Datetime => "datetime",
                                super::assertion::MatchKind::Uuid => "uuid",
                                super::assertion::MatchKind::Uri => "uri",
                                super::assertion::MatchKind::Decimal => "decimal",
                                super::assertion::MatchKind::NonNull => "non-null",
                            }),
                        )]),
                    };
                    format!("{}:{}", string(field.as_str()), leaf)
                })
                .collect();
            object(vec![
                ("entity", string(entity.as_str())),
                ("expect", expectation),
                ("fields", raw(&format!("{{{}}}", field_bytes.join(",")))),
                ("kind", string("entity_state")),
                (
                    "where",
                    array(
                        &selector
                            .iter()
                            .map(predicate_bytes)
                            .collect::<Vec<String>>(),
                    ),
                ),
            ])
        }
        Assertion::Emitted { target, count } => {
            let occurrence = count.map(|occurrence| match occurrence {
                super::assertion::OccurrenceCount::AtLeast(least) => {
                    object(vec![("atLeast", number(i64::from(least)))])
                }
                super::assertion::OccurrenceCount::Exactly(exactly) => {
                    object(vec![("exactly", number(i64::from(exactly)))])
                }
            });
            object(vec![
                ("count", optional(occurrence)),
                ("kind", string("emitted")),
                (
                    "target",
                    object(vec![
                        ("id", string(target.id.as_str())),
                        (
                            "kind",
                            string(match target.kind {
                                super::assertion::EmitKind::Event => "event",
                                super::assertion::EmitKind::Job => "job",
                            }),
                        ),
                    ]),
                ),
            ])
        }
        Assertion::ForbiddenEffect {
            effect,
            scope,
            field,
        } => object(vec![
            ("effect", string(effect.as_str())),
            (
                "field",
                optional(field.as_ref().map(|name| string(name.as_str()))),
            ),
            ("kind", string("forbidden_effect")),
            (
                "scope",
                string(match scope {
                    super::assertion::EffectScope::Entity => "entity",
                    super::assertion::EffectScope::Field => "field",
                    super::assertion::EffectScope::Resource => "resource",
                }),
            ),
        ]),
        Assertion::Authorization {
            actor,
            policy,
            outcome,
        } => object(vec![
            ("actor", ref_bytes(actor)),
            ("kind", string("authorization")),
            (
                "outcome",
                string(match outcome {
                    super::assertion::AuthOutcome::Allowed => "allowed",
                    super::assertion::AuthOutcome::Denied => "denied",
                }),
            ),
            ("policy", string(policy.as_str())),
        ]),
        Assertion::Idempotency {
            replay,
            equivalence,
        } => object(vec![
            ("duplicates", string("none")),
            ("equivalence", string(equivalence.as_str())),
            ("kind", string("idempotency")),
            ("replay", string(replay.as_str())),
        ]),
        Assertion::ContractMatch {
            contract,
            projection,
        } => object(vec![
            ("contract", string(contract.as_str())),
            ("kind", string("contract_match")),
            (
                "projection",
                array(
                    &projection
                        .iter()
                        .map(|path| string(path))
                        .collect::<Vec<String>>(),
                ),
            ),
        ]),
        Assertion::DeterministicFixture {
            digest,
            clock,
            id_source,
        } => object(vec![
            ("clock", optional(clock.as_ref().map(ref_bytes))),
            ("digest", string(digest.as_str())),
            ("idSource", optional(id_source.as_ref().map(ref_bytes))),
            ("kind", string("deterministic_fixture")),
        ]),
        Assertion::Unsupported { capability, note } => object(vec![
            ("capability", string(capability.as_str())),
            ("kind", string("unsupported")),
            ("note", optional(note.as_deref().map(string))),
        ]),
    }
}

/// The canonical bytes of one typed backend binding.
fn binding_bytes(binding: &Binding) -> String {
    object(vec![
        (
            "backend",
            string(match binding.backend {
                Backend::FakeReference => "fake-reference",
                Backend::Native => "native",
            }),
        ),
        (
            "capabilities",
            array(
                &binding
                    .capabilities
                    .iter()
                    .map(|id| string(id.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "capabilityDigest",
            string(binding.capability_digest.as_str()),
        ),
        (
            "evidenceDigest",
            optional(
                binding
                    .evidence_digest
                    .as_ref()
                    .map(|digest| string(digest.as_str())),
            ),
        ),
        (
            "mode",
            optional(binding.mode.as_ref().map(|mode| {
                string(match mode {
                    Mode::Generated => "generated",
                    Mode::Scaffolded => "scaffolded",
                    Mode::Checked => "checked",
                })
            })),
        ),
        (
            "profile",
            optional(binding.profile.as_ref().map(|pin| {
                object(vec![
                    ("id", string(pin.id.as_str())),
                    ("version", string(pin.version.as_str())),
                ])
            })),
        ),
        (
            "protocol",
            optional(binding.protocol.as_ref().map(|pin| {
                object(vec![
                    ("id", string(pin.id.as_str())),
                    ("version", string(pin.version.as_str())),
                ])
            })),
        ),
        ("runner", string(binding.runner.as_str())),
        ("runnerVersion", string(binding.runner_version.as_str())),
        ("test", string(binding.test.as_str())),
    ])
}

/// The canonical bytes of one source-map reference.
fn source_map_bytes(reference: &SourceMapRef) -> String {
    object(vec![
        ("digest", string(reference.digest.as_str())),
        ("entries", number(i64::from(reference.entries))),
        ("identity", string(super::version::SOURCE_MAP_IDENTITY)),
    ])
}

/// The canonical bytes of one typed value or reference leaf.
pub(crate) fn leaf_bytes(leaf: &ValueOrRef) -> String {
    match leaf {
        ValueOrRef::Value(value) => typed_value_bytes(value),
        ValueOrRef::Reference(reference) => ref_bytes(reference),
    }
}

/// The canonical bytes of one typed value.
fn typed_value_bytes(value: &TypedValue) -> String {
    match value {
        TypedValue::Null => object(vec![("type", string("null")), ("value", raw("null"))]),
        TypedValue::Boolean(flag) => object(vec![
            ("type", string("boolean")),
            ("value", raw(if *flag { "true" } else { "false" })),
        ]),
        TypedValue::Integer(value) => {
            object(vec![("type", string("integer")), ("value", number(*value))])
        }
        TypedValue::String(text) => {
            object(vec![("type", string("string")), ("value", string(text))])
        }
        TypedValue::Decimal(text) => {
            object(vec![("type", string("decimal")), ("value", string(text))])
        }
        TypedValue::Date(text) => object(vec![("type", string("date")), ("value", string(text))]),
        TypedValue::Datetime(text) => {
            object(vec![("type", string("datetime")), ("value", string(text))])
        }
        TypedValue::Uuid(text) => object(vec![("type", string("uuid")), ("value", string(text))]),
        TypedValue::Uri(text) => object(vec![("type", string("uri")), ("value", string(text))]),
        TypedValue::List(items) => object(vec![
            ("type", string("list")),
            (
                "value",
                array(&items.iter().map(typed_value_bytes).collect::<Vec<String>>()),
            ),
        ]),
        TypedValue::Object(entries) => {
            let members: Vec<String> = entries
                .iter()
                .map(|(field, value)| {
                    format!("{}:{}", string(field.as_str()), typed_value_bytes(value))
                })
                .collect();
            object(vec![
                ("type", string("object")),
                ("value", raw(&format!("{{{}}}", members.join(",")))),
            ])
        }
    }
}

/// The canonical bytes of one reference.
fn ref_bytes(reference: &Ref) -> String {
    object(vec![
        ("$ref", string(reference.kind.as_str())),
        (
            "id",
            string(match &reference.id {
                RefTarget::Semantic(id) => id.as_str(),
                RefTarget::Namespaced(id) => id.as_str(),
                RefTarget::Step(step) => step.as_str(),
            }),
        ),
        ("path", optional(reference.path.as_deref().map(string))),
        (
            "type",
            optional(
                reference
                    .expected_type
                    .as_ref()
                    .map(|id| string(id.as_str())),
            ),
        ),
    ])
}

/// The canonical bytes of one sorted explicit field map.
fn field_map_bytes(fields: &[(super::id::FieldName, ValueOrRef)]) -> String {
    let members: Vec<String> = fields
        .iter()
        .map(|(field, leaf)| format!("{}:{}", string(field.as_str()), leaf_bytes(leaf)))
        .collect();
    raw(&format!("{{{}}}", members.join(",")))
}

/// The canonical bytes of one metadata value.
fn metadata_value_bytes(value: &MetadataValue) -> String {
    match value {
        MetadataValue::Null => raw("null"),
        MetadataValue::Boolean(flag) => raw(if *flag { "true" } else { "false" }),
        MetadataValue::Integer(raw_number) => number(*raw_number),
        MetadataValue::String(text) => string(text),
    }
}

/// One JSON object assembled from already-canonical members; absent
/// optional members contribute nothing.
fn object(members: Vec<(&str, String)>) -> String {
    let joined: Vec<String> = members
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| format!("{}:{}", string(key), value))
        .collect();
    format!("{{{}}}", joined.join(","))
}

/// One JSON array assembled from already-canonical members.
fn array(members: &[String]) -> String {
    format!("[{}]", members.join(","))
}

/// One absent optional member (the empty string contributes nothing).
fn optional(member: Option<String>) -> String {
    member.unwrap_or_default()
}

/// Pre-serialized JSON content passed through verbatim.
fn raw(text: &str) -> String {
    text.to_owned()
}

/// One JSON string literal with minimal canonical escaping.
fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// One JSON number.
fn number(value: i64) -> String {
    value.to_string()
}

/// The canonical bytes of the sorted namespaced metadata map.
pub(crate) fn metadata_bytes(metadata: &[(String, MetadataValue)]) -> String {
    let members: Vec<String> = metadata
        .iter()
        .map(|(key, value)| format!("{}:{}", string(key), metadata_value_bytes(value)))
        .collect();
    format!("{{{}}}", members.join(","))
}

/// The canonical sort key of one binding: its exact canonical bytes.
pub(crate) fn binding_sort_key(binding: &Binding) -> String {
    binding_bytes(binding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_literals_escape_minimally_and_deterministically() {
        assert_eq!(string("plain"), "\"plain\"");
        assert_eq!(string("a\"b\\c\nd\te"), "\"a\\\"b\\\\c\\nd\\te\"");
        assert_eq!(string("\u{1}"), "\"\\u0001\"");
        assert_eq!(string("привет"), "\"привет\"");
    }

    #[test]
    fn absent_optional_members_are_skipped() {
        let bytes = object(vec![
            ("a", string("1")),
            ("gone", optional(None)),
            ("c", string("3")),
        ]);
        assert_eq!(bytes, "{\"a\":\"1\",\"c\":\"3\"}");
    }

    #[test]
    fn raw_literals_pass_through_verbatim() {
        assert_eq!(raw("null"), "null");
        assert_eq!(number(-3), "-3");
    }
}
