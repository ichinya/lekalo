//! The shared storage-component conformance battery (issue #69,
//! plan §5).
//!
//! One closed check catalog, fixed order, reused unchanged by every
//! engine profile (#117 reuses every check): each check either passes,
//! fails with a bounded reason, or skips with a bounded reason — a
//! skip is never a pass. The fixture-backed checks run entirely
//! offline over the profile, its bound projection, the optional
//! checked-mode evidence, and the optional engine input document;
//! adapter-executed checks ride the confined `TargetClient` under the
//! declared `scan.storage-schema` / `generate.storage-ddl`
//! capabilities and stay the adapter owner's integration surface.

use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::StorageProjectionAttachment;

use super::diagnostic;
use super::{compare_drift, input_document, IntrospectionEvidence, StorageEngineAttachment};

/// The closed outcome of one check.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Outcome {
    /// The check held.
    Pass,
    /// The check failed with its bounded reason.
    Fail(&'static str),
    /// The check could not run, with its bounded reason; never a pass.
    Skip(&'static str),
}

impl Outcome {
    /// The exact wire key.
    pub const fn key(&self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail(_) => "fail",
            Self::Skip(_) => "skip",
        }
    }

    /// The bounded reason, when the check did not pass.
    pub const fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Pass => None,
            Self::Fail(reason) | Self::Skip(reason) => Some(reason),
        }
    }
}

/// One battery check with its fixed-order position.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Check {
    pub(crate) id: &'static str,
    pub(crate) outcome: Outcome,
}

impl Check {
    /// The stable check id.
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// The closed outcome.
    pub const fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}

/// The finished battery run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Battery {
    pub(crate) checks: Vec<Check>,
}

impl Battery {
    /// The checks, in the fixed catalog order.
    pub fn checks(&self) -> &[Check] {
        &self.checks
    }

    /// Whether every executed check passed (skips do not fail the
    /// battery, but they are visible).
    pub fn ok(&self) -> bool {
        self.checks
            .iter()
            .all(|check| !matches!(check.outcome, Outcome::Fail(_)))
    }

    /// The canonical battery bytes (compact JSON, byte-sorted keys),
    /// or the typed over-bound refusal.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        use super::canonical::{array, object, optional_string, string};
        let checks: Vec<String> = self
            .checks
            .iter()
            .map(|check| {
                object(vec![
                    ("id", Some(string(check.id))),
                    ("outcome", Some(string(check.outcome.key()))),
                    ("reason", optional_string(check.outcome.reason())),
                ])
            })
            .collect();
        let bytes = super::canonical::object(vec![
            (
                "ok",
                Some(if self.ok() { "true" } else { "false" }.to_owned()),
            ),
            ("checks", Some(array(&checks))),
        ]);
        if bytes.len() > super::version::MAX_CANONICAL_BYTES {
            return Err(diagnostic::export_limit_set(bytes.len()));
        }
        Ok(bytes)
    }
}

/// The closed battery inputs: the profile and its bound projection are
/// required; the evidence, the input document bytes, and the runtime
/// goldens are the optional fixture set a check skips without.
pub struct BatteryInputs<'a> {
    pub profile: &'a StorageEngineAttachment,
    pub projection: &'a StorageProjectionAttachment,
    pub evidence: Option<&'a IntrospectionEvidence>,
    pub drifted: Option<&'a IntrospectionEvidence>,
    pub input: Option<&'a str>,
    pub runtime_goldens: &'a [&'a str],
}

/// Run the closed check catalog in its fixed order. Pure and
/// read-only.
pub fn run(inputs: &BatteryInputs<'_>) -> Battery {
    let mut checks: Vec<Check> = Vec::new();
    let mut check = |id: &'static str, outcome: Outcome| {
        checks.push(Check { id, outcome });
    };
    // 1. profile.wellformed — the profile passed normalization to be
    // here; the typed revalidation is the proof.
    check(
        "profile.wellformed",
        match inputs.profile.validate_attachment() {
            Ok(()) => Outcome::Pass,
            Err(_) => Outcome::Fail("profile-refused"),
        },
    );
    // 2. type-mapping.total — every declared domain field renders or
    // refuses explicitly.
    check(
        "type-mapping.total",
        match type_mapping_total(inputs.profile, inputs.projection) {
            Ok(()) => Outcome::Pass,
            Err(_) => Outcome::Fail("mapping-refused"),
        },
    );
    // 3. ddl.deterministic — two renders are byte-identical.
    check(
        "ddl.deterministic",
        match ddl_deterministic(inputs.profile, inputs.projection) {
            Ok(()) => Outcome::Pass,
            Err(_) => Outcome::Fail("render-divergent"),
        },
    );
    // 4. unique-index-coverage — every one-to-one foreign key carries
    // a unique index in the derived projection.
    check(
        "unique-index-coverage",
        match unique_index_coverage(inputs.projection) {
            Ok(()) => Outcome::Pass,
            Err(_) => Outcome::Fail("unique-missing"),
        },
    );
    // 5. cas-column-declared — a version_column policy names a
    // non-nullable technical column.
    check(
        "cas-column-declared",
        cas_column_declared(inputs.profile, inputs.projection),
    );
    // 6. isolation-honest — the snapshot never upgrades the documented
    // partials.
    check(
        "isolation-honest",
        isolation_honest(inputs.profile, inputs.projection),
    );
    // 7-9. drift machinery over the optional evidence.
    match (inputs.evidence, inputs.drifted) {
        (Some(clean), Some(drifted)) => {
            check(
                "drift.detects-extra|missing|divergent",
                match drift_detects(&compare_result(
                    inputs.profile,
                    inputs.projection,
                    clean,
                    drifted,
                )) {
                    Ok(()) => Outcome::Pass,
                    Err(_) => Outcome::Fail("drift-blind"),
                },
            );
            check(
                "introspection.mode-checked",
                introspection_mode_checked(inputs.profile, clean),
            );
            check(
                "extension.allow-listed",
                extension_allow_listed(inputs.profile, clean),
            );
            check(
                "unsupported.reported",
                unsupported_reported(inputs.profile, inputs.projection, clean, drifted),
            );
        }
        _ => {
            check(
                "drift.detects-extra|missing|divergent",
                Outcome::Skip("evidence-absent"),
            );
            check(
                "introspection.mode-checked",
                Outcome::Skip("evidence-absent"),
            );
            check("extension.allow-listed", Outcome::Skip("evidence-absent"));
            check("unsupported.reported", Outcome::Skip("evidence-absent"));
        }
    }
    // 10. migration.gate-blocks — the gate refuses a wrong planId on a
    // destructive plan derived from a destructive self-mutation.
    check(
        "migration.gate-blocks",
        migration_gate_blocks(inputs.profile, inputs.projection),
    );
    // 11-12. lifecycle rules.
    check(
        "lifecycle.isolation-declared",
        lifecycle_isolation(inputs.profile),
    );
    check(
        "lifecycle.no-production",
        lifecycle_no_production(inputs.profile),
    );
    // 13. input.single-document — every runtime golden is the exact
    // engine input document.
    check(
        "input.single-document",
        input_single_document(
            inputs.profile,
            inputs.projection,
            inputs.input,
            inputs.runtime_goldens,
        ),
    );
    Battery { checks }
}

/// Every declared domain field of the mapped entities renders through
/// the policy table under its actual declared type, or refuses with
/// the registered explicit refusal — a mapping failure of any other
/// rule is hollow and fails the check.
fn type_mapping_total(
    profile: &StorageEngineAttachment,
    projection: &StorageProjectionAttachment,
) -> Result<(), ()> {
    let derived = crate::storage_projection::project(
        projection,
        crate::storage_projection::Namespace::Postgres,
    )
    .map_err(|_| ())?;
    for table in derived.tables() {
        for column in table.columns() {
            if column.origin().key() != "field" {
                continue;
            }
            // The declared type of the field the column derives from;
            // the field and column grammars coincide.
            let field_type = projection
                .entity(table.entity())
                .and_then(|entity| {
                    entity
                        .fields()
                        .iter()
                        .find(|field| field.name().as_str() == column.name().as_str())
                })
                .map(|field| field.field_type())
                .ok_or(())?;
            match super::postgres::types::map_type(field_type, profile.policies()) {
                Ok(_) => {}
                Err(error) => {
                    // The registered render refusal is an explicit,
                    // acceptable answer; anything else is hollow.
                    if error.reason_ids().first().copied()
                        != Some("storage-engine.render-unsupported")
                    {
                        return Err(());
                    }
                }
            }
        }
    }
    Ok(())
}

/// The DDL renderer is deterministic: two renders over value-equal
/// inputs agree byte for byte.
fn ddl_deterministic(
    profile: &StorageEngineAttachment,
    projection: &StorageProjectionAttachment,
) -> Result<(), ()> {
    let first = super::postgres::ddl::render(profile, projection).map_err(|_| ())?;
    let second = super::postgres::ddl::render(profile, projection).map_err(|_| ())?;
    let left = first.canonical_bytes().map_err(|_| ())?;
    let right = second.canonical_bytes().map_err(|_| ())?;
    if left == right {
        Ok(())
    } else {
        Err(())
    }
}

/// Every one-to-one relation's foreign key carries the derived unique
/// index; the database_constraint invariant stays provable.
fn unique_index_coverage(projection: &StorageProjectionAttachment) -> Result<(), ()> {
    use crate::storage_projection::relation::RelationKind;
    let derived = crate::storage_projection::project(
        projection,
        crate::storage_projection::Namespace::Postgres,
    )
    .map_err(|_| ())?;
    for relation in projection.relations() {
        if relation.kind() != RelationKind::OneToOne {
            continue;
        }
        // A one-to-one relation places its key on the target table.
        let Some(column) = relation.foreign_key() else {
            return Err(());
        };
        let Some(table) = derived.table(relation.target()) else {
            return Err(());
        };
        let covered = table.indexes().iter().any(|index| {
            index.unique() && index.columns().len() == 1 && index.columns()[0] == *column
        });
        if !covered {
            return Err(());
        }
    }
    Ok(())
}

/// A version_column policy names at least one non-nullable technical
/// column in the projection.
fn cas_column_declared(
    profile: &StorageEngineAttachment,
    projection: &StorageProjectionAttachment,
) -> Outcome {
    let Some(concurrency) = profile.concurrency() else {
        return Outcome::Skip("policy-absent");
    };
    if !concurrency.version_column() {
        return Outcome::Skip("policy-none");
    }
    let derived = match crate::storage_projection::project(
        projection,
        crate::storage_projection::Namespace::Postgres,
    ) {
        Ok(derived) => derived,
        Err(_) => return Outcome::Fail("projection-refused"),
    };
    let declared = derived.tables().iter().any(|table| {
        table
            .columns()
            .iter()
            .any(|column| column.origin().key() == "technical" && !column.nullable())
    });
    if declared {
        Outcome::Pass
    } else {
        Outcome::Fail("version-column-absent")
    }
}

/// The snapshot isolation and range-lock answers stay honest: never
/// upgraded to full.
fn isolation_honest(
    profile: &StorageEngineAttachment,
    projection: &StorageProjectionAttachment,
) -> Outcome {
    let snapshot = super::postgres::snapshot::build(profile, projection);
    if snapshot.support_of("isolation.snapshot")
        == crate::transaction_concurrency::SnapshotSupport::Full
        || snapshot.support_of("lock.range")
            == crate::transaction_concurrency::SnapshotSupport::Full
    {
        Outcome::Fail("upgraded")
    } else {
        Outcome::Pass
    }
}

/// The drift comparison detects the seeded divergence class.
fn drift_detects(result: &Result<(bool, usize), ()>) -> Result<(), ()> {
    let (clean_ok, drifted_findings) = result.as_ref().map_err(|_| ())?;
    if *clean_ok && *drifted_findings > 0 {
        Ok(())
    } else {
        Err(())
    }
}

/// The committed evidence pair, compared once for the drift checks.
fn compare_result(
    profile: &StorageEngineAttachment,
    projection: &StorageProjectionAttachment,
    clean: &IntrospectionEvidence,
    drifted: &IntrospectionEvidence,
) -> Result<(bool, usize), ()> {
    let clean_report = compare_drift(profile, projection, clean).map_err(|_| ())?;
    let drifted_report = compare_drift(profile, projection, drifted).map_err(|_| ())?;
    Ok((clean_report.ok(), drifted_report.findings().len()))
}

/// Every observed extension is on the profile allow-list (reported
/// findings for the rest are the drift comparison's job; this check
/// pins the allow-list answer itself).
fn extension_allow_listed(
    profile: &StorageEngineAttachment,
    evidence: &IntrospectionEvidence,
) -> Outcome {
    let listed = evidence.extensions().iter().all(|extension| {
        profile
            .extensions()
            .iter()
            .any(|declared| declared.name().as_str() == extension)
    });
    if listed {
        Outcome::Pass
    } else {
        // The allow-list answer is a reported finding elsewhere; the
        // check records the observed state honestly.
        Outcome::Fail("extension-unallowlisted")
    }
}

/// The unsupported records of the evidence surface verbatim as drift
/// findings.
fn unsupported_reported(
    profile: &StorageEngineAttachment,
    projection: &StorageProjectionAttachment,
    clean: &IntrospectionEvidence,
    drifted: &IntrospectionEvidence,
) -> Outcome {
    let drifted_report = match compare_drift(profile, projection, drifted) {
        Ok(report) => report,
        Err(_) => return Outcome::Fail("drift-refused"),
    };
    let _clean_report = match compare_drift(profile, projection, clean) {
        Ok(report) => report,
        Err(_) => return Outcome::Fail("drift-refused"),
    };
    let surfaced = drifted_report
        .findings()
        .iter()
        .filter(|finding| finding.kind().key() == "unsupported")
        .count();
    let clean_unsupported = clean.unsupported().len();
    if surfaced >= drifted.unsupported().len() && clean_unsupported == 0 {
        Outcome::Pass
    } else {
        Outcome::Fail("unsupported-swallowed")
    }
}

/// The checked-mode facts hold: the normalizer const-checks
/// `mode=checked` and read-only at parse, so the check verifies what a
/// checked exchange must additionally pin — the server-reported
/// version equals the profile's pin, the evidence binds the exact
/// projection, and the scope read is declared and non-empty.
fn introspection_mode_checked(
    profile: &StorageEngineAttachment,
    clean: &IntrospectionEvidence,
) -> Outcome {
    if clean.engine_version() == profile.engine_version()
        && clean.projection_ref().as_str() == profile.projection_ref().as_str()
        && !clean.scopes().is_empty()
    {
        Outcome::Pass
    } else {
        Outcome::Fail("evidence-unstable")
    }
}

/// The gate blocks: the battery builds a destructive self-mutation of
/// the bound projection (one declared index removed), plans it, and
/// requires the Blocked status plus the wrong-digest refusal. A
/// projection without a removable index cannot represent the mutation
/// and skips — a skip is never a pass.
fn migration_gate_blocks(
    profile: &StorageEngineAttachment,
    projection: &StorageProjectionAttachment,
) -> Outcome {
    let mut value: serde_json::Value = match projection
        .canonical_bytes()
        .ok()
        .and_then(|bytes| serde_json::from_str(&bytes).ok())
    {
        Some(value) => value,
        None => return Outcome::Fail("gate-open"),
    };
    let removed = value
        .get_mut("projections")
        .and_then(serde_json::Value::as_array_mut)
        .and_then(|projections| {
            projections.iter_mut().find(|projection| {
                projection.get("namespace").and_then(serde_json::Value::as_str)
                    == Some("postgres")
            })
        })
        .and_then(|projection| projection.get_mut("tables"))
        .and_then(serde_json::Value::as_array_mut)
        .and_then(|tables| {
            tables.iter_mut().find_map(|table| {
                let indexes = table.get_mut("indexes")?.as_array_mut()?;
                if indexes.len() >= 2 {
                    indexes.pop();
                    Some(())
                } else {
                    None
                }
            })
        })
        .is_some();
    if !removed {
        return Outcome::Skip("self-mutation-unrepresentable");
    }
    let candidate = match StorageProjectionAttachment::from_value(&value) {
        Ok(candidate) => candidate,
        Err(_) => return Outcome::Fail("gate-open"),
    };
    // The base stays the bound projection, so the profile binding
    // holds; the candidate is the destructive proposal.
    let plan = match super::plan_migration(profile, projection, &candidate, None) {
        Ok(plan) => plan,
        Err(_) => return Outcome::Fail("gate-open"),
    };
    if !plan.gated()
        || plan.status() != super::PlanStatus::Blocked
        || !plan.steps().iter().any(|step| step.risk().key() == "destructive")
    {
        return Outcome::Fail("gate-open");
    }
    // The wrong digest refuses with the registered gate rule.
    let wrong = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    match super::plan_migration(profile, projection, &candidate, Some(wrong)) {
        Err(error) => {
            if error.reason_ids().first().copied() == Some("storage-engine.migration-gated") {
                Outcome::Pass
            } else {
                Outcome::Fail("gate-open")
            }
        }
        Ok(_) => Outcome::Fail("gate-open"),
    }
}

/// A declared lifecycle names its isolation boundary.
fn lifecycle_isolation(profile: &StorageEngineAttachment) -> Outcome {
    match profile.test_lifecycle() {
        None => Outcome::Skip("lifecycle-absent"),
        Some(lifecycle) => {
            let _ = lifecycle.isolation();
            Outcome::Pass
        }
    }
}

/// Production access through the test lifecycle stays forbidden: the
/// wire const admits only `forbidden`, so a parsed profile passes by
/// construction and the check records it.
fn lifecycle_no_production(profile: &StorageEngineAttachment) -> Outcome {
    match profile.test_lifecycle() {
        None => Outcome::Skip("lifecycle-absent"),
        Some(_) => Outcome::Pass,
    }
}

/// The engine input document equals every committed runtime golden.
fn input_single_document(
    profile: &StorageEngineAttachment,
    projection: &StorageProjectionAttachment,
    input: Option<&str>,
    runtime_goldens: &[&str],
) -> Outcome {
    let Some(input) = input else {
        return Outcome::Skip("input-absent");
    };
    let document = match input_document(profile, projection) {
        Ok(document) => document,
        Err(_) => return Outcome::Fail("input-refused"),
    };
    if document != input {
        return Outcome::Fail("input-divergent");
    }
    if runtime_goldens.iter().any(|golden| *golden != input) {
        return Outcome::Fail("runtime-divergent");
    }
    Outcome::Pass
}
