//! The minimal closed predicate AST of the invariant-transition
//! attachment (issue #63).
//!
//! Fourteen finite operations over the closed value nodes: comparison,
//! nullness, membership, quantifiers over collections, conjunction and
//! disjunction, temporal ordering and duration, counting, and set
//! membership. There is no expression language here: richer conditions
//! reference the future #66 typed-expression family by opaque
//! reference, and nothing in this AST evaluates anything.

use super::state::{Duration, ValueNode};

/// One closed predicate node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PredicateNode {
    /// `left` equals `right`.
    Equal {
        /// The left operand.
        left: ValueNode,
        /// The right operand.
        right: ValueNode,
    },
    /// `left` does not equal `right`.
    NotEqual {
        /// The left operand.
        left: ValueNode,
        /// The right operand.
        right: ValueNode,
    },
    /// The operand is null.
    IsNull {
        /// The operand.
        operand: ValueNode,
    },
    /// The operand is not null.
    NotNull {
        /// The operand.
        operand: ValueNode,
    },
    /// The operand is one of the closed values.
    InSet {
        /// The operand.
        operand: ValueNode,
        /// The closed member values.
        values: Vec<ValueNode>,
    },
    /// Every member of `from` satisfies `predicate`.
    All {
        /// The collection operand.
        from: ValueNode,
        /// The member predicate.
        predicate: Box<PredicateNode>,
    },
    /// At least one member of `from` satisfies `predicate`.
    Any {
        /// The collection operand.
        from: ValueNode,
        /// The member predicate.
        predicate: Box<PredicateNode>,
    },
    /// Every operand holds.
    And {
        /// The conjuncts.
        operands: Vec<PredicateNode>,
    },
    /// At least one operand holds.
    Or {
        /// The disjuncts.
        operands: Vec<PredicateNode>,
    },
    /// `left` is ordered before `right`.
    Before {
        /// The left operand.
        left: ValueNode,
        /// The right operand.
        right: ValueNode,
    },
    /// `left` is ordered after `right`.
    After {
        /// The left operand.
        left: ValueNode,
        /// The right operand.
        right: ValueNode,
    },
    /// `left` lies within the bounded duration.
    Within {
        /// The left operand.
        left: ValueNode,
        /// The bounded duration.
        duration: Duration,
    },
    /// The collection length lies in the closed range.
    Count {
        /// The collection operand.
        from: ValueNode,
        /// The inclusive minimum.
        min: i64,
        /// The inclusive maximum.
        max: i64,
    },
    /// The operand is a member of the set value.
    MemberOf {
        /// The operand.
        operand: ValueNode,
        /// The set value.
        set: ValueNode,
    },
}

impl PredicateNode {
    /// The depth of the subtree rooted here; one leaf operation has
    /// depth one.
    pub fn depth(&self) -> usize {
        let nested = |operands: &[PredicateNode]| {
            operands.iter().map(PredicateNode::depth).max().unwrap_or(0)
        };
        match self {
            Self::And { operands } | Self::Or { operands } => 1 + nested(operands),
            Self::All { predicate, .. } | Self::Any { predicate, .. } => 1 + predicate.depth(),
            Self::InSet { values, .. } => 1 + values.iter().map(value_depth).max().unwrap_or(0),
            Self::MemberOf { operand, set } => 1 + value_depth(operand).max(value_depth(set)),
            _ => 1,
        }
    }
}

/// The nesting depth of one value subtree (lists and objects count).
pub(crate) fn value_depth(value: &ValueNode) -> usize {
    match value {
        ValueNode::List(items) => 1 + items.iter().map(value_depth).max().unwrap_or(0),
        ValueNode::Object(entries) => {
            1 + entries
                .iter()
                .map(|(_, value)| value_depth(value))
                .max()
                .unwrap_or(0)
        }
        _ => 1,
    }
}
