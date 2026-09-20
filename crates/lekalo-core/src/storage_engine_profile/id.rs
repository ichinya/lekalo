//! Typed identifiers of the storage-engine-profile attachment (issue
//! #117).
//!
//! Every wire string becomes one closed typed value before it can exist
//! in a profile. The engine and variant tokens, the sql-mode tokens,
//! and the closed capability-id vocabulary are all enumerated; the
//! free-form members (charset, collation, storage engine name, time
//! zone) are bounded lowercase/offset spellings checked at
//! normalization.

use crate::scenario::id::IdError;

/// The exact engine identity spelling of one profile. MySQL and
/// MariaDB are separate profiles, never one family.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EngineToken {
    /// The MySQL engine.
    Mysql,
    /// The MariaDB engine.
    Mariadb,
}

impl EngineToken {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Mysql => "mysql",
            Self::Mariadb => "mariadb",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "mysql" => Some(Self::Mysql),
            "mariadb" => Some(Self::Mariadb),
            _ => None,
        }
    }
}

/// The closed distribution variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum VariantToken {
    /// The MySQL Community distribution.
    MysqlCommunity,
    /// The MariaDB distribution.
    Mariadb,
}

impl VariantToken {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::MysqlCommunity => "mysql-community",
            Self::Mariadb => "mariadb",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "mysql-community" => Some(Self::MysqlCommunity),
            "mariadb" => Some(Self::Mariadb),
            _ => None,
        }
    }
}

/// One closed sql-mode token.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SqlModeToken {
    AnsiQuotes,
    ErrorForDivisionByZero,
    NoEngineSubstitution,
    NoZeroDate,
    NoZeroInDate,
    OnlyFullGroupBy,
    StrictAllTables,
    StrictTransTables,
    Traditional,
}

impl SqlModeToken {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::AnsiQuotes => "ANSI_QUOTES",
            Self::ErrorForDivisionByZero => "ERROR_FOR_DIVISION_BY_ZERO",
            Self::NoEngineSubstitution => "NO_ENGINE_SUBSTITUTION",
            Self::NoZeroDate => "NO_ZERO_DATE",
            Self::NoZeroInDate => "NO_ZERO_IN_DATE",
            Self::OnlyFullGroupBy => "ONLY_FULL_GROUP_BY",
            Self::StrictAllTables => "STRICT_ALL_TABLES",
            Self::StrictTransTables => "STRICT_TRANS_TABLES",
            Self::Traditional => "TRADITIONAL",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "ANSI_QUOTES" => Some(Self::AnsiQuotes),
            "ERROR_FOR_DIVISION_BY_ZERO" => Some(Self::ErrorForDivisionByZero),
            "NO_ENGINE_SUBSTITUTION" => Some(Self::NoEngineSubstitution),
            "NO_ZERO_DATE" => Some(Self::NoZeroDate),
            "NO_ZERO_IN_DATE" => Some(Self::NoZeroInDate),
            "ONLY_FULL_GROUP_BY" => Some(Self::OnlyFullGroupBy),
            "STRICT_ALL_TABLES" => Some(Self::StrictAllTables),
            "STRICT_TRANS_TABLES" => Some(Self::StrictTransTables),
            "TRADITIONAL" => Some(Self::Traditional),
            _ => None,
        }
    }
}

/// One closed profile capability id. The vocabulary spans the #24
/// engine-neutral concurrency ids plus the storage/index/test ids the
/// engine evidence needs; every id is one enumerated member, never an
/// open string.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProfileCapabilityId {
    ConcurrencyCompareAndSet,
    ConcurrencyEtagIfMatch,
    IdempotencyDurableKey,
    IdempotencyReplay,
    IndexDescending,
    IndexFunctional,
    IndexInvisible,
    InvariantCollationAware,
    InvariantUniqueConcurrent,
    IsolationReadCommitted,
    IsolationRepeatableRead,
    IsolationSerializable,
    IsolationSnapshot,
    LockExclusive,
    LockKey,
    LockNowait,
    LockRange,
    LockShared,
    LockSkipLocked,
    PaginationKeysetCursor,
    PaginationLimitOffset,
    StorageAdvisoryLocks,
    StorageArrayTypes,
    StorageCheckConstraints,
    StorageCollationAware,
    StorageDeferredConstraints,
    StorageExclusionConstraints,
    StorageFulltextIndex,
    StorageGeneratedColumns,
    StorageIntrospectionChecked,
    StorageJsonOperators,
    StoragePartialIndex,
    StoragePrefixIndex,
    StorageReturning,
    StorageSequences,
    StorageTimestamptz,
    TestCreateSchema,
    TestDropSchema,
    TransactionAtomicGroup,
    TransactionRollback,
    TransactionTransactionalDdl,
}

impl ProfileCapabilityId {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::ConcurrencyCompareAndSet => "concurrency.compare_and_set",
            Self::ConcurrencyEtagIfMatch => "concurrency.etag_if_match",
            Self::IdempotencyDurableKey => "idempotency.durable_key",
            Self::IdempotencyReplay => "idempotency.replay",
            Self::IndexDescending => "index.descending",
            Self::IndexFunctional => "index.functional",
            Self::IndexInvisible => "index.invisible",
            Self::InvariantCollationAware => "invariant.collation_aware",
            Self::InvariantUniqueConcurrent => "invariant.unique_concurrent",
            Self::IsolationReadCommitted => "isolation.read_committed",
            Self::IsolationRepeatableRead => "isolation.repeatable_read",
            Self::IsolationSerializable => "isolation.serializable",
            Self::IsolationSnapshot => "isolation.snapshot",
            Self::LockExclusive => "lock.exclusive",
            Self::LockKey => "lock.key",
            Self::LockNowait => "lock.nowait",
            Self::LockRange => "lock.range",
            Self::LockShared => "lock.shared",
            Self::LockSkipLocked => "lock.skip_locked",
            Self::PaginationKeysetCursor => "pagination.keyset_cursor",
            Self::PaginationLimitOffset => "pagination.limit_offset",
            Self::StorageAdvisoryLocks => "storage.advisory_locks",
            Self::StorageArrayTypes => "storage.array_types",
            Self::StorageCheckConstraints => "storage.check_constraints",
            Self::StorageCollationAware => "storage.collation_aware",
            Self::StorageDeferredConstraints => "storage.deferred_constraints",
            Self::StorageExclusionConstraints => "storage.exclusion_constraints",
            Self::StorageFulltextIndex => "storage.fulltext_index",
            Self::StorageGeneratedColumns => "storage.generated_columns",
            Self::StorageIntrospectionChecked => "storage.introspection_checked",
            Self::StorageJsonOperators => "storage.json_operators",
            Self::StoragePartialIndex => "storage.partial_index",
            Self::StoragePrefixIndex => "storage.prefix_index",
            Self::StorageReturning => "storage.returning",
            Self::StorageSequences => "storage.sequences",
            Self::StorageTimestamptz => "storage.timestamptz",
            Self::TestCreateSchema => "test.create_schema",
            Self::TestDropSchema => "test.drop_schema",
            Self::TransactionAtomicGroup => "transaction.atomic_group",
            Self::TransactionRollback => "transaction.rollback",
            Self::TransactionTransactionalDdl => "transaction.transactional_ddl",
        }
    }

    /// Parse one wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        use ProfileCapabilityId as Id;
        Some(match text {
            "concurrency.compare_and_set" => Id::ConcurrencyCompareAndSet,
            "concurrency.etag_if_match" => Id::ConcurrencyEtagIfMatch,
            "idempotency.durable_key" => Id::IdempotencyDurableKey,
            "idempotency.replay" => Id::IdempotencyReplay,
            "index.descending" => Id::IndexDescending,
            "index.functional" => Id::IndexFunctional,
            "index.invisible" => Id::IndexInvisible,
            "invariant.collation_aware" => Id::InvariantCollationAware,
            "invariant.unique_concurrent" => Id::InvariantUniqueConcurrent,
            "isolation.read_committed" => Id::IsolationReadCommitted,
            "isolation.repeatable_read" => Id::IsolationRepeatableRead,
            "isolation.serializable" => Id::IsolationSerializable,
            "isolation.snapshot" => Id::IsolationSnapshot,
            "lock.exclusive" => Id::LockExclusive,
            "lock.key" => Id::LockKey,
            "lock.nowait" => Id::LockNowait,
            "lock.range" => Id::LockRange,
            "lock.shared" => Id::LockShared,
            "lock.skip_locked" => Id::LockSkipLocked,
            "pagination.keyset_cursor" => Id::PaginationKeysetCursor,
            "pagination.limit_offset" => Id::PaginationLimitOffset,
            "storage.advisory_locks" => Id::StorageAdvisoryLocks,
            "storage.array_types" => Id::StorageArrayTypes,
            "storage.check_constraints" => Id::StorageCheckConstraints,
            "storage.collation_aware" => Id::StorageCollationAware,
            "storage.deferred_constraints" => Id::StorageDeferredConstraints,
            "storage.exclusion_constraints" => Id::StorageExclusionConstraints,
            "storage.fulltext_index" => Id::StorageFulltextIndex,
            "storage.generated_columns" => Id::StorageGeneratedColumns,
            "storage.introspection_checked" => Id::StorageIntrospectionChecked,
            "storage.json_operators" => Id::StorageJsonOperators,
            "storage.partial_index" => Id::StoragePartialIndex,
            "storage.prefix_index" => Id::StoragePrefixIndex,
            "storage.returning" => Id::StorageReturning,
            "storage.sequences" => Id::StorageSequences,
            "storage.timestamptz" => Id::StorageTimestamptz,
            "test.create_schema" => Id::TestCreateSchema,
            "test.drop_schema" => Id::TestDropSchema,
            "transaction.atomic_group" => Id::TransactionAtomicGroup,
            "transaction.rollback" => Id::TransactionRollback,
            "transaction.transactional_ddl" => Id::TransactionTransactionalDdl,
            _ => return None,
        })
    }
}

/// The closed session time-zone spelling: an offset, UTC, or the
/// explicit SYSTEM declaration.
pub fn is_time_zone(text: &str) -> bool {
    let bytes = text.as_bytes();
    if text == "SYSTEM" || text == "UTC" {
        return true;
    }
    if bytes.len() != 6 || text.as_bytes()[0] != b'+' && text.as_bytes()[0] != b'-' {
        return false;
    }
    bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3] == b':'
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_digit()
}

/// The exact engine-release spelling: major.minor.patch, never a range
/// or a wildcard. Each part is 1..=4 ASCII digits without leading
/// zeros beyond the single zero.
pub fn is_engine_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts.iter().all(|part| {
        if part.is_empty() || part.len() > 4 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        *part == "0" || !part.starts_with('0')
    })
}

/// Unused-by-wire error re-export so the id module shape matches the
/// established family layout.
#[allow(dead_code)]
pub(crate) type IdErrorAlias = IdError;
