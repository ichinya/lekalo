//! Legacy typed DTO facade for the external storage provider. No SQL renderer is embedded.
use super::super::diagnostic::MAPPING_INVALID;
use super::super::{diagnostic, provider, StorageEngineAttachment};
use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::derivation::project;
use crate::storage_projection::entity::FieldDefault;
use crate::storage_projection::id::StorageName;
use crate::storage_projection::StorageProjectionAttachment;
use serde_json::json;
/// The declared explain hook point of one statement (the hook surface
/// declared by the migration plan; the plain DDL document carries no
/// hooks).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ExplainHook {
    /// Run EXPLAIN before the statement applies.
    Before,
    /// Run EXPLAIN after the statement applies.
    After,
}

impl ExplainHook {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

/// One rendered statement with its ordinal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DdlStatement {
    pub(crate) id: usize,
    pub(crate) statement: String,
    pub(crate) explain: Option<ExplainHook>,
}

impl DdlStatement {
    /// The 1-based ordinal.
    pub const fn id(&self) -> usize {
        self.id
    }

    /// The deterministic SQL text.
    pub fn statement(&self) -> &str {
        &self.statement
    }

    /// The declared explain hook point, when declared.
    pub const fn explain(&self) -> Option<ExplainHook> {
        self.explain
    }
}

/// One finished DDL document: immutable and byte-identical for
/// value-equal inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DdlDocument {
    pub(crate) engine_version: String,
    pub(crate) statements: Vec<DdlStatement>,
}

impl DdlDocument {
    /// The pinned engine version the DDL targets.
    pub fn engine_version(&self) -> &str {
        &self.engine_version
    }

    /// The ordered statements.
    pub fn statements(&self) -> &[DdlStatement] {
        &self.statements
    }

    /// The canonical document bytes (compact JSON, byte-sorted keys,
    /// no trailing LF), or the typed over-bound refusal.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        let bytes = self.payload();
        if bytes.len() > super::super::version::MAX_CANONICAL_BYTES {
            return Err(diagnostic::export_limit_set(bytes.len()));
        }
        Ok(bytes)
    }

    fn payload(&self) -> String {
        let statements: Vec<String> = self
            .statements
            .iter()
            .map(|statement| {
                super::super::canonical::object(vec![
                    ("id", Some(statement.id.to_string())),
                    (
                        "explain",
                        statement.explain.map(|hook| format!("\"{}\"", hook.key())),
                    ),
                    (
                        "statement",
                        Some(
                            serde_json::to_string(&statement.statement)
                                .unwrap_or_else(|_| "\"\"".to_owned()),
                        ),
                    ),
                ])
            })
            .collect();
        super::super::canonical::object(vec![
            ("engine", Some("\"postgres\"".to_owned())),
            (
                "engineVersion",
                Some(
                    serde_json::to_string(&self.engine_version)
                        .unwrap_or_else(|_| "\"\"".to_owned()),
                ),
            ),
            ("statements", Some(format!("[{}]", statements.join(",")))),
        ])
    }
}

pub fn render(
    profile: &StorageEngineAttachment,
    attachment: &StorageProjectionAttachment,
) -> Result<DdlDocument, DiagnosticSet> {
    let binding = attachment.canonical_bytes()?;
    if format!("sha256:{}", crate::digest::sha256_hex(binding.as_bytes()))
        != profile.projection_ref().as_str()
    {
        return Err(diagnostic::rule_invalid(
            MAPPING_INVALID,
            "projection-binding-mismatch",
            None,
        ));
    }
    let projection = project(attachment, crate::storage_projection::Namespace::Postgres)?;
    let value = provider::call(
        "storage.ddl",
        json!({"profile":provider::profile(profile)?,"projection":provider::projection(attachment,&projection)}),
    )?;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct Document {
        engine_version: String,
        statements: Vec<Statement>,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Statement {
        id: usize,
        statement: String,
    }
    let document: Document = serde_json::from_value(value)
        .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "provider-response", None))?;
    if document.engine_version != profile.engine_version().as_str()
        || document
            .statements
            .iter()
            .enumerate()
            .any(|(i, s)| s.id != i + 1 || s.statement.is_empty())
    {
        return Err(diagnostic::rule_invalid(
            MAPPING_INVALID,
            "provider-response",
            None,
        ));
    }
    Ok(DdlDocument {
        engine_version: document.engine_version,
        statements: document
            .statements
            .into_iter()
            .map(|s| DdlStatement {
                id: s.id,
                statement: s.statement,
                explain: None,
            })
            .collect(),
    })
}
pub(crate) fn create_table(
    profile: &StorageEngineAttachment,
    attachment: &StorageProjectionAttachment,
    table: &crate::storage_projection::derivation::DerivedTable,
) -> Result<String, DiagnosticSet> {
    provider::text(
        "storage.table",
        json!({"profile":provider::profile(profile)?,"table":provider::table(attachment,table)}),
    )
}
pub(crate) fn create_join_table(
    join: &crate::storage_projection::derivation::DerivedJoin,
) -> Result<String, DiagnosticSet> {
    provider::text("storage.join", json!({"join":provider::join(join)}))
}
pub(crate) fn render_default(
    default: &FieldDefault,
    table: &StorageName,
) -> Result<String, DiagnosticSet> {
    provider::text(
        "storage.default",
        json!({"value":provider::default(default),"table":table.as_str()}),
    )
}
pub(crate) fn sequence_default(
    table: &StorageName,
    column: &StorageName,
) -> Result<String, DiagnosticSet> {
    provider::text(
        "storage.sequence",
        json!({"table":table.as_str(),"column":column.as_str()}),
    )
}
pub(crate) fn derived_index_name(
    table: &StorageName,
    index: &crate::storage_projection::Index,
) -> Result<StorageName, DiagnosticSet> {
    let name = provider::text(
        "storage.index-name",
        json!({"table":table.as_str(),"index":provider::index(index)}),
    )?;
    StorageName::parse(&name)
        .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "provider-identifier", None))
}
/// Resolve the referenced table name and primary key of one join
/// side: the join's relation names its owner and target entities, and
/// their derived tables carry the deterministic targets.
pub(crate) fn referenced_join_target(
    attachment: &StorageProjectionAttachment,
    projection: &crate::storage_projection::derivation::DerivedProjection,
    join: &crate::storage_projection::derivation::DerivedJoin,
    owner_side: bool,
) -> Option<(StorageName, StorageName)> {
    let relation = attachment
        .relations()
        .iter()
        .find(|relation| relation.relation_id() == join.relation())?;
    let entity = if owner_side {
        relation.owner()
    } else {
        relation.target()
    };
    let table = projection.table(entity)?;
    Some((table.table().clone(), table.primary_key().first()?.clone()))
}

/// The declared enum members of one derived table's field-origin
/// columns, keyed by column name (the field and column grammars
/// coincide). Empty when the entity declares no enum fields.
pub(crate) fn declared_enum_members(
    attachment: &StorageProjectionAttachment,
    table: &crate::storage_projection::derivation::DerivedTable,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut members = std::collections::BTreeMap::new();
    let Some(declared) = attachment
        .projections()
        .iter()
        .find(|declared| declared.namespace().key() == "postgres")
        .and_then(|declared| {
            declared
                .tables()
                .iter()
                .find(|declared| declared.entity() == table.entity())
        })
    else {
        return members;
    };
    let Some(entity) = attachment.entity(declared.entity()) else {
        return members;
    };
    for field in entity.fields() {
        if let crate::storage_projection::entity::DomainType::Enum {
            members: field_members,
        } = field.field_type()
        {
            members.insert(field.name().as_str().to_owned(), field_members.clone());
        }
    }
    members
}

/// The exact storage type of one derived column under the profile's
/// declared policies: field-origin columns render through the policy
/// type table (the `json` and `array` policies are observable here —
/// `json:"json"` renders the textual type, `array:"json"` renders
/// `jsonb`, and `array:"unsupported"` refuses), and every policy-
/// owned column renders through its namespace table exactly as the
/// projection derivation fixed it. Empty for the opaque technical
/// spellings the type table does not own (e.g. a declared `bytea`
/// technical column).
pub(crate) fn column_storage_type(
    profile: &StorageEngineAttachment,
    attachment: &StorageProjectionAttachment,
    table: &crate::storage_projection::derivation::DerivedTable,
    column: &crate::storage_projection::derivation::DerivedColumn,
) -> Result<String, DiagnosticSet> {
    if column.origin().key() != "field" {
        return Ok(column.storage_type().to_owned());
    }
    let Some(field_type) = attachment
        .entity(table.entity())
        .and_then(|entity| {
            entity
                .fields()
                .iter()
                .find(|field| field.name().as_str() == column.name().as_str())
        })
        .map(|field| field.field_type())
    else {
        return Ok(column.storage_type().to_owned());
    };
    // The policy table's refusal propagates for every field-origin
    // column — including enums under `native_enum`: silently keeping
    // the derived varchar would emit a schema the declared policy
    // refuses to render (the planner and the DDL document must never
    // disagree on a declared-refuse path).
    super::types::map_type(field_type, profile.policies())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_engine::StorageEngineAttachment;

    fn projection_value() -> serde_json::Value {
        // The committed valid storage-projection golden, read from the
        // fixture so the DDL golden and the projection golden stay
        // bound to one declaration.
        serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/storage-projection/valid/planner-storage.json"
        ))
        .expect("fixture json")
    }

    /// One fresh profile wire value bound to the projection fixture;
    /// tests mutate members (policies, binding) before parsing.
    fn profile_value() -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": "lekalo/storage-engine/v0.4.0",
            "identity": "dev.lekalo.storage-engine@0.4.0",
            "attachmentRevision": "0.4.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "0.2.16",
                "digest": "sha256:0101010101010101010101010101010101010101010101010101010101010101"
            },
            "irRef": {
                "identity": "dev.lekalo.ir@0.2.16",
                "digest": "sha256:0202020202020202020202020202020202020202020202020202020202020202"
            },
            "projectionRef": digest_of_attachment(),
            "engine": "postgres",
            "engineVersion": "16.4.0",
            "policies": {
                "identifierQuote": "always",
                "json": "jsonb",
                "enum": "check",
                "array": "native",
                "time": {"instant": "timestamptz", "local": "forbidden"},
                "pagination": {"offset": "allowed", "cursor": "keyset"}
            },
            "tenancy": {
                "enforcement": "rls",
                "rls": {"sessionVariable": "app.tenant_id", "force": true}
            },
            "extensions": [
                {"name": "pgcrypto", "state": "required"}
            ]
        })
    }

    fn digest_of_attachment() -> String {
        let attachment =
            crate::storage_projection::StorageProjectionAttachment::from_value(&projection_value())
                .expect("valid fixture");
        let bytes = attachment.canonical_bytes().expect("canonical");
        format!("sha256:{}", crate::digest::sha256_hex(bytes.as_bytes()))
    }

    fn profile() -> StorageEngineAttachment {
        StorageEngineAttachment::from_value(&profile_value()).expect("valid profile")
    }

    fn attachment() -> crate::storage_projection::StorageProjectionAttachment {
        crate::storage_projection::StorageProjectionAttachment::from_value(&projection_value())
            .expect("valid fixture")
    }

    #[test]
    fn the_render_is_deterministic_and_ordered() {
        let profile = profile();
        let attachment = attachment();
        let document = render(&profile, &attachment).expect("renders");
        let again = render(&profile, &attachment).expect("renders");
        assert_eq!(
            document.canonical_bytes().expect("bytes"),
            again.canonical_bytes().expect("bytes"),
            "two renders are byte-identical"
        );
        let statements: Vec<&str> = document
            .statements()
            .iter()
            .map(|statement| statement.statement())
            .collect();
        // Extension first, sequences before tables, ownership after.
        assert!(statements[0].starts_with("CREATE EXTENSION IF NOT EXISTS"));
        let first_sequence = statements
            .iter()
            .enumerate()
            .find(|(_, text)| text.starts_with("CREATE SEQUENCE"))
            .expect("sequence")
            .0;
        let first_table = statements
            .iter()
            .enumerate()
            .find(|(_, text)| text.starts_with("CREATE TABLE"))
            .expect("table")
            .0;
        assert!(first_sequence < first_table, "sequences before tables");
    }

    #[test]
    fn the_rendered_statements_carry_the_declared_facts() {
        let profile = profile();
        let attachment = attachment();
        let document = render(&profile, &attachment).expect("renders");
        let joined = document
            .statements()
            .iter()
            .map(|statement| statement.statement())
            .collect::<Vec<&str>>()
            .join("\n");
        // Always-quoted identifiers, deterministic names, defaults,
        // identity, checks, partial indexes, FKs, RLS.
        assert!(joined.contains("\"task\""), "quoted table name");
        assert!(
            joined.contains("CREATE SEQUENCE \"seq_focus_session_session_no\""),
            "the declared sequence column owns a deterministic sequence"
        );
        assert!(
            !joined.contains("DEFAULT gen_random_uuid()"),
            "no uuid default is declared in the golden"
        );
        assert!(joined.contains("DEFAULT 0"), "the declared literal default");
        assert!(joined.contains("CONSTRAINT \"chk_task_window\" CHECK (\"deleted_at\" IS NULL)"));
        assert!(
            joined.contains("CONSTRAINT \"pk_task\" PRIMARY KEY (\"id\")"),
            "the primary key is created under its deterministic name"
        );
        assert!(
            joined.contains(
                "CONSTRAINT \"chk_tag_color\" CHECK (\"color\" IN ('blue', 'green', 'red'))"
            ),
            "the check enum policy renders the bounded member list"
        );
        assert!(
            joined.contains("WHERE \"deleted_at\" IS NULL;"),
            "partial index"
        );
        assert!(
            joined.contains("CREATE UNIQUE INDEX \"idx_task_detail_task_id_uq\""),
            "the derived one-to-one unique index name is deterministic"
        );
        assert!(joined.contains("ON DELETE CASCADE"));
        assert!(joined.contains("ENABLE ROW LEVEL SECURITY"));
        assert!(joined.contains("FORCE ROW LEVEL SECURITY"));
        assert!(joined.contains("current_setting('app.tenant_id')::uuid"));
        assert!(joined.contains("CREATE SEQUENCE \"seq_focus_session_session_no\""));
        assert!(
            joined.contains(
                "\"session_no\" bigint DEFAULT nextval('seq_focus_session_session_no') NOT NULL",
            ),
            "the generated sequence column draws from its own owned sequence"
        );
        assert!(
            !joined.contains("nextval('session_no')"),
            "a sequence default never names the bare column"
        );
        assert!(joined.contains("ALTER SEQUENCE \"seq_focus_session_session_no\" OWNED BY"));
        // Every id is 1-based sequential.
        for (index, statement) in document.statements().iter().enumerate() {
            assert_eq!(statement.id(), index + 1);
            assert!(statement.explain().is_none(), "plain DDL carries no hooks");
        }
    }

    #[test]
    fn the_committed_ddl_golden_matches_byte_for_byte() {
        let profile = profile();
        let attachment = attachment();
        let document = render(&profile, &attachment).expect("renders");
        let committed =
            include_str!("../../../../../tests/fixtures/storage-engine/derived/postgres-ddl.json");
        assert_eq!(
            document.canonical_bytes().expect("bytes"),
            committed.trim_end(),
            "the committed DDL golden matches"
        );
    }

    #[test]
    fn an_over_long_derived_name_refuses_instead_of_panicking() {
        // Both the table and the column fit the storage-name grammar;
        // the composed sequence name does not. The renderer must answer
        // with the typed mapping diagnostic, never a panic.
        let mut value = projection_value();
        let table = value
            .get_mut("projections")
            .and_then(|projections| projections.as_array_mut())
            .and_then(|projections| {
                projections.iter_mut().find(|projection| {
                    projection
                        .get("namespace")
                        .and_then(serde_json::Value::as_str)
                        == Some("postgres")
                })
            })
            .and_then(|projection| projection.get_mut("tables"))
            .and_then(|tables| tables.get_mut(0))
            .expect("projection tables");
        let long_table = format!("{}_tbl", "a".repeat(58));
        let long_column = format!("{}_col", "b".repeat(58));
        table["table"] = serde_json::Value::String(long_table.clone());
        table["generatedColumns"] = serde_json::json!([{
            "kind": "sequence",
            "name": long_column,
        }]);
        let attachment = crate::storage_projection::StorageProjectionAttachment::from_value(&value)
            .expect("valid projection");
        let mut profile_value = profile_value();
        let bytes = attachment.canonical_bytes().expect("canonical");
        profile_value["projectionRef"] = serde_json::Value::String(format!(
            "sha256:{}",
            crate::digest::sha256_hex(bytes.as_bytes())
        ));
        let profile = StorageEngineAttachment::from_value(&profile_value).expect("valid profile");
        let error = render(&profile, &attachment).expect_err("over-long composition");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.mapping-invalid")
        );
    }

    #[test]
    fn a_computed_generated_column_refuses_explicitly() {
        // The 0.4.0 generatedColumns member carries no expression, so a
        // computed column must refuse with the registered unsupported
        // rule — never render as a plain stored column.
        let mut value = projection_value();
        let table = value
            .get_mut("projections")
            .and_then(|projections| projections.as_array_mut())
            .and_then(|projections| {
                projections.iter_mut().find(|projection| {
                    projection
                        .get("namespace")
                        .and_then(serde_json::Value::as_str)
                        == Some("postgres")
                })
            })
            .and_then(|projection| projection.get_mut("tables"))
            .and_then(|tables| tables.as_array_mut())
            .and_then(|tables| {
                tables.iter_mut().find(|table| {
                    table.get("table").and_then(serde_json::Value::as_str) == Some("task")
                })
            })
            .expect("projection table");
        table["generatedColumns"] = serde_json::json!([{
            "kind": "computed",
            "name": "total",
        }]);
        let attachment = crate::storage_projection::StorageProjectionAttachment::from_value(&value)
            .expect("valid projection");
        let mut profile_value = profile_value();
        let bytes = attachment.canonical_bytes().expect("canonical");
        profile_value["projectionRef"] = serde_json::Value::String(format!(
            "sha256:{}",
            crate::digest::sha256_hex(bytes.as_bytes())
        ));
        let profile = StorageEngineAttachment::from_value(&profile_value).expect("valid profile");
        let error = render(&profile, &attachment).expect_err("computed refuses");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.render-unsupported")
        );
    }

    #[test]
    fn the_native_enum_policy_refuses_explicitly() {
        // The declared-but-unimplemented native_enum policy refuses the
        // render with the registered unsupported rule — never a silent
        // varchar coercion.
        let mut value = profile_value();
        value["policies"]["enum"] = serde_json::Value::String("native_enum".to_owned());
        let profile = StorageEngineAttachment::from_value(&value).expect("valid profile");
        let error = render(&profile, &attachment()).expect_err("native_enum refuses");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.render-unsupported")
        );
    }

    #[test]
    fn the_array_policy_gates_the_rendered_column_type() {
        // The array policy is observable in the emitted DDL: `json`
        // renders jsonb, `unsupported` refuses with the registered
        // rule — never a native array under a refusing policy.
        let mut json_value = profile_value();
        json_value["policies"]["array"] = serde_json::Value::String("json".to_owned());
        let json_profile = StorageEngineAttachment::from_value(&json_value).expect("valid profile");
        let document = render(&json_profile, &attachment()).expect("renders");
        let joined = document
            .statements()
            .iter()
            .map(|statement| statement.statement())
            .collect::<Vec<&str>>()
            .join("\n");
        assert!(
            joined.contains("CREATE TABLE \"task_roster\" (\"id\" uuid NOT NULL, \"members\" jsonb, CONSTRAINT \"pk_task_roster\" PRIMARY KEY (\"id\"))"),
            "the json array policy renders jsonb"
        );
        assert!(!joined.contains("[]"), "no native array survives");

        let mut unsup_value = profile_value();
        unsup_value["policies"]["array"] = serde_json::Value::String("unsupported".to_owned());
        let unsup_profile =
            StorageEngineAttachment::from_value(&unsup_value).expect("valid profile");
        let error = render(&unsup_profile, &attachment()).expect_err("array unsupported refuses");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.render-unsupported")
        );
    }

    #[test]
    fn a_foreign_binding_refuses() {
        let mut value = serde_json::json!({
            "schemaVersion": "lekalo/storage-engine/v0.4.0",
            "identity": "dev.lekalo.storage-engine@0.4.0",
            "attachmentRevision": "0.4.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "0.2.16",
                "digest": "sha256:0101010101010101010101010101010101010101010101010101010101010101"
            },
            "irRef": {
                "identity": "dev.lekalo.ir@0.2.16",
                "digest": "sha256:0202020202020202020202020202020202020202020202020202020202020202"
            },
            "projectionRef": "sha256:0404040404040404040404040404040404040404040404040404040404040404",
            "engine": "postgres",
            "engineVersion": "16.4.0",
            "policies": {
                "identifierQuote": "always",
                "json": "jsonb",
                "enum": "check",
                "array": "native",
                "time": {"instant": "timestamptz", "local": "forbidden"},
                "pagination": {"offset": "allowed", "cursor": "keyset"}
            }
        });
        let _ = &mut value;
        let profile = StorageEngineAttachment::from_value(&value).expect("valid profile");
        let error = render(&profile, &attachment()).expect_err("binding mismatch");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.mapping-invalid")
        );
    }
}
