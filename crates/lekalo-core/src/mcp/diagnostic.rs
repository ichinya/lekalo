//! MCP transport diagnostics routed through the accepted #11 contract and
//! the issue #33 registry successor (`dev.lekalo.diagnostic-registry@0.8.0`).
//!
//! Every transport refusal is one registered `mcp.*` rule (`LEK-MCP-NNN`)
//! assembled through the shared registry-backed constructor. Transport
//! diagnostics supplement original core diagnostics and never rename them:
//! a domain failure keeps its own codes, a transport refusal carries the
//! exact MCP pin.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject, DataValue};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

/// A known-tool request or server configuration violated a semantic
/// invariant; bounded closed field/reason, no input echo.
pub const INPUT_INVALID: &str = "mcp.input-invalid";
/// An unregistered project/group/input or an attempted scope widening.
pub const SCOPE_DENIED: &str = "mcp.scope-denied";
/// The result or log cannot be disclosed safely under its contract.
pub const SENSITIVE_OUTPUT: &str = "mcp.sensitive-output";
/// An admission bound (queue/concurrency/frame/nesting/root) was exceeded.
pub const REQUEST_LIMIT: &str = "mcp.request-limit";
/// The complete encoded response exceeded its effective byte cap.
pub const OUTPUT_LIMIT: &str = "mcp.output-limit";
/// The request deadline expired; no late success exists.
pub const TIMEOUT: &str = "mcp.timeout";
/// The request was cancelled internally before completion.
pub const CANCELLED: &str = "mcp.cancelled";
/// An unknown exact tool/result/protocol pin; no automatic downgrade.
pub const CONTRACT_UNSUPPORTED: &str = "mcp.contract-unsupported";
/// Authorized input bytes changed during evaluation, or a stale cursor.
pub const SNAPSHOT_STALE: &str = "mcp.snapshot-stale";
/// A required safe read/confinement/host capability is unavailable.
pub const BACKEND_UNAVAILABLE: &str = "mcp.backend-unavailable";
/// The typed backend output violated schema/status/pin invariants.
pub const RESULT_INVALID: &str = "mcp.result-invalid";

pub(crate) fn one(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
) -> Result<crate::diagnostics::Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, symbol, None, data)
}

pub(crate) fn token(text: &str) -> DataValue {
    token_value(text)
}

fn set(id: &str, status: Status, symbol: Option<String>, data: DataObject) -> DiagnosticSet {
    match one(id, symbol, data) {
        Ok(diagnostic) => DiagnosticSet::try_from_unsorted(vec![diagnostic], status)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

fn object(fields: &[(&str, DataValue)]) -> DataObject {
    let mut data = DataObject::new();
    for (name, value) in fields {
        data.insert((*name).to_owned(), value.clone());
    }
    data
}

/// The `invalid` set for a semantically invalid known-tool request.
pub fn input_invalid_set(field: &str, reason: &str) -> DiagnosticSet {
    set(
        INPUT_INVALID,
        Status::Invalid,
        None,
        object(&[("field", token(field)), ("reason", token(reason))]),
    )
}

/// The `denied` set for an unregistered or widened scope.
pub fn scope_denied_set(subject: &str, reason: &str) -> DiagnosticSet {
    set(
        SCOPE_DENIED,
        Status::Denied,
        None,
        object(&[("subject", token(subject)), ("reason", token(reason))]),
    )
}

/// The `denied` set for output or log text that cannot be disclosed.
pub fn sensitive_output_set(surface: &str, reason: &str) -> DiagnosticSet {
    set(
        SENSITIVE_OUTPUT,
        Status::Denied,
        None,
        object(&[("surface", token(surface)), ("reason", token(reason))]),
    )
}

/// The `denied` set for an exceeded admission bound.
pub fn request_limit_set(bound: &str) -> DiagnosticSet {
    set(
        REQUEST_LIMIT,
        Status::Denied,
        None,
        object(&[("bound", token(bound))]),
    )
}

/// The `denied` set for a complete response beyond its cap. The exact
/// byte counts are recorded without echoing payload text.
pub fn output_limit_set(limit: usize, actual: usize) -> DiagnosticSet {
    set(
        OUTPUT_LIMIT,
        Status::Denied,
        None,
        object(&[
            ("limit", DataValue::Count(limit as u64)),
            ("actual", DataValue::Count(actual as u64)),
        ]),
    )
}

/// The `unavailable` set for an expired deadline.
pub fn timeout_set(deadline_ms: u64) -> DiagnosticSet {
    set(
        TIMEOUT,
        Status::Unavailable,
        None,
        object(&[("deadlineMs", DataValue::Count(deadline_ms))]),
    )
}

/// The `unavailable` set for an internal cancellation outcome.
pub fn cancelled_set() -> DiagnosticSet {
    set(CANCELLED, Status::Unavailable, None, DataObject::new())
}

/// The `unsupported-version` set for an unknown exact pin.
pub fn contract_unsupported_set(pin: &str) -> DiagnosticSet {
    set(
        CONTRACT_UNSUPPORTED,
        Status::UnsupportedVersion,
        None,
        object(&[("pin", token(pin))]),
    )
}

/// The `unavailable` set for input bytes that changed during evaluation.
pub fn snapshot_stale_set(domain: &str) -> DiagnosticSet {
    set(
        SNAPSHOT_STALE,
        Status::Unavailable,
        None,
        object(&[("domain", token(domain))]),
    )
}

/// The `unavailable` set for a missing required read/confinement
/// capability; no broad fallback exists.
pub fn backend_unavailable_set(capability: &str) -> DiagnosticSet {
    set(
        BACKEND_UNAVAILABLE,
        Status::Unavailable,
        None,
        object(&[("capability", token(capability))]),
    )
}

/// The `invalid` set for a backend output violating its invariants.
pub fn result_invalid_set(detail: &str) -> DiagnosticSet {
    set(
        RESULT_INVALID,
        Status::Invalid,
        None,
        object(&[("detail", token(detail))]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagnosticRegistry;

    #[test]
    fn every_mcp_rule_is_registered_and_active() {
        let registry = DiagnosticRegistry::mcp_successor().expect("mcp registry");
        for (id, code, status) in [
            (INPUT_INVALID, "LEK-MCP-001", Status::Invalid),
            (SCOPE_DENIED, "LEK-MCP-002", Status::Denied),
            (SENSITIVE_OUTPUT, "LEK-MCP-003", Status::Denied),
            (REQUEST_LIMIT, "LEK-MCP-004", Status::Denied),
            (OUTPUT_LIMIT, "LEK-MCP-005", Status::Denied),
            (TIMEOUT, "LEK-MCP-006", Status::Unavailable),
            (CANCELLED, "LEK-MCP-007", Status::Unavailable),
            (
                CONTRACT_UNSUPPORTED,
                "LEK-MCP-008",
                Status::UnsupportedVersion,
            ),
            (SNAPSHOT_STALE, "LEK-MCP-009", Status::Unavailable),
            (BACKEND_UNAVAILABLE, "LEK-MCP-010", Status::Unavailable),
            (RESULT_INVALID, "LEK-MCP-011", Status::Invalid),
        ] {
            let entry = registry.entry(id).unwrap_or_else(|| panic!("{id}"));
            assert_eq!(entry.code(), code);
            assert!(entry.allows_status(status), "{id} allows {:?}", status);
        }
    }

    #[test]
    fn constructors_carry_the_registered_identity() {
        let set = scope_denied_set("project", "unregistered");
        let diagnostic = &set.as_slice()[0];
        assert_eq!(diagnostic.code(), "LEK-MCP-002");
        assert_eq!(diagnostic.registry_version, "0.8.0");
        assert_eq!(diagnostic.severity.as_str(), "error");
        assert_eq!(diagnostic.category.as_str(), "security");

        let limit = output_limit_set(1024, 2048);
        assert_eq!(limit.as_slice()[0].code(), "LEK-MCP-005");
        assert_eq!(cancelled_set().as_slice()[0].code(), "LEK-MCP-007");
        assert_eq!(
            contract_unsupported_set("lekalo/mcp-tool-x-output/v0.8.0").as_slice()[0].code(),
            "LEK-MCP-008"
        );
    }
}
