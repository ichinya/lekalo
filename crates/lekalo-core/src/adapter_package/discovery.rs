//! Adapter package discovery sources (issue #32).
//!
//! Four closed sources produce [`DiscoveryCandidate`]s, in the exact
//! vocabulary the manifest's `source.kind` declares:
//!
//! - `path` — an explicit manifest file or package directory. A
//!   manifest-bearing package directory is verified whole; a bare
//!   manifest file without its package bytes is refused at the
//!   integrity gate, not here (discovery is not verification).
//! - `path-exec` — explicit enumeration of the `PATH` environment
//!   entries (never a shell) for `lekalo-target-*` executables or one
//!   explicit name. A PATH hit is never trusted: it is a `community`
//!   candidate.
//! - `release` — a local release record under
//!   `.lekalo/adapters/evidence/releases.json`. Remote fetch does not
//!   exist in v1 (a declared provider seam), so an unrecorded release is
//!   honestly `adapter.source-unavailable`.
//! - `registry` — a cached AIFHub catalog snapshot record under
//!   `.lekalo/adapters/evidence/registry.json`, offline-faithful for the
//!   same reason.
//!
//! The implicit descriptor closes the gate: a bare `-- PROGRAM` argv
//! synthesizes an unsigned `local-development` manifest document from
//! the launched entry, so every adapter execution carries an effective
//! manifest check without breaking the shipped flows.
//!
//! Discovery is never install and never trust: enumeration only finds
//! candidates; the trust gate and the install plan decide everything
//! else.

use std::path::{Path, PathBuf};

use super::manifest::ManifestDocument;
use super::types::PackageFailure;

/// How many PATH entries may be enumerated before the walk stops (the
/// accepted workspace bound pattern).
const MAX_PATH_ENTRIES: usize = 256;

/// How many candidates one enumeration may produce before it stops.
const MAX_CANDIDATES: usize = 64;

/// One discovered package candidate: the verified manifest plus the
/// package root the manifest describes.
#[derive(Clone, Debug)]
pub struct DiscoveryCandidate {
    /// The decoded, validated manifest.
    pub manifest: ManifestDocument,
    /// The package root directory the manifest's file set is relative
    /// to. `None` only for synthesized implicit descriptors, whose
    /// integrity is the launched-entry digest itself.
    pub package_root: Option<PathBuf>,
    /// Whether this candidate was synthesized (bare `-- PROGRAM` argv),
    /// which pins its trust to `local-development`.
    pub synthesized: bool,
}

/// One resolved adapter: the gate's output before describe runs.
#[derive(Clone, Debug)]
pub struct ResolvedAdapter {
    /// The winning candidate.
    pub candidate: DiscoveryCandidate,
    /// The trust level assigned by the trust gate.
    pub trust: crate::adapter_package::TrustLevel,
}

impl ResolvedAdapter {
    /// The adapter id shortcut.
    pub fn adapter_id(&self) -> &str {
        self.candidate.manifest.adapter_id()
    }

    /// The adapter version shortcut.
    pub fn adapter_version(&self) -> &crate::adapter_package::types::SemVer {
        self.candidate.manifest.adapter_version()
    }
}

/// The closed discovery-source selector.
#[derive(Clone, Debug)]
pub enum DiscoverySource {
    /// An explicit project or filesystem path: a manifest file or a
    /// package directory containing `adapter.manifest.json`.
    Path(PathBuf),
    /// Enumerate PATH entries for adapter executables; the argument is
    /// an optional explicit name filter (`exec:<name>`).
    PathExec(Option<String>),
    /// Resolve a release coordinate from the local release records.
    Release(String),
    /// Resolve a registry coordinate from the cached snapshot records.
    Registry(String),
}

impl DiscoverySource {
    /// Parse the closed `SOURCE` grammar:
    /// `path:<fs-path>` | `exec:<name>` | `release:<channel>/<id>` |
    /// `registry:<registry-id>/<package-id>`. Anything URL-shaped,
    /// absolute-host-shaped, or unknown refuses.
    pub fn parse(text: &str) -> Result<Self, PackageFailure> {
        let unavailable = || PackageFailure::SourceUnavailable {
            source: bounded_coordinate(text),
        };
        if text.contains("://") || text.is_empty() || text.len() > 512 {
            return Err(unavailable());
        }
        let (kind, value) = text.split_once(':').ok_or_else(unavailable)?;
        match kind {
            "path" => {
                if value.is_empty() || value.contains('\0') {
                    return Err(unavailable());
                }
                Ok(Self::Path(PathBuf::from(value)))
            }
            "exec" => {
                if value.is_empty() || value.len() > 128 {
                    return Err(unavailable());
                }
                Ok(Self::PathExec(Some(value.to_owned())))
            }
            "release" => {
                if !value.contains('/') || value.len() > 256 {
                    return Err(unavailable());
                }
                Ok(Self::Release(value.to_owned()))
            }
            "registry" => {
                if !value.contains('/') || value.len() > 256 {
                    return Err(unavailable());
                }
                Ok(Self::Registry(value.to_owned()))
            }
            _ => Err(unavailable()),
        }
    }
}

fn bounded_coordinate(text: &str) -> String {
    let mut token: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '/' || c == ':' {
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

/// The discovery entry point: source → candidates.
pub fn discover(source: &DiscoverySource) -> Result<Vec<DiscoveryCandidate>, PackageFailure> {
    let mut candidates = match source {
        DiscoverySource::Path(path) => discover_path(path)?,
        DiscoverySource::PathExec(filter) => discover_path_exec(filter.as_deref())?,
        DiscoverySource::Release(coordinate) => {
            return super::records::resolve_record(
                ".lekalo/adapters/evidence/releases.json",
                "release",
                coordinate,
            )
        }
        DiscoverySource::Registry(coordinate) => {
            return super::records::resolve_record(
                ".lekalo/adapters/evidence/registry.json",
                "registry",
                coordinate,
            )
        }
    };
    // Deterministic candidate order: adapter id, then version ascending.
    candidates.sort_by(|left, right| {
        left.manifest
            .adapter_id()
            .cmp(right.manifest.adapter_id())
            .then_with(|| {
                left.manifest
                    .adapter_version()
                    .cmp(right.manifest.adapter_version())
            })
    });
    candidates.truncate(MAX_CANDIDATES);
    Ok(candidates)
}

/// `path`: manifest file or package directory.
fn discover_path(path: &Path) -> Result<Vec<DiscoveryCandidate>, PackageFailure> {
    let invalid = || PackageFailure::SourceUnavailable {
        source: bounded_coordinate(&path.to_string_lossy()),
    };
    if path.is_dir() {
        let manifest_path = path.join("adapter.manifest.json");
        let bytes = std::fs::read(&manifest_path).map_err(|_| invalid())?;
        let manifest = ManifestDocument::from_bytes(&bytes)?;
        return Ok(vec![DiscoveryCandidate {
            manifest,
            package_root: Some(path.to_path_buf()),
            synthesized: false,
        }]);
    }
    if path.is_file() {
        let bytes = std::fs::read(path).map_err(|_| invalid())?;
        let manifest = ManifestDocument::from_bytes(&bytes)?;
        // A manifest file's package root is its parent directory.
        let package_root = path
            .parent()
            .map(|parent| parent.to_path_buf())
            .filter(|parent| parent.exists());
        return Ok(vec![DiscoveryCandidate {
            manifest,
            package_root,
            synthesized: false,
        }]);
    }
    Err(invalid())
}

/// `path-exec`: explicit enumeration of the PATH entries, no shell.
///
/// On Windows the `PATHEXT` convention applies; an explicit name that
/// already ends in `.exe` is matched exactly, otherwise both spellings
/// are enumerated. Only regular files are candidates; directories and
/// reparse points are skipped by the `is_file` check.
fn discover_path_exec(filter: Option<&str>) -> Result<Vec<DiscoveryCandidate>, PackageFailure> {
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let mut candidates = Vec::new();
    let mut entries = 0usize;
    for dir in std::env::split_paths(&path_var) {
        entries += 1;
        if entries > MAX_PATH_ENTRIES || candidates.len() >= MAX_CANDIDATES {
            break;
        }
        let read = match std::fs::read_dir(&dir) {
            Ok(read) => read,
            // A missing or unreadable PATH entry is normal; skip it.
            Err(_) => continue,
        };
        for entry in read.flatten() {
            if candidates.len() >= MAX_CANDIDATES {
                break;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !is_adapter_executable(&name, filter) {
                continue;
            }
            let full = entry.path();
            if let Some(candidate) = executable_manifest(&full) {
                candidates.push(candidate);
            }
        }
    }
    Ok(candidates)
}

/// Whether one PATH file name is an adapter executable candidate: either
/// the explicit filter name or the `lekalo-target-*` convention.
fn is_adapter_executable(name: &str, filter: Option<&str>) -> bool {
    if let Some(filter) = filter {
        let stem = name.strip_suffix(".exe").unwrap_or(name);
        return stem == filter || name == filter;
    }
    name.starts_with("lekalo-target-")
}

/// Load the manifest beside a discovered executable. A PATH executable
/// without a manifest is a discovery gap, not a skip: the honest answer
/// is one candidate whose synthesized manifest pins the launched entry,
/// which the integrity gate then binds to the actual bytes.
fn executable_manifest(executable: &Path) -> Option<DiscoveryCandidate> {
    let manifest_path = executable.parent()?.join("adapter.manifest.json");
    if let Ok(bytes) = std::fs::read(&manifest_path) {
        if let Ok(manifest) = ManifestDocument::from_bytes(&bytes) {
            return Some(DiscoveryCandidate {
                manifest,
                package_root: executable.parent().map(|parent| parent.to_path_buf()),
                synthesized: false,
            });
        }
    }
    // No manifest beside the executable: synthesize the implicit
    // local-development descriptor from the launched entry itself.
    implicit_local_development(executable).ok()
}

/// Synthesize the implicit `local-development` manifest for a bare
/// `-- PROGRAM` argv (plan §3.5 step 1): the launched entry is the
/// package, the entry digest is the package digest, nothing else is
/// claimed. The descriptor is unsigned and declares no capabilities —
/// the describe handshake supplies the truth, and every gate still runs.
pub fn implicit_local_development(entry: &Path) -> Result<DiscoveryCandidate, PackageFailure> {
    let bytes = std::fs::read(entry).map_err(|_| PackageFailure::SourceUnavailable {
        source: bounded_coordinate(&entry.to_string_lossy()),
    })?;
    let digest = crate::digest::sha256_hex(&bytes);
    let file_name = entry
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "adapter".to_owned());
    let id = format!("local-{}", sanitize_id(&file_name));
    let manifest_json = serde_json::json!({
        "schemaVersion": super::version::MANIFEST_SCHEMA_VERSION,
        "identity": super::version::MANIFEST_IDENTITY,
        "adapter": { "id": id, "name": file_name, "version": "0.0.0" },
        "source": {
            "kind": "path",
            "coordinate": format!("path:{}", sanitize_coordinate(&file_name)),
            "digest": format!("sha256:{digest}")
        },
        "compatibility": {
            "protocolVersions": [crate::target_protocol::version::VERSION],
            "irVersions": [crate::ir::version::VERSION],
            "extensions": []
        },
        "publisher": { "id": "local", "trustAnchor": "none" },
        "license": { "spdx": "OTHER", "file": "LICENSE", "fileDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
        "platforms": ["any"],
        "executable": {
            "entry": sanitize_path_member(&file_name),
            "argvPreview": [sanitize_path_member(&file_name)]
        },
        "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
        "hooks": [],
        "conformance": { "reportDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
        "integrity": {
            "packageDigest": format!("sha256:{digest}"),
            "files": [ { "path": sanitize_path_member(&file_name),
                         "digest": format!("sha256:{digest}"),
                         "bytes": bytes.len() } ],
            "signaturePolicy": "unsigned",
            "signature": null
        },
        "status": "active",
        "revocation": null
    });
    let manifest = ManifestDocument::from_value(manifest_json)?;
    Ok(DiscoveryCandidate {
        manifest,
        package_root: None,
        synthesized: true,
    })
}

fn sanitize_id(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    out.truncate(96);
    while out.ends_with('-') || out.ends_with('.') {
        out.pop();
    }
    if out.is_empty() || out.starts_with('-') {
        out.insert(0, 'a');
    }
    out
}

fn sanitize_coordinate(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '/' || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    out.truncate(96);
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn sanitize_path_member(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    out.truncate(96);
    while out.ends_with('-') || out.ends_with('.') {
        out.pop();
    }
    if out.is_empty() {
        out.push('a');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, bytes).expect("write");
    }

    fn valid_manifest_json(id: &str, version: &str) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": id, "name": "Test adapter", "version": version },
            "source": { "kind": "path", "coordinate": "path:fixtures/x", "digest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": {
                "protocolVersions": [crate::target_protocol::version::VERSION],
                "irVersions": [crate::ir::version::VERSION],
                "extensions": []
            },
            "executable": { "entry": "adapter.mjs", "argvPreview": ["node", "adapter.mjs"] },
        "publisher": { "id": "test-pub", "trustAnchor": "none" },
        "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
        "platforms": ["any"],
        "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
        "hooks": [],
        "conformance": { "reportDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "integrity": {
                "packageDigest": format!("sha256:{}", "22".repeat(32)),
                "files": [ { "path": "adapter.mjs", "digest": format!("sha256:{}", "33".repeat(32)), "bytes": 10 } ],
                "signaturePolicy": "unsigned",
                "signature": null
            },
            "status": "active",
            "revocation": null
        })
    }

    #[test]
    fn source_grammar_is_closed() {
        assert!(matches!(
            DiscoverySource::parse("path:x/y"),
            Ok(DiscoverySource::Path(_))
        ));
        assert!(matches!(
            DiscoverySource::parse("exec:lekalo-target-node"),
            Ok(DiscoverySource::PathExec(Some(_)))
        ));
        assert!(matches!(
            DiscoverySource::parse("release:channel/id"),
            Ok(DiscoverySource::Release(_))
        ));
        assert!(matches!(
            DiscoverySource::parse("registry:hub/pkg"),
            Ok(DiscoverySource::Registry(_))
        ));
        // URL-shaped coordinates are refused.
        assert!(DiscoverySource::parse("https://example.invalid/x").is_err());
        assert!(DiscoverySource::parse("path:").is_err());
        assert!(DiscoverySource::parse("unknown:x").is_err());
    }

    #[test]
    fn path_discovery_reads_a_package_directory() {
        let root = std::env::temp_dir().join(format!("lekalo-ap-disc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let package = root.join("pkg");
        let manifest_bytes =
            serde_json::to_vec(&valid_manifest_json("pkg-adapter", "1.0.0")).unwrap();
        write(&package.join("adapter.manifest.json"), &manifest_bytes);
        let candidates = discover(&DiscoverySource::Path(package.clone())).expect("discover");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].manifest.adapter_id(), "pkg-adapter");
        assert_eq!(
            candidates[0].package_root.as_deref(),
            Some(package.as_path())
        );
        assert!(!candidates[0].synthesized);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_discovery_refuses_a_missing_path() {
        let missing = std::env::temp_dir().join("lekalo-ap-disc-missing-does-not-exist");
        let error = discover(&DiscoverySource::Path(missing)).expect_err("must refuse");
        assert!(matches!(error, PackageFailure::SourceUnavailable { .. }));
    }

    #[test]
    fn implicit_descriptor_is_unsigned_local_development() {
        let root = std::env::temp_dir().join(format!("lekalo-ap-implicit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let entry = root.join("my-adapter.mjs");
        write(&entry, b"console.log('adapter')\n");
        let candidate = implicit_local_development(&entry).expect("implicit");
        assert!(candidate.synthesized);
        assert!(candidate.package_root.is_none());
        assert_eq!(candidate.manifest.signature_policy().as_str(), "unsigned");
        assert!(candidate.manifest.adapter_id().starts_with("local-"));
        // The implicit descriptor's package digest is the entry digest.
        let digest = crate::digest::sha256_hex(b"console.log('adapter')\n");
        assert_eq!(
            candidate.manifest.package_digest().as_str(),
            format!("sha256:{digest}")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_exec_enumeration_finds_prefixed_executables() {
        // Build a synthetic PATH with one hit and one non-adapter.
        let root = std::env::temp_dir().join(format!("lekalo-ap-pathexec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("bin");
        write(&dir.join("lekalo-target-fake.mjs"), b"export default 1;\n");
        write(&dir.join("unrelated.exe"), b"not an adapter");
        // Save and swap PATH.
        let saved = std::env::var_os("PATH");
        std::env::set_var("PATH", &dir);
        let result = discover(&DiscoverySource::PathExec(None));
        std::env::set_var("PATH", saved.unwrap_or_default());
        let candidates = result.expect("enumerate");
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].manifest.adapter_id().starts_with("local-"));
        assert!(candidates[0].synthesized);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn release_and_registry_sources_are_honest_offline() {
        let error = discover(&DiscoverySource::Release("channel/pkg".to_owned()))
            .expect_err("no records in v1");
        assert!(matches!(error, PackageFailure::SourceUnavailable { .. }));
        let error = discover(&DiscoverySource::Registry("hub/pkg".to_owned()))
            .expect_err("no records in v1");
        assert!(matches!(error, PackageFailure::SourceUnavailable { .. }));
    }
}
