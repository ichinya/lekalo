//! Parallel-change conflict classification over explicit change sets
//! (issue #14).
//!
//! The caller supplies the changed operations as a typed set — this
//! module never parses Git, owns a diff, or infers changed symbols (#16
//! owns changed-input derivation). Classification is a closed,
//! conservative matrix over direct effect edges: exact and overlapping
//! writes conflict, a delete conflicts with anything on its resource,
//! exact reads alone never conflict, independent fields stay independent
//! unless an entity-wide effect overlaps, and event or external
//! collisions require the same typed scope. Unknown identity or degraded
//! evidence yields `unknown`, never a silent no-conflict. This module
//! reports facts; scheduling policy stays with #16/#20/#91.

use super::diagnostic;
use super::edge::EffectEdge;
use super::identity::OperationId;
use super::index::EffectIndex;
use super::kind::EffectKind;
use super::provenance::TrustState;
use super::version::{MAX_CHANGED_OPERATIONS, MAX_CONFLICT_ITEMS};
use super::EffectGraph;

/// The closed conflict-classification vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ConflictKind {
    /// The compared effects do not overlap.
    None,
    /// One operation reads what another writes on the same resource.
    PotentialReadWrite,
    /// Two operations write the same resource or exact field.
    DefiniteWriteWrite,
    /// An entity-wide effect overlaps a field-scoped effect.
    EntityFieldOverlap,
    /// A delete overlaps any other effect on its resource.
    DeleteOverlap,
    /// Two operations emit the same event or enqueue the same job.
    EventJobCollision,
    /// Two operations call the same external service.
    ExternalCallCollision,
    /// Two operations share a transaction-group boundary.
    TransactionGroupOverlap,
    /// The overlap carries a classified sensitivity marker.
    SensitivityPolicyGate,
    /// Degraded evidence or unknown identity: visibly unresolved.
    Unknown,
}

impl ConflictKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PotentialReadWrite => "potential-read-write",
            Self::DefiniteWriteWrite => "definite-write-write",
            Self::EntityFieldOverlap => "entity-field-overlap",
            Self::DeleteOverlap => "delete-overlap",
            Self::EventJobCollision => "event-job-collision",
            Self::ExternalCallCollision => "external-call-collision",
            Self::TransactionGroupOverlap => "transaction-group-overlap",
            Self::SensitivityPolicyGate => "sensitivity-policy-gate",
            Self::Unknown => "unknown",
        }
    }

    /// The stable explanation template for one classified pair.
    pub(crate) fn explanation(self, left: &str, right: &str, subject: &str) -> String {
        let frame = match self {
            Self::None => "no overlapping effect",
            Self::PotentialReadWrite => "one operation reads what the other writes",
            Self::DefiniteWriteWrite => "both operations write the same scope",
            Self::EntityFieldOverlap => "an entity-wide effect overlaps a field effect",
            Self::DeleteOverlap => "a delete overlaps an effect on the same resource",
            Self::EventJobCollision => "both operations collide on the same event or job",
            Self::ExternalCallCollision => "both operations call the same external service",
            Self::TransactionGroupOverlap => "both operations share a transaction group",
            Self::SensitivityPolicyGate => "the overlap carries a classified sensitivity marker",
            Self::Unknown => "degraded evidence leaves the overlap unresolved",
        };
        format!("{left} x {right} on {subject}: {frame}")
    }
}

/// One explicitly supplied change set: the operations the caller says
/// changed (typed handoff; #16 owns any derivation).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeSet {
    changed: Vec<OperationId>,
}

impl ChangeSet {
    /// Assemble one bounded change set from explicit operations.
    pub fn new(changed: Vec<OperationId>) -> Result<Self, crate::diagnostics::DiagnosticSet> {
        if changed.len() > MAX_CHANGED_OPERATIONS {
            return Err(diagnostic::traversal_limit_set("changed-limit"));
        }
        let mut changed = changed;
        changed.sort();
        changed.dedup();
        Ok(Self { changed })
    }

    /// The changed operations in canonical order.
    pub fn changed(&self) -> &[OperationId] {
        &self.changed
    }
}

/// One classified conflict between two operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictItem {
    classification: ConflictKind,
    left: OperationId,
    right: OperationId,
    subject: String,
    left_key: String,
    right_key: String,
    explanation: String,
}

impl ConflictItem {
    /// The closed classification.
    pub const fn classification(&self) -> ConflictKind {
        self.classification
    }

    /// The left operation of the pair.
    pub const fn left(&self) -> &OperationId {
        &self.left
    }

    /// The right operation of the pair.
    pub const fn right(&self) -> &OperationId {
        &self.right
    }

    /// The overlapping subject (resource plus optional field).
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The canonical key of the left effect.
    pub fn left_key(&self) -> &str {
        &self.left_key
    }

    /// The canonical key of the right effect.
    pub fn right_key(&self) -> &str {
        &self.right_key
    }

    /// The stable, deterministic explanation.
    pub fn explanation(&self) -> &str {
        &self.explanation
    }
}

/// One finished conflict report: sorted items, explicit completeness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictReport {
    items: Vec<ConflictItem>,
    complete: bool,
    bounded_reason: Option<&'static str>,
}

impl ConflictReport {
    /// The conflict items in canonical order.
    pub fn items(&self) -> &[ConflictItem] {
        &self.items
    }

    /// Whether every pair was classified within every bound.
    pub const fn complete(&self) -> bool {
        self.complete
    }

    /// The recorded bound that stopped the query, when bounded.
    pub const fn bounded_reason(&self) -> Option<&'static str> {
        self.bounded_reason
    }
}

/// Classify the changed set against the whole graph: every changed
/// operation is compared with every other operation's direct effects.
///
/// An operation the graph does not know is an explicit
/// `graph.unknown-node` failure, never an empty success.
pub(crate) fn conflicts(
    graph: &EffectGraph,
    index: &EffectIndex,
    change_set: &ChangeSet,
) -> Result<ConflictReport, crate::diagnostics::DiagnosticSet> {
    for operation in change_set.changed() {
        if !graph.knows_operation(operation) {
            return Err(diagnostic::unknown_subject_set(operation.as_str()));
        }
    }
    // Every changed operation is classified against every other
    // operation of the graph (changed or unchanged), each unordered pair
    // once, in canonical (left < right) byte order.
    let others = graph.operations();
    let mut items: Vec<ConflictItem> = Vec::new();
    let mut complete = true;
    let mut bounded_reason: Option<&'static str> = None;
    'pairs: for changed in change_set.changed() {
        for other in &others {
            if **other == *changed {
                continue;
            }
            let (left_operation, right_operation) = if changed.as_str() < other.as_str() {
                (changed, *other)
            } else {
                (*other, changed)
            };
            let left_edges =
                index.operation_edges(left_operation, graph.declared(), graph.detected());
            let right_edges =
                index.operation_edges(right_operation, graph.declared(), graph.detected());
            for left in &left_edges {
                for right in &right_edges {
                    if let Some(item) = classify(left_operation, right_operation, left, right) {
                        items.push(item);
                        if items.len() >= MAX_CONFLICT_ITEMS {
                            complete = false;
                            bounded_reason = Some("conflict-limit");
                            break 'pairs;
                        }
                    }
                }
            }
        }
    }
    items.sort_by(|left, right| {
        (
            left.subject.clone(),
            left.left.as_str().to_owned(),
            left.right.as_str().to_owned(),
            left.classification,
            left.left_key.clone(),
            left.right_key.clone(),
        )
            .cmp(&(
                right.subject.clone(),
                right.left.as_str().to_owned(),
                right.right.as_str().to_owned(),
                right.classification,
                right.left_key.clone(),
                right.right_key.clone(),
            ))
    });
    items.dedup_by(|left, right| {
        left.subject == right.subject
            && left.left == right.left
            && left.right == right.right
            && left.classification == right.classification
            && left.left_key == right.left_key
            && left.right_key == right.right_key
    });
    Ok(ConflictReport {
        items,
        complete,
        bounded_reason,
    })
}

/// Classify one edge pair between two distinct operations.
fn classify(
    left_operation: &OperationId,
    right_operation: &OperationId,
    left: &EffectEdge,
    right: &EffectEdge,
) -> Option<ConflictItem> {
    let left_key = left.key();
    let right_key = right.key();
    let left_resource = left_key.subject().resource();
    let right_resource = right_key.subject().resource();
    if left_resource != right_resource {
        return None;
    }
    let subject = match (left_key.subject().field(), right_key.subject().field()) {
        (None, None) => left_resource.as_str().to_owned(),
        (Some(field), None) | (None, Some(field)) => {
            format!("{}#{}", left_resource.as_str(), field.as_str())
        }
        (Some(left_field), Some(right_field)) => {
            if left_field != right_field {
                // Independent exact fields never conflict.
                return None;
            }
            format!("{}#{}", left_resource.as_str(), left_field.as_str())
        }
    };
    let classification = classify_pair(left, right);
    if classification == ConflictKind::None {
        return None;
    }
    Some(ConflictItem {
        explanation: classification.explanation(
            left_operation.as_str(),
            right_operation.as_str(),
            &subject,
        ),
        classification,
        left: left_operation.clone(),
        right: right_operation.clone(),
        subject,
        left_key: left_key.to_canonical_string(),
        right_key: right_key.to_canonical_string(),
    })
}

/// The conservative classification matrix for one overlapping pair.
fn classify_pair(left: &EffectEdge, right: &EffectEdge) -> ConflictKind {
    let left_kind = left.key().kind();
    let right_kind = right.key().kind();
    let left_wide = left.key().subject().is_entity_wide();
    let right_wide = right.key().subject().is_entity_wide();

    // Two emitted events / enqueued jobs collide only on the same scope;
    // the resources already match, so same-scope is proven above.
    let event_collision = matches!(left_kind, EffectKind::EmitEvent | EffectKind::EnqueueJob)
        && left_kind == right_kind;
    let external_collision =
        left_kind == EffectKind::ExternalCall && right_kind == EffectKind::ExternalCall;
    let group_overlap = match (left.transaction_group(), right.transaction_group()) {
        (Some(left_group), Some(right_group)) => left_group == right_group,
        _ => false,
    };

    let base = match (left_kind.is_mutation(), right_kind.is_mutation()) {
        (true, true) => {
            if left_kind == EffectKind::Delete || right_kind == EffectKind::Delete {
                ConflictKind::DeleteOverlap
            } else if left_wide != right_wide {
                ConflictKind::EntityFieldOverlap
            } else {
                ConflictKind::DefiniteWriteWrite
            }
        }
        (true, false) | (false, true) => match (left_kind, right_kind) {
            (EffectKind::Read, write) | (write, EffectKind::Read) if write.is_mutation() => {
                if left_kind == EffectKind::Delete || right_kind == EffectKind::Delete {
                    ConflictKind::DeleteOverlap
                } else if left_wide != right_wide {
                    ConflictKind::EntityFieldOverlap
                } else {
                    ConflictKind::PotentialReadWrite
                }
            }
            _ => ConflictKind::None,
        },
        (false, false) => {
            if event_collision {
                ConflictKind::EventJobCollision
            } else if external_collision {
                ConflictKind::ExternalCallCollision
            } else {
                ConflictKind::None
            }
        }
    };

    if base == ConflictKind::None {
        if group_overlap {
            return ConflictKind::TransactionGroupOverlap;
        }
        return ConflictKind::None;
    }
    if left.is_sensitivity_classified() || right.is_sensitivity_classified() {
        return ConflictKind::SensitivityPolicyGate;
    }
    if degraded(left) || degraded(right) {
        return ConflictKind::Unknown;
    }
    if group_overlap {
        return ConflictKind::TransactionGroupOverlap;
    }
    base
}

/// Whether one edge's evidence is degraded (stale or unknown trust), or
/// its identity cannot be anchored in the declared model.
fn degraded(edge: &EffectEdge) -> bool {
    matches!(
        edge.provenance(),
        super::EffectProvenance::Evidence {
            trust: TrustState::Stale | TrustState::Unknown,
            ..
        }
    )
}
