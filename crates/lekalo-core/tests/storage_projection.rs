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

/// The derived projection goldens: four renderings of one domain model.
const DERIVED_POSTGRES: &str =
    include_str!("../../../tests/fixtures/storage-projection/derived/postgres.json");
const DERIVED_LARAVEL: &str =
    include_str!("../../../tests/fixtures/storage-projection/derived/laravel.json");
const DERIVED_MYSQL: &str =
    include_str!("../../../tests/fixtures/storage-projection/derived/mysql.json");
const DERIVED_MARIADB: &str =
    include_str!("../../../tests/fixtures/storage-projection/derived/mariadb.json");

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
const DIFF_COLLATION: &[u8] =
    include_bytes!("../../../tests/fixtures/storage-projection/diff/candidate-collation.json");

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
        "migration-history-rejected",
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/migration-history-rejected.json"
        ),
        include_bytes!(
            "../../../tests/fixtures/storage-projection/invalid/migration-history-rejected.expect.json"
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
    assert_eq!(attachment.attachment_revision().as_str(), "0.4.0");
    assert_eq!(attachment.entities().len(), 7);
    assert_eq!(attachment.relations().len(), 7);
    assert_eq!(attachment.projections().len(), 4);
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

/// The collation-sensitive-uniqueness headline (issue #117, ADR-0042
/// §6): a collation or text-defaults change is breaking with destructive
/// data risk, and an index member change classifies by engine
/// semantics — never folded into a silent aggregate.
#[test]
fn collation_and_index_member_changes_classify_in_the_diff() {
    let base = parse(DIFF_BASE);
    let changed = parse(DIFF_COLLATION);
    let diff = compare(&base, &changed).expect("comparable");
    assert!(!diff.equal());
    // The mysql table collation flipped utf8mb4_0900_ai_ci → utf8mb4_bin
    // on the table carrying the unique textual index: breaking +
    // destructive. The projection textDefaults flipped with it.
    let find = |path: &str| {
        diff.paths()
            .iter()
            .find(|entry| entry.path() == path)
            .unwrap_or_else(|| panic!("path {path}"))
    };
    let collation = find("storage/mysql/tables/tag/charsetCollation");
    assert_eq!(collation.layer(), DiffLayer::Storage);
    assert_eq!(collation.class(), DiffClass::Breaking);
    assert_eq!(collation.risk(), Some(DataRisk::Destructive));
    let defaults = find("storage/mysql/textDefaults");
    assert_eq!(defaults.class(), DiffClass::Breaking);
    assert_eq!(defaults.risk(), Some(DataRisk::Destructive));
    // Non-mysql namespaces untouched by the candidate stay equal.
    assert!(!diff
        .paths()
        .iter()
        .any(|path| path.path().starts_with("storage/postgres")));
    // An index member change (kind or prefixLengths) on the named
    // tenant index surfaces at its own path with the rewrite
    // obligation, not as the silent aggregate.
    let mut member: serde_json::Value = serde_json::from_slice(DIFF_BASE).expect("json");
    member["projections"]
        .as_array_mut()
        .expect("projections")
        .iter_mut()
        .for_each(|projection| {
            if projection["namespace"] == "mysql" {
                for table in projection["tables"].as_array_mut().expect("tables") {
                    if table["entity"] == "task" {
                        table["indexes"][1]["prefixLengths"] = serde_json::json!([3072]);
                    }
                }
            }
        });
    let member_candidate = StorageProjectionAttachment::from_value(&member).expect("parses");
    let member_diff = compare(&base, &member_candidate).expect("comparable");
    let member_path = member_diff
        .paths()
        .iter()
        .find(|path| path.path() == "storage/mysql/tables/task/indexes/idx_task_tenant")
        .expect("the exact member path");
    assert_eq!(member_path.class(), DiffClass::PolicyChange);
    assert_eq!(member_path.risk(), Some(DataRisk::Destructive));
    // Dropping uniqueness from the named unique external-identity index
    // is a breaking narrowing.
    let mut ununique: serde_json::Value = serde_json::from_slice(DIFF_BASE).expect("json");
    ununique["projections"]
        .as_array_mut()
        .expect("projections")
        .iter_mut()
        .for_each(|projection| {
            if projection["namespace"] == "mysql" {
                for table in projection["tables"].as_array_mut().expect("tables") {
                    if table["entity"] == "task_external_link" {
                        table["indexes"][0]["unique"] = serde_json::Value::Bool(false);
                    }
                }
            }
        });
    let ununique_candidate = StorageProjectionAttachment::from_value(&ununique).expect("parses");
    let ununique_diff = compare(&base, &ununique_candidate).expect("comparable");
    let ununique_path = ununique_diff
        .paths()
        .iter()
        .find(|path| {
            path.path() == "storage/mysql/tables/task_external_link/indexes/uq_external_identity"
        })
        .expect("the unique-narrowing path");
    assert_eq!(ununique_path.class(), DiffClass::Breaking);
    assert_eq!(ununique_path.risk(), Some(DataRisk::Destructive));
    // The same narrowing on an ANONYMOUS unique index (the tag table's
    // unnamed `label` index) must classify identically at its own
    // column-list path — uniqueness is a member, not identity, so the
    // change cannot hide in the aggregate policy path (round-3 F-2).
    let mut anon: serde_json::Value = serde_json::from_slice(DIFF_BASE).expect("json");
    anon["projections"]
        .as_array_mut()
        .expect("projections")
        .iter_mut()
        .for_each(|projection| {
            if projection["namespace"] == "mysql" {
                for table in projection["tables"].as_array_mut().expect("tables") {
                    if table["entity"] == "tag" {
                        table["indexes"][0]["unique"] = serde_json::Value::Bool(false);
                    }
                }
            }
        });
    let anon_candidate = StorageProjectionAttachment::from_value(&anon).expect("parses");
    let anon_diff = compare(&base, &anon_candidate).expect("comparable");
    let anon_path = anon_diff
        .paths()
        .iter()
        .find(|path| path.path() == "storage/mysql/tables/tag/indexes/label")
        .expect("the anonymous narrowing path");
    assert_eq!(anon_path.class(), DiffClass::Breaking);
    assert_eq!(anon_path.risk(), Some(DataRisk::Destructive));
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
        (Namespace::Mysql, DERIVED_MYSQL),
        (Namespace::Mariadb, DERIVED_MARIADB),
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
    let mysql = project(&attachment, Namespace::Mysql).expect("derives");
    let mariadb = project(&attachment, Namespace::Mariadb).expect("derives");
    // Every local entity is mapped exactly once in every rendering.
    for derived in [&postgres, &laravel, &mysql, &mariadb] {
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

/// The MySQL-family namespace arms (issue #117): the type tables, the
/// instant render, the instant-refusal surface, and the byte-identity
/// of the committed goldens over the same domain model.
#[test]
fn mysql_family_namespaces_derive_their_type_tables() {
    let attachment = parse(VALID);
    let mysql = project(&attachment, Namespace::Mysql).expect("derives");
    let mariadb = project(&attachment, Namespace::Mariadb).expect("derives");
    fn task_of(
        derived: &lekalo_core::storage_projection::DerivedProjection,
    ) -> &lekalo_core::storage_projection::DerivedTable {
        derived
            .table(&EntityKey::parse("task").expect("key"))
            .expect("task table")
    }
    // One domain model, four explicit type renders.
    let cases = [
        (
            task_of(&mysql),
            "tinyint(1)",
            "bigint",
            "varchar(200)",
            "binary(16)",
            "datetime(6)",
        ),
        (
            task_of(&mariadb),
            "tinyint(1)",
            "bigint",
            "varchar(200)",
            "binary(16)",
            "datetime(6)",
        ),
    ];
    for (table, boolean, integer, string, uuid, instant) in cases {
        let find = |name: &str| {
            table
                .columns()
                .iter()
                .find(|column| column.name().as_str() == name)
                .unwrap_or_else(|| panic!("column {name}"))
                .storage_type()
        };
        // Boolean fields are not declared on the planner task; the
        // instant and string renders are visible directly.
        assert_eq!(find("title"), string);
        assert_eq!(find("created_at"), instant);
        let _ = (boolean, integer, uuid);
    }
    // The foreign-key and discriminator columns resolve through the
    // mysql type table: binary(16) uuid, varchar(64) discriminator.
    let detail = mysql
        .table(&EntityKey::parse("task_detail").expect("key"))
        .expect("task_detail");
    let key = detail
        .columns()
        .iter()
        .find(|column| column.name().as_str() == "task_id")
        .expect("task_id");
    assert_eq!(key.storage_type(), "binary(16)");
    let comment = mysql
        .table(&EntityKey::parse("comment").expect("key"))
        .expect("comment");
    let discriminator = comment
        .columns()
        .iter()
        .find(|column| column.name().as_str() == "target_type")
        .expect("target_type");
    assert_eq!(discriminator.storage_type(), "varchar(64)");
    // Determinism: deriving twice is byte-identical.
    let again = project(&attachment, Namespace::Mysql).expect("derives");
    assert_eq!(
        canonical::derived_bytes(&mysql).expect("bytes"),
        canonical::derived_bytes(&again).expect("bytes")
    );
}

/// The MySQL namespace vocabulary is closed at the typed layer: the
/// MariaDB-only `uuid` token refuses, and the declared projection
/// validation surfaces the sequence refusal in the mysql namespace.
#[test]
fn mysql_namespace_stays_separate_from_mariadb() {
    assert!(Namespace::Mysql.accepts_storage_type("json"));
    assert!(!Namespace::Mysql.accepts_storage_type("uuid"));
    assert!(Namespace::Mariadb.accepts_storage_type("uuid"));
    assert!(Namespace::Mysql.is_mysql_family());
    assert!(Namespace::Mariadb.is_mysql_family());
    assert!(!Namespace::Postgres.is_mysql_family());
    assert!(!Namespace::Laravel.is_mysql_family());
    // The mysql render of a sequence-bearing table would be refused at
    // validation: the golden declares identity in mysql and sequence in
    // mariadb, exactly the divergent capability pair the profile
    // evidence later binds.
    let attachment = parse(VALID);
    let mysql = project(&attachment, Namespace::Mysql).expect("derives");
    let session = mysql
        .table(&EntityKey::parse("focus_session").expect("key"))
        .expect("focus_session");
    let session_no = session
        .columns()
        .iter()
        .find(|column| column.name().as_str() == "session_no")
        .expect("session_no");
    assert_eq!(session_no.storage_type(), "bigint");
}

/// Prefix obligation is decided per resolved spelling, not family:
/// fixed-width `binary(16)` (the uuid/FK render) indexes without a
/// spurious prefix, while unbounded `text` still refuses. The
/// dominant index shape in real mysql schemas is now ceremony-free
/// (round-4 review F-3).
#[test]
fn prefix_required_decides_per_resolved_spelling() {
    // (a) uuid pk index (binary(16)) without a prefix: valid.
    let mut value: serde_json::Value = serde_json::from_slice(VALID).expect("json");
    for projection in value["projections"].as_array_mut().expect("projections") {
        if projection["namespace"] == "mysql" {
            for table in projection["tables"].as_array_mut().expect("tables") {
                if table["entity"] == "tag" {
                    table["indexes"]
                        .as_array_mut()
                        .expect("indexes")
                        .push(serde_json::json!({"columns": ["id"], "unique": false}));
                }
            }
        }
    }
    StorageProjectionAttachment::from_value(&value)
        .expect("parses")
        .validate_attachment()
        .expect("binary(16) indexes without a prefix");
    // (b) FK index (binary(16) via the referenced pk) without a
    // prefix: valid.
    let mut value: serde_json::Value = serde_json::from_slice(VALID).expect("json");
    for projection in value["projections"].as_array_mut().expect("projections") {
        if projection["namespace"] == "mysql" {
            for table in projection["tables"].as_array_mut().expect("tables") {
                if table["entity"] == "task_external_link" {
                    table["indexes"]
                        .as_array_mut()
                        .expect("indexes")
                        .push(serde_json::json!({"columns": ["task_id"], "unique": false}));
                }
            }
        }
    }
    StorageProjectionAttachment::from_value(&value)
        .expect("parses")
        .validate_attachment()
        .expect("FK binary(16) indexes without a prefix");
    // (c) unbounded text still refuses without a prefix.
    let mut value: serde_json::Value = serde_json::from_slice(VALID).expect("json");
    for projection in value["projections"].as_array_mut().expect("projections") {
        if projection["namespace"] == "mysql" {
            for table in projection["tables"].as_array_mut().expect("tables") {
                if table["entity"] == "task" {
                    table["indexes"]
                        .as_array_mut()
                        .expect("indexes")
                        .push(serde_json::json!({"columns": ["note"], "unique": false}));
                }
            }
        }
    }
    let error = StorageProjectionAttachment::from_value(&value)
        .expect_err("unbounded text without a prefix refuses");
    let rendered = serde_json::to_string(&error).expect("diagnostic json");
    assert!(rendered.contains("prefix-required"));
}

/// The valid coverage vector for the round-4 F-1 fix: a mixed
/// textual+non-textual composite index with sparse prefixLengths
/// `[16, null]` validates, and the sparse positions survive
/// normalization (null = no prefix) and canonical re-rendering.
#[test]
fn mixed_textual_and_non_textual_composite_index_is_expressible() {
    const MIXED: &[u8] = include_bytes!(
        "../../../tests/fixtures/storage-projection/valid/mixed-composite-prefix.json"
    );
    let attachment = parse(MIXED);
    attachment
        .validate_attachment()
        .expect("the mixed composite validates");
    let projection = attachment
        .projection(Namespace::Mysql)
        .expect("mysql projection");
    let task = projection
        .tables()
        .iter()
        .find(|table| table.entity().as_str() == "task")
        .expect("task table");
    let mixed = task
        .indexes()
        .iter()
        .find(|index| index.columns().len() == 2)
        .expect("the mixed composite index");
    let lengths = mixed.prefix_lengths().expect("sparse lengths");
    assert_eq!(lengths[0], Some(16), "the textual member keeps its prefix");
    assert_eq!(lengths[1], None, "the non-textual member declares none");
    // Canonical bytes re-render the sparse position as null.
    let bytes = canonical::attachment_bytes(&attachment).expect("canonical");
    assert!(bytes.contains("prefixLengths\":[16,null]"));
    // Deriving the namespace still works over the mixed index.
    project(&attachment, Namespace::Mysql).expect("derives");
}

/// A grammar-legal pk→fk cycle (a primary key naming a foreign-key
/// column, resolution crossing the self-referencing FK) must refuse
/// with the typed `cyclic-key-resolution` rule — never overflow the
/// stack (round-3 review F-1). Both reproductions from the review are
/// pinned: the self-FK pk cycle, and the cross-table chain where
/// task_detail's index on task_id resolves into task's self-FK pk.
#[test]
fn cyclic_key_resolution_refuses_instead_of_crashing() {
    // (a) self-FK cycle: task's pk names its own optional_reference FK
    // column, and an index keys on it.
    let mut value: serde_json::Value = serde_json::from_slice(VALID).expect("json");
    for projection in value["projections"].as_array_mut().expect("projections") {
        if projection["namespace"] == "mysql" {
            for table in projection["tables"].as_array_mut().expect("tables") {
                if table["entity"] == "task" {
                    table["primaryKey"] = serde_json::json!(["parent_task_id"]);
                    table["indexes"]
                        .as_array_mut()
                        .expect("indexes")
                        .push(serde_json::json!({"columns": ["parent_task_id"], "unique": false}));
                }
            }
        }
    }
    let error =
        StorageProjectionAttachment::from_value(&value).expect_err("a pk→fk cycle must refuse");
    let rendered = serde_json::to_string(&error).expect("diagnostic json");
    assert!(
        rendered.contains("cyclic-key-resolution"),
        "expected the cyclic refusal, got {rendered}"
    );
    // (b) cross-table chain: task_detail's index on task_id resolves
    // into task's self-FK pk, which resolves back through the same
    // chain — bounded, refused, no crash.
    let mut value: serde_json::Value = serde_json::from_slice(VALID).expect("json");
    for projection in value["projections"].as_array_mut().expect("projections") {
        if projection["namespace"] == "mysql" {
            for table in projection["tables"].as_array_mut().expect("tables") {
                if table["entity"] == "task" {
                    table["primaryKey"] = serde_json::json!(["parent_task_id"]);
                }
                if table["entity"] == "task_detail" {
                    table["indexes"] =
                        serde_json::json!([{"columns": ["task_id"], "unique": false}]);
                }
            }
        }
    }
    let error = StorageProjectionAttachment::from_value(&value)
        .expect_err("the cross-table cycle must refuse");
    let rendered = serde_json::to_string(&error).expect("diagnostic json");
    assert!(
        rendered.contains("cyclic-key-resolution"),
        "expected the cyclic refusal, got {rendered}"
    );
}

/// A foreign key whose referenced side is an external (unmapped)
/// entity is non-cyclic and unresolvable: the index rules skip the
/// column honestly instead of firing the lying cyclic refusal
/// (round-4 review F-2).
#[test]
fn external_entity_foreign_key_index_skips_prefix_rules_honestly() {
    let mut value: serde_json::Value = serde_json::from_slice(VALID).expect("json");
    // Flip the external_reference relation into an FK-bearing relation
    // owned by the external jira_issue entity, and add a matching field
    // plus an index on the FK column in the mysql projection.
    let relation = value["relations"]
        .as_array_mut()
        .expect("relations")
        .iter_mut()
        .find(|relation| relation["relationId"] == "planner.relation.link_provider")
        .expect("link_provider");
    relation["kind"] = serde_json::json!("one_to_many");
    relation["deleteBehavior"] = serde_json::json!("restrict");
    relation["owner"] = serde_json::json!("jira_issue");
    relation["target"] = serde_json::json!("task_external_link");
    relation["foreignKey"] = serde_json::json!("jira_id");
    let external = value["entities"]
        .as_array_mut()
        .expect("entities")
        .iter_mut()
        .find(|entity| entity["entityKey"] == "jira_issue")
        .expect("jira_issue");
    external["fields"]
        .as_array_mut()
        .expect("fields")
        .push(serde_json::json!({
            "field": "jira_id",
            "required": true,
            "type": {"name": "uuid"},
            "visibility": "internal"
        }));
    for projection in value["projections"].as_array_mut().expect("projections") {
        if projection["namespace"] == "mysql" {
            for table in projection["tables"].as_array_mut().expect("tables") {
                if table["entity"] == "task_external_link" {
                    table["indexes"]
                        .as_array_mut()
                        .expect("indexes")
                        .push(serde_json::json!({"columns": ["jira_id"], "unique": false}));
                }
            }
        }
    }
    let attachment = StorageProjectionAttachment::from_value(&value).expect("parses");
    attachment
        .validate_attachment()
        .expect("a non-cyclic unresolvable reference validates");
}

/// Collation-sensitive uniqueness is visible in the derived surface:
/// the declared tag table carries its collation evidence, and the
/// derived unique index over the textual column stays visible with it.
#[test]
fn mysql_collation_evidence_stays_visible() {
    let attachment = parse(VALID);
    let mysql = project(&attachment, Namespace::Mysql).expect("derives");
    let tag = mysql
        .table(&EntityKey::parse("tag").expect("key"))
        .expect("tag");
    let unique = tag
        .indexes()
        .iter()
        .find(|index| index.unique())
        .expect("the declared unique index");
    assert_eq!(
        unique
            .columns()
            .iter()
            .map(|column| column.as_str())
            .collect::<Vec<_>>(),
        vec!["label"]
    );
    // The declared projection side keeps the collation members.
    let projection = attachment.projection(Namespace::Mysql).expect("mysql");
    let (charset, collation) = projection.text_defaults().expect("textDefaults");
    assert_eq!(charset, "utf8mb4");
    assert_eq!(collation, "utf8mb4_0900_ai_ci");
    let tag_table = projection
        .tables()
        .iter()
        .find(|table| table.entity().as_str() == "tag")
        .expect("tag table");
    assert_eq!(tag_table.collation(), Some("utf8mb4_0900_ai_ci"));
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
