//! Canonical serialization of the storage-projection attachment and
//! the derived storage projections (issue #65).
//!
//! Canonical bytes are compact UTF-8 JSON with byte-sorted object keys,
//! no BOM, and no trailing LF; absent optional members are dropped
//! entirely. Set-like collections (entities, fields, relations,
//! scenarios, constraints, references, tables, technical and generated
//! columns, joins, polymorphic materializations, migration tables)
//! normalize to unsigned UTF-8-byte order while primary keys, join
//! column pairs, and the migration history keep their declared
//! behavioral order. The output is path-independent and byte-identical
//! for value-equal attachments.

use crate::diagnostics::DiagnosticSet;

use super::entity::DomainType;
use super::StorageProjectionAttachment;

/// The canonical attachment payload bytes, or the typed over-bound
/// refusal.
pub fn attachment_bytes(attachment: &StorageProjectionAttachment) -> Result<String, DiagnosticSet> {
    let bytes = attachment_payload(attachment);
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(super::diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The canonical derived-projection payload bytes, or the typed
/// over-bound refusal.
pub fn derived_bytes(
    derived: &super::derivation::DerivedProjection,
) -> Result<String, DiagnosticSet> {
    let bytes = derived_payload(derived);
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(super::diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The full canonical attachment payload.
fn attachment_payload(attachment: &StorageProjectionAttachment) -> String {
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
                "dev.lekalo.ir@0.1.0",
                attachment.ir_digest().as_str(),
            )),
        ),
        (
            "sourceMapRef",
            attachment
                .source_map_ref()
                .map(|reference| string(reference.as_str())),
        ),
        (
            "entities",
            Some(array(
                &attachment
                    .entities()
                    .iter()
                    .map(entity_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "relations",
            Some(array(
                &attachment
                    .relations()
                    .iter()
                    .map(relation_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "projections",
            Some(array(
                &attachment
                    .projections()
                    .iter()
                    .map(projection_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
    ])
}

/// One canonical Model pin.
fn model_ref_payload(pin: &super::ModelPin) -> String {
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

/// One canonical domain entity.
fn entity_payload(entity: &super::entity::DomainEntity) -> String {
    let mut fields = entity.fields().to_vec();
    fields.sort_by(|left, right| left.name().as_str().cmp(right.name().as_str()));
    object(vec![
        ("entityKey", Some(string(entity.entity_key().as_str()))),
        ("entity", Some(string(entity.entity().as_str()))),
        ("external", flag_if(entity.external(), false)),
        ("aggregateRoot", flag_if(entity.aggregate_root(), false)),
        (
            "aggregateOwner",
            entity.aggregate_owner().map(|owner| string(owner.as_str())),
        ),
        ("visibility", Some(string(entity.visibility().key()))),
        (
            "fields",
            optional_array(&fields.iter().map(field_payload).collect::<Vec<String>>()),
        ),
        (
            "invariants",
            optional_array(
                &entity
                    .invariants()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "stateSpaces",
            optional_array(
                &entity
                    .state_spaces()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        ("description", entity.description().map(string)),
    ])
}

/// One canonical domain field.
fn field_payload(field: &super::entity::DomainField) -> String {
    object(vec![
        ("field", Some(string(field.name().as_str()))),
        ("type", Some(domain_type_payload(field.field_type()))),
        ("required", flag_if(field.required(), false)),
        ("visibility", Some(string(field.visibility().key()))),
    ])
}

/// One canonical domain value type.
fn domain_type_payload(field_type: &DomainType) -> String {
    match field_type {
        DomainType::Boolean => simple_type("boolean"),
        DomainType::Integer => simple_type("integer"),
        DomainType::Text => simple_type("text"),
        DomainType::Uuid => simple_type("uuid"),
        DomainType::Date => simple_type("date"),
        DomainType::Timestamp => simple_type("timestamp"),
        DomainType::Binary => simple_type("binary"),
        DomainType::Json => simple_type("json"),
        DomainType::String { length } => object(vec![
            ("name", Some(string("string"))),
            ("length", Some(integer(*length))),
        ]),
        DomainType::Decimal { precision, scale } => object(vec![
            ("name", Some(string("decimal"))),
            ("precision", Some(integer(*precision))),
            ("scale", Some(integer(*scale))),
        ]),
    }
}

/// One canonical parameterless domain type.
fn simple_type(name: &str) -> String {
    object(vec![("name", Some(string(name)))])
}

/// One canonical relation.
fn relation_payload(relation: &super::relation::Relation) -> String {
    object(vec![
        ("relationId", Some(string(relation.relation_id().as_str()))),
        ("owner", Some(string(relation.owner().as_str()))),
        ("target", Some(string(relation.target().as_str()))),
        ("kind", Some(string(relation.kind().key()))),
        ("min", Some(integer(relation.min()))),
        ("max", Some(integer(relation.max()))),
        (
            "deleteBehavior",
            Some(string(relation.delete_behavior().key())),
        ),
        (
            "foreignKey",
            relation.foreign_key().map(|column| string(column.as_str())),
        ),
        (
            "scenarios",
            optional_array(
                &relation
                    .scenarios()
                    .iter()
                    .map(scenario_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "constraints",
            optional_array(
                &relation
                    .constraints()
                    .iter()
                    .map(|reference| string(reference.as_str()))
                    .collect::<Vec<String>>(),
            ),
        ),
        ("description", relation.description().map(string)),
    ])
}

/// One canonical scenario reference.
fn scenario_payload(reference: &super::relation::ScenarioRef) -> String {
    object(vec![
        ("scenarioId", Some(string(reference.scenario_id()))),
        (
            "scenarioVersion",
            Some(string(reference.scenario_version())),
        ),
        ("irDigest", Some(string(reference.ir_digest().as_str()))),
    ])
}

/// One canonical projection.
fn projection_payload(projection: &super::projection::Projection) -> String {
    object(vec![
        ("namespace", Some(string(projection.namespace().key()))),
        (
            "tables",
            Some(array(
                &projection
                    .tables()
                    .iter()
                    .map(table_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "joins",
            optional_array(
                &projection
                    .joins()
                    .iter()
                    .map(join_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "polymorphics",
            optional_array(
                &projection
                    .polymorphics()
                    .iter()
                    .map(polymorphic_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "migrationHistory",
            optional_array(
                &projection
                    .migration_history()
                    .iter()
                    .map(migration_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
    ])
}

/// One canonical table.
fn table_payload(table: &super::projection::Table) -> String {
    object(vec![
        ("entity", Some(string(table.entity().as_str()))),
        ("table", Some(string(table.table().as_str()))),
        (
            "primaryKey",
            Some(array(
                &table
                    .primary_key()
                    .iter()
                    .map(|column| string(column.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "technicalColumns",
            optional_array(
                &table
                    .technical_columns()
                    .iter()
                    .map(technical_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "generatedColumns",
            optional_array(
                &table
                    .generated_columns()
                    .iter()
                    .map(generated_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "softDelete",
            table
                .soft_delete()
                .map(|column| object(vec![("column", Some(string(column.as_str())))])),
        ),
        (
            "tenantKey",
            table.tenant_key().map(|(column, storage_type)| {
                object(vec![
                    ("column", Some(string(column.as_str()))),
                    ("type", Some(string(storage_type.as_str()))),
                ])
            }),
        ),
        (
            "timestamps",
            table.timestamps().map(|(created_at, updated_at)| {
                object(vec![
                    ("createdAt", Some(string(created_at.as_str()))),
                    ("updatedAt", Some(string(updated_at.as_str()))),
                ])
            }),
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
    ])
}

/// One canonical technical column.
fn technical_payload(column: &super::projection::TechnicalColumn) -> String {
    object(vec![
        ("name", Some(string(column.name().as_str()))),
        ("type", Some(string(column.storage_type().as_str()))),
        ("purpose", Some(string(column.purpose()))),
        ("nullable", flag_if(column.nullable(), true)),
    ])
}

/// One canonical generated column.
fn generated_payload(column: &super::projection::GeneratedColumn) -> String {
    object(vec![
        ("name", Some(string(column.name().as_str()))),
        ("kind", Some(string(column.kind().key()))),
        (
            "type",
            column
                .storage_type()
                .map(|storage_type| string(storage_type.as_str())),
        ),
    ])
}

/// One canonical index.
fn index_payload(index: &super::projection::Index) -> String {
    object(vec![
        ("name", index.name().map(|name| string(name.as_str()))),
        (
            "columns",
            Some(array(
                &index
                    .columns()
                    .iter()
                    .map(|column| string(column.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        ("unique", Some(flag(index.unique()))),
    ])
}

/// One canonical join table.
fn join_payload(join: &super::projection::Join) -> String {
    object(vec![
        ("relation", Some(string(join.relation().as_str()))),
        ("table", Some(string(join.table().as_str()))),
        (
            "columns",
            Some(array(
                &join
                    .columns()
                    .iter()
                    .map(|column| string(column.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        ("uniquePair", flag_if(join.unique_pair(), false)),
    ])
}

/// One canonical polymorphic materialization.
fn polymorphic_payload(materialization: &super::projection::Polymorphic) -> String {
    object(vec![
        (
            "relation",
            Some(string(materialization.relation().as_str())),
        ),
        (
            "keyColumn",
            Some(string(materialization.key_column().as_str())),
        ),
        (
            "typeColumn",
            Some(string(materialization.type_column().as_str())),
        ),
    ])
}

/// One canonical migration-history record.
fn migration_payload(migration: &super::projection::Migration) -> String {
    object(vec![
        (
            "migrationId",
            Some(string(migration.migration_id().as_str())),
        ),
        (
            "tables",
            Some(array(
                &migration
                    .tables()
                    .iter()
                    .map(|table| string(table.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        ("risk", Some(string(migration.risk().key()))),
    ])
}

/// The canonical derived-projection payload.
fn derived_payload(derived: &super::derivation::DerivedProjection) -> String {
    object(vec![
        ("namespace", Some(string(derived.namespace().key()))),
        (
            "tables",
            Some(array(
                &derived
                    .tables()
                    .iter()
                    .map(derived_table_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "joins",
            optional_array(
                &derived
                    .joins()
                    .iter()
                    .map(derived_join_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
    ])
}

/// One canonical derived table.
fn derived_table_payload(table: &super::derivation::DerivedTable) -> String {
    object(vec![
        ("entity", Some(string(table.entity.as_str()))),
        ("table", Some(string(table.name.as_str()))),
        (
            "columns",
            Some(array(
                &table
                    .columns
                    .iter()
                    .map(derived_column_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "primaryKey",
            Some(array(
                &table
                    .primary_key
                    .iter()
                    .map(|column| string(column.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "foreignKeys",
            optional_array(
                &table
                    .foreign_keys
                    .iter()
                    .map(derived_foreign_key_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "indexes",
            optional_array(
                &table
                    .indexes
                    .iter()
                    .map(index_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
        (
            "polymorphics",
            optional_array(
                &table
                    .polymorphics
                    .iter()
                    .map(derived_polymorphic_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
    ])
}

/// One canonical derived column.
fn derived_column_payload(column: &super::derivation::DerivedColumn) -> String {
    object(vec![
        ("name", Some(string(column.name.as_str()))),
        ("type", Some(string(&column.storage_type))),
        ("nullable", Some(flag(column.nullable))),
        ("origin", Some(string(column.origin.key()))),
        ("visibility", Some(string(column.visibility.key()))),
    ])
}

/// One canonical derived foreign key.
fn derived_foreign_key_payload(foreign_key: &super::derivation::DerivedForeignKey) -> String {
    object(vec![
        ("column", Some(string(foreign_key.column.as_str()))),
        (
            "referencesTable",
            Some(string(foreign_key.references_table.as_str())),
        ),
        ("onDelete", Some(string(foreign_key.on_delete.key()))),
    ])
}

/// One canonical derived polymorphic materialization.
fn derived_polymorphic_payload(materialization: &super::derivation::DerivedPolymorphic) -> String {
    object(vec![
        ("relation", Some(string(materialization.relation.as_str()))),
        (
            "keyColumn",
            Some(string(materialization.key_column.as_str())),
        ),
        (
            "typeColumn",
            Some(string(materialization.type_column.as_str())),
        ),
    ])
}

/// One canonical derived join table.
fn derived_join_payload(join: &super::derivation::DerivedJoin) -> String {
    object(vec![
        ("relation", Some(string(join.relation.as_str()))),
        ("table", Some(string(join.name.as_str()))),
        (
            "columns",
            Some(array(
                &join
                    .columns
                    .iter()
                    .map(derived_column_payload)
                    .collect::<Vec<String>>(),
            )),
        ),
        ("uniquePair", Some(flag(join.unique_pair))),
        ("onOwnerDelete", Some(string(join.on_owner_delete.key()))),
        ("onTargetDelete", Some(string(join.on_target_delete.key()))),
    ])
}

/// One canonical JSON string value.
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

/// One canonical JSON integer.
fn integer(value: i64) -> String {
    value.to_string()
}

/// One canonical JSON boolean, dropped when it equals the default.
fn flag_if(value: bool, default: bool) -> Option<String> {
    if value == default {
        None
    } else {
        Some(flag(value))
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

/// One canonical JSON object with byte-sorted keys; `None` members are
/// dropped entirely.
fn object(members: Vec<(&str, Option<String>)>) -> String {
    let mut present: Vec<(&str, String)> = members
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect();
    present.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let body: Vec<String> = present
        .iter()
        .map(|(key, value)| format!("{}:{}", string(key), value))
        .collect();
    format!("{{{}}}", body.join(","))
}
