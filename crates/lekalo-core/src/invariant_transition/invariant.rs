//! Invariant records of the invariant-transition attachment (issue
//! #63).
//!
//! Eleven closed kinds, each stating intended domain semantics only.
//! No invariant ever claims target enforcement: enforcement evidence
//! lives in verification mappings, never in the invariant itself.

use crate::extended_effects::identity::ErrorRef;
use crate::scenario::id::{FieldName, NamespacedId, SemanticId};

use super::capability::CapabilityRequirement;
use super::expr::PredicateNode;
use super::id::StateRef;
use super::trace::ScenarioRef;

/// One typed field reference with an optional owning entity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldRef {
    pub(crate) entity: Option<SemanticId>,
    pub(crate) field: FieldName,
}

impl FieldRef {
    /// The optional owning entity.
    pub fn entity(&self) -> Option<&SemanticId> {
        self.entity.as_ref()
    }

    /// The referenced field.
    pub fn field(&self) -> &FieldName {
        &self.field
    }
}

/// The closed invariant kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum InvariantKind {
    /// One field holds one value predicate.
    FieldValue,
    /// Two or more fields of one entity cohere.
    CrossField,
    /// A key combination is unique.
    Uniqueness,
    /// A collection size lies in a bounded range.
    Cardinality,
    /// Fields cohere under a deterministic clock ordering.
    Temporal,
    /// An aggregate and its members cohere.
    AggregateConsistency,
    /// Under a guard, a field set is required.
    ConditionalRequirement,
    /// At most one active member per partition key.
    OneActive,
    /// The subject state is one of the closed allowed states.
    MemberOfSet,
    /// Fields are immutable once the entity reaches a trigger state.
    ImmutableAfterState,
    /// A target capability is required with a minimum guarantee.
    TargetCapability,
}

impl InvariantKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::FieldValue => "field_value",
            Self::CrossField => "cross_field",
            Self::Uniqueness => "uniqueness",
            Self::Cardinality => "cardinality",
            Self::Temporal => "temporal",
            Self::AggregateConsistency => "aggregate_consistency",
            Self::ConditionalRequirement => "conditional_requirement",
            Self::OneActive => "one_active",
            Self::MemberOfSet => "member_of_set",
            Self::ImmutableAfterState => "immutable_after_state",
            Self::TargetCapability => "target_capability",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "field_value" => Some(Self::FieldValue),
            "cross_field" => Some(Self::CrossField),
            "uniqueness" => Some(Self::Uniqueness),
            "cardinality" => Some(Self::Cardinality),
            "temporal" => Some(Self::Temporal),
            "aggregate_consistency" => Some(Self::AggregateConsistency),
            "conditional_requirement" => Some(Self::ConditionalRequirement),
            "one_active" => Some(Self::OneActive),
            "member_of_set" => Some(Self::MemberOfSet),
            "immutable_after_state" => Some(Self::ImmutableAfterState),
            "target_capability" => Some(Self::TargetCapability),
            _ => None,
        }
    }
}

/// One declared invariant: stable identity, closed kind, and the
/// kind-bound declaration members.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invariant {
    pub(crate) invariant_id: SemanticId,
    pub(crate) state_space_id: SemanticId,
    pub(crate) kind: InvariantKind,
    pub(crate) description: Option<String>,
    pub(crate) fields: Vec<FieldRef>,
    pub(crate) predicate: Option<PredicateNode>,
    pub(crate) min: Option<i64>,
    pub(crate) max: Option<i64>,
    pub(crate) trigger_state: Option<StateRef>,
    pub(crate) allowed_states: Vec<StateRef>,
    pub(crate) partition: Option<FieldRef>,
    pub(crate) max_active: bool,
    pub(crate) aggregate_ref: Option<SemanticId>,
    pub(crate) required_fields: Vec<FieldName>,
    pub(crate) capability_requirement: Option<CapabilityRequirement>,
    pub(crate) conflict_error_ref: Option<ErrorRef>,
    pub(crate) error_refs: Vec<ErrorRef>,
    pub(crate) requirement_refs: Vec<NamespacedId>,
    pub(crate) scenario_refs: Vec<ScenarioRef>,
    pub(crate) capability_refs: Vec<NamespacedId>,
}

impl Invariant {
    /// The stable invariant identifier.
    pub fn invariant_id(&self) -> &SemanticId {
        &self.invariant_id
    }

    /// The state space this invariant constrains.
    pub fn state_space_id(&self) -> &SemanticId {
        &self.state_space_id
    }

    /// The closed invariant kind.
    pub const fn kind(&self) -> InvariantKind {
        self.kind
    }

    /// The bounded description, when declared.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// The bound field references.
    pub fn fields(&self) -> &[FieldRef] {
        &self.fields
    }

    /// The bound predicate, when declared.
    pub fn predicate(&self) -> Option<&PredicateNode> {
        self.predicate.as_ref()
    }

    /// The cardinality minimum, when declared.
    pub const fn min(&self) -> Option<i64> {
        self.min
    }

    /// The cardinality maximum, when declared.
    pub const fn max(&self) -> Option<i64> {
        self.max
    }

    /// The immutable-after trigger state, when declared.
    pub fn trigger_state(&self) -> Option<&StateRef> {
        self.trigger_state.as_ref()
    }

    /// The member-of-set allowed states.
    pub fn allowed_states(&self) -> &[StateRef] {
        &self.allowed_states
    }

    /// The one-active partition key, when declared.
    pub fn partition(&self) -> Option<&FieldRef> {
        self.partition.as_ref()
    }

    /// Whether the one-active `maxActive: 1` member is declared.
    pub const fn max_active(&self) -> bool {
        self.max_active
    }

    /// The aggregate reference, when declared.
    pub fn aggregate_ref(&self) -> Option<&SemanticId> {
        self.aggregate_ref.as_ref()
    }

    /// The conditionally required fields.
    pub fn required_fields(&self) -> &[FieldName] {
        &self.required_fields
    }

    /// The target-capability requirement, when declared.
    pub fn capability_requirement(&self) -> Option<&CapabilityRequirement> {
        self.capability_requirement.as_ref()
    }

    /// The uniqueness conflict error reference, when declared.
    pub fn conflict_error_ref(&self) -> Option<&ErrorRef> {
        self.conflict_error_ref.as_ref()
    }

    /// The typed error references.
    pub fn error_refs(&self) -> &[ErrorRef] {
        &self.error_refs
    }

    /// The requirement references.
    pub fn requirement_refs(&self) -> &[NamespacedId] {
        &self.requirement_refs
    }

    /// The scenario references.
    pub fn scenario_refs(&self) -> &[ScenarioRef] {
        &self.scenario_refs
    }

    /// The capability requirement references.
    pub fn capability_refs(&self) -> &[NamespacedId] {
        &self.capability_refs
    }
}
