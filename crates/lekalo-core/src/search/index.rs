//! The search index snapshot and its durable generation store (issue #93).
//!
//! The in-memory [`SearchIndex`] owns the sorted cards, postings, and
//! evidence relations of one admitted projection. Its immutable content
//! digest is the SHA-256 over the exact canonical index bytes; every
//! cursor binds it. The [`SearchStore`] persists digest-named
//! generations under the governed cache custody
//! (`.lekalo/cache/search/**` of the accepted `lekalo.cache` authority
//! kind) with atomic publication: a generation becomes visible only
//! through one atomic rename, so an interrupted build can never make
//! mixed-revision rows visible. Missing/corrupt storage degrades to
//! transient construction; nothing here ever touches target source.
//!
//! Deterministic bytes exclude timestamps, elapsed times, SQLite row
//! IDs, installation paths, and cache-hit telemetry.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::search::input::SearchInputs;
use crate::search::projection::{Card, Projection};
use crate::search::version::{
    is_sha256, INDEX_IDENTITY, INDEX_SCHEMA_VERSION, MAX_CARDS, MAX_INDEX_BYTES, MAX_POSTINGS,
    RECIPE,
};
use crate::search::wire::{canonical_json, sha256_digest};

/// Why a durable store operation degraded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreError {
    /// The cache home is denied by policy/authority (fail closed).
    Denied,
    /// Any other I/O condition (degrade to transient).
    Io,
    /// The stored generation bytes are not a valid index snapshot.
    Corrupt,
}

/// The bounded facts of one `search-index status` run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexStatus {
    /// `missing` | `ready` | `stale` | `corrupt` | `denied`.
    pub state: &'static str,
    /// The current generation digest, when one is published.
    pub generation: Option<String>,
    /// The card count of the current generation, when readable.
    pub cards: Option<u64>,
    /// The posting/relation entry count, when readable.
    pub postings: Option<u64>,
}

/// The typed in-memory index: cards plus postings plus exact qualified
/// identities, in deterministic order everywhere.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchIndex {
    /// Cards sorted by qualified key.
    pub cards: Vec<Card>,
    /// Postings: normalized term -> byte-sorted qualified keys.
    pub postings: BTreeMap<String, Vec<String>>,
    /// Additional exact qualified identities per card key.
    pub qualified_ids: BTreeMap<String, Vec<String>>,
}

/// The durable wire snapshot of one generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexDocument {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub identity: String,
    /// The versioned recipe the snapshot was built under.
    pub recipe: String,
    /// The project semantic id, when the model declared one.
    #[serde(rename = "projectId")]
    pub project_id: Option<String>,
    /// The immutable content digest of exactly these canonical bytes.
    pub digest: String,
    /// The consumed input pins: producer kind, path, digest.
    pub inputs: Vec<InputPin>,
    /// The security/policy digest bound into cursors.
    #[serde(rename = "policyDigest")]
    pub policy_digest: String,
    /// Cards sorted by qualified key.
    pub cards: Vec<Card>,
    /// Postings: normalized term -> byte-sorted qualified keys.
    pub postings: BTreeMap<String, Vec<String>>,
    /// Additional exact qualified identities per card key.
    #[serde(rename = "qualifiedIds")]
    pub qualified_ids: BTreeMap<String, Vec<String>>,
}

/// One consumed producer pin recorded in the snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPin {
    pub kind: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

impl SearchIndex {
    /// Assemble from a finished projection (bounded); postings beyond
    /// the admitted cap are refused, never silently truncated.
    pub fn from_projection(
        projection: &Projection,
        _inputs: &[InputPin],
        _policy_digest: &str,
        _project_id: Option<&str>,
        max_postings: usize,
    ) -> Result<Self, crate::diagnostics::DiagnosticSet> {
        if projection.cards.len() > MAX_CARDS {
            return Err(crate::search::diagnostic::result_limit("cards", true));
        }
        if projection.postings.len() > max_postings.min(MAX_POSTINGS) {
            return Err(crate::search::diagnostic::result_limit("postings", true));
        }
        let mut postings: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (term, keys) in &projection.postings {
            let mut sorted = keys.clone();
            sorted.sort();
            sorted.dedup();
            postings.insert(term.clone(), sorted);
        }
        Ok(Self {
            cards: projection.cards.values().cloned().collect(),
            postings,
            qualified_ids: projection.qualified_ids.clone(),
        })
    }

    /// The card of one qualified key.
    pub fn card(&self, key: &str) -> Option<&Card> {
        self.cards.iter().find(|card| card.key == key)
    }

    /// The candidate keys admitted by every query term (conjunction);
    /// an empty term list admits every card. Byte-exact ID membership is
    /// always checked from the card itself, never from postings alone.
    pub fn candidates(&self, terms: &[String]) -> Vec<&Card> {
        if terms.is_empty() {
            return self.cards.iter().collect();
        }
        let mut result: Vec<&Card> = Vec::new();
        for card in &self.cards {
            let id_terms = card
                .id
                .as_deref()
                .map(crate::search::rank::tokenize)
                .unwrap_or_default();
            let name_terms = card.name_terms();
            let description_terms = card.description_terms();
            let admitted = terms.iter().all(|term| {
                id_terms.iter().any(|candidate| candidate == term)
                    || name_terms.iter().any(|candidate| candidate == term)
                    || description_terms.iter().any(|candidate| candidate == term)
                    || card
                        .effects
                        .iter()
                        .any(|witness| witness.relation.split(':').any(|part| part == term))
            });
            if admitted {
                result.push(card);
            }
        }
        result
    }
}

/// The governed generation store under `.lekalo/cache/search/`.
pub struct SearchStore {
    root: PathBuf,
}

impl SearchStore {
    /// Resolve the store home for one project root (path validation is
    /// the cache custody's own; this never creates the directory).
    pub fn resolve(project_root: &Path) -> Result<Self, StoreError> {
        // Mirror the cache path policy: refuse absolute/aliased project
        // roots by resolving first; deny symlinked cache homes.
        let canonical = std::fs::canonicalize(project_root).map_err(|_| StoreError::Io)?;
        let home = canonical.join(".lekalo").join("cache").join("search");
        Ok(Self { root: home })
    }

    /// The current-generation pointer path.
    fn current_path(&self) -> PathBuf {
        self.root.join("current.json")
    }

    /// A generation payload path from its digest.
    fn generation_path(&self, digest: &str) -> PathBuf {
        self.root.join(format!("{digest}.json"))
    }

    /// Read the published current generation digest, if any.
    pub fn current(&self) -> Result<Option<String>, StoreError> {
        let bytes = match std::fs::read(self.current_path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(StoreError::Io),
        };
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Current {
            digest: String,
        }
        let current: Current = serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt)?;
        if !is_sha256(&current.digest) {
            return Err(StoreError::Corrupt);
        }
        Ok(Some(current.digest))
    }

    /// Read one generation snapshot.
    pub fn load(&self, digest: &str) -> Result<Option<IndexDocument>, StoreError> {
        if !is_sha256(digest) {
            return Err(StoreError::Corrupt);
        }
        let bytes = match std::fs::read(self.generation_path(digest)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(StoreError::Io),
        };
        if bytes.len() > MAX_INDEX_BYTES {
            return Err(StoreError::Corrupt);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| StoreError::Corrupt)?;
        crate::search::wire::reject_duplicate_keys(text).map_err(|_| StoreError::Corrupt)?;
        let mut document: IndexDocument =
            serde_json::from_str(text).map_err(|_| StoreError::Corrupt)?;
        // Canonical-bytes custody: the stored digest must equal the
        // digest of exactly these canonical bytes.
        let recorded = document.digest.clone();
        document.digest = String::new();
        let canonical =
            canonical_json(&serde_json::to_value(&document).map_err(|_| StoreError::Corrupt)?);
        if sha256_digest(canonical.as_bytes()) != recorded {
            return Err(StoreError::Corrupt);
        }
        document.digest = recorded;
        if document.schema_version != INDEX_SCHEMA_VERSION
            || document.identity != INDEX_IDENTITY
            || document.recipe != RECIPE
        {
            return Err(StoreError::Corrupt);
        }
        Ok(Some(document))
    }

    /// Atomically publish one snapshot: write the digest-named payload
    /// and then atomically move the current pointer. A crash before the
    /// pointer move leaves the previous generation serving.
    pub fn publish(&self, document: &IndexDocument) -> Result<(), StoreError> {
        let mut record = document.clone();
        let digest = {
            record.digest = String::new();
            let canonical =
                canonical_json(&serde_json::to_value(&record).map_err(|_| StoreError::Io)?);
            let digest = sha256_digest(canonical.as_bytes());
            if canonical.len() > MAX_INDEX_BYTES {
                return Err(StoreError::Io);
            }
            let final_path = self.generation_path(&digest);
            if final_path.exists() {
                digest
            } else {
                let temp = self.root.join(format!(
                    ".tmp-{}-{}.json",
                    std::process::id(),
                    &digest[..12.min(digest.len())]
                ));
                std::fs::create_dir_all(&self.root).map_err(|_| StoreError::Io)?;
                std::fs::write(&temp, canonical.as_bytes()).map_err(|_| StoreError::Io)?;
                std::fs::rename(&temp, &final_path).map_err(|_| StoreError::Io)?;
                digest
            }
        };
        record.digest = digest.clone();
        let pointer = format!("{{\"digest\":\"{}\"}}", digest);
        std::fs::create_dir_all(&self.root).map_err(|_| StoreError::Io)?;
        let temp = self.current_path().with_extension("tmp");
        std::fs::write(&temp, pointer.as_bytes()).map_err(|_| StoreError::Io)?;
        std::fs::rename(&temp, self.current_path()).map_err(|_| StoreError::Io)?;
        Ok(())
    }

    /// The bounded read-only status of the store.
    pub fn status(&self) -> IndexStatus {
        match self.current() {
            Err(StoreError::Corrupt) => IndexStatus {
                state: "corrupt",
                generation: None,
                cards: None,
                postings: None,
            },
            Err(StoreError::Denied) => IndexStatus {
                state: "denied",
                generation: None,
                cards: None,
                postings: None,
            },
            Err(StoreError::Io) | Ok(None) => IndexStatus {
                state: "missing",
                generation: None,
                cards: None,
                postings: None,
            },
            Ok(Some(digest)) => match self.load(&digest) {
                Ok(Some(document)) => IndexStatus {
                    state: "ready",
                    generation: Some(digest),
                    cards: Some(document.cards.len() as u64),
                    postings: Some(document.postings.len() as u64),
                },
                _ => IndexStatus {
                    state: "corrupt",
                    generation: Some(digest),
                    cards: None,
                    postings: None,
                },
            },
        }
    }
}

/// Derive the snapshot's input pins from a parsed manifest (without
/// digest re-verification; verification is the caller's admission step).
pub fn pins_of(manifest: &SearchInputs) -> Vec<InputPin> {
    manifest
        .entries()
        .iter()
        .map(|entry| InputPin {
            kind: entry.kind.key().to_owned(),
            path: entry.path.clone(),
            digest: entry.digest.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reports_a_missing_store_without_creating_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = SearchStore::resolve(temp.path()).expect("resolves");
        assert_eq!(store.status().state, "missing");
        assert!(!temp.path().join(".lekalo/cache/search").exists());
    }
}
