//! The typed error → status projection of the transport attachment
//! (issue #70).
//!
//! Every projection keys on the immutable #62 error identity: the
//! wire body is always the canonical quadruple
//! `{"ok":false,"error":{id,code,category,payload}}` with public
//! payload fields only, so a domain error ID survives independently
//! of the HTTP status. There is no status-only mapping and no
//! catch-all: entries outside the operation's declared union refuse,
//! and unmapped union members refuse under the strict profile.
//! Undeclared infrastructure failures render the fixed generic body
//! (category `infrastructure`, no declared id/code) and never
//! masquerade as a declared error — validation, auth, domain, and
//! infrastructure failures stay distinguishable on the wire.

use crate::diagnostics::DiagnosticSet;
use crate::error_contract::result::PublicPayload;
use crate::error_contract::types::OperationErrorContract;
use crate::error_contract::{normalize, ErrorContract, ErrorRegistry};

use super::diagnostic;
use super::types::{ErrorDefaults, ErrorEntry};

/// The canonical status of one declared category: the explicit entry
/// when present, otherwise the fixed category default. A status of
/// zero means "no projection" and never serializes.
pub fn effective_status(
    entries: &[ErrorEntry],
    defaults: &ErrorDefaults,
    error: &ErrorContract,
) -> u16 {
    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.error.as_str() == error.id().as_str())
    {
        return entry.status;
    }
    defaults
        .status_of(error.category().as_str())
        .unwrap_or(defaults.infrastructure)
}

/// The canonical error envelope bytes of one declared error: the
/// #62 identity quadruple with an empty public payload. The runtime
/// fills validated public payload values; private fields never
/// cross the boundary.
pub fn error_envelope_bytes(error: &ErrorContract) -> String {
    format!(
        "{{\"error\":{},\"ok\":false}}",
        normalize::identity_bytes(error, &PublicPayload::empty())
    )
}

/// The fixed generic infrastructure response body: no declared id,
/// no code, no detail echo — never claiming a declared semantic
/// error occurred. The HTTP status is the declared infrastructure
/// default of the endpoint.
pub fn infrastructure_envelope_bytes() -> String {
    "{\"error\":{\"category\":\"infrastructure\",\"payload\":{}},\"ok\":false}".to_owned()
}

/// Check one endpoint's error map against the bound operation's
/// declared #62 union: every entry belongs to the union, every
/// mapped error resolves in the registry, and — under the strict
/// profile — every union member is mapped.
pub fn check_mapping(
    binding: &OperationErrorContract,
    registry: &ErrorRegistry,
    entries: &[ErrorEntry],
    strict: bool,
    subject: &str,
) -> Result<(), DiagnosticSet> {
    for entry in entries {
        let id =
            crate::error_contract::id::ErrorId::new(entry.error.as_str()).ok_or_else(|| {
                diagnostic::rule_invalid(
                    diagnostic::CONTRACT_INVALID,
                    "entry-unresolved",
                    Some(subject),
                )
            })?;
        let error = registry.error(&id).ok_or_else(|| {
            diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "entry-unresolved",
                Some(subject),
            )
        })?;
        if !binding.errors().contains(error.id()) {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "entry-outside-union",
                Some(subject),
            ));
        }
    }
    if strict {
        for member in binding.errors().members() {
            let mapped = entries
                .iter()
                .any(|entry| entry.error.as_str() == member.id().as_str());
            if !mapped {
                return Err(diagnostic::rule_invalid(
                    diagnostic::MAPPING_MISSING,
                    "union-member-unmapped",
                    Some(subject),
                ));
            }
        }
    }
    Ok(())
}

/// Whether the operation's declared error metadata forces an
/// idempotency key (any member `key-required`), forbids one (every
/// member `not-applicable`), or leaves it open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdempotencyDemand {
    /// At least one declared error requires a client key.
    KeyRequired,
    /// Every declared error is effect-free.
    NotApplicable,
    /// Neither direction is forced.
    Open,
}

/// Derive the idempotency demand of one bound operation from its
/// declared error metadata. An unbound operation leaves it open.
pub fn idempotency_demand(
    binding: Option<&OperationErrorContract>,
    registry: &ErrorRegistry,
) -> IdempotencyDemand {
    let Some(binding) = binding else {
        return IdempotencyDemand::Open;
    };
    let mut any_key_required = false;
    let mut all_not_applicable = true;
    for member in binding.errors().members() {
        let Some(error) = registry.error(member.id()) else {
            return IdempotencyDemand::Open;
        };
        match error.idempotency() {
            crate::error_contract::types::Idempotency::KeyRequired => {
                any_key_required = true;
                all_not_applicable = false;
            }
            crate::error_contract::types::Idempotency::NotApplicable => {}
            _ => all_not_applicable = false,
        }
    }
    if any_key_required {
        IdempotencyDemand::KeyRequired
    } else if all_not_applicable {
        IdempotencyDemand::NotApplicable
    } else {
        IdempotencyDemand::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_contract::types::ErrorCategory;

    #[test]
    fn the_infrastructure_envelope_is_generic_and_fixed() {
        let body = infrastructure_envelope_bytes();
        assert!(body.contains("\"category\":\"infrastructure\""));
        assert!(!body.contains("code"));
    }

    #[test]
    fn category_defaults_project_the_closed_vocabulary() {
        let defaults = ErrorDefaults {
            validation: 400,
            auth: 403,
            conflict: 409,
            not_found: 404,
            domain: 422,
            infrastructure: 500,
        };
        assert_eq!(
            defaults.status_of(ErrorCategory::Validation.as_str()),
            Some(400)
        );
        assert_eq!(defaults.status_of(ErrorCategory::Auth.as_str()), Some(403));
        assert_eq!(
            defaults.status_of(ErrorCategory::Infrastructure.as_str()),
            Some(500)
        );
        assert_eq!(defaults.status_of("not-a-category"), None);
    }
}
