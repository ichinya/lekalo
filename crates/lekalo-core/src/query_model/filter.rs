//! The minimal closed filter AST of the query-model attachment
//! (issue #64).
//!
//! Ten closed comparison operators over typed field references and
//! three logical combinators (`and`, `or`, `not`) with a bounded
//! grammar: depth, fanout, leaf count, literal size, and set size are
//! hard denial limits. There is no expression language here: richer
//! conditions reference the future #66 typed-expression family by
//! opaque reference, and nothing in this AST evaluates anything. The
//! AST never mutates: a query is read-only by construction.

use super::id::ParameterName;
use super::version::{
    MAX_FILTER_DEPTH, MAX_FILTER_LEAVES, MAX_FILTER_OPERANDS, MAX_LITERAL_BYTES, MAX_SET_ITEMS,
};

/// The closed comparison-operator vocabulary of one filter leaf.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum FilterOp {
    /// Field equals the value.
    Eq,
    /// Field does not equal the value.
    Ne,
    /// Field is ordered before the value.
    Lt,
    /// Field is ordered at most at the value.
    Le,
    /// Field is ordered after the value.
    Gt,
    /// Field is ordered at least at the value.
    Ge,
    /// Field is one of the closed set values.
    In,
    /// Field is none of the closed set values.
    NotIn,
    /// The field value is absent.
    IsNull,
    /// The field value is present.
    IsNotNull,
}

impl FilterOp {
    /// The exact wire text of the operator.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
            Self::In => "in",
            Self::NotIn => "not-in",
            Self::IsNull => "is-null",
            Self::IsNotNull => "is-not-null",
        }
    }

    /// The operator for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "eq" => Self::Eq,
            "ne" => Self::Ne,
            "lt" => Self::Lt,
            "le" => Self::Le,
            "gt" => Self::Gt,
            "ge" => Self::Ge,
            "in" => Self::In,
            "not-in" => Self::NotIn,
            "is-null" => Self::IsNull,
            "is-not-null" => Self::IsNotNull,
            _ => return None,
        })
    }

    /// Whether the operator carries a value member.
    pub const fn carries_value(self) -> bool {
        !matches!(self, Self::IsNull | Self::IsNotNull)
    }

    /// Whether the operator compares a scalar against a set literal.
    pub const fn expects_set(self) -> bool {
        matches!(self, Self::In | Self::NotIn)
    }
}

/// One closed filter value: a typed literal or a declared parameter
/// reference. Parameters resolve against the query input; their types
/// are checked against the field type at resolution time.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum FilterValue {
    /// A typed literal.
    Literal(Literal),
    /// A reference to one declared query parameter.
    Param(ParameterName),
}

/// One closed literal scalar. There are no floats, no binary blobs,
/// and no raw target text: every variant is bounded and closed.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Literal {
    /// `true` or `false`.
    Bool(bool),
    /// A bounded signed integer.
    Integer(i64),
    /// A canonical decimal literal (no floats).
    Decimal(String),
    /// A bounded UTF-8 string.
    Str(String),
    /// A calendar date literal.
    Date(String),
    /// A timestamp literal with an explicit offset.
    DateTime(String),
    /// A lowercase UUID literal.
    Uuid(String),
    /// A bounded absolute URI literal.
    Uri(String),
    /// A member of the field's declared enum type.
    Enum(String),
    /// A closed set of literals for `in`/`not-in`.
    Set(Vec<Literal>),
}

impl Literal {
    /// The closed literal-kind token for diagnostics and plans.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Bool(_) => "boolean",
            Self::Integer(_) => "integer",
            Self::Decimal(_) => "decimal",
            Self::Str(_) => "string",
            Self::Date(_) => "date",
            Self::DateTime(_) => "datetime",
            Self::Uuid(_) => "uuid",
            Self::Uri(_) => "uri",
            Self::Enum(_) => "enum-member",
            Self::Set(_) => "set",
        }
    }
}

/// One comparison leaf: a source-entity field, one operator, and the
/// value the operator carries (absent for the nullness operators).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilterLeaf {
    /// The source-entity field name.
    pub field: crate::scenario::id::FieldName,
    /// The closed comparison operator.
    pub op: FilterOp,
    /// The compared value; `None` exactly for `is-null`/`is-not-null`.
    pub value: Option<FilterValue>,
}

/// One closed filter expression: a comparison leaf or a bounded
/// logical combination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FilterExpr {
    /// One comparison leaf.
    Leaf(FilterLeaf),
    /// Every operand holds.
    And(Vec<FilterExpr>),
    /// At least one operand holds.
    Or(Vec<FilterExpr>),
    /// The operand does not hold.
    Not(Box<FilterExpr>),
}

/// One measured bound violation of a filter AST.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FilterBounds {
    /// The measured maximum nesting depth.
    pub depth: usize,
    /// The measured comparison-leaf count.
    pub leaves: usize,
}

impl FilterExpr {
    /// The measured depth and leaf count of the AST.
    pub(crate) fn measure(&self) -> FilterBounds {
        let mut bounds = FilterBounds {
            depth: 1,
            leaves: 0,
        };
        self.measure_into(1, &mut bounds);
        bounds
    }

    fn measure_into(&self, depth: usize, bounds: &mut FilterBounds) {
        bounds.depth = bounds.depth.max(depth);
        match self {
            Self::Leaf(_) => bounds.leaves += 1,
            Self::And(operands) | Self::Or(operands) => {
                for operand in operands {
                    operand.measure_into(depth + 1, bounds);
                }
            }
            Self::Not(operand) => operand.measure_into(depth + 1, bounds),
        }
    }

    /// Whether the AST stays inside the closed grammar bounds.
    pub fn within_bounds(&self) -> bool {
        let bounds = self.measure();
        bounds.depth <= MAX_FILTER_DEPTH
            && bounds.leaves <= MAX_FILTER_LEAVES
            && self.operands_within_bounds()
    }

    fn operands_within_bounds(&self) -> bool {
        match self {
            Self::Leaf(_) => true,
            Self::And(operands) | Self::Or(operands) => {
                operands.len() <= MAX_FILTER_OPERANDS
                    && operands.iter().all(Self::operands_within_bounds)
            }
            Self::Not(operand) => operand.operands_within_bounds(),
        }
    }

    /// The depth-first leaf references, in declared order.
    pub fn leaves(&self) -> Vec<&FilterLeaf> {
        let mut leaves = Vec::new();
        self.collect_leaves(&mut leaves);
        leaves
    }

    fn collect_leaves<'a>(&'a self, leaves: &mut Vec<&'a FilterLeaf>) {
        match self {
            Self::Leaf(leaf) => leaves.push(leaf),
            Self::And(operands) | Self::Or(operands) => {
                for operand in operands {
                    operand.collect_leaves(leaves);
                }
            }
            Self::Not(operand) => operand.collect_leaves(leaves),
        }
    }
}

/// The maximum measured size of one literal (bytes of one string, or
/// member count of one set).
pub(crate) fn literal_within_bounds(literal: &Literal) -> bool {
    match literal {
        Literal::Set(items) => {
            items.len() <= MAX_SET_ITEMS && items.iter().all(literal_within_bounds)
        }
        Literal::Str(text) | Literal::Decimal(text) | Literal::Enum(text) => {
            text.len() <= MAX_LITERAL_BYTES
        }
        Literal::Date(text)
        | Literal::DateTime(text)
        | Literal::Uuid(text)
        | Literal::Uri(text) => text.len() <= MAX_LITERAL_BYTES,
        Literal::Bool(_) | Literal::Integer(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str, op: FilterOp) -> FilterExpr {
        FilterExpr::Leaf(FilterLeaf {
            field: crate::scenario::id::FieldName::parse(name).expect("field"),
            op,
            value: None,
        })
    }

    #[test]
    fn measures_depth_and_leaves() {
        let tree = FilterExpr::And(vec![
            leaf("a", FilterOp::IsNull),
            FilterExpr::Not(Box::new(FilterExpr::Or(vec![
                leaf("b", FilterOp::IsNull),
                leaf("c", FilterOp::IsNull),
            ]))),
        ]);
        let bounds = tree.measure();
        assert_eq!(bounds.depth, 4);
        assert_eq!(bounds.leaves, 3);
        assert_eq!(tree.leaves().len(), 3);
    }

    #[test]
    fn operator_wire_round_trip() {
        for op in [
            FilterOp::Eq,
            FilterOp::Ne,
            FilterOp::Lt,
            FilterOp::Le,
            FilterOp::Gt,
            FilterOp::Ge,
            FilterOp::In,
            FilterOp::NotIn,
            FilterOp::IsNull,
            FilterOp::IsNotNull,
        ] {
            assert_eq!(FilterOp::parse(op.as_str()), Some(op));
        }
        assert_eq!(FilterOp::parse("contains"), None);
        assert!(!FilterOp::IsNull.carries_value());
        assert!(FilterOp::Eq.carries_value());
        assert!(FilterOp::In.expects_set());
        assert!(!FilterOp::Eq.expects_set());
    }

    #[test]
    fn set_literals_bound_members() {
        let ok = Literal::Set(vec![Literal::Integer(1), Literal::Integer(2)]);
        assert!(literal_within_bounds(&ok));
        let long = Literal::Str("x".repeat(MAX_LITERAL_BYTES + 1));
        assert!(!literal_within_bounds(&long));
    }

    #[test]
    fn bound_constants_are_locked() {
        assert_eq!(MAX_FILTER_DEPTH, 8);
        assert_eq!(MAX_FILTER_OPERANDS, 16);
        assert_eq!(MAX_FILTER_LEAVES, 64);
        assert_eq!(MAX_SET_ITEMS, 64);
    }
}
