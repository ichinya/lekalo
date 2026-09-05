//! Declared versus detected effect comparison (issue #14).
//!
//! The comparison joins the declared and detected edge sets by their
//! typed canonical identity (operation, kind with action, subject scope)
//! and classifies every difference into one closed, explicit state: an
//! unpaired mutation whose operation still touches the same resource is
//! an action or scope mismatch, not a plain declared/detected difference.
//! Unknown or stale evidence never collapses into `matched`, and a
//! bounded report says so instead of pretending completeness. Every item
//! carries a stable, deterministic explanation built only from canonical
//! data — never adapter prose.

use std::collections::{HashMap, HashSet};

use super::diagnostic;
use super::edge::EffectEdge;
use super::identity::OperationId;
use super::kind::EffectKind;
use super::provenance::TrustState;
use super::version::MAX_COMPARISON_ITEMS;
use super::{Confidence, EffectGraph};

/// The closed comparison-state vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ComparisonState {
    /// Declared but not observed: an undeclared runtime-effect risk.
    DeclaredOnly,
    /// Observed but never declared: an undeclared effect.
    DetectedOnly,
    /// Declared and observed with the same identity.
    Matched,
    /// Same operation and resource, different mutation action.
    ActionMismatch,
    /// Same operation and resource, entity-wide versus field scope.
    ScopeMismatch,
    /// The detected counterpart is captured against a stale revision.
    StaleEvidence,
    /// The detected counterpart is unverifiable.
    UnknownEvidence,
    /// The detected record claims a capability nothing declares.
    UnsupportedCapability,
    /// Two detected records contradict each other.
    ConflictingEvidence,
}

impl ComparisonState {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::DeclaredOnly => "declared-only",
            Self::DetectedOnly => "detected-only",
            Self::Matched => "matched",
            Self::ActionMismatch => "action-mismatch",
            Self::ScopeMismatch => "scope-mismatch",
            Self::StaleEvidence => "stale-evidence",
            Self::UnknownEvidence => "unknown-evidence",
            Self::UnsupportedCapability => "unsupported-capability",
            Self::ConflictingEvidence => "conflicting-evidence",
        }
    }

    /// The stable explanation template for one item of this state.
    pub(crate) fn explanation(self, key: &str) -> String {
        let frame = match self {
            Self::DeclaredOnly => "declared but not observed in evidence",
            Self::DetectedOnly => "detected but never declared",
            Self::Matched => "declared and detected with the same identity",
            Self::ActionMismatch => "declared and detected actions differ for the same resource",
            Self::ScopeMismatch => "declared and detected scopes differ (entity versus field)",
            Self::StaleEvidence => "detected evidence is stale for the declared effect",
            Self::UnknownEvidence => "detected evidence is unverifiable for the declared effect",
            Self::UnsupportedCapability => "detected capability claim has no declared counterpart",
            Self::ConflictingEvidence => "detected evidence records contradict each other",
        };
        format!("{key}: {frame}")
    }
}

/// The comparison request: the whole graph or one operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComparisonSpec {
    operation: Option<OperationId>,
}

impl ComparisonSpec {
    /// Compare everything.
    pub fn new() -> Self {
        Self { operation: None }
    }

    /// Compare only one operation's effects.
    pub fn for_operation(operation: OperationId) -> Self {
        Self {
            operation: Some(operation),
        }
    }

    /// The narrowed operation, when set.
    pub const fn operation(&self) -> Option<&OperationId> {
        self.operation.as_ref()
    }
}

impl Default for ComparisonSpec {
    fn default() -> Self {
        Self::new()
    }
}

/// One classified comparison result item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComparisonItem {
    state: ComparisonState,
    key: String,
    operation: OperationId,
    declared_confidence: Option<Confidence>,
    detected_confidence: Option<Confidence>,
    explanation: String,
}

impl ComparisonItem {
    /// The closed comparison state.
    pub const fn state(&self) -> ComparisonState {
        self.state
    }

    /// The canonical key of the compared effect.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The acting operation.
    pub const fn operation(&self) -> &OperationId {
        &self.operation
    }

    /// The declared-side confidence, when a declared edge took part.
    pub const fn declared_confidence(&self) -> Option<Confidence> {
        self.declared_confidence
    }

    /// The detected-side confidence, when a detected edge took part.
    pub const fn detected_confidence(&self) -> Option<Confidence> {
        self.detected_confidence
    }

    /// The stable, deterministic explanation.
    pub fn explanation(&self) -> &str {
        &self.explanation
    }
}

/// One finished comparison: sorted items, explicit completeness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Comparison {
    items: Vec<ComparisonItem>,
    complete: bool,
    bounded_reason: Option<&'static str>,
}

impl Comparison {
    /// The comparison items in canonical order.
    pub fn items(&self) -> &[ComparisonItem] {
        &self.items
    }

    /// Whether the comparison saw the complete input within every bound.
    pub const fn complete(&self) -> bool {
        self.complete
    }

    /// The recorded bound that stopped the comparison, when bounded.
    pub const fn bounded_reason(&self) -> Option<&'static str> {
        self.bounded_reason
    }
}

/// The kind identity used across comparison keys: the kind key with the
/// action folded in for field writes.
fn kind_with_action(kind: EffectKind) -> String {
    match kind {
        EffectKind::WriteField { action } => format!("write-field:{}", action.key()),
        other => other.key().to_owned(),
    }
}

/// One side's membership indexes for mismatch classification.
#[derive(Debug, Default)]
struct SideIndex {
    /// Operation-plus-resource pairs that carry any effect.
    resources: HashSet<(String, String)>,
    /// Operation/kind/resource triple to the field scopes seen (`""` is
    /// the entity-wide scope).
    scopes: HashMap<(String, String, String), HashSet<String>>,
}

impl SideIndex {
    fn build(edges: &[&EffectEdge]) -> Self {
        let mut index = Self::default();
        for edge in edges {
            let key = edge.key();
            index.resources.insert(op_resource(key));
            let triple = (
                key.operation().semantic_id().to_owned(),
                kind_with_action(key.kind()),
                key.subject().resource().as_str().to_owned(),
            );
            let field = key
                .subject()
                .field()
                .map_or(String::new(), |field| field.as_str().to_owned());
            index.scopes.entry(triple).or_default().insert(field);
        }
        index
    }

    /// Whether one mutation edge has a same-kind counterpart in the
    /// opposite scope (then `scope-mismatch`) or the same resource is
    /// only touched under other identities (then `action-mismatch`).
    fn classify_mutation(&self, edge: &EffectEdge) -> ComparisonState {
        let key = edge.key();
        let triple = (
            key.operation().semantic_id().to_owned(),
            kind_with_action(key.kind()),
            key.subject().resource().as_str().to_owned(),
        );
        let mine = key
            .subject()
            .field()
            .map_or(String::new(), |field| field.as_str().to_owned());
        if let Some(scopes) = self.scopes.get(&triple) {
            if scopes.iter().any(|field| *field != mine) {
                return ComparisonState::ScopeMismatch;
            }
        }
        ComparisonState::ActionMismatch
    }
}

/// Run the declared-versus-detected comparison over one graph.
///
/// A spec naming an operation the graph does not know is an explicit
/// `graph.unknown-node` failure, never an empty success.
pub(crate) fn compare(
    graph: &EffectGraph,
    spec: &ComparisonSpec,
) -> Result<super::Comparison, crate::diagnostics::DiagnosticSet> {
    if let Some(operation) = &spec.operation {
        if !graph.knows_operation(operation) {
            return Err(diagnostic::unknown_subject_set(operation.as_str()));
        }
    }
    let declared = select(graph.declared(), spec.operation.as_ref());
    let detected = select(graph.detected(), spec.operation.as_ref());
    let declared_side = SideIndex::build(&declared);
    let detected_side = SideIndex::build(&detected);

    let mut items: Vec<ComparisonItem> = Vec::new();
    let mut complete = true;
    let mut bounded_reason: Option<&'static str> = None;

    let mut declared_iter = declared.iter();
    let mut detected_iter = detected.iter();
    let mut left = declared_iter.next();
    let mut right = detected_iter.next();
    loop {
        match (left, right) {
            (None, None) => break,
            (Some(declared_edge), None) => {
                items.push(declared_item(declared_edge, &detected_side));
                left = declared_iter.next();
            }
            (None, Some(detected_edge)) => {
                items.push(detected_item(detected_edge, &declared_side));
                right = detected_iter.next();
            }
            (Some(declared_edge), Some(detected_edge)) => {
                match shape(declared_edge).cmp(&shape(detected_edge)) {
                    std::cmp::Ordering::Less => {
                        items.push(declared_item(declared_edge, &detected_side));
                        left = declared_iter.next();
                    }
                    std::cmp::Ordering::Greater => {
                        items.push(detected_item(detected_edge, &declared_side));
                        right = detected_iter.next();
                    }
                    std::cmp::Ordering::Equal => {
                        items.push(joined(declared_edge, detected_edge));
                        left = declared_iter.next();
                        right = detected_iter.next();
                    }
                }
            }
        }
        if items.len() >= MAX_COMPARISON_ITEMS {
            complete = false;
            bounded_reason = Some("comparison-limit");
            break;
        }
    }
    Ok(super::Comparison {
        items,
        complete,
        bounded_reason,
    })
}

/// The declared and detected edges under the spec, both already canonical.
fn select<'a>(edges: &'a [EffectEdge], operation: Option<&OperationId>) -> Vec<&'a EffectEdge> {
    edges
        .iter()
        .filter(|edge| operation.map_or(true, |operation| edge.key().operation() == operation))
        .collect()
}

/// The comparison join identity: operation semantic id, kind with its
/// action, resource id, and the exact field scope.
fn shape(edge: &EffectEdge) -> (String, String, String, String) {
    let key = edge.key();
    (
        key.operation().semantic_id().to_owned(),
        kind_with_action(key.kind()),
        key.subject().resource().as_str().to_owned(),
        key.subject()
            .field()
            .map_or(String::new(), |field| field.as_str().to_owned()),
    )
}

fn declared_item(declared_edge: &EffectEdge, detected_side: &SideIndex) -> ComparisonItem {
    let key = declared_edge.key();
    let state = if key.kind().is_mutation() && detected_side.resources.contains(&op_resource(key)) {
        detected_side.classify_mutation(declared_edge)
    } else {
        ComparisonState::DeclaredOnly
    };
    build_item(state, declared_edge, Some(declared_edge.confidence()), None)
}

fn detected_item(detected_edge: &EffectEdge, declared_side: &SideIndex) -> ComparisonItem {
    let key = detected_edge.key();
    let state = match degraded_state(detected_edge) {
        Some(state) => state,
        None if key.kind() == EffectKind::ExternalCall => ComparisonState::UnsupportedCapability,
        None if key.kind().is_mutation() && declared_side.resources.contains(&op_resource(key)) => {
            declared_side.classify_mutation(detected_edge)
        }
        None => ComparisonState::DetectedOnly,
    };
    build_item(state, detected_edge, None, Some(detected_edge.confidence()))
}

/// The evidence-degraded state of a detected edge, when degraded.
fn degraded_state(detected_edge: &EffectEdge) -> Option<ComparisonState> {
    match detected_edge.provenance() {
        super::EffectProvenance::Evidence { trust, .. } => match trust {
            TrustState::Stale => Some(ComparisonState::StaleEvidence),
            TrustState::Unknown | TrustState::Inferred => Some(ComparisonState::UnknownEvidence),
            _ => None,
        },
        _ => None,
    }
}

fn joined(declared_edge: &EffectEdge, detected_edge: &EffectEdge) -> ComparisonItem {
    let state = degraded_state(detected_edge).unwrap_or(ComparisonState::Matched);
    build_item(
        state,
        declared_edge,
        Some(declared_edge.confidence()),
        Some(detected_edge.confidence()),
    )
}

/// The coarser operation-plus-resource identity.
fn op_resource(key: &super::EffectKey) -> (String, String) {
    (
        key.operation().semantic_id().to_owned(),
        key.subject().resource().as_str().to_owned(),
    )
}

/// Assemble one item: the canonical edge key plus its stable explanation.
fn build_item(
    state: ComparisonState,
    edge: &EffectEdge,
    declared_confidence: Option<Confidence>,
    detected_confidence: Option<Confidence>,
) -> ComparisonItem {
    let key = edge.key();
    let canonical = key.to_canonical_string();
    ComparisonItem {
        state,
        explanation: state.explanation(&canonical),
        key: canonical,
        operation: key.operation().clone(),
        declared_confidence,
        detected_confidence,
    }
}
