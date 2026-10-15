//! Registry-owned portability diagnostics (issue #19); producers supply
//! only bounded evidence tokens.
//!
//! The portability family is registered in the reviewed M8 union
//! successor registry (`dev.lekalo.diagnostic-registry@0.8.0`); the
//! constructor resolves the entry through the shared registry chain and
//! every message, code, severity, and category comes from the registry.

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::Status;

/// Build one registered portability diagnostic with bounded tokens.
pub fn diagnostic(
    id: &str,
    subject: &str,
    detail: &'static str,
) -> Result<Diagnostic, DiagnosticSet> {
    let data = DataObject::from([
        ("detail".to_owned(), token_value(detail)),
        ("subject".to_owned(), token_value(subject)),
    ]);
    build(id, None, None, data)
        .map_err(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// Build one normalized set under one envelope status.
pub fn set(id: &str, status: Status, subject: &str, detail: &'static str) -> DiagnosticSet {
    match diagnostic(id, subject, detail) {
        Ok(d) => DiagnosticSet::try_from_unsorted(vec![d], status)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(set) => set,
    }
}

/// `portability.input-invalid` (LEK-PORT-001): malformed input.
pub fn invalid(detail: &'static str) -> DiagnosticSet {
    set(
        "portability.input-invalid",
        Status::Invalid,
        "portability",
        detail,
    )
}

/// `portability.version-unsupported` (LEK-PORT-002): unsupported
/// portability contract version.
pub fn unsupported_version(detail: &'static str) -> DiagnosticSet {
    set(
        "portability.version-unsupported",
        Status::UnsupportedVersion,
        "portability",
        detail,
    )
}

/// `portability.check-denied` (LEK-PORT-009): strict `--check` denial.
pub fn check_denied(detail: &'static str) -> DiagnosticSet {
    set(
        "portability.check-denied",
        Status::Denied,
        "portability",
        detail,
    )
}
