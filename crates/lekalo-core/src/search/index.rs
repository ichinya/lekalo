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
    INDEX_IDENTITY, INDEX_SCHEMA_VERSION, MAX_CARDS, MAX_INDEX_BYTES, MAX_POSTINGS, RECIPE,
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
    /// The validated current generation document, when readable. The
    /// freshness oracle re-verifies the recorded input pins against
    /// this, never a re-guessed manifest location.
    pub document: Option<IndexDocument>,
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InputPin {
    pub kind: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// The exact contract identity the producing manifest pinned (when
    /// it did); a freshness re-admission replays the identical
    /// admission semantics, not a loosened guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

impl SearchIndex {
    /// Assemble from a finished projection (bounded); postings beyond
    /// the admitted cap are refused, never silently truncated. The cap
    /// counts actual posting entries (term -> key pairs), not distinct
    /// terms, so the budget is the real work bound.
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
        let entries: usize = projection.postings.values().map(Vec::len).sum();
        if entries > max_postings.min(MAX_POSTINGS) {
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
    ///
    /// Custody: when the store home already exists, it must resolve
    /// (junctions and symlinks included) inside the canonical project
    /// root. A relocated or junctioned cache home is `Denied`, never
    /// silently followed — otherwise a status read would happily serve
    /// generations stored outside the project.
    pub fn resolve(project_root: &Path) -> Result<Self, StoreError> {
        // Mirror the cache path policy: refuse absolute/aliased project
        // roots by resolving first; deny symlinked cache homes.
        let canonical = std::fs::canonicalize(project_root).map_err(|_| StoreError::Io)?;
        let home = canonical.join(".lekalo").join("cache").join("search");
        if let Ok(resolved) = std::fs::canonicalize(&home) {
            if !resolved.starts_with(&canonical) {
                return Err(StoreError::Denied);
            }
        }
        Ok(Self { root: home })
    }

    /// The current-generation pointer path.
    fn current_path(&self) -> PathBuf {
        self.root.join("current.json")
    }

    /// A generation payload path from its digest: exactly the 64 hex
    /// characters, so every path segment satisfies the project path
    /// policy (segments at most 64 bytes).
    fn generation_path(&self, digest: &str) -> PathBuf {
        self.root.join(digest)
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
        // The pointer stores the bare hex generation name (the path
        // policy limits path segments); consumers spell `sha256:`.
        if current.digest.len() != 64
            || !current
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(StoreError::Corrupt);
        }
        Ok(Some(current.digest))
    }

    /// Read one generation snapshot.
    ///
    /// Custody: the requested 64-hex name, the document's own recorded
    /// digest, and the canonical-bytes digest of exactly the stored
    /// payload must all agree, and the payload must satisfy the closed
    /// structural invariants (every posting and qualified identity
    /// resolves to a carried card). An aliased copy under a foreign
    /// digest name, a forged posting, or a re-hashed payload is
    /// `Corrupt`, never served.
    pub fn load(&self, digest: &str) -> Result<Option<IndexDocument>, StoreError> {
        // The generation name is exactly 64 lowercase hex characters.
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
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
        // digest of exactly these canonical bytes...
        let Some(recorded) = document.digest.strip_prefix("sha256:") else {
            return Err(StoreError::Corrupt);
        };
        let recorded = recorded.to_owned();
        document.digest = String::new();
        let canonical =
            canonical_json(&serde_json::to_value(&document).map_err(|_| StoreError::Corrupt)?);
        if sha256_digest(canonical.as_bytes()) != recorded {
            return Err(StoreError::Corrupt);
        }
        document.digest = format!("sha256:{recorded}");
        // ...and the recorded digest must be the name it was stored
        // under: a valid payload copied beneath a foreign digest name is
        // an alias, not a generation.
        if recorded != digest {
            return Err(StoreError::Corrupt);
        }
        if document.schema_version != INDEX_SCHEMA_VERSION
            || document.identity != INDEX_IDENTITY
            || document.recipe != RECIPE
        {
            return Err(StoreError::Corrupt);
        }
        validate_document(&document).map_err(|_| StoreError::Corrupt)?;
        Ok(Some(document))
    }

    /// Atomically publish one snapshot: write the digest-named payload
    /// and then atomically move the current pointer. A crash before the
    /// pointer move leaves the previous generation serving.
    ///
    /// An existing payload under the target name is served only when it
    /// validates as exactly this generation; poisoned or aliased bytes
    /// are atomically replaced (a rebuild repairs corrupt custody, it
    /// never reports success over it).
    pub fn publish(&self, document: &IndexDocument) -> Result<(), StoreError> {
        let mut record = document.clone();
        // The content digest covers exactly the canonical bytes of the
        // snapshot with an empty digest field; the stored payload then
        // carries the digest itself.
        record.digest = String::new();
        let canonical = canonical_json(&serde_json::to_value(&record).map_err(|_| StoreError::Io)?);
        if canonical.len() > MAX_INDEX_BYTES {
            return Err(StoreError::Io);
        }
        let digest = sha256_digest(canonical.as_bytes());
        record.digest = format!("sha256:{digest}");
        let payload = canonical_json(&serde_json::to_value(&record).map_err(|_| StoreError::Io)?);
        let final_path = self.generation_path(&digest);
        let existing_ok = self.load(&digest).is_ok_and(|loaded| loaded.is_some());
        if !existing_ok {
            let temp = self.root.join(format!(
                "tmp-{}-{}",
                std::process::id(),
                &digest[..12.min(digest.len())]
            ));
            std::fs::create_dir_all(&self.root).map_err(|_| StoreError::Io)?;
            std::fs::write(&temp, payload.as_bytes()).map_err(|_| StoreError::Io)?;
            std::fs::rename(&temp, &final_path).map_err(|_| StoreError::Io)?;
        }
        let pointer = format!("{{\"digest\":\"{digest}\"}}");
        std::fs::create_dir_all(&self.root).map_err(|_| StoreError::Io)?;
        let temp = self.current_path().with_extension("tmp");
        std::fs::write(&temp, pointer.as_bytes()).map_err(|_| StoreError::Io)?;
        std::fs::rename(&temp, self.current_path()).map_err(|_| StoreError::Io)?;
        Ok(())
    }

    /// The bounded read-only status of the store. A generation is
    /// `ready` only when its full payload validates; the current
    /// generation document rides along for the freshness oracle.
    pub fn status(&self) -> IndexStatus {
        match self.current() {
            Err(StoreError::Corrupt) => IndexStatus {
                state: "corrupt",
                generation: None,
                cards: None,
                postings: None,
                document: None,
            },
            Err(StoreError::Denied) => IndexStatus {
                state: "denied",
                generation: None,
                cards: None,
                postings: None,
                document: None,
            },
            Err(StoreError::Io) | Ok(None) => IndexStatus {
                state: "missing",
                generation: None,
                cards: None,
                postings: None,
                document: None,
            },
            Ok(Some(digest)) => match self.load(&digest) {
                Ok(Some(document)) => IndexStatus {
                    state: "ready",
                    cards: Some(document.cards.len() as u64),
                    postings: Some(document.postings.len() as u64),
                    document: Some(document),
                    generation: Some(digest),
                },
                _ => IndexStatus {
                    state: "corrupt",
                    generation: Some(digest),
                    cards: None,
                    postings: None,
                    document: None,
                },
            },
        }
    }
}

/// The closed structural invariants of one generation document: every
/// posting entry and qualified identity must resolve to a carried
/// card, posting lists stay byte-sorted and duplicate-free, and the
/// bounded card count holds. A forged posting that names no card is
/// corrupt custody, never silently served content.
fn validate_document(document: &IndexDocument) -> Result<(), &'static str> {
    if document.cards.len() > MAX_CARDS {
        return Err("cards");
    }
    let mut keys: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for card in &document.cards {
        if card.key.is_empty() || card.key.len() > 256 {
            return Err("card-key");
        }
        if !keys.insert(card.key.as_str()) {
            return Err("duplicate-card");
        }
    }
    let mut entries = 0usize;
    for (term, list) in &document.postings {
        if term.is_empty() {
            return Err("posting-term");
        }
        entries += list.len();
        if list.iter().any(|key| !keys.contains(key.as_str())) {
            return Err("posting-key");
        }
        let mut sorted = list.clone();
        sorted.sort();
        sorted.dedup();
        if &sorted != list {
            return Err("posting-order");
        }
    }
    if entries > MAX_POSTINGS {
        return Err("posting-entries");
    }
    for (key, list) in &document.qualified_ids {
        if !keys.contains(key.as_str()) {
            return Err("qualified-key");
        }
        let mut sorted = list.clone();
        sorted.sort();
        sorted.dedup();
        if &sorted != list {
            return Err("qualified-order");
        }
    }
    Ok(())
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
            identity: entry.identity.clone(),
        })
        .collect()
}

/// The exact content digest of one snapshot: SHA-256 over the canonical
/// JSON of the document with an empty digest field.
pub fn digest_of(document: &IndexDocument) -> String {
    let mut record = document.clone();
    record.digest = String::new();
    match serde_json::to_value(&record) {
        Ok(value) => sha256_digest(canonical_json(&value).as_bytes()),
        Err(_) => String::new(),
    }
}

/// SHA-256 over the exact bytes, spelled bare lowercase hex.
pub fn sha256_of(bytes: &[u8]) -> String {
    sha256_digest(bytes)
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
