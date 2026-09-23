//! Package integrity verification (issue #32).
//!
//! Checksum verification is unconditional and runs **before any child
//! process exists** — describe included. [`verify_package`] re-digests
//! every manifest-listed file against `integrity.files[]`, checks the
//! byte lengths, and recomputes the package digest over the sorted
//! per-file canonical byte set. One flipped byte refuses the package.
//!
//! The package digest domain: SHA-256 over the concatenation of, for
//! every file in canonical (path-sorted) order, the path bytes, one NUL,
//! the 8-byte big-endian byte length, one NUL, and the exact file bytes.
//! The manifest file itself is an entry named `adapter.manifest.json`;
//! its declared digest is computed over its stored bytes **minus** the
//! self-referential `manifestDigest` member when present (the same
//! exclusion the manifest identity digest uses), so a manifest can never
//! carry a digest it authored.

use std::path::Path;

use super::discovery::DiscoveryCandidate;
use super::manifest::ManifestDocument;
use super::types::PackageFailure;

/// The canonical manifest file name inside a package root.
pub const MANIFEST_FILE: &str = "adapter.manifest.json";

/// Verify one candidate's whole package tree against its manifest.
/// A synthesized candidate (bare `-- PROGRAM`) verifies the single
/// launched entry the descriptor names.
pub fn verify_package(candidate: &DiscoveryCandidate) -> Result<(), PackageFailure> {
    let manifest = &candidate.manifest;
    match (&candidate.package_root, candidate.synthesized) {
        (None, true) => verify_synthesized(manifest),
        (Some(root), false) => verify_tree(root, manifest),
        (None, false) => Err(PackageFailure::ChecksumMismatch {
            domain: "package".to_owned(),
            identity: manifest.adapter_id().to_owned(),
        }),
        (Some(_), true) => verify_synthesized(manifest),
    }
}

fn verify_synthesized(manifest: &ManifestDocument) -> Result<(), PackageFailure> {
    // The synthesized descriptor lists exactly one file: the launched
    // entry, digest-equal to the package digest. Its path is a logical
    // member, so re-reading it from disk is impossible by construction —
    // the bytes were digested at synthesis time. The gate therefore
    // verifies the descriptor's internal consistency here and binds the
    // on-disk bytes at launch time (execution binding).
    if manifest.files().len() != 1 {
        return Err(PackageFailure::ChecksumMismatch {
            domain: "package".to_owned(),
            identity: manifest.adapter_id().to_owned(),
        });
    }
    let file = &manifest.files()[0];
    if file.digest().as_str() != manifest.package_digest().as_str() {
        return Err(PackageFailure::ChecksumMismatch {
            domain: "entry".to_owned(),
            identity: manifest.adapter_id().to_owned(),
        });
    }
    Ok(())
}

fn verify_tree(root: &Path, manifest: &ManifestDocument) -> Result<(), PackageFailure> {
    let identity = manifest.adapter_id().to_owned();
    // 0. The manifest self-contribution is implicit in the package
    //    digest domain (the manifest cannot cover its own digest), so
    //    listing it in files[] would double-frame it and make the
    //    declared packageDigest circular.
    if manifest
        .files()
        .iter()
        .any(|file| file.path() == MANIFEST_FILE)
    {
        return Err(PackageFailure::ManifestInvalid {
            reason: "manifest-self-entry".to_owned(),
        });
    }
    // 1. Every declared file must exist with exactly the declared bytes.
    for file in manifest.files() {
        let path = root.join(file.path().replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = std::fs::read(&path).map_err(|_| PackageFailure::ChecksumMismatch {
            domain: "file".to_owned(),
            identity: file.path().to_owned(),
        })?;
        if bytes.len() as u64 != file.bytes() {
            return Err(PackageFailure::ChecksumMismatch {
                domain: "file".to_owned(),
                identity: file.path().to_owned(),
            });
        }
        let actual = crate::digest::sha256_hex(&bytes);
        if actual != file.digest().as_str()["sha256:".len()..] {
            return Err(PackageFailure::ChecksumMismatch {
                domain: "file".to_owned(),
                identity: file.path().to_owned(),
            });
        }
    }
    // 2. The declared package digest must equal the digest over the
    //    canonical byte set. The manifest entry's contribution uses its
    //    digest-excluded canonical bytes, not the stored bytes.
    let package_bytes = package_root_bytes(root, manifest)?;
    let actual = package_digest_hex(&package_bytes);
    if actual != manifest.package_digest().as_str()["sha256:".len()..] {
        return Err(PackageFailure::ChecksumMismatch {
            domain: "package".to_owned(),
            identity,
        });
    }
    Ok(())
}

/// The canonical byte set of one package root, in the manifest's
/// declared (sorted) file order.
fn package_root_bytes(
    root: &Path,
    manifest: &ManifestDocument,
) -> Result<Vec<Vec<u8>>, PackageFailure> {
    let mut parts = Vec::with_capacity(manifest.files().len());
    for file in manifest.files() {
        let path = root.join(file.path().replace('/', std::path::MAIN_SEPARATOR_STR));
        let mut bytes = std::fs::read(&path).map_err(|_| PackageFailure::ChecksumMismatch {
            domain: "file".to_owned(),
            identity: file.path().to_owned(),
        })?;
        let mut part = Vec::with_capacity(file.path().len() + bytes.len() + 16);
        part.extend_from_slice(file.path().as_bytes());
        part.push(0);
        part.extend_from_slice(&file.bytes().to_be_bytes());
        part.push(0);
        part.extend_from_slice(&bytes);
        parts.push(part);
    }
    // The manifest's own contribution: canonical bytes minus the
    // self-referential manifestDigest member with packageDigest
    // zeroed — the same exclusion the JS generator applies. This is
    // the normative domain of ADR-0042; one definition, both sides.
    parts.push(framed_manifest_part(manifest.digest_domain_bytes()));
    Ok(parts)
}

/// Frame the implicit manifest self-contribution.
fn framed_manifest_part(canonical_manifest: Vec<u8>) -> Vec<u8> {
    let mut part = Vec::with_capacity(MANIFEST_FILE.len() + canonical_manifest.len() + 16);
    part.extend_from_slice(MANIFEST_FILE.as_bytes());
    part.push(0);
    part.extend_from_slice(&canonical_manifest.len().to_be_bytes());
    part.push(0);
    part.extend_from_slice(&canonical_manifest);
    part
}

/// The package digest over the canonical byte set.
pub(crate) fn package_digest_hex(parts: &[Vec<u8>]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    for part in parts {
        hasher.update(part.len().to_be_bytes());
        hasher.update(part);
    }
    crate::digest::hex(&hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest_of(bytes: &[u8]) -> String {
        crate::digest::sha256_hex(bytes)
    }

    fn manifest_json(
        entry_bytes: &[u8],
        entry_digest: &str,
        package_digest: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "integrity-adapter", "name": "Integrity", "version": "1.0.0" },
            "source": { "kind": "path", "coordinate": "path:fixtures/x", "digest": format!("sha256:{}", "44".repeat(32)) },
            "compatibility": {
                "protocolVersions": [crate::target_protocol::version::VERSION],
                "irVersions": [crate::ir::version::VERSION],
                "extensions": []
            },
            "executable": { "entry": "adapter.mjs" },
        "publisher": { "id": "test-pub", "trustAnchor": "none" },
        "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
        "platforms": ["any"],
        "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
        "hooks": [],
        "conformance": { "reportDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "integrity": {
                "packageDigest": package_digest,
                "files": [ { "path": "adapter.mjs", "digest": entry_digest, "bytes": entry_bytes.len() } ],
                "signaturePolicy": "unsigned",
                "signature": null
            },
            "status": "active",
            "revocation": null
        })
    }

    fn package_digest_for(
        root: &Path,
        entry_rel: &str,
        entry_digest: &str,
        bytes_len: u64,
    ) -> String {
        let mut part = Vec::new();
        part.extend_from_slice(entry_rel.as_bytes());
        part.push(0);
        part.extend_from_slice(&bytes_len.to_be_bytes());
        part.push(0);
        let bytes = std::fs::read(root.join(entry_rel)).expect("entry bytes");
        assert_eq!(format!("sha256:{}", digest_of(&bytes)), entry_digest);
        part.extend_from_slice(&bytes);
        // The normative domain appends the implicit manifest part:
        // canonical bytes minus manifestDigest, packageDigest zeroed.
        let stored = std::fs::read(root.join(MANIFEST_FILE)).expect("manifest on disk");
        let parsed =
            crate::adapter_package::ManifestDocument::from_bytes(&stored).expect("manifest parses");
        let mut manifest_part = Vec::new();
        manifest_part.extend_from_slice(MANIFEST_FILE.as_bytes());
        manifest_part.push(0);
        manifest_part.extend_from_slice(&parsed.digest_domain_bytes().len().to_be_bytes());
        manifest_part.push(0);
        manifest_part.extend_from_slice(&parsed.digest_domain_bytes());
        format!("sha256:{}", package_digest_hex(&[part, manifest_part]))
    }

    #[test]
    fn a_flipped_byte_refuses_the_package() {
        let root = std::env::temp_dir().join(format!("lekalo-ap-integrity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let entry = root.join("adapter.mjs");
        std::fs::write(&entry, b"export const x = 1;\n").expect("write");
        let entry_digest = format!("sha256:{}", digest_of(b"export const x = 1;\n"));
        // The manifest must be on disk first: the normative package
        // digest domain includes its implicit self-contribution.
        let provisional = serde_json::to_vec(&manifest_json(
            b"export const x = 1;\n",
            &entry_digest,
            &format!("sha256:{}", "0".repeat(64)),
        ))
        .unwrap();
        std::fs::write(root.join(MANIFEST_FILE), &provisional).expect("manifest");
        let package_digest = package_digest_for(
            &root,
            "adapter.mjs",
            &entry_digest,
            b"export const x = 1;\n".len() as u64,
        );
        let manifest_bytes = serde_json::to_vec(&manifest_json(
            b"export const x = 1;\n",
            &entry_digest,
            &package_digest,
        ))
        .unwrap();
        std::fs::write(root.join(MANIFEST_FILE), &manifest_bytes).expect("manifest");
        let manifest = ManifestDocument::from_bytes(&manifest_bytes).expect("manifest parses");
        let candidate = super::DiscoveryCandidate {
            manifest,
            package_root: Some(root.clone()),
            synthesized: false,
        };
        assert!(verify_package(&candidate).is_ok(), "honest package passes");
        // Flip one byte.
        std::fs::write(&entry, b"export const x = 2;\n").expect("tamper");
        let error = verify_package(&candidate).expect_err("tampered package refuses");
        assert!(matches!(error, PackageFailure::ChecksumMismatch { .. }));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_file_refuses_with_the_file_domain() {
        let root =
            std::env::temp_dir().join(format!("lekalo-ap-integrity-miss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let entry_digest = format!("sha256:{}", digest_of(b"gone"));
        let package_digest = format!("sha256:{}", "55".repeat(32));
        let manifest_bytes =
            serde_json::to_vec(&manifest_json(b"gone", &entry_digest, &package_digest)).unwrap();
        let manifest = ManifestDocument::from_bytes(&manifest_bytes).expect("manifest parses");
        let candidate = super::DiscoveryCandidate {
            manifest,
            package_root: Some(root.clone()),
            synthesized: false,
        };
        let error = verify_package(&candidate).expect_err("missing file refuses");
        match error {
            PackageFailure::ChecksumMismatch { domain, .. } => assert_eq!(domain, "file"),
            other => panic!("unexpected: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn synthesized_candidates_verify_the_entry_binding() {
        let manifest_json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "local-x", "name": "x", "version": "0.0.0" },
            "source": { "kind": "path", "coordinate": "path:x.mjs", "digest": format!("sha256:{}", "66".repeat(32)) },
            "compatibility": {
                "protocolVersions": [crate::target_protocol::version::VERSION],
                "irVersions": [crate::ir::version::VERSION],
                "extensions": []
            },
            "executable": { "entry": "x.mjs" },
        "publisher": { "id": "test-pub", "trustAnchor": "none" },
        "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
        "platforms": ["any"],
        "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
        "hooks": [],
        "conformance": { "reportDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "integrity": {
                "packageDigest": format!("sha256:{}", digest_of(b"bytes")),
                "files": [ { "path": "x.mjs", "digest": format!("sha256:{}", digest_of(b"bytes")), "bytes": 5 } ],
                "signaturePolicy": "unsigned",
                "signature": null
            },
            "status": "active",
            "revocation": null
        });
        let manifest = ManifestDocument::from_value(manifest_json).expect("parses");
        let candidate = super::DiscoveryCandidate {
            manifest,
            package_root: None,
            synthesized: true,
        };
        assert!(verify_package(&candidate).is_ok());
    }
}

#[cfg(test)]
mod committed_exemplar_tests {
    use super::*;

    /// The cross-check both reviews demanded: the Rust verifier accepts
    /// exactly what the JS generator (scripts/regen-adapter-manifest.mjs)
    /// emits for the shipped exemplar. If the two digest domains ever
    /// diverge again, this fails (issue #32 fix round 1, finding F-2).
    #[test]
    fn the_committed_exemplar_clears_the_integrity_gate() {
        let root = std::env::temp_dir().join(format!(
            "lekalo-ap-exemplar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("mkdir");
        let manifest_source =
            include_bytes!("../../../../adapters/node-typescript/adapter.manifest.json");
        let document = crate::adapter_package::ManifestDocument::from_bytes(manifest_source)
            .expect("committed manifest parses");
        for file in document.files() {
            let bytes = include_bytes!(concat!(
                "../../../../adapters/node-typescript/",
                "adapter.mjs"
            ));
            let _ = bytes;
            break; // the only payload file; the real copy happens below
        }
        // Copy the committed payload into the temp package root.
        let payload = include_bytes!("../../../../adapters/node-typescript/adapter.mjs");
        std::fs::write(root.join("adapter.mjs"), payload).expect("payload");
        std::fs::write(root.join(MANIFEST_FILE), manifest_source).expect("manifest");
        let candidate = crate::adapter_package::discovery::DiscoveryCandidate {
            manifest: document,
            package_root: Some(root.clone()),
            synthesized: false,
        };
        verify_package(&candidate).expect("the shipped exemplar passes verify_package");
        let _ = std::fs::remove_dir_all(&root);
    }
}
