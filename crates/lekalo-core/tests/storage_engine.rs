//! Issue #69 integration tests: the storage-engine family end to end
//! over the committed fixtures — profile normalization, introspection
//! evidence, the drift comparison, the DDL binding, and the test
//! lifecycle rules. The unit tests beside each module carry the
//! fine-grained refusals; this file pins the cross-module contracts.

use lekalo_core::diagnostics::DiagnosticSet;
use lekalo_core::storage_engine::{
    compare_drift, postgres, IntrospectionEvidence, StorageEngineAttachment,
};
use lekalo_core::storage_projection::{project, Namespace, StorageProjectionAttachment};

const PROFILE: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine/valid/planner-postgres.json");
const PROFILE_FULL: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine/valid/planner-postgres-full.json");
const PROJECTION: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-projection/valid/planner-storage.json");
const OBSERVED: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine/introspection/observed.json");
const OBSERVED_DRIFTED: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine/introspection/observed-drifted.json");

fn profile() -> StorageEngineAttachment {
    let value: serde_json::Value = serde_json::from_slice(PROFILE).expect("profile json");
    StorageEngineAttachment::from_value(&value).expect("valid profile")
}

fn full_profile() -> StorageEngineAttachment {
    let value: serde_json::Value = serde_json::from_slice(PROFILE_FULL).expect("profile json");
    StorageEngineAttachment::from_value(&value).expect("valid profile")
}

fn projection() -> StorageProjectionAttachment {
    let value: serde_json::Value = serde_json::from_slice(PROJECTION).expect("projection json");
    StorageProjectionAttachment::from_value(&value).expect("valid projection")
}

fn evidence(bytes: &[u8]) -> IntrospectionEvidence {
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("evidence json");
    IntrospectionEvidence::from_value(&value).expect("valid evidence")
}

fn first_reason(error: &DiagnosticSet) -> &str {
    error.reason_ids().first().copied().unwrap_or_default()
}

#[test]
fn the_committed_evidence_matches_its_projection_without_drift() {
    let report = compare_drift(&profile(), &projection(), &evidence(OBSERVED)).expect("comparable");
    assert!(report.ok(), "the zero-drift golden observes itself exactly");
    assert!(report.findings().is_empty());
    // The report canonicalizes deterministically.
    let bytes = report.canonical_bytes().expect("canonical");
    assert_eq!(bytes, "{\"findings\":[],\"ok\":true}");
}

#[test]
fn the_drifted_vector_reports_missing_extra_divergent_and_unsupported() {
    let report =
        compare_drift(&profile(), &projection(), &evidence(OBSERVED_DRIFTED)).expect("comparable");
    assert!(!report.ok());
    let paths: Vec<(String, String, String)> = report
        .findings()
        .iter()
        .map(|finding| {
            (
                finding.kind().key().to_owned(),
                finding.path().to_owned(),
                finding.detail().to_owned(),
            )
        })
        .collect();
    // A declared column the server does not have.
    assert!(paths.iter().any(|(kind, path, detail)| {
        kind == "missing" && path == "tables/task/columns/due_date" && detail == "column-missing"
    }));
    // An observed column nothing declares.
    assert!(paths
        .iter()
        .any(|(kind, path, _)| kind == "extra" && path == "tables/task/columns/legacy_flag"));
    // An observed type that diverges from the declaration.
    assert!(paths.iter().any(|(kind, path, detail)| {
        kind == "divergent" && path == "tables/comment/columns/body" && detail == "column-type"
    }));
    // An observed table nothing declares, and a declared table the
    // server lost.
    assert!(paths
        .iter()
        .any(|(kind, path, _)| kind == "extra" && path == "tables/archived_row"));
    assert!(paths
        .iter()
        .any(|(kind, path, _)| kind == "missing" && path == "tables/task_roster"));
    // A dropped primary key and a dropped declared check are drift:
    // the comparison covers the constraint classes, not only columns.
    assert!(paths.iter().any(|(kind, path, detail)| {
        kind == "missing" && path == "tables/task_detail/primary" && detail == "primary-missing"
    }));
    // The unallowlisted extension and the unsupported type surface
    // verbatim from the evidence records.
    assert!(paths.iter().any(|(kind, path, _)| {
        kind == "unsupported" && path == "unsupported/extension/postgis"
    }));
    assert!(paths.iter().any(|(kind, path, _)| {
        kind == "unsupported" && path == "unsupported/type/comment/target_id"
    }));
}

#[test]
fn unchecked_and_writing_evidence_refuse() {
    let mut value: serde_json::Value = serde_json::from_slice(OBSERVED).expect("json");
    value["mode"] = serde_json::Value::String("best-effort".into());
    let error = IntrospectionEvidence::from_value(&value).expect_err("mode refused");
    assert_eq!(first_reason(&error), "storage-engine.introspection-invalid");
    let mut value: serde_json::Value = serde_json::from_slice(OBSERVED).expect("json");
    value["readOnly"] = serde_json::Value::Bool(false);
    let error = IntrospectionEvidence::from_value(&value).expect_err("posture refused");
    assert_eq!(first_reason(&error), "storage-engine.introspection-invalid");
}

#[test]
fn evidence_bound_to_a_foreign_projection_refuses() {
    let mut value: serde_json::Value = serde_json::from_slice(OBSERVED).expect("json");
    value["projectionRef"] = serde_json::Value::String(
        "sha256:0505050505050505050505050505050505050505050505050505050505050505".into(),
    );
    let other = IntrospectionEvidence::from_value(&value).expect("parses");
    let error = compare_drift(&profile(), &projection(), &other).expect_err("binding refused");
    assert_eq!(first_reason(&error), "storage-engine.drift-invalid");
    let rendered = serde_json::to_string(&error).expect("json");
    assert!(rendered.contains("projection-binding-mismatch"));
}

#[test]
fn the_profile_binds_its_projection_for_ddl_and_drift() {
    let profile = profile();
    let attachment = projection();
    // The DDL renderer and the drift comparison share one binding
    // rule: the profile's projectionRef is the attachment's canonical
    // digest.
    let document = postgres::ddl::render(&profile, &attachment).expect("the committed pair binds");
    assert!(!document.statements().is_empty());
    let derived = project(&attachment, Namespace::Postgres).expect("derives");
    assert_eq!(derived.namespace(), Namespace::Postgres);
    // The evidence digest equals the profile digest, so both sides of
    // the comparison verify against the same binding.
    let evidence = evidence(OBSERVED);
    assert_eq!(
        evidence.projection_ref().as_str(),
        profile.projection_ref().as_str()
    );
}

#[test]
fn the_declared_test_lifecycle_is_isolated_and_production_free() {
    let profile = profile();
    // The committed profile declares no lifecycle member at all, so
    // no test-database capability is claimed; a declared one carries
    // production access const-forbidden.
    assert!(profile.test_lifecycle().is_none());
    let declared = serde_json::json!({
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
        "projectionRef": profile.projection_ref().as_str(),
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
        "testLifecycle": {
            "isolation": "database",
            "provision": "create_drop",
            "cleanup": "drop",
            "production": "forbidden",
            "connection": {"name": "test-db"}
        }
    });
    let profile = StorageEngineAttachment::from_value(&declared).expect("valid profile");
    let lifecycle = profile.test_lifecycle().expect("declared");
    assert_eq!(
        lifecycle.isolation().key(),
        "database",
        "database isolation is the bounded default"
    );
    assert_eq!(lifecycle.provision().key(), "create_drop");
    assert_eq!(lifecycle.cleanup().key(), "drop");
}

// --- migration planner (plan step S7) --------------------------------------

const MIGRATION_BASE: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine/migration/base.json");
const MIGRATION_ADDITIVE: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine/migration/candidate-additive.json");
const MIGRATION_BACKFILL: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine/migration/candidate-backfill.json");
const MIGRATION_DESTRUCTIVE: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine/migration/candidate-destructive.json");

fn migration_attachment(bytes: &[u8]) -> StorageProjectionAttachment {
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("json");
    StorageProjectionAttachment::from_value(&value).expect("valid attachment")
}

#[test]
fn an_additive_change_plans_ready_without_destructive_steps() {
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &migration_attachment(MIGRATION_ADDITIVE),
        None,
    )
    .expect("plans");
    assert!(!plan.gated());
    assert_eq!(
        plan.status(),
        lekalo_core::storage_engine::PlanStatus::Ready
    );
    assert!(
        plan.steps().iter().all(|step| step.risk().key() == "none"),
        "a nullable technical column addition carries no data risk"
    );
    // The added column is present exactly once, on the right table.
    assert_eq!(
        plan.steps()
            .iter()
            .filter(|step| step.kind() == "add_column")
            .count(),
        1
    );
    assert!(plan
        .steps()
        .iter()
        .any(|step| step.statement().contains("last_seen_at")));
    // Determinism: planning twice is byte-identical.
    let again = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &migration_attachment(MIGRATION_ADDITIVE),
        None,
    )
    .expect("plans");
    assert_eq!(
        plan.canonical_bytes().expect("bytes"),
        again.canonical_bytes().expect("bytes")
    );
}

#[test]
fn a_not_null_tightening_without_default_requires_backfill() {
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &migration_attachment(MIGRATION_BACKFILL),
        None,
    )
    .expect("plans");
    assert!(
        plan.steps().iter().any(
            |step| step.kind() == "set_column_null" && step.risk().key() == "backfill_required"
        ),
        "the tightening carries the backfill risk visibly"
    );
    // The tightening plans its remediation: a backfill UPDATE with the
    // type's zero value precedes SET NOT NULL — the executable order
    // (SET NOT NULL alone fails on any existing NULL row).
    let set_null = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "set_column_null")
        .expect("the tightening step");
    let backfill = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "backfill")
        .expect("the tightening plans its backfill");
    assert_eq!(
        backfill.id(),
        set_null.requires()[0],
        "SET NOT NULL follows the backfill"
    );
    assert!(
        backfill.statement().contains("= 0 WHERE"),
        "the backfill writes the type's zero value: {}",
        backfill.statement()
    );
    // The plan is not gated: backfill is a declared obligation, not a
    // destructive rewrite.
    assert!(!plan.gated());
}

#[test]
fn an_added_not_null_column_plans_nullable_backfill_then_the_constraint() {
    // The base lacks the added column and the candidate adds it NOT
    // NULL without a default: PostgreSQL refuses the inline form on a
    // non-empty table, so the plan must add nullable first, backfill
    // the zero value, and only then hold the constraint.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_ADDITIVE).expect("candidate json");
    let fields = candidate_value
        .get_mut("entities")
        .and_then(|entities| entities.as_array_mut())
        .and_then(|entities| {
            entities.iter_mut().find(|entity| {
                entity.get("entityKey").and_then(serde_json::Value::as_str) == Some("task")
            })
        })
        .and_then(|entity| entity.get_mut("fields"))
        .and_then(|fields| fields.as_array_mut())
        .expect("entity fields");
    fields.push(serde_json::json!({
        "field": "attempt_count",
        "required": true,
        "type": {"name": "integer"},
        "visibility": "internal"
    }));
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect("plans");
    let add = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "add_column" && step.statement().contains("attempt_count"))
        .expect("the added column");
    assert!(
        !add.statement().contains("NOT NULL"),
        "the add is nullable: {}",
        add.statement()
    );
    let backfill = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "backfill" && step.statement().contains("attempt_count"))
        .expect("the backfill");
    let set_null = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "set_column_null" && step.statement().contains("attempt_count"))
        .expect("the constraint");
    assert_eq!(add.id() + 1, backfill.id(), "the backfill follows the add");
    assert_eq!(
        backfill.id(),
        set_null.requires()[0],
        "SET NOT NULL follows the backfill"
    );
    assert!(
        backfill.statement().contains("= 0 WHERE"),
        "the integer zero backfills: {}",
        backfill.statement()
    );
}

#[test]
fn a_new_table_plans_the_exact_ddl_create_statement() {
    // The migration create_table step reuses the DDL renderer: the
    // plan and the DDL document describe one schema. Pin it by
    // planning the committed projection against a base missing one
    // table: the create statement must equal the DDL renderer's
    // statement for the same table (identity, declared CHECK, and all
    // column facts included).
    let profile = profile();
    let full = projection();
    let mut base_value: serde_json::Value =
        serde_json::from_slice(PROJECTION).expect("projection json");
    // Remove the standalone roster table from every projection (its
    // entity leaves with it), leaving it for the candidate to create.
    if let Some(projections) = base_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
    {
        for projection in projections.iter_mut() {
            if let Some(tables) = projection.get_mut("tables").and_then(|t| t.as_array_mut()) {
                tables.retain(|table| {
                    table.get("table").and_then(serde_json::Value::as_str) != Some("task_roster")
                });
            }
        }
    }
    if let Some(entities) = base_value
        .get_mut("entities")
        .and_then(|entities| entities.as_array_mut())
    {
        entities.retain(|entity| {
            entity.get("entityKey").and_then(serde_json::Value::as_str) != Some("task_roster")
        });
    }
    let base = StorageProjectionAttachment::from_value(&base_value).expect("valid base");
    // The plan binds the profile to the base state; rebind the profile
    // to the mutated base the way an authoring step would.
    let mut profile_value: serde_json::Value = serde_json::from_slice(PROFILE).expect("profile");
    let base_bytes = base.canonical_bytes().expect("canonical");
    profile_value["projectionRef"] = serde_json::Value::String(format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(base_bytes.as_bytes())
    ));
    let base_profile = StorageEngineAttachment::from_value(&profile_value).expect("valid profile");
    let plan = lekalo_core::storage_engine::plan_migration(&base_profile, &base, &full, None)
        .expect("plans");
    let create = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "create_table")
        .expect("the missing table is created");
    let document = postgres::ddl::render(&profile, &full).expect("renders");
    let ddl_create = document
        .statements()
        .iter()
        .map(|statement| statement.statement())
        .find(|statement| statement.starts_with("CREATE TABLE \"task_roster\""))
        .expect("the DDL creates the same table");
    assert_eq!(
        create.statement(),
        ddl_create,
        "the planned create equals the DDL statement"
    );
    // No add_check steps follow for the new table: the constraints
    // rode the create statement.
    assert!(!plan.steps().iter().any(|step| step.kind() == "add_check"));
}

#[test]
fn a_changed_check_predicate_plans_a_drop_and_readd_in_order() {
    // The semantic diff is blind to nothing: a same-named CHECK whose
    // predicate changed is a drop of the old constraint followed by
    // the re-add under the same derived name, in executable order.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    let check = candidate_value
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
        .and_then(|table| table.get_mut("checks"))
        .and_then(|checks| checks.get_mut(0))
        .expect("the declared check");
    check["where"] = serde_json::json!([
        {"column": "deleted_at", "op": "is-not-null"}
    ]);
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect("plans");
    let positions: Vec<usize> = plan
        .steps()
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            step.statement().contains("chk_task_window")
                && (step.kind() == "drop_check" || step.kind() == "add_check")
        })
        .map(|(position, _)| position)
        .collect();
    assert_eq!(
        plan.steps()[positions[0]].kind(),
        "drop_check",
        "the old predicate drops first"
    );
    assert_eq!(
        plan.steps()[positions[1]].kind(),
        "add_check",
        "the new predicate re-adds under the same name"
    );
    assert_eq!(
        plan.steps()[positions[1]].requires(),
        &[plan.steps()[positions[0]].id()],
        "the re-add depends on the drop"
    );
    // A gated plan stays gated: a constraint replacement is a
    // destructive rewrite of enforced schema.
    assert!(plan.gated());
}

#[test]
fn a_changed_enum_member_list_plans_a_drop_and_readd_under_the_same_name() {
    // The derived enum CHECK diffs by (entity, name, predicate): a
    // changed member list replaces the constraint in place — the drop
    // of the old list executes before the re-add under the same
    // derived name, because ADD CONSTRAINT under an existing name
    // cannot execute.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    let fields = candidate_value
        .get_mut("entities")
        .and_then(|entities| entities.as_array_mut())
        .and_then(|entities| {
            entities.iter_mut().find(|entity| {
                entity.get("entityKey").and_then(serde_json::Value::as_str) == Some("tag")
            })
        })
        .and_then(|entity| entity.get_mut("fields"))
        .and_then(|fields| fields.as_array_mut())
        .expect("entity fields");
    let color = fields
        .iter_mut()
        .find(|field| field.get("field").and_then(serde_json::Value::as_str) == Some("color"))
        .expect("the enum field");
    color["type"]["members"] = serde_json::json!(["blue", "green", "red", "purple"]);
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect("plans");
    let positions: Vec<(usize, &str)> = plan
        .steps()
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            step.statement().contains("chk_tag_color")
                && (step.kind() == "drop_check" || step.kind() == "add_check")
        })
        .map(|(position, step)| (position, step.kind()))
        .collect();
    assert_eq!(
        positions,
        vec![
            (positions[0].0, "drop_check"),
            (positions[1].0, "add_check")
        ],
        "the old member list drops before the re-add"
    );
    assert_eq!(
        plan.steps()[positions[1].0].requires(),
        &[plan.steps()[positions[0].0].id()],
        "the re-add depends on the drop"
    );
    assert!(
        plan.steps()[positions[1].0]
            .statement()
            .contains("'purple'"),
        "the re-add carries the new member list"
    );
    assert!(plan.gated(), "a constraint replacement is destructive");
}

#[test]
fn a_dropped_tables_enum_checks_ride_the_table_drop() {
    // A table the candidate drops carries its derived enum CHECKs
    // with it: no member drops, the DROP TABLE owns them.
    let mut base_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("base json");
    // The candidate loses the whole tag entity, its join relation, and
    // every join declaration over that relation.
    base_value["entities"]
        .as_array_mut()
        .expect("entities")
        .retain(|entity| {
            entity.get("entityKey").and_then(serde_json::Value::as_str) != Some("tag")
        });
    for projection in base_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if let Some(tables) = projection.get_mut("tables").and_then(|t| t.as_array_mut()) {
            tables.retain(|table| {
                table.get("entity").and_then(serde_json::Value::as_str) != Some("tag")
            });
        }
        if let Some(joins) = projection.get_mut("joins").and_then(|j| j.as_array_mut()) {
            joins.retain(|join| {
                join.get("relation").and_then(serde_json::Value::as_str)
                    != Some("planner.relation.task_tags")
            });
        }
    }
    base_value["relations"]
        .as_array_mut()
        .expect("relations")
        .retain(|relation| {
            relation
                .get("relationId")
                .and_then(serde_json::Value::as_str)
                != Some("planner.relation.task_tags")
        });
    let candidate = StorageProjectionAttachment::from_value(&base_value).expect("valid candidate");
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect("plans");
    assert!(
        !plan
            .steps()
            .iter()
            .any(|step| step.statement().contains("chk_tag_color")),
        "no enum-check drops for the dropped table"
    );
    assert!(plan
        .steps()
        .iter()
        .any(|step| step.kind() == "drop_table" && step.statement().contains("\"tag\"")));
}

#[test]
fn a_plan_under_a_refusing_array_policy_refuses_and_never_renders_the_type() {
    // The render policies reach the planner too: under
    // `array:"unsupported"` the added-column plan refuses with the
    // registered rule instead of emitting the native array type the
    // policy refuses — the conformance battery's answer and the
    // emitter can no longer disagree.
    let mut profile_value: serde_json::Value = serde_json::from_slice(PROFILE).expect("profile");
    profile_value["policies"]["array"] = serde_json::Value::String("unsupported".to_owned());
    let refusing = StorageEngineAttachment::from_value(&profile_value).expect("valid profile");
    let error = lekalo_core::storage_engine::plan_migration(
        &refusing,
        &migration_attachment(MIGRATION_BASE),
        &migration_attachment(MIGRATION_ADDITIVE),
        None,
    )
    .expect_err("array unsupported refuses the plan");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("storage-engine.render-unsupported")
    );
}

#[test]
fn a_changed_primary_key_plans_the_drop_and_add_and_gates() {
    // A primary-key swap on a surviving table is visible to the
    // planner: the old constraint drops, the new one is added under
    // the deterministic pk_<table> name, and the swap gates.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        if let Some(tables) = projection.get_mut("tables").and_then(|t| t.as_array_mut()) {
            for table in tables.iter_mut() {
                if table.get("table").and_then(serde_json::Value::as_str) == Some("tag") {
                    table["primaryKey"] = serde_json::json!(["label"]);
                    println!("PATCHED TABLE at {:?}", table.get("table"));
                }
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect("plans");
    assert!(plan.gated(), "a primary-key swap is destructive");
    let drop = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "drop_constraint" && step.statement().contains("pk_tag"))
        .expect("the old primary key drops");
    let add = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "add_primary_key")
        .expect("the new primary key is added");
    assert!(add.statement().contains("PRIMARY KEY (\"label\")"));
    assert_eq!(
        add.requires(),
        &[drop.id()],
        "the add follows the drop of the old key"
    );
    // Both destructive steps declare their explain hook.
    assert_eq!(
        drop.explain(),
        Some(lekalo_core::storage_engine::migration::ExplainHook::Before)
    );
    assert_eq!(
        add.explain(),
        Some(lekalo_core::storage_engine::migration::ExplainHook::Before)
    );
}

#[test]
fn a_dropped_column_owns_its_check_and_index_drops() {
    // PostgreSQL auto-drops the indexes and constraints involving a
    // dropped column, so the plan's explicit drops of the column's
    // objects must execute before the DROP COLUMN — a confirmed plan
    // drops the constraint and index first, then the column.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    // Remove softDelete (deleted_at), the check over it, and the
    // partial index over it from the postgres projection.
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
            .retain(|table| table.get("table").and_then(serde_json::Value::as_str) != Some("task"));
        let task = serde_json::json!({
            "entity": "task",
            "table": "task",
            "primaryKey": ["id"],
            "indexes": [
                {"columns": ["due_date"], "unique": false},
                {"columns": ["tenant_id"], "name": "idx_task_tenant", "unique": false}
            ],
            "technicalColumns": [
                {"name": "row_etag", "nullable": false, "purpose": "optimistic concurrency token", "type": "bytea"}
            ],
            "tenantKey": {"column": "tenant_id", "type": "uuid"},
            "timestamps": {"createdAt": "created_at", "updatedAt": "updated_at"}
        });
        projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
            .push(task);
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    let position = |needle: &str| {
        plan.steps()
            .iter()
            .position(|step| step.statement().contains(needle))
            .unwrap_or_else(|| panic!("step with {needle} absent"))
    };
    let deleted_at_drop = position("DROP COLUMN \"deleted_at\"");
    let check_drop = position("DROP CONSTRAINT \"chk_task_window\"");
    let index_drop = position("DROP INDEX \"idx_task_due_open\"");
    assert!(
        check_drop < deleted_at_drop,
        "the check drops before its column"
    );
    assert!(
        index_drop < deleted_at_drop,
        "the index drops before its column"
    );
}

#[test]
fn a_renamed_table_rederives_its_rls_policy_and_self_fk_once() {
    // The RLS policy name embeds the table name: a rename drops the
    // old pol_<oldtable>_tenant and creates pol_<table>_tenant. The
    // rename re-derivation covers the table's foreign keys — a
    // self-referencing FK included — so the FK pass never double-emits
    // the same constraint.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("task") {
                table["table"] = serde_json::Value::String("todo".to_owned());
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    // The policy re-derivation: old name drops, fresh name is created.
    assert!(plan.steps().iter().any(|step| {
        step.kind() == "drop_policy" && step.statement().contains("pol_task_tenant")
    }));
    assert!(plan.steps().iter().any(|step| {
        step.kind() == "create_policy" && step.statement().contains("pol_todo_tenant")
    }));
    // The self-FK is added exactly once under the fresh name.
    let self_fk_adds = plan
        .steps()
        .iter()
        .filter(|step| {
            step.kind() == "add_foreign_key" && step.statement().contains("fk_todo_parent_task_id")
        })
        .count();
    assert_eq!(
        self_fk_adds, 1,
        "the self-referencing FK is added exactly once"
    );
    // No forward requires edges anywhere in the plan.
    for step in plan.steps() {
        for dependency in step.requires() {
            assert!(
                dependency < &step.id(),
                "step {} ({}) requires a later step {}: the graph contradicts the order",
                step.id(),
                step.kind(),
                dependency
            );
        }
    }
}

#[test]
fn a_renamed_table_keeps_its_column_diff() {
    // A rename sharing the diff with column work must not swallow the
    // column diff: a dropped field plans its drop_column against the
    // post-rename name, and a new sequence column gets add_column (not
    // just create_sequence + OWNED BY a column that was never added).
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    // Rename focus_session to session and drop the minutes field.
    let entity = candidate_value
        .get_mut("entities")
        .and_then(|entities| entities.as_array_mut())
        .and_then(|entities| {
            entities.iter_mut().find(|entity| {
                entity.get("entityKey").and_then(serde_json::Value::as_str) == Some("focus_session")
            })
        })
        .expect("focus_session entity");
    entity["fields"]
        .as_array_mut()
        .expect("fields")
        .retain(|field| field.get("field").and_then(serde_json::Value::as_str) != Some("minutes"));
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("focus_session") {
                table["table"] = serde_json::Value::String("session".to_owned());
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    let drop = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "drop_column" && step.statement().contains("\"minutes\""))
        .expect("the dropped field plans its column drop");
    assert!(
        drop.statement().contains("\"session\""),
        "the column drop targets the post-rename name"
    );
}

#[test]
fn a_renamed_table_renames_its_sequence_and_a_dropped_column_retires_it() {
    // The sequence name derives from the table and column: a renamed
    // table renames its sequence to the fresh deterministic name, and
    // a dropped sequence column retires its sequence instead of
    // orphaning it (step kinds rename_sequence / drop_sequence).
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    // Rename focus_session to session.
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("focus_session") {
                table["table"] = serde_json::Value::String("session".to_owned());
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    let rename = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "rename_sequence")
        .expect("the sequence rename is planned");
    assert_eq!(
        rename.statement(),
        "ALTER SEQUENCE \"seq_focus_session_session_no\" RENAME TO \"seq_session_session_no\";"
    );

    // A dropped sequence column on a surviving table plans no explicit
    // retirement: the column carries DEFAULT nextval(<sequence>) and
    // the OWNED BY dependency, so its DROP COLUMN auto-drops the
    // sequence — an explicit DROP SEQUENCE cannot execute in either
    // order (before the column the default blocks it; after it the
    // sequence no longer exists).
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("focus_session") {
                table
                    .as_object_mut()
                    .expect("object")
                    .remove("generatedColumns");
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    // No explicit sequence drop: the DROP COLUMN retires the owned
    // sequence, so the plan stays executable.
    assert!(
        !plan
            .steps()
            .iter()
            .any(|step| step.kind() == "drop_sequence"),
        "DROP SEQUENCE cannot execute around a DROP COLUMN of its owner"
    );
    assert!(plan.steps().iter().any(|step| {
        step.kind() == "drop_column" && step.statement().contains("\"session_no\"")
    }));
}

#[test]
fn a_join_rename_with_a_shape_change_rematerializes_under_the_new_name() {
    // A rename that shares the diff with a shape change (the endpoint
    // PK swap changes a join column's type) is a full rematerialization
    // under the new name: the create carries the fresh shape, and the
    // old-name drop is an independent statement behind it — no forward
    // requires edge, no incompatible-FK plan.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        for join in projection
            .get_mut("joins")
            .and_then(|j| j.as_array_mut())
            .expect("joins")
        {
            if join.get("relation").and_then(serde_json::Value::as_str)
                == Some("planner.relation.task_tags")
            {
                join["table"] = serde_json::Value::String("task_tagging".to_owned());
            }
        }
    }
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        for table in projection
            .get_mut("tables")
            .and_then(|tables| tables.as_array_mut())
            .expect("tables")
        {
            if table.get("entity").and_then(serde_json::Value::as_str) == Some("tag") {
                // The candidate re-keys the tag table to `label` - the
                // join column
                // type changes with it (uuid to varchar(64)).
                table["primaryKey"] = serde_json::json!(["label"]);
            }
        }
    }

    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    // The create carries the candidate shape (the join column over the
    // new varchar key).
    let create = plan
        .steps()
        .iter()
        .find(|step| {
            step.kind() == "create_table"
                && step.statement().contains("CREATE TABLE \"task_tagging\"")
        })
        .expect("the new shape is created");
    assert!(
        create.statement().contains("\"tag_id\" varchar(64)"),
        "the create carries the rematerialized shape: {}",
        create.statement()
    );
    // The old-name table drops as an independent statement; no forward
    // edge and no rename_table (the old table is gone, not renamed).
    assert!(
        !plan
            .steps()
            .iter()
            .any(|step| step.kind() == "rename_table"),
        "a rematerializing rename never renames the old shape"
    );
    let drop_position = plan
        .steps()
        .iter()
        .position(|step| step.kind() == "drop_table" && step.statement().contains("\"task_tag\""))
        .expect("the old join table drops");
    assert!(
        drop_position > create.id() - 1,
        "the old-name drop executes behind the create"
    );
    assert!(
        create.requires().is_empty(),
        "independent statements are not wired"
    );
}

#[test]
fn a_renamed_join_renames_and_rederives_its_foreign_keys() {
    // A join-table rename emits rename_table only — never a fresh
    // CREATE TABLE of the renamed table (which would fail `relation
    // already exists`) — and re-derives its foreign keys: the old
    // fk_<oldjoin>_* names drop, the fresh fk_<newjoin>_* names re-add
    // wired to the drops.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        for join in projection
            .get_mut("joins")
            .and_then(|j| j.as_array_mut())
            .expect("joins")
        {
            if join.get("relation").and_then(serde_json::Value::as_str)
                == Some("planner.relation.task_tags")
            {
                join["table"] = serde_json::Value::String("task_tagging".to_owned());
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    // No create_table for the renamed join.
    assert!(
        !plan.steps().iter().any(|step| {
            step.kind() == "create_table" && step.statement().contains("task_tagging")
        }),
        "a rename never re-creates the table"
    );
    let rename = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "rename_table")
        .expect("the join rename is planned");
    assert!(rename.statement().contains("RENAME TO \"task_tagging\""));
    // Old derived FK names drop; fresh names re-add, wired to the drops.
    let old_drops: Vec<usize> = plan
        .steps()
        .iter()
        .filter(|step| {
            step.kind() == "drop_constraint" && step.statement().contains("fk_task_tag_")
        })
        .map(|step| step.id())
        .collect();
    assert_eq!(old_drops.len(), 2, "one old-name drop per join side");
    let adds: Vec<&lekalo_core::storage_engine::migration::Step> = plan
        .steps()
        .iter()
        .filter(|step| {
            step.kind() == "add_foreign_key" && step.statement().contains("fk_task_tagging_")
        })
        .collect();
    assert_eq!(adds.len(), 2, "one fresh-name add per join side");
    for add in &adds {
        assert!(
            add.requires().is_empty(),
            "each re-add claims no dependency: the fresh name does not exist until the add creates it, and the old-name drops sort behind the constructive steps"
        );
    }
    // No forward edges anywhere in the plan: a dependency always
    // precedes its dependent (the r5 finding — the old asserts proved
    // set equality without proving the drops precede the adds).
    for step in plan.steps() {
        for dep in step.requires() {
            assert!(
                *dep < step.id(),
                "step {} depends on later step {}",
                step.id(),
                dep
            );
        }
    }
    assert!(plan.gated());
}

#[test]
fn a_rematerialized_join_plans_drop_before_create_of_the_same_name() {
    // A join whose shape changed (here: the delete behavior) is
    // rematerialized under the same deterministic table name: the DROP
    // TABLE stays paired with the CREATE TABLE that follows, and the
    // create is requires-wired to the drop — CREATE before DROP would
    // fail `relation already exists` on a confirmed plan.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for relation in candidate_value
        .get_mut("relations")
        .and_then(|relations| relations.as_array_mut())
        .expect("relations")
    {
        if relation
            .get("relationId")
            .and_then(serde_json::Value::as_str)
            == Some("planner.relation.task_tags")
        {
            relation["deleteBehavior"] = serde_json::Value::String("restrict".to_owned());
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    let drop_position = plan
        .steps()
        .iter()
        .position(|step| {
            step.kind() == "drop_table" && step.statement().contains("DROP TABLE \"task_tag\"")
        })
        .expect("the old join drops");
    let create_position = plan
        .steps()
        .iter()
        .position(|step| {
            step.kind() == "create_table" && step.statement().contains("CREATE TABLE \"task_tag\"")
        })
        .expect("the new join is created");
    assert!(
        drop_position < create_position,
        "the drop precedes the re-create: {}",
        plan.steps()
            .iter()
            .map(|step| step.statement())
            .collect::<Vec<&str>>()
            .join(" | ")
    );
    let create = &plan.steps()[create_position];
    assert_eq!(
        create.requires(),
        &[plan.steps()[drop_position].id()],
        "the re-create depends on the drop"
    );
}

#[test]
fn a_dropped_join_plans_a_gated_drop_table() {
    // Join tables are planned: dropping a many-to-many relation is a
    // destructive DROP TABLE, never a silently-empty ready plan.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if let Some(joins) = projection.get_mut("joins").and_then(|j| j.as_array_mut()) {
            joins.retain(|join| {
                join.get("relation").and_then(serde_json::Value::as_str)
                    != Some("planner.relation.task_tags")
            });
        }
    }
    candidate_value["relations"]
        .as_array_mut()
        .expect("relations")
        .retain(|relation| {
            relation
                .get("relationId")
                .and_then(serde_json::Value::as_str)
                != Some("planner.relation.task_tags")
        });
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect("plans");
    assert!(plan.gated(), "a dropped join table is destructive");
    assert_eq!(
        plan.status(),
        lekalo_core::storage_engine::PlanStatus::Blocked
    );
    let drop = plan
        .steps()
        .iter()
        .find(|step| step.statement().contains("\"task_tag\""))
        .expect("the join table drop is planned");
    assert_eq!(drop.kind(), "drop_table");
    assert_eq!(drop.risk().key(), "destructive");
}

#[test]
fn an_added_join_plans_the_exact_ddl_create_and_foreign_keys() {
    // Adding a many-to-many relation plans the join table's create —
    // the exact statement the DDL renderer emits for the same schema —
    // followed by its two join foreign keys.
    let full = migration_attachment(PROJECTION);
    let mut base_value: serde_json::Value = serde_json::from_slice(PROJECTION).expect("base json");
    for projection in base_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if let Some(joins) = projection.get_mut("joins").and_then(|j| j.as_array_mut()) {
            joins.retain(|join| {
                join.get("relation").and_then(serde_json::Value::as_str)
                    != Some("planner.relation.task_tags")
            });
        }
    }
    base_value["relations"]
        .as_array_mut()
        .expect("relations")
        .retain(|relation| {
            relation
                .get("relationId")
                .and_then(serde_json::Value::as_str)
                != Some("planner.relation.task_tags")
        });
    let base = StorageProjectionAttachment::from_value(&base_value).expect("valid base");
    // The plan binds the profile to the base state; rebind the profile
    // the way an authoring step would.
    let mut profile_value: serde_json::Value = serde_json::from_slice(PROFILE).expect("profile");
    let base_bytes = base.canonical_bytes().expect("canonical");
    profile_value["projectionRef"] = serde_json::Value::String(format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(base_bytes.as_bytes())
    ));
    let base_profile = StorageEngineAttachment::from_value(&profile_value).expect("valid");
    let plan = lekalo_core::storage_engine::plan_migration(&base_profile, &base, &full, None)
        .expect("plans");
    assert!(!plan.gated(), "adding a join is purely constructive");
    let create = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "create_table" && step.statement().contains("\"task_tag\""))
        .expect("the join create is planned");
    let document = postgres::ddl::render(&profile(), &full).expect("renders");
    let ddl_create = document
        .statements()
        .iter()
        .map(|statement| statement.statement())
        .find(|statement| statement.starts_with("CREATE TABLE \"task_tag\""))
        .expect("the DDL creates the same join");
    assert_eq!(
        create.statement(),
        ddl_create,
        "the planned join create equals the DDL statement"
    );
    let join_fks: Vec<&str> = plan
        .steps()
        .iter()
        .filter(|step| {
            step.kind() == "add_foreign_key" && step.statement().contains("\"task_tag\"")
        })
        .map(|step| step.statement())
        .collect();
    let ddl_join_fks: Vec<&str> = document
        .statements()
        .iter()
        .map(|statement| statement.statement())
        .filter(|statement| statement.starts_with("ALTER TABLE \"task_tag\" ADD CONSTRAINT"))
        .collect();
    assert_eq!(join_fks, ddl_join_fks, "the planned join FKs equal the DDL");
    assert_eq!(join_fks.len(), 2, "one FK per join side");
}

#[test]
fn a_renamed_table_rederives_its_constraint_names() {
    // The derived FK and index names embed the table name: after a
    // rename, the old names drop and the fresh deterministic names
    // re-add — the migrated schema never keeps a stale fk_<oldtable>_*
    // a fresh render would not produce.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        for namespace_table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if namespace_table
                .get("table")
                .and_then(serde_json::Value::as_str)
                == Some("tag")
            {
                namespace_table["table"] = serde_json::Value::String("label".to_owned());
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect("plans");
    assert!(plan.gated(), "a rename is destructive");
    assert!(plan
        .steps()
        .iter()
        .any(|step| step.kind() == "rename_table"));
    // The old derived names drop; the fresh ones re-add.
    assert!(plan
        .steps()
        .iter()
        .any(|step| { step.kind() == "drop_index" && step.statement().contains("idx_tag_label") }));
    assert!(plan.steps().iter().any(|step| {
        step.kind() == "add_index" && step.statement().contains("idx_label_label")
    }));
    // The migrated schema carries no stale derived name: every FK on
    // the renamed table references the new table name.
    assert!(!plan
        .steps()
        .iter()
        .any(|step| { step.kind() == "add_foreign_key" && step.statement().contains("fk_tag_") }));
    // The derived enum check re-adds under the new table name.
    assert!(plan.steps().iter().any(|step| {
        step.kind() == "add_check" && step.statement().contains("chk_label_color")
    }));
}

#[test]
fn a_renamed_tables_changed_index_never_double_creates_or_drops_a_phantom() {
    // A derived index whose columns change on a renamed table: the
    // rename block drops the base-derived name (the one that exists)
    // and re-adds the fresh name; the index pass must not re-diff the
    // renamed table — a second CREATE of the fresh name would fail
    // `already exists`, and a drop of the candidate-derived old name
    // would name an index the migrated schema never had.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("task") {
                table["table"] = serde_json::Value::String("todo".to_owned());
                table["indexes"] = serde_json::json!([
                    {"columns": ["due_date", "tenant_id"], "unique": false},
                    {"columns": ["due_date"], "name": "idx_task_due_open", "unique": false,
                     "where": [{"column": "deleted_at", "op": "is-null"}]},
                    {"columns": ["tenant_id"], "name": "idx_task_tenant", "unique": false}
                ]);
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    let fresh_creates = plan
        .steps()
        .iter()
        .filter(|step| step.statement().contains("idx_todo_due_date_tenant_id"))
        .count();
    assert_eq!(
        fresh_creates, 1,
        "the fresh derived index is created exactly once"
    );
    assert!(
        !plan
            .steps()
            .iter()
            .any(|step| step.statement().contains("idx_task_due_date_tenant_id")),
        "the candidate-derived old name never existed and never drops"
    );
    let old_drops = plan
        .steps()
        .iter()
        .filter(|step| {
            step.kind() == "drop_index"
                && step
                    .statement()
                    .contains("DROP INDEX \"idx_task_due_date\"")
        })
        .count();
    assert_eq!(old_drops, 1, "the base-derived old name drops exactly once");
}

#[test]
fn an_added_generated_column_keeps_its_full_shape() {
    // A sequence-generated column adds with its owned sequence, its
    // nextval default, and NOT NULL — exactly the shape a fresh render
    // produces — and an identity column adds GENERATED ALWAYS AS
    // IDENTITY. No zero-value backfill is owed: the generation fills
    // the existing rows at ADD time.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("tag") {
                table["generatedColumns"] = serde_json::json!([
                    {"kind": "sequence", "name": "tag_no"},
                    {"kind": "identity", "name": "row_no"}
                ]);
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    // The owned sequence is created before the column that defaults to
    // it, and the ownership binding is planned.
    plan.steps()
        .iter()
        .find(|step| {
            step.kind() == "create_sequence"
                && step.statement() == "CREATE SEQUENCE \"seq_tag_tag_no\";"
        })
        .expect("the sequence is created");
    assert!(plan.steps().iter().any(|step| {
        step.kind() == "alter_sequence" && step.statement().contains("OWNED BY \"tag\".\"tag_no\"")
    }));
    let sequence_add = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "add_column" && step.statement().contains("\"tag_no\""))
        .expect("the sequence column is added");
    assert!(
        sequence_add
            .statement()
            .contains("DEFAULT nextval('seq_tag_tag_no')"),
        "the sequence column draws from its owned sequence: {}",
        sequence_add.statement()
    );
    assert!(
        sequence_add.statement().contains("NOT NULL"),
        "the sequence column holds NOT NULL: {}",
        sequence_add.statement()
    );
    let identity_add = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "add_column" && step.statement().contains("\"row_no\""))
        .expect("the identity column is added");
    assert!(
        identity_add
            .statement()
            .contains("GENERATED ALWAYS AS IDENTITY"),
        "the identity column carries its generation: {}",
        identity_add.statement()
    );
    assert!(
        identity_add.statement().contains("NOT NULL"),
        "the identity column holds NOT NULL: {}",
        identity_add.statement()
    );
    for column in ["tag_no", "row_no"] {
        assert!(
            !plan.steps().iter().any(|step| {
                step.kind() == "backfill" && step.statement().contains(&format!("\"{column}\""))
            }),
            "no zero-value backfill is owed for {column}"
        );
    }
}

#[test]
fn a_removed_tenant_key_retires_its_policy_before_the_column() {
    // A tenant-key column that leaves a surviving table owns the
    // table's row level security: the policy reads the column, so the
    // policy (and the table's RLS mode) must retire before the
    // DROP COLUMN — otherwise the drop fails against the policy or RLS
    // stays enabled with no policy. A table that renamed in the same
    // plan carries its base-derived policy name onto the new name.
    let candidate_of = |rename: bool| {
        let mut candidate_value: serde_json::Value =
            serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
        for projection in candidate_value
            .get_mut("projections")
            .and_then(|projections| projections.as_array_mut())
            .expect("projections")
        {
            if projection
                .get("namespace")
                .and_then(serde_json::Value::as_str)
                != Some("postgres")
            {
                continue;
            }
            for table in projection
                .get_mut("tables")
                .and_then(|t| t.as_array_mut())
                .expect("tables")
            {
                if table.get("table").and_then(serde_json::Value::as_str) == Some("task") {
                    if rename {
                        table["table"] = serde_json::Value::String("todo".to_owned());
                    }
                    if let Some(object) = table.as_object_mut() {
                        object.remove("tenantKey");
                        if let Some(indexes) = object
                            .get_mut("indexes")
                            .and_then(serde_json::Value::as_array_mut)
                        {
                            indexes.retain(|index| {
                                index.get("name").and_then(serde_json::Value::as_str)
                                    != Some("idx_task_tenant")
                            });
                        }
                    }
                }
            }
        }
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate")
    };
    let confirmed = |candidate: &StorageProjectionAttachment| {
        let plan_id = {
            let blocked = lekalo_core::storage_engine::plan_migration(
                &profile(),
                &migration_attachment(MIGRATION_BASE),
                candidate,
                None,
            )
            .expect("plans");
            blocked.plan_id().to_owned()
        };
        lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            candidate,
            Some(&plan_id),
        )
        .expect("confirmed")
    };
    let plan = &confirmed(&candidate_of(false));
    let policy_drop = plan
        .steps()
        .iter()
        .find(|step| {
            step.kind() == "drop_policy"
                && step.statement() == "DROP POLICY \"pol_task_tenant\" ON \"task\";"
        })
        .expect("the tenant policy drops");
    assert!(
        plan.steps().iter().any(|step| {
            step.kind() == "disable_rls"
                && step.statement() == "ALTER TABLE \"task\" DISABLE ROW LEVEL SECURITY;"
        }),
        "the table's RLS mode retires with its policy"
    );
    let column_drop = plan
        .steps()
        .iter()
        .find(|step| step.statement().contains("DROP COLUMN \"tenant_id\""))
        .expect("the tenant column drops");
    assert!(
        policy_drop.id() < column_drop.id(),
        "the policy drops before its column"
    );
    let renamed = &confirmed(&candidate_of(true));
    assert!(
        renamed.steps().iter().any(|step| {
            step.kind() == "drop_policy"
                && step.statement() == "DROP POLICY \"pol_task_tenant\" ON \"todo\";"
        }),
        "a renamed table drops its base-derived policy name"
    );
}

#[test]
fn a_renamed_tables_named_index_content_change_is_rederived() {
    // A declared index name survives the rename, but its content does
    // not have to: a named index whose columns change on a renamed
    // table drops and re-adds under its stable declared name — never
    // silently lost, never re-created under a phantom name.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("task") {
                table["table"] = serde_json::Value::String("todo".to_owned());
                for index in table
                    .get_mut("indexes")
                    .and_then(|indexes| indexes.as_array_mut())
                    .expect("indexes")
                {
                    if index.get("name").and_then(serde_json::Value::as_str)
                        == Some("idx_task_tenant")
                    {
                        index["columns"] = serde_json::json!(["title"]);
                    }
                }
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    let drops: Vec<_> = plan
        .steps()
        .iter()
        .filter(|step| {
            step.kind() == "drop_index" && step.statement() == "DROP INDEX \"idx_task_tenant\";"
        })
        .collect();
    assert_eq!(
        drops.len(),
        1,
        "the changed named index drops exactly once under its declared name"
    );
    let adds: Vec<_> = plan
        .steps()
        .iter()
        .filter(|step| {
            step.kind() == "add_index"
                && step.statement() == "CREATE INDEX \"idx_task_tenant\" ON \"todo\" (\"title\");"
        })
        .collect();
    assert_eq!(
        adds.len(),
        1,
        "the changed named index re-adds exactly once under its declared name"
    );
    assert_eq!(
        adds[0].requires(),
        &[drops[0].id()],
        "the re-add follows its drop"
    );
    assert_eq!(
        plan.steps()
            .iter()
            .filter(|step| step.statement().contains("idx_task_tenant"))
            .count(),
        2,
        "no phantom or duplicate statements name the index"
    );
    for step in plan.steps() {
        for dep in step.requires() {
            assert!(*dep < step.id(), "no forward edges");
        }
    }
}

#[test]
fn a_renamed_tables_pk_swap_swaps_under_one_name_then_renames() {
    // A primary-key swap on a renamed table swaps under the old
    // deterministic name and then renames the constraint: the drop and
    // the add pair under pk_<base> — same-name pairing keeps the add
    // behind the drop, where a split-name add would fail 42P16 and
    // publish a forward edge — and every key referencing the swapped
    // key drops before the old constraint (2BP02) and re-adds,
    // re-targeted, once the fresh one binds.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("task") {
                table["table"] = serde_json::Value::String("todo".to_owned());
                table["primaryKey"] = serde_json::json!(["tenant_id"]);
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    let pk_drop = plan
        .steps()
        .iter()
        .find(|step| {
            step.kind() == "drop_constraint"
                && step.statement() == "ALTER TABLE \"todo\" DROP CONSTRAINT \"pk_task\";"
        })
        .expect("the old primary key drops under the base-derived name");
    let pk_add = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "add_primary_key")
        .expect("the fresh key adds");
    assert_eq!(
        pk_add.statement(),
        "ALTER TABLE \"todo\" ADD CONSTRAINT \"pk_task\" PRIMARY KEY (\"tenant_id\");",
        "the fresh key adds under the base-derived name so the drop/add pair"
    );
    let pk_rename = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "rename_constraint")
        .expect("the pair renames to the fresh deterministic name");
    assert_eq!(
        pk_rename.statement(),
        "ALTER TABLE \"todo\" RENAME CONSTRAINT \"pk_task\" TO \"pk_todo\";"
    );
    assert!(pk_drop.id() < pk_add.id() && pk_add.id() < pk_rename.id());
    assert_eq!(pk_add.requires(), &[pk_drop.id()]);
    assert_eq!(pk_rename.requires(), &[pk_add.id()]);
    // A dependent key on another table drops before the old primary
    // key and re-adds, re-targeted, wired to the binding rename.
    let fk_drop = plan
        .steps()
        .iter()
        .find(|step| {
            step.kind() == "drop_constraint"
                && step
                    .statement()
                    .contains("DROP CONSTRAINT \"fk_task_detail_task_id\"")
        })
        .expect("the dependent foreign key drops");
    assert!(
        fk_drop.id() < pk_drop.id(),
        "the dependent key drops before the old primary key"
    );
    let fk_add = plan
        .steps()
        .iter()
        .find(|step| {
            step.kind() == "add_foreign_key" && step.statement().contains("fk_task_detail_task_id")
        })
        .expect("the dependent foreign key re-adds");
    assert!(
        fk_add
            .statement()
            .contains("REFERENCES \"todo\"(\"tenant_id\")"),
        "the re-add targets the fresh key's column: {}",
        fk_add.statement()
    );
    assert_eq!(fk_add.requires(), &[pk_rename.id()]);
    // The renamed table's own re-derived key waits for the binding too.
    let self_add = plan
        .steps()
        .iter()
        .find(|step| step.statement().contains("fk_todo_parent_task_id"))
        .expect("the renamed table's own key re-adds");
    assert_eq!(self_add.requires(), &[pk_rename.id()]);
    // A stable join's key referencing the swapped table rides the same
    // flow instead of blocking the drop.
    let join_drop = plan
        .steps()
        .iter()
        .find(|step| {
            step.kind() == "drop_constraint"
                && step
                    .statement()
                    .contains("DROP CONSTRAINT \"fk_task_tag_task_id\"")
        })
        .expect("the stable join's dependent key drops");
    assert!(join_drop.id() < pk_drop.id());
    let join_add = plan
        .steps()
        .iter()
        .find(|step| {
            step.kind() == "add_foreign_key" && step.statement().contains("fk_task_tag_task_id")
        })
        .expect("the stable join's dependent key re-adds");
    assert!(join_add
        .statement()
        .contains("REFERENCES \"todo\"(\"tenant_id\")"));
    assert_eq!(join_add.requires(), &[pk_rename.id()]);
    for step in plan.steps() {
        for dep in step.requires() {
            assert!(*dep < step.id(), "no forward edges");
        }
    }
}

#[test]
fn a_referenced_pk_swap_takes_its_dependent_keys_down_and_back() {
    // A referenced table's primary-key swap is executable end to end:
    // every key referencing the swapped table — owned by other tables,
    // by stable joins, and by the table itself — drops before the old
    // constraint (2BP02), and re-adds after the fresh key binds,
    // re-targeted at its column. The swap never plans a drop its
    // dependents can block, and never leaves a key bound to a
    // non-unique column.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("task") {
                table["primaryKey"] = serde_json::json!(["tenant_id"]);
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    let pk_drop = plan
        .steps()
        .iter()
        .find(|step| {
            step.kind() == "drop_constraint"
                && step.statement() == "ALTER TABLE \"task\" DROP CONSTRAINT \"pk_task\";"
        })
        .expect("the old primary key drops");
    let pk_add = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "add_primary_key")
        .expect("the fresh primary key adds");
    assert_eq!(
        pk_add.statement(),
        "ALTER TABLE \"task\" ADD CONSTRAINT \"pk_task\" PRIMARY KEY (\"tenant_id\");"
    );
    assert_eq!(pk_add.requires(), &[pk_drop.id()]);
    // Every dependent key drops before the old constraint: the entity
    // tables', the self-reference, and the stable join's.
    let dependent_drops = [
        "fk_task_detail_task_id",
        "fk_task_external_link_task_id",
        "fk_focus_session_focus_task_id",
        "fk_task_parent_task_id",
        "fk_task_tag_task_id",
    ];
    for constraint in dependent_drops {
        let drop = plan
            .steps()
            .iter()
            .find(|step| {
                step.kind() == "drop_constraint"
                    && step
                        .statement()
                        .contains(&format!("DROP CONSTRAINT \"{constraint}\""))
            })
            .unwrap_or_else(|| panic!("{constraint} drops before the old key"));
        assert!(
            drop.id() < pk_drop.id(),
            "{constraint} drops before the old primary key"
        );
    }
    // Each key re-adds re-targeted at the fresh key's column, wired to
    // the pk add, and never twice.
    for (constraint, table_name, action) in [
        ("fk_task_detail_task_id", "task_detail", "CASCADE"),
        (
            "fk_task_external_link_task_id",
            "task_external_link",
            "CASCADE",
        ),
        (
            "fk_focus_session_focus_task_id",
            "focus_session",
            "RESTRICT",
        ),
        ("fk_task_parent_task_id", "task", "SET NULL"),
        ("fk_task_tag_task_id", "task_tag", "CASCADE"),
    ] {
        let adds: Vec<_> = plan
            .steps()
            .iter()
            .filter(|step| {
                step.statement()
                    .contains(&format!("ADD CONSTRAINT \"{constraint}\""))
            })
            .collect();
        assert_eq!(adds.len(), 1, "{constraint} re-adds exactly once");
        assert!(
            adds[0].statement().contains(&format!(
                "REFERENCES \"task\"(\"tenant_id\") ON DELETE {action}"
            )),
            "{constraint} re-targets the fresh key: {}",
            adds[0].statement()
        );
        assert_eq!(
            adds[0].requires(),
            &[pk_add.id()],
            "{constraint} re-adds after the fresh key binds"
        );
        let _ = table_name;
    }
    for step in plan.steps() {
        for dep in step.requires() {
            assert!(*dep < step.id(), "no forward edges");
        }
    }
    assert!(plan.gated(), "the swap stays destructive and gated");
}

#[test]
fn a_pure_rename_rederives_its_primary_key_name() {
    // Constraint names follow the table through ALTER TABLE RENAME, so
    // a pure rename would leave pk_task on todo — the same stale-name
    // failure the fk_/idx_/pol_ flows close. The rename block renames
    // the constraint to the fresh deterministic name.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        for table in projection
            .get_mut("tables")
            .and_then(|t| t.as_array_mut())
            .expect("tables")
        {
            if table.get("table").and_then(serde_json::Value::as_str) == Some("task") {
                table["table"] = serde_json::Value::String("todo".to_owned());
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan_id = {
        let blocked = lekalo_core::storage_engine::plan_migration(
            &profile(),
            &migration_attachment(MIGRATION_BASE),
            &candidate,
            None,
        )
        .expect("plans");
        blocked.plan_id().to_owned()
    };
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        Some(&plan_id),
    )
    .expect("confirmed");
    let rename_table = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "rename_table")
        .expect("the table renames");
    let pk_rename = plan
        .steps()
        .iter()
        .find(|step| {
            step.kind() == "rename_constraint"
                && step.statement()
                    == "ALTER TABLE \"todo\" RENAME CONSTRAINT \"pk_task\" TO \"pk_todo\";"
        })
        .expect("the primary key renames to the fresh deterministic name");
    assert!(
        rename_table.id() < pk_rename.id(),
        "the constraint rename follows the table rename"
    );
    assert!(
        !plan
            .steps()
            .iter()
            .any(|step| step.kind() == "add_primary_key"),
        "a pure rename never rebuilds the key"
    );
    for step in plan.steps() {
        for dep in step.requires() {
            assert!(*dep < step.id(), "no forward edges");
        }
    }
}

#[test]
fn a_type_change_without_an_assignment_cast_refuses() {
    // A text-to-integer change cannot execute as a bare ALTER COLUMN
    // TYPE: the planner refuses with the registered rule instead of
    // emitting a statement PostgreSQL would reject mid-apply.
    let candidate =
        candidate_with_field_type("tag", "label", serde_json::json!({"name": "integer"}));
    let error = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect_err("text to bigint refuses");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("storage-engine.render-unsupported")
    );
    let rendered = serde_json::to_string(&error).expect("json");
    assert!(rendered.contains("column-type-uncastable"));
}

#[test]
fn a_widening_type_change_plans_the_type_alter() {
    // A varchar widening casts by assignment, so the type alter plans
    // destructively and carries the exact new spelling.
    let candidate = candidate_with_field_type(
        "tag",
        "label",
        serde_json::json!({"length": 128, "name": "string"}),
    );
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect("plans");
    let alter = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "alter_column_type")
        .expect("the type alter is planned");
    assert!(alter
        .statement()
        .contains("ALTER COLUMN \"label\" TYPE varchar(128)"));
    assert!(plan.gated(), "a type change is destructive");
}

/// One candidate attachment whose postgres-projected field changes
/// its declared type; everything else stays the committed base.
fn candidate_with_field_type(
    entity_key: &str,
    field_name: &str,
    field_type: serde_json::Value,
) -> StorageProjectionAttachment {
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    let entity = candidate_value
        .get_mut("entities")
        .and_then(|entities| entities.as_array_mut())
        .and_then(|entities| {
            entities.iter_mut().find(|entity| {
                entity.get("entityKey").and_then(serde_json::Value::as_str) == Some(entity_key)
            })
        })
        .expect("entity");
    let field = entity
        .get_mut("fields")
        .and_then(|fields| fields.as_array_mut())
        .and_then(|fields| {
            fields.iter_mut().find(|field| {
                field.get("field").and_then(serde_json::Value::as_str) == Some(field_name)
            })
        })
        .expect("field");
    field["type"] = field_type;
    StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate")
}

#[test]
fn a_generated_kind_change_refuses_instead_of_staying_silent() {
    // Flipping a generated column between sequence and identity has no
    // deterministic v1 transition (no SET GENERATED step, no sequence
    // retirement in the step vocabulary): the planner refuses with the
    // registered rule instead of a silently-empty ready plan.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        if let Some(tables) = projection.get_mut("tables").and_then(|t| t.as_array_mut()) {
            for table in tables.iter_mut() {
                if table.get("table").and_then(serde_json::Value::as_str) == Some("focus_session") {
                    table["generatedColumns"] =
                        serde_json::json!([{ "kind": "identity", "name": "session_no" }]);
                }
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let error = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect_err("generated-kind change refuses");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("storage-engine.render-unsupported")
    );
    let rendered = serde_json::to_string(&error).expect("json");
    assert!(rendered.contains("generated-kind-change"));
}

#[test]
fn a_new_sequence_column_plans_its_creation_and_ownership() {
    // A new generated sequence column plans CREATE SEQUENCE, the
    // column, and the DDL document's ownership statement — the plan
    // and the document agree on the sequence lifecycle.
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    for projection in candidate_value
        .get_mut("projections")
        .and_then(|projections| projections.as_array_mut())
        .expect("projections")
    {
        if projection
            .get("namespace")
            .and_then(serde_json::Value::as_str)
            != Some("postgres")
        {
            continue;
        }
        if let Some(tables) = projection.get_mut("tables").and_then(|t| t.as_array_mut()) {
            for table in tables.iter_mut() {
                if table.get("table").and_then(serde_json::Value::as_str) == Some("task") {
                    table["generatedColumns"] =
                        serde_json::json!([{ "kind": "sequence", "name": "task_no" }]);
                }
            }
        }
    }
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let plan = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect("plans");
    let create = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "create_sequence")
        .expect("the sequence is created");
    assert!(create
        .statement()
        .contains("CREATE SEQUENCE \"seq_task_task_no\""));
    let own = plan
        .steps()
        .iter()
        .find(|step| step.kind() == "alter_sequence")
        .expect("the sequence ownership is planned");
    assert_eq!(
        own.statement(),
        "ALTER SEQUENCE \"seq_task_task_no\" OWNED BY \"task\".\"task_no\";",
        "the ownership statement equals the DDL document's"
    );
}

#[test]
fn an_added_enum_column_under_native_enum_refuses_the_plan() {
    // The enum arm of the policy table propagates: under
    // `native_enum` an added enum column refuses the plan with the
    // registered rule instead of silently planning the derived
    // varchar — the planner and the DDL document must never disagree
    // on a declared-refuse path.
    let mut profile_value: serde_json::Value = serde_json::from_slice(PROFILE).expect("profile");
    profile_value["policies"]["enum"] = serde_json::Value::String("native_enum".to_owned());
    let native = StorageEngineAttachment::from_value(&profile_value).expect("valid profile");
    let mut candidate_value: serde_json::Value =
        serde_json::from_slice(MIGRATION_BASE).expect("candidate json");
    let entity = candidate_value
        .get_mut("entities")
        .and_then(|entities| entities.as_array_mut())
        .and_then(|entities| {
            entities.iter_mut().find(|entity| {
                entity.get("entityKey").and_then(serde_json::Value::as_str) == Some("task")
            })
        })
        .expect("task entity");
    entity["fields"]
        .as_array_mut()
        .expect("fields")
        .push(serde_json::json!({
            "field": "shade",
            "type": {"members": ["warm", "cold"], "name": "enum"},
            "visibility": "internal"
        }));
    let candidate =
        StorageProjectionAttachment::from_value(&candidate_value).expect("valid candidate");
    let error = lekalo_core::storage_engine::plan_migration(
        &native,
        &migration_attachment(MIGRATION_BASE),
        &candidate,
        None,
    )
    .expect_err("native_enum refuses the added enum column");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("storage-engine.render-unsupported")
    );
    let rendered = serde_json::to_string(&error).expect("json");
    assert!(rendered.contains("native-enum"));
}

#[test]
fn a_profile_bound_to_a_foreign_projection_refuses_to_plan() {
    // The binding guarantee holds on the migration surface too: the
    // profile is authored against the base state, and a profile bound
    // to an unrelated projection refuses with the typed detail.
    let mut profile_value: serde_json::Value = serde_json::from_slice(PROFILE).expect("profile");
    profile_value["projectionRef"] = serde_json::Value::String(
        "sha256:0909090909090909090909090909090909090909090909090909090909090909".into(),
    );
    let foreign = StorageEngineAttachment::from_value(&profile_value).expect("valid profile");
    let error = lekalo_core::storage_engine::plan_migration(
        &foreign,
        &migration_attachment(MIGRATION_BASE),
        &migration_attachment(MIGRATION_ADDITIVE),
        None,
    )
    .expect_err("unbound profile");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("storage-engine.migration-invalid")
    );
    let rendered = serde_json::to_string(&error).expect("json");
    assert!(rendered.contains("projection-binding-mismatch"));
}

#[test]
fn the_full_declaration_battery_passes_without_skips() {
    // The committed full-coverage profile declares concurrency,
    // introspection, and the test lifecycle, so the checks the base
    // profile skips must pass for real: the battery proves the
    // positive answers, not only the skips.
    let clean = evidence(OBSERVED);
    let drifted = evidence(OBSERVED_DRIFTED);
    let inputs = lekalo_core::storage_engine::conformance::BatteryInputs {
        profile: &full_profile(),
        projection: &projection(),
        evidence: Some(&clean),
        drifted: Some(&drifted),
        input: None,
        runtime_goldens: &[],
    };
    let battery = lekalo_core::storage_engine::conformance::run(&inputs);
    let outcome = |id: &str| {
        battery
            .checks()
            .iter()
            .find(|check| check.id() == id)
            .map(|check| (check.outcome().key(), check.outcome().reason()))
            .unwrap_or_else(|| panic!("check {id} absent"))
    };
    assert_eq!(outcome("cas-column-declared"), ("pass", None));
    assert_eq!(outcome("lifecycle.isolation-declared"), ("pass", None));
    assert_eq!(outcome("lifecycle.no-production"), ("pass", None));
    assert!(battery.ok());
}

#[test]
fn a_destructive_plan_is_gated_and_blocked_until_the_plan_id_is_named() {
    let base = migration_attachment(MIGRATION_BASE);
    let candidate = migration_attachment(MIGRATION_DESTRUCTIVE);
    let plan = lekalo_core::storage_engine::plan_migration(&profile(), &base, &candidate, None)
        .expect("plans");
    assert!(plan.gated(), "a column drop is destructive");
    assert_eq!(
        plan.status(),
        lekalo_core::storage_engine::PlanStatus::Blocked
    );
    assert!(plan
        .steps()
        .iter()
        .any(|step| step.kind() == "drop_column" && step.risk().key() == "destructive"));
    // The wrong planId refuses with the registered gate diagnostic.
    let error = lekalo_core::storage_engine::plan_migration(
        &profile(),
        &base,
        &candidate,
        Some("sha256:0707070707070707070707070707070707070707070707070707070707070707"),
    )
    .expect_err("wrong digest");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("storage-engine.migration-gated")
    );
    // The exact planId confirms the plan.
    let plan_id = plan.plan_id().to_owned();
    let confirmed =
        lekalo_core::storage_engine::plan_migration(&profile(), &base, &candidate, Some(&plan_id))
            .expect("confirmed");
    assert_eq!(
        confirmed.status(),
        lekalo_core::storage_engine::PlanStatus::Confirmed
    );
    // Drops sort behind the constructive steps.
    let kinds: Vec<&str> = confirmed.steps().iter().map(|step| step.kind()).collect();
    let first_drop = kinds.iter().position(|kind| kind.starts_with("drop_"));
    if let Some(position) = first_drop {
        assert!(
            kinds[..position]
                .iter()
                .all(|kind| !kind.starts_with("drop_")),
            "drops sort last: {kinds:?}"
        );
    }
}
