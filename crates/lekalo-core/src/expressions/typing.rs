//! The exhaustive static type checker of the expression family
//! (issue #66).
//!
//! One pass computes the type of every node bottom-up against the
//! record's declared references and decides every operator
//! combination; an invalid combination is a registered
//! `expression.type-invalid` rejection with a closed detail token
//! and the record's declared span. Nullable references are confined
//! to `is-null`/`not-null` operands and to the `then` branch of an
//! `if` whose condition establishes the guard (`not-null(ref)`
//! directly or inside an `and` chain) — definite assignment, not
//! data-flow guesswork. Complexity bounds (depth, node count) reject
//! with `expression.complexity-limit`, whose contract routes the
//! author to the foreign implementation family (#30) instead of
//! growing the DSL. Nothing here evaluates anything.

use crate::diagnostics::DiagnosticSet;

use super::ast::{ExprNode, RefScope};
use super::builtin;
use super::diagnostic;
use super::types::{ExprType, ScalarType};
use super::version::{MAX_DEPTH, MAX_NODES};
use super::ExpressionRecord;

/// The closed detail vocabulary of the checker. Fixed tokens only;
/// never raw input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TypeDetail {
    /// The body type differs from the declared result type.
    BodyType,
    /// `eq`/`ne` operands disagree or are sets.
    EqOperands,
    /// Ordering operands disagree in type.
    CompareOperands,
    /// Ordering was applied to an unordered type (`bool`).
    CompareUnordered,
    /// `is-null`/`not-null` operand is not a declared nullable
    /// reference.
    NullOperand,
    /// A nullable reference was used outside a null check or an
    /// established guard.
    NullableReference,
    /// Set membership operand and set types disagree.
    MembershipType,
    /// A boolean combinator operand is not boolean.
    CombinandType,
    /// A boolean combinator operand count is outside 2–16.
    CombinandCount,
    /// The `if` condition is not boolean.
    IfCondition,
    /// The `if` branches disagree in type.
    IfBranch,
    /// An arithmetic operand combination is not in the closed table.
    Arithmetic,
    /// The built-in name is not registered.
    BuiltinUnknown,
    /// The built-in arity disagrees with the registry.
    BuiltinArity,
    /// A built-in argument type disagrees with the registry.
    BuiltinArg,
    /// The reference is not declared in the record's params.
    ParamUnknown,
}

impl TypeDetail {
    /// The exact wire token.
    pub const fn key(self) -> &'static str {
        match self {
            Self::BodyType => "body-type",
            Self::EqOperands => "eq-operands",
            Self::CompareOperands => "compare-operands",
            Self::CompareUnordered => "compare-unordered",
            Self::NullOperand => "null-operand",
            Self::NullableReference => "nullable-reference",
            Self::MembershipType => "membership-type",
            Self::CombinandType => "combinand-type",
            Self::CombinandCount => "combinand-count",
            Self::IfCondition => "if-condition",
            Self::IfBranch => "if-branch",
            Self::Arithmetic => "arithmetic-type",
            Self::BuiltinUnknown => "builtin-unknown",
            Self::BuiltinArity => "builtin-arity",
            Self::BuiltinArg => "builtin-arg",
            Self::ParamUnknown => "param-unknown",
        }
    }
}

/// One established not-null guard: the reference a condition proved
/// non-null for the branch it dominates.
pub(crate) type Guard = (RefScope, String);

/// The static type of one conditional's then-branch, computed with
/// the same definite-assignment guards the checker establishes from
/// its condition on top of the guards the enclosing context already
/// established. The projection renderer uses this to pick the branch
/// helper without re-deriving guard analysis.
pub(crate) fn conditional_then_type(
    record: &ExpressionRecord,
    inherited: &[Guard],
    condition: &ExprNode,
    then: &ExprNode,
) -> ExprType {
    let mut established: Vec<Guard> = inherited.to_vec();
    collect_guards(condition, &mut established);
    type_of(then, record, false, &established).unwrap_or(ExprType::Scalar(ScalarType::Int))
}

/// The static type of one node in record context under guards the
/// enclosing branches already established. The projection renderer
/// asks exactly this question while compiling a guarded then-branch:
/// a legal guarded subexpression answers with its real type, never
/// `NullableReference`, so the compiled helpers stay typed. The empty
/// guard list is the checker's own body-level question.
pub(crate) fn node_type_guards(
    node: &ExprNode,
    record: &ExpressionRecord,
    guards: &[Guard],
) -> Result<ExprType, TypeDetail> {
    type_of(node, record, false, guards)
}

/// Check one record: complexity bounds, kind/target coherence, and
/// the exhaustive static typing of the body.
pub(crate) fn check_record(record: &ExpressionRecord) -> Result<(), DiagnosticSet> {
    let span = record.span();
    let subject = record.id();
    let reject = |rule: &str, detail: &str| -> DiagnosticSet {
        diagnostic::rule_invalid(rule, detail, Some(subject), span)
    };
    if record.body.depth() > MAX_DEPTH {
        return Err(reject(diagnostic::COMPLEXITY_LIMIT, "depth"));
    }
    if record.body.node_count() > MAX_NODES {
        return Err(reject(diagnostic::COMPLEXITY_LIMIT, "nodes"));
    }
    match record.kind {
        super::ExpressionKind::Condition => {
            if record.result != ExprType::Scalar(ScalarType::Bool) {
                return Err(reject(diagnostic::CONTRACT_INVALID, "condition-result"));
            }
            if record.target.is_some() {
                return Err(reject(diagnostic::CONTRACT_INVALID, "condition-target"));
            }
        }
        super::ExpressionKind::Assignment => {
            let Some(target) = record.target.as_ref() else {
                return Err(reject(diagnostic::CONTRACT_INVALID, "assignment-target"));
            };
            if !matches!(target.scope, RefScope::Input | RefScope::Entity) {
                return Err(reject(
                    diagnostic::CONTRACT_INVALID,
                    "assignment-target-scope",
                ));
            }
            if record.result != target.ty {
                return Err(reject(diagnostic::CONTRACT_INVALID, "assignment-type"));
            }
        }
    }
    let body_type = type_of(&record.body, record, false, &[])
        .map_err(|detail| reject(diagnostic::TYPE_INVALID, detail.key()))?;
    if body_type != record.result {
        return Err(reject(diagnostic::TYPE_INVALID, TypeDetail::BodyType.key()));
    }
    Ok(())
}

/// Compute the static type of one node.
///
/// `nullable_slot` is true only for the direct operand position of
/// `is-null`/`not-null`. `guards` lists the references the enclosing
/// `if` condition has proven non-null for this branch.
fn type_of(
    node: &ExprNode,
    record: &ExpressionRecord,
    nullable_slot: bool,
    guards: &[Guard],
) -> Result<ExprType, TypeDetail> {
    let child =
        |node: &ExprNode| -> Result<ExprType, TypeDetail> { type_of(node, record, false, guards) };
    let scalar = |node: &ExprNode| -> Result<ExprType, TypeDetail> {
        let ty = child(node)?;
        if ty.as_scalar().is_none() {
            return Err(TypeDetail::EqOperands);
        }
        Ok(ty)
    };
    match node {
        ExprNode::Bool(_) => Ok(ExprType::Scalar(ScalarType::Bool)),
        ExprNode::Int(_) => Ok(ExprType::Scalar(ScalarType::Int)),
        ExprNode::Str(_) => Ok(ExprType::Scalar(ScalarType::Str)),
        ExprNode::DateTime(_) => Ok(ExprType::Scalar(ScalarType::DateTime)),
        ExprNode::Duration(_) => Ok(ExprType::Scalar(ScalarType::Duration)),
        ExprNode::Set(items) => {
            let inner = items
                .first()
                .map(|item| item.ty())
                .unwrap_or(ScalarType::Str);
            Ok(ExprType::Set(inner))
        }
        ExprNode::Ref { scope, field } => {
            let param = record
                .params
                .iter()
                .find(|param| param.scope == *scope && param.field == *field);
            let Some(param) = param else {
                return Err(TypeDetail::ParamUnknown);
            };
            let guarded = guards.iter().any(|(g_scope, g_field)| {
                g_scope == scope && g_field.as_str() == param.field.as_str()
            });
            if param.nullable && !nullable_slot && !guarded {
                return Err(TypeDetail::NullableReference);
            }
            Ok(param.ty.clone())
        }
        ExprNode::Now => Ok(ExprType::Scalar(ScalarType::DateTime)),
        ExprNode::Equal { left, right } | ExprNode::NotEqual { left, right } => {
            let left_type = scalar(left)?;
            let right_type = scalar(right)?;
            if left_type != right_type {
                return Err(TypeDetail::EqOperands);
            }
            Ok(ExprType::Scalar(ScalarType::Bool))
        }
        ExprNode::Less { left, right }
        | ExprNode::LessEqual { left, right }
        | ExprNode::Greater { left, right }
        | ExprNode::GreaterEqual { left, right } => {
            let left_type = scalar(left)?;
            let right_type = scalar(right)?;
            let (Some(left_scalar), Some(right_scalar)) =
                (left_type.as_scalar(), right_type.as_scalar())
            else {
                return Err(TypeDetail::CompareOperands);
            };
            if !left_scalar.ordered() || !right_scalar.ordered() {
                return Err(TypeDetail::CompareUnordered);
            }
            if left_type != right_type {
                return Err(TypeDetail::CompareOperands);
            }
            Ok(ExprType::Scalar(ScalarType::Bool))
        }
        ExprNode::IsNull { operand } | ExprNode::NotNull { operand } => match operand.as_ref() {
            ExprNode::Ref { scope, field } => {
                let param = record
                    .params
                    .iter()
                    .find(|param| param.scope == *scope && param.field == *field);
                match param {
                    Some(param) if param.nullable => Ok(ExprType::Scalar(ScalarType::Bool)),
                    Some(_) => Err(TypeDetail::NullOperand),
                    None => Err(TypeDetail::ParamUnknown),
                }
            }
            _ => Err(TypeDetail::NullOperand),
        },
        ExprNode::InSet { operand, set } | ExprNode::NotInSet { operand, set } => {
            let operand_type = child(operand)?;
            let set_type = child(set)?;
            let Some(inner) = operand_type.as_scalar() else {
                return Err(TypeDetail::MembershipType);
            };
            if set_type != ExprType::Set(inner) {
                return Err(TypeDetail::MembershipType);
            }
            Ok(ExprType::Scalar(ScalarType::Bool))
        }
        ExprNode::And(operands) | ExprNode::Or(operands) => {
            if operands.len() < 2 || operands.len() > super::version::MAX_OPERANDS {
                return Err(TypeDetail::CombinandCount);
            }
            for operand in operands {
                if child(operand)? != ExprType::Scalar(ScalarType::Bool) {
                    return Err(TypeDetail::CombinandType);
                }
            }
            Ok(ExprType::Scalar(ScalarType::Bool))
        }
        ExprNode::Not { operand } => {
            if child(operand)? != ExprType::Scalar(ScalarType::Bool) {
                return Err(TypeDetail::CombinandType);
            }
            Ok(ExprType::Scalar(ScalarType::Bool))
        }
        ExprNode::Add { left, right } => arithmetic(
            child(left)?,
            child(right)?,
            &[
                (ScalarType::Int, ScalarType::Int, ScalarType::Int),
                (
                    ScalarType::Duration,
                    ScalarType::Duration,
                    ScalarType::Duration,
                ),
                (
                    ScalarType::DateTime,
                    ScalarType::Duration,
                    ScalarType::DateTime,
                ),
                (
                    ScalarType::Duration,
                    ScalarType::DateTime,
                    ScalarType::DateTime,
                ),
            ],
        ),
        ExprNode::Subtract { left, right } => arithmetic(
            child(left)?,
            child(right)?,
            &[
                (ScalarType::Int, ScalarType::Int, ScalarType::Int),
                (
                    ScalarType::Duration,
                    ScalarType::Duration,
                    ScalarType::Duration,
                ),
                (
                    ScalarType::DateTime,
                    ScalarType::Duration,
                    ScalarType::DateTime,
                ),
                (
                    ScalarType::DateTime,
                    ScalarType::DateTime,
                    ScalarType::Duration,
                ),
            ],
        ),
        ExprNode::Multiply { left, right } => arithmetic(
            child(left)?,
            child(right)?,
            &[
                (ScalarType::Int, ScalarType::Int, ScalarType::Int),
                (ScalarType::Duration, ScalarType::Int, ScalarType::Duration),
                (ScalarType::Int, ScalarType::Duration, ScalarType::Duration),
            ],
        ),
        ExprNode::Divide { left, right } => arithmetic(
            child(left)?,
            child(right)?,
            &[
                (ScalarType::Int, ScalarType::Int, ScalarType::Int),
                (ScalarType::Duration, ScalarType::Int, ScalarType::Duration),
            ],
        ),
        ExprNode::Modulo { left, right } => arithmetic(
            child(left)?,
            child(right)?,
            &[(ScalarType::Int, ScalarType::Int, ScalarType::Int)],
        ),
        ExprNode::If {
            condition,
            then,
            otherwise,
        } => {
            if child(condition)? != ExprType::Scalar(ScalarType::Bool) {
                return Err(TypeDetail::IfCondition);
            }
            // Definite assignment: not-null guards in the condition
            // (directly or through and-chains) hold inside the `then`
            // branch only.
            let mut established: Vec<Guard> = guards.to_vec();
            collect_guards(condition, &mut established);
            let then_type = type_of(then, record, false, &established)?;
            // `if (is-null(x)) … else …`: the else branch runs only
            // when `x` is non-null, so the exact is-null leaf guards
            // the else branch too.
            let mut else_guards: Vec<Guard> = guards.to_vec();
            if let ExprNode::IsNull { operand } = condition.as_ref() {
                if let ExprNode::Ref { scope, field } = operand.as_ref() {
                    else_guards.push((*scope, field.clone()));
                }
            }
            let else_type = type_of(otherwise, record, false, &else_guards)?;
            if then_type != else_type {
                return Err(TypeDetail::IfBranch);
            }
            Ok(then_type)
        }
        ExprNode::Builtin { name, args } => {
            let Some(entry) = builtin::lookup(name) else {
                return Err(TypeDetail::BuiltinUnknown);
            };
            if args.len() != entry.args.len() {
                return Err(TypeDetail::BuiltinArity);
            }
            for (argument, expected) in args.iter().zip(entry.args.iter()) {
                if child(argument)? != ExprType::Scalar(*expected) {
                    return Err(TypeDetail::BuiltinArg);
                }
            }
            Ok(ExprType::Scalar(entry.result))
        }
    }
}

/// Collect the not-null guards one boolean condition establishes.
/// Only `not-null(ref)` leaves (directly, or as operands of `and`
/// chains) prove anything; negation and disjunction prove nothing.
pub(crate) fn collect_guards(node: &ExprNode, guards: &mut Vec<Guard>) {
    match node {
        ExprNode::NotNull { operand } => {
            if let ExprNode::Ref { scope, field } = operand.as_ref() {
                guards.push((*scope, field.clone()));
            }
        }
        ExprNode::And(operands) => {
            for operand in operands {
                collect_guards(operand, guards);
            }
        }
        _ => {}
    }
}

/// Resolve one arithmetic combination against the closed table.
fn arithmetic(
    left: ExprType,
    right: ExprType,
    table: &[(ScalarType, ScalarType, ScalarType)],
) -> Result<ExprType, TypeDetail> {
    let (Some(left_scalar), Some(right_scalar)) = (left.as_scalar(), right.as_scalar()) else {
        return Err(TypeDetail::Arithmetic);
    };
    for (from_left, from_right, result) in table.iter() {
        if left_scalar == *from_left && right_scalar == *from_right {
            return Ok(ExprType::Scalar(*result));
        }
    }
    Err(TypeDetail::Arithmetic)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_with(body: ExprNode, result: ExprType) -> ExpressionRecord {
        ExpressionRecord {
            id: "expr.planner/test".to_owned(),
            kind: super::super::ExpressionKind::Condition,
            description: None,
            params: vec![
                super::super::ParamDecl {
                    scope: RefScope::Input,
                    field: "estimate".to_owned(),
                    ty: ExprType::Scalar(ScalarType::Int),
                    nullable: false,
                },
                super::super::ParamDecl {
                    scope: RefScope::Input,
                    field: "note".to_owned(),
                    ty: ExprType::Scalar(ScalarType::Str),
                    nullable: true,
                },
            ],
            result,
            target: None,
            body,
            span: None,
        }
    }

    #[test]
    fn well_typed_bodies_pass() {
        let body = ExprNode::GreaterEqual {
            left: Box::new(ExprNode::Ref {
                scope: RefScope::Input,
                field: "estimate".to_owned(),
            }),
            right: Box::new(ExprNode::Int(1)),
        };
        assert!(check_record(&record_with(body, ExprType::Scalar(ScalarType::Bool))).is_ok());
    }

    #[test]
    fn invalid_operator_combinations_are_diagnosed() {
        // eq across types
        let mismatched = record_with(
            ExprNode::Equal {
                left: Box::new(ExprNode::Int(1)),
                right: Box::new(ExprNode::Bool(true)),
            },
            ExprType::Scalar(ScalarType::Bool),
        );
        assert!(check_record(&mismatched).is_err());
        // ordering on bool
        let unordered = record_with(
            ExprNode::Less {
                left: Box::new(ExprNode::Bool(false)),
                right: Box::new(ExprNode::Bool(true)),
            },
            ExprType::Scalar(ScalarType::Bool),
        );
        assert!(check_record(&unordered).is_err());
        // undeclared reference
        let unknown = record_with(
            ExprNode::Equal {
                left: Box::new(ExprNode::Ref {
                    scope: RefScope::Actor,
                    field: "role".to_owned(),
                }),
                right: Box::new(ExprNode::Str("admin".to_owned())),
            },
            ExprType::Scalar(ScalarType::Bool),
        );
        assert!(check_record(&unknown).is_err());
        // datetime + int is not in the closed table
        let temporal = record_with(
            ExprNode::Add {
                left: Box::new(ExprNode::DateTime(0)),
                right: Box::new(ExprNode::Int(1)),
            },
            ExprType::Scalar(ScalarType::DateTime),
        );
        assert!(check_record(&temporal).is_err());
        // body type differs from declared result
        let body_type = record_with(ExprNode::Int(1), ExprType::Scalar(ScalarType::Bool));
        assert!(check_record(&body_type).is_err());
    }

    #[test]
    fn guard_aware_type_queries_answer_guarded_subexpressions() {
        // The projection compiles the inside of an established guard;
        // its type queries must see the inherited guard, not start
        // from an empty list.
        let note = ExprNode::Ref {
            scope: RefScope::Input,
            field: "note".to_owned(),
        };
        assert_eq!(
            node_type_guards(
                &note,
                &record_with(note.clone(), ExprType::Scalar(ScalarType::Bool)),
                &[]
            ),
            Err(TypeDetail::NullableReference)
        );
        let guards: Vec<Guard> = vec![(RefScope::Input, "note".to_owned())];
        let record = record_with(note.clone(), ExprType::Scalar(ScalarType::Bool));
        assert_eq!(
            node_type_guards(&note, &record, &guards),
            Ok(ExprType::Scalar(ScalarType::Str))
        );
        // An unrelated guard does not unguard the reference.
        let other: Vec<Guard> = vec![(RefScope::Input, "estimate".to_owned())];
        assert_eq!(
            node_type_guards(&note, &record, &other),
            Err(TypeDetail::NullableReference)
        );
        // The conditional helper carries the inherited guards into the
        // then-branch it types.
        let condition = ExprNode::Bool(true);
        assert_eq!(
            conditional_then_type(&record, &guards, &condition, &note),
            ExprType::Scalar(ScalarType::Str)
        );
    }

    #[test]
    fn nullable_references_need_a_null_check_or_guard() {
        let nullable = record_with(
            ExprNode::Equal {
                left: Box::new(ExprNode::Ref {
                    scope: RefScope::Input,
                    field: "note".to_owned(),
                }),
                right: Box::new(ExprNode::Str("x".to_owned())),
            },
            ExprType::Scalar(ScalarType::Bool),
        );
        assert!(check_record(&nullable).is_err());
        // A not-null check alone is fine.
        let checked = record_with(
            ExprNode::NotNull {
                operand: Box::new(ExprNode::Ref {
                    scope: RefScope::Input,
                    field: "note".to_owned(),
                }),
            },
            ExprType::Scalar(ScalarType::Bool),
        );
        assert!(check_record(&checked).is_ok());
        // The guarded then-branch of an if may use the reference.
        let guarded = record_with(
            ExprNode::If {
                condition: Box::new(ExprNode::And(vec![
                    ExprNode::NotNull {
                        operand: Box::new(ExprNode::Ref {
                            scope: RefScope::Input,
                            field: "note".to_owned(),
                        }),
                    },
                    ExprNode::Bool(true),
                ])),
                then: Box::new(ExprNode::Equal {
                    left: Box::new(ExprNode::Ref {
                        scope: RefScope::Input,
                        field: "note".to_owned(),
                    }),
                    right: Box::new(ExprNode::Str("x".to_owned())),
                }),
                otherwise: Box::new(ExprNode::Bool(false)),
            },
            ExprType::Scalar(ScalarType::Bool),
        );
        assert!(check_record(&guarded).is_ok());
        // The else-branch gains no guard.
        let unguarded_else = record_with(
            ExprNode::If {
                condition: Box::new(ExprNode::NotNull {
                    operand: Box::new(ExprNode::Ref {
                        scope: RefScope::Input,
                        field: "note".to_owned(),
                    }),
                }),
                then: Box::new(ExprNode::Bool(true)),
                otherwise: Box::new(ExprNode::Equal {
                    left: Box::new(ExprNode::Ref {
                        scope: RefScope::Input,
                        field: "note".to_owned(),
                    }),
                    right: Box::new(ExprNode::Str("x".to_owned())),
                }),
            },
            ExprType::Scalar(ScalarType::Bool),
        );
        assert!(check_record(&unguarded_else).is_err());
        // Disjunction proves nothing.
        let disjunction = record_with(
            ExprNode::If {
                condition: Box::new(ExprNode::Or(vec![
                    ExprNode::NotNull {
                        operand: Box::new(ExprNode::Ref {
                            scope: RefScope::Input,
                            field: "note".to_owned(),
                        }),
                    },
                    ExprNode::Bool(true),
                ])),
                then: Box::new(ExprNode::Equal {
                    left: Box::new(ExprNode::Ref {
                        scope: RefScope::Input,
                        field: "note".to_owned(),
                    }),
                    right: Box::new(ExprNode::Str("x".to_owned())),
                }),
                otherwise: Box::new(ExprNode::Bool(false)),
            },
            ExprType::Scalar(ScalarType::Bool),
        );
        assert!(check_record(&disjunction).is_err());
    }
}
