//! The closed `TypeRef` → JSON-Schema projection of the OpenAPI
//! renderer (issue #46, plan §2.3).
//!
//! The mapping table is the shared #45/#46 contract: `required` is
//! presence (the field key lands in the parent `required[]`),
//! `optional` is nullability (`Optional(T)` renders the 2020-12 type
//! array `[T…, "null"]` at declared version 3.1 and the
//! `nullable: true` sibling at 3.0). One rule, two renderers: the
//! adapter's JS renderer mirrors this table byte for byte, so the two
//! provably agree on the same inputs.
//!
//! Reusable components follow the #45 export-name rule
//! (`<ModulePascal><NamePascal>`), are deduplicated by semantic id,
//! and render once. No domain meaning is invented: a reference the
//! compiled project cannot resolve surfaces as an
//! `openapi.projection-partial` finding with an open schema — never a
//! guessed shape.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value as Json};

use crate::ir::{Common, Definition, Field, ScalarBase, SymbolId, TypeRef};

use super::id::{component_name, escape_pointer, COMPONENTS_SCHEMAS};
use super::types::{DocumentVersion, Finding};

/// The type-to-schema mapper over one compiled project.
pub struct SchemaMapper<'a> {
    project: &'a crate::ir::CompiledProject,
    version: DocumentVersion,
    findings: Vec<Finding>,
    /// Every named `Ref` target, deduplicated by semantic id.
    components: BTreeMap<String, String>,
}

impl<'a> SchemaMapper<'a> {
    /// Map over one compiled project at the declared version.
    pub fn new(project: &'a crate::ir::CompiledProject, version: DocumentVersion) -> Self {
        Self {
            project,
            version,
            findings: Vec::new(),
            components: BTreeMap::new(),
        }
    }

    /// The findings accumulated so far, byte-sorted and deduplicated.
    pub fn findings(&self) -> Vec<Finding> {
        let mut findings = self.findings.clone();
        findings.sort();
        findings.dedup();
        findings
    }

    /// Every registered reusable component: semantic id → component
    /// name, byte-sorted by semantic id.
    pub fn components(&self) -> &BTreeMap<String, String> {
        &self.components
    }

    /// The declared version the mapper renders at.
    pub const fn version(&self) -> DocumentVersion {
        self.version
    }

    /// Record one not-expressible member.
    pub(crate) fn partial(&mut self, symbol: &str, detail: &str) {
        self.findings.push(Finding {
            detail: detail.to_owned(),
            symbol: symbol.to_owned(),
        });
    }

    /// Map one closed type reference.
    pub fn map_type(&mut self, typeref: &TypeRef) -> Json {
        match typeref {
            TypeRef::Ref(symbol) => self.map_symbol(symbol.as_str()),
            TypeRef::List(inner) => json!({
                "type": "array",
                "items": self.map_type(inner),
            }),
            TypeRef::Optional(inner) => {
                let inner = self.map_type(inner);
                self.optional(inner)
            }
        }
    }

    /// Map the closed #62 payload type expression. The reference names
    /// a Model symbol resolved against the compiled project.
    pub fn map_error_type(&mut self, expr: &crate::error_contract::types::TypeExpr) -> Json {
        match expr {
            crate::error_contract::types::TypeExpr::Ref(symbol) => self.map_symbol(symbol.as_str()),
            crate::error_contract::types::TypeExpr::List(inner) => json!({
                "type": "array",
                "items": self.map_error_type(inner),
            }),
            crate::error_contract::types::TypeExpr::Optional(inner) => {
                let inner = self.map_error_type(inner);
                self.optional(inner)
            }
        }
    }

    /// The reusable-schema `$ref` of one named symbol, registering the
    /// component on first use. An unknown symbol is a projection-partial
    /// finding with an open schema — never a guessed shape.
    pub fn ref_schema(&mut self, symbol: &SymbolId) -> Json {
        self.map_symbol(symbol.as_str())
    }

    /// The `$ref` (or open-fallback) schema of one symbol name.
    pub fn map_symbol(&mut self, symbol: &str) -> Json {
        let resolved = self.project.definitions.iter().any(|definition| {
            definition.id().as_str() == symbol && component_kind(definition).is_some()
        });
        if !resolved {
            self.partial(symbol, "symbol-unresolved");
            return json!({});
        }
        let name = component_name(symbol);
        let name = match self.components.get(symbol) {
            Some(existing) => existing.clone(),
            None => {
                self.components.insert(symbol.to_owned(), name.clone());
                name
            }
        };
        json!({ "$ref": format!("{}/{}", COMPONENTS_SCHEMAS, escape_pointer(&name)) })
    }

    /// Render one value schema as nullable at the declared version.
    pub fn optional(&self, inner: Json) -> Json {
        let Json::Object(object) = &inner else {
            return inner;
        };
        match (self.version, object.contains_key("$ref")) {
            // A `$ref` cannot carry value-changing siblings: 3.1
            // composes `oneOf` with the JSON-Schema null type; 3.0
            // uses the `allOf` + `nullable` spelling.
            (DocumentVersion::V3_1, true) => {
                json!({ "oneOf": [inner, { "type": "null" }] })
            }
            (DocumentVersion::V3_0, true) => {
                json!({ "nullable": true, "allOf": [inner] })
            }
            // Value schemas widen in place: the 2020-12 type array
            // grows a `"null"` member; the 3.0 sibling gains
            // `nullable: true`.
            (DocumentVersion::V3_1, false) => {
                let mut nullable = Map::new();
                for (key, value) in object {
                    if key == "type" {
                        let mut types = match value {
                            Json::Array(items) => items.clone(),
                            other => vec![other.clone()],
                        };
                        types.push(Json::String("null".to_owned()));
                        nullable.insert("type".to_owned(), Json::Array(types));
                    } else {
                        nullable.insert(key.clone(), value.clone());
                    }
                }
                Json::Object(nullable)
            }
            (DocumentVersion::V3_0, false) => {
                let mut nullable = object.clone();
                nullable.insert("nullable".to_owned(), Json::Bool(true));
                Json::Object(nullable)
            }
        }
    }

    /// One `const` value at the declared version: 2020-12 spells
    /// `const`; 3.0 spells a single-value `enum`.
    pub fn constant(&self, value: Json) -> Json {
        match self.version {
            DocumentVersion::V3_1 => json!({ "const": value }),
            DocumentVersion::V3_0 => json!({ "enum": [value] }),
        }
    }

    /// One object schema over declared fields: byte-sorted
    /// properties, the presence axis in `required[]`, and
    /// `additionalProperties: false`.
    pub fn object_schema(&mut self, fields: &[Field]) -> Json {
        let mut properties = Map::new();
        let mut required: Vec<String> = Vec::new();
        for field in fields {
            let schema = self.field_schema(field);
            properties.insert(field.name.as_str().to_owned(), schema);
            if field.required {
                required.push(field.name.as_str().to_owned());
            }
        }
        required.sort();
        let mut object = Map::new();
        object.insert("type".to_owned(), Json::String("object".to_owned()));
        object.insert("properties".to_owned(), Json::Object(properties));
        if !required.is_empty() {
            let names: Vec<Json> = required.into_iter().map(Json::String).collect();
            object.insert("required".to_owned(), Json::Array(names));
        }
        object.insert("additionalProperties".to_owned(), Json::Bool(false));
        Json::Object(object)
    }

    /// One field schema with its declared description (G6: verbatim,
    /// present-tense metadata only).
    fn field_schema(&mut self, field: &Field) -> Json {
        let schema = self.map_type(&field.r#type);
        match &field.description {
            Some(description) => match schema {
                Json::Object(mut inner) => {
                    inner.insert(
                        "description".to_owned(),
                        Json::String(description.as_str().to_owned()),
                    );
                    Json::Object(inner)
                }
                other => other,
            },
            None => schema,
        }
    }
}

/// The closed reusable-component kinds: entity, value-object, enum,
/// and scalar. Command/query bodies render inline at their use site;
/// only named `Ref` targets become components (plan §2.2).
pub(crate) fn component_kind(definition: &Definition) -> Option<&'static str> {
    match definition {
        Definition::Scalar(_) => Some("scalar"),
        Definition::Enum(_) => Some("enum"),
        Definition::ValueObject(_) => Some("value-object"),
        Definition::Entity(_) => Some("entity"),
        _ => None,
    }
}

/// The full schema body of one reusable component, or nothing when the
/// definition kind never renders as a component.
pub fn component_body(definition: &Definition, mapper: &mut SchemaMapper) -> Option<Json> {
    let body = match definition {
        Definition::Scalar(def) => scalar_json(def.base),
        Definition::Enum(def) => {
            let mut object = Map::new();
            object.insert("type".to_owned(), Json::String("string".to_owned()));
            object.insert(
                "enum".to_owned(),
                Json::Array(
                    def.values
                        .iter()
                        .map(|value| Json::String(value.value.as_str().to_owned()))
                        .collect(),
                ),
            );
            Json::Object(object)
        }
        Definition::ValueObject(def) => mapper.object_schema(&def.fields),
        Definition::Entity(def) => mapper.object_schema(&def.fields),
        _ => return None,
    };
    Some(with_description(body, definition.common()))
}

/// The closed scalar-base projection (the §2.3 table).
pub(crate) fn scalar_json(base: ScalarBase) -> Json {
    match base {
        ScalarBase::String => json!({ "type": "string" }),
        ScalarBase::Number => json!({ "type": "number" }),
        ScalarBase::Boolean => json!({ "type": "boolean" }),
        ScalarBase::Date => json!({ "type": "string", "format": "date" }),
        ScalarBase::Datetime => json!({ "type": "string", "format": "date-time" }),
        ScalarBase::Uuid => json!({ "type": "string", "format": "uuid" }),
        ScalarBase::Uri => json!({ "type": "string", "format": "uri" }),
    }
}

/// Attach the declared definition description when present (G6).
fn with_description(mut body: Json, common: &Common) -> Json {
    if let Some(description) = &common.description {
        if let Json::Object(object) = &mut body {
            object.insert(
                "description".to_owned(),
                Json::String(description.as_str().to_owned()),
            );
        }
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::CompiledProject;

    fn empty_project() -> CompiledProject {
        CompiledProject {
            model_version: crate::loader::ModelVersion::Current,
            project: None,
            modules: Vec::new(),
            definitions: Vec::new(),
        }
    }

    /// One project declaring the two scalars the table tests reference:
    /// `planner.due_date` (date) and `planner.text` (string).
    fn fixture_project() -> CompiledProject {
        use crate::ir::{Common, ScalarDef, Visibility};
        let common = || Common {
            version: 1,
            description: None,
            derived_from: Vec::new(),
            visibility: Some(Visibility::Project),
            portability: Some(crate::ir::Portability::Portable),
            renamed_from: Vec::new(),
        };
        let scalar = |id: &str, base: ScalarBase| {
            Definition::Scalar(ScalarDef {
                id: crate::ir::SymbolId::of(id),
                common: common(),
                base,
            })
        };
        CompiledProject {
            model_version: crate::loader::ModelVersion::Current,
            project: None,
            modules: Vec::new(),
            definitions: vec![
                scalar("planner.due_date", ScalarBase::Date),
                scalar("planner.text", ScalarBase::String),
            ],
        }
    }

    fn field(name: &str, typeref: TypeRef, required: bool) -> Field {
        Field {
            name: crate::ir::FieldName::of(name),
            r#type: typeref,
            required,
            description: None,
        }
    }

    #[test]
    fn an_optional_ref_renders_one_of_at_31_through_the_fixture() {
        let project = fixture_project();
        let mut mapper = SchemaMapper::new(&project, DocumentVersion::V3_1);
        let due = TypeRef::Optional(Box::new(TypeRef::Ref(crate::ir::SymbolId::of(
            "planner.due_date",
        ))));
        let schema = mapper.map_type(&due);
        assert_eq!(
            schema,
            json!({ "oneOf": [
                { "$ref": "#/components/schemas/PlannerDueDate" },
                { "type": "null" }
            ] })
        );
        assert_eq!(
            mapper.components().get("planner.due_date"),
            Some(&"PlannerDueDate".to_owned())
        );
    }

    #[test]
    fn scalar_bases_project_the_closed_formats() {
        assert_eq!(scalar_json(ScalarBase::String), json!({ "type": "string" }));
        assert_eq!(scalar_json(ScalarBase::Number), json!({ "type": "number" }));
        assert_eq!(
            scalar_json(ScalarBase::Boolean),
            json!({ "type": "boolean" })
        );
        assert_eq!(
            scalar_json(ScalarBase::Date),
            json!({ "type": "string", "format": "date" })
        );
        assert_eq!(
            scalar_json(ScalarBase::Datetime),
            json!({ "type": "string", "format": "date-time" })
        );
        assert_eq!(
            scalar_json(ScalarBase::Uuid),
            json!({ "type": "string", "format": "uuid" })
        );
        assert_eq!(
            scalar_json(ScalarBase::Uri),
            json!({ "type": "string", "format": "uri" })
        );
    }

    #[test]
    fn optional_31_widens_the_type_array_not_the_value_space_of_refs() {
        let project = empty_project();
        let mapper = SchemaMapper::new(&project, DocumentVersion::V3_1);
        assert_eq!(
            mapper.optional(json!({ "type": "string" })),
            json!({ "type": ["string", "null"] })
        );
        assert_eq!(
            mapper.optional(json!({ "type": "string", "format": "date" })),
            json!({ "type": ["string", "null"], "format": "date" })
        );
        assert_eq!(
            mapper.optional(json!({ "$ref": "#/components/schemas/PlannerTaskId" })),
            json!({ "oneOf": [
                { "$ref": "#/components/schemas/PlannerTaskId" },
                { "type": "null" }
            ] })
        );
    }

    #[test]
    fn optional_30_uses_the_nullable_sibling() {
        let project = empty_project();
        let mapper = SchemaMapper::new(&project, DocumentVersion::V3_0);
        assert_eq!(
            mapper.optional(json!({ "type": "string" })),
            json!({ "type": "string", "nullable": true })
        );
        assert_eq!(
            mapper.optional(json!({ "$ref": "#/components/schemas/PlannerTaskId" })),
            json!({ "nullable": true, "allOf": [
                { "$ref": "#/components/schemas/PlannerTaskId" }
            ] })
        );
    }

    #[test]
    fn the_presence_axis_is_orthogonal_to_nullability() {
        // The four required × optional combinations of one field over
        // a `planner.due_date`-shaped optional ref.
        let project = fixture_project();
        let due = || {
            TypeRef::Optional(Box::new(TypeRef::Ref(crate::ir::SymbolId::of(
                "planner.due_date",
            ))))
        };
        let plain = || TypeRef::Ref(crate::ir::SymbolId::of("planner.text"));

        let mut mapper = SchemaMapper::new(&project, DocumentVersion::V3_1);
        // Named refs always project as component `$ref`s.
        let title_ref = json!({ "$ref": "#/components/schemas/PlannerText" });
        let due_ref = json!({ "$ref": "#/components/schemas/PlannerDueDate" });
        // required presence + non-optional type.
        let schema = mapper.object_schema(&[field("title", plain(), true)]);
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("title")));
        assert_eq!(schema["properties"]["title"], title_ref);
        // required presence + optional type (may be null, key must exist).
        let schema = mapper.object_schema(&[field("due", due(), true)]);
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("due")));
        assert_eq!(
            schema["properties"]["due"],
            json!({ "oneOf": [due_ref, { "type": "null" }] })
        );
        // absent presence + non-optional type.
        let schema = mapper.object_schema(&[field("title", plain(), false)]);
        assert!(schema.get("required").is_none());
        assert_eq!(schema["properties"]["title"], title_ref);
        // absent presence + optional type.
        let schema = mapper.object_schema(&[field("due", due(), false)]);
        assert!(schema.get("required").is_none());
        assert_eq!(
            schema["properties"]["due"],
            json!({ "oneOf": [due_ref, { "type": "null" }] })
        );
    }

    #[test]
    fn an_unresolved_symbol_is_a_finding_with_an_open_schema() {
        let project = empty_project();
        let mut mapper = SchemaMapper::new(&project, DocumentVersion::V3_1);
        let schema = mapper.ref_schema(&crate::ir::SymbolId::of("planner.missing"));
        assert_eq!(schema, json!({}));
        let findings = mapper.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].detail, "symbol-unresolved");
        assert_eq!(findings[0].symbol, "planner.missing");
    }

    #[test]
    fn constants_spell_per_declared_version() {
        let project = empty_project();
        let mapper_31 = SchemaMapper::new(&project, DocumentVersion::V3_1);
        assert_eq!(mapper_31.constant(json!(false)), json!({ "const": false }));
        let mapper_30 = SchemaMapper::new(&project, DocumentVersion::V3_0);
        assert_eq!(mapper_30.constant(json!(false)), json!({ "enum": [false] }));
    }
}
