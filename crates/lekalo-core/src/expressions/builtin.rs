//! The closed built-in function registry with versioned semantics
//! (issue #66).
//!
//! Fifteen target-neutral built-ins: string length/concat/casing/
//! containment, integer absolute value, calendar accessors, duration
//! access, and the two closed casts. Every entry carries its
//! capability token (`expression.builtin/<name>`) so managed-mode
//! generation can block a target that does not declare support, and
//! the whole table shares [`BUILTIN_SEMANTICS_VERSION`][super::version::
//! BUILTIN_SEMANTICS_VERSION]: a behavioral change is a new version,
//! never a silent reinterpretation. Casing is ASCII-only by contract
//! so the Node, PHP, and Go renderings agree byte for byte.

use super::types::ScalarType;

/// One registered built-in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Builtin {
    /// The exact wire name (`string-length`).
    pub name: &'static str,
    /// The closed argument type list.
    pub args: &'static [ScalarType],
    /// The result type.
    pub result: ScalarType,
}

/// The closed built-in table, sorted by name.
pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "cast-int-to-string",
        args: &[ScalarType::Int],
        result: ScalarType::Str,
    },
    Builtin {
        name: "cast-string-to-int",
        args: &[ScalarType::Str],
        result: ScalarType::Int,
    },
    Builtin {
        name: "datetime-day",
        args: &[ScalarType::DateTime],
        result: ScalarType::Int,
    },
    Builtin {
        name: "datetime-month",
        args: &[ScalarType::DateTime],
        result: ScalarType::Int,
    },
    Builtin {
        name: "datetime-weekday",
        args: &[ScalarType::DateTime],
        result: ScalarType::Int,
    },
    Builtin {
        name: "datetime-year",
        args: &[ScalarType::DateTime],
        result: ScalarType::Int,
    },
    Builtin {
        name: "duration-seconds",
        args: &[ScalarType::Duration],
        result: ScalarType::Int,
    },
    Builtin {
        name: "int-abs",
        args: &[ScalarType::Int],
        result: ScalarType::Int,
    },
    Builtin {
        name: "string-concat",
        args: &[ScalarType::Str, ScalarType::Str],
        result: ScalarType::Str,
    },
    Builtin {
        name: "string-contains",
        args: &[ScalarType::Str, ScalarType::Str],
        result: ScalarType::Bool,
    },
    Builtin {
        name: "string-ends-with",
        args: &[ScalarType::Str, ScalarType::Str],
        result: ScalarType::Bool,
    },
    Builtin {
        name: "string-length",
        args: &[ScalarType::Str],
        result: ScalarType::Int,
    },
    Builtin {
        name: "string-lower",
        args: &[ScalarType::Str],
        result: ScalarType::Str,
    },
    Builtin {
        name: "string-starts-with",
        args: &[ScalarType::Str, ScalarType::Str],
        result: ScalarType::Bool,
    },
    Builtin {
        name: "string-upper",
        args: &[ScalarType::Str],
        result: ScalarType::Str,
    },
];

/// Look up one built-in by its exact wire name.
pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.name == name)
}

/// The capability token of one built-in
/// (`expression.builtin/string-length`).
pub fn capability(name: &str) -> String {
    format!("{}{}", super::version::BUILTIN_CAPABILITY_PREFIX, name)
}

/// The closed evaluation error vocabulary. These tokens are the wire
/// contract of the deterministic evaluator and of every generated
/// target program: an identical failure class renders identically in
/// Rust, Node, PHP, and Go.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EvalError {
    /// Integer arithmetic left the ±2^53−1 bound (or overflowed the
    /// host 64-bit range before the bound check).
    IntOverflow,
    /// Duration arithmetic left the duration bound.
    DurationOverflow,
    /// Datetime arithmetic left the year 0001–9999 range.
    DateTimeOverflow,
    /// Truncated division by zero.
    DivideByZero,
    /// Truncated remainder by zero.
    ModuloByZero,
    /// `cast-string-to-int` received a non-canonical decimal
    /// spelling.
    CastInvalid,
    /// `string-concat` left the literal bound.
    ConcatOverflow,
}

impl EvalError {
    /// The exact wire token.
    pub const fn key(self) -> &'static str {
        match self {
            Self::IntOverflow => "int-overflow",
            Self::DurationOverflow => "duration-overflow",
            Self::DateTimeOverflow => "datetime-overflow",
            Self::DivideByZero => "divide-by-zero",
            Self::ModuloByZero => "modulo-by-zero",
            Self::CastInvalid => "cast-invalid",
            Self::ConcatOverflow => "concat-overflow",
        }
    }

    /// Parse one wire token.
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "int-overflow" => Some(Self::IntOverflow),
            "duration-overflow" => Some(Self::DurationOverflow),
            "datetime-overflow" => Some(Self::DateTimeOverflow),
            "divide-by-zero" => Some(Self::DivideByZero),
            "modulo-by-zero" => Some(Self::ModuloByZero),
            "cast-invalid" => Some(Self::CastInvalid),
            "concat-overflow" => Some(Self::ConcatOverflow),
            _ => None,
        }
    }
}

/// Validate one canonical decimal integer spelling for
/// `cast-string-to-int`: optional `-`, one to sixteen ASCII digits,
/// no `+`, no leading zero unless the value is exactly `0`, and the
/// value inside the family integer bound.
pub(crate) fn parse_canonical_int(text: &str) -> Option<i64> {
    let body = text.strip_prefix('-').unwrap_or(text);
    if body.is_empty() || !body.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if body.len() > 1 && body.starts_with('0') {
        return None;
    }
    if body.len() > 16 {
        return None;
    }
    let value: i64 = text.parse().ok()?;
    super::types::int_in_range(value).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_closed_sorted_and_unique() {
        let mut names: Vec<&str> = BUILTINS.iter().map(|b| b.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        names.dedup();
        assert_eq!(names.len(), BUILTINS.len());
        assert_eq!(BUILTINS.len(), 15);
        for builtin in BUILTINS {
            assert_eq!(
                capability(builtin.name),
                format!("expression.builtin/{}", builtin.name)
            );
            assert!(lookup(builtin.name).is_some());
        }
        assert!(lookup("eval").is_none());
        assert!(lookup("system").is_none());
    }

    #[test]
    fn error_tokens_round_trip() {
        for error in [
            EvalError::IntOverflow,
            EvalError::DurationOverflow,
            EvalError::DateTimeOverflow,
            EvalError::DivideByZero,
            EvalError::ModuloByZero,
            EvalError::CastInvalid,
            EvalError::ConcatOverflow,
        ] {
            assert_eq!(EvalError::parse(error.key()), Some(error));
        }
        assert_eq!(EvalError::parse("shell-escape"), None);
    }

    #[test]
    fn canonical_int_casts_accept_only_canonical_spellings() {
        assert_eq!(parse_canonical_int("0"), Some(0));
        assert_eq!(parse_canonical_int("-12"), Some(-12));
        assert_eq!(
            parse_canonical_int("9007199254740991"),
            Some(super::super::version::MAX_INT),
        );
        assert_eq!(parse_canonical_int("9007199254740992"), None);
        assert_eq!(parse_canonical_int("01"), None);
        assert_eq!(parse_canonical_int("+1"), None);
        assert_eq!(parse_canonical_int("1.0"), None);
        assert_eq!(parse_canonical_int(""), None);
        assert_eq!(parse_canonical_int("-"), None);
        assert_eq!(parse_canonical_int("12345678901234567"), None);
    }
}
