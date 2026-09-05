//! Source-map validation (issue #21).
//!
//! A manifest source map maps semantic ids to half-open byte ranges inside
//! exactly one recorded artifact, bound to the exact inputs revision the
//! artifact was generated from. Mapping is traceability, never ownership
//! transfer: the source remains the owner of its generated code until an
//! explicit reviewed adoption.
//!
//! The parse gate already enforces the closed shape: unique artifact keys,
//! sorted non-overlapping ranges per semantic id, well-formed ids, and
//! `start < end`. Validation here adds the project-bound invariants: the
//! recorded input revision must equal the digest over the manifest's own
//! recorded inputs, and every range must lie inside the observed artifact
//! bytes (a missing artifact file is already a `missing` verdict, so its
//! ranges get only the revision check).

use super::check::{inputs_revision, observe, Prepared};
use super::types::{ArtifactManifest, Lifecycle};
use super::ArtifactFailure;

pub(crate) fn validate(
    manifest: &ArtifactManifest,
    prepared: &Prepared,
) -> Result<(), ArtifactFailure> {
    let revision = inputs_revision(manifest.model(), manifest.ir());
    for binding in manifest.source_maps() {
        // A foreign revision means the map was not produced by the
        // generator of this manifest.
        if binding.input_revision().as_str() != revision.as_str() {
            return Err(ArtifactFailure::SourceMapInvalid);
        }
        let entry = manifest
            .entry(binding.key())
            .ok_or(ArtifactFailure::SourceMapInvalid)?;
        // A map on a never-generated lifecycle carries no generated
        // ranges: adapters do not produce bytes for it.
        if entry.lifecycle() != Lifecycle::Generated && !binding.entries().is_empty() {
            return Err(ArtifactFailure::SourceMapInvalid);
        }
        if let Some(observed) = observe(prepared.fs(), entry.key().path().as_str())? {
            for range in binding.entries() {
                if range.end() as u64 > observed.size {
                    return Err(ArtifactFailure::SourceMapInvalid);
                }
            }
        }
    }
    Ok(())
}
