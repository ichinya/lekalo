//! The adapter package manifest, discovery, install and trust model
//! (issue #32).
//!
//! This module owns the custody chain between "bytes on a disk" and "a
//! target adapter the core may execute": the closed package manifest
//! ([`manifest`]), the discovery sources ([`discovery`]), the integrity
//! and signature gates ([`integrity`], [`signature`]), the trust
//! vocabulary with the revocation store ([`trust`]), the governed
//! `.lekalo/adapters/**` store ([`inventory`]), the previewed-and-
//! confirmed atomic install ([`install`]), and the manifest-to-manifest
//! update diff ([`diff`]).
//!
//! Security invariants, in gate order — every execution path enters
//! through [`resolve`], and no child process exists before the gates
//! pass:
//!
//! 1. **Manifest gate.** The package manifest decodes and validates
//!    against the closed contract (`adapter.manifest-invalid`).
//! 2. **Compatibility gate.** The manifest's exact-set compatibility
//!    must cover the current protocol and IR contracts
//!    (`adapter.incompatible`).
//! 3. **Integrity gate.** Every declared file digest and the package
//!    digest are recomputed from the bytes on disk **before any
//!    execution, describe included** (`adapter.checksum-mismatch`); the
//!    declared signature policy is then evaluated honestly
//!    (`adapter.signature-unverified`).
//! 4. **Trust gate.** The revocation store overrides everything
//!    (`adapter.revoked`); the assigned trust level decides selection
//!    eligibility and quarantine posture (`adapter.trust-insufficient`).
//! 5. **Describe.** The safe handshake runs under the gate-derived
//!    posture, and its self-asserted outcome is cross-checked against
//!    the verified manifest (`adapter.manifest-mismatch`).
//!
//! Trust never widens runtime permissions: the level selects candidacy
//! and confinement strictness only; the declared `permissions` block is
//! the policy the execution-isolation issue enforces. Discovery is never
//! install, and install is never trust: auto-discovery neither installs
//! nor promotes anything.

pub mod canonical;
pub mod consistency;
pub mod diagnostic;
pub mod diff;
pub mod discovery;
pub mod install;
pub mod integrity;
pub mod inventory;
pub mod manifest;
pub mod permissions;
pub mod quarantine;
pub mod records;
pub mod signature;
pub mod trust;
pub mod types;
pub mod version;

pub use diff::{diff_manifests, ManifestDiff};
pub use discovery::{discover, implicit_local_development, DiscoverySource, ResolvedAdapter};
pub use install::{ApplyRejection, InstallAction, InstallPlan};
pub use integrity::verify_package;
pub use inventory::Inventory;
pub use manifest::{ManifestDigest, ManifestDocument};
pub use signature::evaluate as evaluate_signature;
pub use trust::{assign as assign_trust, gate as trust_gate, RevocationStore, TrustLevel};
pub use types::PackageFailure;

/// Stable context the resolution gate runs in: the project root (for the
/// revocation store) and the offline flag.
#[derive(Clone, Debug)]
pub struct ResolveContext {
    /// The project root; `None` for store-less invocations (the bare
    /// `adapter test` surface), where the revocation store is empty.
    pub root: Option<std::path::PathBuf>,
    /// Refuse every source that is not already local. `path` sources are
    /// always local; `release`/`registry` resolve only from the local
    /// evidence records under `.lekalo/adapters/evidence/` (no fetch in
    /// v1 — see [`records`]).
    pub offline: bool,
}

/// Why one resolved candidate was refused by the post-discovery gates.
/// The gate order is fixed: integrity → signature → trust.
#[derive(Clone, Debug)]
pub enum ResolveRejection {
    /// The bytes on disk do not match the manifest.
    Integrity(PackageFailure),
    /// The signature policy cannot be honored.
    Signature(PackageFailure),
    /// The trust or revocation gate refused.
    Trust(PackageFailure),
}

impl ResolveRejection {
    /// The packaged refusal.
    pub fn failure(&self) -> &PackageFailure {
        match self {
            Self::Integrity(failure) | Self::Signature(failure) | Self::Trust(failure) => failure,
        }
    }
}

/// The resolution gate (plan §3.5): source → candidates → integrity →
/// signature → trust/revocation. Every execution surface calls this
/// before any child process exists; describe runs only on a fully
/// resolved candidate.
///
/// Returns the winning candidate (deterministically ordered by
/// discovery) with its assigned trust level, or the first refusal in
/// gate order.
pub fn resolve(
    source: &DiscoverySource,
    context: &ResolveContext,
) -> Result<ResolvedAdapter, PackageFailure> {
    if context.offline {
        if let DiscoverySource::Release(_) | DiscoverySource::Registry(_) = source {
            return Err(PackageFailure::SourceUnavailable {
                source: coordinate_of(source),
            });
        }
    }
    // The context root scopes release/registry record lookups (issue #32
    // fix round 2, cline F-7 / devin F-10): records resolve against the
    // selected project, never the CWD's project.
    let candidates = discover(source, context.root.as_ref())?;
    let candidate =
        candidates
            .first()
            .cloned()
            .ok_or_else(|| PackageFailure::SourceUnavailable {
                source: coordinate_of(source),
            })?;
    resolve_candidate(candidate, context)
}

/// Run the post-discovery gates over one candidate.
pub fn resolve_candidate(
    candidate: discovery::DiscoveryCandidate,
    context: &ResolveContext,
) -> Result<ResolvedAdapter, PackageFailure> {
    // Gate 0: compatibility — the manifest's exact-set compatibility must
    // cover the current protocol and IR contracts before anything else
    // runs (`adapter.incompatible`, exit 5).
    {
        let manifest = &candidate.manifest;
        if !manifest.covers(
            crate::target_protocol::version::VERSION,
            crate::ir::version::VERSION,
        ) {
            return Err(PackageFailure::Incompatible {
                adapter: manifest.adapter_id().to_owned(),
            });
        }
    }
    // Gate 1: integrity — checksums before anything else executes.
    integrity::verify_package(&candidate)?;
    // Gate 2: the declared signature policy, evaluated honestly.
    signature::evaluate(&candidate.manifest)?;
    // Gate 3: trust assignment and the revocation override.
    let store = match &context.root {
        Some(root) => RevocationStore::load(root)?,
        None => RevocationStore::default(),
    };
    let level = trust::assign(&candidate);
    let level = trust::gate(&candidate.manifest, level, &store)?;
    Ok(ResolvedAdapter {
        candidate,
        trust: level,
    })
}

fn coordinate_of(source: &DiscoverySource) -> String {
    match source {
        DiscoverySource::Path(path) => format!("path:{}", path.to_string_lossy()),
        DiscoverySource::PathExec(Some(name)) => format!("exec:{name}"),
        DiscoverySource::PathExec(None) => "exec:*".to_owned(),
        DiscoverySource::Release(coordinate) => format!("release:{coordinate}"),
        DiscoverySource::Registry(coordinate) => format!("registry:{coordinate}"),
    }
    .chars()
    .take(128)
    .collect()
}

/// The stable module-local reason codes. Every refusal maps onto a
/// registered `adapter.*` rule through [`diagnostic`]; the list here is
/// the closed spelling the callers match on.
pub mod reasons {
    /// The manifest is not a valid closed-wire document.
    pub const MANIFEST_INVALID: &str = "adapter.manifest-invalid";
    /// The describe outcome contradicts the verified manifest.
    pub const MANIFEST_MISMATCH: &str = "adapter.manifest-mismatch";
    /// The manifest compatibility set excludes the current contracts.
    pub const INCOMPATIBLE: &str = "adapter.incompatible";
    /// Declared digests do not match the bytes on disk.
    pub const CHECKSUM_MISMATCH: &str = "adapter.checksum-mismatch";
    /// A required signature cannot be verified by any shipped verifier.
    pub const SIGNATURE_UNVERIFIED: &str = "adapter.signature-unverified";
    /// The adapter id/version is recorded in the revocation store.
    pub const REVOKED: &str = "adapter.revoked";
    /// The package bytes are in quarantine custody.
    pub const QUARANTINED: &str = "adapter.quarantined";
    /// The trust level requires an explicit opt-in that was not given.
    pub const TRUST_INSUFFICIENT: &str = "adapter.trust-insufficient";
    /// The discovery source cannot be resolved (including offline).
    pub const SOURCE_UNAVAILABLE: &str = "adapter.source-unavailable";
    /// A mutating install/update/rollback ran without a confirmed plan.
    pub const INSTALL_PLAN_REQUIRED: &str = "adapter.install-plan-required";
    /// Inputs drifted between preview and confirmation.
    pub const SOURCE_CHANGED: &str = "adapter.source-changed";
    /// The planned path is occupied by foreign content.
    pub const INSTALL_CONFLICT: &str = "adapter.install-conflict";
    /// An install journal is ambiguous; explicit recovery is required.
    pub const RECOVERY_REQUIRED: &str = "adapter.recovery-required";
    /// The package declares hooks; v1 refuses non-empty hooks.
    pub const HOOKS_DECLARED: &str = "adapter.hooks-declared";
    /// An update widens permissions without the explicit policy.
    pub const PERMISSION_ESCALATED: &str = "adapter.permission-escalated";
}

#[cfg(test)]
mod resolve_tests {
    use super::discovery::DiscoveryCandidate;
    use super::manifest::ManifestDocument;
    use super::types::PackageFailure;
    use super::{resolve_candidate, ResolveContext};
    use std::path::PathBuf;

    fn candidate(id: &str, bytes: &'static [u8]) -> DiscoveryCandidate {
        let digest = crate::digest::sha256_hex(bytes);
        let json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": id, "name": "R", "version": "1.0.0" },
            "source": { "kind": "path", "coordinate": "path:fixtures/r", "digest": format!("sha256:{}", "11".repeat(32)) },
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
                "packageDigest": format!("sha256:{digest}"),
                "files": [ { "path": "a.mjs", "digest": format!("sha256:{digest}"), "bytes": bytes.len() } ],
                "signaturePolicy": "unsigned",
                "signature": null
            },
            "status": "active",
            "revocation": null
        });
        DiscoveryCandidate {
            manifest: ManifestDocument::from_value(json).expect("parses"),
            package_root: None,
            synthesized: true,
        }
    }

    #[test]
    fn an_incompatible_manifest_refuses_at_the_compatibility_gate() {
        let context = ResolveContext {
            root: None,
            offline: false,
        };
        let mut candidate = candidate(
            "legacy-adapter",
            b"const x = 1;
",
        );
        // Rebuild the manifest declaring an unsupported protocol: the
        // integrity data stays honest, so the refusal can only come from
        // the compatibility gate.
        let digest = crate::digest::sha256_hex(
            b"const x = 1;
",
        );
        let mut json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "legacy-adapter", "name": "L", "version": "1.0.0" },
            "publisher": { "id": "p", "trustAnchor": "none" },
            "source": { "kind": "path", "coordinate": "path:x", "digest": format!("sha256:{}", "11".repeat(32)) },
            "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": { "protocolVersions": [crate::target_protocol::version::VERSION], "irVersions": [crate::ir::version::VERSION], "extensions": [] },
            "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
            "executable": { "runtime": { "kind": "node", "minVersion": "18.0.0" }, "entry": "a.mjs", "argvPreview": ["node", "a.mjs"], "assets": [] },
            "platforms": ["any"],
            "integrity": { "packageDigest": format!("sha256:{digest}"), "files": [ { "path": "a.mjs", "digest": format!("sha256:{digest}"), "bytes": 15 } ], "signaturePolicy": "unsigned", "signature": null },
            "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
            "hooks": [],
            "conformance": { "reportDigest": format!("sha256:{}", "11".repeat(32)), "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "status": "active",
            "revocation": null
        });
        json["compatibility"]["protocolVersions"] = serde_json::json!(["9.9.9"]);
        json["integrity"]["packageDigest"] =
            serde_json::Value::String(format!("sha256:{}", "99".repeat(32)));
        candidate.manifest = ManifestDocument::from_value(json).expect("parses");
        let error = resolve_candidate(candidate, &context).expect_err("incompatible");
        assert!(matches!(error, PackageFailure::Incompatible { .. }));
    }

    #[test]
    fn gate_order_is_integrity_signature_trust() {
        let context = ResolveContext {
            root: None,
            offline: false,
        };
        // A consistent synthesized candidate passes every gate.
        let resolved = resolve_candidate(candidate("ok-adapter", b"const a = 1;\n"), &context)
            .expect("passes");
        assert_eq!(resolved.trust.as_str(), "local-development");
    }

    #[test]
    fn a_revoked_id_refuses_before_anything_runs() {
        let root = std::env::temp_dir().join(format!("lekalo-ap-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let mut store = super::RevocationStore::load(&root).expect("store");
        store
            .append(
                &root,
                super::trust::RevocationRecord {
                    id: "gone-adapter".to_owned(),
                    version: "*".to_owned(),
                    reason: "compromised".to_owned(),
                },
            )
            .expect("append");
        let context = ResolveContext {
            root: Some(root.clone()),
            offline: false,
        };
        let error = resolve_candidate(candidate("gone-adapter", b"const a = 2;\n"), &context)
            .expect_err("revoked refuses");
        assert!(matches!(error, PackageFailure::Revoked { .. }));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn offline_refuses_remote_sources_without_enumeration() {
        let context = ResolveContext {
            root: None,
            offline: true,
        };
        let error = super::resolve(
            &super::DiscoverySource::Release("ch/pkg".to_owned()),
            &context,
        )
        .expect_err("offline release refuses");
        assert!(matches!(error, PackageFailure::SourceUnavailable { .. }));
        let error = super::resolve(
            &super::DiscoverySource::Registry("hub/pkg".to_owned()),
            &context,
        )
        .expect_err("offline registry refuses");
        assert!(matches!(error, PackageFailure::SourceUnavailable { .. }));
    }

    #[test]
    fn end_to_end_path_resolution_over_a_fixture_package() {
        let root =
            std::env::temp_dir().join(format!("lekalo-ap-resolve-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let package = root.join("pkg");
        std::fs::create_dir_all(&package).expect("mkdir");
        let entry_bytes = b"export const gate = true;\n";
        std::fs::write(package.join("a.mjs"), entry_bytes).expect("entry");
        let entry_digest = crate::digest::sha256_hex(entry_bytes);
        // The manifest must exist on disk before the package digest is
        // computed: the normative domain includes its implicit
        // self-contribution (canonical bytes, packageDigest zeroed).
        let manifest_json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "e2e-adapter", "name": "E2E", "version": "1.0.0" },
            "source": { "kind": "path", "coordinate": "path:pkg", "digest": format!("sha256:{}", "22".repeat(32)) },
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
                "packageDigest": format!("sha256:{}", "0".repeat(64)),
                "files": [ { "path": "a.mjs", "digest": format!("sha256:{entry_digest}"), "bytes": entry_bytes.len() } ],
                "signaturePolicy": "unsigned",
                "signature": null
            },
            "status": "active",
            "revocation": null
        });
        let provisional = serde_json::to_vec_pretty(&manifest_json).unwrap();
        std::fs::write(package.join(super::integrity::MANIFEST_FILE), &provisional)
            .expect("manifest");
        let parsed = ManifestDocument::from_bytes(&provisional).expect("manifest parses");
        let manifest_part = {
            let mut part = Vec::new();
            part.extend_from_slice(super::integrity::MANIFEST_FILE.as_bytes());
            part.push(0);
            part.extend_from_slice(&parsed.digest_domain_bytes().len().to_be_bytes());
            part.push(0);
            part.extend_from_slice(&parsed.digest_domain_bytes());
            part
        };
        let entry_part = {
            let mut part = Vec::new();
            part.extend_from_slice(b"a.mjs");
            part.push(0);
            part.extend_from_slice(&(entry_bytes.len() as u64).to_be_bytes());
            part.push(0);
            part.extend_from_slice(entry_bytes);
            part
        };
        let package_digest = super::integrity::package_digest_hex(&[entry_part, manifest_part]);
        let mut final_manifest = manifest_json;
        final_manifest["integrity"]["packageDigest"] =
            serde_json::Value::String(format!("sha256:{package_digest}"));
        let manifest_bytes = serde_json::to_vec_pretty(&final_manifest).unwrap();
        std::fs::write(
            package.join(super::integrity::MANIFEST_FILE),
            &manifest_bytes,
        )
        .expect("manifest");
        let context = ResolveContext {
            root: Some(root.clone()),
            offline: true,
        };
        let resolved = super::resolve(&super::DiscoverySource::Path(package.clone()), &context)
            .expect("resolves");
        assert_eq!(resolved.adapter_id(), "e2e-adapter");
        assert_eq!(resolved.trust.as_str(), "local-development");
        assert_eq!(resolved.candidate.package_root, Some(package.clone()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = PathBuf::new();
    }
}
