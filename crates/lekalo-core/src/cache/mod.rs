//! Issue #20: the Lekalo incremental cache.
//!
//! The cache is Lekalo-owned derived data under `.lekalo/cache/**` only,
//! backed by SQLite at exactly `.lekalo/cache/cache.sqlite`. It is never a
//! canonical source, never read as model input, and never authoritative:
//! every hit is a pure acceleration whose output is byte-identical to a
//! clean full rebuild. Content digests — never mtime, never wall-clock —
//! decide validity. Corruption degrades to recomputation; path, authority,
//! and privacy violations of the cache home stay fail-closed denials.
//!
//! The module is composed of:
//! - [`version`]: the closed `dev.lekalo.cache@1.0.0` contract identity;
//! - [`canonical`]: canonical bytes and SHA-256 digests;
//! - [`key`]: the closed typed key vocabulary and key digests;
//! - [`record`]: the closed self-describing record envelope;
//! - [`snapshot`]: the cache-owned spanned-tree document snapshot;
//! - [`limits`]: the bounds checked before any allocation;
//! - [`path`]: the governed cache home and its containment checks;
//! - [`sqlite`]: the durable SQLite backend;
//! - [`pipeline`]: the cached load pipeline and the session.

pub(crate) mod canonical;
pub(crate) mod key;
pub(crate) mod limits;
pub(crate) mod path;
pub(crate) mod pipeline;
pub(crate) mod record;
pub(crate) mod snapshot;
pub(crate) mod sqlite;
pub(crate) mod version;

use crate::loader::error::Diagnostic;
use crate::loader::LoadSelection;
use crate::result::{DomainResult, Status};

pub use pipeline::Session;

/// One raw stored entry returned by the backend before typed validation.
pub(crate) struct StoredEntry {
    pub(crate) record_kind: String,
    pub(crate) key_bytes: Vec<u8>,
    pub(crate) payload: Vec<u8>,
    pub(crate) payload_digest: String,
    pub(crate) snapshot: Option<Vec<u8>>,
}

/// Backend failure classes the pipeline maps onto degradation states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StoreError {
    /// Another writer holds the store within the bounded wait.
    Locked,
    /// The bytes are not a valid store for this contract version.
    Corrupt,
    /// A store written by a different cache contract version.
    UnsupportedVersion,
    /// A record exceeded a closed bound.
    LimitExceeded,
    /// A read-only store was asked to write.
    ReadOnly,
    /// Any other I/O condition.
    Io,
}

/// `lekalo load`: the cached model envelope. The bypass path is the
/// published loader path itself; the cached path reuses decoded documents
/// and renders through the loader's own renderer, so both are
/// byte-identical.
pub fn run_load(selection: &LoadSelection, spans: bool, bypass: bool) -> DomainResult {
    let session = match Session::open(selection, bypass) {
        Err(result) => return result,
        Ok(session) => session,
    };
    let model = match session.load_model(selection) {
        Err(result) => return result,
        Ok(model) => model,
    };
    crate::loader::render_model_envelope(&model, spans)
}

/// `lekalo load --ir`: the cached load plus the typed IR compilation.
pub fn load_compiled(
    selection: &LoadSelection,
    bypass: bool,
) -> Result<(crate::loader::NormalizedModel, crate::ir::Compilation), DomainResult> {
    let session = Session::open(selection, bypass)?;
    session.load_compiled(selection)
}

/// `lekalo cache status`: the read-only bounded health projection.
pub fn status(selection: &LoadSelection) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let home = match path::CacheHome::resolve(&root) {
        Err(path::HomeFailure::Denied { code, logical }) => {
            return crate::loader::diagnostic::failure(
                Status::Denied,
                vec![Diagnostic::new(code).with_path(logical)],
            );
        }
        Err(path::HomeFailure::Io) => {
            return health_result(Health {
                state: "unreadable",
                records: Vec::new(),
                dependency_edges: 0,
                total_bytes: 0,
            })
        }
        Ok(home) => home,
    };
    let exists = match home.database_exists() {
        Err(_) => {
            return health_result(Health {
                state: "unreadable",
                records: Vec::new(),
                dependency_edges: 0,
                total_bytes: 0,
            })
        }
        Ok(exists) => exists,
    };
    let total_bytes = home.total_bytes().unwrap_or(0);
    if !exists {
        return health_result(Health {
            state: "missing",
            records: Vec::new(),
            dependency_edges: 0,
            total_bytes,
        });
    }
    let Ok(db_path) = home.database_path() else {
        return health_result(Health {
            state: "unreadable",
            records: Vec::new(),
            dependency_edges: 0,
            total_bytes,
        });
    };
    match sqlite::SqliteStore::open_read_only(&db_path) {
        Err(StoreError::Corrupt) | Err(StoreError::UnsupportedVersion) => health_result(Health {
            state: "corrupt",
            records: Vec::new(),
            dependency_edges: 0,
            total_bytes,
        }),
        Err(_) => health_result(Health {
            state: "unreadable",
            records: Vec::new(),
            dependency_edges: 0,
            total_bytes,
        }),
        Ok(store) => {
            if !store.integrity_ok() {
                return health_result(Health {
                    state: "corrupt",
                    records: Vec::new(),
                    dependency_edges: 0,
                    total_bytes,
                });
            }
            let records: Vec<(String, u64)> = store
                .record_counts()
                .unwrap_or_default()
                .into_iter()
                .map(|(kind, count)| (kind.as_str().to_owned(), count))
                .collect();
            let dependency_edges = store.dependency_edge_count().unwrap_or(0);
            let state = if records.iter().all(|(_, count)| *count == 0) || records.is_empty() {
                "empty"
            } else {
                "ok"
            };
            health_result(Health {
                state,
                records,
                dependency_edges,
                total_bytes,
            })
        }
    }
}

/// `lekalo cache clear --yes`: clear the cache home except the issue #9
/// migration home, which stays under its own custody.
pub fn clear(selection: &LoadSelection) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let home = match path::CacheHome::resolve(&root) {
        Err(path::HomeFailure::Denied { code, logical }) => {
            return crate::loader::diagnostic::failure(
                Status::Denied,
                vec![Diagnostic::new(code).with_path(logical)],
            );
        }
        Err(path::HomeFailure::Io) => {
            return crate::loader::diagnostic::failure(
                Status::Invalid,
                vec![Diagnostic::new("loader.io")
                    .with_path(".lekalo/cache")
                    .with_data(serde_json::json!({ "detail": "read-failed" }))],
            );
        }
        Ok(home) => home,
    };
    match home.clear() {
        Err(path::HomeFailure::Denied { code, logical }) => crate::loader::diagnostic::failure(
            Status::Denied,
            vec![Diagnostic::new(code).with_path(logical)],
        ),
        Err(path::HomeFailure::Io) => crate::loader::diagnostic::failure(
            Status::Invalid,
            vec![Diagnostic::new("loader.io")
                .with_path(".lekalo/cache")
                .with_data(serde_json::json!({ "detail": "clear-failed" }))],
        ),
        Ok(removed) => {
            let json = format!(
                "{{\n  \"status\": \"valid\",\n  \"operation\": \"cache-clear\",\n  \"removedEntries\": {}\n}}",
                removed
            );
            let human = format!("cache cleared : {} top-level entries removed", removed);
            DomainResult::receipt(json, human)
        }
    }
}

/// The bounded health facts of one status run.
struct Health {
    state: &'static str,
    records: Vec<(String, u64)>,
    dependency_edges: u64,
    total_bytes: u64,
}

/// Render the health projection as the accepted envelope: the fixed key
/// order `status`, `cache`, with the records array sorted by kind.
fn health_result(health: Health) -> DomainResult {
    let mut json = String::from("{\"status\":\"valid\",\"cache\":{\"schemaVersion\":");
    json.push_str(&serde_json::to_string(version::SCHEMA_VERSION).expect("const serializes"));
    json.push_str(",\"identity\":");
    json.push_str(&serde_json::to_string(version::IDENTITY).expect("const serializes"));
    json.push_str(",\"state\":");
    json.push_str(&serde_json::to_string(health.state).expect("state serializes"));
    json.push_str(",\"backend\":");
    json.push_str(&serde_json::to_string(version::BACKEND).expect("const serializes"));
    json.push_str(",\"records\":[");
    let mut records = health.records;
    records.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    for (index, (kind, count)) in records.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str("{\"kind\":");
        json.push_str(&serde_json::to_string(kind).expect("kind serializes"));
        json.push_str(",\"count\":");
        json.push_str(&count.to_string());
        json.push('}');
    }
    json.push_str("],\"dependencyEdges\":");
    json.push_str(&health.dependency_edges.to_string());
    json.push_str(",\"totalBytes\":");
    json.push_str(&health.total_bytes.to_string());
    json.push_str("}}");
    let record_total: u64 = records.iter().map(|(_, count)| count).sum();
    let human = format!(
        "cache {} : {} records, {} edges, {} bytes",
        health.state, record_total, health.dependency_edges, health.total_bytes
    );
    // The generic JSON+human success payload seam (the same one `effects`
    // uses): one `DomainResult`, both projections.
    DomainResult::graph(json, human, Vec::new())
}

#[cfg(test)]
mod tests;
