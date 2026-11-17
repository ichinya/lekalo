//! Data-only requirements provider. File access remains a bounded host capability.
use super::{version, ProviderDecl};
use crate::project_fs::{EntryType, Fs, FsErrorKind};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) enum Loaded {
    Resolved(Snapshot),
    Absent,
    Invalid(String),
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogEntry {
    pub id: String,
    pub digest: String,
    pub origin: Origin,
    pub change: Option<String>,
    pub path: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Origin {
    Accepted,
    Change,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProviderStatus {
    Ok,
    Absent,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConflictRow {
    pub capability: String,
    pub id: String,
    pub detail: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub source: String,
    pub root: String,
    pub status: ProviderStatus,
    pub entries: Vec<CatalogEntry>,
    pub conflicts: Vec<ConflictRow>,
    pub renames: BTreeMap<String, String>,
    pub removals: BTreeSet<String>,
}
pub(crate) fn absent(decl: &ProviderDecl) -> Snapshot {
    Snapshot {
        source: decl.source.clone(),
        root: decl.root.clone(),
        status: ProviderStatus::Absent,
        entries: Vec::new(),
        conflicts: Vec::new(),
        renames: BTreeMap::new(),
        removals: BTreeSet::new(),
    }
}
fn inside(path: &str, root: &str) -> bool {
    crate::project_fs::path_violation(path).is_none()
        && (path == root || path.strip_prefix(root).is_some_and(|s| s.starts_with('/')))
}
fn admit(s: &Snapshot, decl: &ProviderDecl, reads: &BTreeMap<String, Value>) -> bool {
    s.source == decl.source
        && s.root == decl.root
        && matches!(s.status, ProviderStatus::Ok)
        && s.entries.len() <= version::MAX_REQUIREMENTS
        && s.conflicts.len() <= version::MAX_REQUIREMENTS
        && s.renames.len() <= version::MAX_REQUIREMENTS
        && s.removals.len() <= version::MAX_REQUIREMENTS
        && s.entries.windows(2).all(|w| w[0].id < w[1].id)
        && s.entries.iter().all(|e| {
            super::is_requirement_id(&e.id)
                && crate::target_protocol::wire::is_sha256_digest(&e.digest)
                && inside(&e.path, &decl.root)
                && reads
                    .get(&format!("file:{}", e.path))
                    .is_some_and(|v| v["content"].is_string())
                && match (e.origin, &e.change) {
                    (Origin::Accepted, None) => true,
                    (Origin::Change, Some(c)) => crate::runtime::config::token(c),
                    _ => false,
                }
        })
        && s.conflicts.iter().all(|c| {
            super::is_requirement_id(&c.id)
                && c.id.starts_with(&format!("{}.REQ-", c.capability))
                && crate::runtime::config::token(&c.detail)
        })
        && s.renames
            .iter()
            .all(|(a, b)| super::is_requirement_id(a) && super::is_requirement_id(b))
        && s.removals.iter().all(|id| super::is_requirement_id(id))
}
fn failure(error: FsErrorKind) -> Value {
    json!({"error":match error {FsErrorKind::NotFound=>"not-found",FsErrorKind::Limit{..}=>"limit",_=>"io"}})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRequest {
    kind: String,
    path: String,
}
#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "lowercase", deny_unknown_fields)]
enum Response {
    Read {
        schema: String,
        requests: Vec<ReadRequest>,
    },
    Complete {
        schema: String,
        snapshot: Snapshot,
    },
    Absent {
        schema: String,
    },
    Invalid {
        schema: String,
        detail: String,
    },
}
fn resolve(root: &Path, fs: &Fs, decl: &ProviderDecl) -> Result<Loaded, String> {
    const API: &str = "lekalo/requirements-provider/v1";
    let provider =
        crate::runtime::services::one(root, "requirements").map_err(|_| "provider-unavailable")?;
    let mut reads = BTreeMap::<String, Value>::new();
    let mut total = 0;
    for _ in 0..1024 {
        let value = provider
            .invoke(
                "requirements.snapshot",
                json!({"schema":API,"source":decl.source,"root":decl.root,"reads":reads}),
            )
            .map_err(|_| "provider-execution")?;
        let response: Response = serde_json::from_value(value).map_err(|_| "provider-response")?;
        match response {
            Response::Absent { schema } if schema == API => return Ok(Loaded::Absent),
            Response::Invalid { schema, detail }
                if schema == API && crate::runtime::config::token(&detail) =>
            {
                return Ok(Loaded::Invalid(detail))
            }
            Response::Complete { schema, snapshot }
                if schema == API && admit(&snapshot, decl, &reads) =>
            {
                return Ok(Loaded::Resolved(snapshot))
            }
            Response::Read { schema, requests }
                if schema == API && !requests.is_empty() && requests.len() <= 256 =>
            {
                for request in requests {
                    if !inside(&request.path, &decl.root) || reads.len() >= 4096 {
                        return Err("provider-read-scope".into());
                    }
                    let key = format!("{}:{}", request.kind, request.path);
                    if reads.contains_key(&key) {
                        return Err("provider-read-repeat".into());
                    }
                    let value = match request.kind.as_str() {
                        "entries" => match fs.entries_bounded(&request.path, 10_000) {
                            Ok(entries) => {
                                json!({"entries":entries.into_iter().map(|(n,k)|(n,if k==EntryType::Directory{"directory"}else{"other"})).collect::<Vec<_>>()})
                            }
                            Err(e) => failure(e),
                        },
                        "file" => match fs.read_logical(&request.path, version::MAX_SPEC_BYTES) {
                            Ok(Some(bytes)) => match String::from_utf8(bytes) {
                                Ok(text) => json!({"content":text}),
                                Err(_) => json!({"error":"io"}),
                            },
                            Ok(None) => json!({"content":null}),
                            Err(e) => failure(e),
                        },
                        _ => return Err("provider-read-kind".into()),
                    };
                    total += serde_json::to_vec(&value)
                        .map_err(|_| "provider-read-size")?
                        .len();
                    if total > 4 * 1024 * 1024 {
                        return Err("provider-read-limit".into());
                    }
                    reads.insert(key, value);
                }
            }
            _ => return Err("provider-response".into()),
        }
    }
    Err("provider-read-rounds".into())
}
pub(crate) fn load_snapshot(root: &Path, fs: &Fs, decl: &ProviderDecl) -> Loaded {
    resolve(root, fs, decl).unwrap_or_else(Loaded::Invalid)
}
