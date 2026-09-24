//! The adapter catalog bridge (issue #91).
//!
//! One invocation-supplied adapter program is discovered through the safe
//! describe handshake and bound to the committed lock: the discovered
//! identity must equal a locked adapter exactly (id, canonical version,
//! package digest, and the digest over the launched entry bytes), or the
//! run refuses. The same discovery feeds the sealed `#10` candidate
//! supply for `lekalo lock`, so committing an adapter pin is always an
//! explicit user command and never a hidden generate side effect.
//!
//! Single-entry project adapters (the shipped convention): the adapter
//! package is one committed project-relative entry, so the package
//! digest, the source-snapshot digest, and the launched-entry digest are
//! the same SHA-256 over those exact bytes.

use std::path::{Path, PathBuf};

use crate::adapter_package::budget::{ExpansionPolicy, SessionBudget};
use crate::lockfile::resolution::{CandidateAdapter, CandidateSet};
use crate::lockfile::types::{
    ArtifactPin, ComponentId, Platform, SemVer, Sha256Digest, SourceKind, SourceRef,
};
use crate::lockfile::{Lockfile, ResolvedAdapter};
use crate::target_protocol::discovery::{DiscoveredAdapter, Discovery};
use crate::target_protocol::transport::{AdapterCommand, TransportLimits};
use crate::target_protocol::TargetClient;
use crate::versioning::compatibility::{AdapterCompatibilityManifest, ProtocolBounds};
use crate::versioning::family::{IrContract, ProtocolContract, RegistryContract};
use crate::versioning::ContractVersion;

use super::Failure;

/// One invocation-supplied adapter program, addressed by a #4-safe
/// project-relative logical path.
#[derive(Clone, Debug)]
pub struct AdapterSupply {
    /// The exact process vector, launched without a shell.
    pub command: AdapterCommand,
    /// The project-relative logical path of the launched entry
    /// (forward slashes, portable segments).
    pub source_id: String,
}

impl AdapterSupply {
    /// Bind one adapter program: the launched entry (the executable, or
    /// its first argument when that names an existing regular file — the
    /// interpreter-script convention shared with discovery) must live
    /// inside the project root, so the lock can carry a project source
    /// coordinate and never an absolute path or host spelling.
    pub fn new(root: &Path, program: &str, args: Vec<String>) -> Result<Self, Failure> {
        if program.is_empty() {
            return Err(Failure::AdapterSupplyRequired);
        }
        let program_path = PathBuf::from(program);
        let entry = args
            .first()
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .unwrap_or_else(|| program_path.clone());
        let canonical_entry =
            std::fs::canonicalize(&entry).map_err(|_| Failure::AdapterSupplyRequired)?;
        let canonical_root =
            std::fs::canonicalize(root).map_err(|_| Failure::AdapterSupplyRequired)?;
        let relative = canonical_entry
            .strip_prefix(&canonical_root)
            .map_err(|_| Failure::AdapterSupplyRequired)?;
        if relative.as_os_str().is_empty() {
            return Err(Failure::AdapterSupplyRequired);
        }
        let source_id = relative.to_string_lossy().replace('\\', "/");
        if crate::project_fs::path_violation(&source_id).is_some() {
            return Err(Failure::AdapterSupplyRequired);
        }
        Ok(Self {
            command: AdapterCommand {
                program: program_path,
                args,
            },
            source_id,
        })
    }
}

/// Discover one adapter through the safe describe handshake.
///
/// Issue #32: the launched entry first passes the adapter package
/// resolution gate — the implicit local-development descriptor is
/// synthesized from the entry bytes and the integrity/trust gates run
/// before any child process exists (checksum before execution,
/// describe included).
pub fn discover(
    client: &mut TargetClient,
    supply: &AdapterSupply,
    root: &Path,
    limits: TransportLimits,
) -> Result<DiscoveredAdapter, Failure> {
    discover_with_policy(client, supply, root, limits, ExpansionPolicy::Refuse)
}

/// [`discover`] with an explicit escalation policy (issue #89): the
/// caller may permit described scopes wider than the manifest ceiling
/// only through this explicit policy spelling; the widening stays
/// visible in the session's confinement evidence.
pub fn discover_with_policy(
    client: &mut TargetClient,
    supply: &AdapterSupply,
    root: &Path,
    limits: TransportLimits,
    policy: ExpansionPolicy,
) -> Result<DiscoveredAdapter, Failure> {
    let _ = limits;
    let (manifest, synthesized) = gate_supply(supply, root)?;
    // A synthesized/implicit descriptor claims nothing: it gets the
    // strict default budget, not its synthesized manifest's empty
    // permission block (which would cap describe to nothing).
    let budget = SessionBudget::budget_for(if synthesized { None } else { manifest.as_ref() })
        .map_err(Failure::AdapterPackage)?;
    client.set_budget(if policy == ExpansionPolicy::AllowEscalated {
        budget.with_expansion_allowed()
    } else {
        budget
    });
    let discovered = Discovery::run(client, &supply.command, root).map_err(Failure::Target)?;
    // The manifest-vs-describe consistency check (issue #32): describe
    // is self-assertion; the verified manifest is the independent
    // claim. Any disagreement refuses (adapter.manifest-mismatch).
    // Synthesized implicit descriptors are skipped: they claim nothing
    // (operations ["describe"], empty targets/profiles/scope) precisely
    // so describe supplies the truth — checking them would refuse every
    // healthy bare `-- PROGRAM` flow (fix round 2, cline F-1).
    if consistency_applies(&manifest, synthesized) {
        let manifest = manifest.as_ref().expect("armed implies a manifest");
        crate::adapter_package::consistency::check(manifest, &discovered).map_err(|mismatch| {
            Failure::AdapterPackage(crate::adapter_package::PackageFailure::ManifestMismatch {
                field: mismatch.as_str().to_owned(),
            })
        })?;
    }
    Ok(discovered)
}

/// Whether the manifest-vs-describe consistency check applies: only a
/// real (non-synthesized) manifest is an independent claim. The
/// implicit descriptor claims nothing — describe supplies truth.
fn consistency_applies(
    manifest: &Option<crate::adapter_package::ManifestDocument>,
    synthesized: bool,
) -> bool {
    manifest.is_some() && !synthesized
}

/// Run the issue #32 resolution gate over one invocation-supplied
/// supply. The project root scopes the revocation store; the gates are
/// offline-faithful (the implicit descriptor is fully local).
/// offline-faithful (the implicit descriptor is fully local). Returns the
/// verified manifest and whether it was **synthesized** (the implicit
/// local-development descriptor claims nothing — the consistency check
/// must not run against it, fix round 2 cline F-1).
fn gate_supply(
    supply: &AdapterSupply,
    root: &Path,
) -> Result<(Option<crate::adapter_package::ManifestDocument>, bool), Failure> {
    let entry = supply
        .command
        .args
        .first()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .unwrap_or_else(|| supply.command.program.clone());
    // Prefer a real manifested package: the entry directory may carry
    // adapter.manifest.json (a manifested path supply).
    let entry_dir = entry
        .parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let manifested = if entry_dir.join("adapter.manifest.json").is_file() {
        crate::adapter_package::discover(
            &crate::adapter_package::DiscoverySource::Path(entry_dir.clone()),
            Some(&root.to_path_buf()),
        )
        .map_err(package_failure)?
        .into_iter()
        .next()
    } else {
        None
    };
    let (candidate, synthesized) = match manifested {
        Some(candidate) => (candidate, false),
        None => (
            crate::adapter_package::implicit_local_development(&entry).map_err(package_failure)?,
            true,
        ),
    };
    let context = crate::adapter_package::ResolveContext {
        root: Some(root.to_path_buf()),
        offline: true,
    };
    let resolved =
        crate::adapter_package::resolve_candidate(candidate, &context).map_err(package_failure)?;
    Ok((Some(resolved.candidate.manifest), synthesized))
}

fn package_failure(failure: crate::adapter_package::PackageFailure) -> Failure {
    Failure::AdapterPackage(failure)
}

/// The locked adapter with the discovered identity, if any.
pub(crate) fn locked_adapter<'a>(
    lock: &'a Lockfile,
    discovered: &DiscoveredAdapter,
) -> Option<&'a ResolvedAdapter> {
    lock.adapters().iter().find(|adapter| {
        adapter.id().as_str() == discovered.adapter.id
            && adapter.version().to_string() == discovered.adapter.version
    })
}

/// Bind the discovered adapter to the locked pins: the verified digest
/// over the launched entry bytes must equal a locked platform artifact
/// pin exactly — the launched bytes are proven to be the locked bytes.
/// The declared package digest stays recorded evidence, never the
/// binding itself; a version-only identity match is invalid by the
/// exact version comparison above.
pub(crate) fn binding_failure(
    locked: &ResolvedAdapter,
    discovered: &DiscoveredAdapter,
) -> Result<Sha256Digest, Failure> {
    let executable = discovered
        .executable_digest
        .as_deref()
        .map(Sha256Digest::parse)
        .transpose()
        .map_err(|_| Failure::AdapterSupplyRequired)?
        .ok_or(Failure::AdapterSupplyRequired)?;
    let pinned = locked
        .artifacts()
        .iter()
        .any(|artifact| artifact.digest().as_str() == executable.as_str());
    if pinned {
        Ok(executable)
    } else {
        Err(Failure::AdapterDigestMismatch {
            adapter: locked.id().as_str().to_owned(),
        })
    }
}

/// Build the sealed `#10` candidate supply from one discovery outcome:
/// the adapter package with a platform-neutral artifact pin and the
/// typed compatibility manifest derived from the declared versions. The
/// bridge pins adapter identity only; generator, profile, and catalog
/// supply arrive with the provider milestones and stay out of v1.
pub fn candidate_supply(
    discovered: &DiscoveredAdapter,
    supply: &AdapterSupply,
) -> Result<CandidateSet, Failure> {
    let executable = discovered
        .executable_digest
        .as_deref()
        .map(Sha256Digest::parse)
        .transpose()
        .map_err(|_| Failure::AdapterSupplyRequired)?
        .ok_or(Failure::AdapterSupplyRequired)?;
    let source = SourceRef::new(SourceKind::Project, &supply.source_id, executable.clone())
        .map_err(Failure::Lock)?;
    let manifest = compatibility_manifest(discovered)?;
    let adapter = CandidateAdapter::new(
        ComponentId::parse(&discovered.adapter.id).map_err(Failure::Lock)?,
        SemVer::parse(&discovered.adapter.version).map_err(Failure::Lock)?,
        executable.clone(),
        source,
        vec![ArtifactPin::new(
            Platform::parse("any").map_err(Failure::Lock)?,
            executable,
        )],
        manifest,
    );
    Ok(CandidateSet::empty().with_adapter(adapter))
}

/// The typed `#9` compatibility manifest derived from the declared
/// versions: the IR range spans the declared IR versions (or exactly the
/// current contract on a legacy 0.2.16 session, whose compatibility the
/// lock governs), and the protocol range spans the declared protocol
/// versions. An adapter whose declared range cannot cover the requested
/// contract versions is refused by the accepted preflight at resolution
/// time — the bridge never widens a declared range.
fn compatibility_manifest(
    discovered: &DiscoveredAdapter,
) -> Result<AdapterCompatibilityManifest, Failure> {
    let ir_current = ContractVersion::<IrContract>::current();
    let (ir_min, ir_max) = if discovered.ir_versions.is_empty() {
        (ir_current.clone(), ir_current)
    } else {
        let mut spellings = discovered.ir_versions.clone();
        spellings.sort();
        let min = ContractVersion::<IrContract>::parse_canonical(&spellings[0])
            .map_err(|_| Failure::AdapterSupplyRequired)?;
        let max = ContractVersion::<IrContract>::parse_canonical(&spellings[spellings.len() - 1])
            .map_err(|_| Failure::AdapterSupplyRequired)?;
        (min, max)
    };
    let mut protocols = discovered.declared_protocols.clone();
    protocols.sort();
    let protocol_bounds = if protocols.is_empty() {
        None
    } else {
        let min = ContractVersion::<ProtocolContract>::parse_canonical(&protocols[0])
            .map_err(|_| Failure::AdapterSupplyRequired)?;
        let max =
            ContractVersion::<ProtocolContract>::parse_canonical(&protocols[protocols.len() - 1])
                .map_err(|_| Failure::AdapterSupplyRequired)?;
        Some(ProtocolBounds { min, max })
    };
    AdapterCompatibilityManifest::new(
        ContractVersion::<RegistryContract>::parse_canonical(
            crate::versioning::compatibility::MANIFEST_SCHEMA_VERSION,
        )
        .map_err(|_| Failure::AdapterSupplyRequired)?,
        discovered.adapter.id.clone(),
        ir_min,
        ir_max,
        protocol_bounds,
        Vec::new(),
        Vec::new(),
    )
    .map_err(|_| Failure::AdapterSupplyRequired)
}

#[cfg(test)]
mod gate_supply_tests {
    use super::*;

    fn fresh_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "lekalo-catalog-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos(),
        ));
        std::fs::create_dir_all(&root).expect("mkdir");
        root
    }

    fn supply_for(entry: &Path) -> AdapterSupply {
        AdapterSupply {
            command: AdapterCommand {
                program: PathBuf::from("node"),
                args: vec![entry.to_string_lossy().into_owned()],
            },
            source_id: entry.to_string_lossy().replace('\\', "/"),
        }
    }

    /// Regression (fix round 2, cline F-1): a bare `-- PROGRAM` supply
    /// synthesizes its descriptor — the consistency check must be
    /// skipped, so generate/verify stay green on spawn-capable hosts.
    #[test]
    fn synthesized_supply_skips_the_consistency_check() {
        let root = fresh_root("synth");
        let src = root.join("src");
        std::fs::create_dir_all(&src).expect("mkdir");
        let entry = src.join("adapter.mjs");
        std::fs::write(&entry, b"export default 1;").expect("entry");
        let supply = supply_for(&entry);
        let (manifest, synthesized) = gate_supply(&supply, &root).expect("gate");
        assert!(synthesized, "no adapter.manifest.json beside the entry");
        assert!(
            !consistency_applies(&manifest, synthesized),
            "synthesized descriptors are never consistency-checked",
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A manifested path supply keeps the consistency check armed.
    #[test]
    fn manifested_supply_keeps_the_consistency_check() {
        let root = fresh_root("man");
        let pkg = root.join("pkg");
        std::fs::create_dir_all(&pkg).expect("mkdir");
        let entry = pkg.join("a.mjs");
        std::fs::write(&entry, b"export default 1;").expect("entry");
        let manifest_json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "manifested-adapter", "name": "M", "version": "1.0.0" },
            "publisher": { "id": "p", "trustAnchor": "none" },
            "source": { "kind": "path", "coordinate": "path:pkg", "digest": format!("sha256:{}", "11".repeat(32)) },
            "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": { "protocolVersions": [crate::target_protocol::version::VERSION], "irVersions": [crate::ir::version::VERSION], "extensions": [] },
            "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
            "executable": { "runtime": { "kind": "node", "minVersion": "18.0.0" }, "entry": "a.mjs", "argvPreview": ["node", "a.mjs"], "assets": [] },
            "platforms": ["any"],
            "integrity": { "packageDigest": format!("sha256:{}", "0".repeat(64)), "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", crate::digest::sha256_hex(b"export default 1;")), "bytes": 17 } ], "signaturePolicy": "unsigned", "signature": null },
            "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
            "hooks": [],
            "conformance": { "reportDigest": format!("sha256:{}", "33".repeat(32)), "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "status": "active",
            "revocation": null
        });
        let provisional = serde_json::to_vec_pretty(&manifest_json).unwrap();
        std::fs::write(pkg.join("adapter.manifest.json"), &provisional).expect("manifest");
        let parsed = crate::adapter_package::ManifestDocument::from_bytes(&provisional)
            .expect("manifest parses");
        let entry_part = {
            let mut part = Vec::new();
            part.extend_from_slice(b"a.mjs");
            part.push(0);
            part.extend_from_slice(&(17u64).to_be_bytes());
            part.push(0);
            part.extend_from_slice(b"export default 1;");
            part
        };
        let manifest_part = {
            let mut part = Vec::new();
            part.extend_from_slice(crate::adapter_package::integrity::MANIFEST_FILE.as_bytes());
            part.push(0);
            part.extend_from_slice(&parsed.digest_domain_bytes().len().to_be_bytes());
            part.push(0);
            part.extend_from_slice(&parsed.digest_domain_bytes());
            part
        };
        let package_digest =
            crate::adapter_package::integrity::package_digest_hex(&[entry_part, manifest_part]);
        let mut final_manifest = manifest_json;
        final_manifest["integrity"]["packageDigest"] =
            serde_json::Value::String(format!("sha256:{package_digest}"));
        let manifest_bytes = serde_json::to_vec_pretty(&final_manifest).unwrap();
        std::fs::write(pkg.join("adapter.manifest.json"), &manifest_bytes).expect("manifest");
        let supply = supply_for(&entry);
        let (manifest, synthesized) = gate_supply(&supply, &root).expect("gate");
        assert!(!synthesized, "a manifested supply is a real claim");
        assert!(
            consistency_applies(&manifest, synthesized),
            "check stays armed"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
