//! The durable SQLite backend of the local run history (issue #121).
//!
//! One logical history mutation is one durable transaction
//! (`BEGIN IMMEDIATE`, rollback-journal mode, `synchronous=EXTRA`)
//! covering the raw record row, the separately stored assertion set,
//! the allowlisted index projection, every dependent invalidation, and
//! the generation bump — a reader never observes half a deletion, and
//! a crash leaves the old complete state or the new complete state,
//! never half-written evidence. Reads use explicit canonical
//! `ORDER BY (recordedAt, runId)` — never rowid or insertion order.
//!
//! Isolation is by locally generated opaque scope tokens bound to the
//! single project-local home; every lookup is composite
//! `(tenantScopeId, ...)` so cross-scope disclosure is structurally
//! impossible. The store is offline: no account, network, telemetry,
//! transport, provider, shell, or adapter process participates.

use std::path::Path;

use rusqlite::Connection;

use super::clock::{self, Clock, Ids, Instant, SystemClock};
use super::limits;
use super::path::{self, HistoryHome};
use super::types::{
    DependentEntry, DependentKind, DependentSource, DependentState, Observation, OperationKind,
    Outcome, Retention, RunRecord, StoreDocument, TenantScope,
};
use super::validate;
use super::version;
use crate::digest::sha256_hex;
use crate::privacy::canonical;

/// One store failure class. Every class maps to exactly one stable
/// diagnostic at the module boundary; the caller never sees raw SQLite
/// or Git error strings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StoreError {
    /// A fail-closed policy denial of the home or a scope.
    Denied {
        code: &'static str,
        detail: &'static str,
    },
    /// A closed-contract or semantic-rule violation of the request.
    Invalid {
        code: &'static str,
        detail: &'static str,
    },
    /// The bytes are not a valid store for this contract.
    Corrupt(&'static str),
    /// The store was written by a different contract version.
    UnsupportedVersion,
    /// Another writer held the store within the bounded wait.
    Busy,
    /// Any other I/O condition.
    Io,
    /// The run id already exists with different record bytes.
    Conflict,
    /// A record exceeded a retention bound.
    LimitExceeded,
    /// The tenant scope token does not resolve in this store.
    ScopeUnknown,
    /// The observation references a parent run that does not exist.
    ParentMissing,
    /// A bound source record no longer resolves to live bytes.
    SourceMissing(&'static str),
    /// A dependent reference was invalidated.
    DependentInvalidated,
    /// A dependent registration would create a dependency cycle.
    Cycle,
    /// The scope already carries this dependent id.
    DependentExists,
    /// The list cursor no longer matches the scope or generation.
    CursorStale,
    /// A stored record references a policy outside the accepted
    /// custody family.
    PolicyMismatch,
}

/// The bounded index row projection of one stored run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexRow {
    pub(crate) run_id: String,
    pub(crate) recorded_at: String,
    pub(crate) outcome: Outcome,
    pub(crate) operation_kind: OperationKind,
    pub(crate) payload_bytes: u64,
    pub(crate) has_assertions: bool,
}

/// The receipt of one successful ingestion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Receipt {
    pub(crate) run_id: String,
    pub(crate) record_digest: String,
    pub(crate) status_outcome: Outcome,
    pub(crate) reason_codes: Vec<String>,
    pub(crate) generation: u64,
    pub(crate) pruned: Vec<String>,
}

/// The report of one dry-run or applied deletion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DeleteReport {
    pub(crate) applied: bool,
    pub(crate) run_id: String,
    pub(crate) invalidated_dependents: Vec<String>,
    pub(crate) generation: u64,
}

/// The report of one dry-run or applied retention prune.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PruneReport {
    pub(crate) applied: bool,
    pub(crate) deleted_runs: Vec<String>,
    pub(crate) invalidated_dependents: Vec<String>,
    pub(crate) generation: u64,
}

/// The resolution of one dependent reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DependentResolution {
    pub(crate) id: String,
    pub(crate) kind: DependentKind,
    pub(crate) generation: u64,
    pub(crate) sources: Vec<DependentSource>,
}

/// The recovery report of one store verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryReport {
    pub(crate) document: StoreDocument,
    pub(crate) run_count: u64,
    pub(crate) assertion_set_count: u64,
    pub(crate) rebuilt_index_rows: u64,
    pub(crate) verified_digests: u64,
}

/// The open history store.
pub(crate) struct Store {
    connection: Connection,
    clock: Box<dyn Clock>,
    ids: Box<dyn Ids>,
}

impl Store {
    /// Open (creating when absent) with the production clock and the
    /// SQLite PRNG identifier source.
    pub(crate) fn open(project_root: &Path) -> Result<Self, StoreError> {
        Self::open_with_injections(project_root, Box::new(SystemClock))
    }

    /// Open with an injected clock. This seam exists for
    /// crash/recovery/retention tests only: production always uses the
    /// system clock, and identifiers are always random opaque tokens.
    pub(crate) fn open_with_injections(
        project_root: &Path,
        clock: Box<dyn Clock>,
    ) -> Result<Self, StoreError> {
        let home = HistoryHome::resolve(project_root).map_err(StoreError::map_home_failure)?;
        let database = home.database_path().map_err(StoreError::map_home_failure)?;
        let created = !database.exists();
        let connection = Connection::open(&database).map_err(|_| StoreError::Io)?;
        if created {
            path::restrict_file_permissions(&database);
        }
        Self::configure(&connection)?;
        let ids = Box::new(SqliteIds {
            connection: Connection::open_in_memory().map_err(|_| StoreError::Io)?,
        });
        let store = Self {
            connection,
            clock,
            ids,
        };
        store.initialize()?;
        Ok(store)
    }

    /// The bounded PRAGMA posture, read back and verified: the store
    /// refuses unexpected modes instead of assuming them.
    fn configure(connection: &Connection) -> Result<(), StoreError> {
        let _ = connection.busy_timeout(std::time::Duration::from_millis(limits::LOCK_WAIT_MILLIS));
        let mode: String = connection
            .pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))
            .map_err(StoreError::classify)?;
        if mode != "delete" {
            return Err(StoreError::Corrupt("journal-mode"));
        }
        connection
            .pragma_update(None, "synchronous", "EXTRA")
            .map_err(StoreError::classify)?;
        let synchronous: i64 = connection
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .map_err(StoreError::classify)?;
        if synchronous != 3 {
            return Err(StoreError::Corrupt("synchronous"));
        }
        connection
            .pragma_update(None, "secure_delete", "ON")
            .map_err(StoreError::classify)?;
        let secure_delete: i64 = connection
            .query_row("PRAGMA secure_delete", [], |row| row.get(0))
            .map_err(StoreError::classify)?;
        if secure_delete != 1 {
            return Err(StoreError::Corrupt("secure-delete"));
        }
        connection
            .pragma_update(None, "temp_store", "MEMORY")
            .map_err(StoreError::classify)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(StoreError::classify)?;
        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(StoreError::classify)?;
        if foreign_keys != 1 {
            return Err(StoreError::Corrupt("foreign-keys"));
        }
        Ok(())
    }

    /// Create the closed schema and the identity metadata.
    fn initialize(&self) -> Result<(), StoreError> {
        self.connection
            .execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS store_meta (
                     key TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS tenant_scopes (
                     tenant_scope_id TEXT PRIMARY KEY,
                     created_at TEXT NOT NULL
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS runs (
                     tenant_scope_id TEXT NOT NULL REFERENCES tenant_scopes(tenant_scope_id),
                     run_id TEXT NOT NULL,
                     occurred_at TEXT NOT NULL,
                     recorded_at TEXT NOT NULL,
                     outcome TEXT NOT NULL,
                     operation_kind TEXT NOT NULL,
                     record_bytes BLOB NOT NULL,
                     record_digest TEXT NOT NULL,
                     content_digest TEXT NOT NULL,
                     assertion_set_id TEXT,
                     payload_bytes INTEGER NOT NULL,
                     PRIMARY KEY (tenant_scope_id, run_id)
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS assertion_sets (
                     tenant_scope_id TEXT NOT NULL REFERENCES tenant_scopes(tenant_scope_id),
                     set_id TEXT NOT NULL,
                     run_id TEXT NOT NULL,
                     bytes BLOB NOT NULL,
                     digest TEXT NOT NULL,
                     row_count INTEGER NOT NULL,
                     PRIMARY KEY (tenant_scope_id, set_id)
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS run_index (
                     tenant_scope_id TEXT NOT NULL,
                     run_id TEXT NOT NULL,
                     recorded_at TEXT NOT NULL,
                     outcome TEXT NOT NULL,
                     operation_kind TEXT NOT NULL,
                     payload_bytes INTEGER NOT NULL,
                     has_assertions INTEGER NOT NULL,
                     PRIMARY KEY (tenant_scope_id, run_id)
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS dependents (
                     tenant_scope_id TEXT NOT NULL REFERENCES tenant_scopes(tenant_scope_id),
                     dependent_id TEXT NOT NULL,
                     kind TEXT NOT NULL,
                     state TEXT NOT NULL,
                     generation INTEGER NOT NULL,
                     PRIMARY KEY (tenant_scope_id, dependent_id)
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS dependent_sources (
                     tenant_scope_id TEXT NOT NULL,
                     dependent_id TEXT NOT NULL,
                     source_kind TEXT NOT NULL,
                     source_id TEXT NOT NULL,
                     record_digest TEXT,
                     assertion_digest TEXT,
                     PRIMARY KEY (tenant_scope_id, dependent_id, source_kind, source_id)
                 ) WITHOUT ROWID;
                 COMMIT;",
            )
            .map_err(StoreError::classify)?;
        self.check_meta()?;
        Ok(())
    }

    /// The persisted schema identity must be exactly this contract; any
    /// other version refuses as unsupported, malformed bytes as
    /// corruption.
    fn check_meta(&self) -> Result<(), StoreError> {
        let meta = self.read_meta()?;
        match meta.get(version::META_SCHEMA_VERSION).map(String::as_str) {
            None => self.seed_meta()?,
            Some(schema_version) if schema_version == version::STORE_SCHEMA_VERSION => {}
            Some(_) => return Err(StoreError::UnsupportedVersion),
        }
        let meta = self.read_meta()?;
        if meta.get(version::META_IDENTITY).map(String::as_str) != Some(version::STORE_IDENTITY) {
            return Err(StoreError::UnsupportedVersion);
        }
        Ok(())
    }

    fn read_meta(&self) -> Result<std::collections::BTreeMap<String, String>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT key, value FROM store_meta ORDER BY key")
            .map_err(|_| StoreError::Corrupt("meta"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| StoreError::Corrupt("meta"))?;
        let mut meta = std::collections::BTreeMap::new();
        for row in rows {
            let (key, value) = row.map_err(|_| StoreError::Corrupt("meta"))?;
            meta.insert(key, value);
        }
        Ok(meta)
    }

    fn seed_meta(&self) -> Result<(), StoreError> {
        let repository_id = self.random_id()?;
        let retention = serde_json::to_string(&Retention::default()).map_err(|_| StoreError::Io)?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('schema_version', ?1)",
                [version::STORE_SCHEMA_VERSION],
            )
            .map_err(StoreError::classify)?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('identity', ?1)",
                [version::STORE_IDENTITY],
            )
            .map_err(StoreError::classify)?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('repository_id', ?1)",
                [repository_id],
            )
            .map_err(StoreError::classify)?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('generation', '0')",
                [],
            )
            .map_err(StoreError::classify)?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('repository_role', 'local-workspace')",
                [],
            )
            .map_err(StoreError::classify)?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('max_recorded_at', '')",
                [],
            )
            .map_err(StoreError::classify)?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('retention', ?1)",
                [retention],
            )
            .map_err(StoreError::classify)?;
        Ok(())
    }

    /// One random opaque 128-bit token from the SQLite PRNG.
    fn random_id(&self) -> Result<String, StoreError> {
        self.connection
            .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
            .map_err(|_| StoreError::Io)
    }

    fn meta_value(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.connection
            .query_row(
                "SELECT value FROM store_meta WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(StoreError::Io),
            })
    }

    /// The locally generated repository id of this store.
    pub(crate) fn repository_id(&self) -> Result<String, StoreError> {
        self.meta_value("repository_id")?
            .ok_or(StoreError::Corrupt("meta"))
    }

    /// The store-selected repository role.
    pub(crate) fn repository_role(&self) -> Result<String, StoreError> {
        Ok(self
            .meta_value("repository_role")?
            .unwrap_or_else(|| "local-workspace".to_owned()))
    }

    /// Set the store-selected repository role at init time. The role
    /// is a #120 vocabulary label and can never be overridden by a
    /// harness observation.
    pub(crate) fn set_repository_role(&self, role: &str) -> Result<(), StoreError> {
        if crate::privacy::vocab::RepositoryRole::parse(role).is_none() {
            return Err(StoreError::Invalid {
                code: super::codes::INPUT_INVALID,
                detail: "repository-role",
            });
        }
        self.connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('repository_role', ?1)",
                [role],
            )
            .map_err(StoreError::classify)?;
        Ok(())
    }

    /// The current mutation generation.
    pub(crate) fn generation(&self) -> Result<u64, StoreError> {
        self.meta_value("generation")?
            .and_then(|value| value.parse().ok())
            .ok_or(StoreError::Corrupt("meta"))
    }

    fn set_generation(connection: &Connection, generation: u64) -> Result<(), StoreError> {
        connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('generation', ?1)",
                [generation.to_string()],
            )
            .map_err(StoreError::classify_write)?;
        Ok(())
    }

    /// The configured retention bounds.
    pub(crate) fn retention(&self) -> Result<Retention, StoreError> {
        let raw = self
            .meta_value("retention")?
            .ok_or(StoreError::Corrupt("meta"))?;
        serde_json::from_str(&raw).map_err(|_| StoreError::Corrupt("retention"))
    }

    /// Configure the retention bounds; out-of-range values refuse.
    pub(crate) fn set_retention(&self, retention: &Retention) -> Result<(), StoreError> {
        if retention.max_age_days == 0
            || retention.max_age_days > 3650
            || retention.max_records == 0
            || retention.max_records > 1_000_000
            || retention.max_bytes < 1024
            || retention.max_bytes > 1_073_741_824
        {
            return Err(StoreError::Invalid {
                code: super::codes::INPUT_INVALID,
                detail: "retention-bound",
            });
        }
        let raw = serde_json::to_string(retention).map_err(|_| StoreError::Io)?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('retention', ?1)",
                [raw],
            )
            .map_err(StoreError::classify)?;
        Ok(())
    }

    /// Create one tenant scope and return its opaque token.
    pub(crate) fn scope_create(&mut self) -> Result<(String, String), StoreError> {
        let scope_id = self.random_id()?;
        let created = clock::render_seconds(self.clock.now());
        self.connection
            .execute(
                "INSERT INTO tenant_scopes(tenant_scope_id, created_at) VALUES (?1, ?2)",
                rusqlite::params![scope_id, created],
            )
            .map_err(StoreError::classify_write)?;
        Ok((scope_id, created))
    }

    /// Resolve a scope token to its opaque id. An unknown token is a
    /// scope-mismatch denial with no cross-scope disclosure.
    pub(crate) fn scope_id(&self, token: &str) -> Result<String, StoreError> {
        if !validate::is_hex_token32(token) {
            return Err(StoreError::Denied {
                code: super::codes::SCOPE_MISMATCH,
                detail: "scope-grammar",
            });
        }
        let resolved: Option<String> = self
            .connection
            .query_row(
                "SELECT tenant_scope_id FROM tenant_scopes WHERE tenant_scope_id = ?1",
                [token],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(StoreError::Io),
            })?;
        resolved.ok_or(StoreError::ScopeUnknown)
    }

    /// List the tenant scopes of this store.
    pub(crate) fn scopes(&self) -> Result<Vec<TenantScope>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT tenant_scope_id, created_at FROM tenant_scopes
                 ORDER BY created_at, tenant_scope_id",
            )
            .map_err(|_| StoreError::Io)?;
        let rows = statement
            .query_map([], |row| {
                Ok(TenantScope {
                    tenant_scope_id: row.get(0)?,
                    created_at: row.get(1)?,
                })
            })
            .map_err(|_| StoreError::Io)?;
        let mut scopes = Vec::new();
        for row in rows {
            scopes.push(row.map_err(|_| StoreError::Io)?);
        }
        Ok(scopes)
    }

    // -----------------------------------------------------------------
    // Ingestion
    // -----------------------------------------------------------------

    /// Validate, normalize, and atomically append one observation.
    ///
    /// The record and its assertion set are bound in one transaction
    /// together with the index projection, retention enforcement, and
    /// the generation bump. A retry carrying the original run id and
    /// the same observation content is an idempotent success returning
    /// the original receipt; changed content refuses.
    pub(crate) fn append(
        &mut self,
        scope_token: &str,
        observation: &Observation,
        input: &[u8],
    ) -> Result<Receipt, StoreError> {
        let scope_id = self.scope_id(scope_token)?;
        validate::validate_observation(observation).map_err(StoreError::map_violation)?;
        let repository_id = self.repository_id()?;
        let repository_role = self.repository_role()?;

        // Resolve the repeat parent before the transaction: the parent
        // must be live in the same scope.
        let repeat = match &observation.repeat_parent_run_id {
            Some(parent) => {
                let parent_fingerprint = self.parent_fingerprint(&scope_id, parent)?;
                validate::resolve_repeat(
                    &normalize_provenance_for_fingerprint(observation),
                    Some(parent),
                    parent_fingerprint.as_deref(),
                )
                .map_err(StoreError::map_violation)?
            }
            None => None,
        };

        let run_id = match &observation.run_id {
            Some(supplied) => supplied.clone(),
            None => self.ids.next_id(),
        };
        // The assertion-set id derives from the observation bytes and
        // the run id, so an identical retry binds the identical set
        // instead of minting a fresh random id.
        let mut set_seed = Vec::with_capacity(input.len() + run_id.len() + 32);
        set_seed.extend_from_slice(b"lekalo/history/assertion-set\n");
        set_seed.extend_from_slice(input);
        set_seed.extend_from_slice(b"\n");
        set_seed.extend_from_slice(run_id.as_bytes());
        let set_id = sha256_hex(&set_seed)[..32].to_owned();
        let recorded_at = self.clock.now();
        let built = validate::build_record(
            observation,
            &run_id,
            &set_id,
            recorded_at,
            &repository_id,
            &scope_id,
            &repository_role,
            repeat,
        )
        .map_err(StoreError::map_violation)?;

        let record_value = serde_json::to_value(&built.record).map_err(|_| StoreError::Io)?;
        let mut record_bytes = canonical::canonical(&record_value).into_bytes();
        record_bytes.push(b'\n');
        if record_bytes.len() > limits::MAX_RECORD_BYTES {
            return Err(StoreError::LimitExceeded);
        }
        let record_digest = format!("sha256:{}", sha256_hex(&record_bytes));
        let content_digest = sha256_hex(strip_recorded_at(&record_value).as_bytes());
        let payload_bytes = u64::try_from(record_bytes.len()).unwrap_or(u64::MAX);

        let assertion_bytes = built
            .assertions
            .as_ref()
            .map(|set| {
                let value = serde_json::to_value(set).map_err(|_| StoreError::Io)?;
                let mut bytes = canonical::canonical(&value).into_bytes();
                bytes.push(b'\n');
                if bytes.len() > limits::MAX_ASSERTION_BYTES {
                    return Err(StoreError::LimitExceeded);
                }
                let digest = format!("sha256:{}", sha256_hex(&bytes));
                Ok((bytes, digest))
            })
            .transpose()?;
        let row_count = built
            .assertions
            .as_ref()
            .map(|set| set.rows.len())
            .unwrap_or(0);

        let retention = self.retention()?;
        if payload_bytes > retention.max_bytes {
            return Err(StoreError::LimitExceeded);
        }

        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(StoreError::classify)?;
        let result = append_in_transaction(
            &transaction,
            &scope_id,
            &run_id,
            &set_id,
            observation,
            &built.record,
            &record_bytes,
            &record_digest,
            &content_digest,
            payload_bytes,
            assertion_bytes
                .as_ref()
                .map(|(bytes, digest)| (bytes.as_slice(), digest.as_str())),
            row_count,
            &retention,
        );
        let receipt = match result {
            Ok(receipt) => receipt,
            Err(error) => {
                let _ = transaction.rollback();
                return Err(error);
            }
        };
        transaction.commit().map_err(StoreError::classify)?;
        Ok(receipt)
    }

    /// The parent run's recorded input fingerprint, for repeat
    /// comparability. A missing parent is an input refusal: a deleted
    /// parent can never yield an exact claim.
    fn parent_fingerprint(
        &self,
        scope_id: &str,
        parent_run_id: &str,
    ) -> Result<Option<String>, StoreError> {
        let record_bytes: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT record_bytes FROM runs WHERE tenant_scope_id = ?1 AND run_id = ?2",
                rusqlite::params![scope_id, parent_run_id],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(StoreError::Io),
            })?;
        match record_bytes {
            None => Err(StoreError::ParentMissing),
            Some(bytes) => {
                let value: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt("record"))?;
                check_frozen_refs(&value)?;
                // The parent's input fingerprint recomputes from its
                // stored provenance: every parent carries one, repeat
                // link or not.
                Ok(validate::fingerprint_from_record(&value))
            }
        }
    }

    // -----------------------------------------------------------------
    // Read path
    // -----------------------------------------------------------------

    /// The bounded, sorted, same-scope-only list page.
    pub(crate) fn list(
        &self,
        scope_token: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<IndexRow>, Option<String>), StoreError> {
        let scope_id = self.scope_id(scope_token)?;
        if limit == 0 || limit > limits::MAX_LIST_LIMIT {
            return Err(StoreError::Invalid {
                code: super::codes::INPUT_INVALID,
                detail: "limit",
            });
        }
        let generation = self.generation()?;
        let after = match cursor {
            None => None,
            Some(raw) => Some(decode_cursor(raw, &scope_id, generation)?),
        };
        let mut statement = self
            .connection
            .prepare(
                "SELECT run_id, recorded_at, outcome, operation_kind, payload_bytes, has_assertions
                 FROM run_index WHERE tenant_scope_id = ?1
                 AND (?2 IS NULL OR (recorded_at, run_id) > (?2, ?3))
                 ORDER BY recorded_at, run_id
                 LIMIT ?4",
            )
            .map_err(|_| StoreError::Io)?;
        let mapped = statement
            .query_map(
                rusqlite::params![
                    scope_id,
                    after.as_ref().map(|(recorded_at, _)| recorded_at),
                    after.as_ref().map(|(_, run_id)| run_id),
                    limit as i64 + 1
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .map_err(|_| StoreError::Io)?;
        let mut raw_rows: Vec<(String, String, String, String, i64, i64)> = Vec::new();
        for row in mapped {
            raw_rows.push(row.map_err(|_| StoreError::Io)?);
        }
        let has_more = raw_rows.len() > limit;
        raw_rows.truncate(limit);
        let mut parsed = Vec::new();
        for (run_id, recorded_at, outcome, operation_kind, payload_bytes, has_assertions) in
            raw_rows
        {
            parsed.push(IndexRow {
                run_id: run_id.clone(),
                recorded_at: recorded_at.clone(),
                outcome: parse_outcome(&outcome).ok_or(StoreError::Corrupt("index"))?,
                operation_kind: parse_operation_kind(&operation_kind)
                    .ok_or(StoreError::Corrupt("index"))?,
                payload_bytes: u64::try_from(payload_bytes).unwrap_or(u64::MAX),
                has_assertions: has_assertions != 0,
            });
        }
        let next_cursor = has_more.then(|| {
            parsed
                .last()
                .map(|row| encode_cursor(&scope_id, generation, &row.recorded_at, &row.run_id))
                .unwrap_or_default()
        });
        Ok((parsed, next_cursor))
    }

    /// The raw record and the separately stored assertion bytes.
    pub(crate) fn get(
        &self,
        scope_token: &str,
        run_id: &str,
    ) -> Result<(String, Option<String>), StoreError> {
        let scope_id = self.scope_id(scope_token)?;
        if !validate::is_hex_token32(run_id) {
            return Err(StoreError::Denied {
                code: super::codes::SCOPE_MISMATCH,
                detail: "run-grammar",
            });
        }
        let row: Option<(Vec<u8>, Option<String>)> = self
            .connection
            .query_row(
                "SELECT record_bytes, assertion_set_id FROM runs
                 WHERE tenant_scope_id = ?1 AND run_id = ?2",
                rusqlite::params![scope_id, run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(StoreError::Io),
            })?;
        let Some((record_bytes, assertion_set_id)) = row else {
            return Err(StoreError::SourceMissing("run"));
        };
        let assertions = match assertion_set_id {
            None => None,
            Some(set_id) => {
                let bytes: Option<Vec<u8>> = self
                    .connection
                    .query_row(
                        "SELECT bytes FROM assertion_sets
                         WHERE tenant_scope_id = ?1 AND set_id = ?2",
                        rusqlite::params![scope_id, set_id],
                        |row| row.get(0),
                    )
                    .map(Some)
                    .or_else(|error| match error {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        _ => Err(StoreError::Io),
                    })?;
                let bytes = bytes.ok_or(StoreError::Corrupt("assertions"))?;
                Some(String::from_utf8(bytes).map_err(|_| StoreError::Corrupt("assertions"))?)
            }
        };
        let record = String::from_utf8(record_bytes).map_err(|_| StoreError::Corrupt("record"))?;
        Ok((record, assertions))
    }

    // -----------------------------------------------------------------
    // Deletion, retention, and dependent invalidation
    // -----------------------------------------------------------------

    /// Delete one raw record: dry-run reports the transitively
    /// invalidated dependents, apply performs the whole deletion and
    /// invalidation in one transaction. A repeated deletion is an
    /// explicit absent result.
    pub(crate) fn delete(
        &mut self,
        scope_token: &str,
        run_id: &str,
        dry_run: bool,
    ) -> Result<DeleteReport, StoreError> {
        let scope_id = self.scope_id(scope_token)?;
        if !validate::is_hex_token32(run_id) {
            return Err(StoreError::Denied {
                code: super::codes::SCOPE_MISMATCH,
                detail: "run-grammar",
            });
        }
        if dry_run {
            let exists = self.run_exists(&scope_id, run_id)?;
            if !exists {
                return Err(StoreError::SourceMissing("run"));
            }
            let invalidated =
                self.collect_invalidated(&scope_id, &[(String::from("run"), run_id.to_owned())])?;
            return Ok(DeleteReport {
                applied: false,
                run_id: run_id.to_owned(),
                invalidated_dependents: invalidated,
                generation: self.generation()?,
            });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(StoreError::classify)?;
        let result = delete_runs_in_transaction(&transaction, &scope_id, &[run_id.to_owned()]);
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = transaction.rollback();
                return Err(error);
            }
        };
        transaction.commit().map_err(StoreError::classify)?;
        match outcome {
            None => Err(StoreError::SourceMissing("run")),
            Some((deleted, invalidated, generation)) => Ok(DeleteReport {
                applied: true,
                run_id: deleted
                    .first()
                    .cloned()
                    .unwrap_or_else(|| run_id.to_owned()),
                invalidated_dependents: invalidated,
                generation,
            }),
        }
    }

    /// Enforce retention for one scope: dry-run lists the victims, apply
    /// deletes them with dependent invalidation in one transaction.
    pub(crate) fn prune(
        &mut self,
        scope_token: &str,
        dry_run: bool,
    ) -> Result<PruneReport, StoreError> {
        let scope_id = self.scope_id(scope_token)?;
        let retention = self.retention()?;
        let now = self.clock.now();
        if dry_run {
            let victims = retention_victims(&self.connection, &scope_id, &retention, now)?;
            let invalidated = if victims.is_empty() {
                Vec::new()
            } else {
                let refs: Vec<(String, String)> = victims
                    .iter()
                    .map(|run_id| (String::from("run"), run_id.clone()))
                    .collect();
                self.collect_invalidated(&scope_id, &refs)?
            };
            return Ok(PruneReport {
                applied: false,
                deleted_runs: victims,
                invalidated_dependents: invalidated,
                generation: self.generation()?,
            });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(StoreError::classify)?;
        let result = (|| {
            let victims = retention_victims(&transaction, &scope_id, &retention, now)?;
            if victims.is_empty() {
                let generation = current_generation(&transaction)?;
                return Ok((victims, Vec::new(), generation));
            }
            delete_runs_in_transaction(&transaction, &scope_id, &victims)?
                .map(|(_, invalidated, generation)| (victims, invalidated, generation))
                .ok_or(StoreError::Io)
        })();
        let (victims, invalidated, generation) = match result {
            Ok(values) => values,
            Err(error) => {
                let _ = transaction.rollback();
                return Err(error);
            }
        };
        transaction.commit().map_err(StoreError::classify)?;
        Ok(PruneReport {
            applied: true,
            deleted_runs: victims,
            invalidated_dependents: invalidated,
            generation,
        })
    }

    /// Clear one whole scope through the same transactional
    /// delete/invalidation path. Store destruction is not a broad
    /// filesystem recursive-delete feature.
    pub(crate) fn clear(&mut self, scope_token: &str) -> Result<(u64, Vec<String>), StoreError> {
        let scope_id = self.scope_id(scope_token)?;
        let all: Vec<String> = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT run_id FROM runs WHERE tenant_scope_id = ?1
                     ORDER BY recorded_at, run_id",
                )
                .map_err(|_| StoreError::Io)?;
            let rows = statement
                .query_map([scope_id.as_str()], |row| row.get(0))
                .map_err(|_| StoreError::Io)?;
            let mut ids = Vec::new();
            for row in rows {
                ids.push(row.map_err(|_| StoreError::Io)?);
            }
            ids
        };
        if all.is_empty() {
            return Ok((0, Vec::new()));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(StoreError::classify)?;
        let result = delete_runs_in_transaction(&transaction, &scope_id, &all);
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = transaction.rollback();
                return Err(error);
            }
        };
        transaction.commit().map_err(StoreError::classify)?;
        match outcome {
            None => Err(StoreError::Io),
            Some((deleted, invalidated, _)) => {
                let cleared = u64::try_from(deleted.len()).unwrap_or(u64::MAX);
                Ok((cleared, invalidated))
            }
        }
    }

    /// Whether the run row exists in the scope.
    fn run_exists(&self, scope_id: &str, run_id: &str) -> Result<bool, StoreError> {
        let exists: i64 = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE tenant_scope_id = ?1 AND run_id = ?2)",
                rusqlite::params![scope_id, run_id],
                |row| row.get(0),
            )
            .map_err(StoreError::classify)?;
        Ok(exists != 0)
    }

    /// The transitively invalidated dependents of the given source
    /// nodes (read-only BFS for dry-run reports).
    fn collect_invalidated(
        &self,
        scope_id: &str,
        sources: &[(String, String)],
    ) -> Result<Vec<String>, StoreError> {
        let mut invalidated = std::collections::BTreeSet::new();
        let mut frontier: Vec<(String, String)> = sources.to_vec();
        while let Some((kind, id)) = frontier.pop() {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT DISTINCT dependent_id FROM dependent_sources
                     WHERE tenant_scope_id = ?1 AND source_kind = ?2 AND source_id = ?3",
                )
                .map_err(|_| StoreError::Io)?;
            let rows = statement
                .query_map(rusqlite::params![scope_id, kind, id], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|_| StoreError::Io)?;
            for row in rows {
                let dependent_id = row.map_err(|_| StoreError::Io)?;
                if invalidated.insert(dependent_id.clone()) {
                    frontier.push((String::from("dependent"), dependent_id));
                }
            }
        }
        Ok(invalidated.into_iter().collect())
    }

    // -----------------------------------------------------------------
    // Dependents
    // -----------------------------------------------------------------

    /// Register a local dependent reference. Every run source must
    /// resolve live in the same scope (digests are bound from the live
    /// rows); every dependent source must be a currently valid
    /// dependent; a registration that would create a cycle refuses.
    pub(crate) fn register_dependent(
        &mut self,
        scope_token: &str,
        dependent_id: &str,
        kind: DependentKind,
        run_sources: &[String],
        dependent_sources: &[String],
    ) -> Result<(), StoreError> {
        let scope_id = self.scope_id(scope_token)?;
        if !validate::is_token(dependent_id) {
            return Err(StoreError::Invalid {
                code: super::codes::INPUT_INVALID,
                detail: "dependent-id",
            });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(StoreError::classify)?;
        let result = (|| {
            if dependent_exists(&transaction, &scope_id, dependent_id)? {
                return Err(StoreError::DependentExists);
            }
            // Cycle check: no source may reach this dependent through
            // dependent-source edges.
            let mut frontier: Vec<String> = dependent_sources.to_vec();
            let mut seen = std::collections::BTreeSet::new();
            while let Some(id) = frontier.pop() {
                if id == dependent_id {
                    return Err(StoreError::Cycle);
                }
                if !seen.insert(id.clone()) {
                    continue;
                }
                let mut statement = transaction
                    .prepare(
                        "SELECT source_id FROM dependent_sources
                         WHERE tenant_scope_id = ?1 AND dependent_id = ?2
                         AND source_kind = 'dependent'",
                    )
                    .map_err(StoreError::classify)?;
                let rows = statement
                    .query_map(rusqlite::params![scope_id, id], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(StoreError::classify)?;
                for row in rows {
                    frontier.push(row.map_err(StoreError::classify)?);
                }
            }
            // Bind run sources to live digests.
            let mut bound: Vec<(String, String, Option<String>)> = Vec::new();
            for run_id in run_sources {
                let row: Option<(String, Option<String>)> = transaction
                    .query_row(
                        "SELECT record_digest, assertion_set_id FROM runs
                         WHERE tenant_scope_id = ?1 AND run_id = ?2",
                        rusqlite::params![scope_id, run_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map(Some)
                    .or_else(|error| match error {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        _ => Err(StoreError::Io),
                    })?;
                let Some((record_digest, assertion_set_id)) = row else {
                    return Err(StoreError::SourceMissing("run"));
                };
                let assertion_digest = match assertion_set_id {
                    None => None,
                    Some(set_id) => transaction
                        .query_row(
                            "SELECT digest FROM assertion_sets
                             WHERE tenant_scope_id = ?1 AND set_id = ?2",
                            rusqlite::params![scope_id, set_id],
                            |row| row.get::<_, String>(0),
                        )
                        .map(Some)
                        .or_else(|error| match error {
                            rusqlite::Error::QueryReturnedNoRows => Ok(None),
                            _ => Err(StoreError::Io),
                        })?,
                };
                bound.push((run_id.clone(), record_digest, assertion_digest));
            }
            for source_id in dependent_sources {
                let state: Option<String> = transaction
                    .query_row(
                        "SELECT state FROM dependents
                         WHERE tenant_scope_id = ?1 AND dependent_id = ?2",
                        rusqlite::params![scope_id, source_id],
                        |row| row.get(0),
                    )
                    .map(Some)
                    .or_else(|error| match error {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        _ => Err(StoreError::Io),
                    })?;
                match state.as_deref() {
                    Some("valid") => {}
                    Some("invalidated") => return Err(StoreError::DependentInvalidated),
                    _ => return Err(StoreError::SourceMissing("dependent")),
                }
            }
            let generation = current_generation(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO dependents(tenant_scope_id, dependent_id, kind, state, generation)
                     VALUES (?1, ?2, ?3, 'valid', ?4)",
                    rusqlite::params![scope_id, dependent_id, kind.as_str(), generation],
                )
                .map_err(StoreError::classify_write)?;
            for (run_id, record_digest, assertion_digest) in &bound {
                transaction
                    .execute(
                        "INSERT INTO dependent_sources
                         (tenant_scope_id, dependent_id, source_kind, source_id, record_digest, assertion_digest)
                         VALUES (?1, ?2, 'run', ?3, ?4, ?5)",
                        rusqlite::params![scope_id, dependent_id, run_id, record_digest, assertion_digest],
                    )
                    .map_err(StoreError::classify_write)?;
            }
            for source_id in dependent_sources {
                transaction
                    .execute(
                        "INSERT INTO dependent_sources
                         (tenant_scope_id, dependent_id, source_kind, source_id, record_digest, assertion_digest)
                         VALUES (?1, ?2, 'dependent', ?3, NULL, NULL)",
                        rusqlite::params![scope_id, dependent_id, source_id],
                    )
                    .map_err(StoreError::classify_write)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => transaction.commit().map_err(StoreError::classify),
            Err(error) => {
                let _ = transaction.rollback();
                Err(error)
            }
        }
    }

    /// Resolve a dependent reference through live revalidation: the
    /// dependent must be registered, still valid, and every bound
    /// digest must resolve to a live same-scope row right now. A cached
    /// metric claim without live references can never resolve.
    pub(crate) fn resolve_dependent(
        &self,
        scope_token: &str,
        dependent_id: &str,
    ) -> Result<DependentResolution, StoreError> {
        let scope_id = self.scope_id(scope_token)?;
        let row: Option<(String, String, i64)> = self
            .connection
            .query_row(
                "SELECT kind, state, generation FROM dependents
                 WHERE tenant_scope_id = ?1 AND dependent_id = ?2",
                rusqlite::params![scope_id, dependent_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(StoreError::Io),
            })?;
        let Some((kind, state, generation)) = row else {
            return Err(StoreError::SourceMissing("dependent"));
        };
        if state != "valid" {
            return Err(StoreError::DependentInvalidated);
        }
        let kind = DependentKind::parse(&kind).ok_or(StoreError::Corrupt("dependent"))?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT source_kind, source_id, record_digest, assertion_digest
                 FROM dependent_sources WHERE tenant_scope_id = ?1 AND dependent_id = ?2
                 ORDER BY source_kind, source_id",
            )
            .map_err(|_| StoreError::Io)?;
        let rows = statement
            .query_map(rusqlite::params![scope_id, dependent_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|_| StoreError::Io)?;
        let mut sources = Vec::new();
        for row in rows {
            let (source_kind, source_id, record_digest, assertion_digest) =
                row.map_err(|_| StoreError::Io)?;
            match source_kind.as_str() {
                "run" => {
                    let live: Option<(String,)> = self
                        .connection
                        .query_row(
                            "SELECT record_digest FROM runs
                             WHERE tenant_scope_id = ?1 AND run_id = ?2",
                            rusqlite::params![scope_id, source_id],
                            |row| Ok((row.get(0)?,)),
                        )
                        .map(Some)
                        .or_else(|error| match error {
                            rusqlite::Error::QueryReturnedNoRows => Ok(None),
                            _ => Err(StoreError::Io),
                        })?;
                    let Some((live_digest,)) = live else {
                        return Err(StoreError::SourceMissing("run"));
                    };
                    if live_digest != record_digest.clone().unwrap_or_default() {
                        return Err(StoreError::SourceMissing("digest"));
                    }
                    if let Some(assertion_digest) = &assertion_digest {
                        let live_assertion: i64 = self
                            .connection
                            .query_row(
                                "SELECT EXISTS(SELECT 1 FROM assertion_sets
                                 WHERE tenant_scope_id = ?1 AND digest = ?2)",
                                rusqlite::params![scope_id, assertion_digest],
                                |row| row.get(0),
                            )
                            .map_err(StoreError::classify)?;
                        if live_assertion == 0 {
                            return Err(StoreError::SourceMissing("assertion"));
                        }
                    }
                    sources.push(DependentSource {
                        run_id: source_id,
                        record_digest: record_digest.unwrap_or_default(),
                        assertion_digest,
                    });
                }
                "dependent" => {
                    let state: Option<String> = self
                        .connection
                        .query_row(
                            "SELECT state FROM dependents
                             WHERE tenant_scope_id = ?1 AND dependent_id = ?2",
                            rusqlite::params![scope_id, source_id],
                            |row| row.get(0),
                        )
                        .map(Some)
                        .or_else(|error| match error {
                            rusqlite::Error::QueryReturnedNoRows => Ok(None),
                            _ => Err(StoreError::Io),
                        })?;
                    if state.as_deref() != Some("valid") {
                        return Err(StoreError::DependentInvalidated);
                    }
                }
                _ => return Err(StoreError::Corrupt("dependent-source")),
            }
        }
        Ok(DependentResolution {
            id: dependent_id.to_owned(),
            kind,
            generation: u64::try_from(generation).unwrap_or(u64::MAX),
            sources,
        })
    }

    /// The store document (the logical state projection over all
    /// scopes of this repository store).
    pub(crate) fn store_document(&self) -> Result<StoreDocument, StoreError> {
        let repository_id = self.repository_id()?;
        let generation = self.generation()?;
        let retention = self.retention()?;
        let tenant_scopes = self.scopes()?;
        let mut dependents = Vec::new();
        let mut statement = self
            .connection
            .prepare(
                "SELECT tenant_scope_id, dependent_id, kind, state, generation FROM dependents
                 ORDER BY tenant_scope_id, dependent_id",
            )
            .map_err(|_| StoreError::Io)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(|_| StoreError::Io)?;
        for row in rows {
            let (scope_id, dependent_id, kind, state, dependent_generation) =
                row.map_err(|_| StoreError::Io)?;
            let kind = DependentKind::parse(&kind).ok_or(StoreError::Corrupt("dependent"))?;
            let dep_state = match state.as_str() {
                "valid" => DependentState::Valid,
                "invalidated" => DependentState::Invalidated,
                _ => return Err(StoreError::Corrupt("dependent")),
            };
            let mut sources = Vec::new();
            let mut source_statement = self
                .connection
                .prepare(
                    "SELECT source_id, record_digest, assertion_digest
                     FROM dependent_sources
                     WHERE tenant_scope_id = ?1 AND dependent_id = ?2 AND source_kind = 'run'
                     ORDER BY source_id",
                )
                .map_err(|_| StoreError::Io)?;
            let source_rows = source_statement
                .query_map(rusqlite::params![scope_id, dependent_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|_| StoreError::Io)?;
            for source_row in source_rows {
                let (source_id, record_digest, assertion_digest) =
                    source_row.map_err(|_| StoreError::Io)?;
                sources.push(DependentSource {
                    run_id: source_id,
                    record_digest: record_digest.unwrap_or_default(),
                    assertion_digest,
                });
            }
            dependents.push(DependentEntry {
                id: dependent_id,
                kind,
                state: dep_state,
                generation: u64::try_from(dependent_generation).unwrap_or(u64::MAX),
                source_runs: sources,
            });
        }
        Ok(StoreDocument {
            schema_version: version::STORE_SCHEMA_VERSION,
            identity: version::STORE_IDENTITY,
            repository_id,
            tenant_scopes,
            generation,
            retention,
            dependents,
        })
    }

    // -----------------------------------------------------------------
    // Recovery and compaction
    // -----------------------------------------------------------------

    /// Recover the store: verify integrity, foreign keys, identity, and
    /// every record digest, then rebuild the secondary index only from
    /// the validated surviving records. Corruption returns
    /// `history.corrupt` and never silently resets the database or
    /// resurrects invalidated claims.
    pub(crate) fn recover(&mut self) -> Result<RecoveryReport, StoreError> {
        let integrity: String = self
            .connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(|_| StoreError::Corrupt("integrity"))?;
        if integrity != "ok" {
            return Err(StoreError::Corrupt("integrity"));
        }
        {
            let mut statement = self
                .connection
                .prepare("PRAGMA foreign_key_check")
                .map_err(|_| StoreError::Corrupt("foreign-keys"))?;
            let violations = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|_| StoreError::Corrupt("foreign-keys"))?;
            if violations.count() > 0 {
                return Err(StoreError::Corrupt("foreign-keys"));
            }
        }
        self.check_meta()?;

        // Verify every raw record digest and collect the validated
        // index projections.
        let mut statement = self
            .connection
            .prepare(
                "SELECT tenant_scope_id, run_id, record_bytes, record_digest, payload_bytes, assertion_set_id
                 FROM runs ORDER BY tenant_scope_id, run_id",
            )
            .map_err(|_| StoreError::Corrupt("runs"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(|_| StoreError::Corrupt("runs"))?;
        let mut index_rows: Vec<(String, String, String, String, String, i64, i64)> = Vec::new();
        let mut verified = 0u64;
        for row in rows {
            let (scope_id, run_id, record_bytes, record_digest, payload_bytes, assertion_set_id) =
                row.map_err(|_| StoreError::Corrupt("runs"))?;
            if format!("sha256:{}", sha256_hex(&record_bytes)) != record_digest {
                return Err(StoreError::Corrupt("record-digest"));
            }
            let value: serde_json::Value =
                serde_json::from_slice(&record_bytes).map_err(|_| StoreError::Corrupt("record"))?;
            check_frozen_refs(&value)?;
            let outcome = value
                .get("status")
                .and_then(|status| status.get("outcome"))
                .and_then(serde_json::Value::as_str)
                .ok_or(StoreError::Corrupt("record"))?
                .to_owned();
            let operation_kind = value
                .get("operation")
                .and_then(|operation| operation.get("kind"))
                .and_then(serde_json::Value::as_str)
                .ok_or(StoreError::Corrupt("record"))?
                .to_owned();
            let recorded_at = value
                .get("recordedAt")
                .and_then(serde_json::Value::as_str)
                .ok_or(StoreError::Corrupt("record"))?
                .to_owned();
            if parse_outcome(&outcome).is_none() || parse_operation_kind(&operation_kind).is_none()
            {
                return Err(StoreError::Corrupt("record"));
            }
            let has_assertions = i64::from(assertion_set_id.is_some());
            index_rows.push((
                scope_id,
                run_id,
                recorded_at,
                outcome,
                operation_kind,
                payload_bytes,
                has_assertions,
            ));
            verified += 1;
        }
        let assertion_set_count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM assertion_sets", [], |row| row.get(0))
            .map_err(|_| StoreError::Corrupt("assertions"))?;

        // Rebuild the secondary index from the validated records only.
        self.connection
            .execute("DELETE FROM run_index", [])
            .map_err(StoreError::classify)?;
        for (
            scope_id,
            run_id,
            recorded_at,
            outcome,
            operation_kind,
            payload_bytes,
            has_assertions,
        ) in &index_rows
        {
            self.connection
                .execute(
                    "INSERT INTO run_index
                     (tenant_scope_id, run_id, recorded_at, outcome, operation_kind, payload_bytes, has_assertions)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        scope_id,
                        run_id,
                        recorded_at,
                        outcome,
                        operation_kind,
                        payload_bytes,
                        has_assertions
                    ],
                )
                .map_err(StoreError::classify_write)?;
        }

        let document = self.store_document()?;
        Ok(RecoveryReport {
            run_count: u64::try_from(index_rows.len()).unwrap_or(u64::MAX),
            assertion_set_count: u64::try_from(assertion_set_count).unwrap_or(u64::MAX),
            rebuilt_index_rows: u64::try_from(index_rows.len()).unwrap_or(u64::MAX),
            verified_digests: verified,
            document,
        })
    }

    /// Compact the store with `VACUUM`. Runs only after deletes have
    /// committed; a failure is reported separately and never undoes a
    /// committed logical deletion. No `VACUUM INTO` outside the home
    /// exists in this seam.
    pub(crate) fn compact(&self) -> Result<(), StoreError> {
        self.connection
            .execute("VACUUM", [])
            .map_err(StoreError::classify)?;
        Ok(())
    }
}

/// The SQLite PRNG identifier source: production identifiers are random
/// opaque tokens derived from the store's own PRNG, never derived from
/// usernames, hostnames, or content.
#[derive(Debug)]
struct SqliteIds {
    connection: Connection,
}

impl Ids for SqliteIds {
    fn next_id(&mut self) -> String {
        self.connection
            .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
            .unwrap_or_default()
    }
}

fn current_generation(connection: &Connection) -> Result<u64, StoreError> {
    connection
        .query_row(
            "SELECT value FROM store_meta WHERE key = 'generation'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(StoreError::classify)?
        .parse()
        .map_err(|_| StoreError::Corrupt("generation"))
}

/// Every stored record must reference the exact accepted #120 custody
/// family. A record that names a different policy or authority version
/// refuses as a policy mismatch; readers never relabel to the latest
/// family silently.
fn check_frozen_refs(record: &serde_json::Value) -> Result<(), StoreError> {
    let privacy = record.get("privacy").ok_or(StoreError::Corrupt("record"))?;
    let policy = privacy
        .get("policyRef")
        .ok_or(StoreError::Corrupt("record"))?;
    let authority = privacy
        .get("authorityRef")
        .ok_or(StoreError::Corrupt("record"))?;
    let matches_ref = |value: &serde_json::Value, id_key: &str, id: &str, digest: &str| {
        value.get(id_key).and_then(serde_json::Value::as_str) == Some(id)
            && value.get("digest").and_then(serde_json::Value::as_str) == Some(digest)
            && value.get("version").and_then(serde_json::Value::as_str)
                == Some(crate::privacy::refs::POLICY_VERSION)
    };
    if !matches_ref(
        policy,
        "policyId",
        crate::privacy::refs::POLICY_ID,
        crate::privacy::refs::POLICY_DIGEST,
    ) || !matches_ref(
        authority,
        "contractId",
        crate::privacy::refs::AUTHORITY_CONTRACT_ID,
        AUTHORITY_DIGEST_REF,
    ) {
        return Err(StoreError::PolicyMismatch);
    }
    Ok(())
}

/// The exact authority digest with the digest-algorithm prefix (the
/// frozen bare-hex constant lives in the privacy refs).
const AUTHORITY_DIGEST_REF: &str =
    "sha256:7ae6454ea20f7b61202d368411ef9bff4e70af96f1f2a408c209d84fe9722f80";

/// The shared transactional append body.
#[allow(clippy::too_many_arguments)]
fn append_in_transaction(
    connection: &Connection,
    scope_id: &str,
    run_id: &str,
    set_id: &str,
    observation: &Observation,
    record: &RunRecord,
    record_bytes: &[u8],
    record_digest: &str,
    content_digest: &str,
    payload_bytes: u64,
    assertion: Option<(&[u8], &str)>,
    row_count: usize,
    retention: &Retention,
) -> Result<Receipt, StoreError> {
    // Idempotent replay and conflict detection.
    let existing: Option<(String, String)> = connection
        .query_row(
            "SELECT record_digest, content_digest FROM runs
             WHERE tenant_scope_id = ?1 AND run_id = ?2",
            rusqlite::params![scope_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            _ => Err(StoreError::Io),
        })?;
    let (fresh_insert, record_digest, reason_codes) = match &existing {
        Some((existing_record_digest, existing_content_digest)) => {
            if existing_record_digest == record_digest {
                // Byte-identical replay.
                (false, existing_record_digest.clone(), Vec::new())
            } else if existing_content_digest == content_digest {
                // A retry of the same observation: identical content,
                // only the store-authored ingestion time differs.
                (
                    false,
                    existing_record_digest.clone(),
                    vec!["replay-idempotent".to_owned()],
                )
            } else {
                return Err(StoreError::Conflict);
            }
        }
        None => {
            insert_run(
                connection,
                scope_id,
                run_id,
                set_id,
                observation,
                record,
                record_bytes,
                record_digest,
                content_digest,
                payload_bytes,
                assertion,
                row_count,
            )?;
            (true, record_digest.to_owned(), Vec::new())
        }
    };

    let mut pruned = Vec::new();
    if fresh_insert {
        // Retention enforcement in the same transaction; a rolled-back
        // clock never accelerates deletion.
        pruned = enforce_retention(connection, scope_id, retention, record)?;
    }
    let generation = if fresh_insert {
        let generation = current_generation(connection)? + 1;
        Store::set_generation(connection, generation)?;
        connection
            .execute(
                "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('max_recorded_at', ?1)",
                [record.recorded_at.as_str()],
            )
            .map_err(StoreError::classify_write)?;
        generation
    } else {
        current_generation(connection)?
    };

    Ok(Receipt {
        run_id: run_id.to_owned(),
        record_digest,
        status_outcome: record.status.outcome,
        reason_codes,
        generation,
        pruned,
    })
}

/// Insert one complete run (raw row + assertions + index projection).
#[allow(clippy::too_many_arguments)]
fn insert_run(
    connection: &Connection,
    scope_id: &str,
    run_id: &str,
    set_id: &str,
    observation: &Observation,
    record: &RunRecord,
    record_bytes: &[u8],
    record_digest: &str,
    content_digest: &str,
    payload_bytes: u64,
    assertion: Option<(&[u8], &str)>,
    row_count: usize,
) -> Result<(), StoreError> {
    let operation_kind = serde_json::to_value(record.operation.kind)
        .map_err(|_| StoreError::Io)?
        .as_str()
        .ok_or(StoreError::Io)?
        .to_owned();
    connection
        .execute(
            "INSERT INTO runs
             (tenant_scope_id, run_id, occurred_at, recorded_at, outcome, operation_kind,
              record_bytes, record_digest, content_digest, assertion_set_id, payload_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                scope_id,
                run_id,
                observation.timestamp,
                record.recorded_at,
                record.status.outcome.as_str(),
                operation_kind,
                record_bytes,
                record_digest,
                content_digest,
                assertion.map(|_| set_id),
                payload_bytes as i64,
            ],
        )
        .map_err(StoreError::classify_write)?;
    if let Some((bytes, digest)) = assertion {
        connection
            .execute(
                "INSERT INTO assertion_sets(tenant_scope_id, set_id, run_id, bytes, digest, row_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![scope_id, set_id, run_id, bytes, digest, row_count as i64],
            )
            .map_err(StoreError::classify_write)?;
    }
    connection
        .execute(
            "INSERT INTO run_index
             (tenant_scope_id, run_id, recorded_at, outcome, operation_kind, payload_bytes, has_assertions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                scope_id,
                run_id,
                record.recorded_at,
                record.status.outcome.as_str(),
                operation_kind,
                payload_bytes as i64,
                i64::from(assertion.is_some()),
            ],
        )
        .map_err(StoreError::classify_write)?;
    Ok(())
}

impl StoreError {
    /// The read/PRAGMA classification.
    fn classify(error: rusqlite::Error) -> StoreError {
        match &error {
            rusqlite::Error::SqliteFailure(code, message)
                if code.code == rusqlite::ErrorCode::DatabaseBusy
                    || message
                        .as_deref()
                        .is_some_and(|text| text.contains("locked")) =>
            {
                StoreError::Busy
            }
            rusqlite::Error::SqliteFailure(code, _)
                if code.code == rusqlite::ErrorCode::DatabaseCorrupt =>
            {
                StoreError::Corrupt("sqlite")
            }
            _ => StoreError::Io,
        }
    }

    /// The write-path classification: a unique/foreign-key constraint
    /// violation of a bound insert is a conflict or corruption class,
    /// never a silent success.
    fn classify_write(error: rusqlite::Error) -> StoreError {
        match &error {
            rusqlite::Error::SqliteFailure(code, message)
                if code.code == rusqlite::ErrorCode::DatabaseBusy
                    || message
                        .as_deref()
                        .is_some_and(|text| text.contains("locked")) =>
            {
                StoreError::Busy
            }
            rusqlite::Error::SqliteFailure(code, _)
                if code.code == rusqlite::ErrorCode::DatabaseCorrupt =>
            {
                StoreError::Corrupt("sqlite")
            }
            _ => StoreError::Io,
        }
    }

    fn map_home_failure(failure: path::HomeFailure) -> StoreError {
        match failure {
            path::HomeFailure::Denied { code, detail } => StoreError::Denied { code, detail },
            path::HomeFailure::Io => StoreError::Io,
        }
    }

    pub(crate) fn map_violation(violation: validate::Violation) -> StoreError {
        if violation.code == super::codes::VERSION_UNSUPPORTED {
            StoreError::UnsupportedVersion
        } else {
            StoreError::Invalid {
                code: violation.code,
                detail: violation.detail,
            }
        }
    }
}

/// The canonical record with `recordedAt` removed (the retry content
/// identity): identical observations produce identical content digests
/// even though the store-authored ingestion time differs.
fn strip_recorded_at(record_value: &serde_json::Value) -> String {
    let mut copy = record_value.clone();
    if let serde_json::Value::Object(map) = &mut copy {
        map.remove("recordedAt");
    }
    canonical::canonical(&copy)
}

/// The retention victims for one scope under the strictest applicable
/// bound, oldest first in stable `(recordedAt, runId)` order. Retention
/// age uses the store-authored `recordedAt`; a rolled-back clock never
/// accelerates deletion because the effective now is the maximum seen.
/// The bytes bound measures logical canonical payload bytes, never the
/// on-disk SQLite size.
fn retention_victims(
    connection: &Connection,
    scope_id: &str,
    retention: &Retention,
    now: Instant,
) -> Result<Vec<String>, StoreError> {
    let max_recorded_at: String = connection
        .query_row(
            "SELECT value FROM store_meta WHERE key = 'max_recorded_at'",
            [],
            |row| row.get(0),
        )
        .map_err(StoreError::classify)?;
    let effective_now = match clock::parse(&max_recorded_at) {
        Some(seen) if seen > now => seen,
        _ => now,
    };
    let cutoff_seconds = effective_now.seconds - i64::from(retention.max_age_days) * 86_400;
    let cutoff = clock::render_millis(Instant {
        seconds: cutoff_seconds,
        nanos: 0,
    });

    // The full candidate list in stable `(recordedAt, runId)` order.
    let mut ordered: Vec<(String, String, i64)> = Vec::new(); // (recorded_at, run_id, payload_bytes)
    {
        let mut statement = connection
            .prepare(
                "SELECT run_id, recorded_at, payload_bytes FROM runs WHERE tenant_scope_id = ?1
                 ORDER BY recorded_at, run_id",
            )
            .map_err(|_| StoreError::Io)?;
        let rows = statement
            .query_map([scope_id], |row| {
                // (recorded_at, run_id, payload_bytes) — the stable
                // retention order is (recordedAt, runId).
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|_| StoreError::Io)?;
        for row in rows {
            ordered.push(row.map_err(|_| StoreError::Io)?);
        }
    }
    let selected = |victims: &[(String, String, i64)]| -> i64 {
        victims.iter().map(|(_, _, bytes)| *bytes).sum()
    };
    let mut victims: Vec<(String, String, i64)> = Vec::new();
    let mut victim_ids = std::collections::BTreeSet::new();
    let mark = |entry: (String, String, i64),
                victims: &mut Vec<(String, String, i64)>,
                ids: &mut std::collections::BTreeSet<String>| {
        if ids.insert(entry.1.clone()) {
            victims.push(entry);
        }
    };

    // Age bound: everything recorded before the cutoff.
    for entry in ordered
        .iter()
        .filter(|(recorded_at, _, _)| recorded_at.as_str() < cutoff.as_str())
    {
        mark(entry.clone(), &mut victims, &mut victim_ids);
    }

    // Record-count bound: the oldest beyond the bound.
    let count = i64::try_from(ordered.len()).unwrap_or(i64::MAX);
    let overflow = count - i64::from(retention.max_records);
    if overflow > 0 {
        for entry in ordered
            .iter()
            .take(usize::try_from(overflow).unwrap_or(usize::MAX))
        {
            mark(entry.clone(), &mut victims, &mut victim_ids);
        }
    }

    // Logical payload-bytes bound: drop oldest first until under the
    // bound.
    let mut excess = selected(&ordered)
        - selected(&victims)
        - i64::try_from(retention.max_bytes).unwrap_or(i64::MAX);
    for entry in &ordered {
        if excess <= 0 {
            break;
        }
        if victim_ids.contains(&entry.1) {
            continue;
        }
        mark(entry.clone(), &mut victims, &mut victim_ids);
        excess -= entry.2;
    }

    // The stable prune order is (recordedAt, runId); the candidate
    // list already arrives in that order.
    victims.sort();
    Ok(victims.into_iter().map(|(_, run_id, _)| run_id).collect())
}

/// Enforce retention inside the append transaction and return the
/// pruned run ids.
fn enforce_retention(
    connection: &Connection,
    scope_id: &str,
    retention: &Retention,
    record: &RunRecord,
) -> Result<Vec<String>, StoreError> {
    let recorded_at = clock::parse(&record.recorded_at).ok_or(StoreError::Corrupt("record"))?;
    let victims = retention_victims(connection, scope_id, retention, recorded_at)?;
    if victims.is_empty() {
        return Ok(victims);
    }
    let (_, _, _) =
        delete_runs_in_transaction(connection, scope_id, &victims)?.ok_or(StoreError::Io)?;
    Ok(victims)
}

/// One transactional deletion outcome: the deleted run ids, the
/// transitively invalidated dependent ids, and the new generation.
type DeletionOutcome = (Vec<String>, Vec<String>, u64);

/// Delete runs, their assertion sets, index rows, and dependent links,
/// invalidating transitive dependents and bumping the generation in the
/// caller's transaction. Returns `(deleted, invalidated, generation)`,
/// or `None` when none of the runs existed.
fn delete_runs_in_transaction(
    connection: &Connection,
    scope_id: &str,
    run_ids: &[String],
) -> Result<Option<DeletionOutcome>, StoreError> {
    let mut existing: Vec<String> = Vec::new();
    for run_id in run_ids {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE tenant_scope_id = ?1 AND run_id = ?2)",
                rusqlite::params![scope_id, run_id],
                |row| row.get(0),
            )
            .map_err(StoreError::classify)?;
        if exists != 0 {
            existing.push(run_id.clone());
        }
    }
    if existing.is_empty() {
        return Ok(None);
    }

    // Transitive invalidation: BFS over run -> dependent -> dependent.
    let mut invalidated = std::collections::BTreeSet::new();
    let mut frontier: Vec<(String, String)> = existing
        .iter()
        .map(|run_id| (String::from("run"), run_id.clone()))
        .collect();
    while let Some((kind, id)) = frontier.pop() {
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT dependent_id FROM dependent_sources
                 WHERE tenant_scope_id = ?1 AND source_kind = ?2 AND source_id = ?3",
            )
            .map_err(|_| StoreError::Io)?;
        let rows = statement
            .query_map(rusqlite::params![scope_id, kind, id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|_| StoreError::Io)?;
        for row in rows {
            let dependent_id = row.map_err(|_| StoreError::Io)?;
            if invalidated.insert(dependent_id.clone()) {
                frontier.push((String::from("dependent"), dependent_id));
            }
        }
    }
    for dependent_id in &invalidated {
        connection
            .execute(
                "UPDATE dependents SET state = 'invalidated'
                 WHERE tenant_scope_id = ?1 AND dependent_id = ?2",
                rusqlite::params![scope_id, dependent_id],
            )
            .map_err(StoreError::classify_write)?;
        // The invalidation state keeps only the opaque id, kind, and
        // generation: the old source identity lists and any cached
        // claim values are removed with the sources.
        connection
            .execute(
                "DELETE FROM dependent_sources
                 WHERE tenant_scope_id = ?1 AND dependent_id = ?2",
                rusqlite::params![scope_id, dependent_id],
            )
            .map_err(StoreError::classify_write)?;
    }
    for run_id in &existing {
        connection
            .execute(
                "DELETE FROM dependent_sources
                 WHERE tenant_scope_id = ?1 AND source_kind = 'run' AND source_id = ?2",
                rusqlite::params![scope_id, run_id],
            )
            .map_err(StoreError::classify_write)?;
        connection
            .execute(
                "DELETE FROM assertion_sets WHERE tenant_scope_id = ?1 AND run_id = ?2",
                rusqlite::params![scope_id, run_id],
            )
            .map_err(StoreError::classify_write)?;
        connection
            .execute(
                "DELETE FROM run_index WHERE tenant_scope_id = ?1 AND run_id = ?2",
                rusqlite::params![scope_id, run_id],
            )
            .map_err(StoreError::classify_write)?;
        connection
            .execute(
                "DELETE FROM runs WHERE tenant_scope_id = ?1 AND run_id = ?2",
                rusqlite::params![scope_id, run_id],
            )
            .map_err(StoreError::classify_write)?;
    }
    let generation = current_generation(connection)? + 1;
    Store::set_generation(connection, generation)?;
    Ok(Some((
        existing,
        invalidated.into_iter().collect(),
        generation,
    )))
}

/// Whether the scope carries this dependent id.
fn dependent_exists(
    connection: &Connection,
    scope_id: &str,
    dependent_id: &str,
) -> Result<bool, StoreError> {
    let exists: i64 = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM dependents
             WHERE tenant_scope_id = ?1 AND dependent_id = ?2)",
            rusqlite::params![scope_id, dependent_id],
            |row| row.get(0),
        )
        .map_err(StoreError::classify)?;
    Ok(exists != 0)
}

/// The opaque list cursor binding scope and generation; any mutation
/// moves the generation and stale cursors refuse.
fn encode_cursor(scope_id: &str, generation: u64, recorded_at: &str, run_id: &str) -> String {
    format!("v1|{scope_id}|{generation}|{recorded_at}|{run_id}")
}

fn decode_cursor(
    raw: &str,
    scope_id: &str,
    generation: u64,
) -> Result<(String, String), StoreError> {
    let parts: Vec<&str> = raw.splitn(5, '|').collect();
    if parts.len() != 5 || parts[0] != "v1" {
        return Err(StoreError::CursorStale);
    }
    if parts[1] != scope_id {
        return Err(StoreError::CursorStale);
    }
    if parts[2] != generation.to_string() {
        return Err(StoreError::CursorStale);
    }
    if !validate::is_hex_token32(parts[4]) {
        return Err(StoreError::CursorStale);
    }
    Ok((parts[3].to_owned(), parts[4].to_owned()))
}

fn parse_outcome(text: &str) -> Option<Outcome> {
    match text {
        "pass" => Some(Outcome::Pass),
        "warn" => Some(Outcome::Warn),
        "fail" => Some(Outcome::Fail),
        "unsupported" => Some(Outcome::Unsupported),
        "infrastructure" => Some(Outcome::Infrastructure),
        _ => None,
    }
}

fn parse_operation_kind(text: &str) -> Option<OperationKind> {
    match text {
        "scan" => Some(OperationKind::Scan),
        "verify" => Some(OperationKind::Verify),
        "context" => Some(OperationKind::Context),
        "generate" => Some(OperationKind::Generate),
        "nfr" => Some(OperationKind::Nfr),
        "scenario" => Some(OperationKind::Scenario),
        "gate" => Some(OperationKind::Gate),
        "evaluation" => Some(OperationKind::Evaluation),
        _ => None,
    }
}

/// Normalize an observation's provenance for the fingerprint pass (the
/// same normalization the record build applies).
pub(crate) fn normalize_provenance_for_fingerprint(
    observation: &Observation,
) -> super::types::Provenance {
    let provenance = &observation.provenance;
    let default_git = super::types::GitProvenanceIn::default();
    let git = provenance.git.as_ref().unwrap_or(&default_git);
    let default_model = super::types::ModelProvenanceIn::default();
    let model = provenance.model.as_ref().unwrap_or(&default_model);
    let default_lock = super::types::LockProvenanceIn::default();
    let lock = provenance.lock.as_ref().unwrap_or(&default_lock);
    let default_core = super::types::CoreProvenanceIn::default();
    let core = provenance.core.as_ref().unwrap_or(&default_core);
    let default_profile = super::types::ProfileProvenanceIn::default();
    let profile = provenance.profile.as_ref().unwrap_or(&default_profile);
    let default_harness = super::types::HarnessProvenanceIn::default();
    let harness = provenance.harness.as_ref().unwrap_or(&default_harness);
    super::types::Provenance {
        git: super::types::GitProvenance {
            commit: normalize(git.commit.clone()),
            dirty: normalize(git.dirty.clone()),
            working_set_digest: normalize(git.working_set_digest.clone()),
        },
        model: super::types::ModelProvenance {
            revision: normalize(model.revision.clone()),
            digest: normalize(model.digest.clone()),
            ir_digest: normalize(model.ir_digest.clone()),
        },
        lock: super::types::LockProvenance {
            version: normalize(lock.version.clone()),
            digest: normalize(lock.digest.clone()),
        },
        core: super::types::CoreProvenance {
            version: normalize(core.version.clone()),
            build_revision: normalize(core.build_revision.clone()),
            build_digest: normalize(core.build_digest.clone()),
        },
        adapters: provenance.adapters.clone(),
        profile: super::types::ProfileProvenance {
            id: normalize(profile.id.clone()),
            version: normalize(profile.version.clone()),
            digest: normalize(profile.digest.clone()),
        },
        harness: super::types::HarnessProvenance {
            id: normalize(harness.id.clone()),
            version: normalize(harness.version.clone()),
            model_id: normalize(harness.model_id.clone()),
            model_revision: normalize(harness.model_revision.clone()),
        },
    }
}

fn normalize<T>(leaf: Option<super::value::Vs<T>>) -> super::value::Vs<T> {
    super::value::Vs::normalize(leaf)
}
