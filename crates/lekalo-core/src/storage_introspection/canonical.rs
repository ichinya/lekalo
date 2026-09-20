//! Canonical serialization of the storage-introspection evidence
//! (issue #117).
//!
//! Canonical bytes are compact UTF-8 JSON with byte-sorted object keys,
//! no BOM, and no trailing LF; absent optional members are dropped
//! entirely. Set-like collections normalize to unsigned UTF-8-byte
//! order. The output is path-independent and byte-identical for
//! value-equal evidence documents.

use crate::diagnostics::DiagnosticSet;

use super::StorageIntrospection;

/// The canonical attachment payload bytes, or the typed over-bound
/// refusal.
pub fn attachment_bytes(attachment: &StorageIntrospection) -> Result<String, DiagnosticSet> {
    let bytes = payload(attachment);
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(super::diagnostic::input_invalid("canonical-bytes"));
    }
    Ok(bytes)
}

/// The full canonical evidence payload.
fn payload(attachment: &StorageIntrospection) -> String {
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
                "dev.lekalo.ir@0.2.16",
                attachment.ir_digest().as_str(),
            )),
        ),
        (
            "sourceMapRef",
            attachment
                .source_map_ref()
                .map(|reference| string(reference.as_str())),
        ),
        ("mode", Some(string("checked"))),
        ("readOnly", Some("true".to_owned())),
        ("engine", Some(engine_payload(attachment.engine()))),
        (
            "testSchema",
            Some(string(attachment.test_schema().as_str())),
        ),
        (
            "tables",
            Some(array(
                &attachment
                    .tables()
                    .iter()
                    .map(table_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "observedDigest",
            Some(string(attachment.observed_digest().as_str())),
        ),
        ("evidence", attachment.evidence().map(evidence_payload)),
    ])
}

/// One canonical Model pin.
fn model_ref_payload(pin: &crate::storage_projection::ModelPin) -> String {
    object(vec![
        ("modelVersion", Some(string(pin.version().as_str()))),
        ("digest", Some(string(pin.digest().as_str()))),
    ])
}

/// One canonical `{identity, digest}` contract reference.
fn digest_payload(identity: &str, digest: &str) -> String {
    object(vec![
        ("identity", Some(string(identity))),
        ("digest", Some(string(digest))),
    ])
}

/// One canonical observed engine block.
fn engine_payload(engine: &super::ObservedEngine) -> String {
    object(vec![
        ("engine", Some(string(engine.engine().key()))),
        ("engineVersion", Some(string(engine.engine_version()))),
        (
            "sqlMode",
            Some(array(
                &engine
                    .sql_mode()
                    .iter()
                    .map(|token| string(token.key()))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "defaultStorageEngine",
            Some(string(engine.default_storage_engine())),
        ),
        ("charset", Some(string(engine.charset()))),
        ("collation", Some(string(engine.collation()))),
        ("timeZone", Some(string(engine.time_zone()))),
    ])
}

/// One canonical observed table.
fn table_payload(table: &super::ObservedTable) -> String {
    object(vec![
        ("name", Some(string(table.name()))),
        ("engine", Some(string(table.engine()))),
        ("collation", Some(string(table.collation()))),
        (
            "columns",
            Some(array(
                &table
                    .columns()
                    .iter()
                    .map(column_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "indexes",
            optional_array(
                &table
                    .indexes()
                    .iter()
                    .map(index_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "foreignKeys",
            optional_array(
                &table
                    .foreign_keys()
                    .iter()
                    .map(foreign_key_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
    ])
}

/// One canonical observed column.
fn column_payload(column: &super::ObservedColumn) -> String {
    object(vec![
        ("name", Some(string(column.name()))),
        ("type", Some(string(column.storage_type()))),
        ("nullable", Some(flag(column.nullable()))),
        ("charset", column.charset().map(string)),
        ("collation", column.collation().map(string)),
        ("default", column.default().map(string)),
        ("extra", column.extra().map(string)),
    ])
}

/// One canonical observed index.
fn index_payload(index: &super::ObservedIndex) -> String {
    object(vec![
        ("name", index.name().map(string)),
        (
            "columns",
            Some(array(
                &index
                    .columns()
                    .iter()
                    .map(|column| string(column))
                    .collect::<Vec<String>>(),
            )),
        ),
        ("unique", Some(flag(index.unique()))),
        ("kind", index.kind().map(|kind| string(kind.key()))),
        (
            "prefixLengths",
            index.prefix_lengths().map(|lengths| {
                array(
                    &lengths
                        .iter()
                        .map(|length| length.to_string())
                        .collect::<Vec<String>>(),
                )
            }),
        ),
    ])
}

/// One canonical observed foreign key.
fn foreign_key_payload(foreign_key: &super::ObservedForeignKey) -> String {
    object(vec![
        ("column", Some(string(foreign_key.column()))),
        (
            "referencesTable",
            Some(string(foreign_key.references_table())),
        ),
        ("onDelete", Some(string(foreign_key.on_delete().key()))),
    ])
}

/// One canonical evidence record.
fn evidence_payload(evidence: &super::Evidence) -> String {
    object(vec![
        ("kind", Some(string(evidence.kind().key()))),
        ("ref", Some(string(evidence.reference()))),
    ])
}

/// One canonical JSON string.
fn string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// One canonical JSON boolean.
fn flag(value: bool) -> String {
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

/// One canonical JSON object with byte-sorted members.
fn object(members: Vec<(&str, Option<String>)>) -> String {
    let mut sorted: Vec<(String, String)> = members
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value)))
        .collect();
    sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let body: Vec<String> = sorted
        .into_iter()
        .map(|(key, value)| format!("{}:{value}", string(&key)))
        .collect();
    format!("{{{}}}", body.join(","))
}
