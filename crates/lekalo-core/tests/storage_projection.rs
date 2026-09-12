//! Issue #65 integration tests: wire normalization over the committed
//! goldens, byte-identical canonicalization, permutation determinism,
//! adversarial rejection with exact registered rules, the layer-
//! classifying diff (domain, wire, storage) with visible data risk,
//! the derivation of the PostgreSQL and Laravel projections from the
//! same domain model, and the public DTO projection that storage-only
//! technical columns can never reach.
//!
//! The tests are hermetic: every fixture is embedded at compile time,
//! nothing touches the network, and no report or cache is written.

use lekalo_core::storage_projection::{
    canonical, compare, project, public_fields, ColumnOrigin, DataRisk, DiffClass, DiffLayer,
    DomainType, EntityKey, Namespace, OnDelete, StorageProjectionAttachment, Visibility,
};

/// The committed valid golden.
const VALID: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-projection/valid/planner-storage.json");

/// The derived projection goldens: two renderings of one domain model.
const DERIVED_POSTGRES: &str =
    include_str!("../../../tests/fixtures/storage-projection/derived/postgres.json");
const DERIVED_LARAVEL: &str =
    include_str!("../../../tests/fixtures/storage-projection/derived/laravel.json");

/// The diff vectors: base, domain rename, storage-only change, and the
/// pure permutation.
const DIFF_BASE: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-projection/diff/base.json");
const DIFF_RENAME: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-projection/diff/candidate-rename.json");
const DIFF_STORAGE: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-projection/diff/candidate-storage.json");
const DIFF_PERMUTATION: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-projection/diff/candidate-permutation.json");

/// The committed adversarial vectors: (name, fixture bytes, expectation
/// bytes). The expectation records the exact registered rule and the
/// fixed detail token of the single diagnostic; vectors that also bind
/// a subject echo assert it.
const INVALID: &[(&str, &[u8], &[u8])] = &[
    (
        "aggregate-both",
        include_bytes!("../../../tests/fixtures/storage-projection/invalid/aggregate-both.json"),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/aggregate-both.expect.json"
        ),
    ),
    (
        "aggregate-child-detach",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/aggregate-child-detach.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/aggregate-child-detach.expect.json"
        ),
    ),
    (
        "aggregate-child-unowned-target",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/aggregate-child-unowned-target.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/aggregate-child-unowned-target.expect.json"
        ),
    ),
    (
        "aggregate-owner-missing",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/aggregate-owner-missing.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/aggregate-owner-missing.expect.json"
        ),
    ),
    (
        "aggregate-owner-not-root",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/aggregate-owner-not-root.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/aggregate-owner-not-root.expect.json"
        ),
    ),
    (
        "bad-attachment-revision",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/bad-attachment-revision.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/bad-attachment-revision.expect.json"
        ),
    ),
    (
        "column-collision",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/column-collision.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/column-collision.expect.json"
        ),
    ),
    (
        "coverage-missing",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/coverage-missing.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/coverage-missing.expect.json"
        ),
    ),
    (
        "decimal-scale-above-precision",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/decimal-scale-above-precision.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/decimal-scale-above-precision.expect.json"
        ),
    ),
    (
        "detach-minimum",
        include_bytes!("../../../tests/fixtures/storage-projection/invalid/detach-minimum.json"),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/detach-minimum.expect.json"
        ),
    ),
    (
        "duplicate-entity-key",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/duplicate-entity-key.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/duplicate-entity-key.expect.json"
        ),
    ),
    (
        "duplicate-field",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/field-duplicate.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/field-duplicate.expect.json"
        ),
    ),
    (
        "duplicate-namespace",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/duplicate-namespace.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/duplicate-namespace.expect.json"
        ),
    ),
    (
        "duplicate-relation-id",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/duplicate-relation-id.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/duplicate-relation-id.expect.json"
        ),
    ),
    (
        "duplicate-table-name",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/duplicate-table-name.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/duplicate-table-name.expect.json"
        ),
    ),
    (
        "external-aggregate",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/external-aggregate.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/external-aggregate.expect.json"
        ),
    ),
    (
        "external-entity-projected",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/external-entity-projected.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/external-entity-projected.expect.json"
        ),
    ),
    (
        "external-reference-cascade",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/external-reference-cascade.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/external-reference-cascade.expect.json"
        ),
    ),
    (
        "external-reference-non-external-target",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/external-reference-non-external-target.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/external-reference-non-external-target.expect.json"
        ),
    ),
    (
        "field-duplicate",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/field-duplicate.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/field-duplicate.expect.json"
        ),
    ),
    (
        "foreign-key-forbidden",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/foreign-key-forbidden.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/foreign-key-forbidden.expect.json"
        ),
    ),
    (
        "foreign-key-required",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/foreign-key-required.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/foreign-key-required.expect.json"
        ),
    ),
    (
        "join-kind",
        include_bytes!("../../../tests/fixtures/storage-projection/invalid/join-kind.json"),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/join-kind.expect.json"
        ),
    ),
    (
        "join-unknown-relation",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/join-unknown-relation.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/join-unknown-relation.expect.json"
        ),
    ),
    (
        "migration-unknown-table",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/migration-unknown-table.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/migration-unknown-table.expect.json"
        ),
    ),
    (
        "min-above-max",
        include_bytes!("../../../tests/fixtures/storage-projection/invalid/min-above-max.json"),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/min-above-max.expect.json"
        ),
    ),
    (
        "missing-join",
        include_bytes!("../../../tests/fixtures/storage-projection/invalid/missing-join.json"),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/missing-join.expect.json"
        ),
    ),
    (
        "missing-polymorphic-materialization",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/missing-polymorphic-materialization.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/missing-polymorphic-materialization.expect.json"
        ),
    ),
    (
        "one-to-many-max-one",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/one-to-many-max-one.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/one-to-many-max-one.expect.json"
        ),
    ),
    (
        "one-to-one-max-two",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/one-to-one-max-two.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/one-to-one-max-two.expect.json"
        ),
    ),
    (
        "optional-reference-cascade",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/optional-reference-cascade.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/optional-reference-cascade.expect.json"
        ),
    ),
    (
        "optional-reference-min-one",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/optional-reference-min-one.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/optional-reference-min-one.expect.json"
        ),
    ),
    (
        "polymorphic-kind",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/polymorphic-kind.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/polymorphic-kind.expect.json"
        ),
    ),
    (
        "relation-unknown-target",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/relation-unknown-target.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/relation-unknown-target.expect.json"
        ),
    ),
    (
        "string-without-length",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/string-without-length.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/string-without-length.expect.json"
        ),
    ),
    (
        "unknown-index-column",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/unknown-index-column.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/unknown-index-column.expect.json"
        ),
    ),
    (
        "unknown-primary-key-column",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/unknown-primary-key-column.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/unknown-primary-key-column.expect.json"
        ),
    ),
    (
        "unmapped-local-entity",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/unmapped-local-entity.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/unmapped-local-entity.expect.json"
        ),
    ),
    (
        "unknown-top-field",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/unknown-top-field.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/unknown-top-field.expect.json"
        ),
    ),
    (
        "wrong-contract-identity",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/wrong-contract-identity.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/wrong-contract-identity.expect.json"
        ),
    ),
    (
        "wrong-schema-version",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/wrong-schema-version.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/wrong-schema-version.expect.json"
        ),
    ),
];

fn parse(bytes: &[u8]) -> StorageProjectionAttachment {
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("fixture JSON");
    StorageProjectionAttachment::from_value(&value).expect("valid fixture")
}

#[test]
fn golden_normalizes_and_canonicalizes_byte_identically() {
    let attachment = parse(VALID);
    assert_eq!(attachment.project_id().as_str(), "planner");
    assert_eq!(attachment.attachment_revision().as_str(), "1.0.0");
    assert_eq!(attachment.entities().len(), 7);
    assert_eq!(attachment.relations().len(), 7);
    assert_eq!(attachment.projections().len(), 2);
    // The seven closed relation kinds are all exercised by the golden.
    let mut kinds: Vec<_> = attachment
        .relations()
        .iter()
        .map(|r| r.kind().key())
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(kinds.len(), 7, "every closed relation kind is covered");
    // The external provider entity is never projected.
    let jira = attachment
        .entity(&EntityKey::parse("jira_issue").expect("key"))
        .expect("entity");
    assert!(jira.external());
    for projection in attachment.projections() {
        assert!(projection
            .tables()
            .iter()
            .all(|table| table.entity().as_str() != "jira_issue"));
    }
    let canonical = attachment.canonical_bytes().expect("canonical bytes");
    let committed = std::str::from_utf8(VALID).expect("utf8").trim_end();
    assert_eq!(canonical, committed, "canonical bytes must match goldens");
}

#[test]
fn permutation_collapses_to_identical_canonical_bytes() {
    let base = parse(DIFF_BASE);
    let permuted = parse(DIFF_PERMUTATION);
    let base_bytes = base.canonical_bytes().expect("base canonical");
    let permuted_bytes = permuted.canonical_bytes().expect("permuted canonical");
    assert_eq!(base_bytes, permuted_bytes);
    let diff = compare(&base, &permuted).expect("comparable");
    assert!(diff.equal());
    assert!(diff.paths().is_empty());
}

#[test]
fn domain_rename_never_renames_a_table() {
    let base = parse(DIFF_BASE);
    let renamed = parse(DIFF_RENAME);
    let diff = compare(&base, &renamed).expect("comparable");
    assert!(!diff.equal());
    let paths: Vec<_> = diff
        .paths()
        .iter()
        .map(|path| {
            (
                path.path().to_owned(),
                path.layer().key(),
                path.class().key(),
            )
        })
        .collect();
    // The rebind of the Model symbol is one domain-layer path plus the
    // envelope revision; no storage path exists at all.
    assert!(
        paths
            .iter()
            .any(|(path, layer, class)| path == "domain/entities/task/entity"
                && *layer == "domain"
                && *class == "non-breaking"),
        "expected the domain rebind path in {paths:?}"
    );
    assert!(
        paths.contains(&(
            "wire/attachmentRevision".to_owned(),
            "wire",
            "policy-change"
        )),
        "expected the wire envelope path in {paths:?}"
    );
    assert!(
        paths.iter().all(|(_, layer, _)| *layer != "storage"),
        "a domain rename must never imply a storage change: {paths:?}"
    );
    // Both attachments derive the identical storage projection.
    for namespace in [Namespace::Postgres, Namespace::Laravel] {
        let base_projection = project(&base, namespace).expect("base derives");
        let renamed_projection = project(&renamed, namespace).expect("renamed derives");
        assert_eq!(
            canonical::derived_bytes(&base_projection).expect("bytes"),
            canonical::derived_bytes(&renamed_projection).expect("bytes"),
        );
    }
}

#[test]
fn storage_change_classifies_separately_with_visible_data_risk() {
    let base = parse(DIFF_BASE);
    let changed = parse(DIFF_STORAGE);
    let diff = compare(&base, &changed).expect("comparable");
    assert!(!diff.equal());
    let find = |prefix: &str| {
        diff.paths()
            .iter()
            .find(|path| path.path().starts_with(prefix))
    };
    let table =
        find("storage/laravel/tables/task/table").expect("the Laravel table rename must appear");
    assert_eq!(table.layer(), DiffLayer::Storage);
    assert_eq!(table.class(), DiffClass::Breaking);
    assert_eq!(table.risk(), Some(DataRisk::Destructive));
    let technical = find("storage/laravel/tables/task/technical/remember_token")
        .expect("the dropped technical column must appear");
    assert_eq!(technical.layer(), DiffLayer::Storage);
    assert_eq!(technical.class(), DiffClass::PolicyChange);
    assert_eq!(technical.risk(), Some(DataRisk::Destructive));
    // The domain layer is untouched by the storage-only candidate.
    assert!(
        diff.paths()
            .iter()
            .all(|path| path.layer() != DiffLayer::Domain),
        "a storage-only change must never classify as domain"
    );
}

#[test]
fn diff_addition_is_non_breaking_and_reversible() {
    let base = parse(DIFF_BASE);
    let changed = parse(DIFF_STORAGE);
    let forward = compare(&changed, &base).expect("comparable");
    assert!(!forward.equal());
    // Reverting the storage candidate drops the renamed table and the
    // technical column removal; the storage side is a pure restoration.
    assert!(forward
        .paths()
        .iter()
        .any(|path| path.layer() == DiffLayer::Storage));
}

#[test]
fn diff_rejects_foreign_projects() {
    let base = parse(DIFF_BASE);
    let mut value: serde_json::Value =
        serde_json::from_slice(DIFF_STORAGE).expect("candidate JSON");
    value["projectId"] = serde_json::Value::String("other".to_owned());
    let foreign = StorageProjectionAttachment::from_value(&value).expect("parses");
    let error = compare(&base, &foreign).expect_err("foreign project");
    let ids = error.reason_ids();
    assert_eq!(ids.first().copied(), Some("storage.diff-invalid"));
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
        let error = StorageProjectionAttachment::from_value(&value)
            .expect_err(&format!("{name} must reject"));
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
        if let Some(subject) = expected["subject"].as_str() {
            assert!(
                rendered.contains(subject),
                "{name}: subject {subject} missing from {rendered}"
            );
        }
    }
}

#[test]
fn bound_frontier_accepts_n_and_rejects_n_plus_one() {
    let value: serde_json::Value = serde_json::from_slice(VALID).expect("golden");
    let columns = |value: &mut serde_json::Value, count: usize| {
        let table = value["projections"][0]["tables"][0].take();
        let mut primary_key = Vec::new();
        for index in 0..count {
            primary_key.push(serde_json::Value::String(format!("pk_column_{index}")));
        }
        let mut table = table;
        table["primaryKey"] = serde_json::Value::Array(primary_key);
        value["projections"][0]["tables"][0] = table;
    };
    // The 8-column primary key is a legal declaration; it fails the
    // derivation only because columns pk_column_* do not resolve. The
    // 9-column key must fail normalization itself.
    let mut eight = value.clone();
    columns(&mut eight, 8);
    let parsed = StorageProjectionAttachment::from_value(&eight);
    assert!(parsed.is_err(), "unresolved key columns must refuse");
    let mut nine = value.clone();
    columns(&mut nine, 9);
    let error = StorageProjectionAttachment::from_value(&nine).expect_err("over bound");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("storage.input-invalid")
    );
}

#[test]
fn validation_is_idempotent_over_a_valid_attachment() {
    let attachment = parse(VALID);
    attachment
        .validate_attachment()
        .expect("revalidation succeeds");
}

#[test]
fn derived_projections_match_the_committed_goldens() {
    let attachment = parse(VALID);
    for (namespace, committed) in [
        (Namespace::Postgres, DERIVED_POSTGRES),
        (Namespace::Laravel, DERIVED_LARAVEL),
    ] {
        let derived = project(&attachment, namespace).expect("derives");
        let bytes = canonical::derived_bytes(&derived).expect("canonical");
        assert_eq!(bytes, committed.trim_end(), "{namespace:?} golden matches");
    }
}

#[test]
fn same_domain_model_derives_both_namespaces() {
    let attachment = parse(VALID);
    let postgres = project(&attachment, Namespace::Postgres).expect("derives");
    let laravel = project(&attachment, Namespace::Laravel).expect("derives");
    // Every local entity is mapped exactly once in both renderings.
    for derived in [&postgres, &laravel] {
        assert_eq!(derived.tables().len(), 6, "every local entity is mapped");
        assert_eq!(derived.joins().len(), 1, "the join table is materialized");
    }
    // The type tables differ per namespace for the same domain type.
    fn task_of(
        derived: &lekalo_core::storage_projection::DerivedProjection,
    ) -> &lekalo_core::storage_projection::DerivedTable {
        derived
            .table(&EntityKey::parse("task").expect("key"))
            .expect("task table")
    }
    let postgres_title = task_of(&postgres)
        .columns()
        .iter()
        .find(|column| column.name().as_str() == "title")
        .expect("title column");
    assert_eq!(postgres_title.storage_type(), "varchar(200)");
    let laravel_title = task_of(&laravel)
        .columns()
        .iter()
        .find(|column| column.name().as_str() == "title")
        .expect("title column");
    assert_eq!(laravel_title.storage_type(), "string(200)");
    // The declared delete behavior becomes the explicit foreign-key
    // action of the derived column.
    let detail = postgres
        .table(&EntityKey::parse("task_detail").expect("key"))
        .expect("task_detail table");
    let foreign_key = detail
        .foreign_keys()
        .iter()
        .find(|key| key.column().as_str() == "task_id")
        .expect("derived foreign key");
    assert_eq!(foreign_key.references_table().as_str(), "task");
    assert_eq!(foreign_key.on_delete(), OnDelete::Cascade);
    // Determinism: deriving twice is byte-identical.
    let again = project(&attachment, Namespace::Postgres).expect("derives");
    assert_eq!(
        canonical::derived_bytes(&postgres).expect("bytes"),
        canonical::derived_bytes(&again).expect("bytes")
    );
}

#[test]
fn derivation_refuses_a_missing_namespace() {
    let mut value: serde_json::Value = serde_json::from_slice(VALID).expect("golden");
    value["projections"] = value["projections"]
        .as_array()
        .expect("array")
        .iter()
        .filter(|projection| projection["namespace"] == "postgres")
        .cloned()
        .collect::<Vec<_>>()
        .into();
    let attachment = StorageProjectionAttachment::from_value(&value).expect("parses");
    let error = project(&attachment, Namespace::Laravel).expect_err("absent namespace");
    assert_eq!(
        error.reason_ids().first().copied(),
        Some("storage.mapping-invalid")
    );
}

#[test]
fn public_dto_never_includes_storage_technical_columns() {
    let attachment = parse(VALID);
    let task = public_fields(&attachment, &EntityKey::parse("task").expect("key"))
        .expect("entity resolves");
    let names: Vec<&str> = task.iter().map(|field| field.as_str()).collect();
    assert_eq!(names, vec!["due_date", "id", "title"]);
    // The Laravel task table declares the storage-only technical
    // column; the DTO derivation never sees it because it reads only
    // the domain declarations.
    let laravel = project(&attachment, Namespace::Laravel).expect("derives");
    let table = laravel
        .table(&EntityKey::parse("task").expect("key"))
        .expect("task table");
    assert!(table
        .columns()
        .iter()
        .any(|column| column.name().as_str() == "remember_token"));
    assert!(names.iter().all(|name| *name != "remember_token"));
    // Technical columns are private by construction; domain public
    // fields stay public.
    let remember = table
        .columns()
        .iter()
        .find(|column| column.name().as_str() == "remember_token")
        .expect("technical column");
    assert_eq!(remember.visibility(), Visibility::Private);
    assert_eq!(remember.origin(), ColumnOrigin::Technical);
}

#[test]
fn domain_type_widening_vocabulary_is_closed() {
    let narrow = DomainType::String { length: 64 };
    let wide = DomainType::String { length: 128 };
    assert!(narrow.widens(&wide));
    assert!(!wide.widens(&narrow));
    assert!(DomainType::Integer.widens(&DomainType::Decimal {
        precision: 10,
        scale: 2
    }));
    assert!(!DomainType::Boolean.widens(&DomainType::Integer));
}
