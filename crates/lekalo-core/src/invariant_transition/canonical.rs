//! Canonical serialization of the invariant-transition attachment
//! (issue #63).
//!
//! Canonical bytes are compact UTF-8 JSON with byte-sorted object keys,
//! no BOM, and no trailing LF; absent optional members are dropped
//! entirely. Set-like collections (state spaces, states, invariants,
//! transitions, mappings, hints, reference lists, allowed states,
//! from-state sets) normalize to unsigned UTF-8-byte order; assignment
//! sets keep their declared behavioral order while their fields stay
//! unique. The output is path-independent and byte-identical for
//! value-equal attachments.

/// The canonical payload bytes, or the typed over-bound refusal.
pub fn attachment_bytes(
    attachment: &super::InvariantTransitionAttachment,
) -> Result<String, DiagnosticSet> {
    let bytes = attachment_payload(attachment);
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(super::diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

use crate::diagnostics::DiagnosticSet;

use super::expr::PredicateNode;
use super::invariant::FieldRef;
use super::state::ValueNode;
use super::transition::AssignmentValue;

/// The full canonical attachment payload.
fn attachment_payload(attachment: &super::InvariantTransitionAttachment) -> String {
    let mut state_spaces: Vec<&super::state::StateSpace> =
        attachment.state_spaces().iter().collect();
    state_spaces.sort_by_key(|space| space.state_space_id().as_str());
    let mut invariants: Vec<&super::invariant::Invariant> =
        attachment.invariants().iter().collect();
    invariants.sort_by_key(|invariant| invariant.invariant_id().as_str());
    let mut transitions: Vec<&super::transition::Transition> =
        attachment.transitions().iter().collect();
    transitions.sort_by_key(|transition| transition.transition_id().as_str());
    let mut mappings: Vec<&super::trace::VerificationMapping> =
        attachment.verification_mappings().iter().collect();
    mappings.sort_by_key(|mapping| mapping.mapping_id().as_str());
    let mut hints: Vec<&super::trace::PropertyHint> = attachment.property_hints().iter().collect();
    hints.sort_by_key(|hint| hint.hint_id().as_str());
    object(vec![
        (
            "schemaVersion",
            Some(string(super::version::SCHEMA_VERSION)),
        ),
        ("identity", Some(string(super::version::IDENTITY))),
        (
            "attachmentRevision",
            Some(string(attachment.attachment_revision().as_str())),
        ),
        ("projectId", Some(string(attachment.project_id().as_str()))),
        ("modelRef", Some(model_ref_payload(attachment.model_ref()))),
        (
            "irRef",
            Some(digest_payload(
                "dev.lekalo.ir@0.1.0",
                attachment.ir_digest().as_str(),
            )),
        ),
        (
            "sourceMapRef",
            attachment
                .source_map_ref()
                .map(|reference| string(reference.as_str())),
        ),
        (
            "stateSpaces",
            Some(array(
                &state_spaces
                    .iter()
                    .map(|space| state_space_payload(space))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "invariants",
            Some(array(
                &invariants
                    .iter()
                    .map(|invariant| invariant_payload(invariant))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "transitions",
            Some(array(
                &transitions
                    .iter()
                    .map(|transition| transition_payload(transition))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "verificationMappings",
            Some(array(
                &mappings
                    .iter()
                    .map(|mapping| mapping_payload(mapping))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "propertyHints",
            optional_array(
                &hints
                    .iter()
                    .map(|hint| hint_payload(hint))
                    .collect::<Vec<String>>(),
            ),
        ),
    ])
}

/// The canonical Model reference.
fn model_ref_payload(model: &super::ModelPin) -> String {
    object(vec![
        ("modelVersion", Some(string(model.version.as_str()))),
        ("digest", Some(string(model.digest.as_str()))),
    ])
}

/// One canonical `{identity, digest}` reference.
fn digest_payload(identity: &str, digest: &str) -> String {
    object(vec![
        ("identity", Some(string(identity))),
        ("digest", Some(string(digest))),
    ])
}

/// One canonical state space.
fn state_space_payload(space: &super::state::StateSpace) -> String {
    let mut states: Vec<&super::state::StateState> = space.states().iter().collect();
    states.sort_by_key(|state| state.state_id().as_str());
    object(vec![
        (
            "stateSpaceId",
            Some(string(space.state_space_id().as_str())),
        ),
        ("entity", Some(string(space.entity().as_str()))),
        (
            "states",
            Some(array(
                &states
                    .iter()
                    .map(|state| {
                        object(vec![
                            ("stateId", Some(string(state.state_id().as_str()))),
                            ("initial", state.initial().then(|| bool_text(true))),
                            ("terminal", state.terminal().then(|| bool_text(true))),
                        ])
                    })
                    .collect::<Vec<String>>(),
            )),
        ),
        ("cyclePolicy", Some(string(space.cycle_policy().key()))),
        ("deadPolicy", Some(string(space.dead_policy().key()))),
    ])
}

/// One canonical field reference.
fn field_ref_payload(field: &FieldRef) -> String {
    object(vec![
        (
            "entity",
            field.entity().map(|entity| string(entity.as_str())),
        ),
        ("field", Some(string(field.field().as_str()))),
    ])
}

/// One canonical value node.
fn value_payload(value: &ValueNode) -> String {
    match value {
        ValueNode::Null => object(vec![("kind", Some(string("null")))]),
        ValueNode::Boolean(value) => object(vec![
            ("kind", Some(string("boolean"))),
            ("value", Some(bool_text(*value))),
        ]),
        ValueNode::Integer(value) => object(vec![
            ("kind", Some(string("integer"))),
            ("value", Some(value.to_string())),
        ]),
        ValueNode::String(value) => object(vec![
            ("kind", Some(string("string"))),
            ("value", Some(string(value))),
        ]),
        ValueNode::Decimal(value) => object(vec![
            ("kind", Some(string("decimal"))),
            ("value", Some(string(value))),
        ]),
        ValueNode::Date(value) => object(vec![
            ("kind", Some(string("date"))),
            ("value", Some(string(value))),
        ]),
        ValueNode::DateTime(value) => object(vec![
            ("kind", Some(string("datetime"))),
            ("value", Some(string(value))),
        ]),
        ValueNode::Uuid(value) => object(vec![
            ("kind", Some(string("uuid"))),
            ("value", Some(string(value))),
        ]),
        ValueNode::Uri(value) => object(vec![
            ("kind", Some(string("uri"))),
            ("value", Some(string(value))),
        ]),
        ValueNode::EnumMember { type_ref, value } => object(vec![
            ("kind", Some(string("enum_member"))),
            ("typeRef", Some(string(type_ref.as_str()))),
            ("value", Some(string(value))),
        ]),
        ValueNode::Field { entity, field } => object(vec![
            ("kind", Some(string("field"))),
            (
                "entity",
                entity.as_ref().map(|entity| string(entity.as_str())),
            ),
            ("field", Some(string(field.as_str()))),
        ]),
        ValueNode::Input { field } => object(vec![
            ("kind", Some(string("input"))),
            ("field", Some(string(field.as_str()))),
        ]),
        ValueNode::Prior { field } => object(vec![
            ("kind", Some(string("prior"))),
            ("field", Some(string(field.as_str()))),
        ]),
        ValueNode::Now => object(vec![("kind", Some(string("now")))]),
        ValueNode::List(items) => object(vec![
            ("kind", Some(string("list"))),
            (
                "items",
                Some(array(
                    &items.iter().map(value_payload).collect::<Vec<String>>(),
                )),
            ),
        ]),
        ValueNode::Object(entries) => {
            let mut sorted: Vec<(&String, &ValueNode)> =
                entries.iter().map(|(key, value)| (key, value)).collect();
            sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            object(vec![
                ("kind", Some(string("object"))),
                (
                    "entries",
                    Some(array(
                        &sorted
                            .iter()
                            .map(|(key, value)| {
                                object(vec![
                                    ("key", Some(string(key))),
                                    ("value", Some(value_payload(value))),
                                ])
                            })
                            .collect::<Vec<String>>(),
                    )),
                ),
            ])
        }
    }
}

/// One canonical predicate node.
fn predicate_payload(predicate: &PredicateNode) -> String {
    let pair = |op: &str, left: &ValueNode, right: &ValueNode| {
        object(vec![
            ("op", Some(string(op))),
            ("left", Some(value_payload(left))),
            ("right", Some(value_payload(right))),
        ])
    };
    let unary = |op: &str, operand: &ValueNode| {
        object(vec![
            ("op", Some(string(op))),
            ("operand", Some(value_payload(operand))),
        ])
    };
    match predicate {
        PredicateNode::Equal { left, right } => pair("equal", left, right),
        PredicateNode::NotEqual { left, right } => pair("not_equal", left, right),
        PredicateNode::IsNull { operand } => unary("is_null", operand),
        PredicateNode::NotNull { operand } => unary("not_null", operand),
        PredicateNode::InSet { operand, values } => object(vec![
            ("op", Some(string("in_set"))),
            ("operand", Some(value_payload(operand))),
            (
                "values",
                Some(array(
                    &values.iter().map(value_payload).collect::<Vec<String>>(),
                )),
            ),
        ]),
        PredicateNode::All { from, predicate } => object(vec![
            ("op", Some(string("all"))),
            ("from", Some(value_payload(from))),
            ("predicate", Some(predicate_payload(predicate))),
        ]),
        PredicateNode::Any { from, predicate } => object(vec![
            ("op", Some(string("any"))),
            ("from", Some(value_payload(from))),
            ("predicate", Some(predicate_payload(predicate))),
        ]),
        PredicateNode::And { operands } | PredicateNode::Or { operands } => object(vec![
            (
                "op",
                Some(string(match predicate {
                    PredicateNode::And { .. } => "and",
                    _ => "or",
                })),
            ),
            (
                "operands",
                Some(array(
                    &operands
                        .iter()
                        .map(predicate_payload)
                        .collect::<Vec<String>>(),
                )),
            ),
        ]),
        PredicateNode::Before { left, right } => pair("before", left, right),
        PredicateNode::After { left, right } => pair("after", left, right),
        PredicateNode::Within { left, duration } => object(vec![
            ("op", Some(string("within"))),
            ("left", Some(value_payload(left))),
            (
                "duration",
                Some(object(vec![
                    ("unit", Some(string(duration.unit().key()))),
                    ("amount", Some(duration.amount().to_string())),
                ])),
            ),
        ]),
        PredicateNode::Count { from, min, max } => object(vec![
            ("op", Some(string("count"))),
            ("from", Some(value_payload(from))),
            ("min", Some(min.to_string())),
            ("max", Some(max.to_string())),
        ]),
        PredicateNode::MemberOf { operand, set } => object(vec![
            ("op", Some(string("member_of"))),
            ("operand", Some(value_payload(operand))),
            ("set", Some(value_payload(set))),
        ]),
    }
}

/// One canonical invariant.
fn invariant_payload(invariant: &super::invariant::Invariant) -> String {
    object(vec![
        (
            "invariantId",
            Some(string(invariant.invariant_id().as_str())),
        ),
        (
            "stateSpaceId",
            Some(string(invariant.state_space_id().as_str())),
        ),
        ("kind", Some(string(invariant.kind().key()))),
        ("description", invariant.description().map(string)),
        (
            "fields",
            optional_array(
                &invariant
                    .fields()
                    .iter()
                    .map(field_ref_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        ("predicate", invariant.predicate().map(predicate_payload)),
        ("min", invariant.min().map(|min| min.to_string())),
        ("max", invariant.max().map(|max| max.to_string())),
        (
            "triggerState",
            invariant
                .trigger_state()
                .map(|state| string(state.as_str())),
        ),
        (
            "allowedStates",
            optional_array(
                &invariant
                    .allowed_states()
                    .iter()
                    .map(|state| string(state.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        ("partition", invariant.partition().map(field_ref_payload)),
        ("maxActive", invariant.max_active().then(|| "1".to_owned())),
        (
            "aggregateRef",
            invariant
                .aggregate_ref()
                .map(|aggregate| string(aggregate.as_str())),
        ),
        (
            "requiredFields",
            optional_array(
                &invariant
                    .required_fields()
                    .iter()
                    .map(|field| string(field.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "capabilityRequirement",
            invariant.capability_requirement().map(|requirement| {
                object(vec![
                    (
                        "requirementId",
                        Some(string(requirement.requirement_id().as_str())),
                    ),
                    ("capability", Some(string(requirement.capability()))),
                    ("minimum", Some(string(requirement.minimum().key()))),
                    ("reason", Some(string(requirement.reason()))),
                ])
            }),
        ),
        (
            "conflictErrorRef",
            invariant
                .conflict_error_ref()
                .map(|reference| string(reference.as_str())),
        ),
        (
            "errorRefs",
            optional_array(
                &invariant
                    .error_refs()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "requirementRefs",
            optional_array(
                &invariant
                    .requirement_refs()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "scenarioRefs",
            optional_array(
                &invariant
                    .scenario_refs()
                    .iter()
                    .map(scenario_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "capabilityRefs",
            optional_array(
                &invariant
                    .capability_refs()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
    ])
}

/// One canonical scenario reference.
fn scenario_payload(scenario: &super::trace::ScenarioRef) -> String {
    object(vec![
        ("scenarioId", Some(string(scenario.scenario_id()))),
        ("scenarioVersion", Some(string(scenario.scenario_version()))),
        ("irDigest", Some(string(scenario.ir_digest().as_str()))),
    ])
}

/// One canonical assignment value.
fn assignment_value_payload(value: &AssignmentValue) -> String {
    match value {
        AssignmentValue::Literal(value) => object(vec![
            ("kind", Some(string("literal"))),
            ("value", Some(value_payload(value))),
        ]),
        AssignmentValue::Input { field } => object(vec![
            ("kind", Some(string("input"))),
            ("field", Some(string(field.as_str()))),
        ]),
        AssignmentValue::Prior { field } => object(vec![
            ("kind", Some(string("prior"))),
            ("field", Some(string(field.as_str()))),
        ]),
        AssignmentValue::Now => object(vec![("kind", Some(string("now")))]),
        AssignmentValue::Expression { expression_ref } => object(vec![
            ("kind", Some(string("expression"))),
            ("expressionRef", Some(string(expression_ref.as_str()))),
        ]),
    }
}

/// One canonical transition.
fn transition_payload(transition: &super::transition::Transition) -> String {
    object(vec![
        (
            "transitionId",
            Some(string(transition.transition_id().as_str())),
        ),
        (
            "stateSpaceId",
            Some(string(transition.state_space_id().as_str())),
        ),
        (
            "fromStates",
            Some(array(
                &transition
                    .from_states()
                    .iter()
                    .map(|state| string(state.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        ("toState", Some(string(transition.to_state().as_str()))),
        ("command", Some(string(transition.command().as_str()))),
        (
            "assignments",
            optional_array(
                &transition
                    .assignments()
                    .iter()
                    .map(|assignment| {
                        object(vec![
                            ("field", Some(string(assignment.field().as_str()))),
                            ("value", Some(assignment_value_payload(assignment.value()))),
                        ])
                    })
                    .collect::<Vec<String>>(),
            ),
        ),
        ("parallel", transition.parallel().then(|| bool_text(true))),
        (
            "preconditions",
            optional_array(
                &transition
                    .preconditions()
                    .iter()
                    .map(predicate_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "policyRef",
            transition
                .policy_ref()
                .map(|reference| string(reference.as_str())),
        ),
        (
            "errorRefs",
            optional_array(
                &transition
                    .error_refs()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "requirementRefs",
            optional_array(
                &transition
                    .requirement_refs()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "scenarioRefs",
            optional_array(
                &transition
                    .scenario_refs()
                    .iter()
                    .map(scenario_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "capabilityRefs",
            optional_array(
                &transition
                    .capability_refs()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
    ])
}

/// One canonical mapping subject.
fn subject_payload(subject: &super::trace::Subject) -> String {
    object(vec![
        ("kind", Some(string(subject.kind().key()))),
        ("id", Some(string(subject.id().as_str()))),
    ])
}

/// One canonical verification mapping.
fn mapping_payload(mapping: &super::trace::VerificationMapping) -> String {
    object(vec![
        ("mappingId", Some(string(mapping.mapping_id().as_str()))),
        ("subject", Some(subject_payload(mapping.subject()))),
        ("relation", Some(string(mapping.relation().key()))),
        (
            "targetRef",
            Some(object(vec![
                (
                    "targetKind",
                    Some(string(mapping.target_ref().target_kind().key())),
                ),
                ("targetId", Some(string(mapping.target_ref().target_id()))),
            ])),
        ),
        (
            "enforcementLayer",
            Some(string(mapping.enforcement_layer().key())),
        ),
        (
            "evidence",
            mapping.evidence().map(|evidence| {
                object(vec![
                    (
                        "profileRef",
                        evidence
                            .profile_ref()
                            .map(|reference| string(reference.as_str())),
                    ),
                    (
                        "adapterRef",
                        evidence
                            .adapter_ref()
                            .map(|reference| string(reference.as_str())),
                    ),
                    (
                        "protocolRef",
                        evidence
                            .protocol_ref()
                            .map(|reference| string(reference.as_str())),
                    ),
                    (
                        "executableDigest",
                        Some(string(evidence.executable_digest().as_str())),
                    ),
                    ("sourceRevision", Some(string(evidence.source_revision()))),
                    ("scenarioRef", evidence.scenario_ref().map(scenario_payload)),
                    (
                        "testRef",
                        evidence
                            .test_ref()
                            .map(|reference| string(reference.as_str())),
                    ),
                    (
                        "gateRef",
                        evidence
                            .gate_ref()
                            .map(|reference| string(reference.as_str())),
                    ),
                    ("resultStatus", Some(string(evidence.result_status().key()))),
                    (
                        "evidenceDigest",
                        Some(string(evidence.evidence_digest().as_str())),
                    ),
                ])
            }),
        ),
        ("status", Some(string(mapping.status().key()))),
        ("confidence", Some(string(mapping.confidence().key()))),
    ])
}

/// One canonical property hint.
fn hint_payload(hint: &super::trace::PropertyHint) -> String {
    object(vec![
        ("hintId", Some(string(hint.hint_id().as_str()))),
        ("subject", Some(subject_payload(hint.subject()))),
        ("property", Some(string(hint.property().key()))),
        ("description", hint.description().map(string)),
    ])
}

/// One canonical JSON string value.
fn string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// One canonical JSON boolean.
fn bool_text(value: bool) -> String {
    if value {
        "true".to_owned()
    } else {
        "false".to_owned()
    }
}

/// One canonical JSON array.
fn array(members: &[String]) -> String {
    format!("[{}]", members.join(","))
}

/// One canonical JSON array, or nothing when empty.
fn optional_array(members: &[String]) -> Option<String> {
    if members.is_empty() {
        None
    } else {
        Some(array(members))
    }
}

/// One canonical JSON object with byte-sorted keys; `None` members are
/// dropped entirely.
fn object(members: Vec<(&str, Option<String>)>) -> String {
    let mut present: Vec<(&str, String)> = members
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect();
    present.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let body: Vec<String> = present
        .iter()
        .map(|(key, value)| format!("{}:{}", string(key), value))
        .collect();
    format!("{{{}}}", body.join(","))
}
