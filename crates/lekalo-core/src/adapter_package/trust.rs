//! The adapter trust vocabulary and the local revocation store
//! (issue #32).
//!
//! Trust levels, spelled exactly as the execution-isolation issue (#89)
//! consumes them: `builtin`, `verified`, `local-development`,
//! `community`, `revoked`. Trust never widens runtime permissions — the
//! level selects candidacy, confinement strictness, and auto-selection
//! eligibility only.
//!
//! The revocation store is append-only local evidence at
//! `.lekalo/adapters/evidence/revocations.json`; it is consulted before
//! selection and again at lock verification. Revocation overrides every
//! other signal, including `builtin`. A manifest's own
//! `status`/`revocation` is publisher self-report; the *enforcing*
//! signal is this store, so an adversarial manifest cannot un-revoke
//! itself.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::discovery::DiscoveryCandidate;
use super::manifest::{ManifestDocument, ManifestStatus, SignaturePolicy, SourceKind};
use super::types::PackageFailure;

/// The exact runtime home of the revocation store.
pub const REVOCATIONS_FILE: &str = ".lekalo/adapters/evidence/revocations.json";

/// The closed trust-level vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustLevel {
    /// Shipped inside the Lekalo distribution (compile-time pinned,
    /// digest-matched).
    Builtin,
    /// Signature-verified per policy with a passing conformance
    /// reference and an anchored publisher.
    Verified,
    /// The project's own explicit path; unsigned or optionally signed.
    LocalDevelopment,
    /// Any package without a verified publisher anchor (PATH hits,
    /// release/registry records).
    Community,
    /// Recorded revocation of any prior trust; never selectable.
    Revoked,
}

impl TrustLevel {
    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "builtin" => Some(Self::Builtin),
            "verified" => Some(Self::Verified),
            "local-development" => Some(Self::LocalDevelopment),
            "community" => Some(Self::Community),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Verified => "verified",
            Self::LocalDevelopment => "local-development",
            Self::Community => "community",
            Self::Revoked => "revoked",
        }
    }

    /// Whether this level is eligible for automatic selection.
    pub const fn auto_selectable(self) -> bool {
        matches!(self, Self::Builtin | Self::Verified)
    }

    /// Whether this level requires quarantine custody at install.
    pub const fn quarantined_at_install(self) -> bool {
        matches!(self, Self::Community)
    }
}

/// One append-only revocation record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RevocationRecord {
    /// The revoked adapter id.
    pub id: String,
    /// The revoked version, or `*` for a whole-id revocation.
    pub version: String,
    /// The closed reason token.
    pub reason: String,
}

/// The parsed revocation store.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RevocationStore {
    records: Vec<RevocationRecord>,
}

impl RevocationStore {
    /// Load the store from the project root; a missing file is an empty
    /// store (nothing has been revoked), a malformed file is a
    /// refusal — revocation evidence never degrades into silence.
    pub fn load(root: &Path) -> Result<Self, PackageFailure> {
        let path = root.join(REVOCATIONS_FILE.replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => {
                return Err(PackageFailure::Revoked {
                    id: "store".to_owned(),
                    version: "*".to_owned(),
                })
            }
        };
        Self::from_bytes(&bytes)
    }

    /// Parse the store from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PackageFailure> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(rename = "schemaVersion")]
            schema_version: String,
            #[serde(default)]
            records: Vec<RecordWire>,
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RecordWire {
            id: String,
            version: String,
            reason: String,
        }
        let wire: Wire = serde_json::from_slice(bytes).map_err(|_| PackageFailure::Revoked {
            id: "store".to_owned(),
            version: "*".to_owned(),
        })?;
        if wire.schema_version != "lekalo/adapter-revocations/v0.3.2" {
            return Err(PackageFailure::Revoked {
                id: "store".to_owned(),
                version: "*".to_owned(),
            });
        }
        let mut records = Vec::with_capacity(wire.records.len());
        for record in wire.records {
            if record.id.is_empty() || record.id.len() > 128 {
                return Err(PackageFailure::Revoked {
                    id: "store".to_owned(),
                    version: "*".to_owned(),
                });
            }
            if record.version.is_empty() || record.version.len() > 32 {
                return Err(PackageFailure::Revoked {
                    id: "store".to_owned(),
                    version: "*".to_owned(),
                });
            }
            records.push(RevocationRecord {
                id: record.id,
                version: record.version,
                reason: record.reason,
            });
        }
        Ok(Self { records })
    }

    /// Whether the exact id/version (or a whole-id `*` row) is revoked.
    pub fn is_revoked(&self, id: &str, version: &str) -> bool {
        self.records
            .iter()
            .any(|record| record.id == id && (record.version == "*" || record.version == version))
    }

    /// Every record, in store order (append-only).
    pub fn records(&self) -> &[RevocationRecord] {
        &self.records
    }

    /// Append one record and write the store back under the project
    /// root. The store is append-only: an existing exact row is
    /// unchanged, everything else is appended.
    pub fn append(&mut self, root: &Path, record: RevocationRecord) -> Result<(), PackageFailure> {
        if !self
            .records
            .iter()
            .any(|existing| existing.id == record.id && existing.version == record.version)
        {
            self.records.push(record);
        }
        self.store(root)
    }

    fn store(&self, root: &Path) -> Result<(), PackageFailure> {
        let path = root.join(REVOCATIONS_FILE.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| PackageFailure::SourceUnavailable {
                source: "registry:revocations".to_owned(),
            })?;
        }
        #[derive(Serialize)]
        struct Wire<'a> {
            #[serde(rename = "schemaVersion")]
            schema_version: &'static str,
            records: &'a [RevocationRecord],
        }
        let wire = Wire {
            schema_version: "lekalo/adapter-revocations/v0.3.2",
            records: &self.records,
        };
        let mut bytes =
            serde_json::to_vec(&wire).map_err(|_| PackageFailure::SourceUnavailable {
                source: "registry:revocations".to_owned(),
            })?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes).map_err(|_| PackageFailure::SourceUnavailable {
            source: "registry:revocations".to_owned(),
        })?;
        Ok(())
    }
}

/// Assign the trust level of one discovered candidate, before any
/// describe runs (plan §3.6 table).
pub fn assign(candidate: &DiscoveryCandidate) -> TrustLevel {
    let manifest = &candidate.manifest;
    if manifest.status() == ManifestStatus::Revoked {
        return TrustLevel::Revoked;
    }
    if candidate.synthesized {
        // Bare `-- PROGRAM`: the project's own explicit entry.
        return TrustLevel::LocalDevelopment;
    }
    match (manifest.source_kind(), manifest.signature_policy()) {
        // An explicit project path stays local development regardless
        // of the optional signature.
        (SourceKind::Path | SourceKind::PathExec, _)
            if manifest.source_coordinate().starts_with("path:") =>
        {
            // `verified` requires the required-policy signature path;
            // the required policy with no shipped verifier already
            // refused, so a surviving explicit path is local dev.
            TrustLevel::LocalDevelopment
        }
        // Release/registry packages with a verified publisher anchor
        // and a passing conformance badge would be verified; with no
        // shipped verifier in v1 they stay community.
        (SourceKind::Release | SourceKind::Registry, _) => TrustLevel::Community,
        // PATH-found executables are community without exception.
        (SourceKind::PathExec, _) => TrustLevel::Community,
        // A project-root path package: explicit local source.
        (SourceKind::Path, SignaturePolicy::Unsigned | SignaturePolicy::Optional) => {
            TrustLevel::LocalDevelopment
        }
        // A required policy cannot reach assignment (the signature gate
        // already refused); the arm exists for exhaustivity and stays
        // conservative.
        (SourceKind::Path, SignaturePolicy::Required) => TrustLevel::Community,
    }
}

/// The trust + revocation gate: run after integrity, before describe.
/// A revocation override fires first; then a manifest whose own status
/// is `yanked`/`revoked` is handled (yanked stays resolvable but
/// flagged; revoked denies); finally the assigned level is returned.
pub fn gate(
    manifest: &ManifestDocument,
    assigned: TrustLevel,
    store: &RevocationStore,
) -> Result<TrustLevel, PackageFailure> {
    if store.is_revoked(manifest.adapter_id(), manifest.adapter_version().as_str()) {
        return Err(PackageFailure::Revoked {
            id: manifest.adapter_id().to_owned(),
            version: manifest.adapter_version().to_string(),
        });
    }
    if assigned == TrustLevel::Revoked || manifest.status() == ManifestStatus::Revoked {
        return Err(PackageFailure::Revoked {
            id: manifest.adapter_id().to_owned(),
            version: manifest.adapter_version().to_string(),
        });
    }
    Ok(assigned)
}

/// The runtime home of the revocation store under one project root
/// (test and doctor helper).
pub fn revocations_path(root: &Path) -> PathBuf {
    root.join(REVOCATIONS_FILE.replace('/', std::path::MAIN_SEPARATOR_STR))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        id: &str,
        source_kind: &str,
        coordinate: &str,
        synthesized: bool,
    ) -> DiscoveryCandidate {
        let json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": id, "name": "T", "version": "1.0.0" },
            "source": { "kind": source_kind, "coordinate": coordinate, "digest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": {
                "protocolVersions": [crate::target_protocol::version::VERSION],
                "irVersions": [crate::ir::version::VERSION],
                "extensions": []
            },
            "executable": { "entry": "a.mjs" },
        "publisher": { "id": "test-pub", "trustAnchor": "none" },
        "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
        "platforms": ["any"],
        "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
        "hooks": [],
        "conformance": { "reportDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "integrity": {
                "packageDigest": format!("sha256:{}", "22".repeat(32)),
                "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "33".repeat(32)), "bytes": 3 } ],
                "signaturePolicy": "unsigned",
                "signature": null
            },
            "status": "active",
            "revocation": null
        });
        DiscoveryCandidate {
            manifest: ManifestDocument::from_value(json).expect("parses"),
            package_root: None,
            synthesized,
        }
    }

    #[test]
    fn assignment_follows_the_plan_table() {
        // Explicit project path → local-development.
        assert_eq!(
            assign(&candidate("a", "path", "path:adapters/x", false)),
            TrustLevel::LocalDevelopment
        );
        // PATH-found executable → community.
        assert_eq!(
            assign(&candidate("b", "path-exec", "exec:lekalo-target-x", false)),
            TrustLevel::Community
        );
        // Release record → community in v1 (no shipped verifier).
        assert_eq!(
            assign(&candidate("c", "release", "release:ch/id", false)),
            TrustLevel::Community
        );
        // Registry record → community in v1.
        assert_eq!(
            assign(&candidate("d", "registry", "registry:hub/pkg", false)),
            TrustLevel::Community
        );
        // Synthesized bare-argv descriptor → local-development.
        assert_eq!(
            assign(&candidate("e", "path", "path:x.mjs", true)),
            TrustLevel::LocalDevelopment
        );
    }

    #[test]
    fn revocation_overrides_everything_including_builtin() {
        let store = RevocationStore::from_bytes(
            b"{\"schemaVersion\":\"lekalo/adapter-revocations/v0.3.2\",\"records\":[{\"id\":\"a\",\"version\":\"*\",\"reason\":\"compromised\"}]}",
        )
        .expect("store");
        let manifest = candidate("a", "path", "path:x", false).manifest;
        let error = gate(&manifest, TrustLevel::Builtin, &store).expect_err("revoked");
        assert!(matches!(error, PackageFailure::Revoked { .. }));
        // A different id passes.
        let other = candidate("other", "path", "path:x", false).manifest;
        assert!(gate(&other, TrustLevel::Builtin, &store).is_ok());
        // Exact-version rows do not match other versions.
        let exact = RevocationStore::from_bytes(
            b"{\"schemaVersion\":\"lekalo/adapter-revocations/v0.3.2\",\"records\":[{\"id\":\"b\",\"version\":\"1.0.0\",\"reason\":\"yanked\"}]}",
        )
        .expect("store");
        let manifest_b = candidate("b", "path", "path:x", false).manifest;
        assert!(gate(&manifest_b, TrustLevel::Community, &exact).is_err());
    }

    #[test]
    fn revoked_manifest_status_denies_without_a_store() {
        let mut candidate = candidate("c", "path", "path:x", false);
        // Rewrite the status through raw JSON to a revoked manifest.
        let store = RevocationStore::default();
        let json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "c", "name": "T", "version": "1.0.0" },
            "source": { "kind": "path", "coordinate": "path:x", "digest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": {
                "protocolVersions": [crate::target_protocol::version::VERSION],
                "irVersions": [crate::ir::version::VERSION],
                "extensions": []
            },
            "executable": { "entry": "a.mjs" },
        "publisher": { "id": "test-pub", "trustAnchor": "none" },
        "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
        "platforms": ["any"],
        "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
        "hooks": [],
        "conformance": { "reportDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "integrity": {
                "packageDigest": format!("sha256:{}", "22".repeat(32)),
                "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "33".repeat(32)), "bytes": 3 } ],
                "signaturePolicy": "unsigned",
                "signature": null
            },
            "status": "revoked",
            "revocation": { "reason": "publisher-withdrawal" }
        });
        candidate.manifest = ManifestDocument::from_value(json).expect("parses");
        let error = gate(&candidate.manifest, assign(&candidate), &store).expect_err("revoked");
        assert!(matches!(error, PackageFailure::Revoked { .. }));
    }

    #[test]
    fn missing_store_is_empty_and_append_round_trips() {
        let root = std::env::temp_dir().join(format!("lekalo-ap-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let store = RevocationStore::load(&root).expect("empty store");
        assert!(store.records().is_empty());
        let mut store = store;
        store
            .append(
                &root,
                RevocationRecord {
                    id: "x".to_owned(),
                    version: "1.0.0".to_owned(),
                    reason: "compromised".to_owned(),
                },
            )
            .expect("append");
        let reloaded = RevocationStore::load(&root).expect("reload");
        assert!(reloaded.is_revoked("x", "1.0.0"));
        assert!(!reloaded.is_revoked("x", "2.0.0"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_store_refuses_instead_of_silence() {
        let error = RevocationStore::from_bytes(b"not json").expect_err("malformed");
        assert!(matches!(error, PackageFailure::Revoked { .. }));
        let error = RevocationStore::from_bytes(
            b"{\"schemaVersion\":\"lekalo/adapter-revocations/v9.9.9\",\"records\":[]}",
        )
        .expect_err("unknown version");
        assert!(matches!(error, PackageFailure::Revoked { .. }));
    }

    #[test]
    fn level_eligibility_flags() {
        assert!(TrustLevel::Builtin.auto_selectable());
        assert!(TrustLevel::Verified.auto_selectable());
        assert!(!TrustLevel::LocalDevelopment.auto_selectable());
        assert!(!TrustLevel::Community.auto_selectable());
        assert!(!TrustLevel::Revoked.auto_selectable());
        assert!(TrustLevel::Community.quarantined_at_install());
        assert!(!TrustLevel::Verified.quarantined_at_install());
    }
}
