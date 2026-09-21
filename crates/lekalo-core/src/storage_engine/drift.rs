//! The declared-versus-observed drift comparison (issue #69).
//!
//! [`compare`] emits the deterministic drift report of one derived
//! storage projection against one adapter-produced checked-mode
//! introspection evidence document: `missing`/`extra`/`divergent`
//! findings per table, column, foreign constraint, and index, plus
//! the explicit `unsupported` findings for every observed type or
//! extension outside the closed vocabulary and profile allow-list.
//! Findings are data — drift never invents a remediation, and the
//! verdict stays `ok` or `drifted` regardless of severity.

use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::projection::GeneratedKind;
use crate::storage_projection::StorageProjectionAttachment;

use super::diagnostic::{self, DRIFT_INVALID};
use super::introspection::IntrospectionEvidence;
use super::{EnumPolicy, StorageEngineAttachment};

/// One typed drift finding.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Finding {
    pub(crate) kind: FindingKind,
    pub(crate) path: String,
    pub(crate) detail: String,
}

impl Finding {
    /// The closed finding kind.
    pub const fn kind(&self) -> FindingKind {
        self.kind
    }

    /// The canonical path of the finding.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The bounded fixed detail token.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// The closed drift finding kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum FindingKind {
    /// The declaration has no observed counterpart.
    Missing,
    /// The observation has no declared counterpart.
    Extra,
    /// Both exist but disagree.
    Divergent,
    /// The observation is outside the closed vocabulary or the
    /// profile allow-list.
    Unsupported,
}

impl FindingKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Extra => "extra",
            Self::Divergent => "divergent",
            Self::Unsupported => "unsupported",
        }
    }
}

/// The finished drift report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriftReport {
    pub(crate) ok: bool,
    pub(crate) findings: Vec<Finding>,
}

impl DriftReport {
    /// Whether the observed schema matches the declaration exactly.
    pub const fn ok(&self) -> bool {
        self.ok
    }

    /// The findings, canonical (byte-sorted) order.
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// The canonical report bytes (compact JSON, byte-sorted keys),
    /// or the typed over-bound refusal.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        use super::canonical::{array, flag, object, string};
        let findings: Vec<String> = self
            .findings
            .iter()
            .map(|finding| {
                object(vec![
                    ("kind", Some(string(finding.kind.key()))),
                    ("path", Some(string(&finding.path))),
                    ("detail", Some(string(&finding.detail))),
                ])
            })
            .collect();
        let bytes = object(vec![
            ("ok", Some(flag(self.ok))),
            ("findings", Some(array(&findings))),
        ]);
        if bytes.len() > super::version::MAX_CANONICAL_BYTES {
            return Err(diagnostic::export_limit_set(bytes.len()));
        }
        Ok(bytes)
    }
}

/// Compare one derived storage projection against one checked-mode
/// evidence document under one engine profile. The evidence binding
/// must name the attachment's canonical digest, the engine versions
/// must agree, and the projection must declare the postgres
/// namespace. Pure and read-only.
pub fn compare(
    profile: &StorageEngineAttachment,
    attachment: &StorageProjectionAttachment,
    evidence: &IntrospectionEvidence,
) -> Result<DriftReport, DiagnosticSet> {
    if evidence.projection_ref().as_str() != profile.projection_ref().as_str() {
        return Err(diagnostic::rule_invalid(
            DRIFT_INVALID,
            "projection-binding-mismatch",
            None,
        ));
    }
    let binding = attachment.canonical_bytes()?;
    let digest = crate::digest::sha256_hex(binding.as_bytes());
    if format!("sha256:{digest}") != profile.projection_ref().as_str() {
        return Err(diagnostic::rule_invalid(
            DRIFT_INVALID,
            "profile-binding-mismatch",
            None,
        ));
    }
    if evidence.engine_version() != profile.engine_version() {
        return Err(diagnostic::rule_invalid(
            DRIFT_INVALID,
            "engine-version-mismatch",
            None,
        ));
    }
    if evidence.project_id().as_str() != attachment.project_id().as_str() {
        return Err(diagnostic::rule_invalid(
            DRIFT_INVALID,
            "project-mismatch",
            None,
        ));
    }
    let projection = crate::storage_projection::project(
        attachment,
        crate::storage_projection::Namespace::Postgres,
    )?;
    let mut findings: Vec<Finding> = Vec::new();
    // Extension allow-list first: an observed extension outside the
    // profile list is a reported finding, never a silent coercion.
    for extension in evidence.extensions() {
        if !profile
            .extensions()
            .iter()
            .any(|declared| declared.name().as_str() == extension)
        {
            findings.push(Finding {
                kind: FindingKind::Unsupported,
                path: format!("extensions/{extension}"),
                detail: "extension-unallowlisted".to_owned(),
            });
        }
    }
    // Tables: missing (declared, unobserved) and extra (observed,
    // undeclared), then per-table columns, primary keys, foreign
    // constraints, and indexes.
    for table in projection.tables() {
        let Some(observed) = evidence.table(table.table().as_str()) else {
            findings.push(Finding {
                kind: FindingKind::Missing,
                path: format!("tables/{}", table.table()),
                detail: "table-missing".to_owned(),
            });
            continue;
        };
        compare_columns(profile, attachment, table, observed, &mut findings)?;
        compare_primary_key(table, observed, &mut findings);
        compare_foreign_keys(table, observed, &mut findings);
        compare_indexes(table, observed, &mut findings);
    }
    // Join tables compare like tables: columns, the unique-pair
    // primary key, and absence. A join the server lost is drift.
    for join in projection.joins() {
        let Some(observed) = evidence.table(join.table().as_str()) else {
            findings.push(Finding {
                kind: FindingKind::Missing,
                path: format!("tables/{}", join.table()),
                detail: "table-missing".to_owned(),
            });
            continue;
        };
        compare_join_columns(join, observed, &mut findings);
        compare_join_primary_key(join, observed, &mut findings);
    }
    // Declared and derived CHECK constraints: every expected check
    // (the declared tables' checks plus the enum member CHECKs under
    // the check policy) must be observed under its deterministic
    // name; observed checks outside the expectation are extra.
    compare_checks(profile, attachment, &projection, evidence, &mut findings)?;
    // Observed unique constraints must map to a declared unique index
    // (or the primary key); an undeclared unique fact is extra.
    compare_unique_constraints(&projection, evidence, &mut findings);
    for observed in evidence.tables() {
        let declared = projection
            .tables()
            .iter()
            .any(|table| table.table().as_str() == observed.name())
            || projection
                .joins()
                .iter()
                .any(|join| join.table().as_str() == observed.name());
        if !declared {
            findings.push(Finding {
                kind: FindingKind::Extra,
                path: format!("tables/{}", observed.name()),
                detail: "table-extra".to_owned(),
            });
        }
    }
    // The explicit unsupported records surface verbatim.
    for record in evidence.unsupported() {
        let path = match (record.table(), record.column()) {
            (Some(table), Some(column)) => {
                format!("unsupported/{}/{}/{column}", record.kind.key(), table)
            }
            (Some(table), None) => format!("unsupported/{}/{}", record.kind.key(), table),
            _ => format!("unsupported/{}/{}", record.kind.key(), record.name()),
        };
        findings.push(Finding {
            kind: FindingKind::Unsupported,
            path,
            detail: format!("unsupported-{}", record.kind.key()),
        });
    }
    findings.sort();
    Ok(DriftReport {
        ok: findings.is_empty(),
        findings,
    })
}

/// Column-level drift: missing, extra, and divergent (type,
/// nullability, default spelling, or identity). The expected type is
/// the policy table's answer for field-origin columns — the profile's
/// `json`/`array` policies are observable here, exactly as the DDL
/// renderer and the migration planner render them.
fn compare_columns(
    profile: &StorageEngineAttachment,
    attachment: &StorageProjectionAttachment,
    declared: &crate::storage_projection::derivation::DerivedTable,
    observed: &super::introspection::ObservedTable,
    findings: &mut Vec<Finding>,
) -> Result<(), DiagnosticSet> {
    for column in declared.columns() {
        let expected_type =
            super::postgres::ddl::column_storage_type(profile, attachment, declared, column)?;
        let Some(observed_column) = observed
            .columns()
            .iter()
            .find(|observed| observed.name() == column.name().as_str())
        else {
            findings.push(Finding {
                kind: FindingKind::Missing,
                path: format!("tables/{}/columns/{}", declared.table(), column.name()),
                detail: "column-missing".to_owned(),
            });
            continue;
        };
        if observed_column.storage_type() != expected_type {
            findings.push(Finding {
                kind: FindingKind::Divergent,
                path: format!("tables/{}/columns/{}", declared.table(), column.name()),
                detail: "column-type".to_owned(),
            });
            continue;
        }
        if observed_column.nullable() != column.nullable() {
            findings.push(Finding {
                kind: FindingKind::Divergent,
                path: format!("tables/{}/columns/{}", declared.table(), column.name()),
                detail: "column-nullability".to_owned(),
            });
        }
        // Identity: a declared identity column must be observed as one.
        let declared_identity = column.generated_kind() == Some(GeneratedKind::Identity);
        if observed_column.identity() != declared_identity {
            findings.push(Finding {
                kind: FindingKind::Divergent,
                path: format!("tables/{}/columns/{}", declared.table(), column.name()),
                detail: "column-identity".to_owned(),
            });
        }
        // Default: the declared default's exact rendering (or the
        // generated sequence's nextval) must be the observed spelling;
        // the adapter-facing comparison strips the regclass cast the
        // server appends to sequence references.
        let expected = if column.generated_kind() == Some(GeneratedKind::Sequence) {
            Some(super::postgres::ddl::sequence_default(
                declared.table(),
                column.name(),
            )?)
        } else {
            column
                .default()
                .map(|default| super::postgres::ddl::render_default(default, declared.table()))
                .transpose()?
        };
        let observed_default = observed_column.default().map(normalized_default);
        if observed_default != expected.as_deref().map(normalized_default) {
            findings.push(Finding {
                kind: FindingKind::Divergent,
                path: format!("tables/{}/columns/{}", declared.table(), column.name()),
                detail: "column-default".to_owned(),
            });
        }
    }
    for observed_column in observed.columns() {
        if !declared
            .columns()
            .iter()
            .any(|column| column.name().as_str() == observed_column.name())
        {
            findings.push(Finding {
                kind: FindingKind::Extra,
                path: format!(
                    "tables/{}/columns/{}",
                    declared.table(),
                    observed_column.name()
                ),
                detail: "column-extra".to_owned(),
            });
        }
    }
    Ok(())
}

/// One default spelling normalized for comparison: trimmed, with the
/// server-side regclass cast stripped from sequence references.
fn normalized_default(text: &str) -> String {
    text.trim().replace("::regclass", "")
}

/// Primary-key drift: the declared key must be observed as the one
/// primary constraint with the same column set.
fn compare_primary_key(
    declared: &crate::storage_projection::derivation::DerivedTable,
    observed: &super::introspection::ObservedTable,
    findings: &mut Vec<Finding>,
) {
    let path = format!("tables/{}/primary", declared.table());
    let observed_primary: Vec<&super::introspection::ObservedConstraint> = observed
        .constraints()
        .iter()
        .filter(|constraint| constraint.kind().key() == "primary")
        .collect();
    let Some(first) = observed_primary.first() else {
        findings.push(Finding {
            kind: FindingKind::Missing,
            path,
            detail: "primary-missing".to_owned(),
        });
        return;
    };
    let declared_columns = sorted_names(declared.primary_key().iter().map(|name| name.as_str()));
    if sorted_names(first.columns().iter().map(String::as_str)) != declared_columns {
        findings.push(Finding {
            kind: FindingKind::Divergent,
            path: path.clone(),
            detail: "primary-columns".to_owned(),
        });
    }
    for extra in observed_primary.iter().skip(1) {
        findings.push(Finding {
            kind: FindingKind::Extra,
            path: format!("{path}/{}", extra.columns().join("_")),
            detail: "primary-extra".to_owned(),
        });
    }
}

/// Join-table column drift: the declared join columns must be observed
/// with the same type and nullability, and nothing extra.
fn compare_join_columns(
    declared: &crate::storage_projection::derivation::DerivedJoin,
    observed: &super::introspection::ObservedTable,
    findings: &mut Vec<Finding>,
) {
    for column in declared.columns() {
        let Some(observed_column) = observed
            .columns()
            .iter()
            .find(|observed| observed.name() == column.name().as_str())
        else {
            findings.push(Finding {
                kind: FindingKind::Missing,
                path: format!("tables/{}/columns/{}", declared.table(), column.name()),
                detail: "column-missing".to_owned(),
            });
            continue;
        };
        if observed_column.storage_type() != column.storage_type()
            || observed_column.nullable() != column.nullable()
        {
            findings.push(Finding {
                kind: FindingKind::Divergent,
                path: format!("tables/{}/columns/{}", declared.table(), column.name()),
                detail: if observed_column.storage_type() != column.storage_type() {
                    "column-type"
                } else {
                    "column-nullability"
                }
                .to_owned(),
            });
        }
    }
    for observed_column in observed.columns() {
        if !declared
            .columns()
            .iter()
            .any(|column| column.name().as_str() == observed_column.name())
        {
            findings.push(Finding {
                kind: FindingKind::Extra,
                path: format!(
                    "tables/{}/columns/{}",
                    declared.table(),
                    observed_column.name()
                ),
                detail: "column-extra".to_owned(),
            });
        }
    }
}

/// Join-table primary-key drift: a unique-pair join declares the pair
/// as its primary key; any other observed primary fact is undeclared.
fn compare_join_primary_key(
    declared: &crate::storage_projection::derivation::DerivedJoin,
    observed: &super::introspection::ObservedTable,
    findings: &mut Vec<Finding>,
) {
    let path = format!("tables/{}/primary", declared.table());
    let observed_primary: Vec<&super::introspection::ObservedConstraint> = observed
        .constraints()
        .iter()
        .filter(|constraint| constraint.kind().key() == "primary")
        .collect();
    if declared.unique_pair() {
        let Some(first) = observed_primary.first() else {
            findings.push(Finding {
                kind: FindingKind::Missing,
                path,
                detail: "primary-missing".to_owned(),
            });
            return;
        };
        let expected = sorted_names(
            declared
                .columns()
                .iter()
                .map(|column| column.name().as_str()),
        );
        if sorted_names(first.columns().iter().map(String::as_str)) != expected {
            findings.push(Finding {
                kind: FindingKind::Divergent,
                path,
                detail: "primary-columns".to_owned(),
            });
        }
    } else if !observed_primary.is_empty() {
        findings.push(Finding {
            kind: FindingKind::Extra,
            path,
            detail: "primary-extra".to_owned(),
        });
    }
}

/// CHECK-constraint drift: the expected check set (declared table
/// checks plus the derived enum member CHECKs) must be observed under
/// its deterministic name with the same constrained columns; observed
/// checks outside the expectation are extra.
fn compare_checks(
    profile: &StorageEngineAttachment,
    attachment: &StorageProjectionAttachment,
    projection: &crate::storage_projection::derivation::DerivedProjection,
    evidence: &IntrospectionEvidence,
    findings: &mut Vec<Finding>,
) -> Result<(), DiagnosticSet> {
    let mut expected: Vec<(String, String, Vec<String>)> = Vec::new();
    if let Some(declared) = attachment
        .projections()
        .iter()
        .find(|declared| declared.namespace().key() == "postgres")
    {
        for table in declared.tables() {
            for check in table.checks() {
                let columns = sorted_names(
                    check
                        .predicates()
                        .iter()
                        .map(|predicate| predicate.column().as_str()),
                );
                expected.push((
                    table.table().as_str().to_owned(),
                    check.name().as_str().to_owned(),
                    columns,
                ));
            }
        }
    }
    if profile.policies().enum_policy() == EnumPolicy::Check {
        for table in projection.tables() {
            for column in super::postgres::ddl::declared_enum_members(attachment, table).keys() {
                expected.push((
                    table.table().as_str().to_owned(),
                    format!("chk_{}_{}", table.table(), column),
                    vec![column.clone()],
                ));
            }
        }
    }
    for (table_name, name, columns) in &expected {
        let Some(observed_table) = evidence.table(table_name) else {
            // The missing table is reported once as table-missing.
            continue;
        };
        let observed_check = observed_table.constraints().iter().find(|constraint| {
            constraint.kind().key() == "check" && constraint.name() == Some(name.as_str())
        });
        let path = format!("tables/{table_name}/checks/{name}");
        match observed_check {
            None => findings.push(Finding {
                kind: FindingKind::Missing,
                path,
                detail: "check-missing".to_owned(),
            }),
            Some(constraint) => {
                if sorted_names(constraint.columns().iter().map(String::as_str)) != *columns {
                    findings.push(Finding {
                        kind: FindingKind::Divergent,
                        path,
                        detail: "check-columns".to_owned(),
                    });
                }
            }
        }
    }
    for table in projection.tables() {
        let Some(observed_table) = evidence.table(table.table().as_str()) else {
            continue;
        };
        let expected_names: std::collections::BTreeSet<String> = expected
            .iter()
            .filter(|(table_name, _, _)| table_name == table.table().as_str())
            .map(|(_, name, _)| name.clone())
            .collect();
        for constraint in observed_table
            .constraints()
            .iter()
            .filter(|constraint| constraint.kind().key() == "check")
        {
            let Some(name) = constraint.name() else {
                continue;
            };
            if !expected_names.contains(name) {
                findings.push(Finding {
                    kind: FindingKind::Extra,
                    path: format!("tables/{}/checks/{name}", table.table()),
                    detail: "check-extra".to_owned(),
                });
            }
        }
    }
    Ok(())
}

/// Observed unique constraints must map to a declared unique index or
/// the declared primary key; an undeclared unique fact is extra.
fn compare_unique_constraints(
    projection: &crate::storage_projection::derivation::DerivedProjection,
    evidence: &IntrospectionEvidence,
    findings: &mut Vec<Finding>,
) {
    for table in projection.tables() {
        let Some(observed_table) = evidence.table(table.table().as_str()) else {
            continue;
        };
        let primary = sorted_names(table.primary_key().iter().map(|name| name.as_str()));
        let declared_unique: Vec<Vec<String>> = table
            .indexes()
            .iter()
            .filter(|index| index.unique())
            .map(|index| sorted_names(index.columns().iter().map(|column| column.as_str())))
            .collect();
        for constraint in observed_table
            .constraints()
            .iter()
            .filter(|constraint| constraint.kind().key() == "unique")
        {
            let columns = sorted_names(constraint.columns().iter().map(String::as_str));
            if columns != primary && !declared_unique.contains(&columns) {
                findings.push(Finding {
                    kind: FindingKind::Extra,
                    path: format!("tables/{}/unique/{}", table.table(), columns.join("_")),
                    detail: "unique-extra".to_owned(),
                });
            }
        }
    }
}

/// One canonical, byte-sorted name set.
fn sorted_names<'a, I: Iterator<Item = &'a str>>(names: I) -> Vec<String> {
    let mut names: Vec<String> = names.map(|name| name.to_owned()).collect();
    names.sort();
    names
}

/// Foreign-constraint drift: every derived foreign key must have an
/// observed foreign constraint on its column.
fn compare_foreign_keys(
    declared: &crate::storage_projection::derivation::DerivedTable,
    observed: &super::introspection::ObservedTable,
    findings: &mut Vec<Finding>,
) {
    for foreign_key in declared.foreign_keys() {
        let observed_constraint = observed.constraints().iter().find(|constraint| {
            constraint.kind().key() == "foreign"
                && constraint.columns().len() == 1
                && constraint.columns()[0] == foreign_key.column().as_str()
        });
        match observed_constraint {
            None => {
                findings.push(Finding {
                    kind: FindingKind::Missing,
                    path: format!(
                        "tables/{}/foreign/{}",
                        declared.table(),
                        foreign_key.column()
                    ),
                    detail: "foreign-missing".to_owned(),
                });
            }
            Some(constraint) => {
                if constraint.target() != Some(foreign_key.references_table().as_str()) {
                    findings.push(Finding {
                        kind: FindingKind::Divergent,
                        path: format!(
                            "tables/{}/foreign/{}",
                            declared.table(),
                            foreign_key.column()
                        ),
                        detail: "foreign-target".to_owned(),
                    });
                }
            }
        }
    }
}

/// Index drift: declared and derived indexes must be observed with the
/// same uniqueness and partiality.
fn compare_indexes(
    declared: &crate::storage_projection::derivation::DerivedTable,
    observed: &super::introspection::ObservedTable,
    findings: &mut Vec<Finding>,
) {
    for index in declared.indexes() {
        let declared_columns: Vec<String> = index
            .columns()
            .iter()
            .map(|column| column.as_str().to_owned())
            .collect();
        let observed_index = observed.indexes().iter().find(|observed| {
            observed.columns() == declared_columns
                && index
                    .name()
                    .map_or(true, |name| observed.name() == Some(name.as_str()))
        });
        match observed_index {
            None => {
                findings.push(Finding {
                    kind: FindingKind::Missing,
                    path: format!(
                        "tables/{}/indexes/{}",
                        declared.table(),
                        index
                            .name()
                            .map(|name| name.as_str().to_owned())
                            .unwrap_or_else(|| index
                                .columns()
                                .iter()
                                .map(|column| column.as_str())
                                .collect::<Vec<&str>>()
                                .join("_"))
                    ),
                    detail: "index-missing".to_owned(),
                });
            }
            Some(observed) => {
                if observed.unique() != index.unique()
                    || observed.partial() != index.where_().is_some()
                {
                    findings.push(Finding {
                        kind: FindingKind::Divergent,
                        path: format!(
                            "tables/{}/indexes/{}",
                            declared.table(),
                            index
                                .name()
                                .map(|name| name.as_str().to_owned())
                                .unwrap_or_else(|| index
                                    .columns()
                                    .iter()
                                    .map(|column| column.as_str())
                                    .collect::<Vec<&str>>()
                                    .join("_"))
                        ),
                        detail: "index-shape".to_owned(),
                    });
                }
            }
        }
    }
}
