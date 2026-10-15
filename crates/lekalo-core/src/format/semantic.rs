//! Semantic verification of formatting candidates (issue #94).
//!
//! The oracle never guesses: every candidate overlay is decoded through the
//! exact loader admission path, normalized through phases 7-8, and compiled
//! to the typed IR. Equal canonical Model bytes and equal canonical IR
//! bytes (the IR carries no source spans) prove a formatting-only change.

use crate::loader::LoadedDocument;
use crate::loader::ModelVersion;
use crate::result::DomainResult;

use super::failure_semantic;

/// Verify that replacing the given document bytes keeps the normalized
/// Model and the compiled IR byte-identical to the originals.
///
/// `originals` are the loader-pinned documents of the accepted load;
/// `replacements` name changed candidates by logical path. Every document
/// — replaced or not — is re-decoded, so a stale or malformed candidate
/// refuses the whole plan before any report or write exists.
pub(super) fn verify(
    originals: &[LoadedDocument],
    model_version: ModelVersion,
    replacements: &[(String, Vec<u8>)],
) -> Result<(), DomainResult> {
    let original_model = super::semantic_oracle(originals, model_version)
        .map_err(|_| failure_semantic("decode", "original-oracle-refused"))?;
    let mut overlay: Vec<LoadedDocument> = Vec::with_capacity(originals.len());
    for document in originals {
        let bytes = replacements
            .iter()
            .find(|(path, _)| *path == document.document.path)
            .map(|(_, bytes)| bytes.clone())
            .unwrap_or_else(|| document.bytes.clone());
        overlay.push(LoadedDocument {
            document: document.document.clone(),
            bytes,
        });
    }
    let candidate_model = super::semantic_oracle(&overlay, model_version)
        .map_err(|_| failure_semantic("redecode", "candidate-refused"))?;
    if original_model != candidate_model {
        return Err(failure_semantic("compare", "model-or-ir-mismatch"));
    }
    Ok(())
}
