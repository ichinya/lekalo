//! The loader → diagnostic wire adapter (issue #11).
//!
//! Converts the loader's internal typed diagnostics into closed wire
//! diagnostics through the embedded registry, then assembles the terminal
//! [`DomainResult`]. The adapter never calls `Display`, reparses messages,
//! or fabricates spans: it only bounds and types what the accepted #7
//! pipeline already produced.

use super::error::Diagnostic;
use crate::diagnostics::normalize::{build, source_location};
use crate::diagnostics::{Diagnostic as Wire, SourceLocation};
use crate::result::{DomainResult, Status, REGISTRY_INVALID};

/// Convert one loader position into the wire position.
fn wire_position(position: super::error::SpanPos) -> crate::diagnostics::Position {
    crate::diagnostics::Position {
        byte: position.byte,
        line: position.line,
        column: position.column,
    }
}

/// Convert one loader diagnostic into a closed wire diagnostic.
///
/// Registry data fields bound every string; the loader's bounded echoes stay
/// verbatim below the bound. A rule the registry does not know collapses the
/// whole set into the single invariant diagnostic.
fn wire_one(diagnostic: &Diagnostic) -> Option<Wire> {
    let source: Option<SourceLocation> = match (&diagnostic.path, &diagnostic.span) {
        (None, None) => None,
        (path, span) => source_location(
            path.as_ref()
                .map(|path| crate::diagnostics::types::bound_token(path)),
            span.as_ref().map(|span| crate::diagnostics::Range {
                start: wire_position(span.start),
                end: wire_position(span.end),
            }),
        ),
    };
    let data = match &diagnostic.data {
        Some(value) => crate::diagnostics::types::data_object_bounded(value)?,
        None => Default::default(),
    };
    build(&diagnostic.code, None, source, data).ok()
}

/// Convert the loader diagnostics of one terminal outcome.
pub(crate) fn to_wire(diagnostics: &[Diagnostic]) -> Vec<Wire> {
    let converted: Option<Vec<Wire>> = diagnostics.iter().map(wire_one).collect();
    converted.unwrap_or_else(|| {
        // A missed registry entry is a developer fault: emit exactly the
        // bounded invariant diagnostic instead of an unvalidated rule.
        build(REGISTRY_INVALID, None, None, Default::default())
            .map(|diagnostic| vec![diagnostic])
            .unwrap_or_default()
    })
}

/// Assemble the terminal loader result: closed wire set plus envelope.
///
/// A path policy denial inside a failing phase outranks generic invalidity
/// (exit 3 stdout), exactly as the accepted #4/#7 protocol fixed it.
pub(crate) fn failure(status: Status, diagnostics: Vec<Diagnostic>) -> DomainResult {
    let status = if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "loader.path-escape")
        && status == Status::Invalid
    {
        Status::Denied
    } else {
        status
    };
    let wire = to_wire(&diagnostics);
    let set = DomainResult::from_wire_set(status, wire);
    match status {
        Status::Denied => DomainResult::denied(set),
        Status::UnsupportedVersion => DomainResult::unsupported_version(set),
        _ => DomainResult::invalid(set),
    }
}
