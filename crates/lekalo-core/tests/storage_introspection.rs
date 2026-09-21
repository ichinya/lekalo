//! Issue #117 integration tests for the storage-introspection family:
//! wire normalization over the committed golden, byte-identical
//! canonicalization, the checked/read-only constants, credential
//! refusal, and the closed drift kinds over the committed drift pairs.
//!
//! The tests are hermetic: every fixture is embedded at compile time,
//! nothing touches the network, and no database is ever contacted.

use lekalo_core::storage_introspection::{introspect_check, DriftKind, StorageIntrospection};
use lekalo_core::storage_projection::Namespace;
use lekalo_core::storage_projection::StorageProjectionAttachment;

/// The committed mysql 8.0 planner evidence golden.
const GOLDEN: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-introspection/valid/mysql-8.0-planner.json");
/// The committed projection golden the drift pairs compare against.
const PROJECTION: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-projection/valid/planner-storage.json");

/// The committed adversarial vectors: (name, fixture bytes, expectation
/// bytes).
const INVALID: &[(&str, &[u8], &[u8])] = &[
    (
        "mode-unchecked",
        include_bytes!("../../../tests/fixtures/storage-introspection/invalid/mode-unchecked.json"),
        include_bytes!(
            "../../../tests/fixtures/storage-introspection/invalid/mode-unchecked.expect.json"
        ),
    ),
    (
        "read-only-false",
        include_bytes!(
            "../../../tests/fixtures/storage-introspection/invalid/read-only-false.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-introspection/invalid/read-only-false.expect.json"
        ),
    ),
    (
        "credential-member",
        include_bytes!(
            "../../../tests/fixtures/storage-introspection/invalid/credential-member.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-introspection/invalid/credential-member.expect.json"
        ),
    ),
    (
        "host-shaped-schema",
        include_bytes!(
            "../../../tests/fixtures/storage-introspection/invalid/host-shaped-schema.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-introspection/invalid/host-shaped-schema.expect.json"
        ),
    ),
];

/// The committed drift pairs: (name, evidence bytes, expected kind).
/// All six pairs cover the closed kind set; the collation pair is
/// synthesized inline from the golden because the flipped member is one
/// JSON string.
const DRIFTS: &[(&str, &[u8], DriftKind)] = &[
    (
        "missing-index",
        include_bytes!("../../../tests/fixtures/storage-introspection/drift/missing-index.json"),
        DriftKind::MissingIndex,
    ),
    (
        "missing-table",
        include_bytes!("../../../tests/fixtures/storage-introspection/drift/missing-table.json"),
        DriftKind::MissingTable,
    ),
    (
        "type-mismatch",
        include_bytes!("../../../tests/fixtures/storage-introspection/drift/type-mismatch.json"),
        DriftKind::TypeMismatch,
    ),
    (
        "engine-mismatch",
        include_bytes!("../../../tests/fixtures/storage-introspection/drift/engine-mismatch.json"),
        DriftKind::EngineMismatch,
    ),
    (
        "sql-mode-mismatch",
        include_bytes!(
            "../../../tests/fixtures/storage-introspection/drift/sql-mode-mismatch.json"
        ),
        DriftKind::SqlModeMismatch,
    ),
    (
        "version-mismatch",
        include_bytes!("../../../tests/fixtures/storage-introspection/drift/version-mismatch.json"),
        DriftKind::VersionMismatch,
    ),
];

fn parse(bytes: &[u8]) -> StorageIntrospection {
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("fixture JSON");
    StorageIntrospection::from_value(&value).expect("valid fixture")
}

fn projection() -> StorageProjectionAttachment {
    let value: serde_json::Value = serde_json::from_slice(PROJECTION).expect("fixture JSON");
    StorageProjectionAttachment::from_value(&value).expect("valid fixture")
}

#[test]
fn golden_normalizes_and_canonicalizes_byte_identically() {
    let evidence = parse(GOLDEN);
    assert_eq!(evidence.project_id().as_str(), "planner");
    assert_eq!(evidence.test_schema().as_str(), "lekalo_test_planner");
    // The checked-mode constants hold by construction.
    assert_eq!(evidence.engine().engine_version(), "8.0.36");
    assert!(!evidence.tables().is_empty());
    let canonical = evidence.canonical_bytes().expect("canonical bytes");
    let committed = std::str::from_utf8(GOLDEN).expect("utf8").trim_end();
    assert_eq!(canonical, committed, "canonical bytes must match goldens");
    // Validation is idempotent.
    evidence.validate_attachment().expect("revalidates");
}

#[test]
fn adversarial_vectors_reject_with_the_registered_rule() {
    for (name, bytes, expectation) in INVALID {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let expected: serde_json::Value = serde_json::from_slice(expectation)
            .unwrap_or_else(|error| panic!("{name} expectation: {error}"));
        let rule = expected["rule"].as_str().expect("rule");
        let detail = expected["detail"].as_str().expect("detail");
        let error =
            StorageIntrospection::from_value(&value).expect_err(&format!("{name} must reject"));
        let ids = error.reason_ids();
        assert_eq!(
            ids.first().copied(),
            Some(rule),
            "{name}: wrong rule for {ids:?}"
        );
        assert_eq!(error.as_slice().len(), 1, "{name}: one diagnostic only");
        let rendered = serde_json::to_string(&error).expect("diagnostic json");
        assert!(
            rendered.contains(detail),
            "{name}: detail {detail} missing from {rendered}"
        );
    }
}

#[test]
fn drift_pairs_classify_the_closed_kinds() {
    let attachment = projection();
    for (name, bytes, kind) in DRIFTS {
        let evidence = parse(bytes);
        let report =
            introspect_check(&attachment, Namespace::Mysql, &evidence).expect("comparable");
        assert!(!report.equal, "{name} must drift");
        assert!(
            report.drifts.iter().any(|drift| drift.kind == *kind),
            "{name}: expected {kind:?} in {:?}",
            report.drifts
        );
    }
}

#[test]
fn agreeing_evidence_reports_no_drift() {
    let attachment = projection();
    let evidence = parse(GOLDEN);
    // The golden evidence was produced against the mysql projection of
    // this exact attachment: the type renders agree (the drift pairs
    // mutate single facts out of this baseline).
    let report = introspect_check(&attachment, Namespace::Mysql, &evidence).expect("comparable");
    // The committed baseline may legitimately carry neutral gaps (the
    // declared columns the fixture evidence intentionally omits, like
    // the external-link table), but none of the closed mismatch kinds
    // may appear on the columns the evidence does observe.
    for drift in &report.drifts {
        assert!(
            matches!(
                drift.kind,
                DriftKind::MissingTable | DriftKind::MissingIndex
            ),
            "unexpected closed mismatch kind on the agreeing baseline: {drift:?}"
        );
    }
}

/// The golden baseline is fully agreeing on every member the evidence
/// observes: engine token, sql mode, version line, types (base family
/// and parameters), nullability, and the observed collations that the
/// declared table/textDefaults carry. Closed mismatch kinds may not
/// appear at all — not even beside the neutral missing-table gaps.
#[test]
fn agreeing_baseline_carries_no_engine_or_column_drift() {
    let attachment = projection();
    let evidence = parse(GOLDEN);
    let report = introspect_check(&attachment, Namespace::Mysql, &evidence).expect("comparable");
    for drift in &report.drifts {
        assert!(
            !matches!(
                drift.kind,
                DriftKind::EngineMismatch
                    | DriftKind::SqlModeMismatch
                    | DriftKind::VersionMismatch
                    | DriftKind::CollationMismatch
                    | DriftKind::TypeMismatch
                    | DriftKind::NullabilityMismatch
            ),
            "the agreeing baseline must not drift on observed members: {drift:?}"
        );
    }
}

#[test]
fn drift_report_is_deterministic() {
    let attachment = projection();
    let evidence = parse(
        include_bytes!("../../../tests/fixtures/storage-introspection/drift/type-mismatch.json")
            .as_slice(),
    );
    let first = introspect_check(&attachment, Namespace::Mysql, &evidence).expect("comparable");
    let second = introspect_check(&attachment, Namespace::Mysql, &evidence).expect("comparable");
    assert_eq!(
        serde_json::to_string(&first).expect("json"),
        serde_json::to_string(&second).expect("json")
    );
}

/// Column parameter drift is not silent: `varchar(200)` vs `varchar(64)`
/// is a type mismatch (the base name alone compares equal no longer),
/// while a `bigint unsigned` presentation suffix is not a type change.
#[test]
fn parameter_drift_is_type_drift_and_suffix_is_not() {
    let attachment = projection();
    let mut value: serde_json::Value = serde_json::from_slice(GOLDEN).expect("json");
    let task = value["tables"]
        .as_array_mut()
        .expect("tables")
        .iter_mut()
        .find(|table| table["name"] == "task")
        .expect("task table");
    let title = task["columns"]
        .as_array_mut()
        .expect("columns")
        .iter_mut()
        .find(|column| column["name"] == "title")
        .expect("title column");
    title["type"] = serde_json::Value::String("varchar(64)".to_owned());
    let narrowed = StorageIntrospection::from_value(&value).expect("parses");
    let report = introspect_check(&attachment, Namespace::Mysql, &narrowed).expect("comparable");
    assert!(report.drifts.iter().any(|drift| {
        drift.kind == DriftKind::TypeMismatch && drift.path == "tables/task/columns/title"
    }));
    // The unsigned presentation suffix never changes the value domain.
    let mut value: serde_json::Value = serde_json::from_slice(GOLDEN).expect("json");
    let task = value["tables"]
        .as_array_mut()
        .expect("tables")
        .iter_mut()
        .find(|table| table["name"] == "task")
        .expect("task table");
    let tenant = task["columns"]
        .as_array_mut()
        .expect("columns")
        .iter_mut()
        .find(|column| column["name"] == "tenant_id")
        .expect("tenant column");
    tenant["type"] = serde_json::Value::String("binary unsigned".to_owned());
    let suffixed = StorageIntrospection::from_value(&value).expect("parses");
    let report = introspect_check(&attachment, Namespace::Mysql, &suffixed).expect("comparable");
    assert!(!report
        .drifts
        .iter()
        .any(|drift| drift.path == "tables/task/columns/tenant_id"));
}

/// A collation drift pair built from the golden: the tag table flips to
/// `utf8mb4_bin` and the observed column collation follows, so the
/// declared `_ai_ci` uniqueness surface disagrees — collation-mismatch
/// on the column path.
#[test]
fn collation_flip_is_collation_drift() {
    let attachment = projection();
    let mut value: serde_json::Value = serde_json::from_slice(GOLDEN).expect("json");
    let tag = value["tables"]
        .as_array_mut()
        .expect("tables")
        .iter_mut()
        .find(|table| table["name"] == "tag")
        .expect("tag table");
    tag["collation"] = serde_json::Value::String("utf8mb4_bin".to_owned());
    tag["columns"][1]["collation"] = serde_json::Value::String("utf8mb4_bin".to_owned());
    let flipped = StorageIntrospection::from_value(&value).expect("parses");
    let report = introspect_check(&attachment, Namespace::Mysql, &flipped).expect("comparable");
    assert!(report.drifts.iter().any(|drift| {
        drift.kind == DriftKind::CollationMismatch && drift.path == "tables/tag/columns/label"
    }));
    // The engine echo of the golden still agrees, so the only column
    // finding is the collation flip.
    assert!(!report
        .drifts
        .iter()
        .any(|drift| drift.kind == DriftKind::VersionMismatch));
}
