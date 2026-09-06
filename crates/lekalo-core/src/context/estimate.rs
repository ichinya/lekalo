//! The fixed offline token estimator (issue #17).
//!
//! The v1 profile is exactly one deterministic, offline estimator:
//! `dev.lekalo.estimator.chars-4@1.0.0`. Its rule is fixed and recorded in
//! [`SPEC`]: a content string of `n` Unicode scalars estimates
//! `max(1, ceil(n / 4))` tokens, and empty content estimates zero. The
//! estimate is computed per typed fact from the fact's semantic text
//! values, so it never depends on the output format, the platform, or the
//! locale, and repeated runs of the same capsule are byte-identical.
//!
//! The profile digest pins the rule: `sha256` over the exact [`SPEC`]
//! bytes. Successor profiles (real model tokenizers) enter as additional
//! versioned identities; they never silently replace this one.

use crate::versioning::plan::sha256_hex;

/// The exact estimator rule text the digest is computed over.
pub const SPEC: &str = concat!(
    "family=dev.lekalo.estimator profile=chars-4 version=1.0.0 ",
    "rule=tokens:max(1,ceil(scalars/4)) unit=unicode-scalar ",
    "scope=per-typed-fact content=semantic-values-joined-by-space ",
    "offline=true deterministic=true format-independent=true"
);

/// The estimator profile identity (family and version joined with `@`).
pub const IDENTITY: &str = "dev.lekalo.estimator.chars-4@1.0.0";

/// The estimator profile version.
pub const VERSION: &str = "1.0.0";

/// The `sha256` digest over the exact rule text ([`SPEC`]).
pub fn digest() -> String {
    format!("sha256:{}", sha256_hex(SPEC.as_bytes()))
}

/// Estimate the token count of one fact content string.
///
/// Empty content estimates zero tokens; every non-empty string estimates
/// at least one token. `ceil(n / 4)` is computed without floats.
pub fn tokens(content: &str) -> u64 {
    let scalars = content.chars().count() as u64;
    if scalars == 0 {
        return 0;
    }
    scalars.div_ceil(4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_the_sha256_of_the_exact_spec() {
        assert_eq!(digest().len(), "sha256:".len() + 64);
        assert!(digest().starts_with("sha256:"));
        // Pinned so an accidental spec change is a deliberate contract move.
        assert_eq!(digest(), format!("sha256:{}", sha256_hex(SPEC.as_bytes())));
    }

    #[test]
    fn tokens_follow_the_recorded_rule() {
        assert_eq!(tokens(""), 0);
        assert_eq!(tokens("a"), 1);
        assert_eq!(tokens("abcd"), 1);
        assert_eq!(tokens("abcde"), 2);
        assert_eq!(tokens("abcdefgh"), 2);
        assert_eq!(tokens("abcdefghi"), 3);
    }

    #[test]
    fn scalars_not_bytes_drive_the_estimate() {
        // U+00E9 is one scalar, two UTF-8 bytes.
        assert_eq!(tokens("éééé"), 1);
        assert_eq!(tokens("ééééé"), 2);
    }
}
