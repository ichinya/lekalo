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
        "drop_table" | "drop_column" | "drop_sequence" | "drop_index" | "drop_constraint"
        | "drop_check" | "drop_policy" => RollbackClass::Irreversible,
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
        // A column default change restores the schema but silently
        // rewrites the derived values of rows inserted or updated
        // while the forward default was live: the plan carries no
        // pre-change default value, so no faithful inverse exists.
        // (The plan v1 contract also renders a reversible step's
        // inverse inline; without one the whole document refuses.)
        "set_column_default" => RollbackClass::Irreversible,
        // Everything else restores exactly: renames (a validated
        // column rename swaps two names exactly like a table rename),
        // constraint and index re-derivations, RLS and policy
        // toggles, ownership bindings.
        _ => RollbackClass::Reversible,
    }
}

/// The quoted object token of one statement, per statement family:
/// for a `CREATE POLICY` the first quoted identifier (the policy
/// name), for an `ADD CONSTRAINT` the constraint name after the ADD
/// keyword, for a `CREATE INDEX` the index name after the CREATE
/// keyword, and for every other family the last quoted pair — the
/// same discipline the ordering pass uses, never a grammar parse.
fn object_token(statement: &str) -> Option<String> {
    let inner = statement.strip_suffix(';')?;
    let upper = inner.to_ascii_uppercase();
    if upper.starts_with("CREATE POLICY ") {
        let rest = &inner["CREATE POLICY ".len()..];
        let start = rest.find('"')?;
        let end = rest[start + 1..].find('"')? + start + 1;
        return Some(rest[start..=end].to_owned());
    }
    if upper.starts_with("CREATE UNIQUE INDEX ") || upper.starts_with("CREATE INDEX ") {
        let offset = if upper.starts_with("CREATE UNIQUE INDEX ") {
            "CREATE UNIQUE INDEX ".len()
        } else {
            "CREATE INDEX ".len()
        };
        let rest = &inner[offset..];
        let start = rest.find('"')?;
        let end = rest[start + 1..].find('"')? + start + 1;
        return Some(rest[start..=end].to_owned());
    }
    if let Some(position) = upper.find(" ADD CONSTRAINT ") {
        let rest = &inner[position + " ADD CONSTRAINT ".len()..];
        let start = rest.find('"')?;
        let end = rest[start + 1..].find('"')? + start + 1;
        return Some(rest[start..=end].to_owned());
    }
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
        "add_primary_key" | "add_unique" | "add_foreign_key" | "add_check" => {
            // The constraint name is the created object: the quoted
            // identifier after ADD CONSTRAINT (an add_check's tail
            // names its columns, not the constraint).
            format!(
                "ALTER TABLE {} DROP CONSTRAINT {};",
                table_token(step),
                object_token(step.statement())?
            )
        }
        "add_index" => {
            // The index name is the created object: the first quoted
            // token after the CREATE keyword (a column name trailing
            // the statement is not the index).
            format!("DROP INDEX {};", object_token(step.statement())?)
        }
        "set_column_null" => {
            // The inverse carries the exact forward shape: a forward
            // SET NOT NULL relaxes (DROP NOT NULL), a forward
            // DROP NOT NULL restores the constraint (SET NOT NULL).
            // The column token is the second quoted identifier of
            // the fixed-shape statement:
            // ALTER TABLE "t" ALTER COLUMN "c" SET NOT NULL;
            let inner = step.statement().strip_suffix(';')?;
            let start = inner.find('"')?;
            let table_end = inner[start + 1..].find('"')? + start + 1;
            let column_start = inner[table_end + 1..].find('"')? + table_end + 1;
            let column_end = inner[column_start + 1..].find('"')? + column_start + 1;
            let verb = if inner[column_end + 1..]
                .to_ascii_uppercase()
                .contains("DROP NOT NULL")
            {
                "SET NOT NULL"
            } else {
                "DROP NOT NULL"
            };
            format!(
                "ALTER TABLE {} ALTER COLUMN {} {verb};",
                &inner[start..=table_end],
                &inner[column_start..=column_end],
            )
        }
        "enable_rls" | "disable_rls" => {
            // The inverse mirrors the exact forward mode: ENABLE
            // inverts to DISABLE, FORCE to NO FORCE, and a DISABLE
            // (dropping a FORCE or plain state) inverts to plain
            // ENABLE. The current verb is the statement's tail.
            let forward = inner_rls_mode(step.statement());
            let verb = match forward {
                Some("ENABLE") => "DISABLE",
                Some("FORCE") => "NO FORCE",
                _ => "ENABLE",
            };
            format!("ALTER TABLE {} {} ROW LEVEL SECURITY;", name, verb)
        }
        "create_policy" => {
            // Both tokens matter: the policy name is the created
            // object (the first quoted identifier) and the target
            // table is the quoted identifier after ON. The policy
            // must drop by its own name on the right table, never by
            // a column token.
            let inner = step.statement().strip_suffix(';')?;
            let upper = inner.to_ascii_uppercase();
            let position = upper.find(" ON ")?;
            let tail = &inner[position + " ON ".len()..];
            let table_start = tail.find('"')?;
            let table_end = tail[table_start + 1..].find('"')? + table_start + 1;
            format!(
                "DROP POLICY {} ON {};",
                object_token(step.statement())?,
                &tail[table_start..=table_end]
            )
        }
        "alter_sequence" => {
            // OWNED BY inverts to removing the ownership binding: the
            // sequence token is the first quoted name.
            let inner = step.statement().strip_suffix(';')?;
            let start = inner.find('"')?;
            let end = inner[start + 1..].find('"')? + start + 1;
            format!("ALTER SEQUENCE {} OWNED BY NONE;", &inner[start..=end])
        }
        "rename_column" => {
            // ALTER TABLE "t" RENAME COLUMN "a" TO "b" inverts to
            // RENAME COLUMN "b" TO "a": the same swap discipline as a
            // table rename — the new name is the last quoted token,
            // the old name the second-to-last, the table the first.
            let inner = step.statement().strip_suffix(';')?;
            let new_end = inner.rfind('"')?;
            let new_start = inner[..new_end].rfind('"')?;
            let head = &inner[..new_start];
            let old_end = head.rfind('"')?;
            let old_start = head[..old_end].rfind('"')?;
            format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {};",
                table_token(step),
                &inner[new_start..=new_end],
                &inner[old_start..=old_end],
            )
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
    let inner = step
        .statement()
        .strip_suffix(';')
        .unwrap_or(step.statement());
    let start = inner.find('"').unwrap_or(0);
    let rest = &inner[start + 1..];
    let end = rest
        .find('"')
        .map(|offset| start + 1 + offset)
        .unwrap_or(start);
    &inner[start..=end]
}

/// The RLS mode verb an ALTER TABLE … <verb> ROW LEVEL SECURITY
/// statement carries: ENABLE, DISABLE, FORCE, or NO FORCE.
fn inner_rls_mode(statement: &str) -> Option<&'static str> {
    let inner = statement.strip_suffix(';')?;
    let upper = inner.to_ascii_uppercase();
    if upper.contains(" NO FORCE ROW LEVEL SECURITY") {
        Some("NO FORCE")
    } else if upper.contains(" FORCE ROW LEVEL SECURITY") {
        Some("FORCE")
    } else if upper.contains(" DISABLE ROW LEVEL SECURITY") {
        Some("DISABLE")
    } else if upper.contains(" ENABLE ROW LEVEL SECURITY") {
        Some("ENABLE")
    } else {
        None
    }
}

/// The typed reverse plan: the inverse statements of every step whose
/// rollback is renderable, in reverse dependency order. A plan with
/// any non-reversible step yields `None` per step; the emitter
/// refuses `down()` unless *every* step carries an inverse.
pub fn build_reverse_plan(steps: &[Step]) -> Vec<Option<String>> {
    let mut ordered: Vec<&Step> = steps.iter().collect();
    ordered.reverse();
    ordered.iter().map(|step| reverse_statement(step)).collect()
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
    fn a_default_change_refuses_instead_of_faking_an_inverse() {
        // The catch-all must not mark a default change Reversible:
        // there is no renderable inverse, and a reversible step
        // without one would make the whole input document
        // un-emittable at the adapter boundary.
        let set_default = step(
            "set_column_default",
            "ALTER TABLE \"task\" ALTER COLUMN \"minutes\" SET DEFAULT 0;",
            DataRisk::None,
        );
        assert_eq!(classify(&set_default), RollbackClass::Irreversible);
        assert!(reverse_statement(&set_default).is_none());
        let drop_default = step(
            "set_column_default",
            "ALTER TABLE \"task\" ALTER COLUMN \"minutes\" DROP DEFAULT;",
            DataRisk::None,
        );
        assert_eq!(classify(&drop_default), RollbackClass::Irreversible);
        assert!(reverse_statement(&drop_default).is_none());
    }

    #[test]
    fn a_column_rename_inverts_to_the_exact_swap() {
        // A validated column rename swaps two names exactly, the same
        // discipline as a table rename: the inverse renames the new
        // name back to the old one on the same table.
        let rename = step(
            "rename_column",
            "ALTER TABLE \"task\" RENAME COLUMN \"title\" TO \"summary\";",
            DataRisk::Destructive,
        );
        assert_eq!(classify(&rename), RollbackClass::Reversible);
        assert_eq!(
            reverse_statement(&rename).expect("inverse"),
            "ALTER TABLE \"task\" RENAME COLUMN \"summary\" TO \"title\";"
        );
    }

    #[test]
    fn constraint_inverses_target_the_constraint_not_a_column() {
        // ADD CONSTRAINT "fk_…" FOREIGN KEY ("focus_task_id") …: the
        // last quoted pair is a column name; the inverse must drop
        // the constraint the statement created.
        let add_fk = step(
            "add_foreign_key",
            "ALTER TABLE \"session\" ADD CONSTRAINT \"fk_session_focus_task_id\" FOREIGN KEY (\"focus_task_id\") REFERENCES \"task\"(\"id\") ON DELETE RESTRICT;",
            DataRisk::None,
        );
        assert_eq!(
            reverse_statement(&add_fk).expect("inverse"),
            "ALTER TABLE \"session\" DROP CONSTRAINT \"fk_session_focus_task_id\";"
        );
        let add_check = step(
            "add_check",
            "ALTER TABLE \"task\" ADD CONSTRAINT \"chk_task_minutes\" CHECK (\"minutes\" >= 0);",
            DataRisk::None,
        );
        assert_eq!(
            reverse_statement(&add_check).expect("inverse"),
            "ALTER TABLE \"task\" DROP CONSTRAINT \"chk_task_minutes\";"
        );
        let add_pk = step(
            "add_primary_key",
            "ALTER TABLE \"session\" ADD CONSTRAINT \"pk_session\" PRIMARY KEY (\"id\");",
            DataRisk::Destructive,
        );
        assert_eq!(
            reverse_statement(&add_pk).expect("inverse"),
            "ALTER TABLE \"session\" DROP CONSTRAINT \"pk_session\";"
        );
    }

    #[test]
    fn a_policy_inverse_names_the_policy_on_its_table() {
        let create = step(
            "create_policy",
            "CREATE POLICY \"pol_session_tenant\" ON \"session\" USING (\"tenant_id\" = current_setting('lekalo.tenant_id')::uuid);",
            DataRisk::None,
        );
        assert_eq!(
            reverse_statement(&create).expect("inverse"),
            "DROP POLICY \"pol_session_tenant\" ON \"session\";"
        );
    }

    #[test]
    fn dropping_not_null_inverts_to_setting_it() {
        let drop_not_null = step(
            "set_column_null",
            "ALTER TABLE \"task\" ALTER COLUMN \"note\" DROP NOT NULL;",
            DataRisk::None,
        );
        assert_eq!(
            reverse_statement(&drop_not_null).expect("inverse"),
            "ALTER TABLE \"task\" ALTER COLUMN \"note\" SET NOT NULL;"
        );
        let set_not_null = step(
            "set_column_null",
            "ALTER TABLE \"task\" ALTER COLUMN \"minutes\" SET NOT NULL;",
            DataRisk::BackfillRequired,
        );
        assert_eq!(
            reverse_statement(&set_not_null).expect("inverse"),
            "ALTER TABLE \"task\" ALTER COLUMN \"minutes\" DROP NOT NULL;"
        );
    }

    #[test]
    fn forced_rls_inverts_to_no_force() {
        let force = step(
            "enable_rls",
            "ALTER TABLE \"task\" FORCE ROW LEVEL SECURITY;",
            DataRisk::None,
        );
        assert_eq!(
            reverse_statement(&force).expect("inverse"),
            "ALTER TABLE \"task\" NO FORCE ROW LEVEL SECURITY;"
        );
        let enable = step(
            "enable_rls",
            "ALTER TABLE \"task\" ENABLE ROW LEVEL SECURITY;",
            DataRisk::None,
        );
        assert_eq!(
            reverse_statement(&enable).expect("inverse"),
            "ALTER TABLE \"task\" DISABLE ROW LEVEL SECURITY;"
        );
        let disable = step(
            "disable_rls",
            "ALTER TABLE \"task\" DISABLE ROW LEVEL SECURITY;",
            DataRisk::None,
        );
        assert_eq!(
            reverse_statement(&disable).expect("inverse"),
            "ALTER TABLE \"task\" ENABLE ROW LEVEL SECURITY;"
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
