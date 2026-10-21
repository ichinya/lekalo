//! Semantic validation of the typed relation and secondary-uniqueness
//! declarations (Model 0.8.0 successor).
//!
//! Decoding proves syntax; this pass proves semantics: every relation
//! target names a declared entity, every local/target field exists on
//! its own side with matching pairwise types, every constrained field
//! exists on its declaring entity, and every declared normalization
//! reference names a scalar. A declaration that references a ghost
//! entity, a ghost field, or a non-scalar normalization fails the whole
//! compilation with one registered `ir.value-invalid` diagnostic per
//! defect — silently accepting an unresolvable join would let generated
//! output drift from the declared Model semantics.

use super::diagnostic::{bounded_field_echo, VALUE_INVALID};
use super::{Compilation, Definition, TypeRef, UniqueConstraintDef};
use std::collections::{BTreeMap, BTreeSet};

/// One entity's decoded semantic surface: the exact type expression of
/// every field, keyed by field name.
type EntitySurface = BTreeMap<String, String>;

/// The canonical type expression of one field type: `id`, `list<T>`,
/// `optional<T>`.
fn type_expression(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Ref(symbol) => symbol.as_str().to_owned(),
        TypeRef::List(inner) => format!("list<{}>", type_expression(inner)),
        TypeRef::Optional(inner) => format!("optional<{}>", type_expression(inner)),
    }
}

/// The source span of one definition, resolved through the source map.
fn definition_span(compilation: &Compilation, id: &str) -> crate::loader::error::Span {
    compilation
        .source_map
        .entries()
        .iter()
        .find(|entry| entry.semantic_id.as_deref() == Some(id))
        .map(|entry| crate::loader::error::Span {
            start: crate::loader::error::SpanPos {
                byte: entry.start.byte,
                line: entry.start.line,
                column: entry.start.column,
            },
            end: crate::loader::error::SpanPos {
                byte: entry.end.byte,
                line: entry.end.line,
                column: entry.end.column,
            },
        })
        .unwrap_or(crate::loader::error::Span::EMPTY_EOF)
}

/// One registered semantic defect over the owning definition's span.
fn defect(
    span: crate::loader::error::Span,
    detail: &'static str,
    value: &str,
) -> crate::loader::error::Diagnostic {
    crate::loader::error::Diagnostic::new(VALUE_INVALID)
        .with_span(span)
        .with_data(serde_json::json!({
            "detail": detail,
            "value": bounded_field_echo(value),
        }))
}

/// Validate every typed declaration of one compilation against the
/// declarations' semantic owners.
pub(crate) fn validate(compilation: &Compilation) -> Vec<crate::loader::error::Diagnostic> {
    let mut entities: BTreeMap<&str, EntitySurface> = BTreeMap::new();
    let mut scalars: BTreeMap<&str, ()> = BTreeMap::new();
    for definition in &compilation.project.definitions {
        match definition {
            Definition::Entity(entity) => {
                let mut fields = BTreeMap::new();
                for field in &entity.fields {
                    fields.insert(
                        field.name.as_str().to_owned(),
                        type_expression(&field.r#type),
                    );
                }
                entities.insert(entity.id.as_str(), fields);
            }
            Definition::Scalar(scalar) => {
                scalars.insert(scalar.id.as_str(), ());
            }
            _ => {}
        }
    }

    let mut diagnostics = Vec::new();
    for definition in &compilation.project.definitions {
        let Definition::Entity(entity) = definition else {
            continue;
        };
        let Some(surface) = entities.get(entity.id.as_str()) else {
            continue;
        };
        for relation in &entity.relations {
            let span = definition_span(compilation, entity.id.as_str());
            let Some(target) = entities.get(relation.target.as_str()) else {
                diagnostics.push(defect(
                    span,
                    "relation-target-unresolved",
                    relation.target.as_str(),
                ));
                continue;
            };
            for field in &relation.local_fields {
                if !surface.contains_key(field.as_str()) {
                    diagnostics.push(defect(span, "relation-local-field-missing", field.as_str()));
                }
            }
            for field in &relation.target_fields {
                if !target.contains_key(field.as_str()) {
                    diagnostics.push(defect(
                        span,
                        "relation-target-field-missing",
                        field.as_str(),
                    ));
                }
            }
            // The join is typed: paired fields must carry the same type
            // expression, or the declared join cannot exist.
            for (local, target_field) in relation.local_fields.iter().zip(&relation.target_fields) {
                if let (Some(local_type), Some(target_type)) = (
                    surface.get(local.as_str()),
                    target.get(target_field.as_str()),
                ) {
                    if local_type != target_type {
                        diagnostics.push(defect(
                            span,
                            "relation-field-type-mismatch",
                            local.as_str(),
                        ));
                    }
                }
            }
        }
        for constraint in &entity.unique_constraints {
            let span = definition_span(compilation, entity.id.as_str());
            for field in &constraint.fields {
                if !surface.contains_key(field.as_str()) {
                    diagnostics.push(defect(span, "constraint-field-missing", field.as_str()));
                }
            }
            if let Some(reference) = &constraint.normalization_ref {
                if !scalars.contains_key(reference.as_str()) {
                    diagnostics.push(defect(
                        span,
                        "constraint-normalization-unresolved",
                        reference.as_str(),
                    ));
                }
            }
        }
    }
    validate_defaults(compilation, &mut diagnostics);
    for definition in &compilation.project.definitions {
        if let Definition::Enum(enumeration) = definition {
            if enumeration.values.iter().any(|v| v.wire_value.is_some()) {
                let mut values = std::collections::BTreeSet::new();
                for value in &enumeration.values {
                    if !values.insert(value.canonical_wire_json()) {
                        diagnostics.push(defect(
                            definition_span(compilation, enumeration.id.as_str()),
                            "enum-wire-duplicate",
                            value.value.as_str(),
                        ));
                    }
                }
            }
        }
    }
    diagnostics
}

fn validate_defaults(
    compilation: &Compilation,
    diagnostics: &mut Vec<crate::loader::error::Diagnostic>,
) {
    if !compilation.project.has_field_defaults() {
        return;
    }
    let definitions: BTreeMap<_, _> = compilation
        .project
        .definitions
        .iter()
        .map(|d| (d.id().as_str(), d))
        .collect();
    let enum_values: BTreeMap<_, BTreeSet<_>> = compilation
        .project
        .definitions
        .iter()
        .filter_map(|d| match d {
            Definition::Enum(e) => Some((
                e.id.as_str(),
                e.values
                    .iter()
                    .map(super::EnumValue::canonical_wire_json)
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    fn matches_type(
        value: &serde_json::Value,
        typ: &TypeRef,
        definitions: &BTreeMap<&str, &Definition>,
        enum_values: &BTreeMap<&str, BTreeSet<String>>,
        depth: usize,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        match typ {
            TypeRef::Optional(inner) => {
                value.is_null() || matches_type(value, inner, definitions, enum_values, depth + 1)
            }
            TypeRef::List(inner) => value.as_array().is_some_and(|items| {
                items
                    .iter()
                    .all(|v| matches_type(v, inner, definitions, enum_values, depth + 1))
            }),
            TypeRef::Ref(id) => match definitions.get(id.as_str()) {
                Some(Definition::Scalar(s)) => match s.base {
                    super::ScalarBase::Boolean => value.is_boolean(),
                    super::ScalarBase::Number => value.is_number(),
                    super::ScalarBase::String
                    | super::ScalarBase::Date
                    | super::ScalarBase::Datetime
                    | super::ScalarBase::Uuid
                    | super::ScalarBase::Uri => value.is_string(),
                },
                Some(Definition::Enum(e)) => super::DefaultValue::from_json(value.clone())
                    .is_some_and(|value| {
                        enum_values
                            .get(e.id.as_str())
                            .is_some_and(|values| values.contains(&value.canonical_json()))
                    }),
                Some(Definition::Entity(e)) => {
                    matches_fields(value, &e.fields, definitions, enum_values, depth + 1)
                }
                Some(Definition::ValueObject(v)) => {
                    matches_fields(value, &v.fields, definitions, enum_values, depth + 1)
                }
                _ => false,
            },
        }
    }
    fn matches_fields(
        value: &serde_json::Value,
        fields: &[super::Field],
        definitions: &BTreeMap<&str, &Definition>,
        enum_values: &BTreeMap<&str, BTreeSet<String>>,
        depth: usize,
    ) -> bool {
        let Some(object) = value.as_object() else {
            return false;
        };
        object
            .keys()
            .all(|k| fields.iter().any(|f| f.name.as_str() == k))
            && fields
                .iter()
                .all(|field| match object.get(field.name.as_str()) {
                    Some(value) => {
                        matches_type(value, &field.r#type, definitions, enum_values, depth + 1)
                    }
                    None => !field.required,
                })
    }
    for definition in &compilation.project.definitions {
        let fields = match definition {
            Definition::Entity(d) => &d.fields,
            Definition::ValueObject(d) => &d.fields,
            Definition::Command(d) => &d.input,
            Definition::Event(d) => &d.payload,
            _ => continue,
        };
        for field in fields {
            if let Some(default) = &field.default {
                if !matches_type(
                    default.as_json(),
                    &field.r#type,
                    &definitions,
                    &enum_values,
                    0,
                ) {
                    diagnostics.push(defect(
                        definition_span(compilation, definition.id().as_str()),
                        "field-default-type-mismatch",
                        field.name.as_str(),
                    ));
                }
            }
        }
    }
}

/// The unique-constraint surface re-exported for the typed signature.
#[allow(dead_code)]
type ConstraintRef = UniqueConstraintDef;
