//! Registry-owned messages; producers supply only bounded evidence tokens.
use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject};
use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::Status;

pub fn diagnostic(id: &str, subject: &str, detail: &str) -> Result<Diagnostic, DiagnosticSet> {
    let data = DataObject::from([
        ("detail".to_owned(), token_value(detail)),
        ("subject".to_owned(), token_value(subject)),
    ]);
    build(id, None, None, data)
        .map_err(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

pub fn set(id: &str, status: Status, detail: &str) -> DiagnosticSet {
    match diagnostic(id, "coupling", detail) {
        Ok(d) => DiagnosticSet::try_from_unsorted(vec![d], status)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(set) => set,
    }
}

pub fn invalid(detail: &str) -> DiagnosticSet {
    set("coupling.input-invalid", Status::Invalid, detail)
}

pub fn unsupported(detail: &str) -> DiagnosticSet {
    set(
        "coupling.profile-unsupported",
        Status::UnsupportedVersion,
        detail,
    )
}
