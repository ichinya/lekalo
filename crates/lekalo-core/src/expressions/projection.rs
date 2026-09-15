//! The target-neutral projection renderer (issue #66).
//!
//! One validated attachment compiles to one complete, self-contained
//! program per target — Node (ECMAScript), PHP, and Go — that reads
//! the shared evaluation-vector document on stdin and writes the
//! computed results to stdout. The three programs are generated from
//! one typed AST by one compiler, so their semantics are equivalent
//! by construction; the shared fixtures prove it by execution.
//!
//! Equivalence guarantees are structural, not aspirational:
//! integers stay within ±2^53−1 (exact in every target and in the
//! JSON double a reader may interpose), all arithmetic goes through
//! identical checked prelude functions, datetimes are integer
//! seconds under one civil-calendar algorithm (the epoch-seconds →
//! civil-day step floors toward −∞ identically everywhere, matching
//! the reference `div_euclid`, and every calendar field is
//! range-checked before use), durations are integer
//! seconds, strings are BMP-only UTF-8 so code-point order is byte
//! order everywhere, and casing is ASCII-only by contract. Arithmetic
//! division and remainder truncate toward zero identically in Rust,
//! Node, PHP, and Go; set bindings normalize to the same sorted
//! duplicate-free order and reject duplicates with the same closed
//! token in every target. No generated program reads anything but
//! stdin, writes anything but stdout, or calls anything but its own
//! prelude.

use super::ast::ExprNode;
use super::types::{ExprType, Scalar, ScalarType};
use super::typing::{collect_guards, conditional_then_type, node_type_guards, Guard};
use super::version::{BUILTIN_SEMANTICS_VERSION, FAMILY, VERSION};
use super::ExpressionRecord;
use super::ExpressionsAttachment;

/// The closed target vocabulary of the renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// Node (ECMAScript; integers are `BigInt`).
    Node,
    /// PHP (64-bit integers).
    Php,
    /// Go (`int64`).
    Go,
}

impl Target {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Php => "php",
            Self::Go => "go",
        }
    }

    /// Parse one wire key.
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "node" => Some(Self::Node),
            "php" => Some(Self::Php),
            "go" => Some(Self::Go),
            _ => None,
        }
    }
}

/// The projection identity header every generated program carries.
pub fn projection_identity() -> String {
    format!(
        "{}@{}:builtin-semantics:{}",
        FAMILY, VERSION, BUILTIN_SEMANTICS_VERSION
    )
}

/// Render one complete target program for the attachment.
pub fn render_program(attachment: &ExpressionsAttachment, target: Target) -> String {
    match target {
        Target::Node => render_node(attachment),
        Target::Php => render_php(attachment),
        Target::Go => render_go(attachment),
    }
}

/// The typed intermediate the per-target printers consume.
#[derive(Clone, Debug)]
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
#[derive(Clone, Copy, Debug)]
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

/// Render one Node program.
fn render_node(attachment: &ExpressionsAttachment) -> String {
    let mut out = String::new();
    out.push_str("// lekalo expressions projection: ");
    out.push_str(&projection_identity());
    out.push_str(
        "\n// Generated by lekalo; do not edit. Reads the shared\n// evaluation vectors on stdin, writes computed results to stdout.\n\"use strict\";\n",
    );
    out.push_str(NODE_PRELUDE);
    for record in attachment.expressions() {
        let compiled = compile(record, record.body());
        out.push_str(&format!(
            "LEK[{}] = (env) => ({});\n",
            node_string(record.id()),
            emit_node(&compiled)
        ));
        out.push_str(&format!(
            "LEK_TYPES[{}] = {};\n",
            node_string(record.id()),
            node_string(&record.result().key())
        ));
        out.push_str(&format!(
            "LEK_PARAMS[{}] = [{}];\n",
            node_string(record.id()),
            node_params(record)
        ));
    }
    out.push_str(NODE_RUNNER);
    out
}

/// Render one PHP program.
fn render_php(attachment: &ExpressionsAttachment) -> String {
    let mut out = String::new();
    out.push_str("<?php\n// lekalo expressions projection: ");
    out.push_str(&projection_identity());
    out.push_str(
        "\n// Generated by lekalo; do not edit. Reads the shared\n// evaluation vectors on stdin, writes computed results to stdout.\n",
    );
    out.push_str(PHP_PRELUDE);
    for record in attachment.expressions() {
        let compiled = compile(record, record.body());
        out.push_str(&format!(
            "$LEK[{}] = function ($env) {{ return ({}); }};\n",
            php_string(record.id()),
            emit_php(&compiled)
        ));
        out.push_str(&format!(
            "$LEK_TYPES[{}] = {};\n",
            php_string(record.id()),
            php_string(&record.result().key())
        ));
        out.push_str(&format!(
            "$LEK_PARAMS[{}] = array({});\n",
            php_string(record.id()),
            php_params(record)
        ));
    }
    out.push_str(PHP_RUNNER);
    out
}

/// Render one Go program.
fn render_go(attachment: &ExpressionsAttachment) -> String {
    let mut out = String::new();
    out.push_str("// lekalo expressions projection: ");
    out.push_str(&projection_identity());
    out.push_str(
        "\n// Generated by lekalo; do not edit. Reads the shared\n// evaluation vectors on stdin, writes computed results to stdout.\npackage main\n\nimport (\n\t\"encoding/json\"\n\t\"fmt\"\n\t\"io\"\n\t\"math\"\n\t\"os\"\n\t\"sort\"\n\t\"strconv\"\n\t\"strings\"\n\t\"unicode/utf8\"\n)\n\n",
    );
    out.push_str(GO_PRELUDE);
    out.push_str("\nvar lekParams = map[string][]lekParam{\n");
    for record in attachment.expressions() {
        out.push_str(&go_params_lines(record));
    }
    out.push_str("}\n");
    out.push_str(GO_RUNNER_HEAD);
    for record in attachment.expressions() {
        let compiled = compile(record, record.body());
        let encoded = emit_go(&compiled);
        let encode_call = go_encode_call(record.result());
        out.push_str(&format!(
            "\tcase {}:\n\t\trow[\"value\"] = {}({})\n",
            go_string(record.id()),
            encode_call,
            encoded
        ));
    }
    out.push_str(GO_RUNNER_TAIL);
    out
}

/// The declared reference table of one record, rendered as Node
/// object literals for the eager binding validation.
fn node_params(record: &ExpressionRecord) -> String {
    record
        .params
        .iter()
        .map(|param| {
            format!(
                "{{\"s\":{},\"f\":{},\"t\":{},\"n\":{}}}",
                node_string(param.scope.key()),
                node_string(&param.field),
                node_string(&param.ty.key()),
                param.nullable
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The declared reference table of one record, rendered as PHP array
/// literals for the eager binding validation.
fn php_params(record: &ExpressionRecord) -> String {
    record
        .params
        .iter()
        .map(|param| {
            format!(
                "array('s'=>{},'f'=>{},'t'=>{},'n'=>{})",
                php_string(param.scope.key()),
                php_string(&param.field),
                php_string(&param.ty.key()),
                if param.nullable { "true" } else { "false" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The declared reference table of one record, rendered as Go struct
/// literals for the eager binding validation.
fn go_params_lines(record: &ExpressionRecord) -> String {
    let mut out = String::new();
    out.push_str(&format!("\t{}: {{\n", go_string(record.id())));
    for param in &record.params {
        out.push_str(&format!(
            "\t\t{{{}, {}, {}, {}}},\n",
            go_string(param.scope.key()),
            go_string(&param.field),
            go_string(&param.ty.key()),
            param.nullable
        ));
    }
    out.push_str("\t},\n");
    out
}

/// The result encoder call for one declared result type.
fn go_encode_call(ty: &ExprType) -> &'static str {
    match ty {
        ExprType::Scalar(ScalarType::Bool) => "lekEncBool",
        ExprType::Scalar(ScalarType::Int) => "lekEncInt",
        ExprType::Scalar(ScalarType::Str) => "lekEncStr",
        ExprType::Scalar(ScalarType::DateTime) => "lekEncDt",
        ExprType::Scalar(ScalarType::Duration) => "lekEncDur",
        ExprType::Set(ScalarType::Bool) => "lekEncSetBool",
        ExprType::Set(ScalarType::Int) => "lekEncSetInt",
        ExprType::Set(ScalarType::Str) => "lekEncSetStr",
        ExprType::Set(ScalarType::DateTime) => "lekEncSetDt",
        ExprType::Set(ScalarType::Duration) => "lekEncSetDur",
    }
}

/// Whether one helper receives the environment as its first
/// argument in the Go rendering.
fn go_takes_env(name: &str) -> bool {
    name.starts_with("lekGet") || name == "lekIsNull" || name == "lekNotNull" || name == "lekClock"
}

/// Emit one intermediate expression in Node syntax.
fn emit_node(code: &Code) -> String {
    match code {
        Code::PlainInt(value) => value.to_string(),
        Code::Int(value) => format!("{}n", value),
        Code::Str(value) => node_string(value),
        Code::Bool(value) => value.to_string(),
        Code::Call(name, args) => format!(
            "{}({})",
            name,
            args.iter().map(emit_node).collect::<Vec<_>>().join(", ")
        ),
        Code::Compare(op, left, right) => {
            format!("({} {} {})", emit_node(left), op, emit_node(right))
        }
        Code::Equal(negated, left, right) => {
            // Each operand is parenthesized individually, so a boolean
            // combinator used as an equality operand keeps its AST
            // grouping instead of being reassociated by target
            // operator precedence.
            let op = if *negated { "!==" } else { "===" };
            format!("(({}) {} ({}))", emit_node(left), op, emit_node(right))
        }
        // Every operand is parenthesized, exactly like the PHP and
        // Go emit: JavaScript `&&` binds tighter than `||`, so a
        // bare `or` nested inside an `and` would silently
        // re-associate.
        Code::And(operands) => operands
            .iter()
            .map(|operand| format!("({})", emit_node(operand)))
            .collect::<Vec<_>>()
            .join(" && "),
        Code::Or(operands) => operands
            .iter()
            .map(|operand| format!("({})", emit_node(operand)))
            .collect::<Vec<_>>()
            .join(" || "),
        Code::Not(operand) => format!("(!({}))", emit_node(operand)),
        Code::If(condition, then, otherwise, _) => format!(
            "(({}) ? ({}) : ({}))",
            emit_node(condition),
            emit_node(then),
            emit_node(otherwise)
        ),
    }
}

/// Emit one intermediate expression in PHP syntax.
fn emit_php(code: &Code) -> String {
    match code {
        Code::PlainInt(value) => value.to_string(),
        Code::Int(value) => value.to_string(),
        Code::Str(value) => php_string(value),
        Code::Bool(value) => {
            if *value {
                "true".to_owned()
            } else {
                "false".to_owned()
            }
        }
        Code::Call(name, args) => format!(
            "{}({})",
            name,
            args.iter().map(emit_php).collect::<Vec<_>>().join(", ")
        ),
        Code::Compare(op, left, right) => {
            format!("({} {} {})", emit_php(left), op, emit_php(right))
        }
        Code::Equal(negated, left, right) => {
            // Each operand is parenthesized individually for the same
            // grouping guarantee as the Node emit.
            let op = if *negated { "!==" } else { "===" };
            format!("(({}) {} ({}))", emit_php(left), op, emit_php(right))
        }
        Code::And(operands) => operands
            .iter()
            .map(|operand| format!("({})", emit_php(operand)))
            .collect::<Vec<_>>()
            .join(" && "),
        Code::Or(operands) => operands
            .iter()
            .map(|operand| format!("({})", emit_php(operand)))
            .collect::<Vec<_>>()
            .join(" || "),
        Code::Not(operand) => format!("(!({}))", emit_php(operand)),
        Code::If(condition, then, otherwise, _) => format!(
            "(({}) ? ({}) : ({}))",
            emit_php(condition),
            emit_php(then),
            emit_php(otherwise)
        ),
    }
}

/// Emit one intermediate expression in Go syntax.
fn emit_go(code: &Code) -> String {
    match code {
        Code::PlainInt(value) => value.to_string(),
        Code::Int(value) => value.to_string(),
        Code::Str(value) => go_string(value),
        Code::Bool(value) => value.to_string(),
        Code::Call(name, args) => {
            let mut all = String::new();
            if go_takes_env(name) {
                all.push_str("env");
                if !args.is_empty() {
                    all.push_str(", ");
                }
            }
            format!(
                "{}({}{})",
                name,
                all,
                args.iter().map(emit_go).collect::<Vec<_>>().join(", ")
            )
        }
        Code::Compare(op, left, right) => {
            format!("({} {} {})", emit_go(left), op, emit_go(right))
        }
        Code::Equal(negated, left, right) => {
            // Each operand is parenthesized individually for the same
            // grouping guarantee as the Node emit.
            let op = if *negated { "!=" } else { "==" };
            format!("(({}) {} ({}))", emit_go(left), op, emit_go(right))
        }
        Code::And(operands) => operands
            .iter()
            .map(|operand| format!("({})", emit_go(operand)))
            .collect::<Vec<_>>()
            .join(" && "),
        Code::Or(operands) => operands
            .iter()
            .map(|operand| format!("({})", emit_go(operand)))
            .collect::<Vec<_>>()
            .join(" || "),
        Code::Not(operand) => format!("(!({}))", emit_go(operand)),
        Code::If(condition, then, otherwise, branch) => {
            // Go has no ternary: an immediately-invoked closure keeps
            // the branch semantics lazy, exactly like the ternary in
            // the Node and PHP emit and the reference evaluator's
            // if: the untaken branch never computes.
            let ty = match branch {
                BranchType::Int => "int64",
                BranchType::Str => "string",
                BranchType::Bool => "bool",
                BranchType::Set(ScalarType::Bool) => "[]bool",
                BranchType::Set(ScalarType::Str) => "[]string",
                BranchType::Set(_) => "[]int64",
            };
            format!(
                "(func() {} {{ if {} {{ return {} }}; return {} }})()",
                ty,
                emit_go(condition),
                emit_go(then),
                emit_go(otherwise)
            )
        }
    }
}

/// One ECMAScript string literal.
fn node_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

/// One PHP string literal (single quoted).
fn php_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// One Go string literal (JSON escaping is valid Go).
fn go_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

const NODE_PRELUDE: &str = r#"
// Prototype-free expression tables: `Object.create(null)` keeps the
// declared records the only members, so an undeclared selector —
// including a name matching an inherited Object.prototype member
// (`constructor`, `toString`, `__proto__`, `hasOwnProperty`) — can
// never pass the declaration check through an inherited property.
const LEK = Object.create(null);
const LEK_TYPES = Object.create(null);
const LEK_PARAMS = Object.create(null);
const LEK_MAX_INT = 9007199254740991n;
const LEK_MAX_DUR = 31536000000n;
let lekEnv = { clockText: null, bindings: {} };
function lekFail(token) { const error = new Error(token); error.lekToken = token; throw error; }
function lekIntBound(v) { if (v > LEK_MAX_INT || v < -LEK_MAX_INT) { lekFail("int-overflow"); } return v; }
function lekAdd(a, b) { return lekIntBound(a + b); }
function lekSub(a, b) { return lekIntBound(a - b); }
function lekMul(a, b) { return lekIntBound(a * b); }
function lekDiv(a, b) { if (b === 0n) { lekFail("divide-by-zero"); } return lekIntBound(a / b); }
function lekMod(a, b) { if (b === 0n) { lekFail("modulo-by-zero"); } return lekIntBound(a % b); }
function lekAbs(a) { return lekIntBound(a < 0n ? -a : a); }
function lekDurBound(v) { if (v > LEK_MAX_DUR || v < -LEK_MAX_DUR) { lekFail("duration-overflow"); } return v; }
function lekDurAdd(a, b) { return lekDurBound(a + b); }
function lekDurSub(a, b) { return lekDurBound(a - b); }
function lekDurMul(a, b) { return lekDurBound(a * b); }
function lekDurDiv(a, b) { if (b === 0n) { lekFail("divide-by-zero"); } return lekDurBound(a / b); }
function lekDurSeconds(d) { return d; }
function lekCivil(z0) {
  const z = z0 + 719468n;
  const era = (z >= 0n ? z : z - 146096n) / 146097n;
  const doe = z - era * 146097n;
  const yoe = (doe - doe / 1460n + doe / 36524n - doe / 146096n) / 365n;
  const y = yoe + era * 400n;
  const doy = doe - (365n * yoe + yoe / 4n - yoe / 100n);
  const mp = (5n * doy + 2n) / 153n;
  const d = doy - (153n * mp + 2n) / 5n + 1n;
  const m = mp < 10n ? mp + 3n : mp - 9n;
  return [m <= 2n ? y + 1n : y, m, d];
}
function lekFromCivil(y0, m, d) {
  const y = m <= 2n ? y0 - 1n : y0;
  const era = (y >= 0n ? y : y - 399n) / 400n;
  const yoe = y - era * 400n;
  const mp = m > 2n ? m - 3n : m + 9n;
  const doy = (153n * mp + 2n) / 5n + d - 1n;
  const doe = yoe * 365n + yoe / 4n - yoe / 100n + doy;
  return era * 146097n + doe - 719468n;
}
function lekFloorDiv(a, b) {
  const q = a / b;
  const r = a % b;
  return r !== 0n && ((r < 0n) !== (b < 0n)) ? q - 1n : q;
}
function lekDaysInMonth(y, m) {
  if (m === 2n) { const leap = (y % 4n === 0n && y % 100n !== 0n) || y % 400n === 0n; return leap ? 29n : 28n; }
  return (m === 4n || m === 6n || m === 9n || m === 11n) ? 30n : 31n;
}
function lekValidFields(y, mo, d, h, mi, s) {
  if (y < 1n || y > 9999n || mo < 1n || mo > 12n) { return false; }
  if (d < 1n || d > lekDaysInMonth(y, mo)) { return false; }
  return h <= 23n && mi <= 59n && s <= 59n;
}
function lekDtRange(d) {
  const y = lekCivil(lekFloorDiv(d, 86400n))[0];
  return y >= 1n && y <= 9999n;
}
function lekDtAdd(d, s) { const r = d + s; if (!lekDtRange(r)) { lekFail("datetime-overflow"); } return r; }
function lekDtSub(d, s) { const r = d - s; if (!lekDtRange(r)) { lekFail("datetime-overflow"); } return r; }
function lekDtDiff(a, b) { return lekDurBound(a - b); }
function lekYear(d) { return lekCivil(lekFloorDiv(d, 86400n))[0]; }
function lekMonth(d) { return lekCivil(lekFloorDiv(d, 86400n))[1]; }
function lekDay(d) { return lekCivil(lekFloorDiv(d, 86400n))[2]; }
function lekWeekday(d) { const days = lekFloorDiv(d, 86400n); return ((days + 3n) % 7n + 7n) % 7n + 1n; }
function lekDtRender(d) {
  const days = lekFloorDiv(d, 86400n);
  const time = ((d % 86400n) + 86400n) % 86400n;
  const parts = lekCivil(days);
  const pad = (v, n) => v.toString().padStart(n, "0");
  return pad(parts[0], 4) + "-" + pad(parts[1], 2) + "-" + pad(parts[2], 2) + "T" + pad(time / 3600n, 2) + ":" + pad((time % 3600n) / 60n, 2) + ":" + pad(time % 60n, 2) + "Z";
}
function lekClock() {
  // Only an omitted clock reads the shared epoch default, exactly
  // like the reference decode; an explicit null or any present
  // noncanonical clock refuses with the closed clock token (the
  // runner validates a supplied clock eagerly, before evaluation).
  if (!lekEnv.clockPresent) { return 0n; }
  const f = lekCheckClock(lekEnv.clockText);
  return lekFromCivil(f[0], f[1], f[2]) * 86400n + f[3] * 3600n + f[4] * 60n + f[5];
}
function lekCheckClock(text) {
  if (typeof text !== "string") { lekFail("clock-invalid"); }
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z$/.exec(text);
  if (!m) { lekFail("clock-invalid"); }
  const f = [1, 2, 3, 4, 5, 6].map((i) => BigInt(m[i]));
  if (!lekValidFields(f[0], f[1], f[2], f[3], f[4], f[5])) { lekFail("clock-invalid"); }
  return f;
}
function lekGet(scope, field, convert) {
  // Own-property membership only: an inherited Object.prototype
  // member (a declared field may be named `constructor`) is never a
  // binding.
  const scopeMap = lekEnv.bindings[scope];
  if (!scopeMap || typeof scopeMap !== "object" || !Object.prototype.hasOwnProperty.call(scopeMap, field)) { lekFail("binding-missing"); }
  const v = scopeMap[field];
  if (v === null || v === undefined) { lekFail("binding-null"); }
  return convert(v);
}
function toBig(v) {
  // The exact JSON scalar types: only a JSON integer literal is an
  // int binding (numeric strings, fractional spellings, and the
  // negative-zero spelling the raw scan misses are refused), and the
  // family integer bound applies (an out-of-range double still
  // compares above the bound after the rounded parse).
  if (typeof v !== "number" || !Number.isInteger(v) || Object.is(v, -0)) { lekFail("binding-value"); }
  if (typeof v !== "number" || !Number.isInteger(v)) { lekFail("binding-value"); }
  const big = BigInt(v);
  if (big > LEK_MAX_INT || big < -LEK_MAX_INT) { lekFail("binding-value"); }
  return big;
}
function toStr(v) {
  if (typeof v !== "string" || !lekStrOk(v) || Buffer.byteLength(v) > 256) { lekFail("binding-value"); }
  return v;
}
function toBool(v) { if (typeof v !== "boolean") { lekFail("binding-value"); } return v; }
function lekDurValue(v) { if (v > LEK_MAX_DUR || v < -LEK_MAX_DUR) { lekFail("binding-value"); } return v; }
function lekSetCmp(a, b) {
  if (typeof a === "boolean") { return a === b ? 0 : (a ? 1 : -1); }
  return a < b ? -1 : (a > b ? 1 : 0);
}
function toSet(v, convert) {
  if (!Array.isArray(v)) { lekFail("binding-value"); }
  if (v.length > 64) { lekFail("binding-value"); }
  const items = v.map((item) => convert(item));
  items.sort((a, b) => lekSetCmp(a, b));
  for (let i = 1; i < items.length; i++) { if (lekSetCmp(items[i - 1], items[i]) === 0) { lekFail("binding-value"); } }
  return items;
}
function lekGetBool(scope, field) { return lekGet(scope, field, toBool); }
function lekGetInt(scope, field) { return lekGet(scope, field, toBig); }
function lekGetStr(scope, field) { return lekGet(scope, field, toStr); }
function lekGetDt(scope, field) { return lekParseClockText(lekGet(scope, field, toStr)); }
function lekGetDur(scope, field) { return lekGet(scope, field, (v) => lekDurValue(toBig(v))); }
function lekGetSetBool(scope, field) { return lekGet(scope, field, (v) => toSet(v, toBool)); }
function lekGetSetInt(scope, field) { return lekGet(scope, field, (v) => toSet(v, toBig)); }
function lekGetSetStr(scope, field) { return lekGet(scope, field, (v) => toSet(v, toStr)); }
function lekGetSetDt(scope, field) { return lekGet(scope, field, (v) => toSet(v, (s) => lekParseClockText(toStr(s)))); }
function lekGetSetDur(scope, field) { return lekGet(scope, field, (v) => toSet(v, (s) => lekDurValue(toBig(s)))); }
const LEK_PARAM_GETTERS = {
  "bool": lekGetBool,
  "int": lekGetInt,
  "string": lekGetStr,
  "datetime": lekGetDt,
  "duration": lekGetDur,
  "set:bool": lekGetSetBool,
  "set:int": lekGetSetInt,
  "set:string": lekGetSetStr,
  "set:datetime": lekGetSetDt,
  "set:duration": lekGetSetDur
};
function lekValidateBindings(params, bindings) {
  // The eager per-record binding validation the reference performs
  // before evaluation, in exactly the reference pass order: first
  // every scope name and scope shape, then every field name across
  // all scopes, then every declared parameter value — presence
  // (even for nullable references), nullability, and every typed
  // value, including values on branches the body never takes.
  if (typeof bindings !== "object" || bindings === null || Array.isArray(bindings)) { lekFail("bindings-shape"); }
  for (const scope of Object.keys(bindings).sort()) {
    if (scope !== "input" && scope !== "actor" && scope !== "entity" && scope !== "result") { lekFail("binding-unknown"); }
    const scopeMap = bindings[scope];
    if (typeof scopeMap !== "object" || scopeMap === null || Array.isArray(scopeMap)) { lekFail("bindings-shape"); }
  }
  for (const scope of Object.keys(bindings).sort()) {
    const scopeMap = bindings[scope];
    for (const field of Object.keys(scopeMap).sort()) {
      let known = false;
      for (const p of params) { if (p.s === scope && p.f === field) { known = true; break; } }
      if (!known) { lekFail("binding-unknown"); }
    }
  }
  for (const p of params) {
    const scopeMap = bindings[p.s];
    if (!scopeMap || typeof scopeMap !== "object" || !Object.prototype.hasOwnProperty.call(scopeMap, p.f)) { lekFail("binding-missing"); }
    if (scopeMap[p.f] === null) { if (!p.n) { lekFail("binding-null"); } continue; }
    const getter = LEK_PARAM_GETTERS[p.t];
    if (!getter) { lekFail("binding-value"); }
    getter(p.s, p.f);
  }
}
function lekParseClockText(text) {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z$/.exec(text);
  if (!m) { lekFail("binding-value"); }
  const f = [1, 2, 3, 4, 5, 6].map((i) => BigInt(m[i]));
  if (!lekValidFields(f[0], f[1], f[2], f[3], f[4], f[5])) { lekFail("binding-value"); }
  return lekFromCivil(f[0], f[1], f[2]) * 86400n + f[3] * 3600n + f[4] * 60n + f[5];
}
function lekIsNull(scope, field) { const scopeMap = lekEnv.bindings[scope]; return !scopeMap || typeof scopeMap !== "object" || !Object.prototype.hasOwnProperty.call(scopeMap, field) || scopeMap[field] === null || scopeMap[field] === undefined; }
function lekNotNull(scope, field) { return !lekIsNull(scope, field); }
function lekLen(s) { return BigInt(s.length); }
function lekConcat(a, b) {
  const s = a + b;
  let bytes = 0;
  for (const ch of s) {
    const c = ch.codePointAt(0);
    bytes += c <= 0x7F ? 1 : (c <= 0x7FF ? 2 : (c <= 0xFFFF ? 3 : 4));
  }
  if (bytes > 256 || !lekStrOk(s)) { lekFail("concat-overflow"); }
  return s;
}
function lekStrOk(s) {
  for (const ch of s) {
    const c = ch.codePointAt(0);
    if (c > 0xFFFF || c < 0x20 || (c >= 0xD800 && c <= 0xDFFF)) { return false; }
  }
  return true;
}
function lekLower(s) {
  let out = "";
  for (const ch of s) {
    const c = ch.codePointAt(0);
    out += c >= 65 && c <= 90 ? String.fromCharCode(c + 32) : ch;
  }
  return out;
}
function lekUpper(s) {
  let out = "";
  for (const ch of s) {
    const c = ch.codePointAt(0);
    out += c >= 97 && c <= 122 ? String.fromCharCode(c - 32) : ch;
  }
  return out;
}
function lekCmpStr(a, b) { return a < b ? -1 : (a > b ? 1 : 0); }
function lekContains(a, b) { return a.includes(b); }
function lekStartsWith(a, b) { return a.startsWith(b); }
function lekEndsWith(a, b) { return a.endsWith(b); }
function lekIntToStr(n) { return n.toString(); }
function lekStrToInt(s) {
  if (!/^-?(0|[1-9][0-9]{0,15})$/.test(s)) { lekFail("cast-invalid"); }
  const v = BigInt(s);
  if (v > LEK_MAX_INT || v < -LEK_MAX_INT) { lekFail("cast-invalid"); }
  return v;
}
function lekHasBool(v, items) { return items.includes(v); }
function lekHasInt(v, items) { return items.includes(v); }
function lekHasStr(v, items) { return items.includes(v); }
function lekHasDt(v, items) { return items.includes(v); }
function lekHasDur(v, items) { return items.includes(v); }
function lekSetLitBool(...items) { return items; }
function lekSetLitInt(...items) { return items; }
function lekSetLitStr(...items) { return items; }
function lekSetLitDt(...items) { return items; }
function lekSetLitDur(...items) { return items; }
function lekSetLitAny(...items) { return items; }
function lekFailInvalid() { lekFail("cast-invalid"); }
"#;

const NODE_RUNNER: &str = r#"
function lekEncodeTyped(value, ty) {
  if (ty === "datetime") { return lekDtRender(value); }
  if (ty && ty.startsWith("set:")) {
    const inner = ty.slice(4);
    return value.map((item) => lekEncodeTyped(item, inner));
  }
  if (typeof value === "bigint") { return Number(value); }
  return value;
}
function lekCanonicalDocumentText(text) {
  // The reference JSON decoder refuses non-integer spellings of
  // int/duration bindings (42.0, 1e2), the negative-zero integer
  // spelling (-0, which the reference parse preserves as a float
  // and refuses), and lone surrogate escapes: scan the raw document
  // text and reject any of those shapes before any row is
  // evaluated.
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === "\"") {
      i += 1;
      while (i < text.length) {
        if (text[i] === "\\") {
          if (text[i + 1] === "u") {
            const cp = parseInt(text.slice(i + 2, i + 6), 16);
            if (Number.isNaN(cp)) { return false; }
            if (cp >= 0xD800 && cp <= 0xDBFF) {
              // A high surrogate must be followed by its low pair.
              if (text.slice(i + 6, i + 8) !== "\\u") { return false; }
              const lo = parseInt(text.slice(i + 8, i + 12), 16);
              if (Number.isNaN(lo) || lo < 0xDC00 || lo > 0xDFFF) { return false; }
              i += 12;
              continue;
            }
            if (cp >= 0xDC00 && cp <= 0xDFFF) { return false; }
            i += 6;
            continue;
          }
          i += 2;
          continue;
        }
        if (text[i] === "\"") { i += 1; break; }
        i += 1;
      }
      continue;
    }
    if (c === "-" || (c >= "0" && c <= "9")) {
      let j = i + 1;
      while (j < text.length && "0123456789.eE+-".includes(text[j])) { j += 1; }
      const token = text.slice(i, j);
      if (!/^-?(0|[1-9][0-9]*)$/.test(token) || token === "-0") { return false; }
      i = j;
      continue;
    }
    i += 1;
  }
  return true;
}
function lekInputRefuse() {
  process.stderr.write("lekalo vector input error\n");
  process.exit(1);
}
function lekMain() {
  const bytes = require("fs").readFileSync(0);
  let raw = null;
  try {
    // Strict UTF-8: the original stdin bytes must decode without
    // replacement, so malformed sequences refuse the document at the
    // runner boundary instead of manufacturing replacement-character
    // binding data. A legal U+FFFD string stays accepted, and a byte
    // order mark is kept so the JSON parse refuses it exactly like
    // the reference.
    raw = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch (error) {
    lekInputRefuse();
  }
  if (!lekCanonicalDocumentText(raw)) {
    lekInputRefuse();
  }
  let doc;
  try {
    doc = JSON.parse(raw);
  } catch (error) {
    lekInputRefuse();
  }
  // The evaluation vectors are a JSON array of non-null objects: an
  // iterable string, a scalar, or a non-object element refuses the
  // document at the boundary instead of iterating into fabricated
  // result rows.
  if (typeof doc !== "object" || doc === null || !Array.isArray(doc.vectors)) {
    lekInputRefuse();
  }
  const results = [];
  for (const vector of doc.vectors) {
    if (typeof vector !== "object" || vector === null || Array.isArray(vector)) {
      lekInputRefuse();
    }
    // The consumed id is echoed into the result row, so only a
    // primitive string id can reach the envelope: a null, number,
    // boolean, object, array, or absent id refuses the document
    // instead of emitting a malformed or id-less value row.
    if (typeof vector.id !== "string") {
      lekInputRefuse();
    }
    // Presence is distinct from value: only an omitted clock reads
    // the epoch default, and an explicit null or malformed supplied
    // clock refuses even when the body never reads it.
    lekEnv = {
      clockText: vector.clock,
      clockPresent: Object.prototype.hasOwnProperty.call(vector, "clock"),
      bindings: vector.bindings
    };
    const row = { id: vector.id };
    try {
      if (lekEnv.clockPresent) { lekCheckClock(lekEnv.clockText); }
      // Only a primitive string selects an expression record: the
      // selector is validated before any table lookup, so an array,
      // object, or scalar can never be coerced into an expression
      // name and computed. The supplied-clock check above keeps its
      // first place on a double-fault vector.
      if (typeof vector.expression !== "string") { lekFail("expression-unknown"); }
      const params = LEK_PARAMS[vector.expression];
      if (!params) { lekFail("expression-unknown"); }
      lekValidateBindings(params, vector.bindings);
      const fn = LEK[vector.expression];
      row.value = lekEncodeTyped(fn(lekEnv), LEK_TYPES[vector.expression]);
    } catch (error) {
      row.error = error.lekToken || "runtime-error";
      delete row.value;
    }
    results.push(row);
  }
  process.stdout.write(JSON.stringify({ results }) + "\n");
}
try {
  lekMain();
} catch (error) {
  // Runner boundary: any document-level failure — read, decode,
  // parse, or envelope shape — emits only the fixed bounded refusal.
  // The caught exception text, a stack trace, raw binding data, and
  // any host path stay undisclosed.
  lekInputRefuse();
}
"#;

const PHP_PRELUDE: &str = r#"
$LEK = array();
$LEK_TYPES = array();
$LEK_PARAMS = array();
$LEK_ENV = null;
function lekFail($token) { throw new Exception($token); }
function lekIntBound($v) { if ($v > 9007199254740991 || $v < -9007199254740991) { lekFail('int-overflow'); } return $v; }
function lekAdd($a, $b) { $r = $a + $b; if (is_float($r)) { lekFail('int-overflow'); } return lekIntBound($r); }
function lekSub($a, $b) { $r = $a - $b; if (is_float($r)) { lekFail('int-overflow'); } return lekIntBound($r); }
function lekMul($a, $b) { $r = $a * $b; if (is_float($r)) { lekFail('int-overflow'); } return lekIntBound($r); }
function lekDiv($a, $b) { if ($b === 0) { lekFail('divide-by-zero'); } $r = intdiv($a, $b); return lekIntBound($r); }
function lekMod($a, $b) { if ($b === 0) { lekFail('modulo-by-zero'); } $r = $a % $b; if (is_float($r)) { lekFail('int-overflow'); } return lekIntBound($r); }
function lekAbs($a) { return lekIntBound(abs($a)); }
function lekDurBound($v) { if ($v > 31536000000 || $v < -31536000000) { lekFail('duration-overflow'); } return $v; }
function lekDurAdd($a, $b) { $r = $a + $b; if (is_float($r)) { lekFail('duration-overflow'); } return lekDurBound($r); }
function lekDurSub($a, $b) { $r = $a - $b; if (is_float($r)) { lekFail('duration-overflow'); } return lekDurBound($r); }
function lekDurMul($a, $b) { $r = $a * $b; if (is_float($r)) { lekFail('duration-overflow'); } return lekDurBound($r); }
function lekDurDiv($a, $b) { if ($b === 0) { lekFail('divide-by-zero'); } return lekDurBound(intdiv($a, $b)); }
function lekDurSeconds($d) { return $d; }
function lekCivil($z0) {
  $z = $z0 + 719468;
  $era = ($z >= 0 ? $z : $z - 146096) / 146097;
  $era = $era >= 0 ? (int) floor($era) : (int) ceil($era);
  $doe = $z - $era * 146097;
  $yoe = ($doe - intdiv($doe, 1460) + intdiv($doe, 36524) - intdiv($doe, 146096)) / 365;
  $yoe = $yoe >= 0 ? (int) floor($yoe) : (int) ceil($yoe);
  $y = $yoe + $era * 400;
  $doy = $doe - (365 * $yoe + intdiv($yoe, 4) - intdiv($yoe, 100));
  $mp = intdiv(5 * $doy + 2, 153);
  $d = $doy - intdiv(153 * $mp + 2, 5) + 1;
  $m = $mp < 10 ? $mp + 3 : $mp - 9;
  return array($m <= 2 ? $y + 1 : $y, $m, $d);
}
function lekFromCivil($y0, $m, $d) {
  $y = $m <= 2 ? $y0 - 1 : $y0;
  $era = ($y >= 0 ? $y : $y - 399) / 400;
  $era = $era >= 0 ? (int) floor($era) : (int) ceil($era);
  $yoe = $y - $era * 400;
  $mp = $m > 2 ? $m - 3 : $m + 9;
  $doy = intdiv(153 * $mp + 2, 5) + $d - 1;
  $doe = $yoe * 365 + intdiv($yoe, 4) - intdiv($yoe, 100) + $doy;
  return $era * 146097 + $doe - 719468;
}
function lekFloorDiv($a, $b) {
  $q = intdiv($a, $b);
  $r = $a % $b;
  if ($r !== 0 && (($r < 0) !== ($b < 0))) { $q--; }
  return $q;
}
function lekDaysInMonth($y, $m) {
  if ($m === 2) { $leap = ($y % 4 === 0 && $y % 100 !== 0) || $y % 400 === 0; return $leap ? 29 : 28; }
  return ($m === 4 || $m === 6 || $m === 9 || $m === 11) ? 30 : 31;
}
function lekValidFields($y, $mo, $d, $h, $mi, $s) {
  if ($y < 1 || $y > 9999 || $mo < 1 || $mo > 12) { return false; }
  if ($d < 1 || $d > lekDaysInMonth($y, $mo)) { return false; }
  return $h <= 23 && $mi <= 59 && $s <= 59;
}
function lekDtRange($d) { $c = lekCivil(lekFloorDiv($d, 86400)); return $c[0] >= 1 && $c[0] <= 9999; }
function lekDtAdd($d, $s) { $r = $d + $s; if (is_float($r) || !lekDtRange($r)) { lekFail('datetime-overflow'); } return $r; }
function lekDtSub($d, $s) { $r = $d - $s; if (is_float($r) || !lekDtRange($r)) { lekFail('datetime-overflow'); } return $r; }
function lekDtDiff($a, $b) { $r = $a - $b; if (is_float($r)) { lekFail('duration-overflow'); } return lekDurBound($r); }
function lekYear($d) { return lekCivil(lekFloorDiv($d, 86400))[0]; }
function lekMonth($d) { return lekCivil(lekFloorDiv($d, 86400))[1]; }
function lekDay($d) { return lekCivil(lekFloorDiv($d, 86400))[2]; }
function lekWeekday($d) { $days = lekFloorDiv($d, 86400); return (($days + 3) % 7 + 7) % 7 + 1; }
function lekDtRender($d) {
  $days = lekFloorDiv($d, 86400);
  $time = (($d % 86400) + 86400) % 86400;
  $c = lekCivil($days);
  return sprintf('%04d-%02d-%02dT%02d:%02d:%02dZ', $c[0], $c[1], $c[2], intdiv($time, 3600), intdiv($time % 3600, 60), $time % 60);
}
function lekClock() {
  $env = $GLOBALS['LEK_ENV'];
  // Only an omitted clock reads the shared epoch default, exactly
  // like the reference decode; an explicit null or any present
  // noncanonical clock refuses with the closed clock token (the
  // runner validates a supplied clock eagerly, before evaluation).
  if (empty($env['clockPresent'])) { return 0; }
  $f = lekCheckClock($env['clockText']);
  return lekFromCivil($f[0], $f[1], $f[2]) * 86400 + $f[3] * 3600 + $f[4] * 60 + $f[5];
}
function lekCheckClock($text) {
  if (!is_string($text)) { lekFail('clock-invalid'); }
  $m = array();
  if (!preg_match('/\A(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z\z/', $text, $m)) { lekFail('clock-invalid'); }
  $f = array((int) $m[1], (int) $m[2], (int) $m[3], (int) $m[4], (int) $m[5], (int) $m[6]);
  if (!lekValidFields($f[0], $f[1], $f[2], $f[3], $f[4], $f[5])) { lekFail('clock-invalid'); }
  return $f;
}
function lekField($scope, $field) {
  // Scope members are objects; a non-object scope refuses instead of
  // crashing the runner.
  $bindings = $GLOBALS['LEK_ENV']['bindings'];
  if (!is_object($bindings) || !property_exists($bindings, $scope)) { lekFail('binding-missing'); }
  $scopeMap = $bindings->$scope;
  if (!is_object($scopeMap) || !property_exists($scopeMap, $field)) { lekFail('binding-missing'); }
  $v = $scopeMap->$field;
  if ($v === null) { lekFail('binding-null'); }
  return $v;
}
function lekGetBool($scope, $field) { return lekToBool(lekField($scope, $field)); }
function lekGetInt($scope, $field) { return lekToInt(lekField($scope, $field)); }
function lekGetStr($scope, $field) { return lekToStr(lekField($scope, $field)); }
function lekGetDt($scope, $field) { return lekParseClockText(lekGetStr($scope, $field)); }
function lekGetDur($scope, $field) { return lekToDur(lekField($scope, $field)); }
function lekSetOf($v, $convert) {
  if (!is_array($v)) { lekFail('binding-value'); }
  if (count($v) > 64) { lekFail('binding-value'); }
  $out = array();
  foreach ($v as $item) { $out[] = $convert($item); }
  usort($out, function ($a, $b) {
    if (is_bool($a)) { return $a === $b ? 0 : ($a ? 1 : -1); }
    if (is_string($a)) { return strcmp($a, $b); }
    return $a < $b ? -1 : ($a > $b ? 1 : 0);
  });
  $n = count($out);
  for ($i = 1; $i < $n; $i++) { if ($out[$i - 1] === $out[$i]) { lekFail('binding-value'); } }
  return $out;
}
function lekToBool($v) { if (!is_bool($v)) { lekFail('binding-value'); } return $v; }
function lekToInt($v) { if (!is_int($v) || $v > 9007199254740991 || $v < -9007199254740991) { lekFail('binding-value'); } return $v; }
function lekToStr($v) { if (!is_string($v) || strlen($v) > 256 || !lekStrOk($v)) { lekFail('binding-value'); } return $v; }
function lekToDur($v) { if (!is_int($v) || $v > 31536000000 || $v < -31536000000) { lekFail('binding-value'); } return $v; }
function lekGetSetBool($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekToBool'); }
function lekGetSetInt($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekToInt'); }
function lekGetSetStr($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekToStr'); }
function lekGetSetDt($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekParseClockText'); }
function lekGetSetDur($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekToDur'); }
function lekParamGetter($ty) {
  static $getters = array(
    'bool' => 'lekGetBool',
    'int' => 'lekGetInt',
    'string' => 'lekGetStr',
    'datetime' => 'lekGetDt',
    'duration' => 'lekGetDur',
    'set:bool' => 'lekGetSetBool',
    'set:int' => 'lekGetSetInt',
    'set:string' => 'lekGetSetStr',
    'set:datetime' => 'lekGetSetDt',
    'set:duration' => 'lekGetSetDur'
  );
  return isset($getters[$ty]) ? $getters[$ty] : 'lekFailInvalidParam';
}
function lekFailInvalidParam() { lekFail('binding-value'); }
function lekValidateBindings($params, $bindings) {
  // The eager per-record binding validation the reference performs
  // before evaluation, in exactly the reference pass order: first
  // every scope name and scope shape, then every field name across
  // all scopes, then every declared parameter value — presence
  // (even for nullable references), nullability, and every typed
  // value, including values on branches the body never takes.
  if (!is_object($bindings)) { lekFail('bindings-shape'); }
  $scopes = array_keys((array) $bindings);
  sort($scopes, SORT_STRING);
  foreach ($scopes as $scope) {
    if ($scope !== 'input' && $scope !== 'actor' && $scope !== 'entity' && $scope !== 'result') { lekFail('binding-unknown'); }
    if (!is_object($bindings->$scope)) { lekFail('bindings-shape'); }
  }
  foreach ($scopes as $scope) {
    $fields = array_keys((array) $bindings->$scope);
    sort($fields, SORT_STRING);
    foreach ($fields as $field) {
      $known = false;
      foreach ($params as $p) { if ($p['s'] === $scope && $p['f'] === $field) { $known = true; break; } }
      if (!$known) { lekFail('binding-unknown'); }
    }
  }
  foreach ($params as $p) {
    // property_exists is an own-member test: a declared field is
    // never satisfied by anything but an actual decoded member.
    if (!property_exists($bindings, $p['s']) || !property_exists($bindings->{$p['s']}, $p['f'])) { lekFail('binding-missing'); }
    $v = $bindings->{$p['s']}->{$p['f']};
    if ($v === null) { if (!$p['n']) { lekFail('binding-null'); } continue; }
    $getter = lekParamGetter($p['t']);
    $getter($p['s'], $p['f']);
  }
}
function lekParseClockText($text) {
  // Strict end-of-input anchors: a trailing newline is never part of
  // a canonical timestamp.
  $m = array();
  if (!is_string($text) || !preg_match('/\A(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z\z/', $text, $m)) { lekFail('binding-value'); }
  $f = array((int) $m[1], (int) $m[2], (int) $m[3], (int) $m[4], (int) $m[5], (int) $m[6]);
  if (!lekValidFields($f[0], $f[1], $f[2], $f[3], $f[4], $f[5])) { lekFail('binding-value'); }
  return lekFromCivil($f[0], $f[1], $f[2]) * 86400 + $f[3] * 3600 + $f[4] * 60 + $f[5];
}
function lekIsNull($scope, $field) {
  $bindings = $GLOBALS['LEK_ENV']['bindings'];
  return !is_object($bindings) || !property_exists($bindings, $scope) || !is_object($bindings->$scope) || !property_exists($bindings->$scope, $field) || $bindings->$scope->$field === null;
}
function lekNotNull($scope, $field) { return !lekIsNull($scope, $field); }
function lekLen($s) {
  $count = 0;
  for ($i = 0; $i < strlen($s); $i++) {
    if ((ord($s[$i]) & 0xC0) !== 0x80) { $count++; }
  }
  return $count;
}
function lekConcat($a, $b) { $s = $a . $b; if (strlen($s) > 256 || !lekStrOk($s)) { lekFail('concat-overflow'); } return $s; }
function lekStrOk($s) {
  if (preg_match('//u', $s) !== 1) { return false; }
  for ($i = 0; $i < strlen($s); $i++) {
    $o = ord($s[$i]);
    if ($o < 0x20 || $o >= 0xF0) { return false; }
  }
  return true;
}
function lekLower($s) { $out = ''; for ($i = 0; $i < strlen($s); $i++) { $o = ord($s[$i]); $out .= ($o >= 65 && $o <= 90) ? chr($o + 32) : $s[$i]; } return $out; }
function lekUpper($s) { $out = ''; for ($i = 0; $i < strlen($s); $i++) { $o = ord($s[$i]); $out .= ($o >= 97 && $o <= 122) ? chr($o - 32) : $s[$i]; } return $out; }
function lekCmpStr($a, $b) { return strcmp($a, $b); }
function lekContains($a, $b) { return strpos($a, $b) !== false; }
function lekStartsWith($a, $b) { return strncmp($a, $b, strlen($b)) === 0; }
function lekEndsWith($a, $b) { $n = strlen($b); return $n === 0 || substr($a, -$n) === $b; }
function lekIntToStr($n) { return (string) $n; }
function lekStrToInt($s) {
  if (!preg_match('/^-?(0|[1-9][0-9]{0,15})$/', $s)) { lekFail('cast-invalid'); }
  $v = (int) $s;
  if ($v > 9007199254740991 || $v < -9007199254740991) { lekFail('cast-invalid'); }
  return $v;
}
function lekHasBool($v, $items) { return in_array($v, $items, true); }
function lekHasInt($v, $items) { return in_array($v, $items, true); }
function lekHasStr($v, $items) { return in_array($v, $items, true); }
function lekHasDt($v, $items) { return in_array($v, $items, true); }
function lekHasDur($v, $items) { return in_array($v, $items, true); }
function lekSetLitBool(...$items) { return $items; }
function lekSetLitInt(...$items) { return $items; }
function lekSetLitStr(...$items) { return $items; }
function lekSetLitDt(...$items) { return $items; }
function lekSetLitDur(...$items) { return $items; }
function lekSetLitAny(...$items) { return $items; }
function lekFailInvalid() { lekFail('cast-invalid'); }
function lekHasNegativeZeroToken($raw) {
  $len = strlen($raw);
  $inStr = false;
  $i = 0;
  while ($i < $len) {
    $c = $raw[$i];
    if (!$inStr) {
      if ($c === '"') { $inStr = true; $i++; continue; }
      $numeric = '0123456789.eE+-';
      if ($c === '-' && $i + 1 < $len && $raw[$i + 1] === '0') {
        $j = $i + 2;
        // The exact token `-0` (a delimiter follows): the spelling
        // the reference parse refuses. Longer spellings (-0.0,
        // -0e2) are fractional/exponent forms the typed getters
        // already refuse; digits directly after would not be valid
        // JSON at all.
        if ($j >= $len || strpos($numeric, $raw[$j]) === false) { return true; }
        $i = $j;
        while ($i < $len && strpos($numeric, $raw[$i]) !== false) { $i++; }
        continue;
      }
      if (strpos($numeric, $c) !== false) {
        $i++;
        while ($i < $len && strpos($numeric, $raw[$i]) !== false) { $i++; }
        continue;
      }
      $i++;
      continue;
    }
    if ($c === '\\') { $i += 2; continue; }
    if ($c === '"') { $inStr = false; }
    $i++;
  }
  return false;
}
"#;

const PHP_RUNNER: &str = r#"
function lekEncodeTyped($value, $ty) {
  if ($ty === 'datetime') { return lekDtRender($value); }
  if (strpos($ty, 'set:') === 0) { $inner = substr($ty, 4); $out = array(); foreach ($value as $item) { $out[] = lekEncodeTyped($item, $inner); } return $out; }
  return $value;
}
$raw = stream_get_contents(STDIN);
// The PHP decoder normalizes the negative-zero integer spelling
// (-0) to integer 0, so it can no longer be detected at the typed
// boundary: scan the raw document text for that exact number token
// (outside strings) and refuse the document first, exactly like
// the reference parse which preserves the spelling and refuses.
if (lekHasNegativeZeroToken($raw)) { fwrite(STDERR, "lekalo vector input error\n"); exit(1); }
// Objects decode as stdClass so the binding structure checks can
// tell an object scope from an array, exactly like the reference.
$doc = json_decode($raw);
if (!is_object($doc) || !isset($doc->vectors) || !is_array($doc->vectors)) { fwrite(STDERR, "lekalo vector input error\n"); exit(1); }
$results = array();
foreach ($doc->vectors as $vector) {
  // The consumed id is echoed into the result row, so only a
  // primitive string id can reach the envelope: a null, number,
  // boolean, object, array, or absent id refuses the document
  // instead of emitting a malformed or id-less value row.
  if (!is_object($vector) || !isset($vector->id) || !is_string($vector->id) || !isset($vector->expression) || !isset($vector->bindings)) { fwrite(STDERR, "lekalo vector input error\n"); exit(1); }
  // Presence is distinct from value: only an omitted clock reads
  // the epoch default, and an explicit null or malformed supplied
  // clock refuses even when the body never reads it.
  $clockPresent = property_exists($vector, 'clock');
  $GLOBALS['LEK_ENV'] = array('clockText' => ($clockPresent ? $vector->clock : null), 'clockPresent' => $clockPresent, 'bindings' => $vector->bindings);
  $row = array('id' => $vector->id);
  try {
    if ($clockPresent) { lekCheckClock($GLOBALS['LEK_ENV']['clockText']); }
    // Only a primitive string selects an expression record: the
    // selector is validated before any table lookup, so an array,
    // object, or scalar can never be coerced into an expression
    // name and computed. The supplied-clock check above keeps its
    // first place on a double-fault vector (a null or absent
    // selector already refused the document above).
    if (!is_string($vector->expression)) { lekFail('expression-unknown'); }
    if (!isset($LEK_PARAMS[$vector->expression])) { lekFail('expression-unknown'); }
    lekValidateBindings($LEK_PARAMS[$vector->expression], $vector->bindings);
    $fn = $LEK[$vector->expression];
    $row['value'] = lekEncodeTyped($fn($GLOBALS['LEK_ENV']), $LEK_TYPES[$vector->expression]);
  } catch (Throwable $e) {
    unset($row['value']);
    $msg = $e->getMessage();
    $row['error'] = (is_string($msg) && preg_match('/^[a-z][a-z0-9-]*$/', $msg) === 1) ? $msg : 'runtime-error';
  }
  $results[] = $row;
}
fwrite(STDOUT, json_encode(array('results' => $results)) . "\n");
"#;

const GO_PRELUDE: &str = r#"func lekFail(token string) { panic(token) }

const lekMaxInt = int64(9007199254740991)
const lekMaxDur = int64(31536000000)
const lekMinI64 = int64(-9223372036854775808)

func lekIntBound(v int64) int64 {
	if v > lekMaxInt || v < -lekMaxInt {
		lekFail("int-overflow")
	}
	return v
}

func lekAdd(a, b int64) int64 {
	r := a + b
	if (a > 0 && b > 0 && r < 0) || (a < 0 && b < 0 && r >= 0) {
		lekFail("int-overflow")
	}
	return lekIntBound(r)
}

func lekSub(a, b int64) int64 {
	r := a - b
	if (a >= 0 && b < 0 && r < 0) || (a < 0 && b > 0 && r >= 0) {
		lekFail("int-overflow")
	}
	return lekIntBound(r)
}

func lekMul(a, b int64) int64 {
	if a == 0 || b == 0 {
		return 0
	}
	r := a * b
	if r/b != a {
		lekFail("int-overflow")
	}
	return lekIntBound(r)
}

func lekDiv(a, b int64) int64 {
	if b == 0 {
		lekFail("divide-by-zero")
	}
	if a == lekMinI64 && b == -1 {
		lekFail("int-overflow")
	}
	return lekIntBound(a / b)
}

func lekMod(a, b int64) int64 {
	if b == 0 {
		lekFail("modulo-by-zero")
	}
	if a == lekMinI64 && b == -1 {
		lekFail("int-overflow")
	}
	return lekIntBound(a % b)
}

func lekAbs(a int64) int64 {
	if a < 0 {
		return lekSub(0, a)
	}
	return a
}

func lekDurBound(v int64) int64 {
	if v > lekMaxDur || v < -lekMaxDur {
		lekFail("duration-overflow")
	}
	return v
}

func lekDurAdd(a, b int64) int64 { return lekDurBound(lekAdd(a, b)) }
func lekDurSub(a, b int64) int64 { return lekDurBound(lekSub(a, b)) }

func lekDurMul(a, b int64) int64 {
	// Duration multiplication refuses with the duration token for
	// both operand orders, whether the machine multiplication or only
	// the narrower duration domain overflows — never int-overflow.
	if a == 0 || b == 0 {
		return 0
	}
	r := a * b
	if r/b != a {
		lekFail("duration-overflow")
	}
	return lekDurBound(r)
}

func lekDurDiv(a, b int64) int64 {
	if b == 0 {
		lekFail("divide-by-zero")
	}
	if a == lekMinI64 && b == -1 {
		lekFail("int-overflow")
	}
	return lekDurBound(a / b)
}

func lekDurSeconds(d int64) int64 { return d }

func lekCivil(z0 int64) (int64, int64, int64) {
	z := z0 + 719468
	// Floor division over the era, exactly like the reference and
	// the Node/PHP preludes: truncation toward zero would shift
	// negative epochs by one era.
	var era int64
	if z >= 0 {
		era = z / 146097
	} else {
		era = (z - 146096) / 146097
	}
	doe := z - era*146097
	yoe := (doe - doe/1460 + doe/36524 - doe/146096) / 365
	y := yoe + era*400
	doy := doe - (365*yoe + yoe/4 - yoe/100)
	mp := (5*doy + 2) / 153
	d := doy - (153*mp+2)/5 + 1
	var m int64
	if mp < 10 {
		m = mp + 3
	} else {
		m = mp - 9
	}
	if m <= 2 {
		y++
	}
	return y, m, d
}

func lekFromCivil(y0, m, d int64) int64 {
	var y int64
	if m <= 2 {
		y = y0 - 1
	} else {
		y = y0
	}
	var era int64
	if y >= 0 {
		era = y / 400
	} else {
		era = (y - 399) / 400
	}
	yoe := y - era*400
	var mp int64
	if m > 2 {
		mp = m - 3
	} else {
		mp = m + 9
	}
	doy := (153*mp+2)/5 + d - 1
	doe := yoe*365 + yoe/4 - yoe/100 + doy
	return era*146097 + doe - 719468
}

func lekFloorDiv(a, b int64) int64 {
	q := a / b
	if (a%b != 0) && ((a < 0) != (b < 0)) {
		q--
	}
	return q
}

func lekDtRange(d int64) bool {
	y, _, _ := lekCivil(lekFloorDiv(d, 86400))
	return y >= 1 && y <= 9999
}

func lekDtAdd(d, s int64) int64 {
	r := d + s
	if (d > 0 && s > 0 && r < 0) || (d < 0 && s < 0 && r >= 0) || !lekDtRange(r) {
		lekFail("datetime-overflow")
	}
	return r
}

func lekDtSub(d, s int64) int64 {
	r := d - s
	if (d >= 0 && s < 0 && r < 0) || (d < 0 && s > 0 && r >= 0) || !lekDtRange(r) {
		lekFail("datetime-overflow")
	}
	return r
}

func lekDtDiff(a, b int64) int64 {
	r := a - b
	if (a >= 0 && b < 0 && r < 0) || (a < 0 && b > 0 && r >= 0) {
		lekFail("duration-overflow")
	}
	return lekDurBound(r)
}

func lekYear(d int64) int64  { y, _, _ := lekCivil(lekFloorDiv(d, 86400)); return y }
func lekMonth(d int64) int64 { _, m, _ := lekCivil(lekFloorDiv(d, 86400)); return m }
func lekDay(d int64) int64   { _, _, dd := lekCivil(lekFloorDiv(d, 86400)); return dd }

func lekWeekday(d int64) int64 {
	days := lekFloorDiv(d, 86400)
	w := (days + 3) % 7
	if w < 0 {
		w += 7
	}
	return w + 1
}

func lekDtRender(d int64) string {
	days := lekFloorDiv(d, 86400)
	time := ((d % 86400) + 86400) % 86400
	y, m, dd := lekCivil(days)
	return fmt.Sprintf("%04d-%02d-%02dT%02d:%02d:%02dZ", y, m, dd, time/3600, (time%3600)/60, time%60)
}

func lekClock(env map[string]any) int64 {
	// The runner stores a *string: absent and null clocks read the
	// shared epoch default, exactly like the reference decode; a
	// present-but-malformed clock still refuses with the closed
	// clock token.
	textPtr, ok := env["clockText"].(*string)
	if !ok || textPtr == nil {
		return 0
	}
	y, mo, d, h, mi, s, ok := lekClockFields(*textPtr)
	if !ok {
		lekFail("clock-invalid")
	}
	return lekFromCivil(y, mo, d)*86400 + h*3600 + mi*60 + s
}

func lekLen(s string) int64 { return int64(len([]rune(s))) }

func lekConcat(a, b string) string {
	s := a + b
	if len(s) > 256 || !lekStrOk(s) {
		lekFail("concat-overflow")
	}
	return s
}

func lekStrOk(s string) bool {
	for _, r := range s {
		if r > 0xFFFF || r < 0x20 {
			return false
		}
	}
	return true
}

func lekLower(s string) string {
	var b strings.Builder
	for _, r := range s {
		if r >= 'A' && r <= 'Z' {
			b.WriteRune(r + 32)
		} else {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func lekUpper(s string) string {
	var b strings.Builder
	for _, r := range s {
		if r >= 'a' && r <= 'z' {
			b.WriteRune(r - 32)
		} else {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func lekCmpStr(a, b string) int   { return strings.Compare(a, b) }
func lekContains(a, b string) bool { return strings.Contains(a, b) }
func lekStartsWith(a, b string) bool { return strings.HasPrefix(a, b) }
func lekEndsWith(a, b string) bool   { return strings.HasSuffix(a, b) }
func lekIntToStr(n int64) string    { return strconv.FormatInt(n, 10) }

func lekIfInt64(c bool, a, b int64) int64 {
	if c {
		return a
	}
	return b
}

func lekIfStr(c bool, a, b string) string {
	if c {
		return a
	}
	return b
}

func lekIfBool(c bool, a, b bool) bool {
	if c {
		return a
	}
	return b
}

func lekIfAny(c bool, a, b any) any {
	if c {
		return a
	}
	return b
}

func lekStrToInt(s string) int64 {
	ok := len(s) > 0
	body := s
	if ok && s[0] == '-' {
		body = s[1:]
	}
	if ok && (len(body) == 0 || len(body) > 16) {
		ok = false
	}
	if ok && len(body) > 1 && body[0] == '0' {
		ok = false
	}
	if ok {
		for i := 0; i < len(body); i++ {
			if body[i] < '0' || body[i] > '9' {
				ok = false
				break
			}
		}
	}
	if !ok {
		lekFail("cast-invalid")
	}
	v, err := strconv.ParseInt(s, 10, 64)
	if err != nil || v > lekMaxInt || v < -lekMaxInt {
		lekFail("cast-invalid")
	}
	return v
}

func lekHasBool(v bool, items []bool) bool {
	for _, item := range items {
		if item == v {
			return true
		}
	}
	return false
}

func lekHasInt(v int64, items []int64) bool {
	for _, item := range items {
		if item == v {
			return true
		}
	}
	return false
}

func lekHasDt(v int64, items []int64) bool { return lekHasInt(v, items) }
func lekHasDur(v int64, items []int64) bool { return lekHasInt(v, items) }

func lekHasStr(v string, items []string) bool {
	for _, item := range items {
		if item == v {
			return true
		}
	}
	return false
}

func lekEncBool(v bool) any     { return v }
func lekEncInt(v int64) any     { return json.Number(strconv.FormatInt(v, 10)) }
func lekEncStr(v string) any    { return v }
func lekEncDt(v int64) any      { return lekDtRender(v) }
func lekEncDur(v int64) any     { return json.Number(strconv.FormatInt(v, 10)) }
func lekEncSetBool(v []bool) any {
	out := make([]any, len(v))
	for i, item := range v {
		out[i] = item
	}
	return out
}

func lekEncSetInt(v []int64) any {
	out := make([]any, len(v))
	for i, item := range v {
		out[i] = json.Number(strconv.FormatInt(item, 10))
	}
	return out
}

func lekEncSetStr(v []string) any {
	out := make([]any, len(v))
	for i, item := range v {
		out[i] = item
	}
	return out
}

func lekEncSetDt(v []int64) any {
	out := make([]any, len(v))
	for i, item := range v {
		out[i] = lekDtRender(item)
	}
	return out
}
func lekEncSetDur(v []int64) any  { return lekEncSetInt(v) }

func lekEnvField(env map[string]any, scope, field string) any {
	bindings, ok := env["bindings"].(map[string]any)
	if !ok {
		lekFail("binding-value")
	}
	sc, ok := bindings[scope].(map[string]any)
	if !ok {
		lekFail("binding-missing")
	}
	v, ok := sc[field]
	if !ok {
		lekFail("binding-missing")
	}
	if v == nil {
		lekFail("binding-null")
	}
	return v
}

func lekAnyInt(v any) int64 {
	switch n := v.(type) {
	case json.Number:
		// The exact JSON scalar types: only a canonical integer
		// spelling is an int binding. Fractional and exponent forms
		// fail Int64 and the textual check alike, and the
		// negative-zero spelling (-0) refuses exactly like the
		// reference parse, which preserves it as a float.
		if text := n.String(); text == "-0" || strings.ContainsAny(text, ".eE") {
			lekFail("binding-value")
		}
		i, err := n.Int64()
		if err != nil {
			lekFail("binding-value")
		}
		if i > lekMaxInt || i < -lekMaxInt {
			lekFail("binding-value")
		}
		return i
	case float64:
		if n != float64(int64(n)) || math.Signbit(n) {
			lekFail("binding-value")
		}
		i := int64(n)
		if i > lekMaxInt || i < -lekMaxInt {
			lekFail("binding-value")
		}
		return i
	}
	lekFail("binding-value")
	return 0
}

func lekAnyDur(v any) int64 {
	d := lekAnyInt(v)
	if d > lekMaxDur || d < -lekMaxDur {
		lekFail("binding-value")
	}
	return d
}

func lekAnyStr(v any) string {
	s, ok := v.(string)
	if !ok {
		lekFail("binding-value")
	}
	if len(s) > 256 || !lekStrOk(s) {
		lekFail("binding-value")
	}
	return s
}

func lekAnyBool(v any) bool {
	b, ok := v.(bool)
	if !ok {
		lekFail("binding-value")
	}
	return b
}

func lekGetBool(env map[string]any, scope, field string) bool {
	return lekAnyBool(lekEnvField(env, scope, field))
}

func lekGetInt(env map[string]any, scope, field string) int64 {
	return lekAnyInt(lekEnvField(env, scope, field))
}

func lekGetStr(env map[string]any, scope, field string) string {
	return lekAnyStr(lekEnvField(env, scope, field))
}

func lekGetDt(env map[string]any, scope, field string) int64 {
	return lekParseClockText(lekGetStr(env, scope, field))
}

func lekGetDur(env map[string]any, scope, field string) int64 {
	return lekAnyDur(lekEnvField(env, scope, field))
}

func lekSetBound(raw []any) []any {
	if len(raw) > 64 {
		lekFail("binding-value")
	}
	return raw
}

func lekGetSetBool(env map[string]any, scope, field string) []bool {
	raw, ok := lekEnvField(env, scope, field).([]any)
	if !ok {
		lekFail("binding-value")
	}
	raw = lekSetBound(raw)
	out := make([]bool, 0, len(raw))
	for _, item := range raw {
		out = append(out, lekAnyBool(item))
	}
	sort.Slice(out, func(i, j int) bool { return !out[i] && out[j] })
	for i := 1; i < len(out); i++ {
		if out[i-1] == out[i] {
			lekFail("binding-value")
		}
	}
	return out
}

func lekGetSetInt(env map[string]any, scope, field string) []int64 {
	raw, ok := lekEnvField(env, scope, field).([]any)
	if !ok {
		lekFail("binding-value")
	}
	raw = lekSetBound(raw)
	out := make([]int64, 0, len(raw))
	for _, item := range raw {
		out = append(out, lekAnyInt(item))
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	for i := 1; i < len(out); i++ {
		if out[i-1] == out[i] {
			lekFail("binding-value")
		}
	}
	return out
}

func lekGetSetStr(env map[string]any, scope, field string) []string {
	raw, ok := lekEnvField(env, scope, field).([]any)
	if !ok {
		lekFail("binding-value")
	}
	raw = lekSetBound(raw)
	out := make([]string, 0, len(raw))
	for _, item := range raw {
		out = append(out, lekAnyStr(item))
	}
	sort.Strings(out)
	for i := 1; i < len(out); i++ {
		if out[i-1] == out[i] {
			lekFail("binding-value")
		}
	}
	return out
}

func lekGetSetDt(env map[string]any, scope, field string) []int64 {
	raw, ok := lekEnvField(env, scope, field).([]any)
	if !ok {
		lekFail("binding-value")
	}
	raw = lekSetBound(raw)
	out := make([]int64, 0, len(raw))
	for _, item := range raw {
		out = append(out, lekParseClockText(lekAnyStr(item)))
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	for i := 1; i < len(out); i++ {
		if out[i-1] == out[i] {
			lekFail("binding-value")
		}
	}
	return out
}

func lekGetSetDur(env map[string]any, scope, field string) []int64 {
	raw, ok := lekEnvField(env, scope, field).([]any)
	if !ok {
		lekFail("binding-value")
	}
	raw = lekSetBound(raw)
	out := make([]int64, 0, len(raw))
	for _, item := range raw {
		out = append(out, lekAnyDur(item))
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	for i := 1; i < len(out); i++ {
		if out[i-1] == out[i] {
			lekFail("binding-value")
		}
	}
	return out
}

func lekIsNull(env map[string]any, scope, field string) bool {
	bindings, ok := env["bindings"].(map[string]any)
	if !ok {
		return true
	}
	sc, ok := bindings[scope].(map[string]any)
	if !ok {
		return true
	}
	v, ok := sc[field]
	return !ok || v == nil
}

func lekNotNull(env map[string]any, scope, field string) bool {
	return !lekIsNull(env, scope, field)
}

func lekSetLitBool(items ...bool) []bool { return items }
func lekSetLitInt(items ...int64) []int64 { return items }
func lekSetLitStr(items ...string) []string { return items }
func lekSetLitDt(items ...int64) []int64 { return items }
func lekSetLitDur(items ...int64) []int64 { return items }
func lekSetLitAny(items ...any) []any { return items }

func lekFailInvalid() { lekFail("cast-invalid") }

func lekDaysInMonth(y, m int64) int64 {
	if m == 2 {
		leap := (y%4 == 0 && y%100 != 0) || y%400 == 0
		if leap {
			return 29
		}
		return 28
	}
	if m == 4 || m == 6 || m == 9 || m == 11 {
		return 30
	}
	return 31
}

func lekValidFields(y, mo, d, h, mi, s int64) bool {
	if y < 1 || y > 9999 || mo < 1 || mo > 12 {
		return false
	}
	if d < 1 || d > lekDaysInMonth(y, mo) {
		return false
	}
	return h <= 23 && mi <= 59 && s <= 59
}

func lekClockFields(text string) (int64, int64, int64, int64, int64, int64, bool) {
	if len(text) != 20 || text[4] != '-' || text[7] != '-' || text[10] != 'T' || text[13] != ':' || text[16] != ':' || text[19] != 'Z' {
		return 0, 0, 0, 0, 0, 0, false
	}
	// Every field byte must be an ASCII digit: signs and negative
	// fields are refused exactly like the reference calendar parse.
	digit := func(s string) (int64, bool) {
		if len(s) == 0 {
			return 0, false
		}
		for i := 0; i < len(s); i++ {
			if s[i] < '0' || s[i] > '9' {
				return 0, false
			}
		}
		v, err := strconv.ParseInt(s, 10, 64)
		if err != nil {
			return 0, false
		}
		return v, true
	}
	y, ok1 := digit(text[0:4])
	mo, ok2 := digit(text[5:7])
	d, ok3 := digit(text[8:10])
	h, ok4 := digit(text[11:13])
	mi, ok5 := digit(text[14:16])
	s, ok6 := digit(text[17:19])
	if !ok1 || !ok2 || !ok3 || !ok4 || !ok5 || !ok6 {
		return 0, 0, 0, 0, 0, 0, false
	}
	return y, mo, d, h, mi, s, lekValidFields(y, mo, d, h, mi, s)
}

func lekParseClockText(text string) int64 {
	y, mo, d, h, mi, s, ok := lekClockFields(text)
	if !ok {
		lekFail("binding-value")
	}
	return lekFromCivil(y, mo, d)*86400 + h*3600 + mi*60 + s
}

func lekHex4(s []byte) (int, bool) {
	v := 0
	for _, c := range s {
		v <<= 4
		switch {
		case c >= '0' && c <= '9':
			v |= int(c - '0')
		case c >= 'a' && c <= 'f':
			v |= int(c-'a') + 10
		case c >= 'A' && c <= 'F':
			v |= int(c-'A') + 10
		default:
			return 0, false
		}
	}
	return v, true
}

func lekCanonicalDocumentText(raw []byte) bool {
	// The reference JSON decoder refuses lone surrogate escapes:
	// scan the raw document bytes and reject them before any row is
	// evaluated. Non-integer number spellings (42.0, 1e2) and the
	// negative-zero spelling (-0) survive the scan and the typed
	// decode (UseNumber keeps the raw text) and refuse at the typed
	// binding getters with binding-value.
	inStr := false
	for i := 0; i < len(raw); i++ {
		c := raw[i]
		if !inStr {
			if c == '"' {
				inStr = true
			}
			continue
		}
		switch c {
		case '\\':
			if i+1 >= len(raw) {
				return false
			}
			if raw[i+1] == 'u' {
				if i+6 > len(raw) {
					return false
				}
				cp, ok := lekHex4(raw[i+2 : i+6])
				if !ok {
					return false
				}
				if cp >= 0xD800 && cp <= 0xDBFF {
					// A high surrogate must be followed by its
					// low pair.
					if i+12 > len(raw) || raw[i+6] != '\\' || raw[i+7] != 'u' {
						return false
					}
					lo, ok := lekHex4(raw[i+8 : i+12])
					if !ok || lo < 0xDC00 || lo > 0xDFFF {
						return false
					}
					i += 6
				} else if cp >= 0xDC00 && cp <= 0xDFFF {
					return false
				}
			}
			i++
		case '"':
			inStr = false
		}
	}
	return true
}

type lekParam struct {
	Scope    string
	Field    string
	Type     string
	Nullable bool
}

func lekValidateBindings(env map[string]any, params []lekParam) {
	// The eager per-record binding validation the reference performs
	// before evaluation, in exactly the reference pass order: first
	// every scope name and scope shape, then every field name across
	// all scopes, then every declared parameter value — presence
	// (even for nullable references), nullability, and every typed
	// value, including values on branches the body never takes.
	bindings, ok := env["bindings"]
	if !ok {
		lekFail("bindings-shape")
	}
	m, ok := bindings.(map[string]any)
	if !ok || m == nil {
		lekFail("bindings-shape")
	}
	scopes := make([]string, 0, len(m))
	for scope := range m {
		scopes = append(scopes, scope)
	}
	sort.Strings(scopes)
	for _, scope := range scopes {
		if scope != "input" && scope != "actor" && scope != "entity" && scope != "result" {
			lekFail("binding-unknown")
		}
		if _, ok := m[scope].(map[string]any); !ok {
			lekFail("bindings-shape")
		}
	}
	for _, scope := range scopes {
		scopeMap, _ := m[scope].(map[string]any)
		fields := make([]string, 0, len(scopeMap))
		for field := range scopeMap {
			fields = append(fields, field)
		}
		sort.Strings(fields)
		for _, field := range fields {
			known := false
			for _, p := range params {
				if p.Scope == scope && p.Field == field {
					known = true
					break
				}
			}
			if !known {
				lekFail("binding-unknown")
			}
		}
	}
	for _, p := range params {
		scopeMap, _ := m[p.Scope].(map[string]any)
		v, present := scopeMap[p.Field]
		if !present {
			lekFail("binding-missing")
		}
		if v == nil {
			if !p.Nullable {
				lekFail("binding-null")
			}
			continue
		}
		switch p.Type {
		case "bool":
			lekGetBool(env, p.Scope, p.Field)
		case "int":
			lekGetInt(env, p.Scope, p.Field)
		case "string":
			lekGetStr(env, p.Scope, p.Field)
		case "datetime":
			lekGetDt(env, p.Scope, p.Field)
		case "duration":
			lekGetDur(env, p.Scope, p.Field)
		case "set:bool":
			lekGetSetBool(env, p.Scope, p.Field)
		case "set:int":
			lekGetSetInt(env, p.Scope, p.Field)
		case "set:string":
			lekGetSetStr(env, p.Scope, p.Field)
		case "set:datetime":
			lekGetSetDt(env, p.Scope, p.Field)
		case "set:duration":
			lekGetSetDur(env, p.Scope, p.Field)
		default:
			lekFail("binding-value")
		}
	}
}
"#;

const GO_RUNNER_HEAD: &str = r#"
func lekRun(expression string, id string, env map[string]any) (row map[string]any) {
	row = map[string]any{"id": id}
	defer func() {
		if r := recover(); r != nil {
			token, ok := r.(string)
			if !ok {
				token = "runtime-error"
			}
			delete(row, "value")
			row["error"] = token
		}
	}()
	// A supplied clock validates eagerly, before the expression
	// lookup, the bindings, and the body: an explicit null never
	// reaches this point (the decode refuses it), and a malformed
	// supplied clock refuses with the closed clock token even when
	// the expression is unknown or the body never reads now — the
	// same clock-first precedence as the reference decode.
	if textPtr, ok := env["clockText"].(*string); ok && textPtr != nil {
		if _, _, _, _, _, _, valid := lekClockFields(*textPtr); !valid {
			lekFail("clock-invalid")
		}
	}
	params, known := lekParams[expression]
	if !known {
		lekFail("expression-unknown")
	}
	lekValidateBindings(env, params)
	switch expression {
"#;

const GO_RUNNER_TAIL: &str = r#"
	default:
		lekFail("expression-unknown")
	}
	return row
}

func main() {
	raw, err := io.ReadAll(os.Stdin)
	if err != nil {
		lekInputFail()
	}
	// Strict UTF-8: the original stdin bytes must be valid UTF-8, so
	// a malformed sequence refuses the document at the boundary
	// instead of being silently replaced with replacement-character
	// binding data. A legal U+FFFD stays accepted.
	if !utf8.Valid(raw) {
		lekInputFail()
	}
	if !lekCanonicalDocumentText(raw) {
		lekInputFail()
	}
	decoder := json.NewDecoder(strings.NewReader(string(raw)))
	// Exact envelope member names: the document and every vector
	// decode into exact-key maps, so a case variant (`Bindings`,
	// `BINDINGS`, `Vectors`, `ID`, `EXPRESSION`) can never satisfy
	// the required lowercase member's presence.
	var members map[string]json.RawMessage
	if err := decoder.Decode(&members); err != nil {
		lekInputFail()
	}
	// Exactly one JSON document is consumed: only whitespace may
	// follow it — any non-whitespace trailing byte or extra value
	// refuses.
	var extra json.RawMessage
	if err := decoder.Decode(&extra); err != io.EOF {
		lekInputFail()
	}
	vectorsRaw, ok := members["vectors"]
	if !ok || string(vectorsRaw) == "null" {
		lekInputFail()
	}
	var vectors []map[string]json.RawMessage
	if err := json.Unmarshal(vectorsRaw, &vectors); err != nil {
		lekInputFail()
	}
	results := make([]map[string]any, 0, len(vectors))
	for _, vector := range vectors {
		// The consumed id is echoed into the result row, so only a
		// primitive string id can reach the envelope: an explicit null
		// refuses here instead of unmarshalling into the empty string.
		idRaw, ok := vector["id"]
		if !ok || string(idRaw) == "null" {
			lekInputFail()
		}
		var id string
		if err := json.Unmarshal(idRaw, &id); err != nil {
			lekInputFail()
		}
		// The selector is a primitive string too: an explicit null
		// refuses here instead of unmarshalling into the empty string,
		// and every other non-string spelling fails the unmarshal.
		expressionRaw, ok := vector["expression"]
		if !ok || string(expressionRaw) == "null" {
			lekInputFail()
		}
		var expression string
		if err := json.Unmarshal(expressionRaw, &expression); err != nil {
			lekInputFail()
		}
		// The clock is a *string plus explicit presence: only an
		// omitted clock reads the epoch default, while an explicit
		// null or a non-string spelling refuses the document exactly
		// like the reference decode.
		var clock *string
		if clockRaw, ok := vector["clock"]; ok {
			if string(clockRaw) == "null" {
				lekInputFail()
			}
			if err := json.Unmarshal(clockRaw, &clock); err != nil || clock == nil {
				lekInputFail()
			}
		}
		// Bindings roots are distinguished exactly: absent and null
		// refuse the document like the reference decode; `{}` stays a
		// valid root for a parameterless record and never computes
		// from a nil map.
		bindingsRaw, ok := vector["bindings"]
		if !ok || string(bindingsRaw) == "null" {
			lekInputFail()
		}
		var bindings map[string]any
		bindingsDecoder := json.NewDecoder(strings.NewReader(string(bindingsRaw)))
		bindingsDecoder.UseNumber()
		if err := bindingsDecoder.Decode(&bindings); err != nil || bindings == nil {
			lekInputFail()
		}
		env := map[string]any{
			"clockText": clock,
			"bindings":  bindings,
		}
		results = append(results, lekRun(expression, id, env))
	}
	encoded, err := json.Marshal(map[string]any{"results": results})
	if err != nil {
		lekInputFail()
	}
	fmt.Println(string(encoded))
}

func lekInputFail() {
	fmt.Fprintln(os.Stderr, "lekalo vector input error")
	os.Exit(1)
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment() -> ExpressionsAttachment {
        let document = serde_json::json!({
            "schemaVersion": super::super::version::SCHEMA_VERSION,
            "identity": super::super::version::IDENTITY,
            "attachmentRevision": "0.2.16",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "0.2.16",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": super::super::version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "builtinSemantics": super::super::version::BUILTIN_SEMANTICS_VERSION,
            "expressions": [
                {
                    "id": "expr.planner/overdue-check",
                    "kind": "condition",
                    "params": [
                        {"scope": "input", "field": "due", "type": "datetime"}
                    ],
                    "result": "bool",
                    "body": {"op": "lt", "left": {"op": "ref", "scope": "input", "field": "due"}, "right": {"op": "now"}}
                }
            ]
        });
        ExpressionsAttachment::from_value(&document).expect("valid")
    }

    #[test]
    fn every_target_renders_a_complete_program() {
        let attachment = attachment();
        for target in [Target::Node, Target::Php, Target::Go] {
            let program = render_program(&attachment, target);
            assert!(
                program.contains("expr.planner/overdue-check"),
                "{:?}",
                target
            );
            assert!(program.contains("lekFail"), "{:?}", target);
            assert!(program.contains("lekGetDt"), "{:?}", target);
        }
    }

    #[test]
    fn target_keys_round_trip() {
        for key in ["node", "php", "go"] {
            assert_eq!(Target::parse(key).expect("target").key(), key);
        }
        assert!(Target::parse("ruby").is_none());
    }
}
