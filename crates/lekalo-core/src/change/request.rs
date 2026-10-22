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

/// One exact managed-coverage source used to admit a module transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipCoverage {
    /// The exact target assignment in the existing ownership policy.
    pub target: String,
    /// A bounded project-relative evidence coordinate (never a write path).
    pub path: String,
    /// Digest of the exact coverage source bytes.
    pub digest: String,
}

/// One typed operation of the implemented vertical.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Change one module's declared mode in the existing managed policy.
    OwnershipSet {
        subject: String,
        mode: crate::managed::Mode,
        ownership_digest: String,
        coverage: Vec<OwnershipCoverage>,
    },
    /// Replace one command/query error union in the exact project registry.
    OperationErrorSet {
        target: String,
        errors: Vec<String>,
        registry_digest: String,
    },
    /// Explicit confirmation of one exact observed binding proposal.
    BindingConfirm {
        symbol: String,
        proposal: String,
        candidate: String,
        index_digest: String,
    },
    /// Relocate one physical module home while preserving its semantic ID.
    ModuleMove { module: String, directory: String },
    /// Move or rename one semantic symbol, recording the same-identity edge.
    SymbolMove {
        symbol: String,
        to_module: String,
        to_name: Option<String>,
    },
    /// A registered operation lowered to the same sealed semantic executor.
    /// Wire intent stays intact for hashing, audit and idempotency.
    Lowered {
        operation: &'static str,
        wire: Json,
        executable: Box<Operation>,
    },
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
    /// `enum-case:change`: rename/redescribe one existing case (wire
    /// values need the reviewed Model successor and stay refused).
    EnumCaseChange {
        enum_id: String,
        name: String,
        change: CaseChangeDecl,
    },
    /// `definition:add`: add one command, query, event, policy, or
    /// scenario definition to a module.
    DefinitionAdd {
        kind: String,
        module: String,
        definition: DefinitionDecl,
    },
    /// `command-effect:add`: append one effect reference to a command.
    /// A declared emission (`emit`) is decoded and refused with the
    /// registered unsupported member until the emission successor lands.
    CommandEffectAdd {
        command: String,
        effect: String,
        emit: Option<String>,
    },
    /// `command-effect:remove`: remove one effect reference from a command.
    CommandEffectRemove {
        command: String,
        effect: String,
        emit: Option<String>,
    },
    /// `operation-input:set`: replace a command or successor query input field list.
    OperationInputSet {
        target: String,
        input: Vec<FieldDecl>,
    },
    /// `operation-output:set`: set a query or successor command return type.
    OperationOutputSet { target: String, r#type: TypeRef },
    /// Add or remove an existing read dependency of a query.
    QueryEffect {
        query: String,
        effect: String,
        add: bool,
    },
    /// Link or unlink a requirement identity in a symbol's derived_from list.
    Requirement {
        symbol: String,
        requirement: String,
        link: bool,
    },
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
            Self::OwnershipSet { .. } => "ownership:set",
            Self::OperationErrorSet { .. } => "operation-error:set",
            Self::BindingConfirm { .. } => "binding:add",
            Self::ModuleMove { .. } => "module:move",
            Self::SymbolMove { .. } => "symbol:move",
            Self::Lowered { operation, .. } => operation,
            Self::EntityFieldAdd { .. } => "entity-field:add",
            Self::EntityFieldRemove { .. } => "entity-field:remove",
            Self::EntityFieldRename { .. } => "entity-field:rename",
            Self::EnumCaseAdd { .. } => "enum-case:add",
            Self::EnumCaseChange { .. } => "enum-case:change",
            Self::DefinitionAdd { .. } => "definition:add",
            Self::CommandEffectAdd { .. } => "command-effect:add",
            Self::CommandEffectRemove { .. } => "command-effect:remove",
            Self::OperationInputSet { .. } => "operation-input:set",
            Self::OperationOutputSet { .. } => "operation-output:set",
            Self::QueryEffect { add: true, .. } => "query-effect:add",
            Self::QueryEffect { add: false, .. } => "query-effect:remove",
            Self::Requirement { link: true, .. } => "requirement:link",
            Self::Requirement { link: false, .. } => "requirement:unlink",
            Self::Unsupported { operation, .. } => operation,
        }
    }

    /// The natural subject token of this operation, when one is declared.
    pub fn subject(&self) -> Option<&str> {
        match self {
            Self::OwnershipSet { subject, .. } => Some(subject),
            Self::OperationErrorSet { target, .. } => Some(target),
            Self::BindingConfirm { symbol, .. } => Some(symbol),
            Self::ModuleMove { module, .. } => Some(module),
            Self::SymbolMove { symbol, .. } => Some(symbol),
            Self::Lowered { executable, .. } => executable.subject(),
            Self::EntityFieldAdd { entity, .. }
            | Self::EntityFieldRemove { entity, .. }
            | Self::EntityFieldRename { entity, .. } => Some(entity),
            Self::EnumCaseAdd { enum_id, .. } | Self::EnumCaseChange { enum_id, .. } => {
                Some(enum_id)
            }
            Self::DefinitionAdd { module, .. } => Some(module),
            Self::CommandEffectAdd { command, .. } | Self::CommandEffectRemove { command, .. } => {
                Some(command)
            }
            Self::OperationInputSet { target, .. } | Self::OperationOutputSet { target, .. } => {
                Some(target)
            }
            Self::QueryEffect { query, .. } => Some(query),
            Self::Requirement { symbol, .. } => Some(symbol),
            Self::Unsupported { subject, .. } => subject.as_deref(),
        }
    }

    /// The canonical closed JSON of this operation (sorted keys, compact).
    pub fn to_canonical_json(&self) -> Json {
        let mut map = serde_json::Map::new();
        map.insert(
            "operation".to_owned(),
            Json::String(self.operation().to_owned()),
        );
        match self {
            Self::OwnershipSet {
                subject,
                mode,
                ownership_digest,
                coverage,
            } => {
                map.insert("subject".to_owned(), Json::String(subject.clone()));
                map.insert("mode".to_owned(), serde_json::json!(mode));
                map.insert("evidence".to_owned(), serde_json::json!({
                    "ownershipDigest": ownership_digest,
                    "coverage": coverage.iter().map(|pin| serde_json::json!({"target":pin.target,"path":pin.path,"digest":pin.digest})).collect::<Vec<_>>()
                }));
            }
            Self::OperationErrorSet {
                target,
                errors,
                registry_digest,
            } => {
                map.insert("operationTarget".to_owned(), Json::String(target.clone()));
                map.insert(
                    "definition".to_owned(),
                    serde_json::json!({"errors": errors, "registryDigest": registry_digest}),
                );
            }
            Self::BindingConfirm {
                symbol,
                proposal,
                candidate,
                index_digest,
            } => {
                map.insert("symbol".to_owned(), Json::String(symbol.clone()));
                map.insert(
                    "definition".to_owned(),
                    serde_json::json!({
                        "kind": "observed-confirmation", "proposal": proposal,
                        "candidate": candidate, "indexDigest": index_digest,
                    }),
                );
            }
            Self::ModuleMove { module, directory } => {
                map.insert("module".to_owned(), Json::String(module.clone()));
                map.insert("directory".to_owned(), Json::String(directory.clone()));
            }
            Self::SymbolMove {
                symbol,
                to_module,
                to_name,
            } => {
                map.insert("symbol".to_owned(), Json::String(symbol.clone()));
                map.insert("toModule".to_owned(), Json::String(to_module.clone()));
                if let Some(name) = to_name {
                    map.insert("toName".to_owned(), Json::String(name.clone()));
                }
            }
            Self::Lowered { wire, .. } => return wire.clone(),
            Self::EntityFieldAdd { entity, field } => {
                map.insert("entity".to_owned(), Json::String(entity.clone()));
                map.insert("field".to_owned(), field.to_canonical_json());
            }
            Self::EntityFieldRemove { entity, name } => {
                map.insert("entity".to_owned(), Json::String(entity.clone()));
                map.insert("name".to_owned(), Json::String(name.clone()));
            }
            Self::EntityFieldRename { entity, name, to } => {
                map.insert("entity".to_owned(), Json::String(entity.clone()));
                map.insert("name".to_owned(), Json::String(name.clone()));
                map.insert("to".to_owned(), Json::String(to.clone()));
            }
            Self::EnumCaseAdd { enum_id, case } => {
                map.insert("enum".to_owned(), Json::String(enum_id.clone()));
                map.insert("case".to_owned(), case.to_canonical_json());
            }
            Self::EnumCaseChange {
                enum_id,
                name,
                change,
            } => {
                map.insert("enum".to_owned(), Json::String(enum_id.clone()));
                map.insert("name".to_owned(), Json::String(name.clone()));
                map.insert("definition".to_owned(), change.to_canonical_json());
            }
            Self::DefinitionAdd {
                kind,
                module,
                definition,
            } => {
                map.insert("kind".to_owned(), Json::String(kind.clone()));
                map.insert("module".to_owned(), Json::String(module.clone()));
                map.insert("definition".to_owned(), definition.to_canonical_json());
            }
            Self::CommandEffectAdd {
                command,
                effect,
                emit,
            }
            | Self::CommandEffectRemove {
                command,
                effect,
                emit,
            } => {
                map.insert("command".to_owned(), Json::String(command.clone()));
                map.insert("effect".to_owned(), Json::String(effect.clone()));
                if let Some(emit) = emit {
                    map.insert("emit".to_owned(), Json::String(emit.clone()));
                }
            }
            Self::OperationInputSet { target, input } => {
                map.insert("operationTarget".to_owned(), Json::String(target.clone()));
                let fields: Vec<Json> = input.iter().map(FieldDecl::to_canonical_json).collect();
                let mut payload = serde_json::Map::new();
                payload.insert("input".to_owned(), Json::Array(fields));
                map.insert("definition".to_owned(), Json::Object(payload));
            }
            Self::OperationOutputSet { target, r#type } => {
                map.insert("operationTarget".to_owned(), Json::String(target.clone()));
                map.insert("type".to_owned(), r#type.to_canonical_json());
            }
            Self::QueryEffect { query, effect, .. } => {
                map.insert("query".to_owned(), Json::String(query.clone()));
                map.insert("effect".to_owned(), Json::String(effect.clone()));
            }
            Self::Requirement {
                symbol,
                requirement,
                ..
            } => {
                map.insert("symbol".to_owned(), Json::String(symbol.clone()));
                map.insert("requirement".to_owned(), Json::String(requirement.clone()));
            }
            Self::Unsupported { .. } => {
                // Unsupported rows never carry canonical bytes: they are
                // refused before any plan is sealed.
            }
        }
        Json::Object(map)
    }

    pub(crate) fn executable(&self) -> &Operation {
        match self {
            Self::Lowered { executable, .. } => executable.executable(),
            other => other,
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

impl FieldDecl {
    /// The canonical closed JSON of this declaration (sorted keys).
    pub(crate) fn to_canonical_json(&self) -> Json {
        let mut map = serde_json::Map::new();
        map.insert("name".to_owned(), Json::String(self.name.clone()));
        map.insert("type".to_owned(), self.r#type.to_canonical_json());
        if self.required {
            map.insert("required".to_owned(), Json::Bool(true));
        }
        if let Some(description) = &self.description {
            map.insert("description".to_owned(), Json::String(description.clone()));
        }
        if let Some(default) = &self.default {
            map.insert("default".to_owned(), default.to_json_value());
        }
        Json::Object(map)
    }
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

impl CaseDecl {
    /// The canonical closed JSON of this declaration (sorted keys).
    pub(crate) fn to_canonical_json(&self) -> Json {
        let mut map = serde_json::Map::new();
        map.insert("name".to_owned(), Json::String(self.name.clone()));
        if let Some(description) = &self.description {
            map.insert("description".to_owned(), Json::String(description.clone()));
        }
        if let Some(value) = &self.value {
            map.insert("value".to_owned(), value.to_json_value());
        }
        Json::Object(map)
    }
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

    /// The canonical closed JSON of this type reference.
    pub(crate) fn to_canonical_json(&self) -> Json {
        match self {
            Self::Ref(id) => serde_json::json!({ "ref": id }),
            Self::List(inner) => serde_json::json!({ "list": inner.to_canonical_json() }),
            Self::Optional(inner) => {
                serde_json::json!({ "optional": inner.to_canonical_json() })
            }
        }
    }
}

/// One closed enum-case change declaration of `enum-case:change`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseChangeDecl {
    /// The new case name, when the change renames it.
    pub rename: Option<String>,
    /// The replaced description, when the change redescribes it.
    pub description: Option<String>,
    /// The new explicit encoding; `Some(Null)` is a JSON null encoding.
    pub value: Option<Literal>,
}

impl CaseChangeDecl {
    /// The canonical closed JSON of this declaration (sorted keys).
    pub(crate) fn to_canonical_json(&self) -> Json {
        let mut map = serde_json::Map::new();
        if let Some(rename) = &self.rename {
            map.insert("rename".to_owned(), Json::String(rename.clone()));
        }
        if let Some(description) = &self.description {
            map.insert("description".to_owned(), Json::String(description.clone()));
        }
        if let Some(value) = &self.value {
            map.insert("value".to_owned(), value.to_json_value());
        }
        Json::Object(map)
    }
}

/// One closed definition declaration of `definition:add`. Exactly the
/// members of the declared kind are present; the others default empty.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefinitionDecl {
    /// The definition name inside its module (the full semantic id is
    /// `module.name`).
    pub name: String,
    pub description: Option<String>,
    /// `command`: the input field list.
    pub input: Vec<FieldDecl>,
    /// `command`: the referenced effect definitions.
    pub effects: Vec<String>,
    /// `query`: the read symbol ids.
    pub reads: Vec<String>,
    /// A query or successor command's returned type.
    pub returns: Option<TypeRef>,
    /// `event`: the payload field list.
    pub payload: Vec<FieldDecl>,
    /// `policy`: the scoped operation ids.
    pub applies_to: Vec<String>,
    /// `policy`: the closed decision (`allow` / `deny`).
    pub decision: Option<String>,
    /// `scenario`: the summary text.
    pub summary: Option<String>,
    /// `scenario`: the covered operation ids.
    pub covers: Vec<String>,
    /// Internal canonical target-binding lowering; not a definition:add member.
    pub target: Option<String>,
}

impl DefinitionDecl {
    /// The canonical closed JSON of this declaration: only the members
    /// the declared kind uses are emitted.
    pub(crate) fn to_canonical_json(&self) -> Json {
        let mut map = serde_json::Map::new();
        map.insert("name".to_owned(), Json::String(self.name.clone()));
        if let Some(description) = &self.description {
            map.insert("description".to_owned(), Json::String(description.clone()));
        }
        if let Some(target) = &self.target {
            map.insert("target".to_owned(), Json::String(target.clone()));
        }
        if !self.input.is_empty() {
            map.insert(
                "input".to_owned(),
                Json::Array(
                    self.input
                        .iter()
                        .map(FieldDecl::to_canonical_json)
                        .collect(),
                ),
            );
        }
        if !self.effects.is_empty() {
            map.insert(
                "effects".to_owned(),
                Json::Array(
                    self.effects
                        .iter()
                        .map(|id| Json::String(id.clone()))
                        .collect(),
                ),
            );
        }
        if !self.reads.is_empty() {
            map.insert(
                "reads".to_owned(),
                Json::Array(
                    self.reads
                        .iter()
                        .map(|id| Json::String(id.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(returns) = &self.returns {
            map.insert("returns".to_owned(), returns.to_canonical_json());
        }
        if !self.payload.is_empty() {
            map.insert(
                "payload".to_owned(),
                Json::Array(
                    self.payload
                        .iter()
                        .map(FieldDecl::to_canonical_json)
                        .collect(),
                ),
            );
        }
        if !self.applies_to.is_empty() {
            map.insert(
                "appliesTo".to_owned(),
                Json::Array(
                    self.applies_to
                        .iter()
                        .map(|id| Json::String(id.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(decision) = &self.decision {
            map.insert("decision".to_owned(), Json::String(decision.clone()));
        }
        if let Some(summary) = &self.summary {
            map.insert("summary".to_owned(), Json::String(summary.clone()));
        }
        if !self.covers.is_empty() {
            map.insert(
                "covers".to_owned(),
                Json::Array(
                    self.covers
                        .iter()
                        .map(|id| Json::String(id.clone()))
                        .collect(),
                ),
            );
        }
        Json::Object(map)
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

    /// The literal as a `serde_json` value with sorted object keys.
    pub(crate) fn to_json_value(&self) -> Json {
        match self {
            Self::Null => Json::Null,
            Self::Bool(value) => Json::Bool(*value),
            Self::Int(value) => Json::Number((*value).into()),
            Self::Str(value) => Json::String(value.clone()),
            Self::Seq(items) => Json::Array(items.iter().map(Literal::to_json_value).collect()),
            Self::Map(entries) => {
                let mut sorted = entries.clone();
                sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
                let mut map = serde_json::Map::new();
                for (key, value) in sorted {
                    map.insert(key, value.to_json_value());
                }
                Json::Object(map)
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
    /// The canonical compact request bytes: sorted object keys, compact
    /// separators, operations in semantic order. Pretty file JSON and
    /// compact CLI-sugar bytes of one request canonicalize identically,
    /// so the request digest pins the intent, not its presentation.
    pub fn to_canonical_json_string(&self) -> String {
        let operations: Vec<Json> = self
            .operations
            .iter()
            .map(|operation| operation.to_canonical_json())
            .collect();
        let mut root = serde_json::Map::new();
        root.insert(
            "schemaVersion".to_owned(),
            Json::String(version::REQUEST_SCHEMA_VERSION.to_owned()),
        );
        if let Some(expect_model) = &self.expect_model {
            root.insert("expectModel".to_owned(), Json::String(expect_model.clone()));
        }
        root.insert("operations".to_owned(), Json::Array(operations));
        // serde_json Map iteration follows insertion order here, and the
        // members above are inserted sorted; literal trees sort inside
        // `to_canonical_json`.
        serde_json::to_string(&Json::Object(root)).unwrap_or_default()
    }

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
            None => None,
            Some(Json::Null) => {
                // The closed schema types expectModel as a sha256 string;
                // explicit null is not absence (round 2, finding 17).
                return Err(RequestInvalid::Shape("expect-model-not-string"));
            }
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
                decode_operation(catalog, operation)
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
        // Closed runtime parity for cataloged-but-unimplemented branches
        // (round 2, finding 17): the same required/optional/unknown-member
        // rules and typed-member grammars the published schema enforces,
        // applied before the availability dispatch. A schema-invalid
        // payload is malformed (exit 1), never a deferred unsupported
        // outcome for a shape the contract refuses.
        let (required, optional) =
            unsupported_member_rules(discriminator).ok_or("implemented-without-executor")?;
        // The allowed set is the UNION of required and optional members:
        // required presence is enforced against that union, so a
        // schema-valid optional member (e.g. `symbol:move`'s `toName`)
        // reaches the explicit unsupported outcome instead of being
        // rejected as an unknown member (round 3, finding 15).
        let allowed: Vec<&str> = required.iter().chain(optional.iter()).copied().collect();
        require_members(map, &allowed, &required)?;
        for (key, value) in map {
            if key != "operation" && !allowed.contains(&key.as_str()) {
                return Err("unknown-member");
            }
            validate_unsupported_member(key, value)?;
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
        "ownership:set" => {
            require_members(
                map,
                &["operation", "subject", "mode", "evidence"],
                &["operation", "subject", "mode", "evidence"],
            )?;
            let subject = member_string(map, "subject")?;
            if !super::field_name_valid(&subject) && !super::symbol_id_valid(&subject) {
                return Err("subject-grammar");
            }
            let mode = serde_json::from_value::<crate::managed::Mode>(
                map.get("mode").cloned().ok_or("missing-member")?,
            )
            .map_err(|_| "mode-grammar")?;
            let evidence = map
                .get("evidence")
                .and_then(Json::as_object)
                .ok_or("evidence-not-object")?;
            require_members(
                evidence,
                &["ownershipDigest", "coverage"],
                &["ownershipDigest"],
            )?;
            let ownership_digest = member_string(evidence, "ownershipDigest")?;
            if super::expect_digest(&ownership_digest).is_none() {
                return Err("digest-grammar");
            }
            let mut coverage = Vec::new();
            if let Some(value) = evidence.get("coverage") {
                let rows = value.as_array().ok_or("coverage-not-array")?;
                if rows.len() > 8 {
                    return Err("coverage-over-bound");
                }
                for row in rows {
                    let row = row.as_object().ok_or("coverage-not-object")?;
                    require_members(
                        row,
                        &["target", "path", "digest"],
                        &["target", "path", "digest"],
                    )?;
                    let target = row
                        .get("target")
                        .and_then(Json::as_str)
                        .filter(|target| {
                            target.len() <= 256
                                && crate::managed::token(target)
                                && crate::managed::credential_free(target)
                        })
                        .ok_or("target-grammar")?
                        .to_owned();
                    let path = row
                        .get("path")
                        .and_then(Json::as_str)
                        .filter(|path| {
                            path.len() <= 256
                                && crate::managed::rel_path(path)
                                && crate::managed::credential_free(path)
                        })
                        .ok_or("path-grammar")?
                        .to_owned();
                    let digest = member_string(row, "digest")?;
                    if super::expect_digest(&digest).is_none() {
                        return Err("digest-grammar");
                    }
                    coverage.push(OwnershipCoverage {
                        target,
                        path,
                        digest,
                    });
                }
            }
            coverage.sort_by(|left, right| left.target.as_bytes().cmp(right.target.as_bytes()));
            if coverage
                .windows(2)
                .any(|pair| pair[0].target == pair[1].target)
            {
                return Err("coverage-duplicate");
            }
            Ok(Operation::OwnershipSet {
                subject,
                mode,
                ownership_digest,
                coverage,
            })
        }
        "symbol:move" => {
            require_members(
                map,
                &["operation", "symbol", "toModule", "toName"],
                &["operation", "symbol", "toModule"],
            )?;
            let symbol = member_string(map, "symbol")?;
            let to_module = member_string(map, "toModule")?;
            let to_name = map
                .get("toName")
                .map(|_| member_string(map, "toName"))
                .transpose()?;
            if !super::symbol_id_valid(&symbol)
                || symbol.len() > 192
                || !super::field_name_valid(&to_module)
                || to_name
                    .as_ref()
                    .is_some_and(|name| !super::field_name_valid(name))
            {
                return Err("symbol-move-grammar");
            }
            Ok(Operation::SymbolMove {
                symbol,
                to_module,
                to_name,
            })
        }
        "operation-error:set" => {
            require_members(
                map,
                &["operation", "operationTarget", "definition"],
                &["operation", "operationTarget", "definition"],
            )?;
            let target = member_string(map, "operationTarget")?;
            if crate::error_contract::ErrorId::new(&target).is_none() {
                return Err("operation-grammar");
            }
            let payload = map
                .get("definition")
                .and_then(Json::as_object)
                .ok_or("payload-not-object")?;
            require_members(
                payload,
                &["errors", "registryDigest"],
                &["errors", "registryDigest"],
            )?;
            let registry_digest = member_string(payload, "registryDigest")?;
            if super::expect_digest(&registry_digest).is_none() {
                return Err("digest-grammar");
            }
            let values = payload
                .get("errors")
                .and_then(Json::as_array)
                .ok_or("errors-not-array")?;
            if values.is_empty() || values.len() > 256 {
                return Err("errors-over-bound");
            }
            let mut errors = std::collections::BTreeSet::new();
            for value in values {
                let id = value.as_str().ok_or("type-invalid")?;
                if crate::error_contract::ErrorId::new(id).is_none() {
                    return Err("error-grammar");
                }
                if !errors.insert(id.to_owned()) {
                    return Err("error-duplicate");
                }
            }
            Ok(Operation::OperationErrorSet {
                target,
                errors: errors.into_iter().collect(),
                registry_digest,
            })
        }
        "binding:add" => {
            require_members(
                map,
                &["operation", "symbol", "definition"],
                &["operation", "symbol", "definition"],
            )?;
            let symbol = member_string(map, "symbol")?;
            if !super::symbol_id_valid(&symbol) {
                return Err("symbol-grammar");
            }
            let payload = map
                .get("definition")
                .and_then(Json::as_object)
                .ok_or("payload-not-object")?;
            match payload.get("kind").and_then(Json::as_str) {
                Some("canonical-target-binding") => {
                    require_members(
                        payload,
                        &["kind", "target", "description"],
                        &["kind", "target"],
                    )?;
                    let (module, name) = symbol.split_once('.').ok_or("symbol-grammar")?;
                    let target = member_string(payload, "target")?;
                    if !super::target_name_valid(&target) {
                        return Err("target-grammar");
                    }
                    let mut definition = definition_decl(&serde_json::json!({"name": "binding"}))?;
                    definition.name = name.to_owned();
                    definition.description = payload
                        .get("description")
                        .map(|value| {
                            let text = value.as_str().ok_or("type-invalid")?;
                            if text.is_empty() || text.chars().count() > 2000 {
                                return Err("string-over-bound");
                            }
                            Ok(text.to_owned())
                        })
                        .transpose()?;
                    definition.target = Some(target);
                    Ok(Operation::Lowered {
                        operation: "binding:add",
                        wire: value.clone(),
                        executable: Box::new(Operation::DefinitionAdd {
                            kind: "target-binding".to_owned(),
                            module: module.to_owned(),
                            definition,
                        }),
                    })
                }
                Some("observed-confirmation") => {
                    require_members(
                        payload,
                        &["kind", "proposal", "candidate", "indexDigest"],
                        &["kind", "proposal", "candidate", "indexDigest"],
                    )?;
                    let proposal = member_string(payload, "proposal")?;
                    let digest = proposal.strip_prefix("prop-").ok_or("proposal-grammar")?;
                    if digest.len() != 64
                        || !digest
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        return Err("proposal-grammar");
                    }
                    let candidate = payload
                        .get("candidate")
                        .and_then(Json::as_str)
                        .ok_or("type-invalid")?
                        .to_owned();
                    if candidate.is_empty()
                        || candidate.chars().count() > 512
                        || candidate.chars().any(char::is_control)
                    {
                        return Err("candidate-grammar");
                    }
                    let index_digest = member_string(payload, "indexDigest")?;
                    if super::expect_digest(&index_digest).is_none() {
                        return Err("digest-grammar");
                    }
                    Ok(Operation::BindingConfirm {
                        symbol,
                        proposal,
                        candidate,
                        index_digest,
                    })
                }
                _ => Err("binding-kind"),
            }
        }
        "module:move" => {
            require_members(
                map,
                &["operation", "module", "directory"],
                &["operation", "module", "directory"],
            )?;
            let module = member_string(map, "module")?;
            let directory = member_string(map, "directory")?;
            if !super::field_name_valid(&module) || !super::module_move::directory_valid(&directory)
            {
                return Err("module-move-grammar");
            }
            Ok(Operation::ModuleMove { module, directory })
        }
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
        "enum-case:change" => {
            require_members(
                map,
                &["operation", "enum", "name", "definition"],
                &["operation", "enum", "name", "definition"],
            )?;
            Ok(Operation::EnumCaseChange {
                enum_id: member_string(map, "enum")?,
                name: member_string(map, "name")?,
                change: case_change_decl(map.get("definition").ok_or("missing-member")?)?,
            })
        }
        "definition:add" => {
            require_members(
                map,
                &["operation", "kind", "module", "definition"],
                &["operation", "kind", "module", "definition"],
            )?;
            let kind = member_string(map, "kind")?;
            if !matches!(
                kind.as_str(),
                "command" | "query" | "event" | "policy" | "scenario"
            ) {
                return Err("kind-unknown");
            }
            let module = member_string(map, "module")?;
            if !super::field_name_valid(&module) {
                return Err("module-grammar");
            }
            Ok(Operation::DefinitionAdd {
                kind,
                module,
                definition: definition_decl(map.get("definition").ok_or("missing-member")?)?,
            })
        }
        "command-effect:add" | "command-effect:remove" => {
            require_members(
                map,
                &["operation", "command", "effect", "emit"],
                &["operation", "command"],
            )?;
            let command = member_string(map, "command")?;
            // Either the effect reference or the emission surface is
            // required; both are decoded when present.
            let effect = match map.get("effect") {
                Some(value) => {
                    let effect = value.as_str().ok_or("type-invalid")?;
                    if !super::symbol_id_valid(effect) {
                        return Err("symbol-grammar");
                    }
                    effect.to_owned()
                }
                None => String::new(),
            };
            if effect.is_empty() && !map.contains_key("emit") {
                return Err("missing-member");
            }
            // The explicit emission representation decodes here and is
            // refused at resolution with the registered unsupported member
            // until the reviewed extended-effect successor lands.
            let emit = match map.get("emit") {
                None => None,
                Some(value) => {
                    let id = value.as_str().ok_or("type-invalid")?;
                    if !super::symbol_id_valid(id) {
                        return Err("symbol-grammar");
                    }
                    Some(id.to_owned())
                }
            };
            if discriminator == "command-effect:add" {
                Ok(Operation::CommandEffectAdd {
                    command,
                    effect,
                    emit,
                })
            } else {
                Ok(Operation::CommandEffectRemove {
                    command,
                    effect,
                    emit,
                })
            }
        }
        "operation-input:set" => {
            require_members(
                map,
                &["operation", "operationTarget", "definition"],
                &["operation", "operationTarget", "definition"],
            )?;
            let target = member_string(map, "operationTarget")?;
            let payload = map
                .get("definition")
                .ok_or("missing-member")?
                .as_object()
                .ok_or("payload-not-object")?;
            require_members(payload, &["input"], &["input"])?;
            let fields = payload
                .get("input")
                .ok_or("missing-member")?
                .as_array()
                .ok_or("input-not-array")?;
            if fields.len() > 64 {
                return Err("input-over-bound");
            }
            let input = fields
                .iter()
                .map(field_decl)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Operation::OperationInputSet { target, input })
        }
        "operation-output:set" => {
            require_members(
                map,
                &["operation", "operationTarget", "type"],
                &["operation", "operationTarget", "type"],
            )?;
            Ok(Operation::OperationOutputSet {
                target: member_string(map, "operationTarget")?,
                r#type: type_ref(map.get("type").ok_or("missing-member")?, 1)?,
            })
        }
        "query-effect:add" | "query-effect:remove" => {
            require_members(
                map,
                &["operation", "query", "effect"],
                &["operation", "query", "effect"],
            )?;
            let query = member_string(map, "query")?;
            let effect = member_string(map, "effect")?;
            if !super::symbol_id_valid(&query) || !super::symbol_id_valid(&effect) {
                return Err("symbol-grammar");
            }
            Ok(Operation::QueryEffect {
                query,
                effect,
                add: discriminator == "query-effect:add",
            })
        }
        "requirement:link" | "requirement:unlink" => {
            require_members(
                map,
                &["operation", "symbol", "requirement"],
                &["operation", "symbol", "requirement"],
            )?;
            let symbol = member_string(map, "symbol")?;
            let requirement = member_string(map, "requirement")?;
            if !super::symbol_id_valid(&symbol) || !valid_requirement(&requirement) {
                return Err("requirement-grammar");
            }
            Ok(Operation::Requirement {
                symbol,
                requirement,
                link: discriminator == "requirement:link",
            })
        }
        "migration:apply" => {
            require_members(
                map,
                &["operation", "step", "parameters"],
                &["operation", "step"],
            )?;
            let step = member_string(map, "step")?;
            validate_unsupported_member("step", map.get("step").expect("required step"))?;
            let parameters = map
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            validate_unsupported_member("parameters", &parameters)?;
            let mut nested = parameters.as_object().ok_or("payload-not-object")?.clone();
            if nested.contains_key("operation") {
                return Err("unknown-member");
            }
            let executor = match step.as_str() {
                "model.entity-field-rename.v0.8.0" => "entity-field:rename",
                _ => {
                    return Ok(Operation::Unsupported {
                        operation: "migration:apply".to_owned(),
                        subject: Some(step),
                    })
                }
            };
            nested.insert("operation".to_owned(), Json::String(executor.to_owned()));
            let executable = decode_operation(catalog, &Json::Object(nested))?;
            Ok(Operation::Lowered {
                operation: "migration:apply",
                wire: value.clone(),
                executable: Box::new(executable),
            })
        }
        _ => Err("implemented-without-executor"),
    }
}

/// The frozen Model requirement identity grammar.
fn valid_requirement(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
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

/// The closed required/optional member rules of the cataloged-but-
/// unsupported operations, mirroring the published request-schema
/// variants exactly (round 2, finding 17).
fn unsupported_member_rules(discriminator: &str) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
    let (required, optional): (&[&str], &[&str]) = match discriminator {
        "operation-error:set" => (&["operation", "operationTarget", "definition"], &[]),
        "query-effect:add" | "query-effect:remove" => (&["operation", "query", "effect"], &[]),
        "binding:add" => (&["operation", "symbol", "definition"], &[]),
        "ownership:set" => (&["operation", "subject", "mode", "evidence"], &[]),
        "requirement:link" | "requirement:unlink" => (&["operation", "symbol", "requirement"], &[]),
        "symbol:move" => (&["operation", "symbol", "toModule"], &["toName"]),
        "module:move" => (&["operation", "module", "directory"], &[]),
        "migration:apply" => (&["operation", "step"], &["parameters"]),
        _ => return None,
    };
    Some((required.to_vec(), optional.to_vec()))
}

/// Enforce the typed-member grammar of one unsupported-branch member
/// exactly as the published schema does (round 2, finding 17).
fn validate_unsupported_member(key: &str, value: &Json) -> Result<(), &'static str> {
    let symbol_member = matches!(
        key,
        "entity"
            | "enum"
            | "command"
            | "query"
            | "symbol"
            | "subject"
            | "operationTarget"
            | "effect"
            | "emit"
    );
    if symbol_member {
        let text = value.as_str().ok_or("type-invalid")?;
        if text.len() > 192 {
            return Err("string-over-bound");
        }
        if !super::symbol_id_valid(text) {
            return Err("symbol-grammar");
        }
        return Ok(());
    }
    // The single-segment identities (module ids, member names) share the
    // closed member grammar the schema pins for them.
    if matches!(key, "module" | "toModule" | "toName" | "requirement") {
        let text = value.as_str().ok_or("type-invalid")?;
        if !super::field_name_valid(text) {
            return Err("member-grammar");
        }
        return Ok(());
    }
    match key {
        "kind" => {
            let text = value.as_str().ok_or("type-invalid")?;
            if matches!(text, "command" | "query" | "event" | "policy" | "scenario") {
                Ok(())
            } else {
                Err("kind-unknown")
            }
        }
        "mode" => {
            let text = value.as_str().ok_or("type-invalid")?;
            if matches!(text, "observed" | "contracted" | "managed") {
                Ok(())
            } else {
                Err("mode-unknown")
            }
        }
        "directory" => {
            let text = value.as_str().ok_or("type-invalid")?;
            // The schema pins the moduleId grammar for the safe leaf.
            if text.len() <= 63
                && text
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            {
                Ok(())
            } else {
                Err("directory-grammar")
            }
        }
        "step" => {
            let text = value.as_str().ok_or("type-invalid")?;
            if !text.is_empty()
                && text.len() <= 128
                && text
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                && text.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'.'
                        || byte == b'-'
                })
            {
                Ok(())
            } else {
                Err("step-grammar")
            }
        }
        "definition" | "evidence" | "parameters" | "type" => {
            // Declaration payloads are closed objects (the schema's
            // closedPayload); `type` is a closed type reference.
            if key == "type" {
                type_ref(value, 1).map(|_| ())
            } else {
                if value.as_object().is_none() {
                    return Err("payload-not-object");
                }
                literal_of(value, 1)
                    .map(|_| ())
                    .map_err(|_| "payload-invalid")
            }
        }
        _ => {
            // The discriminator and any remaining member are bounded
            // literals.
            if key == "operation" {
                value.as_str().map(|_| ()).ok_or("discriminator-missing")
            } else {
                literal_of(value, 1)
                    .map(|_| ())
                    .map_err(|_| "payload-invalid")
            }
        }
    }
}

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

/// Decode one closed optional-text member. The schema types these
/// members as strings, so explicit null is refused (round 2, finding 17).
fn member_text(
    map: &serde_json::Map<String, Json>,
    key: &str,
) -> Result<Option<String>, &'static str> {
    match map.get(key) {
        None => Ok(None),
        Some(Json::Null) => Err("type-invalid"),
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
        None => false,
        // The schema pins `required` to the constant true; null (or false)
        // is not absence (round 2, finding 17).
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

/// Decode one closed enum-case change declaration: at least one of
/// rename/description/value; wire values require the Model successor.
fn case_change_decl(value: &Json) -> Result<CaseChangeDecl, &'static str> {
    let map = value.as_object().ok_or("type-invalid")?;
    require_members(map, &["rename", "description", "value"], &[])?;
    if map.is_empty() {
        return Err("empty-change");
    }
    let rename = match map.get("rename") {
        None => None,
        Some(_rename) => {
            let name = member_string(map, "rename")?;
            if !super::field_name_valid(&name) {
                return Err("name-grammar");
            }
            Some(name)
        }
    };
    Ok(CaseChangeDecl {
        rename,
        description: member_text(map, "description")?,
        value: map
            .get("value")
            .map(|v| literal_of(v, 1).map_err(|_| "literal-invalid"))
            .transpose()?,
    })
}

/// Decode one closed definition declaration; the members the declared
/// kind does not use are refused by resolution.
fn definition_decl(value: &Json) -> Result<DefinitionDecl, &'static str> {
    let map = value.as_object().ok_or("type-invalid")?;
    require_members(
        map,
        &[
            "name",
            "description",
            "input",
            "effects",
            "reads",
            "returns",
            "payload",
            "appliesTo",
            "decision",
            "summary",
            "covers",
        ],
        &["name"],
    )?;
    let name = member_string(map, "name")?;
    if !super::field_name_valid(&name) {
        return Err("name-grammar");
    }
    let field_list = |key: &str| -> Result<Vec<FieldDecl>, &'static str> {
        match map.get(key) {
            None => Ok(Vec::new()),
            Some(value) => {
                let items = value.as_array().ok_or("list-not-array")?;
                if items.len() > 64 {
                    return Err("list-over-bound");
                }
                items.iter().map(field_decl).collect()
            }
        }
    };
    let id_list = |key: &str| -> Result<Vec<String>, &'static str> {
        match map.get(key) {
            None => Ok(Vec::new()),
            Some(value) => {
                let items = value.as_array().ok_or("list-not-array")?;
                if items.len() > 64 {
                    return Err("list-over-bound");
                }
                items
                    .iter()
                    .map(|item| {
                        let id = item.as_str().ok_or("type-invalid")?;
                        if !super::symbol_id_valid(id) {
                            return Err("symbol-grammar");
                        }
                        Ok(id.to_owned())
                    })
                    .collect()
            }
        }
    };
    Ok(DefinitionDecl {
        target: None,
        name,
        description: member_text(map, "description")?,
        input: field_list("input")?,
        effects: id_list("effects")?,
        reads: id_list("reads")?,
        returns: match map.get("returns") {
            None => None,
            Some(value) => Some(type_ref(value, 1)?),
        },
        payload: field_list("payload")?,
        applies_to: id_list("appliesTo")?,
        decision: match map.get("decision") {
            None => None,
            Some(value) => {
                let decision = value.as_str().ok_or("type-invalid")?;
                if !matches!(decision, "allow" | "deny") {
                    return Err("decision-unknown");
                }
                Some(decision.to_owned())
            }
        },
        summary: member_text(map, "summary")?,
        covers: id_list("covers")?,
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
pub(crate) fn unique_json(bytes: &[u8]) -> Result<Json, ()> {
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
    fn ownership_operation_decodes_and_carries_its_exact_subject() {
        let text = request(
            "{\"operation\":\"ownership:set\",\"subject\":\"tasks.task\",\"mode\":\"observed\",\"evidence\":{\"ownershipDigest\":\"sha256:0000000000000000000000000000000000000000000000000000000000000000\"}}",
        );
        let req = decode_ok(&text);
        match &req.operations[0] {
            Operation::OwnershipSet {
                subject,
                mode,
                coverage,
                ..
            } => {
                assert_eq!(subject, "tasks.task");
                assert_eq!(*mode, crate::managed::Mode::Observed);
                assert!(coverage.is_empty());
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

    #[test]
    fn null_expect_model_is_not_absence() {
        let text = "{\"schemaVersion\":\"lekalo/change-request/v0.8.0\",\"expectModel\":null,\"operations\":[{\"operation\":\"entity-field:add\",\"entity\":\"tasks.task\",\"field\":{\"name\":\"priority\",\"type\":{\"ref\":\"tasks.text\"}}}]}";
        assert_eq!(
            decode_err(text),
            RequestInvalid::Shape("expect-model-not-string")
        );
    }

    #[test]
    fn null_description_and_null_required_are_type_invalid() {
        let text = request(
            "{\"operation\":\"entity-field:add\",\"entity\":\"tasks.task\",\"field\":{\"name\":\"priority\",\"type\":{\"ref\":\"tasks.text\"},\"description\":null}}",
        );
        assert!(matches!(
            decode_err(&text),
            RequestInvalid::Operation {
                detail: "type-invalid",
                ..
            }
        ));
        let text = request(
            "{\"operation\":\"entity-field:add\",\"entity\":\"tasks.task\",\"field\":{\"name\":\"priority\",\"type\":{\"ref\":\"tasks.text\"},\"required\":null}}",
        );
        assert!(matches!(
            decode_err(&text),
            RequestInvalid::Operation {
                detail: "type-invalid",
                ..
            }
        ));
    }

    #[test]
    fn schema_valid_optional_members_reach_the_registered_executor() {
        // Round 3, finding 15: an optional member the published schema
        // admits (`symbol:move`'s `toName`, `migration:apply`'s
        // `parameters`) decodes before dispatch. Symbol moves now have a
        // sealed executor; an unknown migration still refuses explicitly.
        let symbol_move = request(
            "{\"operation\":\"symbol:move\",\"symbol\":\"tasks.task\",\"toModule\":\"other\",\"toName\":\"new_task\"}",
        );
        match &decode_ok(&symbol_move).operations[0] {
            Operation::SymbolMove {
                symbol,
                to_module,
                to_name,
            } => {
                assert_eq!(symbol, "tasks.task");
                assert_eq!(to_module, "other");
                assert_eq!(to_name.as_deref(), Some("new_task"));
            }
            other => panic!("unexpected operation: {other:?}"),
        }
        let migration = request(
            "{\"operation\":\"migration:apply\",\"step\":\"adopt.entities\",\"parameters\":{\"mode\":\"dry\"}}",
        );
        match &decode_ok(&migration).operations[0] {
            Operation::Unsupported { operation, .. } => {
                assert_eq!(operation, "migration:apply");
            }
            other => panic!("unexpected operation: {other:?}"),
        }
    }

    #[test]
    fn unsupported_branches_enforce_the_closed_member_rules() {
        // A schema-invalid payload is malformed (exit 1), never a deferred
        // unsupported outcome (round 2, finding 17).
        let cases = [
            // binding:add with an arbitrary member and a missing payload.
            request("{\"operation\":\"binding:add\",\"symbol\":\"tasks.task\",\"force\":true}"),
            // definition:add missing its required payload.
            request("{\"operation\":\"definition:add\",\"kind\":\"command\",\"module\":\"tasks\"}"),
            // definition:add with an unknown kind.
            request("{\"operation\":\"definition:add\",\"kind\":\"mixin\",\"module\":\"tasks\",\"definition\":{}}"),
            // ownership:set with an unknown mode.
            request("{\"operation\":\"ownership:set\",\"subject\":\"tasks.task\",\"mode\":\"theirs\",\"evidence\":{}}"),
            // symbol:move with a multi-segment toModule.
            request("{\"operation\":\"symbol:move\",\"symbol\":\"tasks.task\",\"toModule\":\"tasks.sub\",\"definition\":{}}"),
        ];
        for case in cases {
            assert!(matches!(
                decode_err(&case),
                RequestInvalid::Operation { .. } | RequestInvalid::Shape("unknown-member")
            ));
        }
        // A schema-valid unsupported payload still decodes and is refused
        // later with the registered unsupported outcome.
        let ok = request(
            "{\"operation\":\"ownership:set\",\"subject\":\"tasks.task\",\"mode\":\"observed\",\"evidence\":{\"ownershipDigest\":\"sha256:0000000000000000000000000000000000000000000000000000000000000000\"}}",
        );
        let decoded = decode_ok(&ok);
        match &decoded.operations[0] {
            Operation::OwnershipSet { subject, .. } => {
                assert_eq!(subject, "tasks.task");
            }
            other => panic!("unexpected operation: {other:?}"),
        }
    }
}
