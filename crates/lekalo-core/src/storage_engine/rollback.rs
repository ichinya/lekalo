//! Rollback classification and the typed reverse plan (issue #57).
//!
//! Every planned step is classified from its closed kind — never by
//! parsing SQL — into one of three classes:
//!
//! - [`RollbackClass::Reversible`]: the inverse restores the schema
//!   exactly; [`reverse_statement`] renders it when the inverse is
//!   fully determined by the kind and the step's quoted object token
//!   (the same token discipline the dependency-ordering pass uses).
//! - [`RollbackClass::DataLossOnRollback`]: the inverse statement is
//!   renderable but destroys data created since the migration
//!   applied; `down()` exists only with explicit rollback permission.
//! - [`RollbackClass::Irreversible`]: no faithful inverse exists
//!   (dropped data, written business values, unpublishable type
//!   history); any `down()` refuses before the first statement.
//!
//! Schema reversibility is deliberately not data recovery: restoring
//! a dropped column never returns its rows, so `drop_*` classifies as
//! irreversible even though the DDL text would be easy to fake.

use serde::Serialize;

use super::migration::Step;

/// The closed rollback classification of one step.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub enum RollbackClass {
    /// The inverse restores the schema exactly.
    Reversible,
    /// The inverse applies but drops data created since.
    DataLossOnRollback,
    /// No faithful inverse exists; `down()` refuses.
    Irreversible,
}

impl RollbackClass {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Reversible => "reversible",
            Self::DataLossOnRollback => "data-loss-on-rollback",
            Self::Irreversible => "irreversible",
        }
    }
}

/// Classify one planned step from its closed kind and declared risk.
/// Pure and total: every step classifies.
pub fn classify(step: &Step) -> RollbackClass {
    match step.kind() {
        // Drops lose the object and its history: the inverse text
        // would recreate an empty shell, never the object's state.
        "drop_table" | "drop_column" | "drop_sequence" | "drop_index"
        | "drop_constraint" | "drop_check" | "drop_policy" => RollbackClass::Irreversible,
        // A backfill writes business values; no inverse restores the
        // pre-migration rows.
        "backfill" => RollbackClass::Irreversible,
        // Enum type alteration is a one-way value union in v1: the
        // added member cannot be unpublished without the type's full
        // usage history, which the plan does not carry.
        "alter_enum_type" => RollbackClass::Irreversible,
        // Creates: the inverse drops data rows created since the
        // migration applied, so the rollback loses data even though
        // the schema restores exactly.
        "create_table" | "add_column" | "create_sequence" | "create_enum_type"
        | "create_extension" => RollbackClass::DataLossOnRollback,
        // A type change may narrow or rewrite values in place; the
        // forward step's declared destructive risk marks it.
        "alter_column_type" => RollbackClass::DataLossOnRollback,
        // Everything else restores exactly: renames, constraint and
        // index re-derivations, RLS and policy toggles, ownership
        // bindings.
        _ => RollbackClass::Reversible,
    }
}

/// The quoted object token of one statement: the last `"…"` pair for
/// a drop-shaped statement, the first after the object keyword
/// otherwise — exactly the token discipline the ordering pass uses,
/// never a grammar parse.
fn object_token(statement: &str) -> Option<String> {
    let inner = statement.strip_suffix(';')?;
    let name_end = inner.rfind('"')?;
    let name_start = inner[..name_end].rfind('"')?;
    Some(inner[name_start..=name_end].to_owned())
}

/// Render the inverse statement of one reversible step whose inverse
/// is fully determined by its kind and its quoted object token.
/// `None` means "the plan cannot render a faithful inverse" — the
/// emitter then refuses `down()` for this step instead of guessing.
pub fn reverse_statement(step: &Step) -> Option<String> {
    if classify(step) != RollbackClass::Reversible {
        return None;
    }
    let name = object_token(step.statement())?;
    let statement = match step.kind() {
        // ALTER TABLE "a" RENAME TO "b" inverts to RENAME TO "a": the
        // inverse's target is the last token (the new name), the
        // source is the second-to-last quoted token (the old name).
        "rename_table" | "rename_sequence" => {
            let inner = step.statement().strip_suffix(';')?;
            let new_end = inner.rfind('"')?;
            let new_start = inner[..new_end].rfind('"')?;
            let head = &inner[..new_start];
            let old_end = head.rfind('"')?;
            let old_start = head[..old_end].rfind('"')?;
            let verb = if step.kind() == "rename_table" {
                "ALTER TABLE"
            } else {
                "ALTER SEQUENCE"
            };
            format!(
                "{verb} {} RENAME TO {};",
                &inner[new_start..=new_end],
                &inner[old_start..=old_end],
            )
        }
        "add_primary_key" | "add_unique" | "add_foreign_key" => {
            format!("ALTER TABLE {} DROP CONSTRAINT {};", table_token(step), name)
        }
        "add_check" => format!("ALTER TABLE {} DROP CONSTRAINT {};", table_token(step), name),
        "add_index" => {
            // The index name is the created object: the first quoted
            // token after the CREATE keyword (a column name trailing
            // the statement is not the index).
            let inner = step.statement().strip_suffix(';')?;
            let upper = inner.to_ascii_uppercase();
            let offset = if upper.starts_with("CREATE UNIQUE INDEX ") {
                "CREATE UNIQUE INDEX ".len()
            } else {
                "CREATE INDEX ".len()
            };
            let rest = &inner[offset..];
            let start = rest.find('"')?;
            let end = rest[start + 1..].find('"')? + start + 1;
            format!("DROP INDEX {};", &rest[start..=end])
        }
        "set_column_null" => {
            // The forward set is always SET NOT NULL in v1; the
            // inverse relaxes it. The statement shape is fixed:
            // ALTER TABLE "t" ALTER COLUMN "c" SET NOT NULL;
            let inner = step.statement().strip_suffix(';')?;
            let lower = inner.to_ascii_uppercase();
            let alter = lower.find(" ALTER COLUMN ")?;
            let column = inner[alter + " ALTER COLUMN ".len()..].trim();
            format!(
                "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL;",
                table_token(step),
                column
            )
        }
        "enable_rls" | "disable_rls" => {
            let verb = if step.kind() == "enable_rls" {
                "DISABLE"
            } else {
                "ENABLE"
            };
            format!("ALTER TABLE {} {} ROW LEVEL SECURITY;", name, verb)
        }
        "create_policy" => format!("DROP POLICY {} ON {};", name, table_token(step)),
        "alter_sequence" => {
            // OWNED BY inverts to removing the ownership binding: the
            // sequence token is the first quoted name.
            let inner = step.statement().strip_suffix(';')?;
            let start = inner.find('"')?;
            let end = inner[start + 1..].find('"')? + start + 1;
            format!("ALTER SEQUENCE {} OWNED BY NONE;", &inner[start..=end])
        }
        "rename_constraint" => {
            // The forward renames old → new; the inverse renames the
            // last token back to the second-to-last.
            let inner = step.statement().strip_suffix(';')?;
            let new_end = inner.rfind('"')?;
            let new_start = inner[..new_end].rfind('"')?;
            let head = &inner[..new_start];
            let old_end = head.rfind('"')?;
            let old_start = head[..old_end].rfind('"')?;
            let table_start = inner.find('"')?;
            let table_end = inner[table_start + 1..].find('"')? + table_start + 1;
            format!(
                "ALTER TABLE {} RENAME CONSTRAINT {} TO {};",
                &inner[table_start..=table_end],
                &inner[new_start..=new_end],
                &inner[old_start..=old_end],
            )
        }
        _ => return None,
    };
    Some(statement)
}

/// The quoted table token of an ALTER TABLE statement: the first
/// quoted identifier.
fn table_token(step: &Step) -> &str {
    let inner = step.statement().strip_suffix(';').unwrap_or(step.statement());
    let start = inner.find('"').unwrap_or(0);
    let rest = &inner[start + 1..];
    let end = rest.find('"').map(|offset| start + 1 + offset).unwrap_or(start);
    &inner[start..=end]
}

/// The typed reverse plan: the inverse statements of every step whose
/// rollback is renderable, in reverse dependency order. A plan with
/// any non-reversible step yields `None` per step; the emitter
/// refuses `down()` unless *every* step carries an inverse.
pub fn build_reverse_plan(steps: &[Step]) -> Vec<Option<String>> {
    let mut ordered: Vec<&Step> = steps.iter().collect();
    ordered.reverse();
    ordered
        .iter()
        .map(|step| reverse_statement(step))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_projection::DataRisk;

    fn step(kind: &'static str, statement: &str, risk: DataRisk) -> Step {
        Step {
            id: 1,
            kind,
            statement: statement.to_owned(),
            risk,
            requires: Vec::new(),
            explain: None,
        }
    }

    #[test]
    fn drops_classify_irreversible_even_when_text_recovers() {
        let drop = step(
            "drop_column",
            "ALTER TABLE \"task\" DROP COLUMN \"due_date\";",
            DataRisk::Destructive,
        );
        assert_eq!(classify(&drop), RollbackClass::Irreversible);
        assert!(reverse_statement(&drop).is_none());
    }

    #[test]
    fn a_reversible_index_inverts_to_its_drop() {
        let add = step(
            "add_index",
            "CREATE INDEX \"idx_task_due\" ON \"task\" (\"due_date\");",
            DataRisk::None,
        );
        assert_eq!(classify(&add), RollbackClass::Reversible);
        assert_eq!(
            reverse_statement(&add).expect("inverse"),
            "DROP INDEX \"idx_task_due\";"
        );
    }

    #[test]
    fn a_table_create_loses_data_on_rollback() {
        let create = step(
            "create_table",
            "CREATE TABLE \"task_tag\" (\"tag_id\" uuid NOT NULL);",
            DataRisk::None,
        );
        assert_eq!(classify(&create), RollbackClass::DataLossOnRollback);
        assert!(reverse_statement(&create).is_none());
    }

    #[test]
    fn a_rename_inverts_exactly() {
        let rename = step(
            "rename_table",
            "ALTER TABLE \"focus_session\" RENAME TO \"session\";",
            DataRisk::Destructive,
        );
        assert_eq!(classify(&rename), RollbackClass::Reversible);
        assert_eq!(
            reverse_statement(&rename).expect("inverse"),
            "ALTER TABLE \"session\" RENAME TO \"focus_session\";"
        );
    }

    #[test]
    fn the_reverse_plan_runs_in_reverse_order_and_refuses_unsafe_steps() {
        let add = step(
            "add_index",
            "CREATE INDEX \"idx_a\" ON \"task\" (\"x\");",
            DataRisk::None,
        );
        let drop = step(
            "drop_column",
            "ALTER TABLE \"task\" DROP COLUMN \"y\";",
            DataRisk::Destructive,
        );
        let reverse = build_reverse_plan(&[add, drop]);
        assert_eq!(reverse.len(), 2);
        assert!(reverse[0].is_none(), "the drop refuses");
        assert!(reverse[1].is_some(), "the add inverts");
    }
}
