//! The canonical identity of one effect edge (issue #14).
//!
//! The key carries every semantic machine field: the acting operation,
//! the closed kind with its action, the typed subject, the effect origin,
//! and the source occurrence ordinal. Exact duplicates collapse only when
//! every machine field matches; the same effect at different occurrences
//! stays distinct. Sorting is by unsigned byte order of the canonical
//! parts — never locale, hash, or filesystem order.

use super::identity::{EffectOrigin, OperationId, Subject};
use super::kind::EffectKind;
use super::provenance::{sensitivity_sort_key, EffectProvenance};
use super::{Confidence, Sensitivity, TransactionGroupId};

/// The canonical identity of one effect edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectKey {
    operation: OperationId,
    kind: EffectKind,
    subject: Subject,
    origin: EffectOrigin,
    occurrence: u32,
}

impl EffectKey {
    /// Assemble one checked key (kind/subject legality is the caller's
    /// construction gate).
    pub fn new(
        operation: OperationId,
        kind: EffectKind,
        subject: Subject,
        origin: EffectOrigin,
        occurrence: u32,
    ) -> Self {
        Self {
            operation,
            kind,
            subject,
            origin,
            occurrence,
        }
    }

    /// The acting operation.
    pub const fn operation(&self) -> &OperationId {
        &self.operation
    }

    /// The closed effect kind.
    pub const fn kind(&self) -> EffectKind {
        self.kind
    }

    /// The typed subject.
    pub const fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The effect origin.
    pub const fn origin(&self) -> &EffectOrigin {
        &self.origin
    }

    /// The 0-based occurrence ordinal.
    pub const fn occurrence(&self) -> u32 {
        self.occurrence
    }

    /// The bounded canonical string form used by derived data and
    /// diagnostics.
    pub fn to_canonical_string(&self) -> String {
        let subject = match self.subject.field() {
            Some(field) => format!("{}#{}", self.subject.resource().as_str(), field.as_str()),
            None => self.subject.resource().as_str().to_owned(),
        };
        format!(
            "{}|{}|{}|{}|{}",
            self.operation.as_str(),
            self.kind.key(),
            subject,
            self.origin.to_canonical_string(),
            self.occurrence
        )
    }

    /// The deterministic canonical byte string: every sort member joined
    /// with a 0 separator, in fixed order — operation, kind rank, action
    /// rank, resource kind rank, resource bytes, field bytes, origin
    /// rank, origin bytes, occurrence.
    pub(crate) fn sort_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.operation.as_str().as_bytes());
        let member = |part: &str, bytes: &mut Vec<u8>| {
            bytes.push(0);
            bytes.extend_from_slice(part.as_bytes());
        };
        member(&self.kind.rank().to_string(), &mut bytes);
        member(
            &self
                .kind
                .action()
                .map_or(u8::MAX, |action| action as u8)
                .to_string(),
            &mut bytes,
        );
        member(
            &self.subject.resource().kind().rank().to_string(),
            &mut bytes,
        );
        member(self.subject.resource().as_str(), &mut bytes);
        member(
            self.subject.field().map_or("", |field| field.as_str()),
            &mut bytes,
        );
        member(&self.origin.rank().to_string(), &mut bytes);
        member(&self.origin.to_canonical_string(), &mut bytes);
        member(&self.occurrence.to_string(), &mut bytes);
        bytes
    }
}

/// One effect edge: identity, provenance, confidence, and the two
/// optional policy references (transaction group, sensitivity marker).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectEdge {
    key: EffectKey,
    provenance: EffectProvenance,
    confidence: Confidence,
    transaction_group: Option<TransactionGroupId>,
    sensitivity: Option<Sensitivity>,
}

impl EffectEdge {
    /// Assemble one edge; the confidence follows from the provenance
    /// kind, so an edge can never claim more trust than its evidence.
    pub(crate) fn new(
        key: EffectKey,
        provenance: EffectProvenance,
        transaction_group: Option<TransactionGroupId>,
        sensitivity: Option<Sensitivity>,
    ) -> Self {
        let confidence = provenance.confidence();
        Self {
            key,
            provenance,
            confidence,
            transaction_group,
            sensitivity,
        }
    }

    /// The edge identity.
    pub const fn key(&self) -> &EffectKey {
        &self.key
    }

    /// The closed provenance record.
    pub const fn provenance(&self) -> &EffectProvenance {
        &self.provenance
    }

    /// The closed confidence level.
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// The descriptive transaction-group reference, when carried.
    pub const fn transaction_group(&self) -> Option<&TransactionGroupId> {
        self.transaction_group.as_ref()
    }

    /// The bounded sensitivity marker, when carried.
    pub const fn sensitivity(&self) -> Option<&Sensitivity> {
        self.sensitivity.as_ref()
    }

    /// The canonical edge sort key: the key bytes, then provenance kind,
    /// then the provenance bytes, then the policy references.
    pub(crate) fn sort_key(&self) -> (Vec<u8>, u8, String, bool, String) {
        let (sensitivity_classified, sensitivity_contract) =
            sensitivity_sort_key(&self.sensitivity);
        (
            self.key.sort_bytes(),
            self.provenance.sort_rank(),
            self.provenance.to_canonical_bytes(),
            sensitivity_classified,
            sensitivity_contract,
        )
    }

    /// Whether this edge carries a classified sensitivity marker.
    pub(crate) fn is_sensitivity_classified(&self) -> bool {
        self.sensitivity
            .as_ref()
            .is_some_and(Sensitivity::classified)
    }
}
