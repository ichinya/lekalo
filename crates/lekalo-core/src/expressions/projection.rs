//! Compile validated expressions to the versioned projection received by external generators.
use super::ast::ExprNode;
use super::types::{ExprType, Scalar, ScalarType};
use super::typing::{collect_guards, conditional_then_type, node_type_guards, Guard};
use super::version::{BUILTIN_SEMANTICS_VERSION, FAMILY, VERSION};
use super::ExpressionRecord;
use super::ExpressionsAttachment;

/// The projection identity header every generated program carries.
pub fn projection_identity() -> String {
    format!(
        "{}@{}:builtin-semantics:{}",
        FAMILY, VERSION, BUILTIN_SEMANTICS_VERSION
    )
}

/// Render a validated attachment using the project's selected expression service.
pub fn render_program(
    root: &std::path::Path,
    attachment: &ExpressionsAttachment,
    target: &str,
) -> crate::runtime::Result<String> {
    use crate::runtime::{error, services};
    if !crate::runtime::config::token(target) || target.contains('/') {
        return Err(error("expression-target-invalid"));
    }
    let service = services::one(root, &format!("expressions/{target}"))?;
    let records: Vec<_> = attachment.expressions().iter().map(|record| {
        let params: Vec<_> = record.params.iter().map(|p| serde_json::json!({
            "scope":p.scope.key(), "field":p.field, "ty":p.ty, "nullable":p.nullable
        })).collect();
        serde_json::json!({"id":record.id(), "params":params, "result":record.result(), "body":compile(record,record.body())})
    }).collect();
    let reply = service.invoke("expressions.render", serde_json::json!({
        "schema":"lekalo/expression-projection/v1", "identity":projection_identity(), "records":records
    }))?;
    if reply["schema"] != "lekalo/expression-program/v1" {
        return Err(error("expression-program-version"));
    }
    reply["program"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| error("expression-program-invalid"))
}

/// The typed intermediate the per-target printers consume.
#[derive(serde::Serialize, Clone, Debug)]
enum Code {
    /// A plain (untyped) integer literal; only the `lekCmpStr`
    /// zero comparison uses it, where a Node `BigInt` would never
    /// equal the helper's plain number.
    PlainInt(i64),
    /// An integer literal (an int, duration seconds, or datetime
    /// seconds — always in a statically typed position).
    Int(i64),
    /// A string literal.
    Str(String),
    /// A boolean literal.
    Bool(bool),
    /// A prelude call with one shared name.
    Call(&'static str, Vec<Code>),
    /// An ordered comparison (never on booleans; string operands
    /// route through `lekCmpStr` at compile time).
    Compare(&'static str, Box<Code>, Box<Code>),
    /// Strict equality (or inequality when negated).
    Equal(bool, Box<Code>, Box<Code>),
    /// Conjunction over two or more operands.
    And(Vec<Code>),
    /// Disjunction over two or more operands.
    Or(Vec<Code>),
    /// Negation.
    Not(Box<Code>),
    /// The branch without a loop; the branch type selects the typed
    /// Go helper.
    If(Box<Code>, Box<Code>, Box<Code>, BranchType),
}

/// The branch typing the Go printer needs. Every branch is a
/// concrete Go type: an `if` over sets closes over the element type
/// so the encoded result stays a typed slice, never an erased
/// `any`.
#[derive(serde::Serialize, Clone, Copy, Debug)]
enum BranchType {
    /// An integer-valued branch (int, duration, or datetime).
    Int,
    /// A string-valued branch.
    Str,
    /// A boolean-valued branch.
    Bool,
    /// A set-valued branch with one concrete element type.
    Set(ScalarType),
}

/// Compile one record body against its declared references.
fn compile(record: &ExpressionRecord, node: &ExprNode) -> Code {
    compile_guards(record, node, &[])
}

/// Compile one node under the not-null guards the enclosing branches
/// have already established. The guards mirror exactly what the
/// static checker established for this position, so every type query
/// below answers with the node's real type — a guarded nullable
/// reference compiles as its declared type instead of falling into
/// an integer default.
fn compile_guards(record: &ExpressionRecord, node: &ExprNode, guards: &[Guard]) -> Code {
    let ty = |node: &ExprNode| -> ExprType {
        node_type_guards(node, record, guards).unwrap_or(ExprType::Scalar(ScalarType::Int))
    };
    match node {
        ExprNode::Bool(value) => Code::Bool(*value),
        ExprNode::Int(value) => Code::Int(*value),
        ExprNode::Str(value) => Code::Str(value.clone()),
        ExprNode::DateTime(seconds) => Code::Int(*seconds),
        ExprNode::Duration(seconds) => Code::Int(*seconds),
        // A set literal is legal in every set-typed position: the
        // membership operand, the whole body of a `set:*`-result
        // assignment, and the branches of a set-typed conditional.
        // All route through the typed literal maker with the wire
        // (already sorted, duplicate-free) items, so every target
        // computes the same concrete set.
        ExprNode::Set(_) => set_operand(record, guards, node),
        ExprNode::Ref { scope, field } => {
            let declared = record
                .params
                .iter()
                .find(|param| param.scope == *scope && param.field == *field)
                .map(|param| param.ty.clone())
                .unwrap_or_else(|| ty(node));
            Code::Call(
                extract_helper(&declared),
                vec![Code::Str(scope.key().to_owned()), Code::Str(field.clone())],
            )
        }
        ExprNode::Now => Code::Call("lekClock", Vec::new()),
        ExprNode::Equal { left, right } => {
            if is_string(record, guards, left) {
                Code::Call(
                    "lekCmpStr",
                    vec![
                        compile_guards(record, left, guards),
                        compile_guards(record, right, guards),
                    ],
                )
                .compare_for_eq()
            } else {
                Code::Equal(
                    false,
                    Box::new(compile_guards(record, left, guards)),
                    Box::new(compile_guards(record, right, guards)),
                )
            }
        }
        ExprNode::NotEqual { left, right } => {
            if is_string(record, guards, left) {
                Code::Call(
                    "lekCmpStr",
                    vec![
                        compile_guards(record, left, guards),
                        compile_guards(record, right, guards),
                    ],
                )
                .compare_for_ne()
            } else {
                Code::Equal(
                    true,
                    Box::new(compile_guards(record, left, guards)),
                    Box::new(compile_guards(record, right, guards)),
                )
            }
        }
        ExprNode::Less { left, right } => compare(record, guards, "<", left, right),
        ExprNode::LessEqual { left, right } => compare(record, guards, "<=", left, right),
        ExprNode::Greater { left, right } => compare(record, guards, ">", left, right),
        ExprNode::GreaterEqual { left, right } => compare(record, guards, ">=", left, right),
        ExprNode::IsNull { operand } => match operand.as_ref() {
            ExprNode::Ref { scope, field } => Code::Call(
                "lekIsNull",
                vec![Code::Str(scope.key().to_owned()), Code::Str(field.clone())],
            ),
            _ => Code::Bool(true),
        },
        ExprNode::NotNull { operand } => match operand.as_ref() {
            ExprNode::Ref { scope, field } => Code::Call(
                "lekNotNull",
                vec![Code::Str(scope.key().to_owned()), Code::Str(field.clone())],
            ),
            _ => Code::Bool(false),
        },
        // Membership evaluates the operand before the set, exactly
        // like the reference evaluator: every target evaluates call
        // arguments left to right, so the operand is the first
        // argument and the set the second — when both children fail,
        // the operand's domain token is the one every target and the
        // reference observe.
        ExprNode::InSet { operand, set } => Code::Call(
            set_helper(record, guards, set),
            vec![
                compile_guards(record, operand, guards),
                set_operand(record, guards, set),
            ],
        ),
        ExprNode::NotInSet { operand, set } => Code::Not(Box::new(Code::Call(
            set_helper(record, guards, set),
            vec![
                compile_guards(record, operand, guards),
                set_operand(record, guards, set),
            ],
        ))),
        ExprNode::And(operands) => Code::And(
            operands
                .iter()
                .map(|operand| compile_guards(record, operand, guards))
                .collect(),
        ),
        ExprNode::Or(operands) => Code::Or(
            operands
                .iter()
                .map(|operand| compile_guards(record, operand, guards))
                .collect(),
        ),
        ExprNode::Not { operand } => Code::Not(Box::new(compile_guards(record, operand, guards))),
        ExprNode::Add { left, right } => Code::Call(
            add_name(record, guards, left, right),
            vec![
                compile_guards(record, left, guards),
                compile_guards(record, right, guards),
            ],
        ),
        ExprNode::Subtract { left, right } => Code::Call(
            sub_name(record, guards, left, right),
            vec![
                compile_guards(record, left, guards),
                compile_guards(record, right, guards),
            ],
        ),
        ExprNode::Multiply { left, right } => Code::Call(
            mul_name(record, guards, left, right),
            vec![
                compile_guards(record, left, guards),
                compile_guards(record, right, guards),
            ],
        ),
        ExprNode::Divide { left, right } => Code::Call(
            div_name(record, guards, left),
            vec![
                compile_guards(record, left, guards),
                compile_guards(record, right, guards),
            ],
        ),
        ExprNode::Modulo { left, right } => Code::Call(
            "lekMod",
            vec![
                compile_guards(record, left, guards),
                compile_guards(record, right, guards),
            ],
        ),
        ExprNode::If {
            condition,
            then,
            otherwise,
        } => {
            // The branches inherit exactly the guards the checker
            // established: the then-branch gains the condition's
            // not-null proofs, and the else branch of `if (is-null(x))`
            // gains the guarded leaf.
            let mut then_guards: Vec<Guard> = guards.to_vec();
            collect_guards(condition, &mut then_guards);
            let mut else_guards: Vec<Guard> = guards.to_vec();
            if let ExprNode::IsNull { operand } = condition.as_ref() {
                if let ExprNode::Ref { scope, field } = operand.as_ref() {
                    else_guards.push((*scope, field.clone()));
                }
            }
            let branch = match conditional_then_type(record, guards, condition, then) {
                ExprType::Scalar(ScalarType::Str) => BranchType::Str,
                ExprType::Scalar(ScalarType::Bool) => BranchType::Bool,
                ExprType::Scalar(_) => BranchType::Int,
                ExprType::Set(inner) => BranchType::Set(inner),
            };
            Code::If(
                Box::new(compile_guards(record, condition, guards)),
                Box::new(compile_guards(record, then, &then_guards)),
                Box::new(compile_guards(record, otherwise, &else_guards)),
                branch,
            )
        }
        ExprNode::Builtin { name, args } => Code::Call(
            builtin_name(name),
            args.iter()
                .map(|arg| compile_guards(record, arg, guards))
                .collect(),
        ),
    }
}

/// One ordered comparison; string operands compare through the
/// prelude byte-order helper so PHP never applies numeric-string
/// comparison.
fn compare(
    record: &ExpressionRecord,
    guards: &[Guard],
    op: &'static str,
    left: &ExprNode,
    right: &ExprNode,
) -> Code {
    if is_string(record, guards, left) {
        Code::Compare(
            op,
            Box::new(Code::Call(
                "lekCmpStr",
                vec![
                    compile_guards(record, left, guards),
                    compile_guards(record, right, guards),
                ],
            )),
            Box::new(Code::Int(0)),
        )
    } else {
        Code::Compare(
            op,
            Box::new(compile_guards(record, left, guards)),
            Box::new(compile_guards(record, right, guards)),
        )
    }
}

/// Whether one node is statically string-typed under the guards in
/// force at this position.
fn is_string(record: &ExpressionRecord, guards: &[Guard], node: &ExprNode) -> bool {
    node_type_guards(node, record, guards) == Ok(ExprType::Scalar(ScalarType::Str))
}

/// Helper shims for string equality through the comparison helper.
impl Code {
    fn compare_for_eq(self) -> Code {
        Code::Equal(false, Box::new(self), Box::new(Code::PlainInt(0)))
    }

    fn compare_for_ne(self) -> Code {
        Code::Equal(true, Box::new(self), Box::new(Code::PlainInt(0)))
    }
}

/// The typed extraction helper of one reference type.
fn extract_helper(ty: &ExprType) -> &'static str {
    match ty {
        ExprType::Scalar(ScalarType::Bool) => "lekGetBool",
        ExprType::Scalar(ScalarType::Int) => "lekGetInt",
        ExprType::Scalar(ScalarType::Str) => "lekGetStr",
        ExprType::Scalar(ScalarType::DateTime) => "lekGetDt",
        ExprType::Scalar(ScalarType::Duration) => "lekGetDur",
        ExprType::Set(ScalarType::Bool) => "lekGetSetBool",
        ExprType::Set(ScalarType::Int) => "lekGetSetInt",
        ExprType::Set(ScalarType::Str) => "lekGetSetStr",
        ExprType::Set(ScalarType::DateTime) => "lekGetSetDt",
        ExprType::Set(ScalarType::Duration) => "lekGetSetDur",
    }
}

/// The membership helper for one set operand.
fn set_helper(record: &ExpressionRecord, guards: &[Guard], set: &ExprNode) -> &'static str {
    match node_type_guards(set, record, guards) {
        Ok(ExprType::Set(ScalarType::Bool)) => "lekHasBool",
        Ok(ExprType::Set(ScalarType::Int)) => "lekHasInt",
        Ok(ExprType::Set(ScalarType::Str)) => "lekHasStr",
        Ok(ExprType::Set(ScalarType::DateTime)) => "lekHasDt",
        Ok(ExprType::Set(ScalarType::Duration)) => "lekHasDur",
        _ => "lekHasInt",
    }
}

/// The compiled set operand: a literal set becomes a typed literal
/// maker call; a set-typed reference becomes its typed extraction.
fn set_operand(record: &ExpressionRecord, guards: &[Guard], set: &ExprNode) -> Code {
    match set {
        ExprNode::Set(items) => {
            Code::Call(set_lit_helper(record, guards, set), encode_set_items(items))
        }
        other => compile_guards(record, other, guards),
    }
}

/// The literal-maker helper of one set literal.
fn set_lit_helper(record: &ExpressionRecord, guards: &[Guard], set: &ExprNode) -> &'static str {
    match node_type_guards(set, record, guards) {
        Ok(ExprType::Set(ScalarType::Bool)) => "lekSetLitBool",
        Ok(ExprType::Set(ScalarType::Int)) => "lekSetLitInt",
        Ok(ExprType::Set(ScalarType::Str)) => "lekSetLitStr",
        Ok(ExprType::Set(ScalarType::DateTime)) => "lekSetLitDt",
        Ok(ExprType::Set(ScalarType::Duration)) => "lekSetLitDur",
        _ => "lekSetLitInt",
    }
}

/// Encode set literal members.
fn encode_set_items(items: &[Scalar]) -> Vec<Code> {
    items.iter().map(scalar_code).collect()
}

/// One scalar literal as code.
fn scalar_code(scalar: &Scalar) -> Code {
    match scalar {
        Scalar::Bool(value) => Code::Bool(*value),
        Scalar::Int(value) => Code::Int(*value),
        Scalar::Str(value) => Code::Str(value.clone()),
        Scalar::DateTime(seconds) => Code::Int(*seconds),
        Scalar::Duration(seconds) => Code::Int(*seconds),
    }
}

/// The checked-add helper for one operand pair.
fn add_name(
    record: &ExpressionRecord,
    guards: &[Guard],
    left: &ExprNode,
    right: &ExprNode,
) -> &'static str {
    match (
        node_type_guards(left, record, guards),
        node_type_guards(right, record, guards),
    ) {
        (Ok(ExprType::Scalar(ScalarType::Int)), Ok(ExprType::Scalar(ScalarType::Int))) => "lekAdd",
        (
            Ok(ExprType::Scalar(ScalarType::Duration)),
            Ok(ExprType::Scalar(ScalarType::Duration)),
        ) => "lekDurAdd",
        _ => "lekDtAdd",
    }
}

/// The checked-subtract helper for one operand pair.
fn sub_name(
    record: &ExpressionRecord,
    guards: &[Guard],
    left: &ExprNode,
    right: &ExprNode,
) -> &'static str {
    match (
        node_type_guards(left, record, guards),
        node_type_guards(right, record, guards),
    ) {
        (Ok(ExprType::Scalar(ScalarType::Int)), Ok(ExprType::Scalar(ScalarType::Int))) => "lekSub",
        (
            Ok(ExprType::Scalar(ScalarType::Duration)),
            Ok(ExprType::Scalar(ScalarType::Duration)),
        ) => "lekDurSub",
        (_, Ok(ExprType::Scalar(ScalarType::Duration))) => "lekDtSub",
        _ => "lekDtDiff",
    }
}

/// The checked-multiply helper for one operand pair.
fn mul_name(
    record: &ExpressionRecord,
    guards: &[Guard],
    left: &ExprNode,
    right: &ExprNode,
) -> &'static str {
    match (
        node_type_guards(left, record, guards),
        node_type_guards(right, record, guards),
    ) {
        (Ok(ExprType::Scalar(ScalarType::Int)), Ok(ExprType::Scalar(ScalarType::Int))) => "lekMul",
        _ => "lekDurMul",
    }
}

/// The checked-divide helper for one operand pair.
fn div_name(record: &ExpressionRecord, guards: &[Guard], left: &ExprNode) -> &'static str {
    match node_type_guards(left, record, guards) {
        Ok(ExprType::Scalar(ScalarType::Duration)) => "lekDurDiv",
        _ => "lekDiv",
    }
}

/// The prelude name of one built-in.
fn builtin_name(name: &str) -> &'static str {
    match name {
        "string-length" => "lekLen",
        "string-concat" => "lekConcat",
        "string-lower" => "lekLower",
        "string-upper" => "lekUpper",
        "string-contains" => "lekContains",
        "string-starts-with" => "lekStartsWith",
        "string-ends-with" => "lekEndsWith",
        "int-abs" => "lekAbs",
        "datetime-year" => "lekYear",
        "datetime-month" => "lekMonth",
        "datetime-day" => "lekDay",
        "datetime-weekday" => "lekWeekday",
        "duration-seconds" => "lekDurSeconds",
        "cast-int-to-string" => "lekIntToStr",
        "cast-string-to-int" => "lekStrToInt",
        _ => "lekFailInvalid",
    }
}
