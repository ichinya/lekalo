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

use crate::adapter_package::discovery::{discover, Custody, DiscoveryCandidate, DiscoverySource};
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
/// discover the recorded package directory. The explicit project root
/// (the CLI's `--project`, threaded through discovery) scopes the
/// record lookup; `None` falls back to the CWD-derived root (issue #32
/// fix round 2, cline F-7 / devin F-10).
pub fn resolve_record(
    record_file: &str,
    prefix: &str,
    coordinate: &str,
    project_root: Option<&PathBuf>,
) -> Result<Vec<DiscoveryCandidate>, PackageFailure> {
    let unavailable = || PackageFailure::SourceUnavailable {
        source: bounded(coordinate),
    };
    let root = match project_root {
        Some(root) => crate::project_fs::Fs::find_root(root)
            .map_err(|_| PackageFailure::SourceUnavailable {
                source: "record-root".to_owned(),
            })?
            .ok_or_else(|| PackageFailure::SourceUnavailable {
                source: "record-root".to_owned(),
            }),
        None => package_root(),
    }?;
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
    // Record custody: the bytes arrived through the evidence record, no
    // matter what the manifest's self-declared source.kind claims (fix
    // round 4, cline F-NEW-1) — trust is assigned from custody, so a
    // record-resolved package is community/quarantined even when it
    // claims a project path.
    Ok(discover(&DiscoverySource::Path(full), project_root)?
        .into_iter()
        .map(|mut candidate| {
            candidate.custody = Custody::Record;
            candidate
        })
        .collect())
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

    /// Regression (fix round 2, cline F-7 / devin F-10): the explicit
    /// project root (the CLI's `--project`) scopes the record lookup —
    /// records resolve from the selected project's evidence tree, never
    /// the CWD's project.
    #[test]
    fn resolve_record_uses_the_provided_project_root() {
        let root = std::env::temp_dir().join(format!("lekalo-records-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let evidence = root.join(".lekalo/adapters/evidence");
        std::fs::create_dir_all(&evidence).expect("evidence dir");
        // The project marker find_root anchors on.
        std::fs::create_dir_all(root.join("lekalo")).expect("lekalo dir");
        std::fs::write(root.join("lekalo/project.yaml"), "project: records-test\n")
            .expect("project marker");
        // The recorded package: a directory carrying a valid manifest whose
        // source digest equals the record's custody digest.
        let package = root.join("pkg");
        std::fs::create_dir_all(&package).expect("package dir");
        let source_digest = format!("sha256:{}", "22".repeat(32));
        let manifest = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "record-adapter", "name": "R", "version": "1.0.0" },
            "publisher": { "id": "p", "trustAnchor": "none" },
            "source": { "kind": "path", "coordinate": "path:pkg", "digest": source_digest },
            "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": { "protocolVersions": [crate::target_protocol::version::VERSION], "irVersions": [crate::ir::version::VERSION], "extensions": [] },
            "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
            "executable": { "runtime": { "kind": "node", "minVersion": "18.0.0" }, "entry": "a.mjs", "argvPreview": ["node", "a.mjs"], "assets": [] },
            "platforms": ["any"],
            "integrity": { "packageDigest": format!("sha256:{}", "33".repeat(32)), "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "44".repeat(32)), "bytes": 3 } ], "signaturePolicy": "unsigned", "signature": null },
            "permissions": {
                "filesystem": { "readScopes": [], "writeScopes": [] },
                "network": { "mode": "denied", "destinations": [] },
                "environment": { "allowlist": [] },
                "processes": { "children": "denied" },
                "secrets": { "handles": [] }
            },
            "hooks": [],
            "conformance": { "reportDigest": format!("sha256:{}", "55".repeat(32)), "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "status": "active",
            "revocation": null
        });
        std::fs::write(
            package.join("adapter.manifest.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest serializes"),
        )
        .expect("manifest written");
        std::fs::write(
            evidence.join("releases.json"),
            format!(
                "{{\"schemaVersion\":\"lekalo/adapter-records/v0.3.2\",\"records\":[{{\"coordinate\":\"release:ch/pkg\",\"path\":\"pkg\",\"digest\":\"{source_digest}\"}}]}}"
            ),
        )
        .expect("record written");

        // The explicit root wins: the record resolves from it even though
        // the test process's CWD is the workspace, not `root`.
        let candidates = resolve_record(
            ".lekalo/adapters/evidence/releases.json",
            "release",
            "ch/pkg",
            Some(&root),
        )
        .expect("the recorded package resolves under the explicit root");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].manifest.adapter_id(), "record-adapter");
        assert_eq!(candidates[0].package_root, Some(package.clone()));

        // A root without the record honestly refuses: the coordinate is
        // not resolved from somewhere else.
        let empty =
            std::env::temp_dir().join(format!("lekalo-records-empty-{}", std::process::id()));
        std::fs::create_dir_all(empty.join("lekalo")).expect("empty root");
        std::fs::write(
            empty.join("lekalo/project.yaml"),
            "project: records-empty\n",
        )
        .expect("project marker");
        let error = resolve_record(
            ".lekalo/adapters/evidence/releases.json",
            "release",
            "ch/pkg",
            Some(&empty),
        )
        .expect_err("no record under the empty root");
        assert!(matches!(error, PackageFailure::SourceUnavailable { .. }));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&empty);
    }

    /// Regression (fix round 4, cline F-NEW-1): a release record whose
    /// manifest lies about its `source.kind` resolves with **record**
    /// custody — the community tier, quarantined at install — never the
    /// local-development tier the manifest claims.
    #[test]
    fn a_record_manifest_lying_about_source_kind_stays_record_custody() {
        use crate::adapter_package::discovery::Custody;
        use crate::adapter_package::trust;

        let root = std::env::temp_dir().join(format!("lekalo-records-liar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("lekalo")).expect("lekalo dir");
        std::fs::write(root.join("lekalo/project.yaml"), "project: records-liar\n")
            .expect("project marker");
        let evidence = root.join(".lekalo/adapters/evidence");
        std::fs::create_dir_all(&evidence).expect("evidence dir");

        let package = root.join("liar-package");
        std::fs::create_dir_all(&package).expect("package dir");
        // The lie: the manifest declares an explicit project path even
        // though the operator resolved it from the release record.
        let manifest = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "liar-adapter", "name": "L", "version": "1.0.0" },
            "publisher": { "id": "p", "trustAnchor": "none" },
            "source": { "kind": "path", "coordinate": "path:liar-package", "digest": format!("sha256:{}", "22".repeat(32)) },
            "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": { "protocolVersions": [crate::target_protocol::version::VERSION], "irVersions": [crate::ir::version::VERSION], "extensions": [] },
            "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
            "executable": { "runtime": { "kind": "node", "minVersion": "18.0.0" }, "entry": "a.mjs", "argvPreview": ["node", "a.mjs"], "assets": [] },
            "platforms": ["any"],
            "integrity": { "packageDigest": format!("sha256:{}", "33".repeat(32)), "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "44".repeat(32)), "bytes": 3 } ], "signaturePolicy": "unsigned", "signature": null },
            "permissions": {
                "filesystem": { "readScopes": [], "writeScopes": [] },
                "network": { "mode": "denied", "destinations": [] },
                "environment": { "allowlist": [] },
                "processes": { "children": "denied" },
                "secrets": { "handles": [] }
            },
            "hooks": [],
            "conformance": { "reportDigest": format!("sha256:{}", "55".repeat(32)), "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "status": "active",
            "revocation": null
        });
        std::fs::write(
            package.join("adapter.manifest.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest serializes"),
        )
        .expect("manifest written");
        std::fs::write(
            evidence.join("releases.json"),
            format!(
                "{{\"schemaVersion\":\"lekalo/adapter-records/v0.3.2\",\"records\":[{{\"coordinate\":\"release:ch/liar\",\"path\":\"liar-package\",\"digest\":\"sha256:{}\"}}]}}",
                "22".repeat(32)
            ),
        )
        .expect("record written");

        let candidates = resolve_record(
            ".lekalo/adapters/evidence/releases.json",
            "release",
            "ch/liar",
            Some(&root),
        )
        .expect("the record resolves");
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].custody,
            Custody::Record,
            "custody is the record, not the manifest's claimed kind"
        );
        assert_eq!(
            trust::assign(&candidates[0]),
            crate::adapter_package::TrustLevel::Community,
            "the lying manifest cannot reach local-development"
        );
        assert!(crate::adapter_package::TrustLevel::Community.quarantined_at_install());
        let _ = std::fs::remove_dir_all(&root);
    }
}
