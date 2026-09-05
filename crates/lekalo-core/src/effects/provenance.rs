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
}

impl DeclaredRole {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::QueryReads => "query-reads",
            Self::CommandEffect => "command-effect",
            Self::EffectEmits => "effect-emits",
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
}

impl EnvelopeViolation {
    /// The bounded detail token for the diagnostic data.
    pub(crate) fn detail(&self) -> String {
        match self {
            Self::MalformedField { field } => (*field).to_owned(),
            Self::MalformedDigest { field } => (*field).to_owned(),
            Self::OverLimit => "envelope-limit".to_owned(),
            Self::IllegalEntry { .. } => "illegal-entry".to_owned(),
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
    if envelope.entries.len() > max_entries {
        return Err(EnvelopeViolation::OverLimit);
    }
    for entry in &envelope.entries {
        if super::kind::check_subject(entry.kind, &entry.subject).is_err() {
            return Err(EnvelopeViolation::IllegalEntry {
                occurrence: entry.occurrence,
            });
        }
    }
    Ok(())
}

/// A bounded source revision: a node-safe semantic id or a namespaced
/// digest-like token, at most 128 bytes.
fn is_bounded_revision(text: &str) -> bool {
    text.len() <= 128 && (is_semantic_id(text) || is_namespaced_id(text))
}

/// A quoted JSON string (the writer adds the surrounding quotes).
pub(crate) fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}
