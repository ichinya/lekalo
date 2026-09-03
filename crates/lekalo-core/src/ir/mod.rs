//! Issue #8: the typed, deterministic, target-neutral Lekalo IR.
//!
//! [`compile`] turns the accepted #7 loader aggregate
//! ([`NormalizedModel`]) into an immutable typed read model plus a separate,
//! occurrence-safe source map. The IR is the only surface downstream issues
//! consume: adapters never re-read source YAML/JSON, graphs never re-parse
//! documents, and every short reference has already been expanded to its
//! fully qualified form by the loader.
//!
//! Closed surface, no silent widening:
//!
//! - [`Definition`] is a closed enum over the 12 symbol kinds of the
//!   accepted Model contracts; `project` and `module` definitions are
//!   represented by [`Project`] and [`Module`]. Every variant is exhaustive
//!   and matchable without a wildcard arm.
//! - [`TypeRef`] mirrors the accepted Model type grammar exactly
//!   (`ref`, `list`, `optional`/nullable; depth at most four). `map` and
//!   `result` wrappers do not exist in any accepted Model version; adding
//!   them here would let IR represent models no source can declare. They
//!   require a reviewed Model successor and are deliberately absent.
//! - [`EffectOperation`] is the closed `create`/`update`/`delete` enum of
//!   the effect definition kind.
//! - Unknown fields fail closed with `ir.unknown-field`; there is no
//!   extension side channel. Extension metadata can only arrive through a
//!   reviewed Model successor whose extension keys the decoder would then
//!   accept explicitly, so foreign keys can never corrupt core enums.
//!
//! Ownership boundaries: ID-grammar reserved words, reference target
//! existence, reference kind checks, and entity identity membership stay
//! with the Model validator and later semantic validation (they are not
//! re-checked here); no migration, cache, graph, or adapter behavior lives
//! in this module. The IR never reads the filesystem and never writes.
//!
//! Determinism: [`CompiledProject::to_canonical_json`] emits compact UTF-8
//! JSON with byte-sorted object keys, semantic-ID-ordered `definitions`,
//! `modules`, and `imports`, and source-order preservation for every
//! semantically ordered array. Repeated runs are byte-identical.

pub mod canonical;
pub mod decode;
pub mod diagnostic;
pub mod grammar;
pub mod version;

use crate::loader::{ModelVersion, NormalizedModel, SourceMapEntry};
use diagnostic::IrFailure;

pub use diagnostic::{
    DUPLICATE_MEMBER, KIND_PLACEMENT, KIND_UNKNOWN, MISSING_FIELD, UNKNOWN_FIELD, VALUE_INVALID,
};
pub use version::{FAMILY, IDENTITY, VERSION};

/// The maximum `list`/`optional` nesting of a [`TypeRef`], leaf included —
/// exactly the accepted Model type-expression depth.
pub const MAX_TYPE_LEVELS: usize = 4;

/// One compiled project: the immutable typed read model.
///
/// Spans are deliberately absent; the sibling [`Compilation::source_map`]
/// carries every source location. Two compilations of equivalent JSON and
/// YAML inputs compare equal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledProject {
    /// The exact source Model version this IR was built from.
    pub model_version: ModelVersion,
    /// The project definition, when the source declared one.
    pub project: Option<Project>,
    /// Modules in semantic-ID byte order.
    pub modules: Vec<Module>,
    /// Symbol definitions in semantic-ID byte order.
    pub definitions: Vec<Definition>,
}

/// One successful compilation: the typed IR plus its separate source map.
#[derive(Clone, Debug)]
pub struct Compilation {
    pub project: CompiledProject,
    pub source_map: SourceMap,
}

/// Compile the loader aggregate into typed IR.
///
/// Any decode violation fails the whole compilation with finalized `ir.*`
/// diagnostics (`invalid`, exit 1); a model that fails here never reaches an
/// adapter as a half-typed value.
pub fn compile(model: &NormalizedModel) -> Result<Compilation, IrFailure> {
    decode::compile(model)
}

/// The bounded human-readable text surface: `description`, `summary`, and
/// rename `note` values (1-2000 Unicode scalars).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Text(String);

impl Text {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A fully qualified symbol reference the loader resolved.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct SymbolId(String);

impl SymbolId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A module semantic ID.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ModuleId(String);

impl ModuleId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A project semantic ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A field name.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct FieldName(String);

impl FieldName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An enum member value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumValueName(String);

impl EnumValueName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A target name (never interpreted by the IR).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetName(String);

impl TargetName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A requirement reference.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct RequirementId(String);

impl RequirementId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A concrete HTTP endpoint path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointPath(String);

impl EndpointPath {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Fields shared by every definition kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Common {
    pub version: u64,
    pub description: Option<Text>,
    pub derived_from: Vec<RequirementId>,
    pub visibility: Option<Visibility>,
    pub portability: Option<Portability>,
    /// Model 1.0.0 only; always empty under Model 0.1.0.
    pub renamed_from: Vec<SymbolId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    Module,
    Project,
}

impl Visibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Project => "project",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Portability {
    Portable,
    TargetSpecific,
}

impl Portability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Portable => "portable",
            Self::TargetSpecific => "target-specific",
        }
    }
}

/// The closed rename-history registry entry (Model 1.0.0 project document).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenameHistoryEntry {
    pub from: SymbolId,
    pub to: SymbolId,
    pub definition_version: u64,
    /// Present only as the exact literal `true`.
    pub same_identity: bool,
    pub note: Option<Text>,
}

/// The closed tombstone vocabulary (Model 1.0.0 project document).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Tombstone {
    Replaced {
        id: SymbolId,
        replaced_by: SymbolId,
        since: u64,
    },
    Deleted {
        id: SymbolId,
        since: u64,
    },
}

/// The closed project ID registry (Model 1.0.0).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdRegistry {
    pub rename_history: Vec<RenameHistoryEntry>,
    pub tombstones: Vec<Tombstone>,
}

/// The `project` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    pub id: ProjectId,
    pub common: Common,
    pub id_registry: Option<IdRegistry>,
}

/// The `module` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    pub id: ModuleId,
    pub common: Common,
    /// Declared imports, set-like: canonical output sorts by module ID.
    pub imports: Vec<ModuleId>,
}

/// One entity/value-object field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub name: FieldName,
    pub r#type: TypeRef,
    /// Present only as the exact literal `true` in source.
    pub required: bool,
    pub description: Option<Text>,
}

/// The closed type reference: a symbol reference, a list, or a nullable
/// wrapper, at most [`MAX_TYPE_LEVELS`] deep including the leaf.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeRef {
    Ref(SymbolId),
    List(Box<TypeRef>),
    Optional(Box<TypeRef>),
}

/// One enum member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumValue {
    pub value: EnumValueName,
    pub description: Option<Text>,
}

/// The closed scalar base vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScalarBase {
    String,
    Number,
    Boolean,
    Date,
    Datetime,
    Uuid,
    Uri,
}

impl ScalarBase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Date => "date",
            Self::Datetime => "datetime",
            Self::Uuid => "uuid",
            Self::Uri => "uri",
        }
    }
}

/// The closed HTTP method vocabulary of the endpoint kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// The closed policy decision vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    Allow,
    Deny,
}

impl Decision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

/// The closed effect operation enum with exhaustive matching.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectOperation {
    Create,
    Update,
    Delete,
}

impl EffectOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

/// The `scalar` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScalarDef {
    pub id: SymbolId,
    pub common: Common,
    pub base: ScalarBase,
}

/// The `enum` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumDef {
    pub id: SymbolId,
    pub common: Common,
    pub values: Vec<EnumValue>,
}

/// The `value-object` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueObjectDef {
    pub id: SymbolId,
    pub common: Common,
    pub fields: Vec<Field>,
}

/// The `entity` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityDef {
    pub id: SymbolId,
    pub common: Common,
    pub fields: Vec<Field>,
    pub identity: Vec<FieldName>,
}

/// The `command` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandDef {
    pub id: SymbolId,
    pub common: Common,
    pub input: Vec<Field>,
    pub effects: Vec<SymbolId>,
}

/// The `query` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryDef {
    pub id: SymbolId,
    pub common: Common,
    pub reads: Vec<SymbolId>,
    pub returns: Option<TypeRef>,
}

/// The `policy` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyDef {
    pub id: SymbolId,
    pub common: Common,
    pub applies_to: Vec<SymbolId>,
    pub decision: Decision,
}

/// The `event` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventDef {
    pub id: SymbolId,
    pub common: Common,
    pub payload: Vec<Field>,
}

/// The `effect` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectDef {
    pub id: SymbolId,
    pub common: Common,
    pub operation: EffectOperation,
    pub entity: SymbolId,
    pub emits: Vec<SymbolId>,
}

/// The `endpoint` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointDef {
    pub id: SymbolId,
    pub common: Common,
    pub invokes: SymbolId,
    pub method: HttpMethod,
    pub path: EndpointPath,
}

/// The `scenario` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioDef {
    pub id: SymbolId,
    pub common: Common,
    pub summary: Text,
    pub covers: Vec<SymbolId>,
}

/// The `target-binding` definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetBindingDef {
    pub id: SymbolId,
    pub common: Common,
    pub target: TargetName,
}

/// The closed definition-kind tag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefinitionKind {
    Scalar,
    Enum,
    ValueObject,
    Entity,
    Command,
    Query,
    Policy,
    Event,
    Effect,
    Endpoint,
    Scenario,
    TargetBinding,
}

impl DefinitionKind {
    /// The exact source `kind` literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Enum => "enum",
            Self::ValueObject => "value-object",
            Self::Entity => "entity",
            Self::Command => "command",
            Self::Query => "query",
            Self::Policy => "policy",
            Self::Event => "event",
            Self::Effect => "effect",
            Self::Endpoint => "endpoint",
            Self::Scenario => "scenario",
            Self::TargetBinding => "target-binding",
        }
    }
}

/// The closed symbol-definition enum: exhaustive, no target-specific nodes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Definition {
    Scalar(ScalarDef),
    Enum(EnumDef),
    ValueObject(ValueObjectDef),
    Entity(EntityDef),
    Command(CommandDef),
    Query(QueryDef),
    Policy(PolicyDef),
    Event(EventDef),
    Effect(EffectDef),
    Endpoint(EndpointDef),
    Scenario(ScenarioDef),
    TargetBinding(TargetBindingDef),
}

impl Definition {
    /// The fully qualified semantic ID of this definition.
    pub fn id(&self) -> &SymbolId {
        match self {
            Self::Scalar(definition) => &definition.id,
            Self::Enum(definition) => &definition.id,
            Self::ValueObject(definition) => &definition.id,
            Self::Entity(definition) => &definition.id,
            Self::Command(definition) => &definition.id,
            Self::Query(definition) => &definition.id,
            Self::Policy(definition) => &definition.id,
            Self::Event(definition) => &definition.id,
            Self::Effect(definition) => &definition.id,
            Self::Endpoint(definition) => &definition.id,
            Self::Scenario(definition) => &definition.id,
            Self::TargetBinding(definition) => &definition.id,
        }
    }

    /// The closed kind tag of this definition.
    pub const fn kind(&self) -> DefinitionKind {
        match self {
            Self::Scalar(_) => DefinitionKind::Scalar,
            Self::Enum(_) => DefinitionKind::Enum,
            Self::ValueObject(_) => DefinitionKind::ValueObject,
            Self::Entity(_) => DefinitionKind::Entity,
            Self::Command(_) => DefinitionKind::Command,
            Self::Query(_) => DefinitionKind::Query,
            Self::Policy(_) => DefinitionKind::Policy,
            Self::Event(_) => DefinitionKind::Event,
            Self::Effect(_) => DefinitionKind::Effect,
            Self::Endpoint(_) => DefinitionKind::Endpoint,
            Self::Scenario(_) => DefinitionKind::Scenario,
            Self::TargetBinding(_) => DefinitionKind::TargetBinding,
        }
    }

    /// The shared common fields of this definition.
    pub fn common(&self) -> &Common {
        match self {
            Self::Scalar(definition) => &definition.common,
            Self::Enum(definition) => &definition.common,
            Self::ValueObject(definition) => &definition.common,
            Self::Entity(definition) => &definition.common,
            Self::Command(definition) => &definition.common,
            Self::Query(definition) => &definition.common,
            Self::Policy(definition) => &definition.common,
            Self::Event(definition) => &definition.common,
            Self::Effect(definition) => &definition.common,
            Self::Endpoint(definition) => &definition.common,
            Self::Scenario(definition) => &definition.common,
            Self::TargetBinding(definition) => &definition.common,
        }
    }
}

/// The occurrence-safe source map of one compilation: a sorted entry per
/// definition and per decoded field, reference, type, and registry
/// position. Separate from the semantic payload; never part of equality or
/// canonical IR bytes.
#[derive(Clone, Debug)]
pub struct SourceMap {
    entries: Vec<SourceMapEntry>,
}

impl SourceMap {
    /// Assemble from decoded `(pointer, path, semantic id, span)` records,
    /// sorted by `(path, pointer, startByte)`.
    pub(crate) fn new(mut entries: Vec<SourceMapEntry>) -> Self {
        entries.sort_by(|left, right| {
            (left.path.clone(), left.pointer.clone(), left.start.byte).cmp(&(
                right.path.clone(),
                right.pointer.clone(),
                right.start.byte,
            ))
        });
        Self { entries }
    }

    /// The sorted entries.
    pub fn entries(&self) -> &[SourceMapEntry] {
        &self.entries
    }

    /// The compact JSON bytes of the sorted entry array.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.entries).expect("source map serializes")
    }
}
