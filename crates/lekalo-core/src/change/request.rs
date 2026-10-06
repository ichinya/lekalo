//! Closed, bounded change request decoding (issue #73).
//!
//! The request is the single pure declaration surface shared by CLI sugar,
//! JSON request files, and a future MCP tool. Decoding rejects duplicate
//! JSON keys before typing (the repository `Unique` convention), enforces
//! the closed operation catalog, and bounds every literal tree. No member
//! is interpreted beyond the closed surface; unknown members fail closed.

use serde::Deserialize;
use serde_json::Value as Json;

use super::catalog::{Catalog, OperationStatus};
use super::version;
use super::{MAX_LITERAL_DEPTH, MAX_OPERATIONS, MAX_REQUEST_BYTES};

/// The maximum bytes of one literal string inside a request payload.
const MAX_LITERAL_STRING_BYTES: usize = 8192;

/// The maximum members of one literal or payload object.
const MAX_LITERAL_MEMBERS: usize = 16;

/// One closed typed change request: an ordered operation batch plus the
/// optional exact Model-digest precondition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeRequest {
    /// Exact `sha256:` digest the whole input set must still have, or `None`.
    pub expect_model: Option<String>,
    /// The operations in semantic order; the order is hashed as written.
    pub operations: Vec<Operation>,
}

/// One typed operation of the implemented vertical.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// `entity-field:add`: add one field to an entity or value-object.
    EntityFieldAdd { entity: String, field: FieldDecl },
    /// `entity-field:remove`: remove one existing field.
    EntityFieldRemove { entity: String, name: String },
    /// `entity-field:rename`: rename one existing field in place.
    EntityFieldRename {
        entity: String,
        name: String,
        to: String,
    },
    /// `enum-case:add`: add one case to an enum.
    EnumCaseAdd { enum_id: String, case: CaseDecl },
    /// A cataloged operation this implementation does not provide. The
    /// payload decoded closed and bounded; preparation refuses later with
    /// the registered unsupported outcome and never mutates.
    Unsupported {
        operation: String,
        subject: Option<String>,
    },
}

impl Operation {
    /// The catalog discriminator of this operation.
    pub fn operation(&self) -> &str {
        match self {
            Self::EntityFieldAdd { .. } => "entity-field:add",
            Self::EntityFieldRemove { .. } => "entity-field:remove",
            Self::EntityFieldRename { .. } => "entity-field:rename",
            Self::EnumCaseAdd { .. } => "enum-case:add",
            Self::Unsupported { operation, .. } => operation,
        }
    }

    /// The natural subject token of this operation, when one is declared.
    pub fn subject(&self) -> Option<&str> {
        match self {
            Self::EntityFieldAdd { entity, .. }
            | Self::EntityFieldRemove { entity, .. }
            | Self::EntityFieldRename { entity, .. } => Some(entity),
            Self::EnumCaseAdd { enum_id, .. } => Some(enum_id),
            Self::Unsupported { subject, .. } => subject.as_deref(),
        }
    }
}

/// One closed field declaration of `entity-field:add`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub r#type: TypeRef,
    pub required: bool,
    pub description: Option<String>,
    /// Declared typed default. Field defaults require the reviewed Model
    /// successor; presence is decoded and preparation refuses explicitly.
    pub default: Option<Literal>,
}

/// One closed enum-case declaration of `enum-case:add`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseDecl {
    pub name: String,
    pub description: Option<String>,
    /// Declared wire value. Wire values require the reviewed Model
    /// successor; presence is decoded and preparation refuses explicitly.
    pub value: Option<Literal>,
}

/// The closed type reference, at most four levels deep including the leaf.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeRef {
    Ref(String),
    List(Box<TypeRef>),
    Optional(Box<TypeRef>),
}

impl TypeRef {
    /// The nesting depth including the leaf.
    pub fn depth(&self) -> usize {
        match self {
            Self::Ref(_) => 1,
            Self::List(inner) | Self::Optional(inner) => 1 + inner.depth(),
        }
    }

    /// The Model type sugar spelling used in source edits (`task.priority`,
    /// `list<task.priority>`, `task.priority?`).
    pub fn to_spelling(&self) -> String {
        match self {
            Self::Ref(id) => id.clone(),
            Self::List(inner) => format!("list<{}>", inner.to_spelling()),
            Self::Optional(inner) => format!("{}?", inner.to_spelling()),
        }
    }
}

/// One bounded JSON literal tree (no floats, bounded depth, bounded strings).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Literal {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Seq(Vec<Literal>),
    Map(Vec<(String, Literal)>),
}

impl Literal {
    /// The compact canonical JSON spelling (sorted object keys).
    pub fn to_json_string(&self) -> String {
        match self {
            Self::Null => "null".to_owned(),
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Str(value) => serde_json::to_string(value).unwrap_or_default(),
            Self::Seq(items) => {
                let inner: Vec<String> = items.iter().map(Literal::to_json_string).collect();
                format!("[{}]", inner.join(","))
            }
            Self::Map(entries) => {
                let mut sorted = entries.clone();
                sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
                let inner: Vec<String> = sorted
                    .iter()
                    .map(|(key, value)| {
                        format!(
                            "{}:{}",
                            serde_json::to_string(key).unwrap_or_default(),
                            value.to_json_string()
                        )
                    })
                    .collect();
                format!("{{{}}}", inner.join(","))
            }
        }
    }
}

/// Why one request refused to decode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestInvalid {
    /// The bytes exceed the request bound.
    OverLimit,
    /// The bytes are not JSON or carry duplicate keys or trailing bytes.
    Malformed,
    /// The discriminator is missing or not the accepted request version.
    UnsupportedVersion,
    /// The top-level shape violates the closed contract.
    Shape(&'static str),
    /// One operation violates its closed payload contract.
    Operation { index: usize, detail: &'static str },
}

impl ChangeRequest {
    /// Decode request bytes: duplicate keys refused, bounds enforced, the
    /// closed surface enforced, and every discriminator catalog-checked.
    pub fn decode(bytes: &[u8]) -> Result<Self, RequestInvalid> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(RequestInvalid::OverLimit);
        }
        let value = unique_json(bytes).map_err(|_| RequestInvalid::Malformed)?;
        let map = value
            .as_object()
            .ok_or(RequestInvalid::Shape("root-not-object"))?;
        for key in map.keys() {
            if !matches!(key.as_str(), "schemaVersion" | "expectModel" | "operations") {
                return Err(RequestInvalid::Shape("unknown-member"));
            }
        }
        let schema_version = map
            .get("schemaVersion")
            .and_then(Json::as_str)
            .ok_or(RequestInvalid::Shape("schema-version-missing"))?;
        if schema_version != version::REQUEST_SCHEMA_VERSION {
            return Err(RequestInvalid::UnsupportedVersion);
        }
        let expect_model = match map.get("expectModel") {
            None | Some(Json::Null) => None,
            Some(value) => {
                let text = value
                    .as_str()
                    .ok_or(RequestInvalid::Shape("expect-model-not-string"))?;
                Some(
                    super::expect_digest(text)
                        .ok_or(RequestInvalid::Shape("expect-model-digest"))?,
                )
            }
        };
        let operations = map
            .get("operations")
            .and_then(Json::as_array)
            .ok_or(RequestInvalid::Shape("operations-missing"))?;
        if operations.is_empty() || operations.len() > MAX_OPERATIONS {
            return Err(RequestInvalid::Shape("operations-bounds"));
        }
        let catalog = Catalog::embedded().map_err(|_| RequestInvalid::Malformed)?;
        let mut decoded = Vec::with_capacity(operations.len());
        for (index, operation) in operations.iter().enumerate() {
            decoded.push(
                decode_operation(&catalog, operation)
                    .map_err(|detail| RequestInvalid::Operation { index, detail })?,
            );
        }
        Ok(Self {
            expect_model,
            operations: decoded,
        })
    }
}

/// Decode one closed operation against the catalog.
fn decode_operation(catalog: &Catalog, value: &Json) -> Result<Operation, &'static str> {
    let map = value.as_object().ok_or("operation-not-object")?;
    let discriminator = map
        .get("operation")
        .and_then(Json::as_str)
        .ok_or("discriminator-missing")?;
    let entry = catalog.entry(discriminator).ok_or("unknown-operation")?;
    if entry.status() == OperationStatus::Unsupported {
        if map.len() - 1 > MAX_LITERAL_MEMBERS {
            return Err("members-over-bound");
        }
        for (key, value) in map {
            if key.len() > 64 {
                return Err("member-key-over-bound");
            }
            literal_of(value, 1).map_err(|_| "payload-invalid")?;
        }
        return Ok(Operation::Unsupported {
            operation: discriminator.to_owned(),
            subject: SUBJECT_MEMBERS
                .iter()
                .find_map(|member| map.get(*member).and_then(Json::as_str))
                .map(str::to_owned),
        });
    }
    match discriminator {
        "entity-field:add" => {
            require_members(
                map,
                &["operation", "entity", "field"],
                &["operation", "entity", "field"],
            )?;
            let entity = member_string(map, "entity")?;
            let field = field_decl(map.get("field").ok_or("missing-member")?)?;
            Ok(Operation::EntityFieldAdd { entity, field })
        }
        "entity-field:remove" => {
            require_members(
                map,
                &["operation", "entity", "name"],
                &["operation", "entity", "name"],
            )?;
            Ok(Operation::EntityFieldRemove {
                entity: member_string(map, "entity")?,
                name: member_string(map, "name")?,
            })
        }
        "entity-field:rename" => {
            require_members(
                map,
                &["operation", "entity", "name", "to"],
                &["operation", "entity", "name", "to"],
            )?;
            Ok(Operation::EntityFieldRename {
                entity: member_string(map, "entity")?,
                name: member_string(map, "name")?,
                to: member_string(map, "to")?,
            })
        }
        "enum-case:add" => {
            require_members(
                map,
                &["operation", "enum", "case"],
                &["operation", "enum", "case"],
            )?;
            let enum_id = member_string(map, "enum")?;
            let case = case_decl(map.get("case").ok_or("missing-member")?)?;
            Ok(Operation::EnumCaseAdd { enum_id, case })
        }
        _ => Err("implemented-without-executor"),
    }
}

/// The closed subject-member vocabulary of cataloged operations.
const SUBJECT_MEMBERS: [&str; 9] = [
    "entity",
    "enum",
    "command",
    "query",
    "symbol",
    "subject",
    "module",
    "operationTarget",
    "step",
];

use crate::result::Status;

/// Reject any member outside the closed member set.
fn require_members(
    map: &serde_json::Map<String, Json>,
    allowed: &[&str],
    required: &[&str],
) -> Result<(), &'static str> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err("unknown-member");
        }
    }
    for required in required {
        if !map.contains_key(*required) {
            return Err("missing-member");
        }
    }
    Ok(())
}

/// Extract one bounded string member.
fn member_string(map: &serde_json::Map<String, Json>, key: &str) -> Result<String, &'static str> {
    let value = map.get(key).ok_or("missing-member")?;
    let text = value.as_str().ok_or("type-invalid")?;
    if text.is_empty() || text.len() > 192 {
        return Err("string-over-bound");
    }
    Ok(text.to_owned())
}

/// Decode one closed optional-text member.
fn member_text(
    map: &serde_json::Map<String, Json>,
    key: &str,
) -> Result<Option<String>, &'static str> {
    match map.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => {
            let text = value.as_str().ok_or("type-invalid")?;
            if text.is_empty() || text.len() > 2000 {
                return Err("string-over-bound");
            }
            Ok(Some(text.to_owned()))
        }
    }
}

/// Decode one closed field declaration.
fn field_decl(value: &Json) -> Result<FieldDecl, &'static str> {
    let map = value.as_object().ok_or("type-invalid")?;
    require_members(
        map,
        &["name", "type", "required", "description", "default"],
        &["name", "type"],
    )?;
    let name = member_string(map, "name")?;
    if !super::field_name_valid(&name) {
        return Err("name-grammar");
    }
    let required = match map.get("required") {
        None | Some(Json::Null) => false,
        Some(Json::Bool(true)) => true,
        Some(_) => return Err("type-invalid"),
    };
    let r#type = type_ref(map.get("type").ok_or("missing-member")?, 1)?;
    // Missing default and explicit null are distinct: `None` means absent,
    // `Some(Literal::Null)` means the field declares a null default.
    let default = match map.get("default") {
        None => None,
        Some(value) => Some(literal_of(value, 1).map_err(|_| "literal-invalid")?),
    };
    Ok(FieldDecl {
        name,
        r#type,
        required,
        description: member_text(map, "description")?,
        default,
    })
}

/// Decode one closed enum-case declaration.
fn case_decl(value: &Json) -> Result<CaseDecl, &'static str> {
    let map = value.as_object().ok_or("type-invalid")?;
    require_members(map, &["name", "description", "value"], &["name"])?;
    let name = member_string(map, "name")?;
    if !super::field_name_valid(&name) {
        return Err("name-grammar");
    }
    let value = match map.get("value") {
        None => None,
        Some(value) => Some(literal_of(value, 1).map_err(|_| "literal-invalid")?),
    };
    Ok(CaseDecl {
        name,
        description: member_text(map, "description")?,
        value,
    })
}

/// Decode one closed bounded type reference.
fn type_ref(value: &Json, depth: usize) -> Result<TypeRef, &'static str> {
    if depth > super::MAX_TYPE_LEVELS {
        return Err("type-depth");
    }
    let map = value.as_object().ok_or("type-invalid")?;
    if map.len() != 1 {
        return Err("type-shape");
    }
    let (key, inner) = map.iter().next().ok_or("type-shape")?;
    match key.as_str() {
        "ref" => {
            let id = inner.as_str().ok_or("type-invalid")?;
            if !super::symbol_id_valid(id) {
                return Err("ref-grammar");
            }
            Ok(TypeRef::Ref(id.to_owned()))
        }
        "list" => Ok(TypeRef::List(Box::new(type_ref(inner, depth + 1)?))),
        "optional" => Ok(TypeRef::Optional(Box::new(type_ref(inner, depth + 1)?))),
        _ => Err("type-shape"),
    }
}

/// Decode one bounded literal tree (no floats, bounded depth and strings).
fn literal_of(value: &Json, depth: usize) -> Result<Literal, ()> {
    if depth > MAX_LITERAL_DEPTH {
        return Err(());
    }
    Ok(match value {
        Json::Null => Literal::Null,
        Json::Bool(value) => Literal::Bool(*value),
        Json::Number(number) => Literal::Int(number.as_i64().ok_or(())?),
        Json::String(text) => {
            if text.len() > MAX_LITERAL_STRING_BYTES {
                return Err(());
            }
            Literal::Str(text.clone())
        }
        Json::Array(items) => {
            if items.len() > 64 {
                return Err(());
            }
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(literal_of(item, depth + 1)?);
            }
            Literal::Seq(out)
        }
        Json::Object(map) => {
            if map.len() > MAX_LITERAL_MEMBERS {
                return Err(());
            }
            let mut out = Vec::with_capacity(map.len());
            for (key, item) in map {
                if key.len() > 64 {
                    return Err(());
                }
                out.push((key.clone(), literal_of(item, depth + 1)?));
            }
            Literal::Map(out)
        }
    })
}

/// Recursive duplicate-key refusal before typed decoding (repository
/// `Unique` convention); `serde_json::Value` alone discards this evidence.
fn unique_json(bytes: &[u8]) -> Result<Json, ()> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = Unique::deserialize(&mut de).map_err(|_| ())?.0;
    de.end().map_err(|_| ())?;
    Ok(value)
}

struct Unique(Json);

impl<'de> serde::Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Unique;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("unique-key bounded JSON")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(Unique(Json::Null))
            }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(Unique(value.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Unique(value.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Unique(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                // Floats are outside the bounded literal grammar.
                Err(E::custom("float"))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.len() > MAX_LITERAL_STRING_BYTES {
                    return Err(E::custom("string-over-bound"));
                }
                Ok(Unique(value.into()))
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                if value.len() > MAX_LITERAL_STRING_BYTES {
                    return Err(E::custom("string-over-bound"));
                }
                Ok(Unique(value.into()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = access.next_element::<Unique>()? {
                    items.push(item.0);
                }
                Ok(Unique(Json::Array(items)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<Self::Value, A::Error> {
                let mut map = serde_json::Map::new();
                while let Some(key) = access.next_key::<String>()? {
                    if map.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate-key"));
                    }
                    if key.len() > MAX_LITERAL_STRING_BYTES {
                        return Err(serde::de::Error::custom("string-over-bound"));
                    }
                    map.insert(key, access.next_value::<Unique>()?.0);
                }
                Ok(Unique(Json::Object(map)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "{\"schemaVersion\":\"lekalo/change-request/v0.8.0\",\"operations\":[";

    fn request(operations: &str) -> String {
        format!("{HEADER}{operations}]}}")
    }

    fn decode_ok(text: &str) -> ChangeRequest {
        ChangeRequest::decode(text.as_bytes()).expect("request decodes")
    }

    fn decode_err(text: &str) -> RequestInvalid {
        ChangeRequest::decode(text.as_bytes()).expect_err("request refuses")
    }

    #[test]
    fn add_field_decodes_closed() {
        let text = request(
            "{\"operation\":\"entity-field:add\",\"entity\":\"tasks.task\",             \"field\":{\"name\":\"priority\",\"type\":{\"ref\":\"task.priority\"},\"required\":true}}",
        );
        let req = decode_ok(&text);
        assert_eq!(req.operations.len(), 1);
        assert_eq!(req.operations[0].operation(), "entity-field:add");
        assert_eq!(req.operations[0].subject(), Some("tasks.task"));
    }

    #[test]
    fn duplicate_keys_are_refused() {
        let text = request(
            "{\"operation\":\"entity-field:add\",\"entity\":\"tasks.task\",\"entity\":\"tasks.other\",             \"field\":{\"name\":\"priority\",\"type\":{\"ref\":\"task.priority\"}}}",
        );
        assert_eq!(decode_err(&text), RequestInvalid::Malformed);
    }

    #[test]
    fn unknown_operation_is_request_invalid_not_unsupported() {
        let text = request("{\"operation\":\"entity-field:append\",\"entity\":\"tasks.task\"}");
        assert!(matches!(
            decode_err(&text),
            RequestInvalid::Operation {
                detail: "unknown-operation",
                ..
            }
        ));
    }

    #[test]
    fn cataloged_unsupported_operation_decodes_and_carries_subject() {
        let text = request(
            "{\"operation\":\"definition:add\",\"kind\":\"command\",\"module\":\"tasks\",             \"definition\":{\"id\":\"tasks.complete\"}}",
        );
        let req = decode_ok(&text);
        match &req.operations[0] {
            Operation::Unsupported { operation, subject } => {
                assert_eq!(operation, "definition:add");
                assert_eq!(subject.as_deref(), Some("tasks"));
            }
            other => panic!("unexpected operation: {other:?}"),
        }
    }

    #[test]
    fn wrong_discriminator_version_is_unsupported_version() {
        let text = "{\"schemaVersion\":\"lekalo/change-request/v0.6.4\",\"operations\":[{\"operation\":\"entity-field:add\",\"entity\":\"a.b\",\"field\":{\"name\":\"x\",\"type\":{\"ref\":\"a.b\"}}}]}".to_owned();
        assert_eq!(decode_err(&text), RequestInvalid::UnsupportedVersion);
    }

    #[test]
    fn unknown_member_fails_closed() {
        let text = request(
            "{\"operation\":\"entity-field:remove\",\"entity\":\"tasks.task\",\"name\":\"done\",\"force\":true}",
        );
        assert!(matches!(
            decode_err(&text),
            RequestInvalid::Operation {
                detail: "unknown-member",
                ..
            }
        ));
    }

    #[test]
    fn float_literal_is_outside_the_bounded_grammar() {
        let text = request(
            "{\"operation\":\"entity-field:add\",\"entity\":\"tasks.task\",             \"field\":{\"name\":\"priority\",\"type\":{\"ref\":\"task.priority\"},\"default\":1.5}}",
        );
        assert_eq!(decode_err(&text), RequestInvalid::Malformed);
    }

    #[test]
    fn deep_type_refuses() {
        let mut inner = "{\"ref\":\"a.b\"}".to_owned();
        for _ in 0..crate::change::MAX_TYPE_LEVELS {
            inner = format!("{{\"list\":{inner}}}");
        }
        let text = request(&format!(
            "{{\"operation\":\"entity-field:add\",\"entity\":\"tasks.task\",\"field\":{{\"name\":\"priority\",\"type\":{inner}}}}}"
        ));
        assert!(matches!(
            decode_err(&text),
            RequestInvalid::Operation {
                detail: "type-depth",
                ..
            }
        ));
    }

    #[test]
    fn literal_spelling_is_canonical() {
        let literal = Literal::Map(vec![
            ("b".to_owned(), Literal::Int(2)),
            ("a".to_owned(), Literal::Str("x".to_owned())),
        ]);
        assert_eq!(literal.to_json_string(), "{\"a\":\"x\",\"b\":2}");
    }
}
