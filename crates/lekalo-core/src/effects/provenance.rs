use super::identity::{
    is_namespaced_id, is_semantic_id, is_semver, OperationId, Subject, TransactionGroupId,
};
use super::kind::EffectKind;

/// The exact `sha256:<64 lowercase hex>` digest grammar.
pub(crate) fn is_sha256_digest(text: &str) -> bool {
    let hex = match text.strip_prefix("sha256:") {
        Some(hex) => hex,
        None => return false,
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// A bounded wire token (error classes and retry vocabulary).
pub(crate) fn is_bounded_token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The closed evidence-trust vocabulary (the wire spelling of an
/// envelope's trust state).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum TrustState {
    /// Confirmed by the accepted typed evidence owner.
    Verified,
    /// Extracted from the current target and still current.
    Current,
    /// Extracted by an adapter without a current re-verification.
    Extracted,
    /// Computed by a recorded algorithm over other evidence.
    Inferred,
    /// Captured against a revision that no longer matches.
    Stale,
    /// Unverifiable: visibly degraded, never optimistic.
    Unknown,
    /// Refused by the evidence owner; never attached as an edge.
    Rejected,
}

impl TrustState {
    /// The closed v1 trust keys in registry rank order.
    pub(crate) const KEYS: [Self; 7] = [
        Self::Verified,
        Self::Current,
        Self::Extracted,
        Self::Inferred,
        Self::Stale,
        Self::Unknown,
        Self::Rejected,
    ];

    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Current => "current",
            Self::Extracted => "extracted",
            Self::Inferred => "inferred",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
            Self::Rejected => "rejected",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::KEYS
            .iter()
            .find(|candidate| candidate.key() == key)
            .copied()
    }

    /// The confidence this trust state carries on its own.
    pub(crate) const fn confidence(self) -> super::Confidence {
        match self {
            Self::Verified => super::Confidence::Verified,
            Self::Current | Self::Extracted => super::Confidence::Extracted,
            Self::Inferred => super::Confidence::Inferred,
            Self::Stale | Self::Unknown | Self::Rejected => super::Confidence::Unknown,
        }
    }
}

/// The closed declared-provenance reference roles: the exact IR surface a
/// declared effect was projected from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum DeclaredRole {
    /// A query `reads` symbol (the declared read).
    QueryReads,
    /// A command `effects` symbol naming an effect definition (the
    /// declared CRUD on that effect's entity).
    CommandEffect,
    /// An effect definition `emits` symbol (the declared event emission).
    EffectEmits,
    /// An event emitted directly by a successor command.
    CommandEmits,
}

impl DeclaredRole {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::QueryReads => "query-reads",
            Self::CommandEffect => "command-effect",
            Self::EffectEmits => "effect-emits",
            Self::CommandEmits => "command-emits",
        }
    }
}

/// The closed provenance sum of one effect edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EffectProvenance {
    /// Projected deterministically from the accepted canonical IR.
    CanonicalIr {
        /// The `sha256` digest of the canonical IR bytes.
        ir_digest: String,
        /// The closed declared role of the source site.
        role: DeclaredRole,
        /// The 0-based site ordinal inside its role.
        occurrence: u32,
        /// The fully qualified semantic id of the referencing definition.
        symbol: String,
    },
    /// One typed adapter or native evidence record.
    Evidence {
        /// The namespaced envelope adapter id.
        adapter_id: String,
        /// The bound target identity.
        target_id: String,
        /// The exact target protocol version.
        protocol_version: String,
        /// The opaque `sha256` evidence digest.
        evidence_digest: String,
        /// The closed trust state of the record.
        trust: TrustState,
    },
}

impl EffectProvenance {
    /// The provenance sort rank (canonical first, then evidence).
    pub(crate) const fn sort_rank(&self) -> u8 {
        match self {
            Self::CanonicalIr { .. } => 0,
            Self::Evidence { .. } => 1,
        }
    }

    /// The confidence this provenance carries on its own.
    pub(crate) const fn confidence(&self) -> super::Confidence {
        match self {
            Self::CanonicalIr { .. } => super::Confidence::Canonical,
            Self::Evidence { trust, .. } => trust.confidence(),
        }
    }

    /// The canonical bytes of this provenance record (byte-sorted keys).
    pub(crate) fn to_canonical_bytes(&self) -> String {
        match self {
            Self::CanonicalIr {
                ir_digest,
                role,
                occurrence,
                symbol,
            } => format!(
                "{{\"irDigest\":{ir},\"occurrence\":{occurrence},\"role\":{r},\"symbol\":{s},\"type\":\"canonical-ir\"}}",
                ir = quote(ir_digest),
                occurrence = occurrence,
                r = quote(role.key()),
                s = quote(symbol),
            ),
            Self::Evidence {
                adapter_id,
                target_id,
                protocol_version,
                evidence_digest,
                trust,
            } => format!(
                "{{\"adapterId\":{a},\"evidenceDigest\":{d},\"protocolVersion\":{p},\"targetId\":{t},\"trust\":{tr},\"type\":\"evidence\"}}",
                a = quote(adapter_id),
                d = quote(evidence_digest),
                p = quote(protocol_version),
                t = quote(target_id),
                tr = quote(trust.key()),
            ),
        }
    }
}

/// One bounded sensitivity marker reference (see [`super::identity`]).
pub(crate) fn sensitivity_sort_key(
    sensitivity: &Option<super::identity::Sensitivity>,
) -> (bool, String) {
    match sensitivity {
        Some(marker) => (marker.classified(), marker.contract().to_owned()),
        None => (false, String::new()),
    }
}

/// One detected-effect record inside an evidence envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceEntry {
    /// The operation the effect was detected on.
    pub operation: OperationId,
    /// The closed effect kind.
    pub kind: EffectKind,
    /// The effect subject (resource plus optional exact field).
    pub subject: Subject,
    /// The 0-based occurrence ordinal inside the envelope.
    pub occurrence: u32,
    /// The per-entry trust override, when the record carries one.
    pub trust: Option<TrustState>,
    /// The optional transaction-group reference.
    pub transaction_group: Option<TransactionGroupId>,
    /// The optional bounded sensitivity marker.
    pub sensitivity: Option<super::identity::Sensitivity>,
    /// The typed partial-failure record for one multi-recipient
    /// dispatch: never a group name — the exact per-recipient results
    /// and the failed-only retry set.
    pub partial_failure: Option<PartialFailure>,
}

/// One typed per-recipient result of a partial-failure dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecipientResult {
    /// The namespaced provider that was called.
    pub provider: String,
    /// The observed result for THIS recipient.
    pub result: RecipientOutcome,
    /// The exact receipt digest (delivered recipients only).
    pub receipt_digest: Option<String>,
    /// The closed error class (failed recipients only).
    pub error_class: Option<String>,
}

/// The closed per-recipient outcome vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecipientOutcome {
    /// The recipient accepted the write; the receipt persists.
    Delivered,
    /// The recipient failed; only these are retry-eligible.
    Failed,
}

impl RecipientOutcome {
    /// The wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Delivered => "delivered",
            Self::Failed => "failed",
        }
    }

    /// Parse the closed wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "delivered" => Some(Self::Delivered),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// The typed partial-failure record: which declared case the trace
/// realizes, the per-recipient results, and the failed-only retry set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartialFailure {
    /// The namespaced declared partial-failure case this trace
    /// realizes (the authoritative typed declaration lives with the
    /// extended-effects family; the envelope carries the reference).
    pub case_ref: String,
    /// The operation whose dispatch partially failed.
    pub operation: String,
    /// The per-recipient results (at least two: a partial failure is
    /// per-recipient, never a single boolean).
    pub recipients: Vec<RecipientResult>,
    /// Exactly the failed recipients: a delivered recipient is never
    /// redelivered as a new action.
    pub retry_eligible: Vec<String>,
}

/// One member of a declared local-atomic transaction: the exact edge
/// (operation, kind, subject) that commits inside the group.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TransactionMember {
    /// The operation that owns the member edge.
    pub operation: String,
    /// The closed effect kind.
    pub kind: EffectKind,
    /// The subject resource in wire spelling (`kind:id`).
    pub subject: String,
}

/// A typed caller-declared local transaction group, NOT proof that its
/// member writes executed atomically. Network calls are never members.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LocalTransaction {
    /// The group identity the member edges carry.
    pub group: TransactionGroupId,
    /// The exact local member writes, in canonical order.
    pub members: Vec<TransactionMember>,
}

/// One typed detected-evidence envelope.
///
/// The envelope is the only way detected effects enter the graph: it is
/// produced by an adapter or native tool, never invented here, and it
/// never mutates the canonical model. Shape, grammar, and digest form are
/// validated; cryptographic authenticity and process truth stay with the
/// evidence owners.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceEnvelope {
    /// The namespaced adapter id that produced the evidence.
    pub adapter_id: String,
    /// The bound target identity (namespaced).
    pub target_id: String,
    /// The exact target protocol version.
    pub protocol_version: String,
    /// The opaque `sha256` digest of the evidence payload.
    pub evidence_digest: String,
    /// The source revision the evidence was captured against (bounded
    /// token; compared for visibility only, never enforced here).
    pub source_revision: String,
    /// The envelope-default trust state.
    pub trust: TrustState,
    /// The declared local-atomic transactions: every grouped entry
    /// must be a member of exactly one declaration, and no declaration
    /// may name a network kind.
    pub transactions: Vec<LocalTransaction>,
    /// The bounded detected-effect records.
    pub entries: Vec<EvidenceEntry>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EnvelopeViolation {
    /// A namespaced id, semantic id, or SemVer field failed its grammar.
    MalformedField { field: &'static str },
    /// A digest failed the exact `sha256:<64 hex>` grammar.
    MalformedDigest { field: &'static str },
    /// The entry count crossed the recorded envelope bound.
    OverLimit,
    /// An entry combined an illegal kind/subject pair.
    IllegalEntry { occurrence: u32 },
    /// An entry tried to enclose a network call inside a local atomic
    /// group: the contract keeps network calls outside transactions.
    NetworkInGroup { occurrence: u32 },
    /// A grouped entry's group has no typed transaction declaration.
    GroupUndeclared { occurrence: u32 },
    /// A declared transaction's members do not match the grouped
    /// entries exactly, in either direction.
    GroupMemberMismatch,
    /// A typed partial-failure record is malformed or misplaced.
    PartialFailureShape { occurrence: u32 },
    /// A partial-failure trace contradicts per-recipient semantics
    /// (retry eligibility, delivered receipts, failed error classes).
    PartialFailureRetry { occurrence: u32 },
}

impl EnvelopeViolation {
    /// The bounded detail token for the diagnostic data.
    pub(crate) fn detail(&self) -> String {
        match self {
            Self::MalformedField { field } => (*field).to_owned(),
            Self::MalformedDigest { field } => (*field).to_owned(),
            Self::OverLimit => "envelope-limit".to_owned(),
            Self::IllegalEntry { .. } => "illegal-entry".to_owned(),
            Self::NetworkInGroup { .. } => "network-in-group".to_owned(),
            Self::GroupUndeclared { .. } => "group-undeclared".to_owned(),
            Self::GroupMemberMismatch => "group-member-mismatch".to_owned(),
            Self::PartialFailureShape { .. } => "partial-failure-shape".to_owned(),
            Self::PartialFailureRetry { .. } => "partial-failure-retry".to_owned(),
        }
    }
}

/// Validate one envelope's shape; entries are validated structurally and
/// returned unchanged — the caller projects them with its own caps.
pub(crate) fn validate_envelope(
    envelope: &EvidenceEnvelope,
    max_entries: usize,
) -> Result<(), EnvelopeViolation> {
    if !is_namespaced_id(&envelope.adapter_id) {
        return Err(EnvelopeViolation::MalformedField {
            field: "adapter-id",
        });
    }
    if !is_namespaced_id(&envelope.target_id) {
        return Err(EnvelopeViolation::MalformedField { field: "target-id" });
    }
    if !is_semver(&envelope.protocol_version) {
        return Err(EnvelopeViolation::MalformedField {
            field: "protocol-version",
        });
    }
    if !is_sha256_digest(&envelope.evidence_digest) {
        return Err(EnvelopeViolation::MalformedDigest {
            field: "evidence-digest",
        });
    }
    if !is_bounded_revision(&envelope.source_revision) {
        return Err(EnvelopeViolation::MalformedField {
            field: "source-revision",
        });
    }
    if envelope.entries.len() > max_entries || envelope.transactions.len() > max_entries {
        return Err(EnvelopeViolation::OverLimit);
    }
    for entry in &envelope.entries {
        if super::kind::check_subject(entry.kind, &entry.subject).is_err() {
            return Err(EnvelopeViolation::IllegalEntry {
                occurrence: entry.occurrence,
            });
        }
        // Network calls sit OUTSIDE local atomic groups: the contract
        // excludes them from transactions, so a grouped external call
        // is a semantic violation, never a rendering choice.
        if entry.kind == EffectKind::ExternalCall && entry.transaction_group.is_some() {
            return Err(EnvelopeViolation::NetworkInGroup {
                occurrence: entry.occurrence,
            });
        }
        // The typed partial-failure record belongs to a multi-recipient
        // network dispatch only.
        if let Some(record) = &entry.partial_failure {
            if entry.kind != EffectKind::ExternalCall {
                return Err(EnvelopeViolation::PartialFailureShape {
                    occurrence: entry.occurrence,
                });
            }
            if !is_namespaced_id(&record.case_ref)
                || !is_semantic_id(&record.operation)
                || record.operation != entry.operation.semantic_id()
                || record.recipients.len() < 2
                || record.recipients.len() > 32
                || record.retry_eligible.len() > 32
            {
                return Err(EnvelopeViolation::PartialFailureShape {
                    occurrence: entry.occurrence,
                });
            }
            let mut providers = std::collections::BTreeSet::new();
            let mut failed = std::collections::BTreeSet::new();
            for recipient in &record.recipients {
                if !is_namespaced_id(&recipient.provider)
                    || !providers.insert(recipient.provider.clone())
                {
                    return Err(EnvelopeViolation::PartialFailureShape {
                        occurrence: entry.occurrence,
                    });
                }
                match recipient.result {
                    RecipientOutcome::Delivered => {
                        let delivered = recipient
                            .receipt_digest
                            .as_deref()
                            .is_some_and(is_sha256_digest)
                            && recipient.error_class.is_none();
                        if !delivered {
                            return Err(EnvelopeViolation::PartialFailureRetry {
                                occurrence: entry.occurrence,
                            });
                        }
                    }
                    RecipientOutcome::Failed => {
                        let failed_shape = recipient
                            .error_class
                            .as_deref()
                            .is_some_and(is_bounded_token)
                            && recipient.receipt_digest.is_none();
                        if !failed_shape {
                            return Err(EnvelopeViolation::PartialFailureRetry {
                                occurrence: entry.occurrence,
                            });
                        }
                        failed.insert(recipient.provider.clone());
                    }
                }
            }
            // A partial failure must contain both delivered and failed
            // recipients, not a relabelled complete success/failure.
            if failed.is_empty() || failed.len() == providers.len() {
                return Err(EnvelopeViolation::PartialFailureShape {
                    occurrence: entry.occurrence,
                });
            }
            // Exactly the failed recipients retry: no delivered/unknown
            // recipient and no silently omitted failed recipient.
            let mut retry_seen = std::collections::BTreeSet::new();
            for provider in &record.retry_eligible {
                if !is_namespaced_id(provider)
                    || !retry_seen.insert(provider.clone())
                    || !failed.contains(provider)
                {
                    return Err(EnvelopeViolation::PartialFailureRetry {
                        occurrence: entry.occurrence,
                    });
                }
            }
            if retry_seen != failed {
                return Err(EnvelopeViolation::PartialFailureRetry {
                    occurrence: entry.occurrence,
                });
            }
        }
    }
    // Typed local-atomic declarations: every grouped entry belongs to
    // exactly one declared transaction, every declared member matches a
    // grouped entry of the same group, and no declaration names a
    // network kind. A group string alone proves nothing.
    {
        let mut declared_groups = std::collections::BTreeSet::new();
        for transaction in &envelope.transactions {
            if !declared_groups.insert(transaction.group.as_str().to_owned())
                || transaction.members.is_empty()
                || transaction.members.len() > super::version::MAX_ENVELOPE_ENTRIES
                || transaction
                    .members
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != transaction.members.len()
            {
                return Err(EnvelopeViolation::GroupMemberMismatch);
            }
            for member in &transaction.members {
                if member.kind == EffectKind::ExternalCall {
                    return Err(EnvelopeViolation::NetworkInGroup { occurrence: 0 });
                }
                // The member subject uses the entry subject grammar:
                // `<resource-kind>:<id>` with the kind-checked id below.
                let Some((kind_key, id)) = member.subject.split_once(':') else {
                    return Err(EnvelopeViolation::GroupMemberMismatch);
                };
                let Some(resource_kind) = super::identity::ResourceKind::from_key(kind_key) else {
                    return Err(EnvelopeViolation::GroupMemberMismatch);
                };
                let Some(resource) = super::identity::ResourceId::new(resource_kind, id) else {
                    return Err(EnvelopeViolation::GroupMemberMismatch);
                };
                let subject = Subject::new(resource);
                if !is_semantic_id(&member.operation)
                    || super::kind::check_subject(member.kind, &subject).is_err()
                {
                    return Err(EnvelopeViolation::GroupMemberMismatch);
                }
            }
        }
        for entry in &envelope.entries {
            let Some(group) = entry.transaction_group.as_ref() else {
                continue;
            };
            let Some(transaction) = envelope
                .transactions
                .iter()
                .find(|transaction| transaction.group.as_str() == group.as_str())
            else {
                return Err(EnvelopeViolation::GroupUndeclared {
                    occurrence: entry.occurrence,
                });
            };
            let member_spelled = TransactionMember {
                operation: entry.operation.semantic_id().to_owned(),
                kind: entry.kind,
                subject: format!(
                    "{}:{}",
                    entry.subject.resource().kind().key(),
                    entry.subject.resource().as_str()
                ),
            };
            if !transaction.members.contains(&member_spelled) {
                return Err(EnvelopeViolation::GroupMemberMismatch);
            }
        }
        for transaction in &envelope.transactions {
            for member in &transaction.members {
                let matched = envelope.entries.iter().any(|entry| {
                    entry
                        .transaction_group
                        .as_ref()
                        .is_some_and(|group| group.as_str() == transaction.group.as_str())
                        && entry.operation.semantic_id() == member.operation
                        && entry.kind == member.kind
                        && format!(
                            "{}:{}",
                            entry.subject.resource().kind().key(),
                            entry.subject.resource().as_str()
                        ) == member.subject
                });
                if !matched {
                    return Err(EnvelopeViolation::GroupMemberMismatch);
                }
            }
        }
    }
    Ok(())
}

/// A bounded source revision: a node-safe semantic id, a namespaced
/// token, or an exact `sha256:<64 hex>` digest — the compilation pin a
/// detected-evidence envelope carries is a digest, so the pin grammar
/// accepts exactly that spelling.
fn is_bounded_revision(text: &str) -> bool {
    text.len() <= 128 && (is_semantic_id(text) || is_namespaced_id(text) || is_sha256_digest(text))
}

/// A quoted JSON string (the writer adds the surrounding quotes).
pub(crate) fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}
