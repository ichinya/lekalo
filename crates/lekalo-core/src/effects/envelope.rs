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
    "transactions",
    "entries",
];

/// The required top-level members: `transactions` is optional — an
/// envelope without grouped entries declares no transactions — but a
/// group carried by any entry must be declared (validated with the
/// entries).
const REQUIRED_TOP_LEVEL_KEYS: &[&str] = &[
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
    "partialFailure",
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
    for key in REQUIRED_TOP_LEVEL_KEYS {
        if !object.contains_key(*key) {
            return Err(diagnostic::input_invalid_detail("envelope-missing"));
        }
    }
    let text = |key: &str| object.get(key).and_then(Json::as_str);
    let trust = TrustState::from_key(text("trust").unwrap_or_default())
        .ok_or_else(|| diagnostic::input_invalid_detail("envelope-trust"))?;
    // The typed local-atomic declarations: every group an entry carries
    // must be declared here with its exact member writes, and network
    // calls are never members (validated with the entries below).
    let transactions_json = object
        .get("transactions")
        .map(|value| {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| diagnostic::input_invalid_detail("envelope-transactions"))
        })
        .transpose()?;
    let mut transactions = Vec::new();
    for transaction in transactions_json.into_iter().flatten() {
        let object = transaction
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-transactions"))?;
        if object.len() != 2 || !object.contains_key("groupId") || !object.contains_key("members") {
            return Err(diagnostic::input_invalid_detail("envelope-transactions"));
        }
        let group = TransactionGroupId::new(
            object
                .get("groupId")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid_detail("envelope-group"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid_detail("envelope-group"))?;
        let members_json = object
            .get("members")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-transactions"))?;
        let mut members = Vec::with_capacity(members_json.len());
        for member in members_json {
            let object = member
                .as_object()
                .ok_or_else(|| diagnostic::input_invalid_detail("envelope-transactions"))?;
            if object.len() != 3
                || !object.contains_key("operation")
                || !object.contains_key("kind")
                || !object.contains_key("subject")
            {
                return Err(diagnostic::input_invalid_detail("envelope-transactions"));
            }
            let operation = object
                .get("operation")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid_detail("envelope-operation"))?;
            let kind = EffectKind::from_key(
                object
                    .get("kind")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid_detail("envelope-kind"))?,
            )
            .ok_or_else(|| diagnostic::input_invalid_detail("envelope-kind"))?;
            let subject = object
                .get("subject")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid_detail("envelope-subject"))?;
            members.push(super::provenance::TransactionMember {
                operation: operation.to_owned(),
                kind,
                subject: subject.to_owned(),
            });
        }
        transactions.push(super::provenance::LocalTransaction { group, members });
    }
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
        // The typed partial-failure trace: the declared case, the exact
        // per-recipient results, and the failed-only retry set — a
        // group NAME never stands in for these semantics.
        let partial_failure = match object.get("partialFailure") {
            None => None,
            Some(value) => {
                let record = value
                    .as_object()
                    .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))?;
                if record.len() != 4
                    || !record.contains_key("caseRef")
                    || !record.contains_key("recipients")
                    || !record.contains_key("retryEligible")
                    || !record.contains_key("operation")
                {
                    return Err(diagnostic::input_invalid_detail("partial-failure"));
                }
                let case_ref = record
                    .get("caseRef")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))?
                    .to_owned();
                let failure_operation = record
                    .get("operation")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))?
                    .to_owned();
                let recipients_json = record
                    .get("recipients")
                    .and_then(Json::as_array)
                    .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))?;
                let mut recipients = Vec::with_capacity(recipients_json.len());
                for recipient in recipients_json {
                    let object = recipient
                        .as_object()
                        .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))?;
                    if object.len() < 2
                        || object.len() > 4
                        || !object.contains_key("provider")
                        || !object.contains_key("result")
                        || object.keys().any(|key| {
                            !matches!(
                                key.as_str(),
                                "provider" | "result" | "receiptDigest" | "errorClass"
                            )
                        })
                    {
                        return Err(diagnostic::input_invalid_detail("partial-failure"));
                    }
                    let result = super::provenance::RecipientOutcome::from_key(
                        object
                            .get("result")
                            .and_then(Json::as_str)
                            .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))?,
                    )
                    .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))?;
                    let optional_text = |key: &str| {
                        object
                            .get(key)
                            .map(|value| {
                                value.as_str().map(str::to_owned).ok_or_else(|| {
                                    diagnostic::input_invalid_detail("partial-failure")
                                })
                            })
                            .transpose()
                    };
                    recipients.push(super::provenance::RecipientResult {
                        provider: object
                            .get("provider")
                            .and_then(Json::as_str)
                            .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))?
                            .to_owned(),
                        result,
                        receipt_digest: optional_text("receiptDigest")?,
                        error_class: optional_text("errorClass")?,
                    });
                }
                let retry_eligible = record
                    .get("retryEligible")
                    .and_then(Json::as_array)
                    .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| diagnostic::input_invalid_detail("partial-failure"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Some(super::provenance::PartialFailure {
                    case_ref,
                    operation: failure_operation,
                    recipients,
                    retry_eligible,
                })
            }
        };
        entries.push(EvidenceEntry {
            operation,
            kind,
            subject,
            occurrence,
            trust: entry_trust,
            transaction_group,
            sensitivity: None,
            partial_failure,
        });
    }
    let envelope = EvidenceEnvelope {
        adapter_id: text("adapterId").unwrap_or_default().to_owned(),
        target_id: text("targetId").unwrap_or_default().to_owned(),
        protocol_version: text("protocolVersion").unwrap_or_default().to_owned(),
        evidence_digest: text("evidenceDigest").unwrap_or_default().to_owned(),
        source_revision: text("sourceRevision").unwrap_or_default().to_owned(),
        trust,
        transactions,
        entries,
    };
    validate_envelope(&envelope, super::version::MAX_ENVELOPE_ENTRIES)
        .map_err(|violation| diagnostic::envelope_violation_set(&violation))?;
    // Integrity, not authenticity: digest compact recursively key-sorted
    // JSON of the entire envelope except evidenceDigest itself. A producer
    // can recompute this hash, so even a matching payload remains extracted.
    let mut payload = value;
    payload
        .as_object_mut()
        .expect("checked object")
        .remove("evidenceDigest");
    let canonical = serde_json::to_vec(&payload)
        .map_err(|_| diagnostic::input_invalid_detail("envelope-shape"))?;
    let computed = format!("sha256:{}", crate::digest::sha256_hex(&canonical));
    if envelope.evidence_digest != computed {
        return Err(diagnostic::input_invalid_detail("evidence-digest-mismatch"));
    }
    Ok(envelope)
}
