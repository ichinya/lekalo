//! Normalization of the adapter-produced storage-introspection
//! evidence (issue #69).
//!
//! [`IntrospectionEvidence::from_value`] is the single entry from
//! parsed JSON to the typed evidence document. It fails closed: the
//! mode is const-checked, the posture is read-only, the connection is
//! one opaque name token, the engine must be postgres, and the
//! `projectionRef` digest binds the exact attachment the evidence was
//! checked against. Core normalizes and compares; it never connects.

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::SemanticId;
use serde_json::Value as Json;

use super::diagnostic;
use super::id::{ConnectionName, Engine, ScopeName, VersionPin};
use super::version;

/// One observed column of the checked catalog read.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ObservedColumn {
    pub(crate) name: String,
    pub(crate) storage_type: String,
    pub(crate) nullable: bool,
    pub(crate) default: Option<String>,
    pub(crate) identity: bool,
}

impl ObservedColumn {
    /// The column name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The observed type token (closed or carried verbatim from the
    /// unsupported records).
    pub fn storage_type(&self) -> &str {
        &self.storage_type
    }

    /// Whether the column accepts null.
    pub const fn nullable(&self) -> bool {
        self.nullable
    }

    /// The observed default spelling, when any.
    pub fn default(&self) -> Option<&str> {
        self.default.as_deref()
    }

    /// Whether the column is generated always as identity.
    pub const fn identity(&self) -> bool {
        self.identity
    }
}

/// One observed index.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ObservedIndex {
    pub(crate) name: Option<String>,
    pub(crate) columns: Vec<String>,
    pub(crate) unique: bool,
    pub(crate) partial: bool,
}

impl ObservedIndex {
    /// The observed index name, when the engine reported one.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The indexed columns, in observed order.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// Whether the index enforces uniqueness.
    pub const fn unique(&self) -> bool {
        self.unique
    }

    /// Whether the index carries a predicate.
    pub const fn partial(&self) -> bool {
        self.partial
    }
}

/// One observed foreign constraint.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ObservedConstraint {
    pub(crate) kind: ConstraintKind,
    pub(crate) name: Option<String>,
    pub(crate) columns: Vec<String>,
    pub(crate) target: Option<String>,
}

impl ObservedConstraint {
    /// The closed constraint kind.
    pub const fn kind(&self) -> ConstraintKind {
        self.kind
    }

    /// The constraint name, when reported.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The constrained columns.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// The referenced table of a foreign constraint.
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }
}

/// The closed observed constraint kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ConstraintKind {
    /// The table's primary key.
    Primary,
    /// A unique constraint.
    Unique,
    /// A foreign key.
    Foreign,
    /// A check constraint.
    Check,
}

impl ConstraintKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Unique => "unique",
            Self::Foreign => "foreign",
            Self::Check => "check",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, super::id::ShapeError> {
        match text {
            "primary" => Ok(Self::Primary),
            "unique" => Ok(Self::Unique),
            "foreign" => Ok(Self::Foreign),
            "check" => Ok(Self::Check),
            _ => Err(super::id::ShapeError::Shape),
        }
    }
}

/// One observed table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedTable {
    pub(crate) name: String,
    pub(crate) scope: String,
    pub(crate) columns: Vec<ObservedColumn>,
    pub(crate) indexes: Vec<ObservedIndex>,
    pub(crate) constraints: Vec<ObservedConstraint>,
}

impl ObservedTable {
    /// The table name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The observed columns, canonical (name) order.
    pub fn columns(&self) -> &[ObservedColumn] {
        &self.columns
    }

    /// The observed indexes, canonical order.
    pub fn indexes(&self) -> &[ObservedIndex] {
        &self.indexes
    }

    /// The observed constraints, canonical order.
    pub fn constraints(&self) -> &[ObservedConstraint] {
        &self.constraints
    }
}

/// One unsupported observed fact, reported explicitly.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct UnsupportedRecord {
    pub(crate) kind: UnsupportedKind,
    pub(crate) name: String,
    pub(crate) table: Option<String>,
    pub(crate) column: Option<String>,
}

impl UnsupportedRecord {
    /// The closed record kind.
    pub const fn kind(&self) -> UnsupportedKind {
        self.kind
    }

    /// The unsupported name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The owning table, when the record names one.
    pub fn table(&self) -> Option<&str> {
        self.table.as_deref()
    }

    /// The owning column, when the record names one.
    pub fn column(&self) -> Option<&str> {
        self.column.as_deref()
    }
}

/// The closed unsupported-record kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum UnsupportedKind {
    /// An unsupported column type.
    Type,
    /// An unsupported extension.
    Extension,
}

impl UnsupportedKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Extension => "extension",
        }
    }
}

/// One finished introspection evidence document: immutable,
/// deterministically ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntrospectionEvidence {
    evidence_revision: SemVer,
    project_id: SemanticId,
    engine_version: VersionPin,
    scopes: Vec<ScopeName>,
    connection: ConnectionName,
    projection_ref: Sha256Digest,
    tables: Vec<ObservedTable>,
    extensions: Vec<String>,
    unsupported: Vec<UnsupportedRecord>,
}

impl IntrospectionEvidence {
    /// Normalize one wire document into typed evidence, or return the
    /// typed rejection set.
    pub fn from_value(json: &Json) -> Result<Self, DiagnosticSet> {
        from_value(json)
    }

    /// The canonical payload bytes (compact JSON, byte-sorted keys,
    /// no trailing LF), or the typed over-bound refusal.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        canonical_bytes(self)
    }

    /// The exact evidence revision.
    pub fn evidence_revision(&self) -> &SemVer {
        &self.evidence_revision
    }

    /// The stable project identity.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The server-reported engine version.
    pub const fn engine_version(&self) -> &VersionPin {
        &self.engine_version
    }

    /// The observed scopes.
    pub fn scopes(&self) -> &[ScopeName] {
        &self.scopes
    }

    /// The opaque connection name.
    pub fn connection(&self) -> &ConnectionName {
        &self.connection
    }

    /// The bound projection digest.
    pub fn projection_ref(&self) -> &Sha256Digest {
        &self.projection_ref
    }

    /// The observed tables, canonical (name) order.
    pub fn tables(&self) -> &[ObservedTable] {
        &self.tables
    }

    /// The installed extensions, canonical order.
    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    /// The unsupported records, canonical order.
    pub fn unsupported(&self) -> &[UnsupportedRecord] {
        &self.unsupported
    }

    /// Resolve one observed table by name.
    pub fn table(&self, name: &str) -> Option<&ObservedTable> {
        self.tables.iter().find(|table| table.name == name)
    }
}

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "evidenceRevision",
    "projectId",
    "engine",
    "engineVersion",
    "mode",
    "readOnly",
    "scopes",
    "connection",
    "projectionRef",
    "tables",
    "extensions",
    "unsupported",
];

/// The required top-level members.
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "evidenceRevision",
    "projectId",
    "engine",
    "engineVersion",
    "mode",
    "readOnly",
    "scopes",
    "connection",
    "projectionRef",
    "tables",
];

/// Normalize one wire document into typed evidence, or return the
/// typed rejection set with no partial document. Pure and read-only.
fn from_value(json: &Json) -> Result<IntrospectionEvidence, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("top-level-shape"))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    for required in REQUIRED_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::input_invalid("missing-field"));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str)
        != Some("lekalo/storage-introspection/v0.4.0")
    {
        return Err(diagnostic::input_invalid("schema-version"));
    }
    if object.get("identity").and_then(Json::as_str)
        != Some("dev.lekalo.storage-introspection@0.4.0")
    {
        return Err(diagnostic::input_invalid("contract-identity"));
    }
    if object.get("mode").and_then(Json::as_str) != Some("checked") {
        return Err(diagnostic::rule_invalid(
            diagnostic::INTROSPECTION_INVALID,
            "mode-not-checked",
            None,
        ));
    }
    if object.get("readOnly") != Some(&Json::Bool(true)) {
        return Err(diagnostic::rule_invalid(
            diagnostic::INTROSPECTION_INVALID,
            "not-read-only",
            None,
        ));
    }
    if object.get("engine").and_then(Json::as_str) != Some("postgres") {
        return Err(diagnostic::rule_invalid(
            diagnostic::INTROSPECTION_INVALID,
            "engine-not-postgres",
            None,
        ));
    }
    let evidence_revision = SemVer::parse(
        object
            .get("evidenceRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("evidence-revision"))?,
    )
    .map_err(|_| diagnostic::input_invalid("evidence-revision"))?;
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("project-id"))?,
    )
    .map_err(|_| diagnostic::input_invalid("project-id"))?;
    let engine_version = VersionPin::parse(
        object
            .get("engineVersion")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("engine-version"))?,
    )
    .map_err(|_| diagnostic::input_invalid("engine-version"))?;
    let scopes = scopes(
        object
            .get("scopes")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("scopes-shape"))?,
    )?;
    let connection = ConnectionName::parse(
        object
            .get("connection")
            .and_then(Json::as_object)
            .ok_or_else(|| diagnostic::input_invalid("connection-shape"))?
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("connection-shape"))?,
    )
    .map_err(|_| diagnostic::input_invalid("connection-name"))?;
    let projection_ref = Sha256Digest::parse(
        object
            .get("projectionRef")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("projection-ref"))?,
    )
    .map_err(|_| diagnostic::input_invalid("projection-ref"))?;
    let tables = tables(
        object
            .get("tables")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("table-list"))?,
    )?;
    for table in &tables {
        let scope_ok = scopes.iter().any(|scope| scope.as_str() == table.scope);
        if !scope_ok {
            return Err(diagnostic::rule_invalid(
                diagnostic::INTROSPECTION_INVALID,
                "table-outside-scope",
                Some(&table.name),
            ));
        }
    }
    let extensions = match object.get("extensions") {
        Some(Json::Null) | None => Vec::new(),
        Some(value) => extension_names(
            value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("extensions-shape"))?,
        )?,
    };
    let unsupported = match object.get("unsupported") {
        Some(Json::Null) | None => Vec::new(),
        Some(value) => unsupported_records(
            value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("unsupported-shape"))?,
        )?,
    };
    let evidence = IntrospectionEvidence {
        evidence_revision,
        project_id,
        engine_version,
        scopes,
        connection,
        projection_ref,
        tables,
        extensions,
        unsupported,
    };
    if evidence.canonical_bytes().is_err() {
        return Err(diagnostic::export_limit_set(usize::MAX));
    }
    Ok(evidence)
}

/// Parse one bounded scope list; canonical order is name.
fn scopes(array: &[Json]) -> Result<Vec<ScopeName>, DiagnosticSet> {
    if array.is_empty() || array.len() > version::MAX_SCOPES {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        parsed.push(
            ScopeName::parse(
                entry
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("scopes-shape"))?,
            )
            .map_err(|_| diagnostic::input_invalid("scope-name"))?,
        );
    }
    parsed.sort();
    parsed.dedup();
    Ok(parsed)
}

/// Parse one observed-extension list; canonical order is name.
fn extension_names(array: &[Json]) -> Result<Vec<String>, DiagnosticSet> {
    if array.len() > super::version::MAX_EXTENSIONS {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let extension = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("extension-shape"))?;
        for key in extension.keys() {
            if key.as_str() != "name" {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = extension
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("extension-shape"))?
            .to_owned();
        if crate::storage_projection::id::StorageName::parse(&name).is_err() {
            return Err(diagnostic::input_invalid("extension-shape"));
        }
        parsed.push(name);
    }
    parsed.sort();
    parsed.dedup();
    Ok(parsed)
}

/// Parse one observed table list; canonical order is name.
fn tables(array: &[Json]) -> Result<Vec<ObservedTable>, DiagnosticSet> {
    if array.len() > 512 {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let table = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("table-shape"))?;
        for key in table.keys() {
            if !matches!(
                key.as_str(),
                "name" | "scope" | "columns" | "indexes" | "constraints"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = table
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("table-shape"))?
            .to_owned();
        if crate::storage_projection::id::StorageName::parse(&name).is_err() {
            return Err(diagnostic::input_invalid("table-shape"));
        }
        let scope = table
            .get("scope")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("table-shape"))?
            .to_owned();
        if crate::storage_projection::id::StorageName::parse(&scope).is_err() {
            return Err(diagnostic::input_invalid("table-shape"));
        }
        let columns = columns(
            table
                .get("columns")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("column-list"))?,
        )?;
        let indexes = match table.get("indexes") {
            Some(Json::Null) | None => Vec::new(),
            Some(value) => indexes(
                value
                    .as_array()
                    .ok_or_else(|| diagnostic::input_invalid("index-list"))?,
            )?,
        };
        let constraints = match table.get("constraints") {
            Some(Json::Null) | None => Vec::new(),
            Some(value) => constraints(
                value
                    .as_array()
                    .ok_or_else(|| diagnostic::input_invalid("constraint-list"))?,
            )?,
        };
        parsed.push(ObservedTable {
            name,
            scope,
            columns,
            indexes,
            constraints,
        });
    }
    parsed.sort_by(|left, right| left.name.cmp(&right.name));
    if parsed
        .windows(2)
        .any(|window| window[0].name == window[1].name)
    {
        return Err(diagnostic::input_invalid("duplicate-table"));
    }
    Ok(parsed)
}

/// Parse one observed column list; canonical order is name.
fn columns(array: &[Json]) -> Result<Vec<ObservedColumn>, DiagnosticSet> {
    if array.len() > 256 {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let column = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("column-shape"))?;
        for key in column.keys() {
            if !matches!(
                key.as_str(),
                "name" | "type" | "nullable" | "default" | "identity"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = column
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("column-shape"))?
            .to_owned();
        if crate::storage_projection::id::StorageName::parse(&name).is_err() {
            return Err(diagnostic::input_invalid("column-shape"));
        }
        let storage_type = column
            .get("type")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("column-shape"))?
            .to_owned();
        if storage_type.is_empty() || storage_type.len() > version::MAX_NAME_BYTES {
            return Err(diagnostic::input_invalid("column-shape"));
        }
        let nullable = column
            .get("nullable")
            .and_then(Json::as_bool)
            .ok_or_else(|| diagnostic::input_invalid("column-shape"))?;
        let default = match column.get("default") {
            Some(Json::Null) | None => None,
            Some(value) => {
                let text = value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("column-shape"))?;
                if text.is_empty() || text.len() > 128 {
                    return Err(diagnostic::input_invalid("column-shape"));
                }
                Some(text.to_owned())
            }
        };
        let identity = match column.get("identity") {
            Some(Json::Null) | None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("column-shape"))?,
        };
        parsed.push(ObservedColumn {
            name,
            storage_type,
            nullable,
            default,
            identity,
        });
    }
    parsed.sort_by(|left, right| left.name.cmp(&right.name));
    if parsed
        .windows(2)
        .any(|window| window[0].name == window[1].name)
    {
        return Err(diagnostic::input_invalid("duplicate-column"));
    }
    Ok(parsed)
}

/// Parse one observed index list; canonical order is (name, columns).
fn indexes(array: &[Json]) -> Result<Vec<ObservedIndex>, DiagnosticSet> {
    if array.len() > 64 {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let index = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("index-shape"))?;
        for key in index.keys() {
            if !matches!(key.as_str(), "name" | "columns" | "unique" | "partial") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = match index.get("name") {
            Some(Json::Null) | None => None,
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("index-shape"))?
                    .to_owned(),
            ),
        };
        let columns = bounded_storage_names(
            index
                .get("columns")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("index-shape"))?,
            16,
        )?;
        let unique = index
            .get("unique")
            .and_then(Json::as_bool)
            .ok_or_else(|| diagnostic::input_invalid("index-shape"))?;
        let partial = match index.get("partial") {
            Some(Json::Null) | None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("index-shape"))?,
        };
        parsed.push(ObservedIndex {
            name,
            columns,
            unique,
            partial,
        });
    }
    parsed.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.columns.cmp(&right.columns))
    });
    Ok(parsed)
}

/// Parse one observed constraint list; canonical order is
/// (kind, columns).
fn constraints(array: &[Json]) -> Result<Vec<ObservedConstraint>, DiagnosticSet> {
    if array.len() > 64 {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let constraint = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?;
        for key in constraint.keys() {
            if !matches!(key.as_str(), "kind" | "name" | "columns" | "target") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let kind = ConstraintKind::parse(
            constraint
                .get("kind")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("constraint-kind"))?,
        )
        .map_err(|_| diagnostic::input_invalid("constraint-kind"))?;
        let name = match constraint.get("name") {
            Some(Json::Null) | None => None,
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?
                    .to_owned(),
            ),
        };
        let columns = bounded_storage_names(
            constraint
                .get("columns")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?,
            16,
        )?;
        let target = match constraint.get("target") {
            Some(Json::Null) | None => None,
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?
                    .to_owned(),
            ),
        };
        parsed.push(ObservedConstraint {
            kind,
            name,
            columns,
            target,
        });
    }
    parsed.sort_by(|left, right| {
        left.kind
            .key()
            .cmp(right.kind.key())
            .then_with(|| left.columns.cmp(&right.columns))
    });
    Ok(parsed)
}

/// Parse one bounded storage-name string list.
fn bounded_storage_names(array: &[Json], bound: usize) -> Result<Vec<String>, DiagnosticSet> {
    if array.is_empty() || array.len() > bound {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let text = entry
            .as_str()
            .ok_or_else(|| diagnostic::input_invalid("member-shape"))?;
        if crate::storage_projection::id::StorageName::parse(text).is_err() {
            return Err(diagnostic::input_invalid("member-shape"));
        }
        parsed.push(text.to_owned());
    }
    Ok(parsed)
}

/// Parse one unsupported-record list; canonical order is
/// (kind, name, table, column).
fn unsupported_records(array: &[Json]) -> Result<Vec<UnsupportedRecord>, DiagnosticSet> {
    if array.len() > 64 {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let record = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("unsupported-shape"))?;
        for key in record.keys() {
            if !matches!(key.as_str(), "kind" | "name" | "table" | "column") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let kind = match record.get("kind").and_then(Json::as_str) {
            Some("type") => UnsupportedKind::Type,
            Some("extension") => UnsupportedKind::Extension,
            _ => return Err(diagnostic::input_invalid("unsupported-kind")),
        };
        let name = record
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("unsupported-shape"))?
            .to_owned();
        if name.is_empty() || name.len() > version::MAX_NAME_BYTES {
            return Err(diagnostic::input_invalid("unsupported-shape"));
        }
        if !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(diagnostic::input_invalid("unsupported-shape"));
        }
        let table = match record.get("table") {
            Some(Json::Null) | None => None,
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("unsupported-shape"))?
                    .to_owned(),
            ),
        };
        let column = match record.get("column") {
            Some(Json::Null) | None => None,
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("unsupported-shape"))?
                    .to_owned(),
            ),
        };
        parsed.push(UnsupportedRecord {
            kind,
            name,
            table,
            column,
        });
    }
    parsed.sort_by(|left, right| {
        left.kind
            .key()
            .cmp(right.kind.key())
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.table.cmp(&right.table))
            .then_with(|| left.column.cmp(&right.column))
    });
    Ok(parsed)
}

/// The canonical evidence payload bytes, or the typed over-bound
/// refusal.
fn canonical_bytes(evidence: &IntrospectionEvidence) -> Result<String, DiagnosticSet> {
    let bytes = payload(evidence);
    if bytes.len() > version::MAX_CANONICAL_BYTES {
        return Err(diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The full canonical evidence payload.
fn payload(evidence: &IntrospectionEvidence) -> String {
    use super::canonical::{array, flag, object, optional_array, string};
    let tables: Vec<String> = evidence
        .tables
        .iter()
        .map(|table| {
            let columns: Vec<String> = table
                .columns
                .iter()
                .map(|column| {
                    object(vec![
                        ("name", Some(string(&column.name))),
                        ("type", Some(string(&column.storage_type))),
                        ("nullable", Some(flag(column.nullable))),
                        ("default", column.default.as_deref().map(string)),
                        ("identity", Some(flag(column.identity))),
                    ])
                })
                .collect();
            let indexes: Vec<String> = table
                .indexes
                .iter()
                .map(|index| {
                    object(vec![
                        ("name", index.name.as_deref().map(string)),
                        (
                            "columns",
                            Some(array(
                                &index
                                    .columns
                                    .iter()
                                    .map(|text| string(text))
                                    .collect::<Vec<String>>(),
                            )),
                        ),
                        ("unique", Some(flag(index.unique))),
                        ("partial", Some(flag(index.partial))),
                    ])
                })
                .collect();
            let constraints: Vec<String> = table
                .constraints
                .iter()
                .map(|constraint| {
                    object(vec![
                        ("kind", Some(string(constraint.kind.key()))),
                        ("name", constraint.name.as_deref().map(string)),
                        (
                            "columns",
                            Some(array(
                                &constraint
                                    .columns
                                    .iter()
                                    .map(|text| string(text))
                                    .collect::<Vec<String>>(),
                            )),
                        ),
                        ("target", constraint.target.as_deref().map(string)),
                    ])
                })
                .collect();
            object(vec![
                ("name", Some(string(&table.name))),
                ("scope", Some(string(&table.scope))),
                ("columns", Some(array(&columns))),
                ("indexes", optional_array(&indexes)),
                ("constraints", optional_array(&constraints)),
            ])
        })
        .collect();
    let unsupported: Vec<String> = evidence
        .unsupported
        .iter()
        .map(|record| {
            object(vec![
                ("kind", Some(string(record.kind.key()))),
                ("name", Some(string(&record.name))),
                ("table", record.table.as_deref().map(string)),
                ("column", record.column.as_deref().map(string)),
            ])
        })
        .collect();
    object(vec![
        (
            "schemaVersion",
            Some(string("lekalo/storage-introspection/v0.4.0")),
        ),
        (
            "identity",
            Some(string("dev.lekalo.storage-introspection@0.4.0")),
        ),
        (
            "evidenceRevision",
            Some(string(evidence.evidence_revision.as_str())),
        ),
        ("projectId", Some(string(evidence.project_id.as_str()))),
        ("engine", Some(string(Engine::Postgres.key()))),
        (
            "engineVersion",
            Some(string(evidence.engine_version.as_str())),
        ),
        ("mode", Some(string("checked"))),
        ("readOnly", Some(flag(true))),
        (
            "scopes",
            Some(array(
                &evidence
                    .scopes
                    .iter()
                    .map(|scope| string(scope.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "connection",
            Some(object(vec![(
                "name",
                Some(string(evidence.connection.as_str())),
            )])),
        ),
        (
            "projectionRef",
            Some(string(evidence.projection_ref.as_str())),
        ),
        ("tables", Some(array(&tables))),
        (
            "extensions",
            optional_array(
                &evidence
                    .extensions
                    .iter()
                    .map(|text| string(text))
                    .collect::<Vec<String>>(),
            ),
        ),
        ("unsupported", optional_array(&unsupported)),
    ])
}
