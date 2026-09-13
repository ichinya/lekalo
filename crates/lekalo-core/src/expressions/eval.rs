//! The deterministic reference evaluator (issue #66).
//!
//! Pure and total over validated attachments: the only ambient input
//! is the injected clock ([`Clock`]), which tests and callers
//! construct explicitly — no expression can read a wall clock, an
//! environment variable, a file, or a network. Every arithmetic
//! result is range-checked; every domain failure (division by zero,
//! overflow, bad cast) is one closed error token identical to the
//! tokens the generated Node/PHP/Go programs emit, so the shared
//! fixtures prove cross-target semantic equivalence. Evaluation
//! bindings are validated against the record's declared references
//! before anything runs: missing, unknown, mistyped, or non-canonical
//! bindings reject with `expression.binding-invalid`.

use std::collections::HashMap;

use serde_json::Value as Json;

use super::ast::{ExprNode, RefScope};
use super::builtin::{parse_canonical_int, EvalError};
use super::diagnostic;
use super::types::{
    civil_from_days, datetime_in_range, datetime_parse, datetime_render, duration_in_range,
    int_in_range, string_is_canonical, ExprType, Scalar, ScalarType, Value,
};
use super::version::MAX_LITERAL_BYTES;
use super::ExpressionRecord;

/// The injected deterministic clock. `now` evaluates to exactly this
/// instant; tests construct it from a canonical UTC spelling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Clock {
    seconds: i64,
}

impl Clock {
    /// Construct from a canonical UTC datetime spelling.
    pub fn from_datetime(text: &str) -> Option<Self> {
        datetime_parse(text).map(|seconds| Self { seconds })
    }

    /// The instant as seconds since the Unix epoch.
    pub const fn as_seconds(&self) -> i64 {
        self.seconds
    }

    /// Construct from epoch seconds (the wire vectors carry the
    /// parsed instant).
    pub const fn from_seconds(seconds: i64) -> Self {
        Self { seconds }
    }

    /// The canonical UTC spelling of the instant.
    pub fn as_datetime(&self) -> String {
        datetime_render(self.seconds)
    }
}

/// The validated evaluation bindings of one evaluation: every
/// declared reference resolved to a typed value (or `None` for a
/// nullable reference that is null).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Bindings {
    values: HashMap<(RefScope, String), Option<Value>>,
}

impl Bindings {
    /// The empty binding set (no declared references resolved).
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    /// Decode and validate the wire binding scopes against one
    /// record's declared references. The wire shape is four optional
    /// scope objects (`input`, `actor`, `entity`, `result`) whose
    /// members are typed values; a nullable reference accepts an
    /// explicit `null`. Unknown scopes and unknown fields inside a
    /// scope object are denied.
    pub fn from_json(
        record: &ExpressionRecord,
        json: &Json,
    ) -> Result<Self, crate::diagnostics::DiagnosticSet> {
        let reject = |detail: &str| -> crate::diagnostics::DiagnosticSet {
            diagnostic::binding_invalid(detail, Some(record.id()))
        };
        let object = json.as_object().ok_or_else(|| reject("bindings-shape"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "input" | "actor" | "entity" | "result") {
                return Err(reject("binding-unknown"));
            }
            if !object.get(key).map(Json::is_object).unwrap_or(false) {
                return Err(reject("bindings-shape"));
            }
        }
        for (key, scope_json) in object.iter() {
            let scope = RefScope::parse(key).ok_or_else(|| reject("bindings-shape"))?;
            let fields = scope_json
                .as_object()
                .ok_or_else(|| reject("bindings-shape"))?;
            for field in fields.keys() {
                if !record
                    .params()
                    .iter()
                    .any(|param| param.scope() == scope && param.field() == field)
                {
                    return Err(reject("binding-unknown"));
                }
            }
        }
        let mut bindings = Self::new();
        for param in record.params() {
            let field_json = object
                .get(param.scope().key())
                .and_then(|scope| scope.get(param.field()));
            let Some(field_json) = field_json else {
                return Err(reject("binding-missing"));
            };
            let value = match field_json {
                Json::Null if param.nullable() => None,
                Json::Null => return Err(reject("binding-null")),
                other => Some(json_binding_value(other, param.ty())?),
            };
            bindings
                .values
                .insert((param.scope(), param.field().to_owned()), value);
        }
        Ok(bindings)
    }

    /// The bound value of one reference.
    pub fn get(&self, scope: RefScope, field: &str) -> Option<&Option<Value>> {
        self.values.get(&(scope, field.to_owned()))
    }
}

/// Decode one typed binding value.
fn json_binding_value(
    json: &Json,
    ty: &ExprType,
) -> Result<Value, crate::diagnostics::DiagnosticSet> {
    let invalid = || diagnostic::binding_invalid("binding-value", None);
    let scalar = |json: &Json,
                  ty: ScalarType|
     -> Result<Scalar, crate::diagnostics::DiagnosticSet> {
        let parsed = match ty {
            ScalarType::Bool => json.as_bool().map(Scalar::Bool),
            ScalarType::Int => json.as_i64().filter(|v| int_in_range(*v)).map(Scalar::Int),
            ScalarType::Str => json.as_str().and_then(|text| {
                (text.len() <= MAX_LITERAL_BYTES && string_is_canonical(text))
                    .then(|| Scalar::Str(text.to_owned()))
            }),
            ScalarType::DateTime => json.as_str().and_then(datetime_parse).map(Scalar::DateTime),
            ScalarType::Duration => json
                .as_i64()
                .filter(|v| duration_in_range(*v))
                .map(Scalar::Duration),
        };
        parsed.ok_or_else(invalid)
    };
    match ty {
        ExprType::Scalar(inner) => Ok(Value::Scalar(scalar(json, *inner)?)),
        ExprType::Set(inner) => {
            let items = json.as_array().ok_or_else(invalid)?;
            if items.len() > super::version::MAX_SET_ITEMS {
                return Err(invalid());
            }
            let mut scalars = Vec::with_capacity(items.len());
            for item in items {
                scalars.push(scalar(item, *inner)?);
            }
            scalars.sort_by_key(|left| left.cmp_key());
            for pair in scalars.windows(2) {
                if pair[0] == pair[1] {
                    return Err(invalid());
                }
            }
            Ok(Value::Set(scalars))
        }
    }
}

/// Evaluate one record against validated bindings and the injected
/// clock. The result type always equals the declared result type of
/// the record (the static checker proved it).
pub fn evaluate(
    record: &ExpressionRecord,
    bindings: &Bindings,
    clock: &Clock,
) -> Result<Value, crate::diagnostics::DiagnosticSet> {
    match eval_node(&record.body, bindings, clock) {
        Ok(value) => Ok(value),
        Err(error) => Err(diagnostic::eval_invalid(error, Some(record.id()))),
    }
}

/// Evaluate one node; domain failures carry the closed token.
fn eval_node(node: &ExprNode, bindings: &Bindings, clock: &Clock) -> Result<Value, EvalError> {
    let scalar = |node: &ExprNode| -> Result<Scalar, EvalError> {
        match eval_node(node, bindings, clock)? {
            Value::Scalar(scalar) => Ok(scalar),
            Value::Set(_) => Err(EvalError::CastInvalid),
        }
    };
    match node {
        ExprNode::Bool(value) => Ok(Value::Scalar(Scalar::Bool(*value))),
        ExprNode::Int(value) => Ok(Value::Scalar(Scalar::Int(*value))),
        ExprNode::Str(value) => Ok(Value::Scalar(Scalar::Str(value.clone()))),
        ExprNode::DateTime(seconds) => Ok(Value::Scalar(Scalar::DateTime(*seconds))),
        ExprNode::Duration(seconds) => Ok(Value::Scalar(Scalar::Duration(*seconds))),
        ExprNode::Set(items) => Ok(Value::Set(items.clone())),
        ExprNode::Ref { scope, field } => match bindings.get(*scope, field) {
            Some(Some(value)) => Ok(value.clone()),
            // The static checker confines unguarded nullable
            // references to is-null/not-null, so a null here is
            // unreachable in a validated attachment.
            Some(None) | None => Err(EvalError::CastInvalid),
        },
        ExprNode::Now => Ok(Value::Scalar(Scalar::DateTime(clock.as_seconds()))),
        ExprNode::Equal { left, right } => {
            Ok(Value::Scalar(Scalar::Bool(scalar(left)? == scalar(right)?)))
        }
        ExprNode::NotEqual { left, right } => {
            Ok(Value::Scalar(Scalar::Bool(scalar(left)? != scalar(right)?)))
        }
        ExprNode::Less { left, right } => {
            Ok(Value::Scalar(Scalar::Bool(scalar(left)? < scalar(right)?)))
        }
        ExprNode::LessEqual { left, right } => {
            Ok(Value::Scalar(Scalar::Bool(scalar(left)? <= scalar(right)?)))
        }
        ExprNode::And(operands) => {
            // Short-circuit: identical to the generated Node/PHP/Go
            // `&&`/`||`, so error observation matches everywhere.
            for operand in operands {
                let Scalar::Bool(value) = scalar(operand)? else {
                    return Err(EvalError::CastInvalid);
                };
                if !value {
                    return Ok(Value::Scalar(Scalar::Bool(false)));
                }
            }
            Ok(Value::Scalar(Scalar::Bool(true)))
        }
        ExprNode::Or(operands) => {
            for operand in operands {
                let Scalar::Bool(value) = scalar(operand)? else {
                    return Err(EvalError::CastInvalid);
                };
                if value {
                    return Ok(Value::Scalar(Scalar::Bool(true)));
                }
            }
            Ok(Value::Scalar(Scalar::Bool(false)))
        }
        ExprNode::Greater { left, right } => {
            Ok(Value::Scalar(Scalar::Bool(scalar(left)? > scalar(right)?)))
        }
        ExprNode::GreaterEqual { left, right } => {
            Ok(Value::Scalar(Scalar::Bool(scalar(left)? >= scalar(right)?)))
        }
        ExprNode::IsNull { operand } => {
            let ExprNode::Ref { scope, field } = operand.as_ref() else {
                return Err(EvalError::CastInvalid);
            };
            Ok(Value::Scalar(Scalar::Bool(
                bindings
                    .get(*scope, field)
                    .map(|value| value.is_none())
                    .unwrap_or(true),
            )))
        }
        ExprNode::NotNull { operand } => {
            let ExprNode::Ref { scope, field } = operand.as_ref() else {
                return Err(EvalError::CastInvalid);
            };
            Ok(Value::Scalar(Scalar::Bool(
                bindings
                    .get(*scope, field)
                    .map(|value| value.is_some())
                    .unwrap_or(false),
            )))
        }
        ExprNode::InSet { operand, set } => {
            let needle = scalar(operand)?;
            let Value::Set(items) = eval_node(set, bindings, clock)? else {
                return Err(EvalError::CastInvalid);
            };
            Ok(Value::Scalar(Scalar::Bool(items.contains(&needle))))
        }
        ExprNode::NotInSet { operand, set } => {
            let needle = scalar(operand)?;
            let Value::Set(items) = eval_node(set, bindings, clock)? else {
                return Err(EvalError::CastInvalid);
            };
            Ok(Value::Scalar(Scalar::Bool(!items.contains(&needle))))
        }
        ExprNode::Not { operand } => match scalar(operand)? {
            Scalar::Bool(value) => Ok(Value::Scalar(Scalar::Bool(!value))),
            _ => Err(EvalError::CastInvalid),
        },
        ExprNode::Add { left, right } => {
            let left_scalar = scalar(left)?;
            let right_scalar = scalar(right)?;
            match (left_scalar, right_scalar) {
                (Scalar::Int(a), Scalar::Int(b)) => {
                    let value = a.checked_add(b).ok_or(EvalError::IntOverflow)?;
                    if !int_in_range(value) {
                        return Err(EvalError::IntOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Int(value)))
                }
                (Scalar::Duration(a), Scalar::Duration(b)) => {
                    let value = a.checked_add(b).ok_or(EvalError::DurationOverflow)?;
                    if !duration_in_range(value) {
                        return Err(EvalError::DurationOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Duration(value)))
                }
                (Scalar::DateTime(a), Scalar::Duration(b)) => datetime_shift(a, b),
                (Scalar::Duration(a), Scalar::DateTime(b)) => datetime_shift(b, a),
                _ => Err(EvalError::CastInvalid),
            }
        }
        ExprNode::Subtract { left, right } => {
            let left_scalar = scalar(left)?;
            let right_scalar = scalar(right)?;
            match (left_scalar, right_scalar) {
                (Scalar::Int(a), Scalar::Int(b)) => {
                    let value = a.checked_sub(b).ok_or(EvalError::IntOverflow)?;
                    if !int_in_range(value) {
                        return Err(EvalError::IntOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Int(value)))
                }
                (Scalar::Duration(a), Scalar::Duration(b)) => {
                    let value = a.checked_sub(b).ok_or(EvalError::DurationOverflow)?;
                    if !duration_in_range(value) {
                        return Err(EvalError::DurationOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Duration(value)))
                }
                (Scalar::DateTime(a), Scalar::Duration(b)) => datetime_shift(a, -b),
                (Scalar::DateTime(a), Scalar::DateTime(b)) => {
                    let value = a.checked_sub(b).ok_or(EvalError::DurationOverflow)?;
                    if !duration_in_range(value) {
                        return Err(EvalError::DurationOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Duration(value)))
                }
                _ => Err(EvalError::CastInvalid),
            }
        }
        ExprNode::Multiply { left, right } => {
            let left_scalar = scalar(left)?;
            let right_scalar = scalar(right)?;
            match (left_scalar, right_scalar) {
                (Scalar::Int(a), Scalar::Int(b)) => {
                    let value = a.checked_mul(b).ok_or(EvalError::IntOverflow)?;
                    if !int_in_range(value) {
                        return Err(EvalError::IntOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Int(value)))
                }
                (Scalar::Duration(a), Scalar::Int(b)) => {
                    let value = a.checked_mul(b).ok_or(EvalError::DurationOverflow)?;
                    if !duration_in_range(value) {
                        return Err(EvalError::DurationOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Duration(value)))
                }
                (Scalar::Int(a), Scalar::Duration(b)) => {
                    let value = a.checked_mul(b).ok_or(EvalError::DurationOverflow)?;
                    if !duration_in_range(value) {
                        return Err(EvalError::DurationOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Duration(value)))
                }
                _ => Err(EvalError::CastInvalid),
            }
        }
        ExprNode::Divide { left, right } => {
            let left_scalar = scalar(left)?;
            let right_scalar = scalar(right)?;
            match (left_scalar, right_scalar) {
                (_, Scalar::Int(0)) => Err(EvalError::DivideByZero),
                (Scalar::Int(a), Scalar::Int(b)) => {
                    let value = checked_trunc_div(a, b)?;
                    if !int_in_range(value) {
                        return Err(EvalError::IntOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Int(value)))
                }
                (Scalar::Duration(a), Scalar::Int(b)) => {
                    let value = checked_trunc_div(a, b)?;
                    if !duration_in_range(value) {
                        return Err(EvalError::DurationOverflow);
                    }
                    Ok(Value::Scalar(Scalar::Duration(value)))
                }
                _ => Err(EvalError::CastInvalid),
            }
        }
        ExprNode::Modulo { left, right } => {
            let Scalar::Int(a) = scalar(left)? else {
                return Err(EvalError::CastInvalid);
            };
            let Scalar::Int(b) = scalar(right)? else {
                return Err(EvalError::CastInvalid);
            };
            if b == 0 {
                return Err(EvalError::ModuloByZero);
            }
            let value = a.checked_rem(b).ok_or(EvalError::IntOverflow)?;
            if !int_in_range(value) {
                return Err(EvalError::IntOverflow);
            }
            Ok(Value::Scalar(Scalar::Int(value)))
        }
        ExprNode::If {
            condition,
            then,
            otherwise,
        } => {
            let Scalar::Bool(holds) = scalar(condition)? else {
                return Err(EvalError::CastInvalid);
            };
            eval_node(if holds { then } else { otherwise }, bindings, clock)
        }
        ExprNode::Builtin { name, args } => eval_builtin(name, args, &scalar, bindings, clock),
    }
}

/// Truncated division identical in Rust, Node, PHP, and Go: the
/// quotient rounds toward zero and `MIN / -1` rejects (the Rust
/// `i64` overflow the other targets cannot express).
fn checked_trunc_div(a: i64, b: i64) -> Result<i64, EvalError> {
    if a == i64::MIN && b == -1 {
        return Err(EvalError::IntOverflow);
    }
    Ok(a / b)
}

/// Shift one datetime by duration seconds with the year bound.
fn datetime_shift(seconds: i64, by: i64) -> Result<Value, EvalError> {
    let value = seconds.checked_add(by).ok_or(EvalError::DateTimeOverflow)?;
    if !datetime_in_range(value) {
        return Err(EvalError::DateTimeOverflow);
    }
    Ok(Value::Scalar(Scalar::DateTime(value)))
}

/// Evaluate one closed built-in call.
fn eval_builtin(
    name: &str,
    args: &[ExprNode],
    scalar: &dyn Fn(&ExprNode) -> Result<Scalar, EvalError>,
    bindings: &Bindings,
    clock: &Clock,
) -> Result<Value, EvalError> {
    let as_str = |index: usize| -> Result<String, EvalError> {
        match scalar(&args[index])? {
            Scalar::Str(text) => Ok(text),
            _ => Err(EvalError::CastInvalid),
        }
    };
    let as_int = |index: usize| -> Result<i64, EvalError> {
        match scalar(&args[index])? {
            Scalar::Int(value) => Ok(value),
            _ => Err(EvalError::CastInvalid),
        }
    };
    let as_datetime = |index: usize| -> Result<i64, EvalError> {
        match scalar(&args[index])? {
            Scalar::DateTime(seconds) => Ok(seconds),
            _ => Err(EvalError::CastInvalid),
        }
    };
    let as_duration = |index: usize| -> Result<i64, EvalError> {
        match scalar(&args[index])? {
            Scalar::Duration(seconds) => Ok(seconds),
            _ => Err(EvalError::CastInvalid),
        }
    };
    match name {
        "string-length" => Ok(Value::Scalar(
            Scalar::Int(as_str(0)?.chars().count() as i64),
        )),
        "string-concat" => {
            let joined = format!("{}{}", as_str(0)?, as_str(1)?);
            if joined.len() > MAX_LITERAL_BYTES || !string_is_canonical(&joined) {
                return Err(EvalError::ConcatOverflow);
            }
            Ok(Value::Scalar(Scalar::Str(joined)))
        }
        "string-lower" => Ok(Value::Scalar(Scalar::Str(ascii_lower(&as_str(0)?)))),
        "string-upper" => Ok(Value::Scalar(Scalar::Str(ascii_upper(&as_str(0)?)))),
        "string-contains" => Ok(Value::Scalar(Scalar::Bool(
            as_str(0)?.contains(&as_str(1)?),
        ))),
        "string-starts-with" => Ok(Value::Scalar(Scalar::Bool(
            as_str(0)?.starts_with(&as_str(1)?),
        ))),
        "string-ends-with" => Ok(Value::Scalar(Scalar::Bool(
            as_str(0)?.ends_with(&as_str(1)?),
        ))),
        "int-abs" => {
            let value = as_int(0)?.checked_abs().ok_or(EvalError::IntOverflow)?;
            if !int_in_range(value) {
                return Err(EvalError::IntOverflow);
            }
            Ok(Value::Scalar(Scalar::Int(value)))
        }
        "datetime-year" => {
            let (year, _, _) = civil_from_days(as_datetime(0)?.div_euclid(86_400));
            Ok(Value::Scalar(Scalar::Int(year)))
        }
        "datetime-month" => {
            let (_, month, _) = civil_from_days(as_datetime(0)?.div_euclid(86_400));
            Ok(Value::Scalar(Scalar::Int(month)))
        }
        "datetime-day" => {
            let (_, _, day) = civil_from_days(as_datetime(0)?.div_euclid(86_400));
            Ok(Value::Scalar(Scalar::Int(day)))
        }
        "datetime-weekday" => {
            // 1970-01-01 was a Thursday (4); ISO weekday 1=Monday..7=Sunday.
            let days = as_datetime(0)?.div_euclid(86_400);
            let weekday = (days + 3).rem_euclid(7) + 1;
            Ok(Value::Scalar(Scalar::Int(weekday)))
        }
        "duration-seconds" => Ok(Value::Scalar(Scalar::Int(as_duration(0)?))),
        "cast-int-to-string" => Ok(Value::Scalar(Scalar::Str(as_int(0)?.to_string()))),
        "cast-string-to-int" => {
            let value = parse_canonical_int(&as_str(0)?).ok_or(EvalError::CastInvalid)?;
            Ok(Value::Scalar(Scalar::Int(value)))
        }
        _ => {
            // Unreachable: the static checker rejects unknown names.
            let _ = (bindings, clock);
            Err(EvalError::CastInvalid)
        }
    }
}

/// ASCII-only lowercasing (the versioned v1 casing contract).
fn ascii_lower(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect()
}

/// ASCII-only uppercasing (the versioned v1 casing contract).
fn ascii_upper(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressions::{ExpressionKind, ParamDecl};
    use serde_json::json;

    /// One condition record with the given native body.
    fn condition(body: ExprNode) -> ExpressionRecord {
        ExpressionRecord {
            id: "expr.planner/test".to_owned(),
            kind: ExpressionKind::Condition,
            description: None,
            params: Vec::new(),
            result: ExprType::Scalar(ScalarType::Bool),
            target: None,
            body,
            span: None,
        }
    }

    /// One integer-result record with the given native body.
    fn int_body(body: ExprNode) -> ExpressionRecord {
        ExpressionRecord {
            id: "expr.planner/test".to_owned(),
            kind: ExpressionKind::Condition,
            description: None,
            params: Vec::new(),
            result: ExprType::Scalar(ScalarType::Int),
            target: None,
            body,
            span: None,
        }
    }

    #[test]
    fn the_clock_is_injected_and_deterministic() {
        let clock = Clock::from_datetime("2026-09-12T00:00:00Z").expect("clock");
        assert_eq!(clock.as_datetime(), "2026-09-12T00:00:00Z");
        let record = condition(ExprNode::Less {
            left: Box::new(ExprNode::DateTime(types_test_datetime(
                "2026-09-11T23:59:59Z",
            ))),
            right: Box::new(ExprNode::Now),
        });
        let bindings = Bindings::new();
        let outcome = evaluate(&record, &bindings, &clock).expect("evaluates");
        assert_eq!(outcome.to_json(), json!(true));
        // A different injected instant flips the verdict; nothing
        // else changed.
        let earlier = Clock::from_datetime("2020-01-01T00:00:00Z").expect("clock");
        let outcome = evaluate(&record, &bindings, &earlier).expect("evaluates");
        assert_eq!(outcome.to_json(), json!(false));
    }

    fn types_test_datetime(text: &str) -> i64 {
        datetime_parse(text).expect("canonical datetime")
    }

    #[test]
    fn arithmetic_is_checked_against_the_bounds() {
        let record = int_body(ExprNode::Multiply {
            left: Box::new(ExprNode::Int(3_037_000_499)),
            right: Box::new(ExprNode::Int(3_037_000_499)),
        });
        let bindings = Bindings::new();
        let clock = Clock::from_datetime("2026-09-12T00:00:00Z").expect("clock");
        let rejection = evaluate(&record, &bindings, &clock).expect_err("overflows");
        assert!(rejection
            .as_slice()
            .iter()
            .any(|d| d.id() == "expression.eval-invalid"));
    }

    #[test]
    fn division_and_cast_domain_errors_carry_closed_tokens() {
        let clock = Clock::from_datetime("2026-09-12T00:00:00Z").expect("clock");
        let bindings = Bindings::new();
        let div = int_body(ExprNode::Divide {
            left: Box::new(ExprNode::Int(1)),
            right: Box::new(ExprNode::Int(0)),
        });
        let rejection = evaluate(&div, &bindings, &clock).expect_err("divide");
        assert!(rejection
            .as_slice()
            .iter()
            .any(|d| d.id() == "expression.eval-invalid"
                && format!("{:?}", d.data()).contains("divide-by-zero")));
        let cast = int_body(ExprNode::Builtin {
            name: "cast-string-to-int".to_owned(),
            args: vec![ExprNode::Str(" 42".to_owned())],
        });
        assert!(evaluate(&cast, &bindings, &clock).is_err());
    }

    #[test]
    fn truncated_division_matches_the_cross_target_contract() {
        let clock = Clock::from_datetime("2026-09-12T00:00:00Z").expect("clock");
        let bindings = Bindings::new();
        for (a, b, quotient, remainder) in [
            (7i64, 3i64, 2i64, 1i64),
            (-7, 3, -2, -1),
            (7, -3, -2, 1),
            (-7, -3, 2, -1),
        ] {
            let div = int_body(ExprNode::Divide {
                left: Box::new(ExprNode::Int(a)),
                right: Box::new(ExprNode::Int(b)),
            });
            let outcome = evaluate(&div, &bindings, &clock).expect("divides");
            assert_eq!(outcome.to_json(), json!(quotient));
            let modulo = int_body(ExprNode::Modulo {
                left: Box::new(ExprNode::Int(a)),
                right: Box::new(ExprNode::Int(b)),
            });
            let outcome = evaluate(&modulo, &bindings, &clock).expect("divides");
            assert_eq!(outcome.to_json(), json!(remainder));
        }
    }

    #[test]
    fn bindings_validate_against_the_declared_references() {
        let record = ExpressionRecord {
            id: "expr.planner/test".to_owned(),
            kind: ExpressionKind::Condition,
            description: None,
            params: vec![ParamDecl {
                scope: RefScope::Input,
                field: "estimate".to_owned(),
                ty: ExprType::Scalar(ScalarType::Int),
                nullable: false,
            }],
            result: ExprType::Scalar(ScalarType::Bool),
            target: None,
            body: ExprNode::GreaterEqual {
                left: Box::new(ExprNode::Ref {
                    scope: RefScope::Input,
                    field: "estimate".to_owned(),
                }),
                right: Box::new(ExprNode::Int(1)),
            },
            span: None,
        };
        let good = serde_json::json!({"input": {"estimate": 5}});
        let bindings = Bindings::from_json(&record, &good).expect("bindings");
        let clock = Clock::from_datetime("2026-09-12T00:00:00Z").expect("clock");
        let outcome = evaluate(&record, &bindings, &clock).expect("evaluates");
        assert_eq!(outcome.to_json(), json!(true));
        let missing = serde_json::json!({"input": {}});
        assert!(Bindings::from_json(&record, &missing).is_err());
        let mistyped = serde_json::json!({"input": {"estimate": "five"}});
        assert!(Bindings::from_json(&record, &mistyped).is_err());
        let unknown = serde_json::json!({"input": {"estimate": 5, "extra": 1}});
        assert!(Bindings::from_json(&record, &unknown).is_err());
        let nulled = serde_json::json!({"input": {"estimate": null}});
        assert!(Bindings::from_json(&record, &nulled).is_err());
    }
}
