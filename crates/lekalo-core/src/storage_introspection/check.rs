//! The checked-mode drift comparison (issue #117).
//!
//! [`introspect_check`] answers one declared storage projection against
//! one adapter-produced evidence document: declared-vs-observed per
//! table/column/index/collation/engine with the closed drift kinds.
//! Drift is data, never a guessed repair: the report lists every closed
//! finding byte-sorted, and an empty report means agreement. Pure and
//! read-only — the core never contacts a database.
//!
//! The engine echo is compared too: the observed engine token must
//! match the projected namespace, the observed `sqlMode` must carry
//! every token the declared engine profile of this contract generation
//! requires (the STRICT + zero-date + engine-substitution baseline the
//! mysql and mariadb goldens pin), and the observed engine version must fall inside
//! the declared major line of the namespace (`8.x` for mysql, `10.x`
//! or `11.x` for mariadb). Anything outside the closed knowledge is
//! reported as the corresponding drift kind, never silently equal.

use serde::Serialize;

use crate::storage_projection::{
    project, DerivedColumn, DerivedProjection, DerivedTable, Namespace, StorageProjectionAttachment,
};

use super::{SqlModeEcho, StorageIntrospection};

/// The closed sql-mode tokens every contracted engine profile of this
/// generation declares (the `STRICT` + zero-date + engine-substitution
/// baseline shared by the mysql and mariadb goldens). An observed mode
/// missing any token is sql-mode drift: silently relaxed server mode is
/// exactly the class of divergence this comparison exists to surface.
const REQUIRED_SQL_MODE: [SqlModeEcho; 5] = [
    SqlModeEcho::ErrorForDivisionByZero,
    SqlModeEcho::NoEngineSubstitution,
    SqlModeEcho::NoZeroDate,
    SqlModeEcho::NoZeroInDate,
    SqlModeEcho::StrictTransTables,
];

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
    // The declared default collation of every textual column: the
    // table's declared collation when declared, else the projection's
    // declared text default. The evidence names the authoritative
    // column collation, so this declared inheritance chain decides
    // what counts as agreement.
    let declared_projection = attachment.projection(namespace);
    let table_collation = move |table: &DerivedTable| -> Option<&str> {
        let declared_table = declared_projection
            .and_then(|projection| {
                projection
                    .tables()
                    .iter()
                    .find(|declared| declared.entity() == &table.entity)
            })
            .and_then(|declared| declared.collation());
        let default = declared_projection
            .and_then(|projection| projection.text_defaults())
            .map(|(_, collation)| collation);
        declared_table.or(default)
    };
    Ok(check_derived_with(&derived, evidence, &table_collation))
}

/// Compare one already-derived projection against the evidence.
pub fn check_derived(derived: &DerivedProjection, evidence: &StorageIntrospection) -> DriftReport {
    check_derived_with(derived, evidence, &|_| None)
}

/// The full comparison with the declared table-collation resolver.
fn check_derived_with<'a>(
    derived: &DerivedProjection,
    evidence: &StorageIntrospection,
    table_collation: &dyn Fn(&DerivedTable) -> Option<&'a str>,
) -> DriftReport {
    let mut drifts: Vec<Drift> = Vec::new();
    let observed_tables = evidence.tables();
    // The engine echo is checked once per comparison: the observed
    // engine token, release, and sql mode against the projected
    // namespace and its closed expectations. Missing tables are
    // collected below; a foreign engine/version/mode is drift on the
    // evidence itself.
    compare_engine(derived.namespace(), evidence.engine(), &mut drifts);
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
        compare_table(table, observed, &table_collation, &mut drifts);
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
fn compare_table<'a>(
    table: &crate::storage_projection::DerivedTable,
    observed: &super::ObservedTable,
    table_collation: &dyn Fn(&DerivedTable) -> Option<&'a str>,
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
        compare_column(
            &prefix,
            column,
            observed_column,
            table,
            table_collation,
            drifts,
        );
    }
}

/// One column's comparison: type, nullability, and collation.
fn compare_column<'a>(
    prefix: &str,
    declared: &DerivedColumn,
    observed: &super::ObservedColumn,
    table: &crate::storage_projection::DerivedTable,
    table_collation: &dyn Fn(&DerivedTable) -> Option<&'a str>,
    drifts: &mut Vec<Drift>,
) {
    let path = format!("{prefix}/columns/{}", declared.name().as_str());
    // Type comparison: the base family must agree, and numeric
    // parameters must agree whenever both spellings carry them —
    // `varchar(200)` vs `varchar(64)` is width drift, `datetime(6)` vs
    // `datetime` is precision drift. A parameter present on one side
    // only (e.g. `bigint unsigned`) is a presentation difference, not
    // a type change, and compares equal.
    if base_name(declared.storage_type()) != base_name(observed.storage_type())
        || parameters(declared.storage_type()) != parameters(observed.storage_type())
    {
        drifts.push(Drift {
            kind: DriftKind::TypeMismatch,
            path,
        });
        return;
    }
    if declared.nullable() != observed.nullable() {
        drifts.push(Drift {
            kind: DriftKind::NullabilityMismatch,
            path: path.clone(),
        });
    }
    // A collation-bearing column that observes a different collation
    // than the table's declared default is collation drift: the
    // observed column collation is authoritative when present.
    if let Some(observed_collation) = observed.collation() {
        if table_collation(table) != Some(observed_collation) {
            drifts.push(Drift {
                kind: DriftKind::CollationMismatch,
                path,
            });
        }
    }
}

/// The declared table-collation resolver is threaded from
/// [`introspect_check`] down to the column-level collation check.
/// The closed base name of one type spelling: `varchar(200)`,
/// `varchar`, and `varchar(200) binary` all reduce to `varchar`.
fn base_name(storage_type: &str) -> &str {
    let base = storage_type.split('(').next().unwrap_or(storage_type);
    base.split(' ').next().unwrap_or(base)
}

/// The numeric parameters of one type spelling, `None` when the
/// spelling carries none: `varchar(200)` is `Some("200")`,
/// `decimal(10,4)` is `Some("10,4")`, `bigint unsigned` is `None`.
/// Compared only when both sides spell parameters.
fn parameters(storage_type: &str) -> Option<&str> {
    let open = storage_type.find('(')?;
    let close = storage_type.rfind(')')?;
    if close < open {
        return None;
    }
    Some(&storage_type[open + 1..close])
}

/// The engine echo comparison: the observed engine token against the
/// projected namespace, the observed sql mode against the closed
/// required baseline, and the observed release against the namespace's
/// declared major line. Drift paths name the echo, keeping the kinds
/// table-level and specific.
fn compare_engine(namespace: Namespace, engine: &super::ObservedEngine, drifts: &mut Vec<Drift>) {
    // The engine token must agree with the projected namespace; an
    // evidence document from a foreign engine can never agree.
    let token_matches = matches!(
        (namespace, engine.engine()),
        (Namespace::Mysql, super::EngineEcho::Mysql)
            | (Namespace::Mariadb, super::EngineEcho::Mariadb)
    );
    if !token_matches {
        drifts.push(Drift {
            kind: DriftKind::EngineMismatch,
            path: "engine".to_owned(),
        });
    }
    // The sql mode must carry every token of the declared baseline.
    if REQUIRED_SQL_MODE
        .iter()
        .any(|required| !engine.sql_mode().contains(required))
    {
        drifts.push(Drift {
            kind: DriftKind::SqlModeMismatch,
            path: "engine/sqlMode".to_owned(),
        });
    }
    // The exact release must fall inside the namespace's declared
    // major line: `8.0.*` for mysql (the contracted generation), `10.x`
    // or `11.x` for mariadb (the LTS lines). A different major/minor is
    // a different engine generation than the one the projection was
    // declared for: version drift, never silently equal.
    if !version_in_line(engine.engine_version(), namespace) {
        drifts.push(Drift {
            kind: DriftKind::VersionMismatch,
            path: "engine/engineVersion".to_owned(),
        });
    }
}

/// Whether one normalized release (`major.minor.patch`) falls inside
/// the namespace's declared line. The profile family normalizes vendor
/// strings to the base triple before emission; a value that no longer
/// parses there has already been refused at the wire layer, so an
/// unparseable version here is version drift rather than a crash.
fn version_in_line(version: &str, namespace: Namespace) -> bool {
    let mut parts = version.split('.');
    let Some(major) = parts.next().and_then(|major| major.parse::<u32>().ok()) else {
        return false;
    };
    let minor = parts
        .next()
        .and_then(|minor| minor.parse::<u32>().ok())
        .unwrap_or(0);
    match namespace {
        // 8.0 is the only contracted MySQL generation; 8.4+ is a
        // different generation with its own contract, not a match.
        Namespace::Mysql => major == 8 && minor == 0,
        Namespace::Mariadb => (major == 10 && minor >= 3) || major == 11,
        // Postgres and Laravel projections are outside the evidence
        // family's contracted engines; any mysql-family echo is a
        // mismatch, already reported above.
        Namespace::Postgres | Namespace::Laravel => false,
    }
}
