//! The SQLite cache backend (issue #20).
//!
//! One logical cache update is one durable transaction (`journal_mode=WAL`,
//! `synchronous=FULL`) covering the entry row and its dependency edges;
//! readers never observe half a record. Writer serialization is SQLite's
//! own (`BEGIN IMMEDIATE` under a bounded busy timeout); readers run
//! concurrently. All reads use explicit canonical `ORDER BY` — never
//! rowid or insertion order. Open-time validation rejects unsupported
//! schema versions and malformed bytes as corruption, which the caller
//! quarantines and rebuilds; it is never a trusted hit.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use super::key::RecordKind;
use super::limits::{LOCK_WAIT_MILLIS, MAX_PAYLOAD_BYTES};
use super::record::Record;
use super::version;
use super::{StoreError, StoredEntry};

/// The open store.
#[derive(Debug)]
pub(crate) struct SqliteStore {
    connection: Connection,
    writable: bool,
}

impl SqliteStore {
    /// Open (creating when absent) for cached writes.
    pub(crate) fn open_create(path: &Path) -> Result<Self, StoreError> {
        let connection = Connection::open(path).map_err(|_| StoreError::Io)?;
        Self::configure(&connection)?;
        let store = Self {
            connection,
            writable: true,
        };
        store.initialize()?;
        Ok(store)
    }

    /// Open read-only for `cache status`; never creates or writes.
    pub(crate) fn open_read_only(path: &Path) -> Result<Self, StoreError> {
        let connection = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
                | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|_| StoreError::Io)?;
        Self::configure(&connection)?;
        let store = Self {
            connection,
            writable: false,
        };
        store.check_meta()?;
        Ok(store)
    }

    fn configure(connection: &Connection) -> Result<(), StoreError> {
        let _ = connection.busy_timeout(Duration::from_millis(LOCK_WAIT_MILLIS));
        let _ = connection.pragma_update(None, "journal_mode", "WAL");
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(Self::classify)?;
        Ok(())
    }

    /// Create the closed schema and the identity metadata.
    fn initialize(&self) -> Result<(), StoreError> {
        self.connection
            .execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS cache_meta (
                     key TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS entries (
                     entry_key_digest TEXT PRIMARY KEY,
                     record_kind TEXT NOT NULL,
                     key_bytes BLOB NOT NULL,
                     payload BLOB NOT NULL,
                     payload_digest TEXT NOT NULL,
                     payload_bytes INTEGER NOT NULL,
                     snapshot BLOB
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS dependencies (
                     entry_key_digest TEXT NOT NULL,
                     dependency_key_digest TEXT NOT NULL,
                     role TEXT NOT NULL,
                     ordinal INTEGER NOT NULL,
                     PRIMARY KEY (entry_key_digest, dependency_key_digest, role, ordinal)
                 ) WITHOUT ROWID;
                 INSERT OR IGNORE INTO cache_meta(key, value)
                     VALUES ('schema_version', 'lekalo/cache/v1.0.0'),
                             ('identity', 'dev.lekalo.cache@1.0.0');
                 COMMIT;",
            )
            .map_err(Self::classify)?;
        self.check_meta()?;
        Ok(())
    }

    /// The persisted schema identity must be exactly this contract; any
    /// other version is corruption for the running implementation.
    fn check_meta(&self) -> Result<(), StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT key, value FROM cache_meta ORDER BY key")
            .map_err(|_| StoreError::Corrupt)?;
        let mut schema_version = None;
        let mut identity = None;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| StoreError::Corrupt)?;
        for row in rows {
            let (key, value) = row.map_err(|_| StoreError::Corrupt)?;
            match key.as_str() {
                "schema_version" => schema_version = Some(value),
                "identity" => identity = Some(value),
                _ => {}
            }
        }
        if schema_version.as_deref() != Some(version::SCHEMA_VERSION)
            || identity.as_deref() != Some(version::IDENTITY)
        {
            return Err(StoreError::UnsupportedVersion);
        }
        Ok(())
    }

    /// Whether the store passes a bounded integrity check.
    pub(crate) fn integrity_ok(&self) -> bool {
        let mut statement = match self.connection.prepare("PRAGMA quick_check") {
            Ok(statement) => statement,
            Err(_) => return false,
        };
        let mut rows = match statement.query([]) {
            Ok(rows) => rows,
            Err(_) => return false,
        };
        match rows.next() {
            Ok(Some(row)) => matches!(row.get::<_, String>(0), Ok(value) if value == "ok"),
            _ => false,
        }
    }

    /// Fetch one raw entry by its key digest. The caller validates the
    /// payload digest and the canonical key bytes before any typed use.
    pub(crate) fn get(&self, key_digest: &str) -> Result<Option<StoredEntry>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT record_kind, key_bytes, payload, payload_digest, snapshot
                 FROM entries WHERE entry_key_digest = ?1",
            )
            .map_err(|_| StoreError::Corrupt)?;
        let mut rows = statement
            .query([key_digest])
            .map_err(|_| StoreError::Corrupt)?;
        let row = match rows.next().map_err(|_| StoreError::Corrupt)? {
            Some(row) => row,
            None => return Ok(None),
        };
        let entry = StoredEntry {
            record_kind: row.get(0).map_err(|_| StoreError::Corrupt)?,
            key_bytes: row.get(1).map_err(|_| StoreError::Corrupt)?,
            payload: row.get(2).map_err(|_| StoreError::Corrupt)?,
            payload_digest: row.get(3).map_err(|_| StoreError::Corrupt)?,
            snapshot: row.get(4).map_err(|_| StoreError::Corrupt)?,
        };
        Ok(Some(entry))
    }

    /// One durable put: the entry row and its dependency edges commit
    /// atomically. Returns `false` when an equal entry already exists.
    pub(crate) fn put(
        &self,
        key_digest: &str,
        record: &Record,
        snapshot: Option<&[u8]>,
    ) -> Result<bool, StoreError> {
        if !self.writable {
            return Err(StoreError::ReadOnly);
        }
        // Full envelope validation before any typed allocation: a record
        // that fails here is corruption by construction, never a hit.
        if !record.validate() {
            return Err(StoreError::Corrupt);
        }
        let payload_value_bytes = super::canonical::canonical_bytes(&record.payload.value);
        if payload_value_bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(StoreError::LimitExceeded);
        }
        let key_bytes = super::canonical::canonical_bytes(&record.key);
        self.begin_immediate()?;
        let result = self.insert_entry(
            key_digest,
            record,
            &key_bytes,
            &payload_value_bytes,
            snapshot,
        );
        if let Err(error) = result {
            let _ = self.connection.execute_batch("ROLLBACK");
            return Err(error);
        }
        self.connection
            .execute_batch("COMMIT")
            .map_err(Self::classify)?;
        Ok(result.expect("checked above"))
    }

    /// One entry insert inside the open immediate transaction; the
    /// return value mirrors `INSERT OR IGNORE`.
    fn insert_entry(
        &self,
        key_digest: &str,
        record: &Record,
        key_bytes: &[u8],
        payload_value_bytes: &[u8],
        snapshot: Option<&[u8]>,
    ) -> Result<bool, StoreError> {
        let inserted = self
            .connection
            .execute(
                "INSERT OR IGNORE INTO entries(
                     entry_key_digest, record_kind, key_bytes,
                     payload, payload_digest, payload_bytes, snapshot
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    key_digest,
                    record.record_kind.as_str(),
                    key_bytes,
                    payload_value_bytes,
                    record.payload_digest,
                    payload_value_bytes.len() as i64,
                    snapshot,
                ],
            )
            .map_err(Self::classify)?;
        if inserted == 0 {
            return Ok(false);
        }
        for edge in &record.dependencies {
            self.connection
                .execute(
                    "INSERT OR IGNORE INTO dependencies(
                         entry_key_digest, dependency_key_digest, role, ordinal
                     ) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        key_digest,
                        edge.key_digest,
                        edge.role.as_str(),
                        edge.ordinal as i64,
                    ],
                )
                .map_err(Self::classify)?;
        }
        Ok(true)
    }

    /// Open one bounded immediate write transaction on the shared
    /// connection; SQLite serializes writers and applies the busy timeout.
    fn begin_immediate(&self) -> Result<(), StoreError> {
        self.connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(Self::classify)?;
        Ok(())
    }

    /// Bounded record counts by kind, in canonical kind order.
    pub(crate) fn record_counts(&self) -> Result<Vec<(RecordKind, u64)>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT record_kind, COUNT(*) FROM entries
                 GROUP BY record_kind ORDER BY record_kind",
            )
            .map_err(|_| StoreError::Corrupt)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|_| StoreError::Corrupt)?;
        let mut counts = Vec::new();
        for row in rows {
            let (kind, count) = row.map_err(|_| StoreError::Corrupt)?;
            let Some(kind) = RecordKind::parse(&kind) else {
                return Err(StoreError::Corrupt);
            };
            counts.push((kind, count.max(0) as u64));
        }
        Ok(counts)
    }

    /// The total number of dependency edges.
    pub(crate) fn dependency_edge_count(&self) -> Result<u64, StoreError> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM dependencies", [], |row| row.get(0))
            .map_err(|_| StoreError::Corrupt)?;
        Ok(count.max(0) as u64)
    }

    /// Deterministic, non-semantic eviction: over the entry or byte limit,
    /// evict by frozen retention class (opaque-reference kinds first,
    /// `source` last), larger payload first, smaller key digest first.
    /// Returns the number of entries removed.
    pub(crate) fn evict(
        &self,
        max_entries: usize,
        max_total_bytes: usize,
    ) -> Result<usize, StoreError> {
        if !self.writable {
            return Err(StoreError::ReadOnly);
        }
        let entries: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .map_err(|_| StoreError::Corrupt)?;
        let bytes: Option<i64> = self
            .connection
            .query_row("SELECT SUM(payload_bytes) FROM entries", [], |row| {
                row.get(0)
            })
            .map_err(|_| StoreError::Corrupt)?;
        let current_entries = entries.max(0) as usize;
        let current_bytes = bytes.unwrap_or(0).max(0) as usize;
        if current_entries <= max_entries && current_bytes <= max_total_bytes {
            return Ok(0);
        }
        // Walk the ranked entries and stop at the first prefix that
        // satisfies both limits: deterministic, bounded, non-semantic.
        let mut statement = self
            .connection
            .prepare(
                "SELECT entry_key_digest, payload_bytes FROM entries
                 ORDER BY CASE record_kind
                     WHEN 'context-key' THEN 0
                     WHEN 'artifact-manifest-key' THEN 1
                     WHEN 'adapter-result' THEN 2
                     WHEN 'adapter-capability' THEN 3
                     WHEN 'effect-fragment' THEN 4
                     WHEN 'graph-fragment' THEN 5
                     WHEN 'ir-fragment' THEN 6
                     WHEN 'parsed-fragment' THEN 7
                     WHEN 'source' THEN 8
                     ELSE 9
                 END ASC,
                 payload_bytes DESC,
                 entry_key_digest ASC",
            )
            .map_err(|_| StoreError::Corrupt)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|_| StoreError::Corrupt)?;
        let mut evicted_bytes = 0usize;
        let mut cut = 0usize;
        let mut victims: Vec<String> = Vec::new();
        for row in rows {
            let (digest, payload_bytes) = row.map_err(|_| StoreError::Corrupt)?;
            victims.push(digest);
            evicted_bytes += payload_bytes.max(0) as usize;
            cut += 1;
            if cut + max_entries >= current_entries
                && evicted_bytes + max_total_bytes >= current_bytes
            {
                break;
            }
        }
        if victims.is_empty() {
            return Ok(0);
        }
        self.begin_immediate()?;
        for digest in &victims {
            let dependencies = self.connection.execute(
                "DELETE FROM dependencies WHERE entry_key_digest = ?1",
                [digest],
            );
            let entries = self
                .connection
                .execute("DELETE FROM entries WHERE entry_key_digest = ?1", [digest]);
            if let Err(error) = dependencies.and(entries) {
                let _ = self.connection.execute_batch("ROLLBACK");
                return Err(Self::classify(error));
            }
        }
        self.connection
            .execute_batch("COMMIT")
            .map_err(Self::classify)?;
        Ok(victims.len())
    }

    fn classify(error: rusqlite::Error) -> StoreError {
        match error {
            rusqlite::Error::SqliteFailure(code, _)
                if code.code == rusqlite::ErrorCode::DatabaseBusy
                    || code.code == rusqlite::ErrorCode::DatabaseLocked =>
            {
                StoreError::Locked
            }
            rusqlite::Error::SqliteFailure(code, _)
                if code.code == rusqlite::ErrorCode::SystemIoFailure
                    || code.code == rusqlite::ErrorCode::DiskFull =>
            {
                StoreError::Io
            }
            rusqlite::Error::SqliteFailure(code, _)
                if code.code == rusqlite::ErrorCode::NotADatabase
                    || code.code == rusqlite::ErrorCode::DatabaseCorrupt =>
            {
                StoreError::Corrupt
            }
            _ => StoreError::Io,
        }
    }
}
