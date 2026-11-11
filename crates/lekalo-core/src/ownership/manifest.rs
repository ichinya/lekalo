//! Handoffs update the established artifact manifest, never a parallel cleaner.
use super::*;
use crate::artifacts::types::{
    AdapterRef, ArtifactEntry, ArtifactKey, ArtifactKind, ArtifactManifest, ArtifactPath,
    Lifecycle, LockRevision, ProjectRef, SemanticOwnerId, SourceMapBinding,
};
use crate::lockfile::{ComponentId, ComponentRef, ContractPin, SemVer, Sha256Digest};
use std::path::Path;

pub(crate) fn prepare(
    root: &Path,
    claims: &[Claim],
    coverage: Option<&mg::Coverage>,
    authority: &mg::authority::Authority,
) -> Outcome<Vec<u8>> {
    let previous = read(root, mg::custody::MANIFEST_LOGICAL)?
        .map(|bytes| {
            ArtifactManifest::parse_canonical(&bytes)
                .map_err(|_| fail("ownership-artifact-manifest"))
        })
        .transpose()?;
    let model_version = crate::loader::ModelVersion::parse_exact(&authority.model_version)
        .ok_or_else(|| fail("ownership-model-version"))?;
    let project = ProjectRef::parse(&authority.project_id, model_version)
        .ok_or_else(|| fail("ownership-project"))?;
    let pin = |value: &str| Sha256Digest::parse(value).map_err(|_| fail("ownership-manifest-pin"));
    let model = ContractPin::new(
        SemVer::parse(&authority.model_version).map_err(|_| fail("ownership-model-version"))?,
        pin(&authority.model_digest)?,
    );
    let ir = ContractPin::new(
        SemVer::parse(crate::ir::version::VERSION).map_err(|_| fail("ownership-ir-version"))?,
        pin(&authority.ir_digest)?,
    );
    let lock = LockRevision::new(crate::lockfile::LockDigest::new(pin(
        &authority.lock_digest
    )?));
    if previous.as_ref().is_some_and(|old| {
        old.project().as_str() != project.as_str()
            || old.lock_ref().digest().as_str() != lock.digest().as_str()
    }) {
        return Err(fail("ownership-manifest-authority"));
    }
    let mut entries = previous
        .as_ref()
        .map(|old| old.artifacts().to_vec())
        .unwrap_or_default();
    for claim in claims {
        let old = entries
            .iter()
            .find(|entry| entry.key().path().as_str() == claim.binding.path)
            .cloned();
        let owner = SemanticOwnerId::parse(&claim.binding.symbol, model_version)
            .ok_or_else(|| fail("ownership-artifact-symbol"))?;
        let key = ArtifactKey::new(
            owner.clone(),
            ArtifactPath::parse(&claim.binding.path)
                .ok_or_else(|| fail("ownership-artifact-path"))?,
            if claim.binding.path.contains("/test/") {
                ArtifactKind::Test
            } else {
                ArtifactKind::Source
            },
        );
        let (adapter, generator) = if let Some(coverage) = coverage {
            let recipe_version = match coverage.recipe.version.split('.').count() {
                1 => format!("{}.0.0", coverage.recipe.version),
                2 => format!("{}.0", coverage.recipe.version),
                _ => coverage.recipe.version.clone(),
            };
            let recipe = ComponentRef::new(
                ComponentId::parse(&coverage.recipe.id).map_err(|_| fail("ownership-recipe"))?,
                SemVer::parse(&recipe_version).map_err(|_| fail("ownership-recipe"))?,
            );
            let adapter = authority
                .adapter
                .as_ref()
                .ok_or_else(|| fail("ownership-adapter-required"))?;
            let adapter = AdapterRef::new(
                ComponentId::parse(&adapter.id).map_err(|_| fail("ownership-adapter"))?,
                SemVer::parse(&adapter.version).map_err(|_| fail("ownership-adapter"))?,
                pin(&adapter.digest)?,
                vec![crate::lockfile::types::ArtifactPin::new(
                    crate::lockfile::types::Platform::parse("any")
                        .map_err(|_| fail("ownership-platform"))?,
                    pin(&adapter.entry_digest)?,
                )],
                Some(
                    SemVer::parse(authority.lock_protocol.as_deref().unwrap_or(mg::PROTOCOL))
                        .map_err(|_| fail("ownership-adapter-protocol"))?,
                ),
            );
            (adapter, Some(recipe))
        } else {
            let old = old
                .as_ref()
                .ok_or_else(|| fail("ownership-artifact-unclaimed"))?;
            (old.adapter().cloned(), old.generator().cloned())
        };
        entries.retain(|entry| entry.key().path().as_str() != claim.binding.path);
        entries.push(ArtifactEntry::new(
            key,
            if claim.lifecycle == "generated" {
                Lifecycle::Generated
            } else {
                Lifecycle::Scaffolded
            },
            adapter,
            generator,
            pin(&claim.digest)?,
            vec![owner],
        ));
    }
    entries.sort_by(|a, b| {
        (
            a.key().semantic_owner().as_str(),
            a.key().path().as_str(),
            a.key().kind().as_str(),
        )
            .cmp(&(
                b.key().semantic_owner().as_str(),
                b.key().path().as_str(),
                b.key().kind().as_str(),
            ))
    });
    let revision = crate::artifacts::check::inputs_revision(&model, &ir);
    let maps = previous
        .as_ref()
        .map(|old| {
            old.source_maps()
                .iter()
                .filter(|row| {
                    !claims
                        .iter()
                        .any(|claim| claim.binding.path == row.key().path().as_str())
                })
                .map(|row| {
                    SourceMapBinding::new(
                        row.key().clone(),
                        revision.clone(),
                        row.entries().to_vec(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let draft = ArtifactManifest::new(
        project.clone(),
        lock.clone(),
        model.clone(),
        ir.clone(),
        entries,
        maps,
        pin(&mg::digest(b""))?,
    );
    let digest = pin(&mg::digest(
        &crate::artifacts::canonical::digest_input_bytes(&draft),
    ))?;
    let manifest = ArtifactManifest::new(
        project,
        lock,
        model,
        ir,
        draft.artifacts().to_vec(),
        draft.source_maps().to_vec(),
        digest,
    );
    let bytes = manifest.canonical_bytes();
    ArtifactManifest::parse_canonical(&bytes).map_err(|_| fail("ownership-artifact-manifest"))?;
    Ok(bytes)
}
