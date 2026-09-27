//! Conservative retry authorization over the #62 error contracts
//! (issue #72).
//!
//! The default is **no automatic retry**: a positive retry budget can
//! never authorize a retry by itself. One recognized declared error
//! may retry exactly when its own `RetryPolicy` and
//! `Idempotency`/`EffectClass` authorize it, preserving the
//! `check_retry_consistency` semantics of #62. An unknown or
//! infrastructure-shaped failure always yields one attempt: there is
//! no operation-level declaration proving retry safety for unknown
//! execution state, and HTTP 429/503 alone is insufficient.
//!
//! `conditional(reconciliation)` never retries automatically: the
//! caller reconciles. A transport disconnect after send is the
//! unknown state: one attempt. Attempt budgets are bounded and
//! validated before the first transport call.

use crate::diagnostics::DiagnosticSet;
use crate::error_contract::types::{EffectClass, Idempotency, RetryCondition, RetryPolicy};

use super::version::MAX_ATTEMPTS;

/// The closed authorization decision for one declared error.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RetryAuthorization {
    /// Never retry this error.
    Never,
    /// Retry is safe under the declared contract.
    Safe,
    /// Retry only with a declared idempotency key, reused byte for
    /// byte across attempts.
    KeyRequired,
    /// No automatic retry; the caller must reconcile first.
    ReconciliationOnly,
}

impl RetryAuthorization {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Safe => "safe",
            Self::KeyRequired => "key-required",
            Self::ReconciliationOnly => "reconciliation-only",
        }
    }

    /// Whether one automatic retry attempt is authorized when the
    /// caller supplied a nonempty idempotency key.
    pub const fn authorizes_with_key(self) -> bool {
        matches!(self, Self::Safe | Self::KeyRequired)
    }
}

/// Derive the conservative authorization of one declared error from
/// its #62 contract. Pure and total.
pub fn authorize(
    retry: RetryPolicy,
    idempotency: Idempotency,
    effect: EffectClass,
) -> RetryAuthorization {
    match retry {
        RetryPolicy::Never => RetryAuthorization::Never,
        RetryPolicy::Safe => {
            // The #62 constructor already refuses `safe` writes without
            // `guaranteed`; the derivation re-checks anyway so a future
            // registry source cannot smuggle an unsafe combination.
            match effect {
                EffectClass::None | EffectClass::Read => RetryAuthorization::Safe,
                EffectClass::Write | EffectClass::Destructive | EffectClass::External
                    if idempotency == Idempotency::Guaranteed =>
                {
                    RetryAuthorization::Safe
                }
                _ => RetryAuthorization::Never,
            }
        }
        RetryPolicy::Conditional(condition) => match condition {
            RetryCondition::IdempotencyKey => match idempotency {
                Idempotency::KeyRequired | Idempotency::Guaranteed => {
                    RetryAuthorization::KeyRequired
                }
                _ => RetryAuthorization::Never,
            },
            RetryCondition::Reconciliation => RetryAuthorization::ReconciliationOnly,
        },
    }
}

/// The bounded attempt plan of one operation call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttemptPlan {
    /// The total attempt bound: one first call plus declared retries.
    pub attempts: u32,
    /// Whether an idempotency key is required for any retry to run.
    pub key_required_for_retry: bool,
}

impl AttemptPlan {
    /// The one-attempt plan (the default and the unknown-failure
    /// plan).
    pub const fn single() -> Self {
        Self {
            attempts: 1,
            key_required_for_retry: false,
        }
    }
}

/// Plan the attempt bound of one call: at most one automatic retry
/// generation is derived from the operation's error union, and the
/// bound never exceeds [`MAX_ATTEMPTS`]. A requested budget of zero
/// or beyond the bound is a refusal, not a silent clamp.
///
/// The v1 plan is deliberately narrow: retries are permitted only per
/// declared error with a reusable key, so the plan bound is `1` when
/// no declared error authorizes a retry and `1 + retries` otherwise.
pub fn plan_attempts(
    authorizations: &[RetryAuthorization],
    requested_retries: u32,
) -> Result<AttemptPlan, DiagnosticSet> {
    if requested_retries == 0 || requested_retries >= MAX_ATTEMPTS {
        return Err(super::diagnostic::rule_invalid(
            super::diagnostic::RETRY_UNSAFE,
            "attempt-bound",
            None,
        ));
    }
    let key_required = authorizations.contains(&RetryAuthorization::KeyRequired);
    let any_safe = authorizations
        .iter()
        .any(|authorization| authorization.authorizes_with_key());
    Ok(AttemptPlan {
        attempts: requested_retries + 1,
        key_required_for_retry: key_required && any_safe,
    })
}

/// Whether one specific retry attempt is authorized: the previous
/// failure must carry a declared authorization that permits it, and a
/// key-required authorization demands a nonempty key reused across
/// attempts.
pub fn retry_permitted(
    authorization: Option<RetryAuthorization>,
    key: Option<&str>,
    attempt: u32,
    plan: AttemptPlan,
) -> bool {
    if attempt >= plan.attempts {
        return false;
    }
    let Some(authorization) = authorization else {
        return false;
    };
    if !authorization.authorizes_with_key() {
        return false;
    }
    if plan.key_required_for_retry {
        matches!(key, Some(k) if !k.is_empty())
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_and_conditional_reconciliation_never_retry() {
        assert_eq!(
            authorize(
                RetryPolicy::Never,
                Idempotency::NotApplicable,
                EffectClass::Read
            ),
            RetryAuthorization::Never
        );
        assert_eq!(
            authorize(
                RetryPolicy::Conditional(RetryCondition::Reconciliation),
                Idempotency::KeyRequired,
                EffectClass::Write
            ),
            RetryAuthorization::ReconciliationOnly
        );
        assert!(!RetryAuthorization::ReconciliationOnly.authorizes_with_key());
    }

    #[test]
    fn safe_reads_retry_and_unsafe_writes_do_not() {
        assert_eq!(
            authorize(
                RetryPolicy::Safe,
                Idempotency::NotApplicable,
                EffectClass::Read
            ),
            RetryAuthorization::Safe
        );
        // A write without a guaranteed idempotency contract can never
        // retry automatically — even if a budget was requested.
        assert_eq!(
            authorize(
                RetryPolicy::Safe,
                Idempotency::NotGuaranteed,
                EffectClass::Write
            ),
            RetryAuthorization::Never
        );
        assert_eq!(
            authorize(
                RetryPolicy::Safe,
                Idempotency::Guaranteed,
                EffectClass::Write
            ),
            RetryAuthorization::Safe
        );
    }

    #[test]
    fn key_condition_requires_a_reused_key() {
        assert_eq!(
            authorize(
                RetryPolicy::Conditional(RetryCondition::IdempotencyKey),
                Idempotency::KeyRequired,
                EffectClass::External
            ),
            RetryAuthorization::KeyRequired
        );
        assert!(RetryAuthorization::KeyRequired.authorizes_with_key());
    }

    #[test]
    fn attempt_plans_are_bounded_and_refuse_zero() {
        let authorizations = [RetryAuthorization::Safe, RetryAuthorization::Never];
        let plan = plan_attempts(&authorizations, 1).expect("plan");
        assert_eq!(plan.attempts, 2);
        assert!(!plan.key_required_for_retry);
        // Zero retries and over-bound budgets refuse, never clamp.
        assert!(plan_attempts(&authorizations, 0).is_err());
        assert!(plan_attempts(&authorizations, MAX_ATTEMPTS).is_err());
        // Key-required errors flip the plan's key demand.
        let keyed = [RetryAuthorization::Never, RetryAuthorization::KeyRequired];
        let keyed_plan = plan_attempts(&keyed, 1).expect("plan");
        assert!(keyed_plan.key_required_for_retry);
    }

    #[test]
    fn retry_permission_checks_key_and_attempt_bound() {
        let keyed = [RetryAuthorization::Never, RetryAuthorization::KeyRequired];
        let plan = plan_attempts(&keyed, 1).expect("plan");
        // A declared key-required error with a key: permitted once.
        assert!(retry_permitted(
            Some(RetryAuthorization::KeyRequired),
            Some("key-1"),
            1,
            plan
        ));
        // Without a key: refused.
        assert!(!retry_permitted(
            Some(RetryAuthorization::KeyRequired),
            None,
            1,
            plan
        ));
        assert!(!retry_permitted(
            Some(RetryAuthorization::KeyRequired),
            Some(""),
            1,
            plan
        ));
        // Unknown failure (the disconnect-after-send shape): refused.
        assert!(!retry_permitted(None, Some("key-1"), 1, plan));
        // Past the bound: refused.
        assert!(!retry_permitted(
            Some(RetryAuthorization::KeyRequired),
            Some("key-1"),
            plan.attempts,
            plan
        ));
    }
}
