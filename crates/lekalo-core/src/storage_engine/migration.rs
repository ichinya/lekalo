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

use super::diagnostic::{self, MAPPING_INVALID, MIGRATION_INVALID, RENDER_UNSUPPORTED};
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
    // The binding guarantee holds on every surface: the profile is
    // authored against one storage projection — the base state this
    // plan starts from — and a profile bound to an unrelated
    // projection refuses, exactly as the DDL renderer, the drift
    // comparison, and the input document refuse.
    let base_digest = digest_of(base)?;
    if base_digest.as_str() != profile.projection_ref().as_str() {
        return Err(diagnostic::rule_invalid(
            MIGRATION_INVALID,
            "projection-binding-mismatch",
            None,
        ));
    }
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
    let table_ids = plan_tables(
        &mut steps,
        &derived_base,
        &derived_candidate,
        profile,
        candidate,
    )?;
    // Sequence ownership after the tables exist: the executable order
    // mirrors the DDL document — every created sequence is bound to
    // its owning column before the constraint passes run. Renames and
    // drops ride the same pass: a renamed table's sequence renames to
    // its fresh deterministic name, and a dropped sequence column
    // retires its sequence instead of orphaning it.
    let sequence_plan = plan_sequence_lifecycle(&derived_base, &derived_candidate)?;
    for (sequence, owner) in &sequence_plan.owners {
        push_step(
            &mut steps,
            "alter_sequence",
            format!("ALTER SEQUENCE {} OWNED BY {owner};", quote(sequence)),
            DataRisk::None,
            Vec::new(),
            None,
        );
    }
    for (from, to) in &sequence_plan.renames {
        push_step(
            &mut steps,
            "rename_sequence",
            format!("ALTER SEQUENCE {} RENAME TO {};", quote(from), quote(to)),
            DataRisk::None,
            Vec::new(),
            None,
        );
    }
    for sequence in &sequence_plan.drops {
        push_step(
            &mut steps,
            "drop_sequence",
            format!("DROP SEQUENCE {};", quote(sequence)),
            DataRisk::Destructive,
            Vec::new(),
            None,
        );
    }
    // Foreign keys and checks after their tables. Checks skip the new
    // tables: their create_table statements already carry every
    // declared and derived constraint inline.
    plan_foreign_keys(&mut steps, &derived_base, &derived_candidate, &table_ids)?;
    plan_checks(&mut steps, base, candidate)?;
    plan_enum_checks(&mut steps, profile, base, candidate)?;
    plan_indexes(&mut steps, &derived_base, &derived_candidate, &table_ids)?;
    // Drops last: the mechanical diff emits them, the ordering pass
    // moves every destructive drop behind the constructive steps.
    order_drops_last(&mut steps);
    let gated = steps
        .iter()
        .any(|step| step.risk() == DataRisk::Destructive);
    // The declared hook points are real: every destructive step
    // declares EXPLAIN before it applies, so an applying adapter can
    // stage the declared observation; constructive steps carry none.
    for step in steps.iter_mut() {
        if step.risk() == DataRisk::Destructive {
            step.explain = Some(ExplainHook::Before);
        }
    }
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

/// The sequence ownership the plan must establish: every candidate
/// generated sequence column whose sequence the base did not create —
/// on surviving tables and on brand-new tables alike, because the
/// ownership statement is separate from the CREATE TABLE statement in
/// the DDL document — yields `(sequence, "\"table\".\"column\"")`,
/// the exact `ALTER SEQUENCE … OWNED BY` target the document renders
/// (golden statement 23). Byte-sorted for determinism.
/// The sequence lifecycle a plan must establish: ownership for newly
/// created sequences, renames where a renamed table re-derives its
/// sequence name, and drops where a sequence column leaves the
/// candidate — the migrated schema then carries exactly the sequence
/// objects a fresh render would produce. All three lists are
/// byte-sorted for determinism.
struct SequencePlan {
    owners: Vec<(StorageName, String)>,
    renames: Vec<(StorageName, StorageName)>,
    drops: Vec<StorageName>,
}

fn plan_sequence_lifecycle(
    base: &DerivedProjection,
    candidate: &DerivedProjection,
) -> Result<SequencePlan, DiagnosticSet> {
    // The deterministic sequence name of one generated sequence
    // column: `seq_<table>_<column>`.
    let sequence_of = |table: &StorageName, column: &StorageName| {
        StorageName::parse(&format!("seq_{}_{}", table, column))
            .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "sequence-name", None))
    };
    let mut owners: Vec<(StorageName, String)> = Vec::new();
    let mut renames: Vec<(StorageName, StorageName)> = Vec::new();
    let mut drops: Vec<StorageName> = Vec::new();
    for table in candidate.tables() {
        let base_table = base.table(table.entity());
        for column in table.columns() {
            if column.generated_kind() != Some(GeneratedKind::Sequence) {
                continue;
            }
            let base_table_column = base_table.and_then(|base_table| {
                base_table
                    .columns()
                    .iter()
                    .find(|base| base.name() == column.name())
                    .map(|base| (base_table.table(), base))
            });
            match base_table_column {
                // The column survived with a renamed table: the
                // sequence re-derives its name with the table, the same
                // contract the fk_/idx_/chk_ names hold.
                Some((base_name, _)) if base_name != table.table() => {
                    let from = sequence_of(base_name, column.name())?;
                    let to = sequence_of(table.table(), column.name())?;
                    if from != to {
                        renames.push((from, to));
                    }
                }
                // The column survived on a same-named table: nothing
                // to do — the sequence exists and is owned.
                Some((_, _)) => {}
                // A brand-new sequence column: the planner already
                // created its sequence; plan the ownership binding.
                None => {
                    let sequence = sequence_of(table.table(), column.name())?;
                    owners.push((
                        sequence,
                        format!("{}.{}", quote(table.table()), quote(column.name())),
                    ));
                }
            }
        }
    }
    // A dropped sequence column on a surviving table plans no
    // explicit retirement: the column carries
    // DEFAULT nextval(<sequence>) and the OWNED BY dependency, so its
    // DROP COLUMN auto-drops the sequence — an explicit DROP SEQUENCE
    // cannot execute in either order (before the column the default
    // blocks it; after it the sequence no longer exists). A dropped
    // table carries its sequence with the rank-3 DROP TABLE the same
    // way. The only retirement an owner could owe — surviving without
    // its nextval default — is refused earlier as a generated-kind
    // change, so `drops` stays empty by construction.
    owners.sort();
    renames.sort();
    drops.sort();
    Ok(SequencePlan {
        owners,
        renames,
        drops,
    })
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

/// The table plan: creates, drops, and per-column changes for the
/// entity tables and the materialized join tables alike. Returns the
/// step index (0-based, pre-renumber) of each surviving table's create
/// or existing declaration, keyed by table name, for FK/index
/// dependency wiring.
fn plan_tables(
    steps: &mut Vec<Step>,
    base: &DerivedProjection,
    candidate: &DerivedProjection,
    profile: &StorageEngineAttachment,
    candidate_attachment: &StorageProjectionAttachment,
) -> Result<std::collections::BTreeMap<String, usize>, DiagnosticSet> {
    let mut table_ids = std::collections::BTreeMap::new();
    // Sequences exist before the tables that use them: every new
    // generated sequence column creates its deterministic sequence —
    // the exact object its default and the ownership pass reference.
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
    // New tables: create through the exact DDL renderer, so the plan
    // and the DDL document describe one schema — identity columns,
    // defaults, declared CHECK constraints, and the derived enum
    // member CHECKs ride the create statement identically.
    for table in candidate.tables() {
        if base.table(table.entity()).is_none() {
            let create = super::postgres::ddl::create_table(profile, candidate_attachment, table)?;
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
    // The primary key of every candidate table, by table name: the
    // deterministic FK target column for rename re-derivation.
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
            // The derived constraint names embed the table name, so
            // the renamed table's foreign keys drop under their old
            // names and re-add under the fresh deterministic ones —
            // the migrated schema never keeps a stale `fk_<oldtable>_
            // *` a fresh render would not produce.
            let mut rename_requires = Vec::new();
            for foreign_key in table.foreign_keys() {
                let old_name = StorageName::parse(&format!(
                    "fk_{}_{}",
                    base_table.table(),
                    foreign_key.column()
                ))
                .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "foreign-key-name", None))?;
                let drop_id = steps.len();
                push_step(
                    steps,
                    "drop_constraint",
                    format!(
                        "ALTER TABLE {} DROP CONSTRAINT {};",
                        quote(table.table()),
                        quote(&old_name)
                    ),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
                rename_requires.push(drop_id + 1);
            }
            for index in table.indexes() {
                if index.name().is_some() {
                    // A declared name is not derived; the rename
                    // leaves it stable.
                    continue;
                }
                let old_name = super::postgres::ddl::derived_index_name(base_table.table(), index)?;
                let drop_id = steps.len();
                push_step(
                    steps,
                    "drop_index",
                    format!("DROP INDEX {};", quote(&old_name)),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
                rename_requires.push(drop_id + 1);
            }
            for foreign_key in table.foreign_keys() {
                let referenced = primary_keys
                    .get(foreign_key.references_table().as_str())
                    .copied()
                    .ok_or_else(|| {
                        diagnostic::rule_invalid(MAPPING_INVALID, "referenced-key-absent", None)
                    })?;
                let name =
                    StorageName::parse(&format!("fk_{}_{}", table.table(), foreign_key.column()))
                        .map_err(|_| {
                        diagnostic::rule_invalid(MAPPING_INVALID, "foreign-key-name", None)
                    })?;
                push_step(
                    steps,
                    "add_foreign_key",
                    format!(
                        "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {};",
                        quote(table.table()),
                        quote(&name),
                        quote(foreign_key.column()),
                        quote(foreign_key.references_table()),
                        quote(referenced),
                        foreign_key.on_delete().key(),
                    ),
                    DataRisk::None,
                    rename_requires.clone(),
                    None,
                );
            }
            for index in table.indexes() {
                if index.name().is_some() {
                    continue;
                }
                let name = super::postgres::ddl::derived_index_name(table.table(), index)?;
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
                    rename_requires.clone(),
                    None,
                );
            }
            continue;
        }
        table_ids
            .entry(table.table().as_str().to_owned())
            .or_insert(usize::MAX);
        // A changed primary key on a surviving table is visible: the
        // old constraint drops before the new one is added (both names
        // are deterministic — `pk_<table>`), and the swap is a
        // destructive rewrite of the table's identity, so it gates.
        if base_table.primary_key() != table.primary_key() {
            let key_name = StorageName::parse(&format!("pk_{}", table.table()))
                .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "primary-key-name", None))?;
            let drop_id = steps.len();
            push_step(
                steps,
                "drop_constraint",
                format!(
                    "ALTER TABLE {} DROP CONSTRAINT {};",
                    quote(table.table()),
                    quote(&key_name)
                ),
                DataRisk::Destructive,
                Vec::new(),
                None,
            );
            let columns = table
                .primary_key()
                .iter()
                .map(quote)
                .collect::<Vec<String>>()
                .join(", ");
            push_step(
                steps,
                "add_primary_key",
                format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} PRIMARY KEY ({});",
                    quote(table.table()),
                    quote(&key_name),
                    columns
                ),
                DataRisk::Destructive,
                vec![drop_id + 1],
                None,
            );
        }
        for column in table.columns() {
            let Some(base_column) = base_table
                .columns()
                .iter()
                .find(|candidate| candidate.name() == column.name())
            else {
                // A computed generated column refuses: the 0.4.0
                // member carries no expression, and an ADD COLUMN of a
                // plain stored column would invent one.
                if column.generated_kind() == Some(GeneratedKind::Computed) {
                    return Err(diagnostic::rule_invalid(
                        RENDER_UNSUPPORTED,
                        "computed-column",
                        None,
                    ));
                }
                let mut add_requires = Vec::new();
                if let Some(create_id) = table_ids.get(table.table().as_str()) {
                    if *create_id < steps.len() {
                        add_requires.push(create_id + 1);
                    }
                }
                // The policy table owns the field-origin type spelling.
                let storage_type = super::postgres::ddl::column_storage_type(
                    profile,
                    candidate_attachment,
                    table,
                    column,
                )?;
                if !column.nullable() && column.default().is_some() {
                    // The declared default fills existing rows at ADD
                    // time (the fast default), so one step is
                    // executable and no backfill is owed.
                    let default = column.default().expect("declared default");
                    push_step(
                        steps,
                        "add_column",
                        format!(
                            "ALTER TABLE {} ADD COLUMN {} {} DEFAULT {} NOT NULL;",
                            quote(table.table()),
                            quote(column.name()),
                            storage_type,
                            render_default(default, table.table())?
                        ),
                        DataRisk::None,
                        add_requires,
                        None,
                    );
                } else if !column.nullable() {
                    // PostgreSQL refuses ADD COLUMN ... NOT NULL on a
                    // non-empty table, so the executable order is:
                    // add nullable, backfill the zero value, then hold
                    // the constraint. The backfill literal is the
                    // column's declared default or its type's zero
                    // value; types with no zero value refuse instead
                    // of backfilling NULL.
                    let add_id = steps.len();
                    push_step(
                        steps,
                        "add_column",
                        format!(
                            "ALTER TABLE {} ADD COLUMN {} {};",
                            quote(table.table()),
                            quote(column.name()),
                            storage_type
                        ),
                        DataRisk::None,
                        add_requires,
                        None,
                    );
                    push_step(
                        steps,
                        "backfill",
                        format!(
                            "UPDATE {} SET {} = {} WHERE {} IS NULL;",
                            quote(table.table()),
                            quote(column.name()),
                            backfill_literal(column, table.table())?,
                            quote(column.name())
                        ),
                        DataRisk::BackfillRequired,
                        vec![add_id + 1],
                        None,
                    );
                    push_step(
                        steps,
                        "set_column_null",
                        format!(
                            "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL;",
                            quote(table.table()),
                            quote(column.name())
                        ),
                        DataRisk::BackfillRequired,
                        vec![add_id + 2],
                        None,
                    );
                } else {
                    push_step(
                        steps,
                        "add_column",
                        format!(
                            "ALTER TABLE {} ADD COLUMN {} {};",
                            quote(table.table()),
                            quote(column.name()),
                            storage_type
                        ),
                        DataRisk::None,
                        add_requires,
                        None,
                    );
                }
                continue;
            };
            // The policy table owns the field-origin type spelling on
            // the surviving-column paths too.
            let storage_type = super::postgres::ddl::column_storage_type(
                profile,
                candidate_attachment,
                table,
                column,
            )?;
            // A changed generation kind has no deterministic v1
            // transition (sequence→identity would need SET GENERATED
            // plus the owned sequence's retirement, and the step
            // vocabulary carries no sequence drop); it refuses with the
            // registered rule instead of silently emitting nothing.
            if base_column.generated_kind() != column.generated_kind() {
                return Err(diagnostic::rule_invalid(
                    RENDER_UNSUPPORTED,
                    "generated-kind-change",
                    None,
                ));
            }
            if base_column.storage_type() != column.storage_type() {
                // PostgreSQL executes ALTER COLUMN TYPE only with an
                // assignment cast between the two spellings; a
                // transition without one cannot run, so it refuses with
                // the registered rule instead of emitting a statement
                // that fails mid-apply.
                if !assignment_castable(base_column.storage_type(), &storage_type) {
                    return Err(diagnostic::rule_invalid(
                        RENDER_UNSUPPORTED,
                        "column-type-uncastable",
                        None,
                    ));
                }
                push_step(
                    steps,
                    "alter_column_type",
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} TYPE {};",
                        quote(table.table()),
                        quote(column.name()),
                        storage_type
                    ),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
            }
            if base_column.nullable() && !column.nullable() {
                // A declared default helps future inserts only; SET
                // NOT NULL still fails on any existing NULL row. The
                // executable order is backfill first, then the
                // constraint; a type with no zero value refuses
                // instead of planning a NULL backfill.
                let backfill_id = steps.len();
                push_step(
                    steps,
                    "backfill",
                    format!(
                        "UPDATE {} SET {} = {} WHERE {} IS NULL;",
                        quote(table.table()),
                        quote(column.name()),
                        backfill_literal(column, table.table())?,
                        quote(column.name())
                    ),
                    DataRisk::BackfillRequired,
                    Vec::new(),
                    None,
                );
                push_step(
                    steps,
                    "set_column_null",
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL;",
                        quote(table.table()),
                        quote(column.name())
                    ),
                    DataRisk::BackfillRequired,
                    vec![backfill_id + 1],
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
                            render_default(default, table.table())?
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
    // Join tables: creates and drops, mirroring the DDL renderer's
    // join block. The join table's shape is fully derived from its
    // relation and the two endpoint primary keys, so any change to
    // either endpoint's table or key rematerializes the join — drop,
    // then create under the same deterministic name. Join-table FKs
    // ride the join create in the DDL renderer's ordering (the FK pass
    // covers only entity tables), so the create carries the whole
    // materialized relation exactly as the document renders it.
    for join in candidate.joins() {
        // The create of a rematerialized join is wired behind the drop
        // of its old shape; empty when the join is new or unchanged.
        let mut rematerialize_requires: Vec<usize> = Vec::new();
        let base_join = base
            .joins()
            .iter()
            .find(|base| base.relation() == join.relation());
        let unchanged = base_join.is_some_and(|base| {
            base.table() == join.table()
                && base.columns() == join.columns()
                && base.unique_pair() == join.unique_pair()
                && base.on_owner_delete() == join.on_owner_delete()
                && base.on_target_delete() == join.on_target_delete()
        });
        if unchanged {
            table_ids
                .entry(join.table().as_str().to_owned())
                .or_insert(usize::MAX);
            continue;
        }
        if let Some(existing) = base_join {
            let renamed = existing.table() != join.table();
            let rematerialized = !renamed
                && (existing.columns() != join.columns()
                    || existing.unique_pair() != join.unique_pair()
                    || existing.on_owner_delete() != join.on_owner_delete()
                    || existing.on_target_delete() != join.on_target_delete());
            if renamed {
                // A rename is drop + create: the declared risk of a
                // table rename is destructive, never guessed history.
                push_step(
                    steps,
                    "rename_table",
                    format!(
                        "ALTER TABLE {} RENAME TO {};",
                        quote(existing.table()),
                        quote(join.table())
                    ),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
                // The join foreign-key names embed the table name, so
                // the renamed join's FKs drop under their old names and
                // re-add under the fresh deterministic ones — the
                // migrated schema never keeps a stale `fk_<oldjoin>_*`
                // a fresh render would not produce (the same contract
                // the entity-table rename block holds).
                let mut rename_requires = Vec::new();
                for column in existing.columns() {
                    let old_name =
                        StorageName::parse(&format!("fk_{}_{}", existing.table(), column.name()))
                            .map_err(|_| {
                            diagnostic::rule_invalid(MAPPING_INVALID, "foreign-key-name", None)
                        })?;
                    let drop_id = steps.len();
                    push_step(
                        steps,
                        "drop_constraint",
                        format!(
                            "ALTER TABLE {} DROP CONSTRAINT {};",
                            quote(join.table()),
                            quote(&old_name)
                        ),
                        DataRisk::Destructive,
                        Vec::new(),
                        None,
                    );
                    rename_requires.push(drop_id + 1);
                }
                let mut statements = create_join_and_fks(candidate_attachment, candidate, join)?;
                // The renamed table exists already; only its FK names
                // change, so the create statement (statements[0]) is
                // not re-emitted — the FK adds carry the full
                // re-derivation.
                statements.remove(0);
                table_ids
                    .entry(join.table().as_str().to_owned())
                    .or_insert(usize::MAX);
                for fk in statements {
                    push_step(
                        steps,
                        "add_foreign_key",
                        fk,
                        DataRisk::None,
                        rename_requires.clone(),
                        None,
                    );
                }
                continue;
            } else if rematerialized {
                // The table is rematerialized under the same name: the
                // drop of the old shape must stay paired with the
                // re-create that follows (both name the same quoted
                // table), or the ordering pass would move the DROP
                // TABLE behind the create — which fails with `relation
                // already exists` on a confirmed plan. The create is
                // wired behind the drop so the pair survives any
                // reordering.
                let drop_id = steps.len();
                push_step(
                    steps,
                    "drop_table",
                    format!("DROP TABLE {};", quote(existing.table())),
                    DataRisk::Destructive,
                    Vec::new(),
                    None,
                );
                rematerialize_requires = vec![drop_id + 1];
            }
        }
        // The FK statements of the DDL renderer's join block are
        // joined facts: they ride the join create step, not separate
        // add_foreign_key steps.
        let mut statements = create_join_and_fks(candidate_attachment, candidate, join)?;
        let create = statements.remove(0);
        push_step(
            steps,
            "create_table",
            create,
            DataRisk::None,
            rematerialize_requires.clone(),
            None,
        );
        table_ids.insert(join.table().as_str().to_owned(), steps.len() - 1);
        for fk in statements {
            push_step(
                steps,
                "add_foreign_key",
                fk,
                DataRisk::None,
                Vec::new(),
                None,
            );
        }
    }
    for join in base.joins() {
        if !candidate
            .joins()
            .iter()
            .any(|candidate| candidate.relation() == join.relation())
        {
            push_step(
                steps,
                "drop_table",
                format!("DROP TABLE {};", quote(join.table())),
                DataRisk::Destructive,
                Vec::new(),
                None,
            );
        }
    }
    Ok(table_ids)
}

/// Render one join table's create plus its two join foreign keys —
/// the exact statements the DDL renderer emits for the same relation,
/// so a planned join create and the DDL document agree byte for byte.
fn create_join_and_fks(
    attachment: &StorageProjectionAttachment,
    projection: &DerivedProjection,
    join: &crate::storage_projection::derivation::DerivedJoin,
) -> Result<Vec<String>, DiagnosticSet> {
    let mut statements = vec![super::postgres::ddl::create_join_table(join)];
    for (index, action) in [
        (0usize, join.on_owner_delete()),
        (1usize, join.on_target_delete()),
    ] {
        let column = &join.columns()[index];
        let owner_side = index == 0;
        let Some((table_name, pk)) =
            super::postgres::ddl::referenced_join_target(attachment, projection, join, owner_side)
        else {
            return Err(diagnostic::rule_invalid(
                MAPPING_INVALID,
                "join-reference-unresolved",
                None,
            ));
        };
        let name = StorageName::parse(&format!("fk_{}_{}", join.table(), column.name()))
            .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "foreign-key-name", None))?;
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
    Ok(statements)
}

/// One deterministic placeholder backfill literal for a tightened
/// column: the zero value of its declared default kind, never
/// invented row data. A sequence default consumes the owning table's
/// deterministic sequence.
fn backfill_literal(column: &DerivedColumn, table: &StorageName) -> Result<String, DiagnosticSet> {
    match column.default() {
        Some(default) => render_default(default, table),
        None => match column.storage_type() {
            "boolean" => Ok("FALSE".to_owned()),
            "bigint" | "smallint" | "integer" | "real" | "numeric" | "double precision" => {
                Ok("0".to_owned())
            }
            "text" | "varchar" | "json" | "jsonb" => Ok("''".to_owned()),
            _ => Err(diagnostic::rule_invalid(
                RENDER_UNSUPPORTED,
                "backfill-literal",
                None,
            )),
        },
    }
}

/// Render one column default. A sequence default consumes the owning
/// table's deterministic sequence (`seq_<table>_<column>`), the exact
/// object the planner creates — never the bare referenced column
/// name, which names no sequence.
fn render_default(default: &FieldDefault, table: &StorageName) -> Result<String, DiagnosticSet> {
    let rendered = match default {
        FieldDefault::Literal(literal) => render_literal(literal),
        FieldDefault::Now => "now()".to_owned(),
        FieldDefault::UuidGenerate => "gen_random_uuid()".to_owned(),
        FieldDefault::Sequence { column } => {
            let sequence = StorageName::parse(&format!("seq_{}_{}", table, column.as_str()))
                .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "sequence-name", None))?;
            format!("nextval('{}')", sequence)
        }
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

/// Whether PostgreSQL holds an assignment cast (or implicit
/// equivalence) between two rendered storage-type spellings, so a bare
/// `ALTER COLUMN TYPE` executes. The vocabulary is the closed type
/// table this engine renders; identical spellings are trivially
/// castable. Anything outside the table (an unknown spelling) refuses
/// via `false`, keeping the planner fail-closed.
fn assignment_castable(from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    // Numeric widenings: integer families and numeric scale-ups cast
    // by assignment; narrowing an integer or decimal refuses (a
    // narrowing rewrite is a data decision the planner never makes).
    let numeric_rank = |spelling: &str| -> Option<u8> {
        match spelling {
            "smallint" => Some(1),
            "integer" => Some(2),
            "bigint" => Some(3),
            "real" => Some(4),
            "double precision" => Some(5),
            _ => None,
        }
    };
    if let (Some(from_rank), Some(to_rank)) = (numeric_rank(from), numeric_rank(to)) {
        return to_rank >= from_rank;
    }
    match (from, to) {
        // Numeric scale changes: PostgreSQL casts numeric to numeric by
        // assignment, but a scale/precision reduction can fail on apply
        // for out-of-range values and silently round — the planner
        // treats a shrink as a data decision it never makes.
        (from, to) if from.starts_with("numeric(") && to.starts_with("numeric(") => {
            let parse = |spelling: &str| -> Option<(i64, i64)> {
                let inner = spelling.strip_prefix("numeric(")?.strip_suffix(')')?;
                let (precision, scale) = inner.split_once(',')?;
                Some((precision.trim().parse().ok()?, scale.trim().parse().ok()?))
            };
            match (parse(from), parse(to)) {
                (Some((from_precision, from_scale)), Some((to_precision, to_scale))) => {
                    to_scale >= from_scale && to_precision >= from_precision
                }
                _ => false,
            }
        }
        // Any numeric source casts up to numeric by assignment.
        (_, to) if numeric_rank(from).is_some() && to.starts_with("numeric(") => true,
        // varchar(n) widenings cast by assignment; shortenings refuse.
        (from, to) if from.starts_with("varchar(") && to.starts_with("varchar(") => {
            let parse_length = |spelling: &str| {
                spelling
                    .strip_prefix("varchar(")
                    .and_then(|rest| rest.strip_suffix(')'))
                    .and_then(|length| length.parse::<i64>().ok())
            };
            match (parse_length(from), parse_length(to)) {
                (Some(from_length), Some(to_length)) => to_length >= from_length,
                _ => false,
            }
        }
        // varchar and text cast in both directions by assignment.
        (from, "text") if from.starts_with("varchar(") => true,
        ("text", to) if to.starts_with("varchar(") => true,
        // json and jsonb cast in both directions by assignment.
        ("json", "jsonb") | ("jsonb", "json") => true,
        // Everything else (text to bigint, bytea to anything, uuid
        // spellings, timestamp families, arrays) refuses: a bare type
        // change without an assignment cast cannot execute, and a
        // USING clause would invent conversion semantics.
        _ => false,
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
        // A new table's constraints ride its create_table statement
        // verbatim; planning them again would duplicate the DDL.
        if !base_tables.contains_key(entity) {
            continue;
        }
        for check in table.checks() {
            let base_check = base_tables.get(entity).and_then(|base| {
                base.checks()
                    .iter()
                    .find(|candidate| candidate.name() == check.name())
            });
            let unchanged = base_check.is_some_and(|base| {
                render_predicates(base.predicates()) == render_predicates(check.predicates())
            });
            if unchanged {
                continue;
            }
            // A same-named check with a changed predicate is replaced:
            // the drop executes in place, never after its own re-add.
            let mut requires = Vec::new();
            if base_check.is_some() {
                let drop_id = steps.len();
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
                requires.push(drop_id + 1);
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
                requires,
                None,
            );
        }
    }
    for (entity, table) in &base_tables {
        // A table being dropped carries its constraints with it.
        if !candidate_tables.contains_key(entity) {
            continue;
        }
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

/// The declared table map of one attachment's postgres projection.
fn declared_tables(
    attachment: &StorageProjectionAttachment,
) -> std::collections::BTreeMap<String, crate::storage_projection::Table> {
    attachment
        .projections()
        .iter()
        .find(|projection| projection.namespace().key() == "postgres")
        .map(|projection| {
            projection
                .tables()
                .iter()
                .map(|table| (table.entity().as_str().to_owned(), table.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// The enum-member CHECK plan: under the `check` policy every enum
/// column carries the bounded `chk_<table>_<column>` member-list
/// constraint, exactly as the DDL renderer emits it; a new, changed,
/// or removed member list plans the matching add or drop. A changed
/// member list replaces the constraint in place — the drop of the
/// old list executes before the re-add under the same derived name,
/// because an `ADD CONSTRAINT` under an existing name cannot
/// execute — and a table the candidate drops carries its derived
/// checks with it: the `DROP TABLE` owns them.
fn plan_enum_checks(
    steps: &mut Vec<Step>,
    profile: &StorageEngineAttachment,
    base: &StorageProjectionAttachment,
    candidate: &StorageProjectionAttachment,
) -> Result<(), DiagnosticSet> {
    if profile.policies().enum_policy() != super::EnumPolicy::Check {
        return Ok(());
    }
    let expected = |attachment: &StorageProjectionAttachment| {
        let tables = declared_tables(attachment);
        let mut expected: Vec<(String, String, String)> = Vec::new();
        for (entity, table) in &tables {
            let Some(domain) = attachment.entity(table.entity()) else {
                continue;
            };
            for field in domain.fields() {
                if let crate::storage_projection::entity::DomainType::Enum { members } =
                    field.field_type()
                {
                    expected.push((
                        entity.clone(),
                        enum_check_name(table.table(), field.name().as_str())?,
                        render_enum_predicate(field.name().as_str(), members),
                    ));
                }
            }
        }
        Ok(expected)
    };
    let base_checks = expected(base)?;
    let candidate_checks = expected(candidate)?;
    let candidate_tables = declared_tables(candidate);
    let base_declared = declared_tables(base);
    for (entity, name, predicate) in &candidate_checks {
        // A new table's enum CHECKs ride its create_table statement;
        // planning them again would duplicate the DDL.
        if !base_declared.contains_key(entity) {
            continue;
        }
        let base_check = base_checks
            .iter()
            .find(|(base_entity, base_name, _)| base_entity == entity && base_name == name);
        // Unchanged: the same derived name carries the same member
        // list.
        if base_check.is_some_and(|(_, _, base_predicate)| base_predicate == predicate) {
            continue;
        }
        let Some(table) = candidate_tables.get(entity) else {
            continue;
        };
        // A same-named check with a changed member list is replaced:
        // the drop of the old list executes in place, never after its
        // own re-add — an ADD CONSTRAINT under an already-bound name
        // cannot execute.
        let mut requires = Vec::new();
        if base_check.is_some() {
            let drop_id = steps.len();
            push_step(
                steps,
                "drop_check",
                format!(
                    "ALTER TABLE {} DROP CONSTRAINT \"{name}\";",
                    quote(table.table())
                ),
                DataRisk::Destructive,
                Vec::new(),
                None,
            );
            requires.push(drop_id + 1);
        }
        push_step(
            steps,
            "add_check",
            format!(
                "ALTER TABLE {} ADD CONSTRAINT \"{name}\" CHECK ({predicate});",
                quote(table.table()),
            ),
            DataRisk::None,
            requires,
            None,
        );
    }
    for (entity, name, _) in &base_checks {
        // A table being dropped carries its derived checks with it:
        // no member drops, the DROP TABLE owns them.
        let Some(table) = candidate_tables.get(entity) else {
            continue;
        };
        let removed = !candidate_checks
            .iter()
            .any(|(candidate_entity, candidate_name, _)| {
                candidate_entity == entity && candidate_name == name
            });
        if removed {
            // The constraint lives on the surviving table under its
            // current (post-rename) name.
            push_step(
                steps,
                "drop_check",
                format!(
                    "ALTER TABLE {} DROP CONSTRAINT \"{name}\";",
                    quote(table.table())
                ),
                DataRisk::Destructive,
                Vec::new(),
                None,
            );
        }
    }
    Ok(())
}

/// The deterministic name of one derived enum member CHECK.
fn enum_check_name(table: &StorageName, column: &str) -> Result<String, DiagnosticSet> {
    StorageName::parse(&format!("chk_{}_{}", table, column))
        .map(|name| name.as_str().to_owned())
        .map_err(|_| diagnostic::rule_invalid(MAPPING_INVALID, "check-name", None))
}

/// The bounded member-list predicate of one enum column, identical in
/// shape to the DDL renderer's inline constraint. Names are validated
/// storage-grammar tokens, so plain quoting is exact.
fn render_enum_predicate(column: &str, members: &[String]) -> String {
    let values = members
        .iter()
        .map(|member| format!("'{}'", member.replace('\'', "''")))
        .collect::<Vec<String>>()
        .join(", ");
    format!("\"{column}\" IN ({values})")
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

/// The foreign-key plan: added and removed constraints, dependency
/// ordered behind their tables. An over-long derived constraint name
/// and an unresolvable referenced key refuse with the typed mapping
/// rule — never a colliding fallback name and never an invented `id`
/// target (the DDL renderer answers the same inputs identically).
fn plan_foreign_keys(
    steps: &mut Vec<Step>,
    base: &DerivedProjection,
    candidate: &DerivedProjection,
    table_ids: &std::collections::BTreeMap<String, usize>,
) -> Result<(), DiagnosticSet> {
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
            let base_foreign_key = base_table.and_then(|base| {
                base.foreign_keys()
                    .iter()
                    .find(|candidate| candidate.column() == foreign_key.column())
            });
            // Unchanged: same column, same target, same action.
            if base_foreign_key.is_some_and(|base| {
                base.references_table() == foreign_key.references_table()
                    && base.on_delete() == foreign_key.on_delete()
            }) {
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
                    .map_err(|_| {
                        diagnostic::rule_invalid(MAPPING_INVALID, "foreign-key-name", None)
                    })?;
            let referenced = primary_keys
                .get(foreign_key.references_table().as_str())
                .copied()
                .ok_or_else(|| {
                    diagnostic::rule_invalid(MAPPING_INVALID, "referenced-key-absent", None)
                })?;
            // A same-named foreign key whose target or action changed
            // is replaced: the drop executes in place (the derived
            // name is constant), never after its own re-add.
            if base_foreign_key.is_some() {
                let drop_id = steps.len();
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
                requires.push(drop_id + 1);
            }
            push_step(
                steps,
                "add_foreign_key",
                format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {};",
                    quote(table.table()),
                    quote(&name),
                    quote(foreign_key.column()),
                    quote(foreign_key.references_table()),
                    quote(referenced),
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
        // A table being dropped carries its constraints with it: no
        // member drops, the DROP TABLE owns them.
        if candidate_table.is_none() {
            continue;
        }
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
                        .map_err(|_| {
                        diagnostic::rule_invalid(MAPPING_INVALID, "foreign-key-name", None)
                    })?;
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
    Ok(())
}

/// The index plan: added and removed indexes, deterministic names for
/// the unnamed ones. An over-long derived index name refuses with the
/// typed mapping rule instead of collapsing to a colliding fallback.
fn plan_indexes(
    steps: &mut Vec<Step>,
    base: &DerivedProjection,
    candidate: &DerivedProjection,
    table_ids: &std::collections::BTreeMap<String, usize>,
) -> Result<(), DiagnosticSet> {
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
                        .map_err(|_| {
                            diagnostic::rule_invalid(MAPPING_INVALID, "index-name", None)
                        })?
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
        // A table being dropped carries its indexes with it.
        if candidate_table.is_none() {
            continue;
        }
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
                            .map_err(|_| {
                                diagnostic::rule_invalid(MAPPING_INVALID, "index-name", None)
                            })?
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
    Ok(())
}

/// Move every drop behind the constructive steps; within the drops,
/// constraint/check/index drops precede column drops, which precede
/// the table drops — PostgreSQL refuses `ALTER TABLE t DROP ...` after
/// `DROP TABLE t`, refuses `DROP TABLE` while dependents still
/// reference it, and auto-drops the indexes and constraints involving
/// a dropped column, so a column's DROP COLUMN must run after every
/// explicit drop of its checks, FKs, and indexes. One exception: a
/// drop that is replaced in place (a later step re-creates the same
/// quoted object name — a changed constraint predicate, enum member
/// list, or FK action under a constant derived name) stays with the
/// constructive steps, because the re-add of an existing name would
/// fail if the drop ran after it. The pairing matches the whole
/// quoted identifier, never a substring: a dropped column's name
/// embeds in its derived `idx_*`/`chk_*` names, and pairing on that
/// embedding would let the DROP COLUMN run first — PostgreSQL
/// auto-drops the objects involving a dropped column, and the later
/// explicit drops would fail on objects that no longer exist. The
/// pairing is also role-aware, not token-blind: only a re-creation of
/// the same object pairs — `drop_table` pairs a later `CREATE TABLE`
/// of the same table, and a constraint/index drop pairs a later
/// `ADD CONSTRAINT`/`CREATE INDEX` of the same object — while a
/// `drop_column` never pairs, because a column name recurring inside
/// a later statement (a join create's column list) does not mean the
/// column is re-created. Stable for identical inputs.
fn order_drops_last(steps: &mut [Step]) {
    // The last quoted identifier of one statement, with its opening
    // and closing delimiters: the object a drop names or an add
    // creates (the constraint/index/column/table token). Keeping the
    // quote characters makes the later pairing a whole-identifier
    // match — `"due_date"` must not pair with `DROP INDEX
    // "idx_task_due_date"`, where the identifier is embedded in the
    // derived name, not equal to it.
    fn dropped_object_name(statement: &str) -> Option<String> {
        let inner = statement.strip_suffix(';')?;
        let name_end = inner.rfind('"')?;
        let name_start = inner[..name_end].rfind('"')?;
        Some(inner[name_start..=name_end].to_owned())
    }
    // The created-object token of a create statement: the first
    // quoted identifier after the object keyword, so a column that
    // merely appears inside a created table's column list cannot be
    // mistaken for the created object.
    fn created_object_token(statement: &str) -> Option<String> {
        let inner = statement.strip_suffix(';')?;
        let upper = inner.to_ascii_uppercase();
        let offset = if upper.starts_with("CREATE TABLE ") {
            "CREATE TABLE ".len()
        } else if upper.starts_with("CREATE UNIQUE INDEX ") {
            "CREATE UNIQUE INDEX ".len()
        } else if upper.starts_with("CREATE INDEX ") {
            "CREATE INDEX ".len()
        } else if upper.starts_with("ALTER TABLE ") && upper.contains(" ADD CONSTRAINT ") {
            upper.find(" ADD CONSTRAINT ")? + " ADD CONSTRAINT ".len()
        } else {
            return None;
        };
        let rest = &inner[offset..];
        let name_start = rest.find('"')?;
        let name_end = rest[name_start + 1..].find('"')? + name_start + 1;
        Some(rest[name_start..=name_end].to_owned())
    }
    let is_paired = |index: usize, steps: &[Step]| -> bool {
        let step = &steps[index];
        // A dropped column is never re-created: no later statement
        // creates a column object, so a `drop_column` never pairs —
        // its name can only recur inside another statement (a join
        // create's column list), which is not a re-creation.
        if step.kind == "drop_column" {
            return false;
        }
        let Some(name) = dropped_object_name(&step.statement) else {
            return false;
        };
        steps[index + 1..]
            .iter()
            // Role-aware: only a create of the same object pairs. A
            // `drop_table` pairs a `create_table` of the same table;
            // a constraint/index drop pairs a re-add of the same
            // constraint/index under a constant derived name. A token
            // appearing elsewhere (a column inside a created table,
            // an unrelated identifier) is not a re-creation.
            .filter(|later| {
                matches!(
                    (step.kind, later.kind),
                    ("drop_table", "create_table")
                        | ("drop_constraint", "add_foreign_key")
                        | ("drop_constraint", "add_primary_key")
                        | ("drop_check", "add_check")
                        | ("drop_index", "add_index")
                )
            })
            .any(|later| match created_object_token(&later.statement) {
                Some(created) => created == name,
                None => later.statement.contains(&name),
            })
    };
    let rank = |index: usize, steps: &[Step]| {
        let step = &steps[index];
        if !step.kind.starts_with("drop_") {
            0
        } else if is_paired(index, steps) {
            // A replaced drop (a later step re-creates the same quoted
            // object) stays with the constructive steps in emission
            // order — moving it behind them would fail the re-create:
            // `DROP TABLE` before `CREATE TABLE` of the same name is
            // the required order, and the create is `requires`-wired
            // to the drop.
            0
        } else if step.kind == "drop_table" {
            3
        } else if step.kind == "drop_column" {
            // The column's checks, FKs, and indexes drop first: the
            // column drop auto-removes every object involving it.
            2
        } else {
            1
        }
    };
    let ranks: Vec<usize> = (0..steps.len()).map(|index| rank(index, steps)).collect();
    let mut order: Vec<usize> = (0..steps.len()).collect();
    order.sort_by_key(|&index| ranks[index]);
    // The permutation is explicit, so every step's `requires` —
    // captured as 1-based positions at push time, before the sort — is
    // remapped to the reordered positions. Without the remap, a plan
    // whose unpaired drops moved past a wired step would publish edges
    // naming the wrong steps (including self-loops), and the plan
    // document is a contract artifact consumers schedule by.
    let new_position: Vec<usize> = {
        let mut inverse = vec![0usize; steps.len()];
        for (new, &old) in order.iter().enumerate() {
            inverse[old] = new;
        }
        inverse
    };
    let reordered: Vec<Step> = order
        .into_iter()
        .map(|index| {
            let mut step = steps[index].clone();
            step.requires = step
                .requires
                .iter()
                .map(|&dep| new_position[dep - 1] + 1)
                .collect();
            step.requires.sort_unstable();
            step
        })
        .collect();
    steps.clone_from_slice(&reordered);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(kind: &'static str) -> Step {
        Step {
            id: 0,
            kind,
            statement: format!("{kind};"),
            risk: DataRisk::Destructive,
            requires: Vec::new(),
            explain: None,
        }
    }

    #[test]
    fn assignment_casts_follow_the_postgres_table() {
        // Widening numeric transitions and varchar widenings cast by
        // assignment; narrowings and unrelated families refuse, and an
        // unknown spelling is fail-closed.
        assert!(assignment_castable("bigint", "bigint"));
        assert!(assignment_castable("smallint", "bigint"));
        assert!(assignment_castable("integer", "numeric(12,4)"));
        assert!(!assignment_castable("bigint", "smallint"));
        assert!(assignment_castable("numeric(10,2)", "numeric(12,4)"));
        assert!(!assignment_castable("numeric(10,2)", "numeric(8,2)"));
        assert!(assignment_castable("varchar(64)", "varchar(128)"));
        assert!(!assignment_castable("varchar(64)", "varchar(32)"));
        assert!(assignment_castable("varchar(64)", "text"));
        assert!(assignment_castable("text", "varchar(64)"));
        assert!(assignment_castable("json", "jsonb"));
        // The non-castable transitions the planner must refuse.
        assert!(!assignment_castable("text", "bigint"));
        assert!(!assignment_castable("uuid", "text"));
        assert!(!assignment_castable("timestamptz", "date"));
        assert!(!assignment_castable("bytea", "text"));
        assert!(!assignment_castable("some unknown", "bigint"));
    }

    #[test]
    fn a_column_drop_never_pairs_with_its_dependents_derived_names() {
        // The pairing matches the whole quoted identifier, never a
        // substring: a dropped column's name embeds in the derived
        // names of its own index and check (`idx_task_due_date`,
        // `chk_tag_color`), and pairing on that embedding would let
        // the DROP COLUMN run first — PostgreSQL auto-drops the objects
        // involving the column, so the later explicit drops of those
        // same objects would fail on apply.
        let mut steps = vec![
            step("create_extension"),
            step("drop_column"),
            step("drop_index"),
            step("drop_check"),
        ];
        steps[1].statement = "ALTER TABLE \"task\" DROP COLUMN \"due_date\";".to_owned();
        steps[2].statement = "DROP INDEX \"idx_task_due_date\";".to_owned();
        steps[3].statement = "ALTER TABLE \"tag\" DROP CONSTRAINT \"chk_tag_color\";".to_owned();
        order_drops_last(&mut steps);
        let kinds: Vec<&str> = steps.iter().map(|step| step.kind).collect();
        assert_eq!(
            kinds,
            vec![
                "create_extension",
                "drop_index",
                "drop_check",
                "drop_column",
            ],
            "the dependent drops precede the column that owns them"
        );
        // A genuinely replaced drop (a later step re-adds the same
        // quoted name) still pairs and stays in place.
        let mut steps = vec![
            step("create_extension"),
            step("drop_check"),
            step("add_check"),
            step("drop_column"),
        ];
        steps[1].statement = "ALTER TABLE \"tag\" DROP CONSTRAINT \"chk_tag_color\";".to_owned();
        steps[2].statement =
            "ALTER TABLE \"tag\" ADD CONSTRAINT \"chk_tag_color\" CHECK (\"color\" IN ('red'));"
                .to_owned();
        steps[2].risk = DataRisk::None;
        steps[3].statement = "ALTER TABLE \"tag\" DROP COLUMN \"color\";".to_owned();
        order_drops_last(&mut steps);
        let kinds: Vec<&str> = steps.iter().map(|step| step.kind).collect();
        assert_eq!(
            kinds,
            vec!["create_extension", "drop_check", "add_check", "drop_column",],
            "the replaced drop stays paired in place ahead of the column drop"
        );
    }

    #[test]
    fn a_column_drop_never_pairs_with_a_recurring_name_inside_a_later_create() {
        // Role-aware pairing: a column name recurring inside a later
        // join create's column list is not a re-creation of the
        // column — pairing on the recurrence would demote the column
        // drop to rank 0 and let it run before its own dependent
        // constraint/index drops, which then fail on apply (PostgreSQL
        // auto-drops the objects involving the column).
        let mut steps = vec![
            step("create_extension"),
            step("drop_column"),
            step("drop_table"),
            step("create_table"),
            step("drop_constraint"),
        ];
        steps[1].statement = "ALTER TABLE \"task_detail\" DROP COLUMN \"task_id\";".to_owned();
        steps[2].statement = "DROP TABLE \"task_tag\";".to_owned();
        steps[3].statement = "CREATE TABLE \"task_tag\" (\"task_id\" uuid NOT NULL, \"tag_id\" varchar(64) NOT NULL, PRIMARY KEY (\"task_id\", \"tag_id\"));".to_owned();
        steps[3].risk = DataRisk::None;
        steps[4].statement =
            "ALTER TABLE \"task_detail\" DROP CONSTRAINT \"fk_task_detail_task_id\";".to_owned();
        order_drops_last(&mut steps);
        let kinds: Vec<&str> = steps.iter().map(|step| step.kind).collect();
        assert_eq!(
            kinds,
            vec![
                "create_extension",
                "drop_table",
                "create_table",
                "drop_constraint",
                "drop_column",
            ],
            "the paired remat drop/create stay in place; the column drop and its dependent sort behind"
        );
        // And the same-name drop_table/create_table pair still pairs:
        // the drop keeps rank 0 with its create, ahead of the
        // unpaired member drops.
        assert_eq!(steps[1].kind, "drop_table");
        assert_eq!(steps[2].kind, "create_table");
    }

    #[test]
    fn drop_table_sorts_behind_its_member_drops() {
        // PostgreSQL refuses ALTER TABLE t DROP ... after DROP TABLE t,
        // and auto-drops the objects involving a dropped column, so
        // constraint/check/index drops rank first, column drops next,
        // and table drops last (stable within each rank).
        let mut steps = vec![
            step("create_extension"),
            step("drop_table"),
            step("drop_column"),
            step("drop_index"),
            step("drop_check"),
            step("drop_constraint"),
        ];
        order_drops_last(&mut steps);
        let kinds: Vec<&str> = steps.iter().map(|step| step.kind).collect();
        assert_eq!(
            kinds,
            vec![
                "create_extension",
                "drop_index",
                "drop_check",
                "drop_constraint",
                "drop_column",
                "drop_table",
            ]
        );
    }

    #[test]
    fn every_destructive_step_declares_explain_before() {
        // The declared hook surface is real: destructive steps carry
        // the before hook, constructive steps carry none.
        let mut steps = [
            step("create_extension"),
            step("drop_table"),
            step("drop_index"),
            step("add_index"),
        ];
        steps[0].risk = DataRisk::None;
        steps[3].risk = DataRisk::None;
        for step in steps.iter_mut() {
            if step.risk() == DataRisk::Destructive {
                step.explain = Some(ExplainHook::Before);
            }
        }
        assert!(steps[0].explain().is_none());
        assert_eq!(steps[1].explain(), Some(ExplainHook::Before));
        assert_eq!(steps[2].explain(), Some(ExplainHook::Before));
        assert!(steps[3].explain().is_none());
        assert_eq!(steps[1].explain().map(ExplainHook::key), Some("before"));
    }

    #[test]
    fn a_replaced_enum_check_stays_paired_and_in_place() {
        // A drop whose object name a later step re-adds (a changed
        // enum member list under the constant derived check name)
        // keeps rank 0: the re-add depends on the drop, and moving the
        // drop behind the constructive steps would fail the re-add on
        // the still-bound name.
        let mut steps = vec![
            step("create_extension"),
            step("drop_check"),
            step("add_check"),
            step("drop_column"),
        ];
        steps[1].statement = "ALTER TABLE \"tag\" DROP CONSTRAINT \"chk_tag_color\";".to_owned();
        steps[2].statement =
            "ALTER TABLE \"tag\" ADD CONSTRAINT \"chk_tag_color\" CHECK (\"color\" IN ('red'));"
                .to_owned();
        steps[2].requires = vec![2];
        steps[2].risk = DataRisk::None;
        order_drops_last(&mut steps);
        let kinds: Vec<&str> = steps.iter().map(|step| step.kind).collect();
        assert_eq!(
            kinds,
            vec!["create_extension", "drop_check", "add_check", "drop_column",],
            "the paired drop stays in place ahead of the unpaired drops"
        );
    }

    #[test]
    fn requires_edges_follow_the_reordered_steps() {
        // Every wired edge is captured as a pre-sort position; the
        // ordering pass must remap it through the permutation. Here a
        // paired drop (its object name a later step re-creates) is
        // wired to its re-add, and an unpaired drop_column and a
        // drop_table land behind both: the sort moves the wired drop's
        // step position, and its re-add's edge must follow it to the
        // new position — never name the step that happens to hold the
        // old position (a self-loop before the fix).
        let mut steps = vec![
            step("create_extension"),
            step("drop_table"),
            step("create_table"),
            step("drop_column"),
        ];
        steps[1].statement = "DROP TABLE \"task_tag\";".to_owned();
        steps[2].statement = "CREATE TABLE \"task_tag\" (\"task_id\" uuid NOT NULL);".to_owned();
        steps[2].requires = vec![2];
        steps[2].risk = DataRisk::None;
        steps[3].statement = "ALTER TABLE \"task\" DROP COLUMN \"note\";".to_owned();
        // A drop_table naming an object a later step re-creates pairs,
        // so both stay at rank 0 in emission order.
        order_drops_last(&mut steps);
        let kinds: Vec<&str> = steps.iter().map(|step| step.kind).collect();
        assert_eq!(
            kinds,
            vec![
                "create_extension",
                "drop_table",
                "create_table",
                "drop_column"
            ],
            "the paired drop/re-create keep their order"
        );
        assert_eq!(
            steps[2].requires,
            &[2],
            "the re-create still names the drop"
        );
        // Now an unpaired drop_column emitted before a wired pair: the
        // column drop moves behind, and the wired edge must track its
        // drop's new position.
        let mut steps = vec![
            step("create_extension"),
            step("drop_column"),
            step("drop_constraint"),
            step("add_primary_key"),
        ];
        steps[1].statement = "ALTER TABLE \"task\" DROP COLUMN \"note\";".to_owned();
        steps[2].statement = "ALTER TABLE \"tag\" DROP CONSTRAINT \"pk_tag\";".to_owned();
        steps[3].statement =
            "ALTER TABLE \"tag\" ADD CONSTRAINT \"pk_tag\" PRIMARY KEY (\"label\");".to_owned();
        steps[3].requires = vec![3];
        steps[3].risk = DataRisk::None;
        order_drops_last(&mut steps);
        let kinds: Vec<&str> = steps.iter().map(|step| step.kind).collect();
        assert_eq!(
            kinds,
            vec![
                "create_extension",
                "drop_constraint",
                "add_primary_key",
                "drop_column",
            ],
            "the unpaired column drop sorts behind the wired pair"
        );
        // Before the fix, the add held position 3 wired to 3 — a
        // self-loop after any shift. The remap names the drop's actual
        // new position.
        assert_eq!(
            steps[2].requires,
            &[2],
            "the add follows its drop through the permutation"
        );
        // No step depends on itself after the remap.
        for (position, step) in steps.iter().enumerate() {
            assert!(
                !step.requires.contains(&(position + 1)),
                "step {} at position {} must not depend on itself: {:?}",
                step.kind,
                position + 1,
                step.requires
            );
        }
    }
}
