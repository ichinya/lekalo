//! Typed wire decoding of one detected-evidence envelope (issue #14).
//!
//! The envelope is the only way detected effects enter the public graph:
//! adapters and native batteries produce it, never the projection
//! itself. Decoding is closed and duplicate-safe: unknown members,
//! duplicate members, malformed identities/digests, unknown kinds or
//! trust states, and illegal kind/subject pairs each return one typed
//! registered diagnostic and no partial envelope.

use serde_json::Value as Json;

use super::diagnostic;
use super::identity::{OperationId, ResourceId, Subject, TransactionGroupId};
use super::kind::EffectKind;
use super::provenance::{validate_envelope, EvidenceEntry, EvidenceEnvelope, TrustState};

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "adapterId",
    "targetId",
    "protocolVersion",
    "evidenceDigest",
    "sourceRevision",
    "trust",
    "entries",
];

/// The closed entry member set.
const ENTRY_KEYS: &[&str] = &[
    "operation",
    "kind",
    "subject",
    "field",
    "occurrence",
    "trust",
    "transactionGroup",
];

/// Decode one detected-evidence envelope from duplicate-checked JSON
/// bytes, or return the typed rejection set with no partial envelope.
pub fn from_json(bytes: &[u8]) -> Result<EvidenceEnvelope, crate::diagnostics::DiagnosticSet> {
    let value = super::json::parse(bytes)?;
    let object = value
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid_detail("envelope-shape"))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid_detail("envelope-field"));
        }
    }
    for key in TOP_LEVEL_KEYS {
        if !object.contains_key(*key) {
            return Err(diagnostic::input_invalid_detail("envelope-missing"));
        }
    }
    let text = |key: &str| object.get(key).and_then(Json::as_str);
    let trust = TrustState::from_key(text("trust").unwrap_or_default())
        .ok_or_else(|| diagnostic::input_invalid_detail("envelope-trust"))?;
    let entries_json = object
        .get("entries")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid_detail("envelope-entries"))?;
    let mut entries = Vec::with_capacity(entries_json.len());
    for entry in entries_json {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-entry-shape"))?;
        for key in object.keys() {
            if !ENTRY_KEYS.contains(&key.as_str()) {
                return Err(diagnostic::input_invalid_detail("envelope-field"));
            }
        }
        for key in ["operation", "kind", "subject", "occurrence"] {
            if !object.contains_key(key) {
                return Err(diagnostic::input_invalid_detail("envelope-missing"));
            }
        }
        let operation = OperationId::from_semantic(
            object
                .get("operation")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid_detail("envelope-operation"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid_detail("envelope-operation"))?;
        let kind_key = object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-kind"))?;
        let kind = EffectKind::from_key(kind_key)
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-kind"))?;
        if matches!(kind, EffectKind::WriteField { .. }) {
            // Write-field actions are declared-surface kinds; the
            // envelope admits them only with their action suffix and a
            // field, which the generic grammar below does not carry.
            return Err(diagnostic::input_invalid_detail("envelope-kind"));
        }
        let resource_text = object
            .get("subject")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-subject"))?;
        let (resource_kind_key, resource_id) = resource_text
            .split_once(':')
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-subject"))?;
        let resource_kind = super::identity::ResourceKind::from_key(resource_kind_key)
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-subject"))?;
        let resource = ResourceId::new(resource_kind, resource_id)
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-subject"))?;
        let field = match object.get("field") {
            None => None,
            Some(value) => Some(
                super::identity::FieldName::new(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid_detail("envelope-field"))?,
                )
                .ok_or_else(|| diagnostic::input_invalid_detail("envelope-field"))?,
            ),
        };
        let subject = if let Some(field) = &field {
            Subject::with_field(resource, field.clone())
        } else {
            Subject::new(resource)
        };
        let occurrence = object
            .get("occurrence")
            .and_then(Json::as_u64)
            .filter(|occurrence| *occurrence <= u32::MAX as u64)
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-occurrence"))?
            as u32;
        let entry_trust = match object.get("trust") {
            None => None,
            Some(value) => Some(
                TrustState::from_key(value.as_str().unwrap_or_default())
                    .ok_or_else(|| diagnostic::input_invalid_detail("envelope-trust"))?,
            ),
        };
        let transaction_group = match object.get("transactionGroup") {
            None => None,
            Some(value) => Some(
                TransactionGroupId::new(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid_detail("envelope-group"))?,
                )
                .ok_or_else(|| diagnostic::input_invalid_detail("envelope-group"))?,
            ),
        };
        entries.push(EvidenceEntry {
            operation,
            kind,
            subject,
            occurrence,
            trust: entry_trust,
            transaction_group,
            sensitivity: None,
        });
    }
    let envelope = EvidenceEnvelope {
        adapter_id: text("adapterId").unwrap_or_default().to_owned(),
        target_id: text("targetId").unwrap_or_default().to_owned(),
        protocol_version: text("protocolVersion").unwrap_or_default().to_owned(),
        evidence_digest: text("evidenceDigest").unwrap_or_default().to_owned(),
        source_revision: text("sourceRevision").unwrap_or_default().to_owned(),
        trust,
        entries,
    };
    validate_envelope(&envelope, super::version::MAX_ENVELOPE_ENTRIES)
        .map_err(|violation| diagnostic::envelope_violation_set(&violation))?;
    Ok(envelope)
}
