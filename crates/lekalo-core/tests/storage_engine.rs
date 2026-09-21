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
