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
use crate::storage_projection::StorageProjectionAttachment;

use super::diagnostic::{self, DRIFT_INVALID};
use super::introspection::IntrospectionEvidence;
use super::StorageEngineAttachment;

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
    // undeclared), then per-table columns, foreign constraints, and
    // indexes.
    for table in projection.tables() {
        let Some(observed) = evidence.table(table.table().as_str()) else {
            findings.push(Finding {
                kind: FindingKind::Missing,
                path: format!("tables/{}", table.table()),
                detail: "table-missing".to_owned(),
            });
            continue;
        };
        compare_columns(table, observed, &mut findings);
        compare_foreign_keys(table, observed, &mut findings);
        compare_indexes(table, observed, &mut findings);
    }
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

/// Column-level drift: missing, extra, and divergent (type or
/// nullability).
fn compare_columns(
    declared: &crate::storage_projection::derivation::DerivedTable,
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
        if observed_column.storage_type() != column.storage_type() {
            findings.push(Finding {
                kind: FindingKind::Divergent,
                path: format!("tables/{}/columns/{}", declared.table(), column.name()),
                detail: "column-type".to_owned(),
            });
        } else if observed_column.nullable() != column.nullable() {
            findings.push(Finding {
                kind: FindingKind::Divergent,
                path: format!("tables/{}/columns/{}", declared.table(), column.name()),
                detail: "column-nullability".to_owned(),
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
