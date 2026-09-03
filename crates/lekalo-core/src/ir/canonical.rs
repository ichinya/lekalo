//! Deterministic canonical serialization of the typed IR (issue #8).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, object keys in unsigned
//! UTF-8 byte order, `definitions`/`modules`/`imports` ordered by semantic
//! ID, and every semantically ordered array (fields, values, references,
//! history) preserved in decoded order. String escaping is the RFC 8259
//! mandatory-escape set with all other Unicode scalars verbatim — the same
//! writer the #7 loader uses, so equivalent inputs produce identical bytes.
//!
//! Every object is serialized as a list of `(key, value)` pairs sorted by
//! raw key bytes before writing, so key order is structurally correct for
//! every type; a duplicated key is a programming error and panics in debug
//! builds.
use super::{
    CommandDef, Common, CompiledProject, Definition, EffectDef, EndpointDef, EntityDef, EnumDef,
    EnumValue, EventDef, Field, IdRegistry, Module, ModuleId, PolicyDef, Project, QueryDef,
    RenameHistoryEntry, RequirementId, ScalarDef, ScenarioDef, SymbolId, TargetBindingDef, Text,
    Tombstone, TypeRef, ValueObjectDef, IDENTITY,
};
use crate::loader::canonical::write_json_string;

impl CompiledProject {
    /// The canonical IR bytes: one deterministic compact JSON document.
    pub fn to_canonical_json(&self) -> String {
        let mut fields: Vec<(&'static str, String)> = Vec::new();
        fields.push(("contract", string(IDENTITY)));
        fields.push((
            "definitions",
            sorted_by(&self.definitions, Definition::write_json, |definition| {
                definition.id().as_str()
            }),
        ));
        fields.push(("modelVersion", string(self.model_version.as_str())));
        fields.push((
            "modules",
            sorted_by(&self.modules, Module::write_json, |module| {
                module.id.as_str()
            }),
        ));
        if let Some(project) = &self.project {
            fields.push(("project", project.write_json()));
        }
        object(&mut fields)
    }
}

/// Emit a field list as one object with byte-sorted keys.
fn object(fields: &mut [(&'static str, String)]) -> String {
    fields.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    debug_assert!(fields.windows(2).all(|pair| pair[0].0 != pair[1].0));
    let mut out = String::new();
    out.push('{');
    for (position, (key, value)) in fields.iter().enumerate() {
        if position > 0 {
            out.push(',');
        }
        write_json_string(key, &mut out);
        out.push(':');
        out.push_str(value);
    }
    out.push('}');
    out
}

/// One serialized `"key": value` pair.
fn pair(key: &'static str, value: String) -> (&'static str, String) {
    (key, value)
}

/// A quoted JSON string value.
fn string(text: &str) -> String {
    let mut out = String::new();
    write_json_string(text, &mut out);
    out
}

/// A closed-enum literal value.
fn literal(text: &str) -> String {
    string(text)
}

/// An unsigned integer value.
fn number(value: u64) -> String {
    value.to_string()
}

/// A boolean value.
fn boolean(value: bool) -> String {
    if value {
        "true".to_owned()
    } else {
        "false".to_owned()
    }
}

/// A JSON array from an ordered slice.
fn array(items: impl IntoIterator<Item = String>) -> String {
    let mut out = String::new();
    out.push('[');
    for (position, item) in items.into_iter().enumerate() {
        if position > 0 {
            out.push(',');
        }
        out.push_str(&item);
    }
    out.push(']');
    out
}

/// A JSON array sorted by its natural key (set-like surfaces).
fn sorted_by<T>(items: &[T], write: fn(&T) -> String, key: fn(&T) -> &str) -> String {
    let mut order: Vec<&T> = items.iter().collect();
    order.sort_by(|left, right| key(left).as_bytes().cmp(key(right).as_bytes()));
    array(order.into_iter().map(write))
}

/// An optional field, present only when the value is present.
fn optional(key: &'static str, value: Option<String>) -> Option<(&'static str, String)> {
    value.map(|value| (key, value))
}

/// A set-like array field, present only when non-empty.
fn present_list(key: &'static str, value: String) -> Option<(&'static str, String)> {
    if value == "[]" {
        None
    } else {
        Some((key, value))
    }
}

fn text_value(text: &Text) -> String {
    string(text.as_str())
}

fn symbol(id: &SymbolId) -> String {
    string(id.as_str())
}

fn module_id(id: &ModuleId) -> String {
    string(id.as_str())
}

fn requirement(id: &RequirementId) -> String {
    string(id.as_str())
}

impl Common {
    /// The common fields, without `id`/`kind` (which each definition places
    /// in its own byte-sorted sequence).
    fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields: Vec<(&'static str, String)> = Vec::new();
        fields.extend(present_list(
            "derived_from",
            sorted_by(&self.derived_from, requirement, RequirementId::as_str),
        ));
        fields.extend(optional(
            "description",
            self.description.as_ref().map(text_value),
        ));
        fields.extend(optional(
            "portability",
            self.portability.map(|value| literal(value.as_str())),
        ));
        fields.extend(present_list(
            "renamed_from",
            sorted_by(&self.renamed_from, symbol, SymbolId::as_str),
        ));
        fields.push(pair("version", number(self.version)));
        fields.extend(optional(
            "visibility",
            self.visibility.map(|value| literal(value.as_str())),
        ));
        fields
    }
}

impl Project {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair("id", string(self.id.as_str())));
        fields.extend(optional(
            "id_registry",
            self.id_registry.as_ref().map(IdRegistry::write_json),
        ));
        fields.push(pair("kind", literal("project")));
        object(&mut fields)
    }
}

impl Module {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair("id", string(self.id.as_str())));
        fields.extend(present_list(
            "imports",
            sorted_by(&self.imports, module_id, ModuleId::as_str),
        ));
        fields.push(pair("kind", literal("module")));
        object(&mut fields)
    }
}

impl Definition {
    fn write_json(&self) -> String {
        match self {
            Self::Scalar(definition) => definition.write_json(),
            Self::Enum(definition) => definition.write_json(),
            Self::ValueObject(definition) => definition.write_json(),
            Self::Entity(definition) => definition.write_json(),
            Self::Command(definition) => definition.write_json(),
            Self::Query(definition) => definition.write_json(),
            Self::Policy(definition) => definition.write_json(),
            Self::Event(definition) => definition.write_json(),
            Self::Effect(definition) => definition.write_json(),
            Self::Endpoint(definition) => definition.write_json(),
            Self::Scenario(definition) => definition.write_json(),
            Self::TargetBinding(definition) => definition.write_json(),
        }
    }
}

impl ScalarDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair("base", literal(self.base.as_str())));
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("kind", literal("scalar")));
        object(&mut fields)
    }
}

impl EnumDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("kind", literal("enum")));
        fields.push(pair(
            "values",
            array(self.values.iter().map(EnumValue::write_json)),
        ));
        object(&mut fields)
    }
}

impl EnumValue {
    fn write_json(&self) -> String {
        let mut fields: Vec<(&'static str, String)> = Vec::new();
        fields.extend(optional(
            "description",
            self.description.as_ref().map(text_value),
        ));
        fields.push(pair("value", string(self.value.as_str())));
        object(&mut fields)
    }
}

impl ValueObjectDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair(
            "fields",
            array(self.fields.iter().map(Field::write_json)),
        ));
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("kind", literal("value-object")));
        object(&mut fields)
    }
}

impl EntityDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair(
            "fields",
            array(self.fields.iter().map(Field::write_json)),
        ));
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair(
            "identity",
            array(self.identity.iter().map(|name| string(name.as_str()))),
        ));
        fields.push(pair("kind", literal("entity")));
        object(&mut fields)
    }
}

impl CommandDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.extend(present_list(
            "effects",
            sorted_by(&self.effects, symbol, SymbolId::as_str),
        ));
        fields.push(pair("id", symbol(&self.id)));
        fields.extend(present_list(
            "input",
            array(self.input.iter().map(Field::write_json)),
        ));
        fields.push(pair("kind", literal("command")));
        object(&mut fields)
    }
}

impl QueryDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("kind", literal("query")));
        fields.push(pair(
            "reads",
            sorted_by(&self.reads, symbol, SymbolId::as_str),
        ));
        fields.extend(optional(
            "returns",
            self.returns.as_ref().map(TypeRef::write_json),
        ));
        object(&mut fields)
    }
}

impl PolicyDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair(
            "applies_to",
            sorted_by(&self.applies_to, symbol, SymbolId::as_str),
        ));
        fields.push(pair("decision", literal(self.decision.as_str())));
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("kind", literal("policy")));
        object(&mut fields)
    }
}

impl EventDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("kind", literal("event")));
        fields.extend(present_list(
            "payload",
            array(self.payload.iter().map(Field::write_json)),
        ));
        object(&mut fields)
    }
}

impl EffectDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.extend(present_list(
            "emits",
            sorted_by(&self.emits, symbol, SymbolId::as_str),
        ));
        fields.push(pair("entity", symbol(&self.entity)));
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("kind", literal("effect")));
        fields.push(pair("operation", literal(self.operation.as_str())));
        object(&mut fields)
    }
}

impl EndpointDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("invokes", symbol(&self.invokes)));
        fields.push(pair("kind", literal("endpoint")));
        fields.push(pair("method", literal(self.method.as_str())));
        fields.push(pair("path", string(self.path.as_str())));
        object(&mut fields)
    }
}

impl ScenarioDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.extend(present_list(
            "covers",
            sorted_by(&self.covers, symbol, SymbolId::as_str),
        ));
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("kind", literal("scenario")));
        fields.push(pair("summary", text_value(&self.summary)));
        object(&mut fields)
    }
}

impl TargetBindingDef {
    fn write_json(&self) -> String {
        let mut fields = self.common.fields();
        fields.push(pair("id", symbol(&self.id)));
        fields.push(pair("kind", literal("target-binding")));
        fields.push(pair("target", string(self.target.as_str())));
        object(&mut fields)
    }
}

impl Field {
    fn write_json(&self) -> String {
        let mut fields: Vec<(&'static str, String)> = Vec::new();
        fields.extend(optional(
            "description",
            self.description.as_ref().map(text_value),
        ));
        fields.push(pair("name", string(self.name.as_str())));
        if self.required {
            fields.push(pair("required", boolean(true)));
        }
        fields.push(pair("type", self.r#type.write_json()));
        object(&mut fields)
    }
}

impl TypeRef {
    fn write_json(&self) -> String {
        match self {
            Self::Ref(id) => object(&mut [pair("ref", symbol(id))]),
            Self::List(inner) => object(&mut [pair("list", inner.write_json())]),
            Self::Optional(inner) => object(&mut [pair("optional", inner.write_json())]),
        }
    }
}

impl IdRegistry {
    fn write_json(&self) -> String {
        let mut fields: Vec<(&'static str, String)> = Vec::new();
        fields.extend(present_list(
            "rename_history",
            array(
                self.rename_history
                    .iter()
                    .map(RenameHistoryEntry::write_json),
            ),
        ));
        fields.extend(present_list(
            "tombstones",
            array(self.tombstones.iter().map(Tombstone::write_json)),
        ));
        object(&mut fields)
    }
}

impl RenameHistoryEntry {
    fn write_json(&self) -> String {
        let mut fields: Vec<(&'static str, String)> = Vec::new();
        fields.push(pair("definition_version", number(self.definition_version)));
        fields.push(pair("from", symbol(&self.from)));
        fields.extend(optional("note", self.note.as_ref().map(text_value)));
        if self.same_identity {
            fields.push(pair("same_identity", boolean(true)));
        }
        fields.push(pair("to", symbol(&self.to)));
        object(&mut fields)
    }
}

impl Tombstone {
    fn write_json(&self) -> String {
        match self {
            Self::Replaced {
                id,
                replaced_by,
                since,
            } => object(&mut [
                pair("id", symbol(id)),
                pair("reason", literal("replaced")),
                pair("replaced_by", symbol(replaced_by)),
                pair("since", number(*since)),
            ]),
            Self::Deleted { id, since } => object(&mut [
                pair("id", symbol(id)),
                pair("reason", literal("deleted")),
                pair("since", number(*since)),
            ]),
        }
    }
}
