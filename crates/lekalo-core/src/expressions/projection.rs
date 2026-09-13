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
//! seconds under one civil-calendar algorithm, durations are integer
//! seconds, strings are BMP-only UTF-8 so code-point order is byte
//! order everywhere, and casing is ASCII-only by contract. Division
//! and remainder truncate toward zero identically in Rust, Node,
//! PHP, and Go. No generated program reads anything but stdin,
//! writes anything but stdout, or calls anything but its own
//! prelude.

use super::ast::ExprNode;
use super::types::{ExprType, Scalar, ScalarType};
use super::typing::{conditional_then_type, node_type};
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

/// The branch typing the Go printer needs.
#[derive(Clone, Copy, Debug)]
enum BranchType {
    /// An integer-valued branch (int, duration, or datetime).
    Int,
    /// A string-valued branch.
    Str,
    /// A boolean-valued branch.
    Bool,
    /// A set-valued branch (opaque `any` in Go).
    Opaque,
}

/// Compile one record body against its declared references.
fn compile(record: &ExpressionRecord, node: &ExprNode) -> Code {
    let ty = |node: &ExprNode| -> ExprType {
        node_type(node, record).unwrap_or(ExprType::Scalar(ScalarType::Int))
    };
    match node {
        ExprNode::Bool(value) => Code::Bool(*value),
        ExprNode::Int(value) => Code::Int(*value),
        ExprNode::Str(value) => Code::Str(value.clone()),
        ExprNode::DateTime(seconds) => Code::Int(*seconds),
        ExprNode::Duration(seconds) => Code::Int(*seconds),
        ExprNode::Set(_) => {
            // A bare set literal is compiled at its only legal site
            // (the membership operand) below.
            Code::Call("lekSetLitAny", Vec::new())
        }
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
            if is_string(left, record) {
                Code::Call(
                    "lekCmpStr",
                    vec![compile(record, left), compile(record, right)],
                )
                .compare_for_eq()
            } else {
                Code::Equal(
                    false,
                    Box::new(compile(record, left)),
                    Box::new(compile(record, right)),
                )
            }
        }
        ExprNode::NotEqual { left, right } => {
            if is_string(left, record) {
                Code::Call(
                    "lekCmpStr",
                    vec![compile(record, left), compile(record, right)],
                )
                .compare_for_ne()
            } else {
                Code::Equal(
                    true,
                    Box::new(compile(record, left)),
                    Box::new(compile(record, right)),
                )
            }
        }
        ExprNode::Less { left, right } => compare(record, "<", left, right),
        ExprNode::LessEqual { left, right } => compare(record, "<=", left, right),
        ExprNode::Greater { left, right } => compare(record, ">", left, right),
        ExprNode::GreaterEqual { left, right } => compare(record, ">=", left, right),
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
        ExprNode::InSet { operand, set } => Code::Call(
            set_helper(record, set),
            vec![set_operand(record, set), compile(record, operand)],
        ),
        ExprNode::NotInSet { operand, set } => Code::Not(Box::new(Code::Call(
            set_helper(record, set),
            vec![set_operand(record, set), compile(record, operand)],
        ))),
        ExprNode::And(operands) => Code::And(
            operands
                .iter()
                .map(|operand| compile(record, operand))
                .collect(),
        ),
        ExprNode::Or(operands) => Code::Or(
            operands
                .iter()
                .map(|operand| compile(record, operand))
                .collect(),
        ),
        ExprNode::Not { operand } => Code::Not(Box::new(compile(record, operand))),
        ExprNode::Add { left, right } => Code::Call(
            add_name(record, left, right),
            vec![compile(record, left), compile(record, right)],
        ),
        ExprNode::Subtract { left, right } => Code::Call(
            sub_name(record, left, right),
            vec![compile(record, left), compile(record, right)],
        ),
        ExprNode::Multiply { left, right } => Code::Call(
            mul_name(record, left, right),
            vec![compile(record, left), compile(record, right)],
        ),
        ExprNode::Divide { left, right } => Code::Call(
            div_name(record, left),
            vec![compile(record, left), compile(record, right)],
        ),
        ExprNode::Modulo { left, right } => Code::Call(
            "lekMod",
            vec![compile(record, left), compile(record, right)],
        ),
        ExprNode::If {
            condition,
            then,
            otherwise,
        } => {
            let branch = match conditional_then_type(record, condition, then) {
                ExprType::Scalar(ScalarType::Str) => BranchType::Str,
                ExprType::Scalar(ScalarType::Bool) => BranchType::Bool,
                ExprType::Scalar(_) => BranchType::Int,
                ExprType::Set(_) => BranchType::Opaque,
            };
            Code::If(
                Box::new(compile(record, condition)),
                Box::new(compile(record, then)),
                Box::new(compile(record, otherwise)),
                branch,
            )
        }
        ExprNode::Builtin { name, args } => Code::Call(
            builtin_name(name),
            args.iter().map(|arg| compile(record, arg)).collect(),
        ),
    }
}

/// One ordered comparison; string operands compare through the
/// prelude byte-order helper so PHP never applies numeric-string
/// comparison.
fn compare(record: &ExpressionRecord, op: &'static str, left: &ExprNode, right: &ExprNode) -> Code {
    if is_string(left, record) {
        Code::Compare(
            op,
            Box::new(Code::Call(
                "lekCmpStr",
                vec![compile(record, left), compile(record, right)],
            )),
            Box::new(Code::Int(0)),
        )
    } else {
        Code::Compare(
            op,
            Box::new(compile(record, left)),
            Box::new(compile(record, right)),
        )
    }
}

/// Whether one node is statically string-typed.
fn is_string(node: &ExprNode, record: &ExpressionRecord) -> bool {
    node_type(node, record) == Ok(ExprType::Scalar(ScalarType::Str))
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
fn set_helper(record: &ExpressionRecord, set: &ExprNode) -> &'static str {
    match node_type(set, record) {
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
fn set_operand(record: &ExpressionRecord, set: &ExprNode) -> Code {
    match set {
        ExprNode::Set(items) => Code::Call(set_lit_helper(record, set), encode_set_items(items)),
        other => compile(record, other),
    }
}

/// The literal-maker helper of one set literal.
fn set_lit_helper(record: &ExpressionRecord, set: &ExprNode) -> &'static str {
    match node_type(set, record) {
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
fn add_name(record: &ExpressionRecord, left: &ExprNode, right: &ExprNode) -> &'static str {
    match (node_type(left, record), node_type(right, record)) {
        (Ok(ExprType::Scalar(ScalarType::Int)), Ok(ExprType::Scalar(ScalarType::Int))) => "lekAdd",
        (
            Ok(ExprType::Scalar(ScalarType::Duration)),
            Ok(ExprType::Scalar(ScalarType::Duration)),
        ) => "lekDurAdd",
        _ => "lekDtAdd",
    }
}

/// The checked-subtract helper for one operand pair.
fn sub_name(record: &ExpressionRecord, left: &ExprNode, right: &ExprNode) -> &'static str {
    match (node_type(left, record), node_type(right, record)) {
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
fn mul_name(record: &ExpressionRecord, left: &ExprNode, right: &ExprNode) -> &'static str {
    match (node_type(left, record), node_type(right, record)) {
        (Ok(ExprType::Scalar(ScalarType::Int)), Ok(ExprType::Scalar(ScalarType::Int))) => "lekMul",
        _ => "lekDurMul",
    }
}

/// The checked-divide helper for one operand pair.
fn div_name(record: &ExpressionRecord, left: &ExprNode) -> &'static str {
    match node_type(left, record) {
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
        "\n// Generated by lekalo; do not edit. Reads the shared\n// evaluation vectors on stdin, writes computed results to stdout.\npackage main\n\nimport (\n\t\"encoding/json\"\n\t\"fmt\"\n\t\"io\"\n\t\"os\"\n\t\"sort\"\n\t\"strconv\"\n\t\"strings\"\n)\n\n",
    );
    out.push_str(GO_PRELUDE);
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
            let op = if *negated { "!==" } else { "===" };
            format!("({} {} {})", emit_node(left), op, emit_node(right))
        }
        Code::And(operands) => operands
            .iter()
            .map(emit_node)
            .collect::<Vec<_>>()
            .join(" && "),
        Code::Or(operands) => operands
            .iter()
            .map(emit_node)
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
            let op = if *negated { "!=" } else { "===" };
            format!("({} {} {})", emit_php(left), op, emit_php(right))
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
                all.push_str("env, ");
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
            let op = if *negated { "!=" } else { "==" };
            format!("({} {} {})", emit_go(left), op, emit_go(right))
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
                BranchType::Opaque => "any",
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
const LEK = {};
const LEK_TYPES = {};
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
function lekDtRange(d) {
  const y = lekCivil(d / 86400n)[0];
  return y >= 1n && y <= 9999n;
}
function lekDtAdd(d, s) { const r = d + s; if (!lekDtRange(r)) { lekFail("datetime-overflow"); } return r; }
function lekDtSub(d, s) { const r = d - s; if (!lekDtRange(r)) { lekFail("datetime-overflow"); } return r; }
function lekDtDiff(a, b) { return lekDurBound(a - b); }
function lekYear(d) { return lekCivil(d / 86400n)[0]; }
function lekMonth(d) { return lekCivil(d / 86400n)[1]; }
function lekDay(d) { return lekCivil(d / 86400n)[2]; }
function lekWeekday(d) { const days = d / 86400n; return ((days + 3n) % 7n + 7n) % 7n + 1n; }
function lekDtRender(d) {
  const days = d / 86400n;
  const time = ((d % 86400n) + 86400n) % 86400n;
  const parts = lekCivil(days);
  const pad = (v, n) => v.toString().padStart(n, "0");
  return pad(parts[0], 4) + "-" + pad(parts[1], 2) + "-" + pad(parts[2], 2) + "T" + pad(time / 3600n, 2) + ":" + pad((time % 3600n) / 60n, 2) + ":" + pad(time % 60n, 2) + "Z";
}
function lekClock() {
  if (typeof lekEnv.clockText !== "string") { lekFail("clock-missing"); }
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z$/.exec(lekEnv.clockText);
  if (!m) { lekFail("clock-invalid"); }
  return lekFromCivil(BigInt(m[1]), BigInt(m[2]), BigInt(m[3])) * 86400n
    + BigInt(m[4]) * 3600n + BigInt(m[5]) * 60n + BigInt(m[6]);
}
function lekGet(scope, field, convert) {
  const scopeMap = lekEnv.bindings[scope];
  if (!scopeMap || !(field in scopeMap)) { lekFail("binding-missing"); }
  const v = scopeMap[field];
  if (v === null || v === undefined) { lekFail("binding-null"); }
  return convert(v);
}
function toBig(v) {
  if (typeof v === "number" && Number.isInteger(v)) { return BigInt(v); }
  if (typeof v === "string" && /^-?[0-9]+$/.test(v)) { return BigInt(v); }
  lekFail("binding-value");
}
function toStr(v) { if (typeof v !== "string") { lekFail("binding-value"); } return v; }
function toBool(v) { if (typeof v !== "boolean") { lekFail("binding-value"); } return v; }
function toSet(v, convert) {
  if (!Array.isArray(v)) { lekFail("binding-value"); }
  return v.map((item) => convert(item));
}
function lekGetBool(scope, field) { return lekGet(scope, field, toBool); }
function lekGetInt(scope, field) { return lekGet(scope, field, toBig); }
function lekGetStr(scope, field) { return lekGet(scope, field, toStr); }
function lekGetDt(scope, field) { return lekParseClockText(lekGet(scope, field, toStr)); }
function lekGetDur(scope, field) { return lekGet(scope, field, toBig); }
function lekGetSetBool(scope, field) { return lekGet(scope, field, (v) => toSet(v, toBool)); }
function lekGetSetInt(scope, field) { return lekGet(scope, field, (v) => toSet(v, toBig)); }
function lekGetSetStr(scope, field) { return lekGet(scope, field, (v) => toSet(v, toStr)); }
function lekGetSetDt(scope, field) { return lekGet(scope, field, (v) => toSet(v, (s) => lekParseClockText(toStr(s)))); }
function lekGetSetDur(scope, field) { return lekGet(scope, field, (v) => toSet(v, toBig)); }
function lekParseClockText(text) {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z$/.exec(text);
  if (!m) { lekFail("binding-value"); }
  return lekFromCivil(BigInt(m[1]), BigInt(m[2]), BigInt(m[3])) * 86400n
    + BigInt(m[4]) * 3600n + BigInt(m[5]) * 60n + BigInt(m[6]);
}
function lekIsNull(scope, field) { const scopeMap = lekEnv.bindings[scope]; return !scopeMap || !(field in scopeMap) || scopeMap[field] === null || scopeMap[field] === undefined; }
function lekNotNull(scope, field) { return !lekIsNull(scope, field); }
function lekLen(s) { return BigInt(s.length); }
function lekConcat(a, b) {
  const s = a + b;
  if (s.length > 256 || !lekStrOk(s)) { lekFail("concat-overflow"); }
  return s;
}
function lekStrOk(s) {
  for (const ch of s) {
    const c = ch.codePointAt(0);
    if (c > 0xFFFF || c < 0x20) { return false; }
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
function lekHasBool(items, v) { return items.includes(v); }
function lekHasInt(items, v) { return items.includes(v); }
function lekHasStr(items, v) { return items.includes(v); }
function lekHasDt(items, v) { return items.includes(v); }
function lekHasDur(items, v) { return items.includes(v); }
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
function lekMain() {
  const raw = require("fs").readFileSync(0, "utf8");
  const doc = JSON.parse(raw);
  const results = [];
  for (const vector of doc.vectors) {
    lekEnv = { clockText: vector.clock, bindings: vector.bindings };
    const row = { id: vector.id };
    try {
      const fn = LEK[vector.expression];
      if (!fn) { lekFail("expression-unknown"); }
      row.value = lekEncodeTyped(fn(lekEnv), LEK_TYPES[vector.expression]);
    } catch (error) {
      row.error = error.lekToken || "runtime-error";
      delete row.value;
    }
    results.push(row);
  }
  process.stdout.write(JSON.stringify({ results }) + "\n");
}
lekMain();
"#;

const PHP_PRELUDE: &str = r#"
$LEK = array();
$LEK_TYPES = array();
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
function lekDtRange($d) { $c = lekCivil(intdiv($d, 86400)); return $c[0] >= 1 && $c[0] <= 9999; }
function lekDtAdd($d, $s) { $r = $d + $s; if (is_float($r) || !lekDtRange($r)) { lekFail('datetime-overflow'); } return $r; }
function lekDtSub($d, $s) { $r = $d - $s; if (is_float($r) || !lekDtRange($r)) { lekFail('datetime-overflow'); } return $r; }
function lekDtDiff($a, $b) { $r = $a - $b; if (is_float($r)) { lekFail('duration-overflow'); } return lekDurBound($r); }
function lekYear($d) { return lekCivil(intdiv($d, 86400))[0]; }
function lekMonth($d) { return lekCivil(intdiv($d, 86400))[1]; }
function lekDay($d) { return lekCivil(intdiv($d, 86400))[2]; }
function lekWeekday($d) { $days = intdiv($d, 86400); return (($days + 3) % 7 + 7) % 7 + 1; }
function lekDtRender($d) {
  $days = intdiv($d, 86400);
  $time = (($d % 86400) + 86400) % 86400;
  $c = lekCivil($days);
  return sprintf('%04d-%02d-%02dT%02d:%02d:%02dZ', $c[0], $c[1], $c[2], intdiv($time, 3600), intdiv($time % 3600, 60), $time % 60);
}
function lekClock() {
  $text = $GLOBALS['LEK_ENV']['clockText'];
  if (!is_string($text)) { lekFail('clock-missing'); }
  $m = array();
  if (!preg_match('/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z$/', $text, $m)) { lekFail('clock-invalid'); }
  return lekFromCivil((int) $m[1], (int) $m[2], (int) $m[3]) * 86400 + ((int) $m[4]) * 3600 + ((int) $m[5]) * 60 + ((int) $m[6]);
}
function lekField($scope, $field) {
  if (!isset($GLOBALS['LEK_ENV']['bindings'][$scope]) || !array_key_exists($field, $GLOBALS['LEK_ENV']['bindings'][$scope])) { lekFail('binding-missing'); }
  $v = $GLOBALS['LEK_ENV']['bindings'][$scope][$field];
  if ($v === null) { lekFail('binding-null'); }
  return $v;
}
function lekGetBool($scope, $field) { $v = lekField($scope, $field); if (!is_bool($v)) { lekFail('binding-value'); } return $v; }
function lekGetInt($scope, $field) { $v = lekField($scope, $field); if (!is_int($v)) { lekFail('binding-value'); } return $v; }
function lekGetStr($scope, $field) { $v = lekField($scope, $field); if (!is_string($v)) { lekFail('binding-value'); } return $v; }
function lekGetDt($scope, $field) { return lekParseClockText(lekGetStr($scope, $field)); }
function lekGetDur($scope, $field) { return lekGetInt($scope, $field); }
function lekSetOf($v, $convert) { if (!is_array($v)) { lekFail('binding-value'); } $out = array(); foreach ($v as $item) { $out[] = $convert($item); } return $out; }
function lekToBool($v) { if (!is_bool($v)) { lekFail('binding-value'); } return $v; }
function lekToInt($v) { if (!is_int($v)) { lekFail('binding-value'); } return $v; }
function lekToStr($v) { if (!is_string($v)) { lekFail('binding-value'); } return $v; }
function lekGetSetBool($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekToBool'); }
function lekGetSetInt($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekToInt'); }
function lekGetSetStr($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekToStr'); }
function lekGetSetDt($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekParseClockText'); }
function lekGetSetDur($scope, $field) { return lekSetOf(lekField($scope, $field), 'lekToInt'); }
function lekParseClockText($text) {
  $m = array();
  if (!is_string($text) || !preg_match('/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z$/', $text, $m)) { lekFail('binding-value'); }
  return lekFromCivil((int) $m[1], (int) $m[2], (int) $m[3]) * 86400 + ((int) $m[4]) * 3600 + ((int) $m[5]) * 60 + ((int) $m[6]);
}
function lekIsNull($scope, $field) { return !isset($GLOBALS['LEK_ENV']['bindings'][$scope]) || !array_key_exists($field, $GLOBALS['LEK_ENV']['bindings'][$scope]) || $GLOBALS['LEK_ENV']['bindings'][$scope][$field] === null; }
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
function lekEndsWith($a, $b) { return substr($a, -strlen($b)) === $b; }
function lekIntToStr($n) { return (string) $n; }
function lekStrToInt($s) {
  if (!preg_match('/^-?(0|[1-9][0-9]{0,15})$/', $s)) { lekFail('cast-invalid'); }
  $v = (int) $s;
  if ($v > 9007199254740991 || $v < -9007199254740991) { lekFail('cast-invalid'); }
  return $v;
}
function lekHasBool($items, $v) { return in_array($v, $items, true); }
function lekHasInt($items, $v) { return in_array($v, $items, true); }
function lekHasStr($items, $v) { return in_array($v, $items, true); }
function lekHasDt($items, $v) { return in_array($v, $items, true); }
function lekHasDur($items, $v) { return in_array($v, $items, true); }
function lekSetLitBool(...$items) { return $items; }
function lekSetLitInt(...$items) { return $items; }
function lekSetLitStr(...$items) { return $items; }
function lekSetLitDt(...$items) { return $items; }
function lekSetLitDur(...$items) { return $items; }
function lekSetLitAny(...$items) { return $items; }
function lekFailInvalid() { lekFail('cast-invalid'); }
"#;

const PHP_RUNNER: &str = r#"
function lekEncodeTyped($value, $ty) {
  if ($ty === 'datetime') { return lekDtRender($value); }
  if (strpos($ty, 'set:') === 0) { $inner = substr($ty, 4); $out = array(); foreach ($value as $item) { $out[] = lekEncodeTyped($item, $inner); } return $out; }
  return $value;
}
$raw = stream_get_contents(STDIN);
$doc = json_decode($raw, true);
if (!is_array($doc) || !isset($doc['vectors'])) { fwrite(STDERR, "lekalo vector input error\n"); exit(1); }
$results = array();
foreach ($doc['vectors'] as $vector) {
  $GLOBALS['LEK_ENV'] = array('clockText' => (isset($vector['clock']) ? $vector['clock'] : null), 'bindings' => $vector['bindings']);
  $row = array('id' => $vector['id']);
  try {
    if (!isset($LEK[$vector['expression']])) { lekFail('expression-unknown'); }
    $fn = $LEK[$vector['expression']];
    $row['value'] = lekEncodeTyped($fn($GLOBALS['LEK_ENV']), $LEK_TYPES[$vector['expression']]);
  } catch (Exception $e) {
    unset($row['value']);
    $row['error'] = $e->getMessage();
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
func lekDurMul(a, b int64) int64 { return lekDurBound(lekMul(a, b)) }

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

func lekClock(env map[string]any) int64 { return lekParseClockText(env["clockText"].(string)) }

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

func lekHasBool(items []bool, v bool) bool {
	for _, item := range items {
		if item == v {
			return true
		}
	}
	return false
}

func lekHasInt(items []int64, v int64) bool {
	for _, item := range items {
		if item == v {
			return true
		}
	}
	return false
}

func lekHasDt(items []int64, v int64) bool { return lekHasInt(items, v) }
func lekHasDur(items []int64, v int64) bool { return lekHasInt(items, v) }

func lekHasStr(items []string, v string) bool {
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

func lekEncSetDt(v []int64) any   { return lekEncSetInt(v) }
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
		i, err := n.Int64()
		if err != nil {
			lekFail("binding-value")
		}
		return i
	case float64:
		if n != float64(int64(n)) {
			lekFail("binding-value")
		}
		return int64(n)
	}
	lekFail("binding-value")
	return 0
}

func lekAnyStr(v any) string {
	s, ok := v.(string)
	if !ok {
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
	return lekGetInt(env, scope, field)
}

func lekGetSetBool(env map[string]any, scope, field string) []bool {
	raw, ok := lekEnvField(env, scope, field).([]any)
	if !ok {
		lekFail("binding-value")
	}
	out := make([]bool, 0, len(raw))
	for _, item := range raw {
		out = append(out, lekAnyBool(item))
	}
	return out
}

func lekGetSetInt(env map[string]any, scope, field string) []int64 {
	raw, ok := lekEnvField(env, scope, field).([]any)
	if !ok {
		lekFail("binding-value")
	}
	out := make([]int64, 0, len(raw))
	for _, item := range raw {
		out = append(out, lekAnyInt(item))
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

func lekGetSetStr(env map[string]any, scope, field string) []string {
	raw, ok := lekEnvField(env, scope, field).([]any)
	if !ok {
		lekFail("binding-value")
	}
	out := make([]string, 0, len(raw))
	for _, item := range raw {
		out = append(out, lekAnyStr(item))
	}
	sort.Strings(out)
	return out
}

func lekGetSetDt(env map[string]any, scope, field string) []int64 {
	raw, ok := lekEnvField(env, scope, field).([]any)
	if !ok {
		lekFail("binding-value")
	}
	out := make([]int64, 0, len(raw))
	for _, item := range raw {
		out = append(out, lekParseClockText(lekAnyStr(item)))
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

func lekGetSetDur(env map[string]any, scope, field string) []int64 {
	return lekGetSetInt(env, scope, field)
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

func lekParseClockText(text string) int64 {
	if len(text) != 20 || text[4] != '-' || text[7] != '-' || text[10] != 'T' || text[13] != ':' || text[16] != ':' || text[19] != 'Z' {
		lekFail("binding-value")
	}
	digit := func(s string) int64 {
		v, err := strconv.ParseInt(s, 10, 64)
		if err != nil {
			lekFail("binding-value")
		}
		return v
	}
	y := digit(text[0:4])
	mo := digit(text[5:7])
	d := digit(text[8:10])
	h := digit(text[11:13])
	mi := digit(text[14:16])
	s := digit(text[17:19])
	return lekFromCivil(y, mo, d)*86400 + h*3600 + mi*60 + s
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
	var doc struct {
		Vectors []struct {
			ID         string         `json:"id"`
			Expression string         `json:"expression"`
			Clock      string         `json:"clock"`
			Bindings   map[string]any `json:"bindings"`
		} `json:"vectors"`
	}
	decoder := json.NewDecoder(strings.NewReader(string(raw)))
	decoder.UseNumber()
	if err := decoder.Decode(&doc); err != nil {
		lekInputFail()
	}
	results := make([]map[string]any, 0, len(doc.Vectors))
	for _, vector := range doc.Vectors {
		env := map[string]any{
			"clockText": vector.Clock,
			"bindings":  vector.Bindings,
		}
		results = append(results, lekRun(vector.Expression, vector.ID, env))
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
            "attachmentRevision": "1.0.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
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
