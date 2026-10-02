//! The closed budget-profile estimator arithmetic (issue #75).
//!
//! v1 reuses exactly the accepted offline `chars-4` estimator
//! (`dev.lekalo.estimator.chars-4@0.2.16`): integer `max(1, ceil(scalars/4))`
//! per content string, zero for empty content, no floats and no
//! dependencies. The profile-scoped projection layers one checked
//! `ceil(value * marginNumerator / marginDenominator) + framingTokens`
//! step over it for the minimum-safe estimate; both steps stay in
//! `u64`/`i128` checked arithmetic and refuse overflow instead of
//! wrapping. Unsupported estimator identities refuse closed (no silent
//! fallback), and a named-model or calibrated tokenizer is only ever
//! recorded as unknown provenance in this generation.

use super::diagnostic;
use crate::context::estimate as legacy;
use crate::diagnostics::DiagnosticSet;

/// The accepted estimator identity (re-exported for report stamps).
pub const CHARS4_IDENTITY: &str = legacy::IDENTITY;
/// The accepted estimator version.
pub const CHARS4_VERSION: &str = legacy::VERSION;
/// The accepted estimator spec digest.
pub fn chars4_digest() -> String {
    legacy::digest()
}

/// Whether `identity` names an estimator this generation accepts.
pub fn accepted(identity: &str) -> bool {
    identity == legacy::IDENTITY
}

/// Estimate one fact content string under the pinned estimator.
pub fn tokens(content: &str) -> u64 {
    legacy::tokens(content)
}

/// The minimum-safe estimate: the profile margin and framing applied to
/// the deterministic required-token sum, with checked arithmetic.
pub fn minimum_safe_estimate(
    required_tokens: u64,
    margin_numerator: u64,
    margin_denominator: u64,
    framing_tokens: u64,
) -> Result<u64, DiagnosticSet> {
    if margin_denominator == 0 {
        return Err(diagnostic::input_invalid("profile-margin-denominator"));
    }
    // ceil(required × numerator / denominator), computed without floats:
    // ceil(a/b) = (a + b − 1) / b for positive a.
    let scaled = (i128::from(required_tokens) * i128::from(margin_numerator)
        + i128::from(margin_denominator)
        - 1)
    .checked_div(i128::from(margin_denominator))
    .ok_or_else(|| diagnostic::input_invalid("profile-margin-overflow"))?;
    let scaled = scaled.max(0);
    let total = scaled.saturating_add(i128::from(framing_tokens));
    u64::try_from(total).map_err(|_| diagnostic::input_invalid("profile-estimate-overflow"))
}

/// The available content budget: the window minus both reservations,
/// refused on nonpositive results or overflow.
pub fn available_content_tokens(
    context_window: u64,
    reserved_output: u64,
    reserved_system_tool: u64,
) -> Result<u64, DiagnosticSet> {
    let window = i128::from(context_window);
    let reserved = i128::from(reserved_output).saturating_add(i128::from(reserved_system_tool));
    let available = window
        .checked_sub(reserved)
        .ok_or_else(|| diagnostic::input_invalid("profile-budget-overflow"))?;
    if available <= 0 {
        return Err(diagnostic::input_invalid("profile-budget-nonpositive"));
    }
    u64::try_from(available).map_err(|_| diagnostic::input_invalid("profile-budget-overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chars4_identity_is_pinned() {
        assert_eq!(CHARS4_IDENTITY, "dev.lekalo.estimator.chars-4@0.2.16");
        assert!(accepted(CHARS4_IDENTITY));
        assert!(!accepted("dev.lekalo.estimator.gpt-4"));
    }

    #[test]
    fn minimum_safe_scales_and_frames() {
        assert_eq!(minimum_safe_estimate(100, 1, 1, 0).unwrap(), 100);
        // ceil(100 × 11/10) = 110, + 5 framing = 115.
        assert_eq!(minimum_safe_estimate(100, 11, 10, 5).unwrap(), 115);
        assert_eq!(minimum_safe_estimate(0, 2, 1, 7).unwrap(), 7);
        // ceil(3/100) = 1: the ceiling never rounds a positive sum to zero.
        assert_eq!(minimum_safe_estimate(3, 1, 100, 0).unwrap(), 1);
        // ceil(101 × 11/10) = ceil(111.1) = 112, + 0 = 112 (the M3 vector).
        assert_eq!(minimum_safe_estimate(101, 11, 10, 0).unwrap(), 112);
        // ceil(355 × 2/3) = 237, + 100 = 337 (the codex #7 vector).
        assert_eq!(minimum_safe_estimate(355, 2, 3, 100).unwrap(), 337);
        assert!(minimum_safe_estimate(u64::MAX, 2, 1, 0).is_err());
        assert!(minimum_safe_estimate(10, 1, 0, 0).is_err());
    }

    #[test]
    fn available_budget_refuses_nonpositive() {
        assert_eq!(
            available_content_tokens(16_384, 2_048, 2_336).unwrap(),
            12_000
        );
        assert!(available_content_tokens(1_000, 600, 500).is_err());
        assert!(available_content_tokens(u64::MAX, u64::MAX, u64::MAX).is_err());
    }

    #[test]
    fn scalar_tokens_follow_the_shared_rule() {
        assert_eq!(tokens(""), 0);
        assert_eq!(tokens("abcd"), 1);
        assert_eq!(tokens("abcde"), 2);
    }
}
