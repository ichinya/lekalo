//! Wire normalization of the storage-projection attachment (issue
//! #65).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`StorageProjectionAttachment`](super::StorageProjectionAttachment).
//! It fails closed before semantic processing: unknown or missing
//! fields, wrong identities, malformed identifiers, digests, bounds,
//! and incoherent type parameters each return one typed registered
//! diagnostic and no partial attachment. Semantic rules (reference
//! resolution, aggregate coherence, kind/cardinality/delete-behavior
//! coherence, coverage, projection-to-domain mapping) live in the
//! attachment's semantic self-check.

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{FieldName, NamespacedId, SemanticId};
use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::entity::{DomainEntity, DomainField, DomainType, Visibility};
use super::id::{EntityKey, StorageName};
use super::projection::{
    DataRisk, GeneratedColumn, GeneratedKind, Index, Join, Migration, Namespace, Polymorphic,
    Projection, StorageType, Table, TechnicalColumn,
};
use super::relation::{DeleteBehavior, Relation, RelationKind, ScenarioRef};
use super::version;
use super::{ModelPin, StorageProjectionAttachment};

/// The parsed wire object type.
type WireMap = Map<String, Json>;

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "sourceMapRef",
    "entities",
    "relations",
    "projections",
];

/// The required top-level members (`sourceMapRef` is optional).
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "entities",
    "relations",
    "projections",
];

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial attachment. Pure: no source,
/// model, cache, report, network, process, or target access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<StorageProjectionAttachment, DiagnosticSet> {
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
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::input_invalid("schema-version"));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
        return Err(diagnostic::input_invalid("contract-identity"));
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
        "dev.lekalo.ir@0.1.0",
    )?;
    let source_map_ref = match object.get("sourceMapRef") {
        Some(value) => Some(
            NamespacedId::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("source-map-ref"))?,
            )
            .map_err(|_| diagnostic::input_invalid("source-map-ref"))?,
        ),
        None => None,
    };
    let entities = entities(
        object
            .get("entities")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("entity-list"))?,
    )?;
    let relations = relations(
        object
            .get("relations")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("relation-list"))?,
    )?;
    let projections = projections(
        object
            .get("projections")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("projection-list"))?,
    )?;
    let attachment = StorageProjectionAttachment::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_digest,
        source_map_ref,
        entities,
        relations,
        projections,
    );
    attachment.semantic_self_check()?;
    Ok(attachment)
}

/// Parse the bound source Model pin.
fn model_pin(json: &Json) -> Result<ModelPin, DiagnosticSet> {
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
        "0.1.0" => crate::scenario::ModelPin::V0_1_0,
        "1.0.0" => crate::scenario::ModelPin::V1_0_0,
        _ => return Err(diagnostic::input_invalid("model-version")),
    };
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("model-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("model-digest"))?;
    Ok(ModelPin {
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

/// One bounded string member.
fn string_member<'a>(object: &'a WireMap, key: &str) -> Result<&'a str, DiagnosticSet> {
    object
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("member-shape"))
}

/// One bounded array member with the closed length bound.
fn bounded_array<'a>(
    object: &'a WireMap,
    key: &str,
    bound: usize,
) -> Result<&'a Vec<Json>, DiagnosticSet> {
    let array = object
        .get(key)
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("member-shape"))?;
    if array.len() > bound {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    Ok(array)
}

/// One optional bounded array member.
fn optional_bounded_array<'a>(
    object: &'a WireMap,
    key: &str,
    bound: usize,
) -> Result<Option<&'a Vec<Json>>, DiagnosticSet> {
    match object.get(key) {
        Some(Json::Null) | None => Ok(None),
        Some(value) => {
            let array = value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("member-shape"))?;
            if array.len() > bound {
                return Err(diagnostic::input_invalid("bound-exceeded"));
            }
            Ok(Some(array))
        }
    }
}

/// One bounded signed integer member.
fn integer_member(object: &WireMap, key: &str) -> Result<i64, DiagnosticSet> {
    object
        .get(key)
        .and_then(Json::as_i64)
        .ok_or_else(|| diagnostic::input_invalid("member-shape"))
}

/// One typed identifier member parsed through the accepted grammar.
fn id_member<T>(
    object: &WireMap,
    key: &str,
    detail: &'static str,
    parse: fn(&str) -> Result<T, crate::scenario::id::IdError>,
) -> Result<T, DiagnosticSet> {
    parse(string_member(object, key)?).map_err(|_| diagnostic::input_invalid(detail))
}

/// One closed bounded description member.
fn description_member(object: &WireMap) -> Result<Option<String>, DiagnosticSet> {
    match object.get("description") {
        Some(Json::Null) | None => Ok(None),
        Some(value) => {
            let text = value
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("description"))?;
            if text.is_empty() || text.len() > 256 {
                return Err(diagnostic::input_invalid("description"));
            }
            Ok(Some(text.to_owned()))
        }
    }
}

/// Parse every declared domain entity; canonical order is entity key.
fn entities(array: &[Json]) -> Result<Vec<DomainEntity>, DiagnosticSet> {
    if array.len() > version::MAX_ENTITIES {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let entity = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("entity-shape"))?;
        for key in entity.keys() {
            if !matches!(
                key.as_str(),
                "entityKey"
                    | "entity"
                    | "external"
                    | "aggregateRoot"
                    | "aggregateOwner"
                    | "visibility"
                    | "fields"
                    | "invariants"
                    | "stateSpaces"
                    | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let entity_key = id_member(entity, "entityKey", "entity-key", EntityKey::parse)?;
        let entity_id = id_member(entity, "entity", "entity-ref", SemanticId::parse)?;
        let external = match entity.get("external") {
            Some(Json::Null) | None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("entity-shape"))?,
        };
        let aggregate_root = match entity.get("aggregateRoot") {
            Some(Json::Null) | None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("aggregate-shape"))?,
        };
        let aggregate_owner = match entity.get("aggregateOwner") {
            Some(Json::Null) | None => None,
            Some(_) => Some(id_member(
                entity,
                "aggregateOwner",
                "aggregate-shape",
                EntityKey::parse,
            )?),
        };
        let visibility = Visibility::parse(
            entity
                .get("visibility")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("visibility"))?,
        )
        .map_err(|_| diagnostic::input_invalid("visibility"))?;
        let fields = match optional_bounded_array(entity, "fields", version::MAX_FIELDS)? {
            Some(entries) => fields(entries)?,
            None => Vec::new(),
        };
        let invariants = namespaced_refs(entity, "invariants", version::MAX_INVARIANT_REFS)?;
        let state_spaces = namespaced_refs(entity, "stateSpaces", version::MAX_STATE_SPACE_REFS)?;
        let description = description_member(entity)?;
        parsed.push(DomainEntity {
            entity_key,
            entity: entity_id,
            external,
            aggregate_root,
            aggregate_owner,
            visibility,
            fields,
            invariants,
            state_spaces,
            description,
        });
    }
    parsed.sort_by(|left, right| left.entity_key.cmp(&right.entity_key));
    if has_adjacent_duplicate(&parsed, |entity| &entity.entity_key) {
        return Err(diagnostic::input_invalid("duplicate-entity-key"));
    }
    Ok(parsed)
}

/// Parse the declared domain fields; canonical order is field name.
fn fields(array: &[Json]) -> Result<Vec<DomainField>, DiagnosticSet> {
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let field = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("field-shape"))?;
        for key in field.keys() {
            if !matches!(key.as_str(), "field" | "type" | "required" | "visibility") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = id_member(field, "field", "field-name", FieldName::parse)?;
        let field_type = field
            .get("type")
            .and_then(Json::as_object)
            .ok_or_else(|| diagnostic::input_invalid("type-shape"))?;
        for key in field_type.keys() {
            if !matches!(key.as_str(), "name" | "length" | "precision" | "scale") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let type_name = field_type
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("type-name"))?;
        let param = |key: &str| match field_type.get(key) {
            Some(Json::Null) | None => Ok(None),
            Some(value) => value
                .as_i64()
                .map(Some)
                .ok_or_else(|| diagnostic::input_invalid("type-params")),
        };
        let length = param("length")?;
        let precision = param("precision")?;
        let scale = param("scale")?;
        let domain_type = match type_name {
            "boolean" => DomainType::Boolean,
            "integer" => DomainType::Integer,
            "text" => DomainType::Text,
            "uuid" => DomainType::Uuid,
            "date" => DomainType::Date,
            "timestamp" => DomainType::Timestamp,
            "binary" => DomainType::Binary,
            "json" => DomainType::Json,
            "string" => {
                let length = length.ok_or_else(|| diagnostic::input_invalid("type-params"))?;
                if !(1..=version::MAX_STRING_LENGTH).contains(&length) {
                    return Err(diagnostic::input_invalid("type-params"));
                }
                DomainType::String { length }
            }
            "decimal" => {
                let precision =
                    precision.ok_or_else(|| diagnostic::input_invalid("type-params"))?;
                let scale = scale.ok_or_else(|| diagnostic::input_invalid("type-params"))?;
                if !(1..=version::MAX_DECIMAL_PRECISION).contains(&precision)
                    || !(0..=precision).contains(&scale)
                {
                    return Err(diagnostic::input_invalid("type-params"));
                }
                DomainType::Decimal { precision, scale }
            }
            _ => return Err(diagnostic::input_invalid("type-name")),
        };
        if type_name != "string" && length.is_some() {
            return Err(diagnostic::input_invalid("type-params"));
        }
        if type_name != "decimal" && (precision.is_some() || scale.is_some()) {
            return Err(diagnostic::input_invalid("type-params"));
        }
        let required = match field.get("required") {
            Some(Json::Null) | None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("field-shape"))?,
        };
        let visibility = Visibility::parse(
            field
                .get("visibility")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("visibility"))?,
        )
        .map_err(|_| diagnostic::input_invalid("visibility"))?;
        parsed.push(DomainField {
            field: name,
            field_type: domain_type,
            required,
            visibility,
        });
    }
    parsed.sort_by(|left, right| left.field.cmp(&right.field));
    if has_adjacent_duplicate(&parsed, |field| &field.field) {
        return Err(diagnostic::input_invalid("duplicate-field"));
    }
    Ok(parsed)
}

/// Parse one sorted, deduplicated namespaced reference list.
fn namespaced_refs(
    object: &WireMap,
    key: &str,
    bound: usize,
) -> Result<Vec<NamespacedId>, DiagnosticSet> {
    let entries = match optional_bounded_array(object, key, bound)? {
        Some(entries) => entries,
        None => return Ok(Vec::new()),
    };
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        parsed.push(
            NamespacedId::parse(
                entry
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("ref-shape"))?,
            )
            .map_err(|_| diagnostic::input_invalid("ref-shape"))?,
        );
    }
    parsed.sort();
    parsed.dedup();
    Ok(parsed)
}

/// Parse every declared relation; canonical order is relation id.
fn relations(array: &[Json]) -> Result<Vec<Relation>, DiagnosticSet> {
    if array.len() > version::MAX_RELATIONS {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let relation = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("relation-shape"))?;
        for key in relation.keys() {
            if !matches!(
                key.as_str(),
                "relationId"
                    | "owner"
                    | "target"
                    | "kind"
                    | "min"
                    | "max"
                    | "deleteBehavior"
                    | "foreignKey"
                    | "scenarios"
                    | "constraints"
                    | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let relation_id = id_member(relation, "relationId", "relation-id", SemanticId::parse)?;
        let owner = id_member(relation, "owner", "relation-owner", EntityKey::parse)?;
        let target = id_member(relation, "target", "relation-target", EntityKey::parse)?;
        let kind = RelationKind::parse(
            relation
                .get("kind")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("relation-kind"))?,
        )
        .map_err(|_| diagnostic::input_invalid("relation-kind"))?;
        let min = integer_member(relation, "min")?;
        let max = integer_member(relation, "max")?;
        if min < 0 || max < 0 || min > version::MAX_CARDINALITY || max > version::MAX_CARDINALITY {
            return Err(diagnostic::input_invalid("cardinality"));
        }
        let delete_behavior = DeleteBehavior::parse(
            relation
                .get("deleteBehavior")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("delete-behavior"))?,
        )
        .map_err(|_| diagnostic::input_invalid("delete-behavior"))?;
        let foreign_key = match relation.get("foreignKey") {
            Some(Json::Null) | None => None,
            Some(_) => Some(id_member(
                relation,
                "foreignKey",
                "foreign-key",
                StorageName::parse,
            )?),
        };
        let scenarios = scenario_refs(relation)?;
        let constraints = namespaced_refs(relation, "constraints", version::MAX_CONSTRAINT_REFS)?;
        let description = description_member(relation)?;
        parsed.push(Relation {
            relation_id,
            owner,
            target,
            kind,
            min,
            max,
            delete_behavior,
            foreign_key,
            scenarios,
            constraints,
            description,
        });
    }
    parsed.sort_by(|left, right| left.relation_id.cmp(&right.relation_id));
    if has_adjacent_duplicate(&parsed, |relation| &relation.relation_id) {
        return Err(diagnostic::input_invalid("duplicate-relation-id"));
    }
    Ok(parsed)
}

/// Parse the pinned scenario reference list with uniqueness.
fn scenario_refs(object: &WireMap) -> Result<Vec<ScenarioRef>, DiagnosticSet> {
    let entries = match optional_bounded_array(object, "scenarios", version::MAX_SCENARIO_REFS)? {
        Some(entries) => entries,
        None => return Ok(Vec::new()),
    };
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        let scenario = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("scenario-shape"))?;
        for key in scenario.keys() {
            if !matches!(key.as_str(), "scenarioId" | "scenarioVersion" | "irDigest") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let scenario_id = string_member(scenario, "scenarioId")?.to_owned();
        if scenario_id.len() < 3 || scenario_id.len() > 191 {
            return Err(diagnostic::input_invalid("scenario-id"));
        }
        let scenario_version = SemVer::parse(string_member(scenario, "scenarioVersion")?)
            .map_err(|_| diagnostic::input_invalid("scenario-version"))?;
        let ir_digest = Sha256Digest::parse(string_member(scenario, "irDigest")?)
            .map_err(|_| diagnostic::input_invalid("scenario-digest"))?;
        parsed.push(ScenarioRef {
            scenario_id,
            scenario_version: scenario_version.as_str().to_owned(),
            ir_digest,
        });
    }
    parsed.sort();
    let before = parsed.len();
    parsed.dedup();
    if parsed.len() != before {
        return Err(diagnostic::input_invalid("duplicate-ref"));
    }
    Ok(parsed)
}

/// Parse every declared projection; canonical order is namespace.
fn projections(array: &[Json]) -> Result<Vec<Projection>, DiagnosticSet> {
    if array.len() > version::MAX_PROJECTIONS {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let projection = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("projection-shape"))?;
        for key in projection.keys() {
            if !matches!(
                key.as_str(),
                "namespace" | "tables" | "joins" | "polymorphics" | "migrationHistory"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let namespace = Namespace::parse(
            projection
                .get("namespace")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("namespace"))?,
        )
        .map_err(|_| diagnostic::input_invalid("namespace"))?;
        let tables = tables(bounded_array(projection, "tables", version::MAX_TABLES)?)?;
        let joins = match optional_bounded_array(projection, "joins", version::MAX_JOINS)? {
            Some(entries) => joins(entries)?,
            None => Vec::new(),
        };
        let polymorphics =
            match optional_bounded_array(projection, "polymorphics", version::MAX_POLYMORPHICS)? {
                Some(entries) => polymorphics(entries)?,
                None => Vec::new(),
            };
        let migration_history = match optional_bounded_array(
            projection,
            "migrationHistory",
            version::MAX_MIGRATIONS,
        )? {
            Some(entries) => migrations(entries)?,
            None => Vec::new(),
        };
        parsed.push(Projection {
            namespace,
            tables,
            joins,
            polymorphics,
            migration_history,
        });
    }
    parsed.sort_by_key(|projection| projection.namespace.key());
    if has_adjacent_duplicate(&parsed, |projection| &projection.namespace) {
        return Err(diagnostic::input_invalid("duplicate-namespace"));
    }
    Ok(parsed)
}

/// Parse the declared tables; canonical order is entity key.
fn tables(array: &[Json]) -> Result<Vec<Table>, DiagnosticSet> {
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let table = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("table-shape"))?;
        for key in table.keys() {
            if !matches!(
                key.as_str(),
                "entity"
                    | "table"
                    | "primaryKey"
                    | "technicalColumns"
                    | "generatedColumns"
                    | "softDelete"
                    | "tenantKey"
                    | "timestamps"
                    | "indexes"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let entity = id_member(table, "entity", "table-entity", EntityKey::parse)?;
        let name = id_member(table, "table", "table-name", StorageName::parse)?;
        let primary_key = storage_name_list(
            table,
            "primaryKey",
            version::MAX_PRIMARY_KEY_COLUMNS,
            "primary-key",
        )?;
        if primary_key.is_empty() {
            return Err(diagnostic::input_invalid("primary-key"));
        }
        let technical_columns = match optional_bounded_array(
            table,
            "technicalColumns",
            version::MAX_TECHNICAL_COLUMNS,
        )? {
            Some(entries) => technical_columns(entries)?,
            None => Vec::new(),
        };
        let generated_columns = match optional_bounded_array(
            table,
            "generatedColumns",
            version::MAX_GENERATED_COLUMNS,
        )? {
            Some(entries) => generated_columns(entries)?,
            None => Vec::new(),
        };
        let soft_delete = match table.get("softDelete") {
            Some(Json::Null) | None => None,
            Some(value) => {
                let object = value
                    .as_object()
                    .ok_or_else(|| diagnostic::input_invalid("soft-delete-shape"))?;
                for key in object.keys() {
                    if key.as_str() != "column" {
                        return Err(diagnostic::input_invalid("unknown-field"));
                    }
                }
                Some(id_member(
                    object,
                    "column",
                    "soft-delete-shape",
                    StorageName::parse,
                )?)
            }
        };
        let tenant_key = match table.get("tenantKey") {
            Some(Json::Null) | None => None,
            Some(value) => {
                let object = value
                    .as_object()
                    .ok_or_else(|| diagnostic::input_invalid("tenant-shape"))?;
                for key in object.keys() {
                    if !matches!(key.as_str(), "column" | "type") {
                        return Err(diagnostic::input_invalid("unknown-field"));
                    }
                }
                let column = id_member(object, "column", "tenant-shape", StorageName::parse)?;
                let storage_type = StorageType::parse(string_member(object, "type")?)
                    .map_err(|_| diagnostic::input_invalid("storage-type"))?;
                Some((column, storage_type))
            }
        };
        let timestamps = match table.get("timestamps") {
            Some(Json::Null) | None => None,
            Some(value) => {
                let object = value
                    .as_object()
                    .ok_or_else(|| diagnostic::input_invalid("timestamps-shape"))?;
                for key in object.keys() {
                    if !matches!(key.as_str(), "createdAt" | "updatedAt") {
                        return Err(diagnostic::input_invalid("unknown-field"));
                    }
                }
                let created_at =
                    id_member(object, "createdAt", "timestamps-shape", StorageName::parse)?;
                let updated_at =
                    id_member(object, "updatedAt", "timestamps-shape", StorageName::parse)?;
                Some((created_at, updated_at))
            }
        };
        let indexes = match optional_bounded_array(table, "indexes", version::MAX_INDEXES)? {
            Some(entries) => indexes(entries)?,
            None => Vec::new(),
        };
        parsed.push(Table {
            entity,
            table: name,
            primary_key,
            technical_columns,
            generated_columns,
            soft_delete,
            tenant_key,
            timestamps,
            indexes,
        });
    }
    parsed.sort_by(|left, right| left.entity.cmp(&right.entity));
    if has_adjacent_duplicate(&parsed, |table| &table.entity) {
        return Err(diagnostic::input_invalid("duplicate-table-entity"));
    }
    Ok(parsed)
}

/// Parse one declared technical column list; canonical order is name.
fn technical_columns(array: &[Json]) -> Result<Vec<TechnicalColumn>, DiagnosticSet> {
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let column = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("technical-shape"))?;
        for key in column.keys() {
            if !matches!(key.as_str(), "name" | "type" | "purpose" | "nullable") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = id_member(column, "name", "technical-shape", StorageName::parse)?;
        let storage_type = StorageType::parse(string_member(column, "type")?)
            .map_err(|_| diagnostic::input_invalid("storage-type"))?;
        let purpose = string_member(column, "purpose")?.to_owned();
        if purpose.is_empty() || purpose.len() > 256 {
            return Err(diagnostic::input_invalid("technical-shape"));
        }
        let nullable = match column.get("nullable") {
            Some(Json::Null) | None => true,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("technical-shape"))?,
        };
        parsed.push(TechnicalColumn {
            name,
            storage_type,
            purpose,
            nullable,
        });
    }
    parsed.sort_by(|left, right| left.name.cmp(&right.name));
    if has_adjacent_duplicate(&parsed, |column| &column.name) {
        return Err(diagnostic::input_invalid("duplicate-column"));
    }
    Ok(parsed)
}

/// Parse one declared generated column list; canonical order is name.
fn generated_columns(array: &[Json]) -> Result<Vec<GeneratedColumn>, DiagnosticSet> {
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let column = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("generated-shape"))?;
        for key in column.keys() {
            if !matches!(key.as_str(), "name" | "kind" | "type") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = id_member(column, "name", "generated-shape", StorageName::parse)?;
        let kind = GeneratedKind::parse(
            column
                .get("kind")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("generated-kind"))?,
        )
        .map_err(|_| diagnostic::input_invalid("generated-kind"))?;
        let storage_type = match column.get("type") {
            Some(Json::Null) | None => None,
            Some(_) => Some(
                StorageType::parse(string_member(column, "type")?)
                    .map_err(|_| diagnostic::input_invalid("storage-type"))?,
            ),
        };
        parsed.push(GeneratedColumn {
            name,
            kind,
            storage_type,
        });
    }
    parsed.sort_by(|left, right| left.name.cmp(&right.name));
    if has_adjacent_duplicate(&parsed, |column| &column.name) {
        return Err(diagnostic::input_invalid("duplicate-column"));
    }
    Ok(parsed)
}

/// Parse one declared index list; canonical order is (name, columns).
fn indexes(array: &[Json]) -> Result<Vec<Index>, DiagnosticSet> {
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let index = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("index-shape"))?;
        for key in index.keys() {
            if !matches!(key.as_str(), "name" | "columns" | "unique") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = match index.get("name") {
            Some(Json::Null) | None => None,
            Some(_) => Some(id_member(index, "name", "index-name", StorageName::parse)?),
        };
        let columns = storage_name_list(
            index,
            "columns",
            version::MAX_INDEX_COLUMNS,
            "index-columns",
        )?;
        if columns.is_empty() {
            return Err(diagnostic::input_invalid("index-columns"));
        }
        let unique = index
            .get("unique")
            .and_then(Json::as_bool)
            .ok_or_else(|| diagnostic::input_invalid("index-shape"))?;
        parsed.push(Index {
            name,
            columns,
            unique,
        });
    }
    parsed.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.columns.cmp(&right.columns))
    });
    Ok(parsed)
}

/// Parse one bounded storage-name list member.
fn storage_name_list(
    object: &WireMap,
    key: &str,
    bound: usize,
    detail: &'static str,
) -> Result<Vec<StorageName>, DiagnosticSet> {
    let entries =
        bounded_array(object, key, bound).map_err(|_| diagnostic::input_invalid(detail))?;
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        parsed.push(
            StorageName::parse(
                entry
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid(detail))?,
            )
            .map_err(|_| diagnostic::input_invalid(detail))?,
        );
    }
    Ok(parsed)
}

/// Parse the declared join tables; canonical order is relation id.
fn joins(array: &[Json]) -> Result<Vec<Join>, DiagnosticSet> {
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let join = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("join-shape"))?;
        for key in join.keys() {
            if !matches!(
                key.as_str(),
                "relation" | "table" | "columns" | "uniquePair"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let relation = id_member(join, "relation", "join-shape", SemanticId::parse)?;
        let table = id_member(join, "table", "table-name", StorageName::parse)?;
        let columns = storage_name_list(join, "columns", 2, "join-columns")?;
        if columns.len() != 2 {
            return Err(diagnostic::input_invalid("join-columns"));
        }
        let unique_pair = match join.get("uniquePair") {
            Some(Json::Null) | None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::input_invalid("join-shape"))?,
        };
        let pair = [columns[0].clone(), columns[1].clone()];
        parsed.push(Join {
            relation,
            table,
            columns: pair,
            unique_pair,
        });
    }
    parsed.sort_by(|left, right| left.relation.cmp(&right.relation));
    if has_adjacent_duplicate(&parsed, |join| &join.relation) {
        return Err(diagnostic::input_invalid("duplicate-join"));
    }
    Ok(parsed)
}

/// Parse the declared polymorphic materializations; canonical order is
/// relation id.
fn polymorphics(array: &[Json]) -> Result<Vec<Polymorphic>, DiagnosticSet> {
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let polymorphic = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("polymorphic-shape"))?;
        for key in polymorphic.keys() {
            if !matches!(key.as_str(), "relation" | "keyColumn" | "typeColumn") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let relation = id_member(
            polymorphic,
            "relation",
            "polymorphic-shape",
            SemanticId::parse,
        )?;
        let key_column = id_member(
            polymorphic,
            "keyColumn",
            "polymorphic-shape",
            StorageName::parse,
        )?;
        let type_column = id_member(
            polymorphic,
            "typeColumn",
            "polymorphic-shape",
            StorageName::parse,
        )?;
        parsed.push(Polymorphic {
            relation,
            key_column,
            type_column,
        });
    }
    parsed.sort_by(|left, right| left.relation.cmp(&right.relation));
    if has_adjacent_duplicate(&parsed, |polymorphic| &polymorphic.relation) {
        return Err(diagnostic::input_invalid("duplicate-polymorphic"));
    }
    Ok(parsed)
}

/// Parse the declared migration history; declared (chronological)
/// order is behavioral and preserved.
fn migrations(array: &[Json]) -> Result<Vec<Migration>, DiagnosticSet> {
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let migration = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("migration-shape"))?;
        for key in migration.keys() {
            if !matches!(key.as_str(), "migrationId" | "tables" | "risk") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let migration_id = id_member(
            migration,
            "migrationId",
            "migration-shape",
            NamespacedId::parse,
        )?;
        let tables = storage_name_list(
            migration,
            "tables",
            version::MAX_MIGRATION_TABLES,
            "migration-tables",
        )?;
        if tables.is_empty() {
            return Err(diagnostic::input_invalid("migration-tables"));
        }
        let mut tables = tables;
        tables.sort();
        tables.dedup();
        let risk = DataRisk::parse(
            migration
                .get("risk")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("migration-risk"))?,
        )
        .map_err(|_| diagnostic::input_invalid("migration-risk"))?;
        parsed.push(Migration {
            migration_id,
            tables,
            risk,
        });
    }
    if has_adjacent_duplicate_by_key(&parsed, |migration| {
        migration.migration_id.as_str().to_owned()
    }) {
        return Err(diagnostic::input_invalid("duplicate-migration"));
    }
    Ok(parsed)
}

/// Whether any two adjacent entries share the same sort key.
fn has_adjacent_duplicate<T, K: PartialEq>(parsed: &[T], key: impl Fn(&T) -> &K) -> bool {
    parsed
        .windows(2)
        .any(|window| key(&window[0]) == key(&window[1]))
}

/// The same adjacency duplicate check over owned keys.
fn has_adjacent_duplicate_by_key<T>(parsed: &[T], key: impl Fn(&T) -> String) -> bool {
    parsed
        .windows(2)
        .any(|window| key(&window[0]) == key(&window[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacency_finds_exact_duplicates_only() {
        let values = vec![1, 2, 2, 3];
        assert!(has_adjacent_duplicate(&values, |value| value));
        let values = vec![1, 2, 3];
        assert!(!has_adjacent_duplicate(&values, |value| value));
    }
}
