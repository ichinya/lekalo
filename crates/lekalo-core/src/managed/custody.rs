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
    AdapterRef, ArtifactEntry, ArtifactKey, ArtifactKind, ArtifactManifest, ArtifactPath,
    Lifecycle, LockRevision, ProjectRef, SemanticOwnerId, SourceMapBinding, SourceRange,
    MANIFEST_DIR, MANIFEST_NAME,
};
use crate::digest::sha256_hex;
use crate::lockfile::types::{ArtifactPin, Platform};
use crate::lockfile::{ComponentId, ComponentRef, ContractPin, SemVer, Sha256Digest};
use std::path::Path;

fn invalid(reason: &str) -> DomainResult {
    super::diagnostic::failure(INPUT_INVALID, &detail(reason))
}

/// The project-relative logical coordinate of the governed ownership
/// manifest (the one established artifact-owner path, never a parallel
/// manifest).
pub const MANIFEST_LOGICAL: &str = ".lekalo/generated/manifests/ownership.json";

/// Retain one generation's exact published manifest bytes under the
/// custody home (`managed/manifests/<generation hex>`), so rollback can
/// restore the destination's own governed manifest (finding 9).
/// Idempotent for identical bytes only (r6, devin F4): a different
/// document already sitting at a retained-manifest name — a pre-planted
/// forged snapshot the rollback/recovery paths would otherwise treat as
/// the authoritative restoration source — is a conflict, never a silent
/// adoption. The retained snapshot of a generation is deterministic, so
/// different bytes under one generation name are a writer's plant or
/// divergence, in both directions refused.
pub fn retain_manifest(root: &Path, generation: &str, bytes: &[u8]) -> Result<(), DomainResult> {
    let hex = generation.strip_prefix("sha256:").unwrap_or(generation);
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(invalid("generation-digest"));
    }
    let logical = super::history::custody_logical(&format!("manifests/{hex}"));
    if let Ok(Some(existing)) = super::fs::read_file(root, &logical) {
        if super::digest(&existing) == super::digest(bytes) {
            return Ok(());
        }
        return Err(invalid("manifest-conflict"));
    }
    super::fs::write_file(root, &logical, bytes).map_err(|_| invalid("manifest-retain"))
}

/// Read one generation's retained manifest snapshot, fully verified as a
/// closed manifest document.
pub fn retained_manifest(root: &Path, generation: &str) -> Result<Vec<u8>, DomainResult> {
    let hex = generation.strip_prefix("sha256:").unwrap_or(generation);
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(invalid("generation-digest"));
    }
    let logical = super::history::custody_logical(&format!("manifests/{hex}"));
    match super::fs::read_file(root, &logical) {
        Ok(Some(bytes)) => ArtifactManifest::parse_canonical(&bytes)
            .map(|_| bytes)
            .map_err(|_| invalid("manifest-read")),
        Ok(None) => Err(invalid("manifest-retained")),
        Err(_) => Err(invalid("manifest-read")),
    }
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
    record: &super::wire::GenerationRecord,
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
    // The locked adapter identity rides every entry (finding 4): the
    // governed manifest binds each artifact to the exact locked adapter
    // package and its launched-entry artifact pin, so `generate --check`
    // verifies the binding instead of reporting an unbound artifact.
    let adapter_ref = authority.adapter.as_ref().and_then(|adapter| {
        let id = crate::lockfile::ComponentId::parse(&adapter.id).ok()?;
        let version = crate::lockfile::SemVer::parse(&adapter.version).ok()?;
        let package = Sha256Digest::parse(adapter.digest.as_str()).ok()?;
        let entry = Sha256Digest::parse(adapter.entry_digest.as_str()).ok()?;
        let protocol = crate::lockfile::SemVer::parse(
            authority
                .lock_protocol
                .as_deref()
                .unwrap_or(super::PROTOCOL),
        )
        .ok()?;
        AdapterRef::new(
            id,
            version,
            package,
            vec![ArtifactPin::new(Platform::parse("any").ok()?, entry)],
            Some(protocol),
        )
    });
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
            adapter_ref.clone(),
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
    // coordinates the plan deleted are dropped, never resurrected. Carried
    // source maps are rebound to this manifest's exact inputs revision
    // (finding 12): the manifest is the whole-project custody statement,
    // and a carried artifact's bytes are proven unchanged by the bijection
    // below, so a scoped Model change must not leave sibling maps pinned
    // to a superseded revision the header no longer names.
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
    source_maps.extend(
        carried_maps
            .into_iter()
            .filter(|binding| {
                !scoped.contains(binding.key().path().as_str())
                    && !deleted.contains(binding.key().path().as_str())
            })
            .map(|binding| {
                SourceMapBinding::new(
                    binding.key().clone(),
                    revision.clone(),
                    binding.entries().to_vec(),
                )
            }),
    );
    // Bijection with the committed record (finding 7): the manifest must
    // be exactly the custody statement of the generation this transaction
    // commits — the caller passes the assembled record. Verified before
    // any byte is written.
    verify_bijection(record, &entries)?;
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

/// Write the ownership manifest atomically inside the governed home, fully
/// confined (finding 9): the stage file is created fresh (`create_new`, so a
/// hardlink planted at the stage name is never truncated) and renamed over
/// the manifest coordinate, and the whole ancestor chain is revalidated so
/// no managed write escapes the project.
fn write_manifest(root: &Path, bytes: &[u8]) -> Result<(), DomainResult> {
    restore_manifest_bytes(root, bytes)
}

/// Restore one exact manifest snapshot over the public manifest coordinate,
/// fully confined (finding 9). Shared by publication, rollback, and
/// recovery: the stage leaf is created fresh (`create_new`, so a hardlink
/// planted at the stage name is never truncated), the rename replaces the
/// destination entry atomically (never a truncate-through on a shared
/// inode), and the whole ancestor chain is revalidated by the confined
/// capability so no managed write escapes the project.
pub(crate) fn restore_manifest_bytes(root: &Path, bytes: &[u8]) -> Result<(), DomainResult> {
    // The bytes must be a closed manifest document before any byte moves.
    ArtifactManifest::parse_canonical(bytes).map_err(|_| invalid("manifest"))?;
    if super::fs::ensure_dir(root, MANIFEST_DIR).is_err() {
        return Err(invalid("manifest-write"));
    }
    let stage = format!("{MANIFEST_DIR}/ownership.json.stage");
    super::fs::write_staged(root, MANIFEST_LOGICAL, &stage, bytes)
        .map_err(|_| invalid("manifest-write"))?;
    // Read back through the confined capability: the published bytes are
    // the staged bytes.
    match super::fs::read_file(root, MANIFEST_LOGICAL) {
        Ok(Some(published)) if published == bytes => Ok(()),
        _ => Err(invalid("manifest-write")),
    }
}

/// Preflight the manifest destination (finding 10): the governed manifests
/// directory must be creatable and the manifest leaf must be either absent
/// or a plain writable file — never a directory, link, hardlink alias, or
/// other alias. A blocked destination fails the run before the transaction
/// spends any staged state, and recovery can complete the journal
/// afterwards.
pub(crate) fn preflight_destination(root: &Path) -> Result<(), DomainResult> {
    let directory = root.join(MANIFEST_DIR);
    std::fs::create_dir_all(&directory).map_err(|_| invalid("manifest-destination"))?;
    let physical = directory.join(MANIFEST_NAME);
    match std::fs::symlink_metadata(&physical) {
        Ok(metadata)
            if metadata.is_dir()
                || metadata.is_symlink()
                || super::fs::is_shared_alias(&physical, &metadata) =>
        {
            return Err(invalid("manifest-destination"))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(invalid("manifest-destination")),
    }
    // The staging leaf must be absent or a plain single-link file (finding
    // 9): a shared inode planted at the stage name refuses the run here,
    // before any application byte moves, so the committed state stays
    // exactly where it was.
    let stage = directory.join("ownership.json.stage");
    match std::fs::symlink_metadata(&stage) {
        Ok(metadata) if super::fs::is_shared_alias(&stage, &metadata) => {
            return Err(invalid("manifest-destination"))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(invalid("manifest-destination")),
    }
    Ok(())
}

/// Read the previously published governed manifest, if any. The read goes
/// through the confined capability (r4 finding 7): the ancestor chain and
/// the leaf are revalidated at read time, so a swapped-in symlink or
/// junction between the preflight and this read fails closed instead of
/// merging outside-manifest bytes into the new union manifest.
fn read_manifest(root: &Path) -> Result<Option<ArtifactManifest>, DomainResult> {
    let bytes = match super::fs::read_file(root, MANIFEST_LOGICAL) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Ok(None),
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
/// The committed custody record being written must biject with the
/// published manifest (finding 7): every recorded output of the new
/// generation carries exactly one manifest entry at the same path, and
/// every entry names a recorded output. Unaffected modules survive;
/// nothing is dropped or invented. The caller passes the exact outputs
/// of the record this transaction is committing (plan outputs that
/// survive deletion plus every preserved output).
fn verify_bijection(
    record: &super::wire::GenerationRecord,
    entries: &[ArtifactEntry],
) -> Result<(), DomainResult> {
    let recorded: std::collections::BTreeMap<&str, &str> = record
        .outputs
        .iter()
        .map(|output| (output.path.as_str(), output.digest.as_str()))
        .collect();
    let published: std::collections::BTreeMap<&str, &str> = entries
        .iter()
        .map(|entry| (entry.key().path().as_str(), entry.content().as_str()))
        .collect();
    if recorded != published {
        return Err(super::diagnostic::failure(
            super::ARTIFACT_COVERAGE_INCOMPLETE,
            &detail("manifest-bijection"),
        ));
    }
    Ok(())
}
