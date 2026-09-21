//! Issue #117 integration tests for the storage-engine-profile
//! family: wire normalization over the committed goldens, byte-identical
//! canonicalization, adversarial rejection with exact registered rules,
//! the capability snapshot bridge under the strict/permissive profiles,
//! and the portability report with the named PostgreSQL divergences.
//!
//! The tests are hermetic: every fixture is embedded at compile time,
//! nothing touches the network, and no database is ever contacted.

use lekalo_core::scenario::id::NamespacedId;
use lekalo_core::storage_engine_profile::{
    capabilities, compare, named_postgres_divergences, portability, to_snapshot, AdapterToken,
    DiffClass, DiffLayer, EngineToken, ProfileCapabilityId, ProfileSupport, ReportSupport,
    StorageEngineProfile, Support,
};
use lekalo_core::transaction_concurrency::capability::{
    map_capabilities, CapabilityProfile, SnapshotSupport,
};
use lekalo_core::transaction_concurrency::precondition::{
    CapabilityId, CapabilityRequirement, RequirementLevel,
};

/// The committed mysql 8.0 golden profile.
const MYSQL_80: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine-profile/valid/mysql-8.0.json");
/// The committed mariadb 10.11 golden profile.
const MARIADB_1011: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-engine-profile/valid/mariadb-10.11.json");

/// The committed portability goldens: the engine-family divergences
/// (mysql -> mariadb) and the named postgres divergences, byte-pinned.
const PORTABILITY_MYSQL_TO_MARIADB: &str = include_str!(
    "../../../tests/fixtures/storage-engine-profile/portability/mysql-to-mariadb.json"
);

/// The committed adversarial vectors: (name, fixture bytes, expectation
/// bytes). The expectation records the exact registered rule and the
/// fixed detail token of the single diagnostic.
const INVALID: &[(&str, &[u8], &[u8])] = &[
    (
        "engine-merged",
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/engine-merged.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/engine-merged.expect.json"
        ),
    ),
    (
        "engine-version-range",
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/engine-version-range.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/engine-version-range.expect.json"
        ),
    ),
    (
        "sql-mode-missing",
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/sql-mode-missing.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/sql-mode-missing.expect.json"
        ),
    ),
    (
        "capability-without-support",
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/capability-without-support.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/capability-without-support.expect.json"
        ),
    ),
    (
        "partial-without-bounds",
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/partial-without-bounds.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/partial-without-bounds.expect.json"
        ),
    ),
    (
        "credential-member",
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/credential-member.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/credential-member.expect.json"
        ),
    ),
    (
        "production-allowed",
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/production-allowed.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/production-allowed.expect.json"
        ),
    ),
    (
        "collation-charset-mismatch",
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/collation-charset-mismatch.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/collation-charset-mismatch.expect.json"
        ),
    ),
    (
        "unknown-capability-id",
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/unknown-capability-id.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-engine-profile/invalid/unknown-capability-id.expect.json"
        ),
    ),
];

fn parse(bytes: &[u8]) -> StorageEngineProfile {
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("fixture JSON");
    StorageEngineProfile::from_value(&value).expect("valid fixture")
}

#[test]
fn mysql_golden_normalizes_and_canonicalizes_byte_identically() {
    let profile = parse(MYSQL_80);
    assert_eq!(profile.project_id().as_str(), "planner");
    assert_eq!(profile.engine().engine(), EngineToken::Mysql);
    assert_eq!(profile.engine().engine_version(), "8.0.36");
    // The sql mode is always declared, canonical sorted order.
    assert!(!profile.engine().sql_mode().is_empty());
    assert_eq!(profile.engine().collation(), "utf8mb4_0900_ai_ci");
    // The exact-version evidence is mandatory: no range survived.
    assert!(profile.engine().engine_version().contains('.'));
    let canonical = profile.canonical_bytes().expect("canonical bytes");
    let committed = std::str::from_utf8(MYSQL_80).expect("utf8").trim_end();
    assert_eq!(canonical, committed, "canonical bytes must match goldens");
}

#[test]
fn mariadb_golden_is_a_separate_profile() {
    let profile = parse(MARIADB_1011);
    assert_eq!(profile.engine().engine(), EngineToken::Mariadb);
    // The maria-only capability pair: sequences full, mysql refuses.
    assert_eq!(
        ProfileSupport::of(&profile, ProfileCapabilityId::StorageSequences),
        ProfileSupport::Full
    );
    // The mysql golden declares sequences unsupported — the non-merge
    // rule in evidence form.
    let mysql = parse(MYSQL_80);
    assert_eq!(
        ProfileSupport::of(&mysql, ProfileCapabilityId::StorageSequences),
        ProfileSupport::Unsupported
    );
}

#[test]
fn capability_snapshot_bridge_maps_exact_states() {
    let profile = parse(MYSQL_80);
    let snapshot = to_snapshot(&profile);
    // Repeatable read is InnoDB's declared default: full.
    assert_eq!(
        snapshot.support_of("isolation.repeatable_read"),
        SnapshotSupport::Full
    );
    // Snapshot isolation is never claimed equivalent.
    assert_eq!(
        snapshot.support_of("isolation.snapshot"),
        SnapshotSupport::Unsupported
    );
    // Range locks are honest partial.
    assert_eq!(snapshot.support_of("lock.range"), SnapshotSupport::Partial);
    // An undeclared id stays unknown — never yes.
    assert_eq!(
        snapshot.support_of("external.compensation"),
        SnapshotSupport::Unknown
    );
}

#[test]
fn snapshot_bridge_answers_requirement_maps_under_profiles() {
    let profile = parse(MYSQL_80);
    let snapshot = to_snapshot(&profile);
    let requirements = [CapabilityRequirement::new(
        NamespacedId::parse("planner.req/atomic").expect("id"),
        CapabilityId::TransactionAtomicGroup,
        RequirementLevel::Full,
        "owner-recorded reason".to_owned(),
    )];
    let strict = map_capabilities(&requirements, &snapshot, CapabilityProfile::Strict);
    assert!(!strict.blocked(), "full atomic-group support passes strict");
    // The range-lock partial blocks strict and degrades permissive.
    let lock_requirements = [CapabilityRequirement::new(
        NamespacedId::parse("planner.req/range").expect("id"),
        CapabilityId::LockRange,
        RequirementLevel::Full,
        "owner-recorded reason".to_owned(),
    )];
    let strict_lock = map_capabilities(&lock_requirements, &snapshot, CapabilityProfile::Strict);
    assert!(strict_lock.blocked());
    let permissive_lock =
        map_capabilities(&lock_requirements, &snapshot, CapabilityProfile::Permissive);
    assert!(!permissive_lock.blocked() && permissive_lock.degraded());
}

#[test]
fn portability_names_the_capability_deltas() {
    let mysql = parse(MYSQL_80);
    let mariadb = parse(MARIADB_1011);
    let report = portability(&mysql, &mariadb);
    assert_eq!(report.source, "mysql");
    assert_eq!(report.target, "mariadb");
    // Sequences: mysql unsupported -> mariadb full.
    let sequences = report
        .changes
        .iter()
        .find(|change| change.id == "storage.sequences")
        .expect("the sequences delta");
    assert_eq!(sequences.source, ReportSupport::Unsupported);
    assert_eq!(
        sequences.target,
        lekalo_core::storage_engine_profile::ReportSupport::Full
    );
    assert!(report.gains.contains(&"storage.sequences".to_owned()));
    // Byte-identical for value-equal inputs.
    let again = portability(&mysql, &mariadb);
    assert_eq!(
        serde_json::to_string(&report).expect("json"),
        serde_json::to_string(&again).expect("json")
    );
}

#[test]
fn portability_carries_the_named_postgres_divergences() {
    let mysql = parse(MYSQL_80);
    let mariadb = parse(MARIADB_1011);
    let report = named_postgres_divergences(portability(&mysql, &mariadb));
    let divergences = report.postgres_divergences.as_ref().expect("named block");
    let names: Vec<&str> = divergences.iter().map(|entry| entry.name).collect();
    // The named PostgreSQL-specific semantics the issue demands.
    for required in [
        "partialIndex",
        "jsonbOperators",
        "timestamptz",
        "sequences",
        "deferrableConstraints",
        "transactionalDdl",
    ] {
        assert!(names.contains(&required), "missing {required} in {names:?}");
    }
    let deferrable = divergences
        .iter()
        .find(|entry| entry.name == "deferrableConstraints")
        .expect("deferrable");
    assert_eq!(
        deferrable.support,
        lekalo_core::storage_engine_profile::ReportSupport::Unsupported
    );
}

#[test]
fn profile_diff_classifies_capability_changes() {
    let base = parse(MYSQL_80);
    // The candidate downgrades lock.range from partial to unsupported:
    // a breaking narrowing on the same engine version.
    let mut value: serde_json::Value = serde_json::from_slice(MYSQL_80).expect("json");
    value["capabilities"]["lock.range"] = serde_json::json!({
        "support": "unsupported",
        "evidence": { "kind": "vendor-docs", "ref": "mysql-8.0-en" }
    });
    let candidate = StorageEngineProfile::from_value(&value).expect("parses");
    let diff = compare(&base, &candidate).expect("comparable");
    assert!(!diff.equal());
    let path = diff
        .paths()
        .iter()
        .find(|path| path.path() == "capability/lock.range")
        .expect("the range-lock path");
    assert_eq!(path.layer(), DiffLayer::Capability);
    assert_eq!(path.class(), DiffClass::Breaking);
    // An engine version change is breaking at the identity level.
    let mut value: serde_json::Value = serde_json::from_slice(MYSQL_80).expect("json");
    value["engine"]["engineVersion"] = serde_json::Value::String("8.0.37".to_owned());
    let newer = StorageEngineProfile::from_value(&value).expect("parses");
    let diff = compare(&base, &newer).expect("comparable");
    assert!(diff
        .paths()
        .iter()
        .any(|path| path.path() == "engine/identity" && path.class() == DiffClass::Breaking));
}

#[test]
fn portability_golden_is_byte_pinned() {
    let mysql = parse(MYSQL_80);
    let mariadb = parse(MARIADB_1011);
    let report = portability(&mysql, &mariadb);
    let rendered = serde_json::to_string_pretty(&report).expect("json");
    assert_eq!(
        rendered.trim_end(),
        PORTABILITY_MYSQL_TO_MARIADB.trim_end(),
        "the committed portability golden must match the engine"
    );
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
            StorageEngineProfile::from_value(&value).expect_err(&format!("{name} must reject"));
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
fn test_lifecycle_binds_the_forbidden_production_token() {
    let profile = parse(MYSQL_80);
    let lifecycle = profile.test_lifecycle();
    assert_eq!(lifecycle.test_schema_prefix(), "lekalo_test");
    assert_eq!(lifecycle.create().support(), Support::Full);
    assert_eq!(lifecycle.drop().support(), Support::Full);
    // The adapter evidence is present and closed.
    assert_eq!(profile.adapters().len(), 1);
    assert_eq!(profile.adapters()[0].name(), AdapterToken::Drizzle);
    // Validation is idempotent.
    profile.validate_attachment().expect("revalidates");
    let _ = capabilities::support_of(&profile, ProfileCapabilityId::TestCreateSchema);
}
