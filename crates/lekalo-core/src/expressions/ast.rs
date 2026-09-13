//! The closed expression AST (issue #66).
//!
//! Sixteen node kinds: six literals (including one homogeneous set
//! literal), typed references, the deterministic clock `now`, the
//! equality/comparison/null/membership predicates, the boolean
//! combinators, checked numeric and temporal arithmetic, the
//! branch-without-loop conditional, and calls into the closed
//! built-in registry. Every node is plain data; nothing here
//! evaluates anything and nothing can represent an arbitrary call,
//! a loop, recursion, reflection, filesystem/network access, or a
//! target code snippet — those shapes simply have no node.

use super::types::Scalar;

/// The closed reference scopes: the operation input, the acting
/// subject, the prior entity state, and the operation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RefScope {
    /// `input.*`: fields of the operation input.
    Input,
    /// `actor.*`: fields of the acting subject.
    Actor,
    /// `entity.*`: fields of the prior entity state.
    Entity,
    /// `result.*`: fields of the operation result.
    Result,
}

impl RefScope {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Actor => "actor",
            Self::Entity => "entity",
            Self::Result => "result",
        }
    }

    /// Parse one wire key.
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "input" => Some(Self::Input),
            "actor" => Some(Self::Actor),
            "entity" => Some(Self::Entity),
            "result" => Some(Self::Result),
            _ => None,
        }
    }
}

/// One closed expression node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExprNode {
    /// A boolean literal.
    Bool(bool),
    /// A bounded integer literal.
    Int(i64),
    /// A bounded BMP-only string literal.
    Str(String),
    /// A canonical UTC datetime literal, in epoch seconds.
    DateTime(i64),
    /// A duration literal in seconds.
    Duration(i64),
    /// A sorted, duplicate-free set literal of one scalar type.
    Set(Vec<Scalar>),
    /// A typed reference into one declared scope.
    Ref {
        /// The referenced scope.
        scope: RefScope,
        /// The referenced field (the bare name).
        field: String,
    },
    /// The deterministic clock reference; the evaluator supplies the
    /// instant, tests inject it, and no expression can read a wall
    /// clock.
    Now,
    /// `left` equals `right`.
    Equal {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// `left` does not equal `right`.
    NotEqual {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    Less {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// `left` is ordered before or equal to `right`.
    LessEqual {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// `left` is ordered after `right`.
    Greater {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// `left` is ordered after or equal to `right`.
    GreaterEqual {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// The nullable reference operand is null.
    IsNull {
        /// The nullable reference operand.
        operand: Box<ExprNode>,
    },
    /// The nullable reference operand is not null.
    NotNull {
        /// The nullable reference operand.
        operand: Box<ExprNode>,
    },
    /// The operand is a member of the set value.
    InSet {
        /// The membership operand.
        operand: Box<ExprNode>,
        /// The set-typed operand.
        set: Box<ExprNode>,
    },
    /// The operand is not a member of the set value.
    NotInSet {
        /// The membership operand.
        operand: Box<ExprNode>,
        /// The set-typed operand.
        set: Box<ExprNode>,
    },
    /// Every operand holds (2–16).
    And(Vec<ExprNode>),
    /// At least one operand holds (2–16).
    Or(Vec<ExprNode>),
    /// The operand does not hold.
    Not {
        /// The negated operand.
        operand: Box<ExprNode>,
    },
    /// `datetime+duration`.
    Add {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// Checked subtraction: `int-int`, `duration-duration`,
    /// `datetime-duration`, `datetime-datetime`.
    Subtract {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// Checked multiplication: `int*int`, `duration*int`, `int*duration`.
    Multiply {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// Truncated division with a nonzero divisor: `int/int`,
    /// `duration/int`.
    Divide {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// Truncated remainder with a nonzero divisor: `int%int`.
    Modulo {
        /// The left operand.
        left: Box<ExprNode>,
        /// The right operand.
        right: Box<ExprNode>,
    },
    /// The ternary conditional: `condition ? then : else`, branches
    If {
        /// The boolean condition.
        condition: Box<ExprNode>,
        /// The branch taken when the condition holds.
        then: Box<ExprNode>,
        /// The branch taken otherwise.
        otherwise: Box<ExprNode>,
    },
    /// One call into the closed built-in registry.
    Builtin {
        /// The registered built-in name.
        name: String,
        /// The closed argument list.
        args: Vec<ExprNode>,
    },
}

impl ExprNode {
    /// The node count of this subtree.
    pub fn node_count(&self) -> usize {
        let children: Vec<&ExprNode> = match self {
            Self::Bool(_)
            | Self::Int(_)
            | Self::Str(_)
            | Self::DateTime(_)
            | Self::Duration(_)
            | Self::Set(_)
            | Self::Ref { .. }
            | Self::Now => Vec::new(),
            Self::Equal { left, right }
            | Self::NotEqual { left, right }
            | Self::Less { left, right }
            | Self::LessEqual { left, right }
            | Self::Greater { left, right }
            | Self::GreaterEqual { left, right }
            | Self::Add { left, right }
            | Self::Subtract { left, right }
            | Self::Multiply { left, right }
            | Self::Divide { left, right }
            | Self::Modulo { left, right } => vec![left, right],
            Self::IsNull { operand } | Self::NotNull { operand } | Self::Not { operand } => {
                vec![operand]
            }
            Self::InSet { operand, set } => vec![operand, set],
            Self::NotInSet { operand, set } => vec![operand, set],
            Self::And(operands) | Self::Or(operands) => operands.iter().collect(),
            Self::If {
                condition,
                then,
                otherwise,
            } => vec![condition, then, otherwise],
            Self::Builtin { args, .. } => args.iter().collect(),
        };
        1 + children
            .iter()
            .map(|child| child.node_count())
            .sum::<usize>()
    }

    /// The maximum nesting depth of this subtree (a leaf is 1).
    pub fn depth(&self) -> usize {
        let children: Vec<&ExprNode> = match self {
            Self::Bool(_)
            | Self::Int(_)
            | Self::Str(_)
            | Self::DateTime(_)
            | Self::Duration(_)
            | Self::Set(_)
            | Self::Ref { .. }
            | Self::Now => Vec::new(),
            Self::Equal { left, right }
            | Self::NotEqual { left, right }
            | Self::Less { left, right }
            | Self::LessEqual { left, right }
            | Self::Greater { left, right }
            | Self::GreaterEqual { left, right }
            | Self::Add { left, right }
            | Self::Subtract { left, right }
            | Self::Multiply { left, right }
            | Self::Divide { left, right }
            | Self::Modulo { left, right } => vec![left, right],
            Self::IsNull { operand } | Self::NotNull { operand } | Self::Not { operand } => {
                vec![operand]
            }
            Self::InSet { operand, set } => vec![operand, set],
            Self::NotInSet { operand, set } => vec![operand, set],
            Self::And(operands) | Self::Or(operands) => operands.iter().collect(),
            Self::If {
                condition,
                then,
                otherwise,
            } => vec![condition, then, otherwise],
            Self::Builtin { args, .. } => args.iter().collect(),
        };
        1 + children
            .iter()
            .map(|child| child.depth())
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_and_count_agree_on_simple_trees() {
        let leaf = ExprNode::Bool(true);
        assert_eq!(leaf.depth(), 1);
        assert_eq!(leaf.node_count(), 1);
        let tree = ExprNode::And(vec![
            ExprNode::Not {
                operand: Box::new(leaf.clone()),
            },
            ExprNode::Equal {
                left: Box::new(ExprNode::Int(1)),
                right: Box::new(ExprNode::Int(2)),
            },
        ]);
        assert_eq!(tree.depth(), 3);
        assert_eq!(tree.node_count(), 6);
    }

    #[test]
    fn literals_carry_their_bounds_at_the_wire_layer() {
        // The wire decoder validates literal bounds; the AST type
        // itself keeps only accepted values.
        assert_eq!(
            ExprNode::Int(crate::expressions::version::MAX_INT).node_count(),
            1
        );
    }

    #[test]
    fn scope_keys_round_trip() {
        for key in ["input", "actor", "entity", "result"] {
            assert_eq!(RefScope::parse(key).expect("scope").key(), key);
        }
        assert!(RefScope::parse("context").is_none());
    }
}
