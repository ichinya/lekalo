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
