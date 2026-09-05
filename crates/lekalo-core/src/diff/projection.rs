//! The typed canonical semantic projection of one `CompiledProject`
//! (issue #18).
//!
//! The projection keeps exactly the semantic content the accepted Model can
//! declare — stable IDs, kinds, definition versions, type wrappers,
//! presence and nullability, references, declared identity — and drops
//! exactly what the accepted contracts call non-semantic: bounded
//! descriptions (documentation), the rename/tombstone registry
//! (provenance consumed by the history resolver), source spans, physical
//! paths, formatting, and source-revision metadata.
//!
//! Member and reference arrays normalize to sorted form (duplicates
//! retained), so permutation of members or references is never observable
//! as a semantic change; the accepted Model declares no positional wire
//! contract. The projection bytes are the semantic-digest preimage: two
//! projects share a digest exactly when their semantics are equal.

use super::identity::{Subject, SubjectFamily};
use super::version::MAX_COMPARE_SUBJECTS;
use crate::ir::{CompiledProject, Definition, TypeRef};
use std::collections::BTreeMap;

/// One projected field: name, canonical type expression, presence.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct FieldProjection {
    pub name: String,
    pub field_type: String,
    pub required: bool,
}

/// The semantic payload of one projected definition, per family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Payload {
    Project,
    Module {
        imports: Vec<String>,
    },
    Scalar {
        base: String,
    },
    Enum {
        values: Vec<String>,
    },
    Struct {
        fields: Vec<FieldProjection>,
        identity: Vec<String>,
    },
    Command {
        input: Vec<FieldProjection>,
        effects: Vec<String>,
    },
    Query {
        reads: Vec<String>,
        returns: Option<String>,
    },
    Policy {
        applies_to: Vec<String>,
        decision: String,
    },
    Event {
        payload: Vec<FieldProjection>,
    },
    Effect {
        operation: String,
        entity: String,
        emits: Vec<String>,
    },
    Endpoint {
        invokes: String,
        method: String,
        path: String,
    },
    Scenario {
        summary: String,
        covers: Vec<String>,
    },
    TargetBinding {
        target: String,
    },
}

/// The semantic content of one subject: shared members plus the payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SubjectNode {
    pub family: SubjectFamily,
    pub version: u64,
    pub derived_from: Vec<String>,
    pub visibility: Option<String>,
    pub portability: Option<String>,
    pub renamed_from: Vec<String>,
    pub payload: Payload,
}

/// The whole projection of one side: subjects keyed by their stable
/// identity, in canonical order.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub(crate) struct Projection {
    pub subjects: BTreeMap<Subject, SubjectNode>,
}

/// The canonical type-expression rendering of a [`TypeRef`]:
/// `id`, `list<T>`, `optional<T>`.
pub(crate) fn type_expression(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Ref(symbol) => symbol.as_str().to_owned(),
        TypeRef::List(inner) => format!("list<{}>", type_expression(inner)),
        TypeRef::Optional(inner) => format!("optional<{}>", type_expression(inner)),
    }
}

/// Sort and deduplicate nothing: sort only, keeping duplicate occurrences.
fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

/// Project the shared common members of one definition.
fn common_node(family: SubjectFamily, common: &crate::ir::Common, payload: Payload) -> SubjectNode {
    SubjectNode {
        family,
        version: common.version,
        derived_from: sorted(
            common
                .derived_from
                .iter()
                .map(|requirement| requirement.as_str().to_owned())
                .collect(),
        ),
        visibility: common.visibility.map(|value| value.as_str().to_owned()),
        portability: common.portability.map(|value| value.as_str().to_owned()),
        renamed_from: common
            .renamed_from
            .iter()
            .map(|symbol| symbol.as_str().to_owned())
            .collect(),
        payload,
    }
}

fn fields(fields: &[crate::ir::Field]) -> Vec<FieldProjection> {
    let mut projected: Vec<FieldProjection> = fields
        .iter()
        .map(|field| FieldProjection {
            name: field.name.as_str().to_owned(),
            field_type: type_expression(&field.r#type),
            required: field.required,
        })
        .collect();
    projected.sort();
    projected
}

/// Build the projection of one compiled project, or reject beyond the
/// subject bound before any comparison runs.
pub(crate) fn project(compiled: &CompiledProject) -> Result<Projection, usize> {
    let mut subjects = BTreeMap::new();
    if compiled.project.is_some() as usize + compiled.modules.len() + compiled.definitions.len()
        > MAX_COMPARE_SUBJECTS
    {
        return Err(MAX_COMPARE_SUBJECTS);
    }

    if let Some(project) = &compiled.project {
        let subject = Subject::new(SubjectFamily::Project, project.id.as_str());
        let node = common_node(SubjectFamily::Project, &project.common, Payload::Project);
        subjects.insert(subject, node);
    }
    for module in &compiled.modules {
        let subject = Subject::new(SubjectFamily::Module, module.id.as_str());
        let payload = Payload::Module {
            imports: sorted(
                module
                    .imports
                    .iter()
                    .map(|import| import.as_str().to_owned())
                    .collect(),
            ),
        };
        let node = common_node(SubjectFamily::Module, &module.common, payload);
        subjects.insert(subject, node);
    }
    for definition in &compiled.definitions {
        let family = SubjectFamily::of_kind(definition.kind());
        let payload = match definition {
            Definition::Scalar(def) => Payload::Scalar {
                base: def.base.as_str().to_owned(),
            },
            Definition::Enum(def) => Payload::Enum {
                values: sorted(
                    def.values
                        .iter()
                        .map(|value| value.value.as_str().to_owned())
                        .collect(),
                ),
            },
            Definition::ValueObject(def) => Payload::Struct {
                fields: fields(&def.fields),
                identity: Vec::new(),
            },
            Definition::Entity(def) => Payload::Struct {
                fields: fields(&def.fields),
                identity: sorted(
                    def.identity
                        .iter()
                        .map(|name| name.as_str().to_owned())
                        .collect(),
                ),
            },
            Definition::Command(def) => Payload::Command {
                input: fields(&def.input),
                effects: sorted(
                    def.effects
                        .iter()
                        .map(|effect| effect.as_str().to_owned())
                        .collect(),
                ),
            },
            Definition::Query(def) => Payload::Query {
                reads: sorted(
                    def.reads
                        .iter()
                        .map(|read| read.as_str().to_owned())
                        .collect(),
                ),
                returns: def.returns.as_ref().map(type_expression),
            },
            Definition::Policy(def) => Payload::Policy {
                applies_to: sorted(
                    def.applies_to
                        .iter()
                        .map(|symbol| symbol.as_str().to_owned())
                        .collect(),
                ),
                decision: def.decision.as_str().to_owned(),
            },
            Definition::Event(def) => Payload::Event {
                payload: fields(&def.payload),
            },
            Definition::Effect(def) => Payload::Effect {
                operation: def.operation.as_str().to_owned(),
                entity: def.entity.as_str().to_owned(),
                emits: sorted(
                    def.emits
                        .iter()
                        .map(|event| event.as_str().to_owned())
                        .collect(),
                ),
            },
            Definition::Endpoint(def) => Payload::Endpoint {
                invokes: def.invokes.as_str().to_owned(),
                method: def.method.as_str().to_owned(),
                path: def.path.as_str().to_owned(),
            },
            Definition::Scenario(def) => Payload::Scenario {
                summary: def.summary.as_str().to_owned(),
                covers: sorted(
                    def.covers
                        .iter()
                        .map(|symbol| symbol.as_str().to_owned())
                        .collect(),
                ),
            },
            Definition::TargetBinding(def) => Payload::TargetBinding {
                target: def.target.as_str().to_owned(),
            },
        };
        let subject = Subject::new(family, definition.id().as_str());
        let node = common_node(family, definition.common(), payload);
        subjects.insert(subject, node);
    }
    Ok(Projection { subjects })
}

/// The canonical semantic bytes of one projection: every subject in
/// canonical order, descriptions and provenance absent. This is the
/// preimage of the semantic digest.
pub(crate) fn canonical_bytes(projection: &Projection) -> String {
    let mut json = String::from("{\"subjects\":[");
    let mut first = true;
    for (subject, node) in &projection.subjects {
        if !first {
            json.push(',');
        }
        first = false;
        json.push_str(&subject_bytes(subject, node));
    }
    json.push_str("]}");
    json
}

/// The canonical bytes of one subject node.
#[allow(clippy::too_many_lines)]
fn subject_bytes(subject: &Subject, node: &SubjectNode) -> String {
    let mut json = String::from("{\"derivedFrom\":");
    json.push_str(&string_array(&node.derived_from));
    json.push_str(",\"family\":");
    json.push_str(&super::canonical::quote(node.family.key()));
    json.push_str(",\"id\":");
    json.push_str(&super::canonical::quote(subject.id()));
    json.push_str(",\"member\":");
    json.push_str(&super::canonical::quote(subject.member().unwrap_or("")));
    json.push_str(",\"payload\":");
    json.push_str(&payload_bytes(&node.payload));
    json.push_str(",\"portability\":");
    json.push_str(&super::canonical::quote(
        node.portability.as_deref().unwrap_or(""),
    ));
    json.push_str(",\"renamedFrom\":");
    json.push_str(&string_array(&node.renamed_from));
    json.push_str(",\"version\":");
    json.push_str(&node.version.to_string());
    json.push_str(",\"visibility\":");
    json.push_str(&super::canonical::quote(
        node.visibility.as_deref().unwrap_or(""),
    ));
    json.push('}');
    json
}

/// The canonical bytes of one payload variant.
fn payload_bytes(payload: &Payload) -> String {
    match payload {
        Payload::Project => "{\"kind\":\"project\"}".to_owned(),
        Payload::Module { imports } => format!(
            "{{\"imports\":{},\"kind\":\"module\"}}",
            string_array(imports)
        ),
        Payload::Scalar { base } => format!(
            "{{\"base\":{},\"kind\":\"scalar\"}}",
            super::canonical::quote(base)
        ),
        Payload::Enum { values } => {
            format!("{{\"kind\":\"enum\",\"values\":{}}}", string_array(values))
        }
        Payload::Struct { fields, identity } => format!(
            "{{\"fields\":{},\"identity\":{},\"kind\":\"struct\"}}",
            field_array(fields),
            string_array(identity)
        ),
        Payload::Command { input, effects } => format!(
            "{{\"effects\":{},\"input\":{},\"kind\":\"command\"}}",
            string_array(effects),
            field_array(input)
        ),
        Payload::Query { reads, returns } => format!(
            "{{\"kind\":\"query\",\"reads\":{},\"returns\":{}}}",
            string_array(reads),
            match returns {
                Some(expression) => super::canonical::quote(expression),
                None => "null".to_owned(),
            }
        ),
        Payload::Policy {
            applies_to,
            decision,
        } => format!(
            "{{\"appliesTo\":{},\"decision\":{},\"kind\":\"policy\"}}",
            string_array(applies_to),
            super::canonical::quote(decision)
        ),
        Payload::Event { payload } => format!(
            "{{\"kind\":\"event\",\"payload\":{}}}",
            field_array(payload)
        ),
        Payload::Effect {
            operation,
            entity,
            emits,
        } => format!(
            "{{\"emits\":{},\"entity\":{},\"kind\":\"effect\",\"operation\":{}}}",
            string_array(emits),
            super::canonical::quote(entity),
            super::canonical::quote(operation)
        ),
        Payload::Endpoint {
            invokes,
            method,
            path,
        } => format!(
            "{{\"invokes\":{},\"kind\":\"endpoint\",\"method\":{},\"path\":{}}}",
            super::canonical::quote(invokes),
            super::canonical::quote(method),
            super::canonical::quote(path)
        ),
        Payload::Scenario { summary, covers } => format!(
            "{{\"covers\":{},\"kind\":\"scenario\",\"summary\":{}}}",
            string_array(covers),
            super::canonical::quote(summary)
        ),
        Payload::TargetBinding { target } => format!(
            "{{\"kind\":\"target-binding\",\"target\":{}}}",
            super::canonical::quote(target)
        ),
    }
}

/// The canonical bytes of one projected field.
fn field_bytes(field: &FieldProjection) -> String {
    format!(
        "{{\"name\":{},\"required\":{},\"type\":{}}}",
        super::canonical::quote(&field.name),
        field.required,
        super::canonical::quote(&field.field_type)
    )
}

/// The canonical bytes of a sorted string array.
fn string_array(values: &[String]) -> String {
    let mut json = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&super::canonical::quote(value));
    }
    json.push(']');
    json
}

/// The canonical bytes of a sorted projected-field array.
fn field_array(values: &[FieldProjection]) -> String {
    let mut json = String::from("[");
    for (index, field) in values.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&field_bytes(field));
    }
    json.push(']');
    json
}

/// Rewrite every symbolic reference of one projection from `from` to
/// `to`: type expressions and reference arrays follow a validated
/// same-identity rename so that mechanical reference updates never
/// masquerade as independent semantic changes. Member names, target
/// names, enum values, requirement references, and history claims are
/// not symbolic references and stay untouched.
pub(crate) fn substitute_references(projection: &mut Projection, from: &str, to: &str) {
    for node in projection.subjects.values_mut() {
        // derived_from carries requirement ids and renamed_from carries
        // history claims; neither is a symbolic reference to rewrite.
        let _ = &node.derived_from;
        match &mut node.payload {
            Payload::Project => {}
            Payload::Module { imports } => substitute_slice(imports, from, to),
            Payload::Scalar { .. } => {}
            Payload::Enum { .. } => {}
            Payload::Struct { fields, identity } => {
                for field in fields {
                    field.field_type = substitute_expression(&field.field_type, from, to);
                }
                substitute_slice(identity, from, to);
            }
            Payload::Command { input, effects } => {
                for field in input {
                    field.field_type = substitute_expression(&field.field_type, from, to);
                }
                substitute_slice(effects, from, to);
            }
            Payload::Query { reads, returns } => {
                substitute_slice(reads, from, to);
                if let Some(expression) = returns {
                    *expression = substitute_expression(expression, from, to);
                }
            }
            Payload::Policy { applies_to, .. } => substitute_slice(applies_to, from, to),
            Payload::Event { payload } => {
                for field in payload {
                    field.field_type = substitute_expression(&field.field_type, from, to);
                }
            }
            Payload::Effect { entity, emits, .. } => {
                *entity = substitute_expression(entity, from, to);
                substitute_slice(emits, from, to);
            }
            Payload::Endpoint { invokes, .. } => {
                *invokes = substitute_expression(invokes, from, to);
            }
            Payload::Scenario { covers, .. } => substitute_slice(covers, from, to),
            Payload::TargetBinding { .. } => {}
        }
    }
}

/// Substitute one exact id across a reference array.
fn substitute_slice(values: &mut [String], from: &str, to: &str) {
    for value in values.iter_mut() {
        if value == from {
            *value = to.to_owned();
        }
    }
}

/// Substitute one exact id inside a canonical type expression: the id
/// must appear at an expression boundary (start or after `<`), followed
/// by the expression end or `>`.
fn substitute_expression(expression: &str, from: &str, to: &str) -> String {
    if expression == from {
        return to.to_owned();
    }
    let mut result = String::with_capacity(expression.len() + to.len());
    let bytes = expression.as_bytes();
    let mut position = 0usize;
    while position < bytes.len() {
        if expression[position..].starts_with('<') {
            result.push('<');
            position += 1;
            continue;
        }
        if expression[position..].starts_with(from)
            && (position == 0 || bytes[position - 1] == b'<')
            && (position + from.len() == bytes.len()
                || expression[position + from.len()..].starts_with('>'))
        {
            result.push_str(to);
            position += from.len();
            continue;
        }
        let character = expression[position..].chars().next().unwrap_or('>');
        result.push(character);
        position += character.len_utf8();
    }
    result
}
