//! Wire normalization of the storage-introspection evidence (issue
//! #117).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`StorageIntrospection`](super::StorageIntrospection). It fails
//! closed before semantic processing: unknown or missing fields, wrong
//! identities, a mode other than the constant `checked`, a readOnly
//! value other than the constant `true`, hostile credential-shaped
//! members, and over-bound observed sets each return one typed
//! registered diagnostic and no partial evidence.

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{NamespacedId, SemanticId};
use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::id::SchemaName;
use super::version;
use super::StorageIntrospection;
use super::{
    EngineEcho, Evidence, EvidenceKind, ObservedColumn, ObservedDeleteAction, ObservedEngine,
    ObservedForeignKey, ObservedIndex, ObservedIndexKind, ObservedTable, SqlModeEcho,
};

/// The parsed wire object type.
type WireMap = Map<String, Json>;

/// The closed top-level member set. Hosts, ports, users, passwords,
/// DSNs, URLs, and connection strings are not members: the grammar
/// refuses them by absence.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "sourceMapRef",
    "mode",
    "readOnly",
    "engine",
    "testSchema",
    "tables",
    "observedDigest",
    "evidence",
];

/// The required top-level members (`sourceMapRef` and `evidence` are
/// optional).
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "mode",
    "readOnly",
    "engine",
    "testSchema",
    "tables",
    "observedDigest",
];

/// The closed host/credential-shaped member names the grammar refuses.
const FORBIDDEN_MEMBERS: &[&str] = &[
    "host",
    "hostname",
    "user",
    "username",
    "password",
    "dsn",
    "url",
    "uri",
    "port",
    "endpoint",
    "connectionString",
];

/// Normalize one wire document into validated evidence, or return the
/// typed rejection set with no partial document. Pure: no source,
/// network, process, or target access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<StorageIntrospection, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("top-level-shape"))?;
    for key in object.keys() {
        if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("credential-member"));
        }
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    for required in REQUIRED_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::input_invalid("missing-field"));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::input_invalid("schema-version"));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
        return Err(diagnostic::input_invalid("contract-identity"));
    }
    // The grammar carries no unchecked form: the mode is the constant
    // `checked`, readOnly the constant `true`.
    if object.get("mode").and_then(Json::as_str) != Some("checked") {
        return Err(diagnostic::input_invalid("mode-checked"));
    }
    if object.get("readOnly").and_then(Json::as_bool) != Some(true) {
        return Err(diagnostic::input_invalid("read-only-true"));
    }
    let attachment_revision = SemVer::parse(
        object
            .get("attachmentRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("attachment-revision"))?,
    )
    .map_err(|_| diagnostic::input_invalid("attachment-revision"))?;
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("project-id"))?,
    )
    .map_err(|_| diagnostic::input_invalid("project-id"))?;
    let model_ref = model_pin(
        object
            .get("modelRef")
            .ok_or_else(|| diagnostic::input_invalid("model-ref"))?,
    )?;
    let ir_digest = digest_member(
        object
            .get("irRef")
            .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?,
        "dev.lekalo.ir@0.2.16",
    )?;
    let source_map_ref = match object.get("sourceMapRef") {
        Some(Json::Null) | None => None,
        Some(value) => Some(
            NamespacedId::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("source-map-ref"))?,
            )
            .map_err(|_| diagnostic::input_invalid("source-map-ref"))?,
        ),
    };
    let engine = observed_engine(
        object
            .get("engine")
            .ok_or_else(|| diagnostic::input_invalid("engine-shape"))?,
    )?;
    let test_schema = SchemaName::parse(
        object
            .get("testSchema")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("test-schema"))?,
    )
    .map_err(|_| diagnostic::input_invalid("test-schema"))?;
    let tables = tables(
        object
            .get("tables")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("table-list"))?,
    )?;
    let observed_digest = Sha256Digest::parse(
        object
            .get("observedDigest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("observed-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("observed-digest"))?;
    let evidence = match object.get("evidence") {
        Some(Json::Null) | None => None,
        Some(value) => Some(evidence(value)?),
    };
    let attachment = StorageIntrospection::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_digest,
        source_map_ref,
        engine,
        test_schema,
        tables,
        observed_digest,
        evidence,
    );
    attachment.semantic_self_check()?;
    Ok(attachment)
}

/// Parse the bound source Model pin.
fn model_pin(json: &Json) -> Result<crate::storage_projection::ModelPin, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("model-ref"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "modelVersion" | "digest") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let pin_version = object
        .get("modelVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("model-version"))?;
    let pin = match pin_version {
        "0.2.16" => crate::scenario::ModelPin::Current,
        _ => return Err(diagnostic::input_invalid("model-version")),
    };
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("model-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("model-digest"))?;
    Ok(crate::storage_projection::ModelPin {
        version: pin,
        digest,
    })
}

/// Parse one `{identity, digest}` contract reference.
fn digest_member(json: &Json, identity: &str) -> Result<Sha256Digest, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "identity" | "digest") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    if object.get("identity").and_then(Json::as_str) != Some(identity) {
        return Err(diagnostic::input_invalid("contract-identity"));
    }
    Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("ir-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("ir-digest"))
}

/// One bounded string member with the declared token bound.
fn bounded_string(object: &WireMap, key: &str, bound: usize) -> Result<String, DiagnosticSet> {
    let text = object
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("member-shape"))?;
    if text.is_empty() || text.len() > bound {
        return Err(diagnostic::input_invalid("member-bound"));
    }
    Ok(text.to_owned())
}

/// Parse one bounded evidence record.
fn evidence(json: &Json) -> Result<Evidence, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("evidence-shape"))?;
    for key in object.keys() {
        if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("credential-member"));
        }
        if !matches!(key.as_str(), "kind" | "ref") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let kind = EvidenceKind::parse(
        object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("evidence-kind"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("evidence-kind"))?;
    let reference = bounded_string(object, "ref", version::MAX_EVIDENCE_REF_BYTES)?;
    Ok(Evidence { kind, reference })
}

/// Parse the observed engine identity echo.
fn observed_engine(json: &Json) -> Result<ObservedEngine, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("engine-shape"))?;
    for key in object.keys() {
        if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("credential-member"));
        }
        if !matches!(
            key.as_str(),
            "engine"
                | "engineVersion"
                | "sqlMode"
                | "defaultStorageEngine"
                | "charset"
                | "collation"
                | "timeZone"
        ) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let engine = EngineEcho::parse(
        object
            .get("engine")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("engine-token"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("engine-token"))?;
    let engine_version = bounded_string(object, "engineVersion", 32)?;
    let sql_mode_array = object
        .get("sqlMode")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("sql-mode-missing"))?;
    if sql_mode_array.len() > version::MAX_SQL_MODE_TOKENS {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut sql_mode = Vec::with_capacity(sql_mode_array.len());
    for entry in sql_mode_array {
        let token = SqlModeEcho::parse(
            entry
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("sql-mode-token"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("sql-mode-token"))?;
        sql_mode.push(token);
    }
    sql_mode.sort();
    sql_mode.dedup();
    let default_storage_engine =
        bounded_string(object, "defaultStorageEngine", version::MAX_TOKEN_BYTES)?;
    let charset = bounded_string(object, "charset", version::MAX_TOKEN_BYTES)?;
    let collation = bounded_string(object, "collation", version::MAX_TOKEN_BYTES)?;
    let time_zone = bounded_string(object, "timeZone", 16)?;
    Ok(ObservedEngine {
        engine,
        engine_version,
        sql_mode,
        default_storage_engine,
        charset,
        collation,
        time_zone,
    })
}

/// Parse the observed tables; canonical order is the table name.
fn tables(array: &[Json]) -> Result<Vec<ObservedTable>, DiagnosticSet> {
    if array.len() > version::MAX_TABLES {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("table-shape"))?;
        for key in object.keys() {
            if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
                return Err(diagnostic::input_invalid("credential-member"));
            }
            if !matches!(
                key.as_str(),
                "name" | "engine" | "collation" | "columns" | "indexes" | "foreignKeys"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = bounded_string(object, "name", version::MAX_TOKEN_BYTES)?;
        let engine = bounded_string(object, "engine", version::MAX_TOKEN_BYTES)?;
        let collation = bounded_string(object, "collation", version::MAX_TOKEN_BYTES)?;
        let columns = match object.get("columns") {
            Some(Json::Null) | None => Vec::new(),
            Some(value) => columns(
                value
                    .as_array()
                    .ok_or_else(|| diagnostic::input_invalid("column-list"))?,
            )?,
        };
        let indexes = match object.get("indexes") {
            Some(Json::Null) | None => Vec::new(),
            Some(value) => indexes(
                value
                    .as_array()
                    .ok_or_else(|| diagnostic::input_invalid("index-list"))?,
            )?,
        };
        let foreign_keys = match object.get("foreignKeys") {
            Some(Json::Null) | None => Vec::new(),
            Some(value) => foreign_keys(
                value
                    .as_array()
                    .ok_or_else(|| diagnostic::input_invalid("foreign-key-list"))?,
            )?,
        };
        parsed.push(ObservedTable {
            name,
            engine,
            collation,
            columns,
            indexes,
            foreign_keys,
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

/// Parse the observed columns; canonical order is the column name.
fn columns(array: &[Json]) -> Result<Vec<ObservedColumn>, DiagnosticSet> {
    if array.len() > version::MAX_COLUMNS {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("column-shape"))?;
        for key in object.keys() {
            if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
                return Err(diagnostic::input_invalid("credential-member"));
            }
            if !matches!(
                key.as_str(),
                "name" | "type" | "nullable" | "charset" | "collation" | "default" | "extra"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = bounded_string(object, "name", version::MAX_TOKEN_BYTES)?;
        let storage_type = bounded_string(object, "type", version::MAX_TOKEN_BYTES)?;
        let nullable = object
            .get("nullable")
            .and_then(Json::as_bool)
            .ok_or_else(|| diagnostic::input_invalid("column-shape"))?;
        let optional_token = |key: &str| -> Result<Option<String>, DiagnosticSet> {
            match object.get(key) {
                Some(Json::Null) | None => Ok(None),
                Some(value) => {
                    let text = value
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("member-shape"))?;
                    if text.is_empty() || text.len() > version::MAX_TOKEN_BYTES {
                        return Err(diagnostic::input_invalid("member-bound"));
                    }
                    Ok(Some(text.to_owned()))
                }
            }
        };
        parsed.push(ObservedColumn {
            name,
            storage_type,
            nullable,
            charset: optional_token("charset")?,
            collation: optional_token("collation")?,
            default: optional_token("default")?,
            extra: optional_token("extra")?,
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

/// Parse the observed indexes; canonical order is (name, columns).
fn indexes(array: &[Json]) -> Result<Vec<ObservedIndex>, DiagnosticSet> {
    if array.len() > version::MAX_INDEXES {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("index-shape"))?;
        for key in object.keys() {
            if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
                return Err(diagnostic::input_invalid("credential-member"));
            }
            if !matches!(
                key.as_str(),
                "name" | "columns" | "unique" | "kind" | "prefixLengths"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = match object.get("name") {
            Some(Json::Null) | None => None,
            Some(_) => Some(bounded_string(object, "name", version::MAX_TOKEN_BYTES)?),
        };
        let columns_array = object
            .get("columns")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("index-columns"))?;
        if columns_array.is_empty() || columns_array.len() > version::MAX_INDEX_COLUMNS {
            return Err(diagnostic::input_invalid("bound-exceeded"));
        }
        // The bounded string list parses plain strings directly.
        let mut columns = Vec::with_capacity(columns_array.len());
        for entry in columns_array {
            let text = entry
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("member-shape"))?;
            if text.is_empty() || text.len() > version::MAX_TOKEN_BYTES {
                return Err(diagnostic::input_invalid("member-bound"));
            }
            columns.push(text.to_owned());
        }
        let unique = object
            .get("unique")
            .and_then(Json::as_bool)
            .ok_or_else(|| diagnostic::input_invalid("index-shape"))?;
        let kind = match object.get("kind") {
            Some(Json::Null) | None => None,
            Some(value) => Some(
                ObservedIndexKind::parse(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("index-kind"))?,
                )
                .ok_or_else(|| diagnostic::input_invalid("index-kind"))?,
            ),
        };
        let prefix_lengths = match object.get("prefixLengths") {
            Some(Json::Null) | None => None,
            Some(value) => {
                let lengths = value
                    .as_array()
                    .ok_or_else(|| diagnostic::input_invalid("prefix-shape"))?;
                if lengths.len() != columns.len() {
                    return Err(diagnostic::input_invalid("prefix-arity"));
                }
                let mut parsed_lengths = Vec::with_capacity(lengths.len());
                for entry in lengths {
                    let length = entry
                        .as_u64()
                        .ok_or_else(|| diagnostic::input_invalid("prefix-shape"))?;
                    if !(1..=3072).contains(&length) {
                        return Err(diagnostic::input_invalid("prefix-bound"));
                    }
                    parsed_lengths.push(length as u16);
                }
                Some(parsed_lengths)
            }
        };
        parsed.push(ObservedIndex {
            name,
            columns,
            unique,
            kind,
            prefix_lengths,
        });
    }
    parsed.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.columns.cmp(&right.columns))
    });
    Ok(parsed)
}

/// Parse the observed foreign keys; canonical order is the column.
fn foreign_keys(array: &[Json]) -> Result<Vec<ObservedForeignKey>, DiagnosticSet> {
    if array.len() > version::MAX_FOREIGN_KEYS {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("foreign-key-shape"))?;
        for key in object.keys() {
            if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
                return Err(diagnostic::input_invalid("credential-member"));
            }
            if !matches!(key.as_str(), "column" | "referencesTable" | "onDelete") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let column = bounded_string(object, "column", version::MAX_TOKEN_BYTES)?;
        let references_table = bounded_string(object, "referencesTable", version::MAX_TOKEN_BYTES)?;
        let on_delete = ObservedDeleteAction::parse(
            object
                .get("onDelete")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("delete-action"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("delete-action"))?;
        parsed.push(ObservedForeignKey {
            column,
            references_table,
            on_delete,
        });
    }
    parsed.sort_by(|left, right| left.column.cmp(&right.column));
    Ok(parsed)
}
