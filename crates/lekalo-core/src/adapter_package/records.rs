//! Local release/registry record readers (issue #32 fix round 1,
//! cline F-11 / devin residual).
//!
//! v1 never fetches: a release or registry source resolves from an
//! explicit local record the operator pre-staged, which is what makes
//! `--offline` exact. Records are closed JSON documents under
//! `.lekalo/adapters/evidence/`:
//!
//! - `releases.json`: `{"schemaVersion": "lekalo/adapter-releases/v0.3.2",
//!   "releases": [{"coordinate": "release:<channel>/<id>", "path":
//!   "<package dir>", "digest": "sha256:…"}]}`
//! - `registry.json`: the same shape with coordinate `registry:<…>`.
//!
//! A record resolves when its coordinate matches exactly; the recorded
//! `path` must point at a directory carrying `adapter.manifest.json`,
//! and the manifest's source digest must equal the recorded digest. The
//! candidate is then discovered like any explicit path supply. A missing
//! record is the honest `adapter.source-unavailable`; a malformed record
//! refuses the same way (never silently skipped).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::adapter_package::discovery::{discover, DiscoveryCandidate, DiscoverySource};
use crate::adapter_package::manifest::ManifestDocument;
use crate::adapter_package::types::PackageFailure;

const RECORD_SCHEMA: &str = "lekalo/adapter-records/v0.3.2";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordWire {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    #[serde(default)]
    records: Vec<EntryWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    coordinate: String,
    path: String,
    digest: String,
}

/// Parse and schema-check one record document.
fn parse_records(bytes: &[u8]) -> Result<Vec<EntryWire>, PackageFailure> {
    let unavailable = || PackageFailure::SourceUnavailable {
        source: "record".to_owned(),
    };
    let record: RecordWire = serde_json::from_slice(bytes).map_err(|_| unavailable())?;
    if record.schema_version != RECORD_SCHEMA {
        return Err(unavailable());
    }
    Ok(record.records)
}

/// Resolve a release/registry coordinate from the local record file and
/// discover the recorded package directory.
pub fn resolve_record(
    record_file: &str,
    prefix: &str,
    coordinate: &str,
) -> Result<Vec<DiscoveryCandidate>, PackageFailure> {
    let unavailable = || PackageFailure::SourceUnavailable {
        source: bounded(coordinate),
    };
    let root = package_root()?;
    let path = root.join(record_file.replace('/', std::path::MAIN_SEPARATOR_STR));
    let bytes = std::fs::read(&path).map_err(|_| unavailable())?;
    let records = parse_records(&bytes)?;
    let wanted = format!("{prefix}:{coordinate}");
    let entry = records
        .iter()
        .find(|entry| entry.coordinate == wanted)
        .ok_or_else(unavailable)?;
    if entry.path.is_empty() || crate::project_fs::path_violation(&entry.path).is_some() {
        return Err(unavailable());
    }
    // Resolve relative record paths against the project root; absolute
    // host paths are refused as private data.
    let full = if Path::new(&entry.path).is_absolute() {
        return Err(unavailable());
    } else {
        root.join(entry.path.replace('/', std::path::MAIN_SEPARATOR_STR))
    };
    if !full.is_dir() {
        return Err(unavailable());
    }
    // The recorded digest must equal the manifest's source digest: the
    // record is the operator's custody claim over the snapshot bytes.
    let manifest_path = full.join("adapter.manifest.json");
    let manifest_bytes = std::fs::read(&manifest_path).map_err(|_| unavailable())?;
    let manifest = ManifestDocument::from_bytes(&manifest_bytes)?;
    if manifest.source_digest().as_str() != entry.digest {
        return Err(unavailable());
    }
    discover(&DiscoverySource::Path(full))
}

fn package_root() -> Result<PathBuf, PackageFailure> {
    let cwd = std::env::current_dir().map_err(|_| PackageFailure::SourceUnavailable {
        source: "record-root".to_owned(),
    })?;
    crate::project_fs::Fs::find_root(&cwd)
        .map_err(|_| PackageFailure::SourceUnavailable {
            source: "record-root".to_owned(),
        })?
        .ok_or_else(|| PackageFailure::SourceUnavailable {
            source: "record-root".to_owned(),
        })
}

fn bounded(text: &str) -> String {
    let mut token: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '/' || c == ':' || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    token.truncate(64);
    if token.is_empty() {
        token.push_str("unknown");
    }
    token
}

/// The project root the record files resolve against (test seam).
pub fn evidence_root(root: &Path) -> PathBuf {
    root.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A malformed or wrong-version record refuses rather than silently
    /// resolving nothing (issue #32 fix round 1, cline F-11).
    #[test]
    fn malformed_records_refuse() {
        assert!(parse_records(b"not json").is_err());
        assert!(parse_records(
            br#"{"schemaVersion":"lekalo/adapter-records/v9.9.9","records":[]}"#
        )
        .is_err());
        assert!(parse_records(
            br#"{"schemaVersion":"lekalo/adapter-records/v0.3.2","records":[]}"#
        )
        .is_ok());
    }
}
