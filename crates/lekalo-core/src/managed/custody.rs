//! Managed custody publication: the governed successor of the established
//! artifact ownership manifest (issue #41).
//!
//! A confirmed managed apply publishes the exact `.lekalo/generated/
//! manifests/ownership.json` the artifact owner already governs: every
//! output role becomes an entry with its semantic owner, exact bytes
//! digest, and generator reference; every rendered artifact carries a
//! source map over its full byte range bound to the exact generation
//! inputs. `lekalo generate --check` then verifies managed custody with
//! the same hermetic read-only gate it applies to every other artifact —
//! clean, drifted, missing, and orphaned alike. Publication never changes
//! what the managed lane writes: it is the custody statement about the
//! bytes the confirmed transaction just published.

use super::canonical::{output_paths, Rendered};
use super::wire::{ArtifactRole, Coverage, GenerationPlan};
use super::{detail, DomainResult, INPUT_INVALID};
use crate::artifacts::check::inputs_revision;
use crate::artifacts::types::{
    ArtifactEntry, ArtifactKey, ArtifactKind, ArtifactManifest, ArtifactPath, Lifecycle,
    LockRevision, ProjectRef, SemanticOwnerId, SourceMapBinding, SourceRange, MANIFEST_DIR,
    MANIFEST_NAME,
};
use crate::digest::sha256_hex;
use crate::lockfile::{ComponentId, ComponentRef, ContractPin, SemVer, Sha256Digest};
use std::path::Path;

fn invalid(reason: &str) -> DomainResult {
    super::diagnostic::failure(INPUT_INVALID, &detail(reason))
}

/// The project-relative logical coordinate of the governed ownership
/// manifest (the one established artifact-owner path, never a parallel
/// manifest).
pub const MANIFEST_LOGICAL: &str = ".lekalo/generated/manifests/ownership.json";

/// The closed artifact kind of one managed output role.
fn artifact_kind(role: ArtifactRole, path: &str) -> ArtifactKind {
    match role {
        ArtifactRole::Test => ArtifactKind::Test,
        ArtifactRole::Map => ArtifactKind::Data,
        ArtifactRole::Manifest | ArtifactRole::Custody => ArtifactKind::Docs,
        ArtifactRole::Implementation => {
            if path.ends_with(".map.json") {
                ArtifactKind::Data
            } else {
                ArtifactKind::Source
            }
        }
    }
}

/// Publish the ownership manifest for one committed managed generation
/// under the derived generation authority.
///
/// The governed manifest is the whole project's custody, not one scope's
/// (finding 7): every unaffected artifact entry and source map of the
/// previously published manifest is preserved verbatim, the scoped
/// generation's entries replace exactly their own coordinates, and the
/// result is verified as a bijection with the committed custody record
/// before any byte is written. Publication is part of the confirmed
/// transaction: a failure invalidates the run (the committed generation
/// stands, and recovery reconciles).
#[allow(clippy::too_many_arguments)]
pub fn publish(
    root: &Path,
    plan: &GenerationPlan,
    coverage: &Coverage,
    generated_root: &str,
    authority: &super::authority::Authority,
    inventory: &[Rendered],
) -> Result<(), DomainResult> {
    // Preflight (finding 10): an unwritable manifest destination refuses
    // before any staged byte is spent, and the caller journal covers the
    // retry.
    preflight_destination(root)?;
    let Some(model_version) = crate::loader::ModelVersion::parse_exact(&authority.model_version)
    else {
        return Err(invalid("model-version"));
    };
    let project = ProjectRef::parse(&authority.project_id, model_version)
        .ok_or_else(|| invalid("project-ref"))?;
    // The semantic owner of every output: the requirement symbol whose
    // declared output paths name the file.
    let mut owners: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for requirement in &coverage.requirements {
        if let Some((implementation, test)) =
            output_paths(generated_root, &coverage.module, requirement)
        {
            owners.insert(implementation, requirement.id.clone());
            owners.insert(test, requirement.id.clone());
        }
    }
    // The recipe reference rides the manifest as the generator identity:
    // the id is a closed component id, the version a canonical semver.
    let recipe_version = match coverage.recipe.version.split('.').count() {
        3 => coverage.recipe.version.clone(),
        1 => format!("{}.0.0", coverage.recipe.version),
        2 => format!("{}.0", coverage.recipe.version),
        _ => return Err(invalid("recipe")),
    };
    let generator = ComponentRef::new(
        ComponentId::parse(&coverage.recipe.id).map_err(|_| invalid("recipe"))?,
        SemVer::parse(&recipe_version).map_err(|_| invalid("recipe"))?,
    );
    // The exact input pins the manifest binds: the compiled Model and IR
    // instance digests plus their exact contract versions, and the exact
    // lock revision.
    let model_semver = SemVer::parse(&authority.model_version).map_err(|_| invalid("ir-digest"))?;
    let ir_semver = SemVer::parse(crate::ir::version::VERSION).map_err(|_| invalid("ir-digest"))?;
    let model_pin = ContractPin::new(
        model_semver.clone(),
        Sha256Digest::parse(authority.model_digest.as_str()).map_err(|_| invalid("authority"))?,
    );
    let ir_pin = ContractPin::new(
        ir_semver.clone(),
        Sha256Digest::parse(authority.ir_digest.as_str()).map_err(|_| invalid("authority"))?,
    );
    let lock = LockRevision::new(crate::lockfile::LockDigest::new(
        Sha256Digest::parse(authority.lock_digest.as_str()).map_err(|_| invalid("lock-digest"))?,
    ));
    let revision = inputs_revision(&model_pin, &ir_pin);
    // Whole-project custody (finding 7): the previously published manifest
    // donates every entry and source map the scoped generation does not
    // re-render; the scoped entries replace exactly their own coordinates.
    let previous = read_manifest(root)?;
    let mut carried_entries: Vec<ArtifactEntry> = Vec::new();
    let mut carried_maps: Vec<SourceMapBinding> = Vec::new();
    if let Some(previous) = &previous {
        // Only a manifest of the same governed project and lock revision
        // may donate entries: anything else is a foreign custody statement.
        if previous.project().as_str() == project.as_str()
            && previous.lock_ref().digest().as_str() == lock.digest().as_str()
        {
            carried_entries.extend(previous.artifacts().iter().cloned());
            carried_maps.extend(previous.source_maps().iter().cloned());
        }
    }
    let mut entries = Vec::new();
    let mut source_maps = Vec::new();
    for output in &plan.outputs {
        let Some(after) = output.after_digest.value() else {
            continue;
        };
        let Some(owner) = owners.get(&output.path) else {
            return Err(invalid("ownership-manifest")); // A
        };
        let semantic_owner =
            SemanticOwnerId::parse(owner, model_version).ok_or_else(|| invalid("owner-id"))?;
        let key = ArtifactKey::new(
            semantic_owner.clone(),
            ArtifactPath::parse(&output.path).ok_or_else(|| invalid("artifact-path"))?,
            artifact_kind(output.role, &output.path),
        );
        entries.push(ArtifactEntry::new(
            key.clone(),
            Lifecycle::Generated,
            None,
            Some(generator.clone()),
            Sha256Digest::parse(after).map_err(|_| invalid("output-digest"))?,
            vec![semantic_owner.clone()],
        ));
        // The managed source map of the artifact: its full rendered byte
        // range bound to the exact generation inputs.
        let bytes = inventory
            .iter()
            .find(|artifact| artifact.path == output.path)
            .map(|artifact| artifact.bytes.len())
            .unwrap_or(0);
        let end = u32::try_from(bytes).unwrap_or(u32::MAX);
        source_maps.push(SourceMapBinding::new(
            key,
            revision.clone(),
            vec![SourceRange::new(semantic_owner, 0, end)],
        ));
    }
    // The scoped coordinates replace the carried ones; carried entries at
    // coordinates the plan deleted are dropped, never resurrected.
    let scoped: std::collections::BTreeSet<String> = entries
        .iter()
        .map(|entry| entry.key().path().as_str().to_owned())
        .collect();
    let deleted: std::collections::BTreeSet<String> = plan
        .outputs
        .iter()
        .filter(|output| output.action == super::wire::OutputAction::Delete)
        .map(|output| output.path.clone())
        .collect();
    entries.extend(carried_entries.into_iter().filter(|entry| {
        !scoped.contains(entry.key().path().as_str())
            && !deleted.contains(entry.key().path().as_str())
    }));
    source_maps.extend(carried_maps.into_iter().filter(|binding| {
        !scoped.contains(binding.key().path().as_str())
            && !deleted.contains(binding.key().path().as_str())
    }));
    // Bijection with the committed record (finding 7): every manifest
    // entry must name a path the committed custody record carries, and
    // every recorded output must carry exactly one manifest entry, so no
    // unaffected artifact is lost and none is invented.
    verify_bijection(root, &entries, plan)?;
    entries.sort_by(|left, right| {
        (
            left.key().semantic_owner().as_str().to_owned(),
            left.key().path().as_str().to_owned(),
            left.key().kind().as_str().to_owned(),
        )
            .cmp(&(
                right.key().semantic_owner().as_str().to_owned(),
                right.key().path().as_str().to_owned(),
                right.key().kind().as_str().to_owned(),
            ))
    });
    source_maps.sort_by(|left, right| {
        (
            left.key().semantic_owner().as_str().to_owned(),
            left.key().path().as_str().to_owned(),
            left.key().kind().as_str().to_owned(),
        )
            .cmp(&(
                right.key().semantic_owner().as_str().to_owned(),
                right.key().path().as_str().to_owned(),
                right.key().kind().as_str().to_owned(),
            ))
    });
    let model = model_pin;
    let ir = ir_pin;
    let draft = ArtifactManifest::new(
        project.clone(),
        lock.clone(),
        model.clone(),
        ir.clone(),
        entries,
        source_maps,
        Sha256Digest::from_hex(&sha256_hex(b"")),
    );
    let computed = Sha256Digest::from_hex(&sha256_hex(
        &crate::artifacts::canonical::digest_input_bytes(&draft),
    ));
    let manifest = ArtifactManifest::new(
        project,
        lock,
        model,
        ir,
        draft.artifacts().to_vec(),
        draft.source_maps().to_vec(),
        computed,
    );
    let bytes = manifest.canonical_bytes();
    ArtifactManifest::parse_canonical(&bytes).map_err(|_| invalid("manifest"))?;
    write_manifest(root, &bytes)
}

/// Write the ownership manifest atomically inside the governed home.
fn write_manifest(root: &Path, bytes: &[u8]) -> Result<(), DomainResult> {
    let directory = root.join(MANIFEST_DIR);
    std::fs::create_dir_all(&directory).map_err(|_| invalid("manifest-write"))?;
    let physical = directory.join(MANIFEST_NAME);
    let stage = directory.join("ownership.json.tmp");
    std::fs::write(&stage, bytes).map_err(|_| invalid("manifest-write"))?;
    std::fs::rename(&stage, &physical).map_err(|_| invalid("manifest-write"))?;
    let read_back = std::fs::read(&physical).map_err(|_| invalid("manifest-write"))?;
    if read_back != bytes {
        return Err(invalid("manifest-write"));
    }
    Ok(())
}

/// Preflight the manifest destination (finding 10): the governed manifests
/// directory must be creatable and the manifest leaf must be either absent
/// or a plain writable file — never a directory, link, or other alias.
/// A blocked destination fails the run before the transaction spends any
/// staged state, and recovery can complete the journal afterwards.
fn preflight_destination(root: &Path) -> Result<(), DomainResult> {
    let directory = root.join(MANIFEST_DIR);
    std::fs::create_dir_all(&directory).map_err(|_| invalid("manifest-destination"))?;
    let physical = directory.join(MANIFEST_NAME);
    match std::fs::symlink_metadata(&physical) {
        Ok(metadata) if metadata.is_dir() || metadata.is_symlink() => {
            return Err(invalid("manifest-destination"))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(invalid("manifest-destination")),
    }
    Ok(())
}

/// Read the previously published governed manifest, if any.
fn read_manifest(root: &Path) -> Result<Option<ArtifactManifest>, DomainResult> {
    let physical = root.join(MANIFEST_DIR).join(MANIFEST_NAME);
    let bytes = match std::fs::read(&physical) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid("manifest-read")),
    };
    ArtifactManifest::parse_canonical(&bytes)
        .map(Some)
        .map_err(|_| invalid("manifest-read"))
}

/// The committed custody record must biject with the published manifest
/// (finding 7): every recorded output of the current generation carries
/// exactly one manifest entry at the same path, and every entry names a
/// recorded output. Unaffected modules survive; nothing is dropped or
/// invented.
fn verify_bijection(
    root: &Path,
    entries: &[ArtifactEntry],
    _plan: &GenerationPlan,
) -> Result<(), DomainResult> {
    let Some(current) = super::history::current(root).ok().flatten() else {
        return Ok(());
    };
    let record = super::history::record(root, &current)?;
    let recorded: std::collections::BTreeSet<&str> =
        record.outputs.iter().map(|o| o.path.as_str()).collect();
    let published: std::collections::BTreeSet<&str> = entries
        .iter()
        .map(|entry| entry.key().path().as_str())
        .collect();
    if recorded != published {
        return Err(super::diagnostic::failure(
            super::ARTIFACT_COVERAGE_INCOMPLETE,
            &detail("manifest-bijection"),
        ));
    }
    // Every entry's content digest must be the exact recorded bytes.
    for entry in entries {
        let output = record
            .outputs
            .iter()
            .find(|output| output.path == entry.key().path().as_str())
            .expect("bijection holds");
        if entry.content().as_str() != output.digest {
            return Err(super::diagnostic::failure(
                super::ARTIFACT_COVERAGE_INCOMPLETE,
                &detail("manifest-bijection"),
            ));
        }
    }
    Ok(())
}
