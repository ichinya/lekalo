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
pub(crate) fn discover(
    client: &mut TargetClient,
    supply: &AdapterSupply,
    root: &Path,
    limits: TransportLimits,
) -> Result<DiscoveredAdapter, Failure> {
    let _ = limits;
    Discovery::run(client, &supply.command, root).map_err(Failure::Target)
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
/// current contract on a legacy 1.0.0 session, whose compatibility the
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
