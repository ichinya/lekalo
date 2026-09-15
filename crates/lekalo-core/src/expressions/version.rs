//! Issue #66 expression contract identity and hard denial limits.
//!
//! The expression contract is its own family: independent of the
//! product release, of the source Model and IR versions, of the
//! Scenario IR, of the invariant-transition, query-model, and
//! authorization families, and of the diagnostic registry. The
//! built-in function semantics carry their own version so a later
//! behavioral change is an explicit migration, never a silent
//! reinterpretation of accepted attachments. The limits below are the
//! v1 constants; every normalization rejects with an explicit
//! registered diagnostic instead of truncating by arrival order.

/// The expression contract family identifier.
pub const FAMILY: &str = "dev.lekalo.expressions";

/// The exact expression contract version.
pub const VERSION: &str = "0.2.16";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.expressions@0.2.16";
/// The exact IR identity this attachment binds (`irRef`).
pub const IR_IDENTITY: &str = "dev.lekalo.ir@0.2.16";

/// The exact wire discriminator of the expression contract.
pub const SCHEMA_VERSION: &str = "lekalo/expressions/v0.2.16";

/// The exact wire discriminator of the shared evaluation-vector
/// document (the cross-target fixture format).
pub const VECTORS_SCHEMA_VERSION: &str = "lekalo/expressions/vectors/v0.2.16";

/// The exact wire discriminator of a built-in capability snapshot
/// (the managed-mode support declaration a target or adapter supplies).
pub const SUPPORT_SCHEMA_VERSION: &str = "lekalo/expressions/builtin-support/v0.2.16";

/// The versioned semantics of every built-in function. Bumping this
/// is a behavioral contract change: accepted attachments pin the
/// version they were authored against.
pub const BUILTIN_SEMANTICS_VERSION: &str = "0.2.16";

/// The capability token of the closed core grammar (everything
/// outside the built-in registry).
pub const CORE_CAPABILITY: &str = "expression.core";

/// The capability token prefix of every built-in function.
pub const BUILTIN_CAPABILITY_PREFIX: &str = "expression.builtin/";

/// The maximum number of expression records one attachment carries.
pub const MAX_EXPRESSIONS: usize = 10_000;

/// The maximum number of typed references one expression declares.
pub const MAX_PARAMS: usize = 64;

/// The maximum depth of one expression AST.
pub const MAX_DEPTH: usize = 12;

/// The maximum node count of one expression AST.
pub const MAX_NODES: usize = 256;

/// The maximum operand count of one boolean combinator.
pub const MAX_OPERANDS: usize = 16;

/// The maximum member count of one set literal.
pub const MAX_SET_ITEMS: usize = 64;

/// The maximum size in bytes of one string literal.
pub const MAX_LITERAL_BYTES: usize = 256;

/// The maximum length of one expression description.
pub const MAX_DESCRIPTION_BYTES: usize = 256;

/// The maximum size in bytes of one source-map file token.
pub const MAX_SPAN_PATH_BYTES: usize = 128;

/// The maximum line or column of one source span.
pub const MAX_SPAN_POSITION: usize = 1_000_000;

/// The inclusive bound of every integer literal and runtime value
/// (`2^53 - 1`): exact in Rust `i64`, in PHP/Go 64-bit integers, and
/// in the IEEE-754 doubles a JSON reader may interpose, so the
/// cross-target results are identical by construction.
pub const MAX_INT: i64 = 9_007_199_254_740_991;

/// The inclusive bound of one duration literal or runtime value in
/// seconds (one thousand years).
pub const MAX_DURATION_SECONDS: i64 = 31_536_000_000;

/// The maximum number of vectors one evaluation-vector document
/// carries.
pub const MAX_VECTORS: usize = 10_000;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 32 * 1024 * 1024;

/// Compile-time bound invariants: the sum of the terms below must
/// be exactly the checked length. A violated invariant shrinks the
/// sum and the length check fails to compile — the bounds cannot
/// drift silently.
const _: [(); 7] = [(); (MAX_DEPTH >= 2 && MAX_NODES >= MAX_DEPTH) as usize
    + (MAX_OPERANDS >= 2) as usize
    + (MAX_SET_ITEMS >= 2) as usize
    + (MAX_INT == (1i64 << 53) - 1) as usize
    + (MAX_DURATION_SECONDS <= MAX_INT) as usize
    + (MAX_EXPRESSIONS >= 1 && MAX_PARAMS >= 1) as usize
    + (MAX_VECTORS >= 1) as usize];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constants_are_self_consistent() {
        assert_eq!(IDENTITY, format!("{}@{}", FAMILY, VERSION));
        assert!(SCHEMA_VERSION.starts_with("lekalo/expressions/v"));
        assert_eq!(BUILTIN_CAPABILITY_PREFIX, "expression.builtin/");
        assert!(CORE_CAPABILITY.starts_with("expression."));
    }
}
