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
/// under the derived generation authority. Publication is part of the
/// confirmed transaction: a failure invalidates the run (the committed
/// generation stands, and recovery reconciles).
#[allow(clippy::too_many_arguments)]
pub fn publish(
    root: &Path,
    plan: &GenerationPlan,
    coverage: &Coverage,
    generated_root: &str,
    authority: &super::authority::Authority,
    inventory: &[Rendered],
) -> Result<(), DomainResult> {
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
    let model_semver =
        SemVer::parse(&authority.model_version).map_err(|_| invalid("ir-digest"))?;
    let ir_semver = SemVer::parse(crate::ir::version::VERSION).map_err(|_| invalid("ir-digest"))?;
    let model_pin = ContractPin::new(
        model_semver.clone(),
        Sha256Digest::parse(authority.model_digest.as_str())
            .map_err(|_| invalid("authority"))?,
    );
    let ir_pin = ContractPin::new(
        ir_semver.clone(),
        Sha256Digest::parse(authority.ir_digest.as_str()).map_err(|_| invalid("authority"))?,
    );
    let lock = LockRevision::new(crate::lockfile::LockDigest::new(
        Sha256Digest::parse(authority.lock_digest.as_str())
            .map_err(|_| invalid("lock-digest"))?,
    ));
    let revision = inputs_revision(&model_pin, &ir_pin);
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
    let manifest = ArtifactManifest::new(project, lock, model, ir, draft.artifacts().to_vec(), draft.source_maps().to_vec(), computed);
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
