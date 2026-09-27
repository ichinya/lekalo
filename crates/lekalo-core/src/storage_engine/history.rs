//! The validated storage rename history (issue #57).
//!
//! [`StorageRenameMap`] carries the owner-reviewed physical renames of
//! one base→candidate transition: which table or column changed its
//! stored name and where the evidence comes from. Renaming a Model
//! symbol never renames a table; a physical rename exists only when
//! this document declares it, and the planner applies it only after
//! [`validate_rename_history`] proved the map consistent with the two
//! attachments — the old name exists in the base, the new name is
//! unoccupied in the candidate, chains and cycles refuse, and the
//! project/digest pins match exactly. Without validated evidence a
//! same-entity name change stays a destructive drop+add pair.

use serde::Serialize;

use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::{id::EntityKey, id::StorageName, Namespace, StorageProjectionAttachment};

use super::diagnostic::{self, MAPPING_INVALID};

/// The contract identity this history document speaks.
pub const IDENTITY: &str = "dev.lekalo.storage-rename-history@0.4.0";
/// The contract schema token this history document speaks.
pub const SCHEMA_VERSION: &str = "lekalo/storage-rename-history/v0.4.0";

/// One declared physical rename.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct StorageRename {
    /// The stable entity key both names belong to.
    pub entity: String,
    /// The closed rename target: a table or a column of one table.
    pub kind: StorageRenameKind,
    /// The physical name in the base projection.
    pub from: String,
    /// The physical name in the candidate projection.
    pub to: String,
    /// The semantic-history reference the review cites (a stable
    /// symbol id, a registry entry, or a reviewed ticket).
    #[serde(rename = "historyRef")]
    pub history_ref: String,
}

impl StorageRename {
    /// The canonical one-line material of this entry (the digest
    /// binding; deliberately excludes nothing — the whole entry is
    /// identity).
    fn material(&self) -> String {
        format!(
            "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{2}",
            self.entity, self.kind.key(), self.from, self.to, self.history_ref
        )
    }
}

/// The closed rename target.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub enum StorageRenameKind {
    /// A table (or join table) rename.
    Table,
    /// A column rename inside one table.
    Column,
}

impl StorageRenameKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Column => "column",
        }
    }
}

/// The declared rename history of one base→candidate transition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StorageRenameMap {
    pub(crate) project_id: String,
    pub(crate) base_digest: String,
    pub(crate) candidate_digest: String,
    pub(crate) renames: Vec<StorageRename>,
}

impl StorageRenameMap {
    /// The stable project identity the map is bound to.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// The pinned base attachment digest (`sha256:…`).
    pub fn base_digest(&self) -> &str {
        &self.base_digest
    }

    /// The pinned candidate attachment digest (`sha256:…`).
    pub fn candidate_digest(&self) -> &str {
        &self.candidate_digest
    }

    /// The declared renames in canonical byte-sorted order.
    pub fn renames(&self) -> &[StorageRename] {
        &self.renames
    }

    /// Parse the history document from its JSON value: closed shape,
    /// sorted renames, and canonical digest pins. Pure.
    pub fn from_value(value: &serde_json::Value) -> Result<Self, DiagnosticSet> {
        let object = value.as_object().ok_or_else(|| refusal("shape"))?;
        if object.get("schemaVersion").and_then(|v| v.as_str()) != Some(SCHEMA_VERSION) {
            return Err(refusal("schema-version"));
        }
        if object.get("identity").and_then(|v| v.as_str()) != Some(IDENTITY) {
            return Err(refusal("identity"));
        }
        let project_id = object
            .get("projectId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| refusal("project-id"))?
            .to_owned();
        let base_digest = digest_member(object, "baseDigest")?;
        let candidate_digest = digest_member(object, "candidateDigest")?;
        let mut renames = Vec::new();
        for entry in object
            .get("renames")
            .and_then(|v| v.as_array())
            .ok_or_else(|| refusal("renames"))?
        {
            let entry = entry.as_object().ok_or_else(|| refusal("rename-shape"))?;
            let known = ["entity", "kind", "from", "to", "historyRef"];
            if entry.keys().any(|key| !known.contains(&key.as_str())) {
                return Err(refusal("rename-member"));
            }
            let entity = entry
                .get("entity")
                .and_then(|v| v.as_str())
                .ok_or_else(|| refusal("rename-entity"))?;
            let kind = match entry.get("kind").and_then(|v| v.as_str()) {
                Some("table") => StorageRenameKind::Table,
                Some("column") => StorageRenameKind::Column,
                _ => return Err(refusal("rename-kind")),
            };
            let from = name_member(entry, "from")?;
            let to = name_member(entry, "to")?;
            let history_ref = entry
                .get("historyRef")
                .and_then(|v| v.as_str())
                .ok_or_else(|| refusal("rename-history-ref"))?;
            if history_ref.is_empty() || history_ref.len() > 256 {
                return Err(refusal("rename-history-ref"));
            }
            renames.push(StorageRename {
                entity: entity.to_owned(),
                kind,
                from,
                to,
                history_ref: history_ref.to_owned(),
            });
        }
        renames.sort();
        Ok(Self {
            project_id,
            base_digest,
            candidate_digest,
            renames,
        })
    }

    /// The canonical history bytes (compact JSON, byte-sorted keys),
    /// or the typed over-bound refusal.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        let bytes = self.payload();
        if bytes.len() > super::version::MAX_CANONICAL_BYTES {
            return Err(diagnostic::export_limit_set(bytes.len()));
        }
        Ok(bytes)
    }

    fn payload(&self) -> String {
        use super::canonical::{array, object, string};
        let renames: Vec<String> = self
            .renames
            .iter()
            .map(|rename| {
                object(vec![
                    ("entity", Some(string(&rename.entity))),
                    ("kind", Some(string(rename.kind.key()))),
                    ("from", Some(string(&rename.from))),
                    ("to", Some(string(&rename.to))),
                    ("historyRef", Some(string(&rename.history_ref))),
                ])
            })
            .collect();
        object(vec![
            ("schemaVersion", Some(string(SCHEMA_VERSION))),
            ("identity", Some(string(IDENTITY))),
            ("projectId", Some(string(&self.project_id))),
            ("baseDigest", Some(string(&self.base_digest))),
            ("candidateDigest", Some(string(&self.candidate_digest))),
            ("renames", Some(array(&renames))),
        ])
    }

    /// The canonical digest of this map (the bytes the planner pins).
    pub fn digest(&self) -> Result<String, DiagnosticSet> {
        let bytes = self.canonical_bytes()?;
        Ok(format!(
            "sha256:{}",
            crate::digest::sha256_hex(bytes.as_bytes())
        ))
    }
}

/// The digest member must be an exact `sha256:<64 hex>` spelling.
fn digest_member(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Result<String, DiagnosticSet> {
    let value = object
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| refusal(key))?;
    let rest = value
        .strip_prefix("sha256:")
        .ok_or_else(|| refusal(key))?;
    if rest.len() != 64 || !rest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(refusal(key));
    }
    Ok(value.to_owned())
}

/// The physical-name member must parse under the storage-name grammar.
fn name_member(entry: &serde_json::Map<String, serde_json::Value>, key: &str) -> Result<String, DiagnosticSet> {
    let value = entry
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| refusal(key))?;
    crate::storage_projection::id::StorageName::parse(value)
        .map(|_| value.to_owned())
        .map_err(|_| refusal(key))
}

/// The single typed refusal this family raises.
fn refusal(code: &str) -> DiagnosticSet {
    diagnostic::rule_invalid(MAPPING_INVALID, code, None)
}

/// Prove the map consistent with its two attachments: the project
/// pins, both digests, the existence of every old name in the base,
/// the vacancy of every new name in the candidate, and acyclicity.
/// On success the renames are safe to hand to the planner. Pure and
/// read-only.
pub fn validate_rename_history(
    map: &StorageRenameMap,
    base: &StorageProjectionAttachment,
    candidate: &StorageProjectionAttachment,
) -> Result<(), DiagnosticSet> {
    if map.project_id() != base.project_id().as_str()
        || map.project_id() != candidate.project_id().as_str()
    {
        return Err(refusal("project-mismatch"));
    }
    let base_digest = attachment_digest(base)?;
    let candidate_digest = attachment_digest(candidate)?;
    if map.base_digest() != base_digest {
        return Err(refusal("base-digest-mismatch"));
    }
    if map.candidate_digest() != candidate_digest {
        return Err(refusal("candidate-digest-mismatch"));
    }
    // Each entry must match both projections: the old name exists in
    // the base under the same entity, the new name occupies the
    // candidate under it, and both directions hold per kind.
    let mut table_renames: Vec<(&str, &str, &str)> = Vec::new();
    for rename in map.renames() {
        let entity = rename.entity.as_str();
        let base_table = declared_table(base, entity);
        let candidate_table = declared_table(candidate, entity);
        let (base_table, candidate_table) = match (base_table, candidate_table) {
            (Some(base_table), Some(candidate_table)) => (base_table, candidate_table),
            _ => return Err(refusal("rename-entity-unmapped")),
        };
        match rename.kind {
            StorageRenameKind::Table => {
                if base_table.as_str() != rename.from || candidate_table.as_str() != rename.to {
                    return Err(refusal("rename-table-name"));
                }
                table_renames.push((&rename.entity, rename.from.as_str(), rename.to.as_str()));
            }
            StorageRenameKind::Column => {
                let base_column = declared_columns(base, base_table)?
                    .into_iter()
                    .find(|column| *column == rename.from);
                let candidate_column = declared_columns(candidate, candidate_table)?
                    .into_iter()
                    .find(|column| *column == rename.to);
                if base_column.is_none() || candidate_column.is_none() {
                    return Err(refusal("rename-column-name"));
                }
            }
        }
    }
    // Table-rename chains and cycles refuse: two entries may not send
    // one table two ways, and no rename's target may be another
    // rename's source within one transition.
    let sources: Vec<&str> = table_renames.iter().map(|(_, from, _)| *from).collect();
    let targets: Vec<&str> = table_renames.iter().map(|(_, _, to)| *to).collect();
    if sources
        .iter()
        .any(|source| sources.iter().filter(|other| *other == source).count() > 1)
    {
        return Err(refusal("rename-conflict"));
    }
    if targets
        .iter()
        .any(|target| sources.contains(target))
    {
        return Err(refusal("rename-cycle"));
    }
    // Column renames hold the same one-way discipline: one entry per
    // source column, one entry per target name, and a target the base
    // table does not already occupy — a surviving column of the target
    // name would collide (42710) the moment the swap applied, and an
    // identity entry is not a rename at all.
    let mut column_sources: Vec<(&str, &str)> = Vec::new();
    let mut column_targets: Vec<(&str, &str)> = Vec::new();
    for rename in map.renames() {
        if rename.kind != StorageRenameKind::Column {
            continue;
        }
        let entity = rename.entity.as_str();
        let base_table = declared_table(base, entity)
            .ok_or_else(|| refusal("rename-entity-unmapped"))?;
        if rename.from == rename.to
            || declared_columns(base, base_table)?
                .iter()
                .any(|column| *column == rename.to)
        {
            return Err(refusal("rename-column-name"));
        }
        if column_sources.contains(&(entity, rename.from.as_str()))
            || column_targets.contains(&(entity, rename.to.as_str()))
        {
            return Err(refusal("rename-conflict"));
        }
        column_sources.push((entity, rename.from.as_str()));
        column_targets.push((entity, rename.to.as_str()));
    }
    Ok(())
}

/// The canonical attachment digest the pins carry.
fn attachment_digest(
    attachment: &StorageProjectionAttachment,
) -> Result<String, DiagnosticSet> {
    let bytes = attachment.canonical_bytes()?;
    Ok(format!(
        "sha256:{}",
        crate::digest::sha256_hex(bytes.as_bytes())
    ))
}

/// The physical table name one entity maps to in the declared
/// postgres projection, or nothing when the entity is unmapped.
fn declared_table<'a>(
    attachment: &'a StorageProjectionAttachment,
    entity_key: &str,
) -> Option<&'a StorageName> {
    let entity_key = EntityKey::parse(entity_key).ok()?;    attachment
        .projection(Namespace::Postgres)?
        .tables()
        .iter()
        .find(|table| table.entity() == &entity_key)
        .map(|table| table.table())
}

/// The physical column names (field, technical, generated, timestamp,
/// soft-delete, and tenant names) of one declared table. Column
/// renames are proved against these declared facts; the physical name
/// grammar is namespace-independent.
fn declared_columns(
    attachment: &StorageProjectionAttachment,
    table: &StorageName,
) -> Result<Vec<String>, DiagnosticSet> {
    let projection = attachment
        .projection(Namespace::Postgres)
        .ok_or_else(|| refusal("namespace-unmapped"))?;
    let declared = projection
        .tables()
        .iter()
        .find(|declared| declared.table() == table)
        .ok_or_else(|| refusal("rename-table-unmapped"))?;
    // The declared field columns live on the domain entity; the
    // physical names come from the entity's field list mapped through
    // the storage name grammar.
    let mut names: Vec<String> = declared_table_field_columns(attachment, declared);
    names.extend(
        declared
            .technical_columns()
            .iter()
            .map(|column| column.name().as_str().to_owned()),
    );
    names.extend(
        declared
            .generated_columns()
            .iter()
            .map(|column| column.name().as_str().to_owned()),
    );
    if let Some((created_at, updated_at)) = declared.timestamps() {
        names.push(created_at.as_str().to_owned());
        names.push(updated_at.as_str().to_owned());
    }
    if let Some(soft_delete) = declared.soft_delete() {
        names.push(soft_delete.as_str().to_owned());
    }
    if let Some((tenant, _)) = declared.tenant_key() {
        names.push(tenant.as_str().to_owned());
    }
    Ok(names)
}

/// The physical field-column names of one declared table: the mapped
/// entity's declared field names, which the projection maps 1:1 onto
/// columns under the shared snake_case grammar.
fn declared_table_field_columns(
    attachment: &StorageProjectionAttachment,
    table: &crate::storage_projection::projection::Table,
) -> Vec<String> {
    attachment
        .entity(table.entity())
        .map(|entity| {
            entity
                .fields()
                .iter()
                .filter_map(|field| {
                    StorageName::parse(field.name().as_str())
                        .ok()
                        .map(|parsed| parsed.as_str().to_owned())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The history digest material the planner mixes into its plan id:
/// one canonical line per validated rename, byte-sorted.
pub(crate) fn history_material(map: &StorageRenameMap) -> String {
    let mut material = map
        .renames()
        .iter()
        .map(StorageRename::material)
        .collect::<Vec<String>>()
        .join("");
    if let Ok(digest) = map.digest() {
        material.push_str(&digest);
    }
    material
}

/// The canonical digest of one rename map, as the plan pins it.
pub(crate) fn history_digest_of(map: &StorageRenameMap) -> Result<String, DiagnosticSet> {
    map.digest()
}



#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(path: &str) -> StorageProjectionAttachment {
        let bytes = std::fs::read(path).unwrap_or_else(|_| panic!("fixture {path}"));
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        StorageProjectionAttachment::from_value(&value).expect("valid")
    }

    #[test]
    fn a_valid_map_round_trips_and_pins_its_digests() {
        // The committed rename fixture keeps the physical table `task`
        // and renames only the domain symbol — exactly the no-op a
        // symbol rename must stay for storage. A column rename must
        // therefore also refuse here (title survives in both), so this
        // test pins the map's canonical bytes and digest only.
        let base = attachment("../../tests/fixtures/storage-engine/migration/base.json");
        let candidate =
            attachment("../../tests/fixtures/storage-engine/migration/candidate-additive.json");
        let base_digest = attachment_digest(&base).expect("digest");
        let candidate_digest = attachment_digest(&candidate).expect("digest");
        let map = StorageRenameMap {
            project_id: base.project_id().as_str().to_owned(),
            base_digest: base_digest.clone(),
            candidate_digest: candidate_digest.clone(),
            renames: Vec::new(),
        };
        validate_rename_history(&map, &base, &candidate).expect("valid");
        let bytes = map.canonical_bytes().expect("canonical");
        let parsed: serde_json::Value = serde_json::from_str(&bytes).expect("json");
        assert_eq!(parsed["schemaVersion"], SCHEMA_VERSION);
        assert_eq!(
            map.digest().expect("digest"),
            format!("sha256:{}", crate::digest::sha256_hex(bytes.as_bytes()))
        );
    }

    #[test]
    fn a_stale_base_digest_refuses() {
        let base = attachment("../../tests/fixtures/storage-engine/migration/base.json");
        let candidate =
            attachment("../../tests/fixtures/storage-engine/migration/candidate-additive.json");
        let map = StorageRenameMap {
            project_id: base.project_id().as_str().to_owned(),
            base_digest: "sha256:0606060606060606060606060606060606060606060606060606060606060606".to_owned(),
            candidate_digest: attachment_digest(&candidate).expect("digest"),
            renames: Vec::new(),
        };
        let error = validate_rename_history(&map, &base, &candidate).expect_err("refused");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.mapping-invalid")
        );
    }

    #[test]
    fn an_unproven_column_rename_refuses() {
        let base = attachment("../../tests/fixtures/storage-engine/migration/base.json");
        let candidate =
            attachment("../../tests/fixtures/storage-engine/migration/candidate-additive.json");
        let map = StorageRenameMap {
            project_id: base.project_id().as_str().to_owned(),
            base_digest: "sha256:bd66d2a7c62819bec13470f3160b335713cb73ed7795fd8af757fea5dd19af9e".to_owned(),
            candidate_digest: "sha256:7b6c78a1ae83ae754843bb6b13e7072e9fac9e0e4b8bcde83e9dff226b5fb7f1".to_owned(),
            renames: vec![StorageRename {
                entity: "task".to_owned(),
                kind: StorageRenameKind::Column,
                from: "title".to_owned(),
                to: "nonexistent".to_owned(),
                history_ref: "planner.focus_task.title".to_owned(),
            }],
        };
        assert!(validate_rename_history(&map, &base, &candidate).is_err());
    }

    #[test]
    fn an_occupied_or_identity_column_rename_refuses() {
        // A rename whose target already exists in the base would
        // collide (42710) the moment the swap applied, and an identity
        // entry is not a rename at all — both refuse instead of
        // planning an inapplicable statement.
        let base = attachment("../../tests/fixtures/storage-engine/migration/base.json");
        let candidate =
            attachment("../../tests/fixtures/storage-engine/migration/candidate-additive.json");
        let digest = |attachment: &StorageProjectionAttachment| {
            format!(
                "sha256:{}",
                crate::digest::sha256_hex(
                    attachment.canonical_bytes().expect("bytes").as_bytes()
                )
            )
        };
        let rename = |from: &str, to: &str| StorageRenameMap {
            project_id: base.project_id().as_str().to_owned(),
            base_digest: digest(&base),
            candidate_digest: digest(&candidate),
            renames: vec![StorageRename {
                entity: "task".to_owned(),
                kind: StorageRenameKind::Column,
                from: from.to_owned(),
                to: to.to_owned(),
                history_ref: "planner.focus_task.title".to_owned(),
            }],
        };
        // candidate-additive keeps `title` occupied in the base: a
        // rename onto it collides with the surviving column.
        assert!(validate_rename_history(&rename("note", "title"), &base, &candidate).is_err());
        assert!(validate_rename_history(&rename("title", "title"), &base, &candidate).is_err());
        // A doubled source and a doubled target each conflict.
        let doubled = |second: StorageRename| {
            let mut map = rename("note", "summary_note");
            map.renames.push(second);
            map
        };
        let source = StorageRename {
            entity: "task".to_owned(),
            kind: StorageRenameKind::Column,
            from: "note".to_owned(),
            to: "other_note".to_owned(),
            history_ref: "planner.focus_task.note".to_owned(),
        };
        let target = StorageRename {
            entity: "task".to_owned(),
            kind: StorageRenameKind::Column,
            from: "due_date".to_owned(),
            to: "summary_note".to_owned(),
            history_ref: "planner.focus_task.due_date".to_owned(),
        };
        assert!(validate_rename_history(&doubled(source), &base, &candidate).is_err());
        assert!(validate_rename_history(&doubled(target), &base, &candidate).is_err());
    }
}
