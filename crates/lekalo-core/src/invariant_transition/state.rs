//! State spaces and closed value nodes of the invariant-transition
//! attachment (issue #63).
//!
//! A state space declares the closed state set of one entity with
//! explicit initial and terminal states and the two owner policies
//! (cycles and dead nonterminal states). Value nodes are the bounded
//! literal and reference leaves of the predicate AST: no floats, no
//! dynamic lookup, no target snippets, no runtime values.

use crate::scenario::id::{FieldName, SemanticId};

use super::id::StateRef;

/// Why one textual state space or value node is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeError {
    /// The value is outside the closed grammar or bounds.
    Shape,
    /// The declared state is unknown in the bound state space.
    UnknownState,
}

/// The explicit cycle policy of one state space.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CyclePolicy {
    /// Cycles are recorded as facts.
    Allow,
    /// Any cycle, including self-cycles, is invalid.
    Forbid,
}

impl CyclePolicy {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Forbid => "forbid",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "allow" => Some(Self::Allow),
            "forbid" => Some(Self::Forbid),
            _ => None,
        }
    }
}

/// The dead nonterminal state policy of one state space.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DeadPolicy {
    /// Dead nonterminal states are allowed.
    Allow,
    /// A reachable nonterminal state without outgoing transitions is
    /// invalid.
    Forbid,
}

impl DeadPolicy {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Forbid => "forbid",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "allow" => Some(Self::Allow),
            "forbid" => Some(Self::Forbid),
            _ => None,
        }
    }
}

/// One declared state with its optional initial and terminal flags.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateState {
    pub(crate) state_id: StateRef,
    pub(crate) initial: bool,
    pub(crate) terminal: bool,
}

impl StateState {
    /// The validated state identifier.
    pub fn state_id(&self) -> &StateRef {
        &self.state_id
    }

    /// Whether the state is declared initial.
    pub const fn initial(&self) -> bool {
        self.initial
    }

    /// Whether the state is declared terminal.
    pub const fn terminal(&self) -> bool {
        self.terminal
    }
}

/// One bounded state space: the closed state set of one entity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateSpace {
    pub(crate) state_space_id: SemanticId,
    pub(crate) entity: SemanticId,
    pub(crate) states: Vec<StateState>,
    pub(crate) cycle_policy: CyclePolicy,
    pub(crate) dead_policy: DeadPolicy,
}

impl StateSpace {
    /// The validated state-space identifier.
    pub fn state_space_id(&self) -> &SemanticId {
        &self.state_space_id
    }

    /// The entity this state space belongs to.
    pub fn entity(&self) -> &SemanticId {
        &self.entity
    }

    /// Every declared state.
    pub fn states(&self) -> &[StateState] {
        &self.states
    }

    /// The explicit cycle policy.
    pub const fn cycle_policy(&self) -> CyclePolicy {
        self.cycle_policy
    }

    /// The explicit dead nonterminal state policy.
    pub const fn dead_policy(&self) -> DeadPolicy {
        self.dead_policy
    }

    /// Whether `state` is declared in this space.
    pub fn contains(&self, state: &str) -> bool {
        self.states
            .iter()
            .any(|entry| entry.state_id.as_str() == state)
    }

    /// Whether at least one state is declared initial.
    pub fn has_initial(&self) -> bool {
        self.states.iter().any(|entry| entry.initial)
    }
}

/// One closed value node: the bounded leaf of the predicate AST.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueNode {
    /// The null literal.
    Null,
    /// A boolean literal.
    Boolean(bool),
    /// A bounded signed integer literal.
    Integer(i64),
    /// A bounded UTF-8 string literal.
    String(String),
    /// A canonical decimal literal (no floats).
    Decimal(String),
    /// A calendar date literal.
    Date(String),
    /// A timestamp literal with an explicit offset.
    DateTime(String),
    /// A lowercase UUID literal.
    Uuid(String),
    /// A bounded absolute URI literal.
    Uri(String),
    /// A member of one declared enum type.
    EnumMember {
        /// The declared enum type.
        type_ref: SemanticId,
        /// The declared member.
        value: String,
    },
    /// A typed field reference.
    Field {
        /// The optional owning entity; cross-entity access is refused
        /// by the semantic check when the invariant subject differs.
        entity: Option<SemanticId>,
        /// The referenced field.
        field: FieldName,
    },
    /// A transition input field reference.
    Input {
        /// The referenced input field.
        field: FieldName,
    },
    /// A prior-state field reference.
    Prior {
        /// The referenced prior-state field.
        field: FieldName,
    },
    /// The deterministic clock reference `now`.
    Now,
    /// A closed list of value nodes.
    List(Vec<ValueNode>),
    /// A closed object with key-sorted entries in canonical form.
    Object(Vec<(String, ValueNode)>),
}

/// One closed duration: unit and bounded amount.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Duration {
    pub(crate) unit: DurationUnit,
    pub(crate) amount: i64,
}

impl Duration {
    /// The declared unit.
    pub const fn unit(&self) -> DurationUnit {
        self.unit
    }

    /// The declared amount.
    pub const fn amount(&self) -> i64 {
        self.amount
    }
}

/// The closed duration units.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DurationUnit {
    /// Seconds.
    Seconds,
    /// Minutes.
    Minutes,
    /// Hours.
    Hours,
    /// Days.
    Days,
}

impl DurationUnit {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Seconds => "seconds",
            Self::Minutes => "minutes",
            Self::Hours => "hours",
            Self::Days => "days",
        }
    }

    /// Parse one wire key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "seconds" => Some(Self::Seconds),
            "minutes" => Some(Self::Minutes),
            "hours" => Some(Self::Hours),
            "days" => Some(Self::Days),
            _ => None,
        }
    }
}
