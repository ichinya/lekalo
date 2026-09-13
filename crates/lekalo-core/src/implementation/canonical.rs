//! Deterministic canonical serialization of the implementation
//! attachment (issue #30).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, no BOM, object keys
//! in unsigned UTF-8 byte order, set-like collections in their
//! normalized sorted order. Two runs over one attachment produce
//! byte-identical output on every host. The payload bound is the
//! owner-approved v1 constant; exceeding it rejects instead of
//! truncating. The IR digest helper pins the exact canonical bytes the
//! compiled project produced, so an attachment can only answer to one
//! IR revision.

use super::version::MAX_CANONICAL_BYTES;
use super::ImplementationDocument;
use crate::diagnostics::DiagnosticSet;
use crate::implementation::diagnostic;
use crate::ir::CompiledProject;
use crate::lockfile::types::Sha256Digest;
use crate::versioning::plan::sha256_hex;

/// The canonical compact JSON bytes, or the payload-bound rejection.
pub(crate) fn to_canonical_json(
    document: &ImplementationDocument,
) -> Result<String, DiagnosticSet> {
    let bytes = serde_json::to_vec(&canonical_value(document))
        .map_err(|_| diagnostic::document_invalid("canonical-bytes", None))?;
    if bytes.len() > MAX_CANONICAL_BYTES {
        return Err(diagnostic::document_invalid("canonical-bytes", None));
    }
    String::from_utf8(bytes).map_err(|_| diagnostic::document_invalid("canonical-bytes", None))
}

/// The SHA-256 digest of one attachment's canonical bytes.
pub(crate) fn digest(document: &ImplementationDocument) -> Result<Sha256Digest, DiagnosticSet> {
    digest_value(&canonical_value(document))
}

/// Digest the canonical value; shared by attachment and IR pinning.
fn digest_value(value: &serde_json::Value) -> Result<Sha256Digest, DiagnosticSet> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| diagnostic::document_invalid("canonical-bytes", None))?;
    if bytes.len() > MAX_CANONICAL_BYTES {
        return Err(diagnostic::document_invalid("canonical-bytes", None));
    }
    Ok(Sha256Digest::from_hex(&sha256_hex(&bytes)))
}

/// The SHA-256 digest over one compiled project's canonical IR bytes:
/// the exact revision pin every attachment binds.
pub(crate) fn ir_digest(project: &CompiledProject) -> Sha256Digest {
    Sha256Digest::from_hex(&sha256_hex(project.to_canonical_json().as_bytes()))
}

/// The canonical JSON value: serde's default map is byte-sorted, and
/// the document already normalizes set-like collections on decode.
fn canonical_value(document: &ImplementationDocument) -> serde_json::Value {
    serde_json::json!({
        "contracts": document.contracts.iter().map(|contract| {
            serde_json::json!({
                "contract": contract.contract,
                "effects": contract.effects,
                "symbol": contract.symbol,
                "targets": contract.targets.iter().map(|binding| {
                    let mut target = serde_json::Map::new();
                    target.insert("kind".to_owned(), serde_json::json!(binding.kind.as_str()));
                    target.insert("target".to_owned(), serde_json::json!(binding.target));
                    if let Some(symbol) = &binding.symbol {
                        target.insert("symbol".to_owned(), serde_json::json!(symbol));
                    }
                    if let Some(port) = &binding.port {
                        target.insert("port".to_owned(), serde_json::json!(port));
                    }
                    if binding.selected {
                        target.insert("selected".to_owned(), serde_json::json!(true));
                    }
                    serde_json::Value::Object(target)
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "identity": super::version::IDENTITY,
        "irRef": {
            "digest": document.ir_digest.as_str(),
            "identity": crate::ir::IDENTITY,
        },
        "modelRef": {
            "digest": document.model_ref.digest.as_str(),
            "modelVersion": document.model_ref.version.as_str(),
        },
        "projectId": document.project_id.as_str(),
        "schemaVersion": super::version::SCHEMA_VERSION,
    })
}
