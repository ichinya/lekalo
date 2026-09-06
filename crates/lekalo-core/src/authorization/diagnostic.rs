//! Authorization diagnostics routed through the accepted #11 contract
//! (issue #25).
//!
//! Every authorization failure is one registered `authorization.*` rule
//! assembled through the shared registry-backed constructor and finalized
//! into a normalized [`DiagnosticSet`]. Every echoed token is bounded
//! before construction: rule data carries only fixed detail tags, rule or
//! policy identifiers that already passed grammar validation, and declared
//! limits — never raw input, paths, or attacker-controlled text.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::Status;

/// The registered rule for a document violating its closed schema.
pub(crate) const DOCUMENT_INVALID: &str = "authorization.document-invalid";
/// The registered rule for an unknown or mismatched actor declaration.
pub(crate) const ACTOR_INVALID: &str = "authorization.actor-invalid";
/// The registered rule for an invalid scope binding.
pub(crate) const SCOPE_INVALID: &str = "authorization.scope-invalid";
/// The registered rule for duplicated or conflicting declarations.
pub(crate) const POLICY_INVALID: &str = "authorization.policy-invalid";
/// The registered rule for an unresolved declared reference.
pub(crate) const REF_UNRESOLVED: &str = "authorization.ref-unresolved";
/// The registered rule for a malformed error-contract reference.
pub(crate) const ERROR_REF_UNRESOLVED: &str = "authorization.error-ref-unresolved";
/// The registered rule for cyclic, ambiguous, or conflicting composition.
pub(crate) const COMPOSITION_INVALID: &str = "authorization.composition-invalid";
/// The registered rule for an uncovered protected field access.
/// Registered for the evidence phase; declared IR effects are
/// entity-scoped in v1, so no core emitter fires it yet.
#[allow(dead_code)]
pub(crate) const FIELD_UNCOVERED: &str = "authorization.field-uncovered";
/// The registered rule for a protected effect without authorization.
pub(crate) const EFFECT_UNPROTECTED: &str = "authorization.effect-unprotected";
/// The registered rule for mapping evidence that cannot pass strict.
pub(crate) const MAPPING_STALE: &str = "authorization.mapping-stale";
/// The registered rule for a closed bound violation.
pub(crate) const LIMIT_EXCEEDED: &str = "authorization.limit-exceeded";
/// The registered rule for an unknown or stale profile reference.
pub(crate) const PROFILE_INVALID: &str = "authorization.profile-invalid";

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault) instead of
/// panicking.
fn one(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
) -> Result<Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, symbol, None, data)
}

/// Build a validated set with the given status or collapse to the
/// invariant set.
fn set(diagnostics: Vec<Diagnostic>, status: Status) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, status)
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// The `invalid` set for document-shape failures.
pub(crate) fn invalid_set(diagnostics: Vec<Diagnostic>) -> DiagnosticSet {
    set(diagnostics, Status::Invalid)
}

/// A bounded echo of text that never passed grammar validation.
pub(crate) fn bounded(text: &str) -> String {
    bound_token(text)
}

/// The `denied` set for strict coverage and mapping failures.
pub(crate) fn denied_set(diagnostics: Vec<Diagnostic>) -> DiagnosticSet {
    set(diagnostics, Status::Denied)
}

/// One bounded, grammar-validated identifier echo.
pub(crate) fn token(text: &str) -> DataValue {
    token_value(text)
}

fn insert_detail(data: &mut DataObject, detail: &str) {
    data.insert("detail".to_owned(), token(detail));
}

/// One `authorization.document-invalid` diagnostic.
pub(crate) fn document_invalid(detail: &str, echoed: Option<&str>) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    insert_detail(&mut data, detail);
    if let Some(echoed) = echoed {
        data.insert("field".to_owned(), token(&bounded(echoed)));
    }
    one(DOCUMENT_INVALID, None, data).ok()
}

/// One `authorization.actor-invalid` diagnostic.
pub(crate) fn actor_invalid(rule: &str, detail: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    insert_detail(&mut data, detail);
    data.insert("rule".to_owned(), token(rule));
    one(ACTOR_INVALID, None, data).ok()
}

/// One `authorization.scope-invalid` diagnostic.
pub(crate) fn scope_invalid(rule: &str, dimension: &str, detail: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    insert_detail(&mut data, detail);
    data.insert("rule".to_owned(), token(rule));
    data.insert("dimension".to_owned(), token(dimension));
    one(SCOPE_INVALID, None, data).ok()
}

/// One `authorization.policy-invalid` diagnostic.
pub(crate) fn policy_invalid(rule: &str, detail: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    insert_detail(&mut data, detail);
    data.insert("rule".to_owned(), token(rule));
    one(POLICY_INVALID, None, data).ok()
}

/// One `authorization.ref-unresolved` diagnostic.
pub(crate) fn ref_unresolved(rule: &str, reference: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    insert_detail(&mut data, "unresolved");
    data.insert("rule".to_owned(), token(rule));
    data.insert("reference".to_owned(), token(reference));
    one(REF_UNRESOLVED, None, data).ok()
}

/// One `authorization.error-ref-unresolved` diagnostic.
pub(crate) fn error_ref_unresolved(rule: &str, error_ref: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    insert_detail(&mut data, "unreachable");
    data.insert("rule".to_owned(), token(rule));
    data.insert("errorRef".to_owned(), token(error_ref));
    one(ERROR_REF_UNRESOLVED, None, data).ok()
}

/// One `authorization.composition-invalid` diagnostic.
pub(crate) fn composition_invalid(rule: &str, detail: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    insert_detail(&mut data, detail);
    data.insert("rule".to_owned(), token(rule));
    one(COMPOSITION_INVALID, None, data).ok()
}

/// One `authorization.field-uncovered` diagnostic (no declared-IR
/// emitter in v1; kept registered for exact-field evidence).
#[allow(dead_code)]
pub(crate) fn field_uncovered(operation: &str, field: &str, mode: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    data.insert("operation".to_owned(), token(operation));
    data.insert("field".to_owned(), token(field));
    data.insert("detail".to_owned(), token(mode));
    one(FIELD_UNCOVERED, None, data).ok()
}

/// One `authorization.effect-unprotected` diagnostic.
pub(crate) fn effect_unprotected(operation: &str, effect: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    data.insert("operation".to_owned(), token(operation));
    data.insert("effect".to_owned(), token(effect));
    one(EFFECT_UNPROTECTED, None, data).ok()
}

/// One `authorization.mapping-stale` diagnostic.
pub(crate) fn mapping_stale(policy: &str, state: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    data.insert("policy".to_owned(), token(policy));
    data.insert("state".to_owned(), token(state));
    one(MAPPING_STALE, None, data).ok()
}

/// One `authorization.limit-exceeded` diagnostic.
pub(crate) fn limit_exceeded(limit: &str, count: u64) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    insert_detail(&mut data, "bound-exceeded");
    data.insert("limit".to_owned(), token(limit));
    data.insert("count".to_owned(), DataValue::Count(count));
    one(LIMIT_EXCEEDED, None, data).ok()
}

/// One `authorization.profile-invalid` diagnostic.
pub(crate) fn profile_invalid(profile: &str) -> Option<Diagnostic> {
    let mut data = DataObject::new();
    insert_detail(&mut data, "unknown-profile");
    data.insert("profile".to_owned(), token(&bounded(profile)));
    one(PROFILE_INVALID, None, data).ok()
}
