//! The registered `search.*` diagnostic family emitters (issue #93).
//!
//! Every failure is one of the eight rules added by the registry union
//! successor `dev.lekalo.diagnostic-registry@0.8.0`, assembled through
//! the shared registry-backed constructor. No diagnostic ever echoes a
//! secret query, hidden path, description body, or candidate identity
//! that policy withheld; every data field is a bounded token or count.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject, DataValue};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::Status;

/// `search.query-invalid` / `LEK-SRCH-001`.
pub(crate) const QUERY_INVALID: &str = "search.query-invalid";
/// `search.version-unsupported` / `LEK-SRCH-002`.
pub(crate) const VERSION_UNSUPPORTED: &str = "search.version-unsupported";
/// `search.input-invalid` / `LEK-SRCH-003`.
pub(crate) const INPUT_INVALID: &str = "search.input-invalid";
/// `search.index-stale` / `LEK-SRCH-004`.
pub(crate) const INDEX_STALE: &str = "search.index-stale";
/// `search.cursor-invalid` / `LEK-SRCH-005`.
pub(crate) const CURSOR_INVALID: &str = "search.cursor-invalid";
/// `search.result-limit` / `LEK-SRCH-006`.
pub(crate) const RESULT_LIMIT: &str = "search.result-limit";
/// `search.coverage-incomplete` / `LEK-SRCH-007`.
pub(crate) const COVERAGE_INCOMPLETE: &str = "search.coverage-incomplete";
/// `search.ambiguous` / `LEK-SRCH-008`.
pub(crate) const AMBIGUOUS: &str = "search.ambiguous";

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault).
pub(crate) fn one(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
) -> Result<Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, symbol, None, data)
}

/// Build a validated set under the given envelope status.
pub(crate) fn set(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
    status: Status,
) -> DiagnosticSet {
    match one(id, symbol, data) {
        Ok(diagnostic) => DiagnosticSet::try_from_unsorted(vec![diagnostic], status)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The `invalid` failure for a closed request/member/selector invariant
/// violation. `member` names the offending request member, never its
/// value (an echoed selector is a bounded token).
pub fn query_invalid(member: &str, detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    data.insert("member".to_owned(), token_value(&bound_token(member)));
    set(QUERY_INVALID, None, data, Status::Invalid)
}

/// The `unsupported-version` failure for an unsupported search query,
/// index, input, or recipe contract.
pub fn version_unsupported(contract: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("contract-unsupported"));
    data.insert("contract".to_owned(), token_value(&bound_token(contract)));
    set(VERSION_UNSUPPORTED, None, data, Status::UnsupportedVersion)
}

/// The `invalid` failure for a malformed/mixed manifest or producer
/// attachment (reason only; upstream parse diagnostics stay theirs).
pub fn input_invalid(detail: &str, producer: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    if let Some(producer) = producer {
        data.insert("producer".to_owned(), token_value(&bound_token(producer)));
    }
    set(INPUT_INVALID, None, data, Status::Invalid)
}

/// The `valid` warning (or `denied` refusal) for index/input/security
/// pins that no longer match. `pin` is the bounded mismatch kind.
pub fn index_stale(pin: &str, denied: bool) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("pin-mismatch"));
    data.insert("pin".to_owned(), token_value(pin));
    let status = if denied {
        Status::Denied
    } else {
        Status::Valid
    };
    set(INDEX_STALE, None, data, status)
}

/// The `invalid` failure for a malformed/oversize/mismatched cursor.
pub fn cursor_invalid(detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    set(CURSOR_INVALID, None, data, Status::Invalid)
}

/// The `valid` warning (or `denied` refusal) when a work/index/output
/// cap prevents a complete answer. Ordinary pagination never warns.
pub fn result_limit(limit: &str, denied: bool) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("cap-reached"));
    data.insert("limit".to_owned(), token_value(limit));
    let status = if denied {
        Status::Denied
    } else {
        Status::Valid
    };
    set(RESULT_LIMIT, None, data, status)
}

/// The `valid` warning for a needed producer/domain that is missing,
/// stale, partial, or unsupported. `domain` is the allowlisted domain
/// key (`effects` | `trace` | `requirements` | `bindings` |
/// `diagnostics` | `waivers`), never a hidden subject.
pub fn coverage_incomplete(domain: &str, detail: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    data.insert("domain".to_owned(), token_value(&bound_token(domain)));
    set(COVERAGE_INCOMPLETE, None, data, Status::Valid)
}

/// The `valid` info diagnostic for equally supported qualified
/// interpretations. `candidates` is the visible retained count.
pub fn ambiguous(candidates: u64) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("tied-candidates"));
    data.insert("candidates".to_owned(), DataValue::Count(candidates));
    set(AMBIGUOUS, None, data, Status::Valid)
}

/// The `valid` warning counting producer rows the projection could not
/// attach: the answer is explicitly partial, never silently so.
pub fn dropped_rows(count: u64) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value("cap-reached"));
    data.insert("limit".to_owned(), token_value("dropped-producer-rows"));
    data.insert("rows".to_owned(), DataValue::Count(count));
    set(RESULT_LIMIT, None, data, Status::Valid)
}
