//! The engine portability report (issue #117).
//!
//! [`portability`] compares two profiles — the source engine and the
//! target engine — and reports, per capability id, what the move gains,
//! loses, strengthens, or weakens, plus the named
//! PostgreSQL-specific-semantics block describing what a PostgreSQL
//! source does not carry over. The report is a plain serializable
//! value: two runs over the same inputs are byte-identical, every id
//! appears in the fixed byte-sorted order, and no entry depends on
//! caller input order. Portability never launches anything and never
//! resolves anything.

use serde::Serialize;

use super::id::ProfileCapabilityId;
use super::StorageEngineProfile;

/// The closed support states the report compares, including the
/// explicit unknown of absence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportSupport {
    Full,
    Partial,
    Unsupported,
    Unknown,
}

impl ReportSupport {
    fn of(profile: &StorageEngineProfile, id: ProfileCapabilityId) -> Self {
        match super::capabilities::ProfileSupport::of(profile, id) {
            super::capabilities::ProfileSupport::Full => Self::Full,
            super::capabilities::ProfileSupport::Partial => Self::Partial,
            super::capabilities::ProfileSupport::Unsupported => Self::Unsupported,
            super::capabilities::ProfileSupport::Unknown => Self::Unknown,
        }
    }
}

/// One capability delta between the two profiles.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct CapabilityChange {
    /// The stable dotted capability id.
    pub id: String,
    /// The source profile's support state.
    pub source: ReportSupport,
    /// The target profile's support state.
    pub target: ReportSupport,
}

/// The whole portability report between two profiles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PortabilityReport {
    /// The source engine token.
    pub source: String,
    /// The target engine token.
    pub target: String,
    /// The ids whose support changed, byte-sorted by id.
    pub changes: Vec<CapabilityChange>,
    /// The ids the source provides (full) and the target does not
    /// (unsupported or unknown), byte-sorted by id.
    pub loses: Vec<String>,
    /// The ids the target provides (full) and the source does not,
    /// byte-sorted by id.
    pub gains: Vec<String>,
    /// The named PostgreSQL-specific semantics block (present only when
    /// the caller asks for the postgres→mysql-family direction through
    /// [`named_postgres_divergences`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "postgresDivergences")]
    pub postgres_divergences: Option<Vec<NamedDivergence>>,
}

/// One named PostgreSQL-specific semantic that MySQL-family engines do
/// not provide, with the closed substitute evidence.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct NamedDivergence {
    /// The closed divergence name.
    pub name: &'static str,
    /// The closed support state on the target engine.
    pub support: ReportSupport,
    /// The bounded substitute/notes declaration.
    pub note: &'static str,
}

/// Compare two profiles. Pure and read-only; byte-identical for
/// value-equal inputs.
pub fn portability(
    source: &StorageEngineProfile,
    target: &StorageEngineProfile,
) -> PortabilityReport {
    let mut changes: Vec<CapabilityChange> = Vec::new();
    let mut loses: Vec<String> = Vec::new();
    let mut gains: Vec<String> = Vec::new();
    for id in ALL_IDS {
        let source_support = ReportSupport::of(source, *id);
        let target_support = ReportSupport::of(target, *id);
        if source_support == target_support {
            continue;
        }
        changes.push(CapabilityChange {
            id: id.key().to_owned(),
            source: source_support,
            target: target_support,
        });
        if source_support == ReportSupport::Full
            && matches!(
                target_support,
                ReportSupport::Unsupported | ReportSupport::Unknown
            )
        {
            loses.push(id.key().to_owned());
        }
        if target_support == ReportSupport::Full
            && matches!(
                source_support,
                ReportSupport::Unsupported | ReportSupport::Unknown
            )
        {
            gains.push(id.key().to_owned());
        }
    }
    changes.sort();
    PortabilityReport {
        source: source.engine().engine().key().to_owned(),
        target: target.engine().engine().key().to_owned(),
        changes,
        loses,
        gains,
        postgres_divergences: None,
    }
}

/// Attach the named PostgreSQL-specific semantics block to a report:
/// every field is data, nothing is silently dropped. The caller names
/// the direction (postgres → a mysql-family target).
pub fn named_postgres_divergences(mut report: PortabilityReport) -> PortabilityReport {
    report.postgres_divergences = Some(
        DIVERGENCES
            .iter()
            .map(|(name, support, note)| NamedDivergence {
                name,
                support: *support,
                note,
            })
            .collect(),
    );
    report
}

/// The closed id table, byte-sorted by wire spelling.
pub(crate) const ALL_IDS: &[ProfileCapabilityId] = &[
    ProfileCapabilityId::ConcurrencyCompareAndSet,
    ProfileCapabilityId::ConcurrencyEtagIfMatch,
    ProfileCapabilityId::IdempotencyDurableKey,
    ProfileCapabilityId::IdempotencyReplay,
    ProfileCapabilityId::IndexDescending,
    ProfileCapabilityId::IndexFunctional,
    ProfileCapabilityId::IndexInvisible,
    ProfileCapabilityId::InvariantCollationAware,
    ProfileCapabilityId::InvariantUniqueConcurrent,
    ProfileCapabilityId::IsolationReadCommitted,
    ProfileCapabilityId::IsolationRepeatableRead,
    ProfileCapabilityId::IsolationSerializable,
    ProfileCapabilityId::IsolationSnapshot,
    ProfileCapabilityId::LockExclusive,
    ProfileCapabilityId::LockKey,
    ProfileCapabilityId::LockNowait,
    ProfileCapabilityId::LockRange,
    ProfileCapabilityId::LockShared,
    ProfileCapabilityId::LockSkipLocked,
    ProfileCapabilityId::PaginationKeysetCursor,
    ProfileCapabilityId::PaginationLimitOffset,
    ProfileCapabilityId::StorageAdvisoryLocks,
    ProfileCapabilityId::StorageArrayTypes,
    ProfileCapabilityId::StorageCheckConstraints,
    ProfileCapabilityId::StorageCollationAware,
    ProfileCapabilityId::StorageDeferredConstraints,
    ProfileCapabilityId::StorageExclusionConstraints,
    ProfileCapabilityId::StorageFulltextIndex,
    ProfileCapabilityId::StorageGeneratedColumns,
    ProfileCapabilityId::StorageIntrospectionChecked,
    ProfileCapabilityId::StorageJsonOperators,
    ProfileCapabilityId::StoragePartialIndex,
    ProfileCapabilityId::StoragePrefixIndex,
    ProfileCapabilityId::StorageReturning,
    ProfileCapabilityId::StorageSequences,
    ProfileCapabilityId::StorageTimestamptz,
    ProfileCapabilityId::TestCreateSchema,
    ProfileCapabilityId::TestDropSchema,
    ProfileCapabilityId::TransactionAtomicGroup,
    ProfileCapabilityId::TransactionRollback,
    ProfileCapabilityId::TransactionTransactionalDdl,
];

/// The named PostgreSQL-specific semantics block (issue #117 §4.8):
/// each field is `full|partial|unsupported` plus the bounded substitute
/// note — nothing silently dropped.
const DIVERGENCES: &[(&str, ReportSupport, &str)] = &[
    (
        "partialIndex",
        ReportSupport::Unsupported,
        "generated column plus index substitute",
    ),
    (
        "jsonbOperators",
        ReportSupport::Partial,
        "JSON_EXTRACT family; mariadb longtext plus JSON_VALID alias",
    ),
    (
        "timestamptz",
        ReportSupport::Unsupported,
        "datetime(6) canonical; timestamp opt-in with session-tz conversion",
    ),
    (
        "sequences",
        ReportSupport::Partial,
        "mariadb 10.3+ sequences; mysql auto_increment only",
    ),
    (
        "deferrableConstraints",
        ReportSupport::Unsupported,
        "immediate checks only; FOREIGN_KEY_CHECKS is a session escape",
    ),
    (
        "transactionalDdl",
        ReportSupport::Unsupported,
        "DDL is implicit-commit",
    ),
    (
        "returning",
        ReportSupport::Partial,
        "mariadb 10.5+ partial; mysql unsupported",
    ),
    (
        "citext",
        ReportSupport::Partial,
        "case-insensitive collation (_ci) substitute",
    ),
    (
        "arrays",
        ReportSupport::Unsupported,
        "no array domain type; json encoding substitute",
    ),
    (
        "exclusionConstraints",
        ReportSupport::Unsupported,
        "no exclusion constraint surface",
    ),
    (
        "advisoryLocks",
        ReportSupport::Partial,
        "GET_LOCK named-lock substitute",
    ),
    (
        "enumDomainType",
        ReportSupport::Unsupported,
        "no enum domain type in the closed vocabulary",
    ),
    (
        "fullTextSearch",
        ReportSupport::Partial,
        "fulltext index; parser and stopword semantics are engine-specific",
    ),
];
