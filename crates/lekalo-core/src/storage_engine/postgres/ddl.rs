//! The deterministic PostgreSQL DDL renderer (issue #69).
//!
//! [`render`] turns one validated engine profile plus its bound
//! storage-projection attachment into the immutable, ordered DDL
//! document: required extensions, owned sequences, tables with keys,
//! defaults, and CHECK constraints, foreign keys, deterministic named
//! indexes (partial where declared), and the declared RLS policy.
//! Every statement is deterministic: identifiers are always quoted,
//! unnamed indexes derive their names from the table and columns, and
//! the emitted name is always present in the document — the engine
//! never invents a name. Each statement carries its ordinal; the
//! explain hook points ride the migration plan (plan step S7). Core
//! never executes any of it.

use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::derivation::project;
use crate::storage_projection::entity::{FieldDefault, Literal};
use crate::storage_projection::id::StorageName;
use crate::storage_projection::projection::{ColumnPredicate, GeneratedKind, PredicateOp};
use crate::storage_projection::StorageProjectionAttachment;

use super::super::diagnostic::{self, MAPPING_INVALID, RENDER_UNSUPPORTED};
use super::super::{EnumPolicy, StorageEngineAttachment};
use super::quoting::quote;

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

/// Render the deterministic DDL document for one storage-projection
/// attachment under one engine profile. The projection must declare
/// the postgres namespace and its canonical digest must equal the
/// profile's `projectionRef` binding. Pure and read-only.
pub fn render(
    profile: &StorageEngineAttachment,
    attachment: &StorageProjectionAttachment,
) -> Result<DdlDocument, DiagnosticSet> {
    let binding = attachment.canonical_bytes()?;
    let digest = crate::digest::sha256_hex(binding.as_bytes());
    if format!("sha256:{digest}") != profile.projection_ref().as_str() {
        return Err(diagnostic::rule_invalid(
            MAPPING_INVALID,
            "projection-binding-mismatch",
            None,
        ));
    }
    let projection = project(attachment, crate::storage_projection::Namespace::Postgres)?;
    // The primary key of every derived table, by table name: the FK
    // target column of one deterministic rendering.
    let primary_keys: std::collections::BTreeMap<&str, &StorageName> = projection
        .tables()
        .iter()
        .filter_map(|table| {
            table
                .primary_key()
                .first()
                .map(|pk| (table.table().as_str(), pk))
        })
        .collect();
    let mut statements: Vec<String> = Vec::new();
    // Required extensions first: nothing else may reference them.
    for extension in profile.extensions() {
        if extension.required() {
            let name = StorageName::parse(extension.name().as_str())
                .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "extension-name", None))?;
            statements.push(format!("CREATE EXTENSION IF NOT EXISTS {};", quote(&name)));
        }
    }
    // Owned sequences exist before the tables that default to them.
    let mut sequence_owners: Vec<(StorageName, String)> = Vec::new();
    for table in projection.tables() {
        for column in table.columns() {
            if column.generated_kind() == Some(GeneratedKind::Sequence) {
                let sequence = derived_name(
                    &format!("seq_{}_{}", table.table(), column.name()),
                    "sequence-name",
                )?;
                statements.push(format!("CREATE SEQUENCE {};", quote(&sequence)));
                sequence_owners.push((
                    sequence,
                    format!("{}.{}", quote(table.table()), quote(column.name())),
                ));
            }
        }
    }
    // Tables: columns with declared defaults, primary key, and the
    // declared CHECK constraints. A computed generated column refuses:
    // the 0.4.0 member carries no expression and inventing one would
    // violate the nothing-is-invented boundary.
    for table in projection.tables() {
        statements.push(create_table(profile, attachment, table)?);
    }
    // Join tables materialize their many-to-many relations with the
    // same column rendering; a unique pair is the primary key.
    for join in projection.joins() {
        let mut lines: Vec<String> = Vec::new();
        for column in join.columns() {
            lines.push(format!(
                "{} {} NOT NULL",
                quote(column.name()),
                column.storage_type()
            ));
        }
        if join.unique_pair() {
            let pair = join
                .columns()
                .iter()
                .map(|column| quote(column.name()))
                .collect::<Vec<String>>()
                .join(", ");
            lines.push(format!("PRIMARY KEY ({pair})"));
        }
        statements.push(format!(
            "CREATE TABLE {} ({});",
            quote(join.table()),
            lines.join(", ")
        ));
    }
    // Foreign keys, after their referenced tables.
    for table in projection.tables() {
        for foreign_key in table.foreign_keys() {
            let referenced_column = primary_keys
                .get(foreign_key.references_table().as_str())
                .copied()
                .ok_or_else(|| {
                    diagnostic::rule_invalid(MAPPING_INVALID, "referenced-key-absent", None)
                })?;
            let name = derived_name(
                &format!("fk_{}_{}", table.table(), foreign_key.column()),
                "foreign-key-name",
            )?;
            statements.push(format!(
                "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {};",
                quote(table.table()),
                quote(&name),
                quote(foreign_key.column()),
                quote(foreign_key.references_table()),
                quote(referenced_column),
                foreign_key.on_delete().key(),
            ));
        }
    }
    // Join-table foreign keys, one per join column, with the declared
    // delete actions of the materialized relation.
    for join in projection.joins() {
        for (index, action) in [
            (0usize, join.on_owner_delete()),
            (1usize, join.on_target_delete()),
        ] {
            let column = &join.columns()[index];
            let owner_side = index == 0;
            let Some((table_name, pk)) =
                referenced_join_target(attachment, &projection, join, owner_side)
            else {
                return Err(diagnostic::rule_invalid(
                    MAPPING_INVALID,
                    "join-reference-unresolved",
                    None,
                ));
            };
            let name = derived_name(
                &format!("fk_{}_{}", join.table(), column.name()),
                "foreign-key-name",
            )?;
            statements.push(format!(
                "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {};",
                quote(join.table()),
                quote(&name),
                quote(column.name()),
                quote(&table_name),
                quote(&pk),
                action.key(),
            ));
        }
    }
    // Indexes: declared and derived, with deterministic names for the
    // unnamed ones and the partial predicate where declared.
    for table in projection.tables() {
        for index in table.indexes() {
            let name = match index.name() {
                Some(name) => name.clone(),
                None => derived_index_name(table.table(), index)?,
            };
            let unique = if index.unique() { "UNIQUE " } else { "" };
            let predicate = match index.where_() {
                Some(predicates) => format!(" WHERE {}", render_predicates(predicates)),
                None => String::new(),
            };
            let columns = index
                .columns()
                .iter()
                .map(quote)
                .collect::<Vec<String>>()
                .join(", ");
            statements.push(format!(
                "CREATE {unique}INDEX {} ON {} ({}){predicate};",
                quote(&name),
                quote(table.table()),
                columns
            ));
        }
    }
    // Sequence ownership after the tables exist.
    for (sequence, owner) in &sequence_owners {
        statements.push(format!(
            "ALTER SEQUENCE {} OWNED BY {owner};",
            quote(sequence)
        ));
    }
    // The declared RLS policy: explicit, never implied by the tenant
    // key column alone; every covered table carries its tenant key.
    if let Some(tenancy) = profile.tenancy() {
        if tenancy.enforcement() == super::super::Enforcement::Rls {
            let rls = tenancy.rls().ok_or_else(|| {
                diagnostic::rule_invalid(MAPPING_INVALID, "rls-policy-missing", None)
            })?;
            for table in projection.tables() {
                // Tables without a declared tenant key are not
                // covered by the row-level policy; the policy is
                // explicit per covered table, never implied.
                let Some(tenant) = table
                    .columns()
                    .iter()
                    .find(|column| column.origin().key() == "tenant_key")
                else {
                    continue;
                };
                statements.push(format!(
                    "ALTER TABLE {} ENABLE ROW LEVEL SECURITY;",
                    quote(table.table())
                ));
                if rls.force() {
                    statements.push(format!(
                        "ALTER TABLE {} FORCE ROW LEVEL SECURITY;",
                        quote(table.table())
                    ));
                }
                let policy_name =
                    derived_name(&format!("pol_{}_tenant", table.table()), "policy-name")?;
                statements.push(format!(
                    "CREATE POLICY {} ON {} USING ({} = current_setting('{}')::{});",
                    quote(&policy_name),
                    quote(table.table()),
                    quote(tenant.name()),
                    rls.session_variable().as_str(),
                    tenant.storage_type(),
                ));
            }
        }
    }
    let rendered = statements
        .into_iter()
        .enumerate()
        .map(|(index, statement)| DdlStatement {
            id: index + 1,
            statement,
            explain: None,
        })
        .collect();
    Ok(DdlDocument {
        engine_version: profile.engine_version().as_str().to_owned(),
        statements: rendered,
    })
}

/// Resolve the referenced table name and primary key of one join
/// side: the join's relation names its owner and target entities, and
/// their derived tables carry the deterministic targets.
fn referenced_join_target(
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

/// Render one CREATE TABLE statement. The enum policy is observable:
/// `check` (the default) renders one bounded member-list CHECK per
/// enum column (`chk_<table>_<column>`, the deterministic name the
/// migration planner and the drift comparison reuse); `native_enum`
/// refuses explicitly — the 0.4.0 renderer creates no enum types, and
/// silently yielding a varchar would violate the nothing-is-invented
/// boundary. The migration planner reuses this renderer for its
/// create_table steps so both emitters describe one schema.
pub(crate) fn create_table(
    profile: &StorageEngineAttachment,
    attachment: &StorageProjectionAttachment,
    table: &crate::storage_projection::derivation::DerivedTable,
) -> Result<String, DiagnosticSet> {
    let enum_members = declared_enum_members(attachment, table);
    if !enum_members.is_empty() && profile.policies().enum_policy() == EnumPolicy::NativeEnum {
        return Err(diagnostic::rule_invalid(
            RENDER_UNSUPPORTED,
            "native-enum",
            None,
        ));
    }
    let mut lines: Vec<String> = Vec::new();
    for column in table.columns() {
        // The policy table owns the field-origin type spelling: the
        // `json` and `array` policies are observable here, exactly as
        // the conformance battery answers them.
        let storage_type = column_storage_type(profile, attachment, table, column)?;
        let mut line = format!("{} {}", quote(column.name()), storage_type);
        if column.generated_kind() == Some(GeneratedKind::Identity) {
            line.push_str(" GENERATED ALWAYS AS IDENTITY");
        } else if column.generated_kind() == Some(GeneratedKind::Computed) {
            // A computed generated column refuses: the 0.4.0 member
            // carries no expression, and rendering a plain stored
            // column would invent a fact the declaration never made.
            return Err(diagnostic::rule_invalid(
                RENDER_UNSUPPORTED,
                "computed-column",
                None,
            ));
        } else if column.generated_kind() == Some(GeneratedKind::Sequence) {
            // A generated sequence column draws from its own owned
            // sequence: the default is the point of 'generated'.
            let default = sequence_default(table.table(), column.name())?;
            line.push_str(&format!(" DEFAULT {default}"));
        } else if let Some(default) = column.default() {
            line.push_str(&format!(
                " DEFAULT {}",
                render_default(default, table.table())?
            ));
        }
        if !column.nullable() {
            line.push_str(" NOT NULL");
        }
        lines.push(line);
    }
    let primary_key = table
        .primary_key()
        .iter()
        .map(quote)
        .collect::<Vec<String>>()
        .join(", ");
    lines.push(format!("PRIMARY KEY ({primary_key})"));
    // The declared CHECK constraints of this table, canonical order.
    if let Some(declared) = attachment
        .projections()
        .iter()
        .find(|declared| declared.namespace().key() == "postgres")
        .and_then(|declared| {
            declared
                .tables()
                .iter()
                .find(|declared| declared.entity() == table.entity())
        })
    {
        for check in declared.checks() {
            lines.push(format!(
                "CONSTRAINT {} CHECK ({})",
                quote(check.name()),
                render_predicates(check.predicates())
            ));
        }
    }
    // The derived member-list CHECKs of the enum columns, column
    // order (the canonical member order of the declaration).
    for column in table.columns() {
        let Some(members) = enum_members.get(column.name().as_str()) else {
            continue;
        };
        let name = derived_name(
            &format!("chk_{}_{}", table.table(), column.name()),
            "check-name",
        )?;
        let values = members
            .iter()
            .map(|member| format!("'{}'", member.replace('\'', "''")))
            .collect::<Vec<String>>()
            .join(", ");
        lines.push(format!(
            "CONSTRAINT {} CHECK ({} IN ({values}))",
            quote(&name),
            quote(column.name())
        ));
    }
    Ok(format!(
        "CREATE TABLE {} ({});",
        quote(table.table()),
        lines.join(", ")
    ))
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
    match super::types::map_type(field_type, profile.policies()) {
        Ok(rendered) => Ok(rendered),
        Err(error)
            if error.reason_ids().first().copied() == Some(RENDER_UNSUPPORTED)
                // The policy table refuses an enum field only under
                // `native_enum`, which `create_table` already refuses
                // on its own; a field the table does not own keeps the
                // derived spelling.
                && !matches!(field_type, crate::storage_projection::entity::DomainType::Enum { .. }) =>
        {
            Err(error)
        }
        Err(_) => Ok(column.storage_type().to_owned()),
    }
}

/// Render one bounded predicate conjunction.
fn render_predicates(predicates: &[ColumnPredicate]) -> String {
    predicates
        .iter()
        .map(|predicate| {
            let column = quote(predicate.column());
            match (predicate.op(), predicate.value()) {
                (PredicateOp::IsNull, _) => format!("{column} IS NULL"),
                (PredicateOp::IsNotNull, _) => format!("{column} IS NOT NULL"),
                (PredicateOp::Eq, Some(value)) => format!("{column} = {}", render_literal(value)),
                (PredicateOp::Ne, Some(value)) => format!("{column} <> {}", render_literal(value)),
                (PredicateOp::Eq, None) | (PredicateOp::Ne, None) => {
                    format!("{column} IS NOT NULL")
                }
            }
        })
        .collect::<Vec<String>>()
        .join(" AND ")
}

/// Render one typed literal.
fn render_literal(value: &Literal) -> String {
    match value {
        Literal::Boolean(true) => "TRUE".to_owned(),
        Literal::Boolean(false) => "FALSE".to_owned(),
        Literal::Integer(value) => value.to_string(),
        Literal::Decimal(text) => text.clone(),
        Literal::Text(text) => format!("'{}'", text.replace('\'', "''")),
    }
}

/// Render one column default. A sequence default consumes the owning
/// table's deterministic sequence (`seq_<table>_<column>`), the exact
/// object the renderer creates — never the bare referenced column
/// name, which names no sequence.
pub(crate) fn render_default(
    default: &FieldDefault,
    table: &StorageName,
) -> Result<String, DiagnosticSet> {
    let rendered = match default {
        FieldDefault::Literal(literal) => render_literal(literal),
        FieldDefault::Now => "now()".to_owned(),
        FieldDefault::UuidGenerate => "gen_random_uuid()".to_owned(),
        FieldDefault::Sequence { column } => {
            let sequence = derived_name(
                &format!("seq_{}_{}", table, column.as_str()),
                "sequence-name",
            )?;
            format!("nextval('{}')", sequence)
        }
    };
    Ok(rendered)
}

/// The exact default spelling of a generated sequence column: the
/// deterministic sequence this renderer creates and owns.
pub(crate) fn sequence_default(
    table: &StorageName,
    column: &StorageName,
) -> Result<String, DiagnosticSet> {
    let sequence = derived_name(&format!("seq_{}_{}", table, column), "sequence-name")?;
    Ok(format!("nextval('{}')", sequence))
}

/// The deterministic name of one unnamed index.
fn derived_index_name(
    table: &StorageName,
    index: &crate::storage_projection::Index,
) -> Result<StorageName, DiagnosticSet> {
    let suffix = if index.unique() { "_uq" } else { "" };
    let columns = index
        .columns()
        .iter()
        .map(|column| column.as_str())
        .collect::<Vec<&str>>()
        .join("_");
    derived_name(&format!("idx_{table}_{columns}{suffix}"), "index-name")
}

/// Keep a derived identifier inside the storage-name grammar; a name
/// beyond the grammar bound refuses with the typed mapping rule, never
/// silently truncated and never a panic: every component is
/// schema-valid on its own, so an over-long composition is legal input
/// the renderer must answer with a diagnostic.
fn derived_name(text: &str, detail: &'static str) -> Result<StorageName, DiagnosticSet> {
    StorageName::parse(text).map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, detail, None))
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
            joined.contains("CREATE TABLE \"task_roster\" (\"id\" uuid NOT NULL, \"members\" jsonb, PRIMARY KEY (\"id\"))"),
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
