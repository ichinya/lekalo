//! The storage migration planner and the destructive gate (issue #69,
//! plan §4.7).
//!
//! [`plan`] derives the ordered migration plan from two same-project
//! storage-projection attachments under one engine profile: it derives
//! both projections, diffs them mechanically (tables, columns,
//! defaults, nullability, foreign keys, indexes, CHECK constraints,
//! sequences, RLS coverage), and emits deterministic steps in
//! dependency order — extensions, sequences, tables, join tables,
//! foreign keys, checks, indexes, sequence ownership, RLS, drops
//! last. Every step carries the closed data risk spelled exactly as
//! the storage-projection [`DataRisk`]; a plan with a destructive step
//! is gated and its status stays `blocked` until the caller names the
//! exact `planId` — the native-gate custody pattern. Core proposes
//! and never executes.

use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::derivation::{project, DerivedColumn, DerivedProjection};
use crate::storage_projection::entity::{FieldDefault, Literal};
use crate::storage_projection::id::StorageName;
use crate::storage_projection::projection::{DataRisk, GeneratedKind, PredicateOp};
use crate::storage_projection::{compare, StorageProjectionAttachment};

use super::diagnostic::{self, MAPPING_INVALID, MIGRATION_INVALID};
use super::postgres::quoting::quote;
use super::StorageEngineAttachment;

/// The closed custody status of one finished plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PlanStatus {
    /// Nothing destructive exists; the plan may apply.
    Ready,
    /// A destructive step exists and the exact planId was not named.
    Blocked,
    /// The caller named the exact planId of a gated plan.
    Confirmed,
}

impl PlanStatus {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Blocked => "blocked",
            Self::Confirmed => "confirmed",
        }
    }
}

/// The declared explain hook point of one step.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ExplainHook {
    /// EXPLAIN before the step applies.
    Before,
    /// EXPLAIN after the step applies.
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

/// One planned step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Step {
    pub(crate) id: usize,
    pub(crate) kind: &'static str,
    pub(crate) statement: String,
    pub(crate) risk: DataRisk,
    pub(crate) requires: Vec<usize>,
    pub(crate) explain: Option<ExplainHook>,
}

impl Step {
    /// The 1-based ordinal.
    pub const fn id(&self) -> usize {
        self.id
    }

    /// The closed step kind.
    pub fn kind(&self) -> &str {
        self.kind
    }

    /// The deterministic SQL text.
    pub fn statement(&self) -> &str {
        &self.statement
    }

    /// The closed data risk.
    pub const fn risk(&self) -> DataRisk {
        self.risk
    }

    /// The step ids this step depends on.
    pub fn requires(&self) -> &[usize] {
        &self.requires
    }

    /// The declared explain hook point, when declared.
    pub const fn explain(&self) -> Option<ExplainHook> {
        self.explain
    }
}

/// One finished migration plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationPlan {
    pub(crate) project_id: String,
    pub(crate) engine_version: String,
    pub(crate) base_digest: String,
    pub(crate) candidate_digest: String,
    pub(crate) diff_digest: String,
    pub(crate) gated: bool,
    pub(crate) status: PlanStatus,
    pub(crate) plan_id: String,
    pub(crate) steps: Vec<Step>,
}

impl MigrationPlan {
    /// The stable project identity.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// The pinned engine version.
    pub fn engine_version(&self) -> &str {
        &self.engine_version
    }

    /// The base attachment canonical digest.
    pub fn base_digest(&self) -> &str {
        &self.base_digest
    }

    /// The candidate attachment canonical digest.
    pub fn candidate_digest(&self) -> &str {
        &self.candidate_digest
    }

    /// The diff binding digest.
    pub fn diff_digest(&self) -> &str {
        &self.diff_digest
    }

    /// Whether the plan contains destructive steps.
    pub const fn gated(&self) -> bool {
        self.gated
    }

    /// The custody status.
    pub const fn status(&self) -> PlanStatus {
        self.status
    }

    /// The exact plan digest that unlocks a gated plan.
    pub fn plan_id(&self) -> &str {
        &self.plan_id
    }

    /// The ordered steps.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The canonical plan bytes (compact JSON, byte-sorted keys), or
    /// the typed over-bound refusal.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        let bytes = self.payload();
        if bytes.len() > super::version::MAX_CANONICAL_BYTES {
            return Err(diagnostic::export_limit_set(bytes.len()));
        }
        Ok(bytes)
    }

    fn payload(&self) -> String {
        use super::canonical::{array, flag, object, string};
        let steps: Vec<String> = self
            .steps
            .iter()
            .map(|step| {
                object(vec![
                    ("id", Some(step.id.to_string())),
                    ("kind", Some(string(step.kind))),
                    ("statement", Some(string(&step.statement))),
                    ("risk", Some(string(step.risk.key()))),
                    ("requires", optional_usize_array(&step.requires)),
                    ("explain", step.explain.map(|hook| string(hook.key()))),
                ])
            })
            .collect();
        object(vec![
            (
                "schemaVersion",
                Some(string("lekalo/storage-migration-plan/v0.4.0")),
            ),
            (
                "identity",
                Some(string("dev.lekalo.storage-migration-plan@0.4.0")),
            ),
            ("engine", Some(string("postgres"))),
            ("engineVersion", Some(string(&self.engine_version))),
            ("projectId", Some(string(&self.project_id))),
            ("baseDigest", Some(string(&self.base_digest))),
            ("candidateDigest", Some(string(&self.candidate_digest))),
            ("diffDigest", Some(string(&self.diff_digest))),
            ("gated", Some(flag(self.gated))),
            ("status", Some(string(self.status.key()))),
            ("planId", Some(string(&self.plan_id))),
            ("steps", Some(array(&steps))),
        ])
    }
}

/// One canonical integer array, or nothing when empty.
fn optional_usize_array(values: &[usize]) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    use super::canonical::array;
    Some(array(
        &values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<String>>(),
    ))
}

/// Derive the migration plan from the base and candidate attachments
/// under one engine profile. When `confirm` carries the exact plan
/// digest of a gated plan the status becomes `confirmed`; a wrong
/// digest refuses with the registered gate diagnostic. Pure and
/// read-only.
pub fn plan(
    profile: &StorageEngineAttachment,
    base: &StorageProjectionAttachment,
    candidate: &StorageProjectionAttachment,
    confirm: Option<&str>,
) -> Result<MigrationPlan, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(diagnostic::rule_invalid(
            MIGRATION_INVALID,
            "project-mismatch",
            None,
        ));
    }
    let base_digest = digest_of(base)?;
    let candidate_digest = digest_of(candidate)?;
    let diff = compare(base, candidate)?;
    let mut diff_material = String::new();
    for path in diff.paths() {
        diff_material.push_str(path.path());
        diff_material.push('\u{1}');
        diff_material.push_str(path.layer().key());
        diff_material.push('\u{1}');
        diff_material.push_str(path.class().key());
        diff_material.push('\u{1}');
        if let Some(risk) = path.risk() {
            diff_material.push_str(risk.key());
        }
        diff_material.push('\u{2}');
    }
    let diff_digest = format!(
        "sha256:{}",
        crate::digest::sha256_hex(diff_material.as_bytes())
    );
    let derived_base = project(base, crate::storage_projection::Namespace::Postgres)?;
    let derived_candidate = project(candidate, crate::storage_projection::Namespace::Postgres)?;
    let mut steps: Vec<Step> = Vec::new();
    // Extensions first.
    for extension in profile.extensions() {
        if extension.required() {
            let name = StorageName::parse(extension.name().as_str())
                .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "extension-name", None))?;
            push_step(
                &mut steps,
                "create_extension",
                format!("CREATE EXTENSION IF NOT EXISTS {};", quote(&name)),
                DataRisk::None,
                Vec::new(),
                None,
            );
        }
    }
    let table_ids = plan_tables(&mut steps, &derived_base, &derived_candidate, profile)?;
    // Foreign keys and checks after their tables.
    plan_foreign_keys(&mut steps, &derived_base, &derived_candidate, &table_ids);
    plan_checks(&mut steps, base, candidate)?;
    plan_indexes(&mut steps, &derived_base, &derived_candidate, &table_ids);
    // Drops last: the mechanical diff emits them, the ordering pass
    // moves every destructive drop behind the constructive steps.
    order_drops_last(&mut steps);
    let gated = steps
        .iter()
        .any(|step| step.risk() == DataRisk::Destructive);
    for (index, step) in steps.iter_mut().enumerate() {
        step.id = index + 1;
    }
    let plan_material = format!("lekalo.storage-migration-plan.v0.4.0\u{1}{}", {
        let mut payload = steps
            .iter()
            .map(|step| format!("{}\u{1}{}\u{1}{}\u{2}", step.id, step.kind, step.statement))
            .collect::<Vec<String>>()
            .join("");
        payload.push_str(&diff_digest);
        payload
    });
    let plan_id = format!(
        "sha256:{}",
        crate::digest::sha256_hex(plan_material.as_bytes())
    );
    let status = match (gated, confirm) {
        (false, _) => PlanStatus::Ready,
        (true, Some(named)) if named == plan_id => PlanStatus::Confirmed,
        (true, Some(_)) => {
            return Err(diagnostic::gated_set("plan-id-mismatch"));
        }
        (true, None) => PlanStatus::Blocked,
    };
    Ok(MigrationPlan {
        project_id: base.project_id().as_str().to_owned(),
        engine_version: profile.engine_version().as_str().to_owned(),
        base_digest,
        candidate_digest,
        diff_digest,
        gated,
        status,
        plan_id,
        steps,
    })
}

/// The canonical digest of one attachment.
fn digest_of(attachment: &StorageProjectionAttachment) -> Result<String, DiagnosticSet> {
    let bytes = attachment.canonical_bytes()?;
    Ok(format!(
        "sha256:{}",
        crate::digest::sha256_hex(bytes.as_bytes())
    ))
}

/// Push one step with deferred ordinal assignment.
fn push_step(
    steps: &mut Vec<Step>,
    kind: &'static str,
    statement: String,
    risk: DataRisk,
    requires: Vec<usize>,
    explain: Option<ExplainHook>,
) {
    steps.push(Step {
        id: 0,
        kind,
        statement,
        risk,
        requires,
        explain,
    });
}

/// The table plan: creates, drops, and per-column changes. Returns the
/// step index (0-based, pre-renumber) of each surviving table's create
/// or existing declaration, keyed by table name, for FK/index
/// dependency wiring.
fn plan_tables(
    steps: &mut Vec<Step>,
    base: &DerivedProjection,
    candidate: &DerivedProjection,
    profile: &StorageEngineAttachment,
) -> Result<std::collections::BTreeMap<String, usize>, DiagnosticSet> {
    let mut table_ids = std::collections::BTreeMap::new();
    // Sequences exist before the tables that use them.
    for table in candidate.tables() {
        for column in table.columns() {
            let base_column = base
                .table(table.entity())
                .and_then(|base_table| {
                    base_table
                        .columns()
                        .iter()
                        .find(|candidate| candidate.name() == column.name())
                })
                .is_some();
            if !base_column && column.generated_kind() == Some(GeneratedKind::Sequence) {
                let sequence =
                    StorageName::parse(&format!("seq_{}_{}", table.table(), column.name()))
                        .map_err(|_| {
                            diagnostic::rule_invalid(MAPPING_INVALID, "sequence-name", None)
                        })?;
                push_step(
                    steps,
                    "create_sequence",
                    format!("CREATE SEQUENCE {};", quote(&sequence)),
                    DataRisk::None,
                    Vec::new(),
                    None,
                );
            }
        }
    }
    // New tables: create with their columns, keys, and defaults.
    for table in candidate.tables() {
        if base.table(table.entity()).is_none() {
            let create = create_table_statement(profile, table)?;
            push_step(
                steps,
                "create_table",
                create,
                DataRisk::None,
                Vec::new(),
                None,
            );
            table_ids.insert(table.table().as_str().to_owned(), steps.len() - 1);
            if let Some(tenancy) = profile.tenancy() {
                if tenancy.enforcement() == super::Enforcement::Rls
                    && table
                        .columns()
                        .iter()
                        .any(|column| column.origin().key() == "tenant_key")
                {
                    let rls = tenancy.rls().ok_or_else(|| {
                        diagnostic::rule_invalid(MAPPING_INVALID, "rls-policy-missing", None)
                    })?;
                    let tenant = table
                        .columns()
                        .iter()
                        .find(|column| column.origin().key() == "tenant_key")
                        .expect("tenant column");
                    push_step(
                        steps,
                        "enable_rls",
                        format!(
                            "ALTER TABLE {} ENABLE ROW LEVEL SECURITY;",
                            quote(table.table())
                        ),
                        DataRisk::None,
                        Vec::new(),
                        None,
                    );
                    if rls.force() {
                        push_step(
                            steps,
                            "enable_rls",
                            format!(
                                "ALTER TABLE {} FORCE ROW LEVEL SECURITY;",
                                quote(table.table())
                            ),
                            DataRisk::None,
                            Vec::new(),
                            None,
                        );
                    }
                    let policy = StorageName::parse(&format!("pol_{}_tenant", table.table()))
                        .map_err(|_| {
                            diagnostic::rule_invalid(MAPPING_INVALID, "policy-name", None)
                        })?;
                    push_step(
                        steps,
                        "create_policy",
                        format!(
                            "CREATE POLICY {} ON {} USING ({} = current_setting('{}')::{});",
                            quote(&policy),
                            quote(table.table()),
                            quote(tenant.name()),
                            rls.session_variable().as_str(),
                            tenant.storage_type(),
                        ),
                        DataRisk::None,
                        Vec::new(),
                        None,
                    );
                }
            }
        }
    }
    // Existing tables: column-level changes.
    for table in candidate.tables() {
        let Some(base_table) = base.table(table.entity()) else {
            continue;
        };
        if base_table.table() != table.table() {
            // A rename is drop + create in v1: the declared risk of a
            // table rename is destructive, never guessed history.
            push_step(
                steps,
                "rename_table",
                format!(
                    "ALTER TABLE {} RENAME TO {};",
                    quote(base_table.table()),
                    quote(table.table())
                ),
                DataRisk::Destructive,
                Vec::new(),
                None,
            );
            continue;
        }
        table_ids
            .entry(table.table().as_str().to_owned())
            .or_insert(usize::MAX);
        for column in table.columns() {
            let Some(base_column) = base_table
                .columns()
                .iter()
                .find(|candidate| candidate.name() == column.name())
            else {
                // An added column: NOT NULL without a declared default
                // needs a backfill before it can hold.
                let tightening = !column.nullable();
                let mut requires = Vec::new();
                if let Some(create_id) = table_ids.get(table.table().as_str()) {
                    if *create_id < steps.len() {
                        requires.push(create_id + 1);
                    }
                }
                push_step(
                    steps,
                    "add_column",
                    format!(
                        "ALTER TABLE {} ADD COLUMN {} {}{};",
                        quote(table.table()),
                        quote(column.name()),
                        column.storage_type(),
                        if tightening { " NOT NULL" } else { "" }
                    ),
                    if tightening {
                        DataRisk::BackfillRequired
                    } else {
                        DataRisk::None
                    },
                    requires,
                    None,
                );
                if tightening {
                    // The backfill uses only the column's declared
                    // default zero value; arbitrary row text is
                    // unrepresentable.
                    push_step(
                        steps,
                        "backfill",
                        format!(
                            "UPDATE {} SET {} = {} WHERE {} IS NULL;",
                            quote(table.table()),
                            quote(column.name()),
                            backfill_literal(column),
                            quote(column.name())
                        ),
                        DataRisk::BackfillRequired,
                        Vec::new(),
                        None,
                    );
                }
                continue;
            };
            if base_column.storage_type() != column.storage_type() {
                push_step(
                    steps,
                    "alter_column_type",
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} TYPE {};",
                        quote(table.table()),
                        quote(column.name()),
                        column.storage_type()
                    ),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
            }
            if base_column.nullable() && !column.nullable() {
                push_step(
                    steps,
                    "set_column_null",
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL;",
                        quote(table.table()),
                        quote(column.name())
                    ),
                    if column.default().is_some() {
                        DataRisk::None
                    } else {
                        DataRisk::BackfillRequired
                    },
                    Vec::new(),
                    None,
                );
            } else if !base_column.nullable() && column.nullable() {
                push_step(
                    steps,
                    "set_column_null",
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL;",
                        quote(table.table()),
                        quote(column.name())
                    ),
                    DataRisk::None,
                    Vec::new(),
                    None,
                );
            }
            if base_column.default() != column.default() {
                match column.default() {
                    Some(default) => push_step(
                        steps,
                        "set_column_default",
                        format!(
                            "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {};",
                            quote(table.table()),
                            quote(column.name()),
                            render_default(default)?
                        ),
                        DataRisk::None,
                        Vec::new(),
                        None,
                    ),
                    None => push_step(
                        steps,
                        "set_column_default",
                        format!(
                            "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT;",
                            quote(table.table()),
                            quote(column.name())
                        ),
                        DataRisk::None,
                        Vec::new(),
                        None,
                    ),
                }
            }
        }
        for column in base_table.columns() {
            if !table
                .columns()
                .iter()
                .any(|candidate| candidate.name() == column.name())
            {
                push_step(
                    steps,
                    "drop_column",
                    format!(
                        "ALTER TABLE {} DROP COLUMN {};",
                        quote(table.table()),
                        quote(column.name())
                    ),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
            }
        }
    }
    // Dropped tables, last of the table work.
    for table in base.tables() {
        if candidate.table(table.entity()).is_none() {
            push_step(
                steps,
                "drop_table",
                format!("DROP TABLE {};", quote(table.table())),
                DataRisk::Destructive,
                Vec::new(),
                None,
            );
        }
    }
    Ok(table_ids)
}

/// One deterministic placeholder backfill literal for a tightened
/// column: the zero value of its declared default kind, never
/// invented row data.
fn backfill_literal(column: &DerivedColumn) -> String {
    match column.default() {
        Some(default) => render_default(default).unwrap_or_else(|_| "NULL".to_owned()),
        None => match column.storage_type() {
            "boolean" => "FALSE".to_owned(),
            "bigint" | "smallint" | "integer" | "real" | "numeric" | "double precision" => {
                "0".to_owned()
            }
            "text" | "varchar" | "json" | "jsonb" => "' '".to_owned(),
            _ => "NULL".to_owned(),
        },
    }
}

/// Render one column default.
fn render_default(default: &FieldDefault) -> Result<String, DiagnosticSet> {
    let rendered = match default {
        FieldDefault::Literal(literal) => render_literal(literal),
        FieldDefault::Now => "now()".to_owned(),
        FieldDefault::UuidGenerate => "gen_random_uuid()".to_owned(),
        FieldDefault::Sequence { column } => format!("nextval('{}')", column.as_str()),
    };
    Ok(rendered)
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

/// Render one CREATE TABLE statement for a new table.
fn create_table_statement(
    _profile: &StorageEngineAttachment,
    table: &crate::storage_projection::derivation::DerivedTable,
) -> Result<String, DiagnosticSet> {
    let mut lines: Vec<String> = Vec::new();
    for column in table.columns() {
        let mut line = format!("{} {}", quote(column.name()), column.storage_type());
        if let Some(default) = column.default() {
            line.push_str(&format!(" DEFAULT {}", render_default(default)?));
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
    Ok(format!(
        "CREATE TABLE {} ({});",
        quote(table.table()),
        lines.join(", ")
    ))
}

/// The foreign-key plan: added and removed constraints, dependency
/// ordered behind their tables.
fn plan_foreign_keys(
    steps: &mut Vec<Step>,
    base: &DerivedProjection,
    candidate: &DerivedProjection,
    table_ids: &std::collections::BTreeMap<String, usize>,
) {
    // The primary key of every candidate table, by table name: the
    // deterministic FK target column.
    let primary_keys: std::collections::BTreeMap<&str, &StorageName> = candidate
        .tables()
        .iter()
        .filter_map(|table| {
            table
                .primary_key()
                .first()
                .map(|pk| (table.table().as_str(), pk))
        })
        .collect();
    for table in candidate.tables() {
        let base_table = base.table(table.entity());
        for foreign_key in table.foreign_keys() {
            let existed = base_table
                .map(|base| {
                    base.foreign_keys()
                        .iter()
                        .any(|candidate| candidate.column() == foreign_key.column())
                })
                .unwrap_or(false);
            if existed {
                continue;
            }
            let mut requires = Vec::new();
            if let Some(create_id) = table_ids.get(table.table().as_str()) {
                if *create_id < steps.len() {
                    requires.push(create_id + 1);
                }
            }
            let name =
                StorageName::parse(&format!("fk_{}_{}", table.table(), foreign_key.column()))
                    .unwrap_or_else(|_| StorageName::parse("fk_derived").expect("literal"));
            push_step(
                steps,
                "add_foreign_key",
                format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {};",
                    quote(table.table()),
                    quote(&name),
                    quote(foreign_key.column()),
                    quote(foreign_key.references_table()),
                    quote(primary_keys.get(foreign_key.references_table().as_str()).copied().unwrap_or(&StorageName::parse("id").expect("literal"))),
                    foreign_key.on_delete().key(),
                ),
                DataRisk::None,
                requires,
                None,
            );
        }
    }
    for table in base.tables() {
        let candidate_table = candidate.table(table.entity());
        for foreign_key in table.foreign_keys() {
            let removed = !candidate_table
                .map(|candidate| {
                    candidate
                        .foreign_keys()
                        .iter()
                        .any(|base| base.column() == foreign_key.column())
                })
                .unwrap_or(false);
            if removed {
                let name =
                    StorageName::parse(&format!("fk_{}_{}", table.table(), foreign_key.column()))
                        .unwrap_or_else(|_| StorageName::parse("fk_derived").expect("literal"));
                push_step(
                    steps,
                    "drop_constraint",
                    format!(
                        "ALTER TABLE {} DROP CONSTRAINT {};",
                        quote(table.table()),
                        quote(&name)
                    ),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
            }
        }
    }
}

/// The CHECK-constraint plan from the declared storage tables.
fn plan_checks(
    steps: &mut Vec<Step>,
    base: &StorageProjectionAttachment,
    candidate: &StorageProjectionAttachment,
) -> Result<(), DiagnosticSet> {
    let declared = |attachment: &StorageProjectionAttachment| {
        attachment
            .projections()
            .iter()
            .find(|projection| projection.namespace().key() == "postgres")
            .map(|projection| {
                projection
                    .tables()
                    .iter()
                    .map(|table| (table.entity().as_str().to_owned(), table.clone()))
                    .collect::<std::collections::BTreeMap<String, crate::storage_projection::Table>>()
            })
    };
    let base_tables = declared(base).unwrap_or_default();
    let candidate_tables = declared(candidate).unwrap_or_default();
    for (entity, table) in &candidate_tables {
        for check in table.checks() {
            let existed = base_tables
                .get(entity)
                .map(|base| {
                    base.checks()
                        .iter()
                        .any(|candidate| candidate.name() == check.name())
                })
                .unwrap_or(false);
            if existed {
                continue;
            }
            push_step(
                steps,
                "add_check",
                format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({});",
                    quote(table.table()),
                    quote(check.name()),
                    render_predicates(check.predicates())
                ),
                DataRisk::None,
                Vec::new(),
                None,
            );
        }
    }
    for (entity, table) in &base_tables {
        for check in table.checks() {
            let removed = !candidate_tables
                .get(entity)
                .map(|candidate| {
                    candidate
                        .checks()
                        .iter()
                        .any(|base| base.name() == check.name())
                })
                .unwrap_or(false);
            if removed {
                push_step(
                    steps,
                    "drop_check",
                    format!(
                        "ALTER TABLE {} DROP CONSTRAINT {};",
                        quote(table.table()),
                        quote(check.name())
                    ),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
            }
        }
    }
    Ok(())
}

/// Render one bounded predicate conjunction.
fn render_predicates(
    predicates: &[crate::storage_projection::projection::ColumnPredicate],
) -> String {
    use crate::storage_projection::projection::ColumnPredicate;
    predicates
        .iter()
        .map(|predicate: &ColumnPredicate| {
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

/// The index plan: added and removed indexes, deterministic names for
/// the unnamed ones.
fn plan_indexes(
    steps: &mut Vec<Step>,
    base: &DerivedProjection,
    candidate: &DerivedProjection,
    table_ids: &std::collections::BTreeMap<String, usize>,
) {
    for table in candidate.tables() {
        let base_table = base.table(table.entity());
        for index in table.indexes() {
            let existed = base_table
                .map(|base| base.indexes().contains(index))
                .unwrap_or(false);
            if existed {
                continue;
            }
            let name = match index.name() {
                Some(name) => name.clone(),
                None => {
                    let suffix = if index.unique() { "_uq" } else { "" };
                    let columns = index
                        .columns()
                        .iter()
                        .map(|column| column.as_str())
                        .collect::<Vec<&str>>()
                        .join("_");
                    StorageName::parse(&format!("idx_{}_{}{}", table.table(), columns, suffix))
                        .unwrap_or_else(|_| StorageName::parse("idx_derived").expect("literal"))
                }
            };
            let unique = if index.unique() { "UNIQUE " } else { "" };
            let mut requires = Vec::new();
            if let Some(create_id) = table_ids.get(table.table().as_str()) {
                if *create_id < steps.len() {
                    requires.push(create_id + 1);
                }
            }
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
            push_step(
                steps,
                "add_index",
                format!(
                    "CREATE {unique}INDEX {} ON {} ({}){predicate};",
                    quote(&name),
                    quote(table.table()),
                    columns
                ),
                DataRisk::None,
                requires,
                None,
            );
        }
    }
    for table in base.tables() {
        let candidate_table = candidate.table(table.entity());
        for index in table.indexes() {
            let removed = !candidate_table
                .map(|candidate| candidate.indexes().contains(index))
                .unwrap_or(false);
            if removed {
                let name = match index.name() {
                    Some(name) => name.clone(),
                    None => {
                        let suffix = if index.unique() { "_uq" } else { "" };
                        let columns = index
                            .columns()
                            .iter()
                            .map(|column| column.as_str())
                            .collect::<Vec<&str>>()
                            .join("_");
                        StorageName::parse(&format!("idx_{}_{}{}", table.table(), columns, suffix))
                            .unwrap_or_else(|_| StorageName::parse("idx_derived").expect("literal"))
                    }
                };
                push_step(
                    steps,
                    "drop_index",
                    format!("DROP INDEX {};", quote(&name)),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
            }
        }
    }
}

/// Move every drop behind the constructive steps; within the drops,
/// columns precede tables. Stable for identical inputs.
fn order_drops_last(steps: &mut [Step]) {
    let is_drop = |step: &Step| step.kind.starts_with("drop_");
    steps.sort_by_key(|step| if is_drop(step) { 1 } else { 0 });
}
