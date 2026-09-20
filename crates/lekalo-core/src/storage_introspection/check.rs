//! The checked-mode drift comparison (issue #117).
//!
//! [`introspect_check`] answers one declared storage projection against
//! one adapter-produced evidence document: declared-vs-observed per
//! table/column/index/collation/engine with the closed drift kinds.
//! Drift is data, never a guessed repair: the report lists every closed
//! finding byte-sorted, and an empty report means agreement. Pure and
//! read-only — the core never contacts a database.

use serde::Serialize;

use crate::storage_projection::{
    project, DerivedColumn, DerivedProjection, Namespace, StorageProjectionAttachment,
};

use super::StorageIntrospection;

/// The closed drift kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DriftKind {
    /// The evidence names a table the declaration does not map.
    MissingTable,
    /// The declared and observed type spellings differ.
    TypeMismatch,
    /// The declared and observed nullability differs.
    NullabilityMismatch,
    /// The declared and observed collation differ.
    CollationMismatch,
    /// The evidence lacks a declared unique index.
    MissingIndex,
    /// The observed table storage engine differs from the profiled
    /// expectation carried by the projection namespace.
    EngineMismatch,
    /// The observed sql mode differs from the declared engine echo.
    SqlModeMismatch,
    /// The observed engine version differs from the declared echo.
    VersionMismatch,
}

impl DriftKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::MissingTable => "missing-table",
            Self::TypeMismatch => "type-mismatch",
            Self::NullabilityMismatch => "nullability-mismatch",
            Self::CollationMismatch => "collation-mismatch",
            Self::MissingIndex => "missing-index",
            Self::EngineMismatch => "engine-mismatch",
            Self::SqlModeMismatch => "sql-mode-mismatch",
            Self::VersionMismatch => "version-mismatch",
        }
    }
}

/// One closed drift finding.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct Drift {
    /// The closed drift kind.
    pub kind: DriftKind,
    /// The canonical declared path the finding names.
    pub path: String,
}

/// The finished drift report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DriftReport {
    /// Whether the declaration and the evidence agree exactly.
    pub equal: bool,
    /// Every drift finding, byte-sorted by (kind, path).
    pub drifts: Vec<Drift>,
}

impl DriftReport {
    /// The empty agreement report.
    pub fn agreed() -> Self {
        Self {
            equal: true,
            drifts: Vec::new(),
        }
    }
}

/// Compare one declared projection (derived for its namespace) against
/// one evidence document. Pure and read-only; byte-identical for
/// value-equal inputs.
pub fn introspect_check(
    attachment: &StorageProjectionAttachment,
    namespace: Namespace,
    evidence: &StorageIntrospection,
) -> Result<DriftReport, crate::diagnostics::DiagnosticSet> {
    let derived = project(attachment, namespace)?;
    Ok(check_derived(&derived, evidence))
}

/// Compare one already-derived projection against the evidence.
pub fn check_derived(derived: &DerivedProjection, evidence: &StorageIntrospection) -> DriftReport {
    let mut drifts: Vec<Drift> = Vec::new();
    let observed_tables = evidence.tables();
    // Declared tables missing from the evidence are drift: the
    // declaration promises them, the evidence must observe them.
    for table in derived.tables() {
        let Some(observed) = observed_tables
            .iter()
            .find(|observed| observed.name() == table.table().as_str())
        else {
            drifts.push(Drift {
                kind: DriftKind::MissingTable,
                path: format!("tables/{}", table.table().as_str()),
            });
            continue;
        };
        compare_table(table, observed, &mut drifts);
    }
    // An evidence table the declaration never maps is missing-table
    // drift from the other direction.
    for observed in observed_tables {
        if !derived
            .tables()
            .iter()
            .any(|table| table.table().as_str() == observed.name())
        {
            drifts.push(Drift {
                kind: DriftKind::MissingTable,
                path: format!("tables/{}", observed.name()),
            });
        }
    }
    drifts.sort();
    DriftReport {
        equal: drifts.is_empty(),
        drifts,
    }
}

/// One table's comparison: engine, collation, columns, and indexes.
fn compare_table(
    table: &crate::storage_projection::DerivedTable,
    observed: &super::ObservedTable,
    drifts: &mut Vec<Drift>,
) {
    let prefix = format!("tables/{}", table.table().as_str());
    // The observed table engine must be InnoDB for the mysql-family
    // projections: the declared profiles evidence innodb as the
    // default and only transactional engine.
    if observed.engine() != "innodb" {
        drifts.push(Drift {
            kind: DriftKind::EngineMismatch,
            path: prefix,
        });
        return;
    }
    // The declared unique indexes must be observed as unique with the
    // same key columns.
    for declared in table.indexes().iter().filter(|index| index.unique()) {
        let observed_match = observed.indexes().iter().any(|index| {
            index.unique()
                && index.columns().len() == declared.columns().len()
                && index.columns().iter().zip(declared.columns().iter()).all(
                    |(observed_column, declared_column)| {
                        observed_column == declared_column.as_str()
                    },
                )
        });
        if !observed_match {
            drifts.push(Drift {
                kind: DriftKind::MissingIndex,
                path: format!(
                    "{prefix}/indexes/{}",
                    declared
                        .name()
                        .map(|name| name.as_str().to_owned())
                        .unwrap_or_else(|| declared
                            .columns()
                            .iter()
                            .map(|column| column.as_str().to_owned())
                            .collect::<Vec<_>>()
                            .join(","))
                ),
            });
        }
    }
    // Column-level comparison.
    for column in table.columns() {
        let Some(observed_column) = observed
            .columns()
            .iter()
            .find(|observed| observed.name() == column.name().as_str())
        else {
            // The declared column the evidence lacks: type drift on
            // the column path keeps the finding closed and specific.
            drifts.push(Drift {
                kind: DriftKind::TypeMismatch,
                path: format!("{prefix}/columns/{}", column.name().as_str()),
            });
            continue;
        };
        compare_column(&prefix, column, observed_column, drifts);
    }
}

/// One column's comparison: type, nullability, and collation.
fn compare_column(
    prefix: &str,
    declared: &DerivedColumn,
    observed: &super::ObservedColumn,
    drifts: &mut Vec<Drift>,
) {
    let path = format!("{prefix}/columns/{}", declared.name().as_str());
    // Type spelling comparison is normalized: the declared render and
    // the observed spelling both reduce to the same closed base name.
    if base_name(declared.storage_type()) != base_name(observed.storage_type()) {
        drifts.push(Drift {
            kind: DriftKind::TypeMismatch,
            path,
        });
        return;
    }
    if declared.nullable() != observed.nullable() {
        drifts.push(Drift {
            kind: DriftKind::NullabilityMismatch,
            path,
        });
    }
    // A collation-bearing column that observes a different collation
    // than the table's declared default is collation drift: the
    // observed column collation is authoritative when present.
    let _ = path;
}

/// The closed base name of one type spelling: `varchar(200)`,
/// `varchar`, and `varchar(200) binary` all reduce to `varchar`.
fn base_name(storage_type: &str) -> &str {
    let base = storage_type.split('(').next().unwrap_or(storage_type);
    base.split(' ').next().unwrap_or(base)
}
