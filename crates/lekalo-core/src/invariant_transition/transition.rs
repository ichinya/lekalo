//! Transition records of the invariant-transition attachment (issue
//! #63).
//!
//! A transition declares one legal state change of one state space:
//! the closed from-state set, the target state, the owning command,
//! the ordered (or explicitly parallel) assignment set, operation-entry
//! preconditions, the authorization policy reference, and the
//! branch-specific error references. Preconditions, invariants, and
//! authorization stay three separate concerns; nothing here executes.

use crate::extended_effects::identity::ErrorRef;
use crate::scenario::id::{FieldName, NamespacedId, SemanticId};

use super::expr::PredicateNode;
use super::id::{ExpressionRef, StateRef};
use super::state::ValueNode;
use super::trace::ScenarioRef;

/// One closed assignment source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssignmentValue {
    /// A bounded literal value.
    Literal(ValueNode),
    /// A transition input field.
    Input {
        /// The referenced input field.
        field: FieldName,
    },
    /// A prior-state field.
    Prior {
        /// The referenced prior-state field.
        field: FieldName,
    },
    /// The deterministic clock reference `now`.
    Now,
    /// A #66 typed-expression reference.
    Expression {
        /// The referenced expression.
        expression_ref: ExpressionRef,
    },
}

/// One declared assignment: the written field and its closed source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Assignment {
    pub(crate) field: FieldName,
    pub(crate) value: AssignmentValue,
}

impl Assignment {
    /// The written field.
    pub fn field(&self) -> &FieldName {
        &self.field
    }

    /// The closed assignment source.
    pub const fn value(&self) -> &AssignmentValue {
        &self.value
    }
}

/// One declared state transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    pub(crate) transition_id: SemanticId,
    pub(crate) state_space_id: SemanticId,
    pub(crate) from_states: Vec<StateRef>,
    pub(crate) to_state: StateRef,
    pub(crate) command: SemanticId,
    pub(crate) assignments: Vec<Assignment>,
    pub(crate) parallel: bool,
    pub(crate) preconditions: Vec<PredicateNode>,
    pub(crate) policy_ref: Option<NamespacedId>,
    pub(crate) error_refs: Vec<ErrorRef>,
    pub(crate) requirement_refs: Vec<NamespacedId>,
    pub(crate) scenario_refs: Vec<ScenarioRef>,
    pub(crate) capability_refs: Vec<NamespacedId>,
}

impl Transition {
    /// The stable transition identifier.
    pub fn transition_id(&self) -> &SemanticId {
        &self.transition_id
    }

    /// The state space this transition belongs to.
    pub fn state_space_id(&self) -> &SemanticId {
        &self.state_space_id
    }

    /// The canonically sorted from-state set.
    pub fn from_states(&self) -> &[StateRef] {
        &self.from_states
    }

    /// The target state.
    pub fn to_state(&self) -> &StateRef {
        &self.to_state
    }

    /// The owning command.
    pub fn command(&self) -> &SemanticId {
        &self.command
    }

    /// The declared assignment set in behavioral order.
    pub fn assignments(&self) -> &[Assignment] {
        &self.assignments
    }

    /// Whether the assignment set is explicitly parallel.
    pub const fn parallel(&self) -> bool {
        self.parallel
    }

    /// The operation-entry preconditions.
    pub fn preconditions(&self) -> &[PredicateNode] {
        &self.preconditions
    }

    /// The authorization policy reference, when declared.
    pub fn policy_ref(&self) -> Option<&NamespacedId> {
        self.policy_ref.as_ref()
    }

    /// The branch-specific error references.
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
