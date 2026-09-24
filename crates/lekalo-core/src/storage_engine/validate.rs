//! Semantic validation of the storage-engine attachment (issue #69).
//!
//! The closed semantic rules over one fully parsed profile: RLS
//! coherence (an RLS enforcement declares its session policy), the
//! declared-model honesty of the optional members (a claimed
//! introspection or lifecycle member is the only way the capability
//! exists), and lifecycle coherence between isolation, provision, and
//! cleanup. Every violation is one registered diagnostic with no
//! partial result. Pure and read-only.

use super::diagnostic;
use super::{Cleanup, Enforcement, Isolation, Provision, StorageEngineAttachment};
use crate::diagnostics::DiagnosticSet;

/// The semantic self-check over one assembled attachment.
pub(crate) fn semantic_self_check(
    attachment: &StorageEngineAttachment,
) -> Result<(), DiagnosticSet> {
    check_version(attachment)?;
    check_tenancy(attachment)?;
    check_lifecycle(attachment)?;
    Ok(())
}

/// The pinned engine version must resolve to one owner-published
/// matrix row; an outside pin is unsupported, never clamped.
fn check_version(attachment: &StorageEngineAttachment) -> Result<(), DiagnosticSet> {
    if attachment.engine() == super::Engine::Postgres
        && super::postgres::version_matrix::row_for(attachment.engine_version().major()).is_none()
    {
        return Err(diagnostic::rule_invalid(
            diagnostic::VERSION_UNSUPPORTED,
            "major-unpublished",
            Some(attachment.engine_version().as_str()),
        ));
    }
    Ok(())
}

/// Tenancy rules: an RLS enforcement carries its RLS policy, and an
/// RLS policy exists only under RLS enforcement.
fn check_tenancy(attachment: &StorageEngineAttachment) -> Result<(), DiagnosticSet> {
    let Some(tenancy) = attachment.tenancy() else {
        return Ok(());
    };
    match tenancy.enforcement() {
        Enforcement::Rls => {
            if tenancy.rls().is_none() {
                return Err(diagnostic::rule_invalid(
                    diagnostic::PROFILE_INVALID,
                    "rls-policy-missing",
                    None,
                ));
            }
        }
        _ => {
            if tenancy.rls().is_some() {
                return Err(diagnostic::rule_invalid(
                    diagnostic::PROFILE_INVALID,
                    "rls-policy-unclaimed",
                    None,
                ));
            }
        }
    }
    Ok(())
}

/// Lifecycle rules: a transaction isolation never drops or truncates,
/// and a `none` provision never declares a `drop` cleanup (nothing was
/// created to drop).
fn check_lifecycle(attachment: &StorageEngineAttachment) -> Result<(), DiagnosticSet> {
    let Some(lifecycle) = attachment.test_lifecycle() else {
        return Ok(());
    };
    if lifecycle.isolation() == Isolation::Transaction
        && matches!(lifecycle.cleanup(), Cleanup::Drop | Cleanup::Truncate)
    {
        return Err(diagnostic::rule_invalid(
            diagnostic::LIFECYCLE_INVALID,
            "transaction-cleanup",
            None,
        ));
    }
    if lifecycle.provision() == Provision::None && lifecycle.cleanup() == Cleanup::Drop {
        return Err(diagnostic::rule_invalid(
            diagnostic::LIFECYCLE_INVALID,
            "unprovisioned-drop",
            None,
        ));
    }
    Ok(())
}
