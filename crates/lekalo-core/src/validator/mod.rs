//! Pure semantic validation over the typed Lekalo IR (issue #12).
//!
//! The entry points consume an accepted #8 [`Compilation`] and an accepted
//! [`ValidationProfile`] and produce a [`ValidationReport`] whose
//! diagnostics are normalized through the accepted #11 contract. The
//! validator never reads source files, never writes, never launches
//! adapters, and never mutates the project: structural, loader, source-map,
//! semantic-ID, and versioning failures pass through from their owning
//! issues.
//!
//! Determinism: a fixed phase order (resolution, semantic, portability),
//! registry-ordered rules, canonical definition order, fixed reference-site
//! order, exact deduplication, and the #11 total sort. Output never depends
//! on the filesystem state, locale, timezone, thread scheduling, or the
//! working directory.
//!
//! Exit mapping (owned by the caller): any surviving error severity means
//! `invalid` (exit 1); valid outcomes carry only warning/info diagnostics
//! and exit 0. Unsupported versions remain issue #9 (exit 5). Severity and
//! category never compute the exit.

pub mod context;
pub mod profile;
pub mod report;
pub mod rules;

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject};
use crate::diagnostics::DiagnosticSet;
use crate::ir::Compilation;
use crate::result::Status;

pub use profile::{ProfileError, RuleSelection, ValidationProfile};
pub use report::{SeverityCounts, ValidationReport};

/// The maximum number of diagnostics one run collects.
///
/// Mirrors the accepted #11 per-result bound; an over-limit producer set is
/// an invariant, so collection simply stops at the bound.
pub const MAX_DIAGNOSTICS: usize = crate::diagnostics::types::limits::DIAGNOSTICS_PER_RESULT;

/// Validate one compiled project under the given profile.
///
/// `Ok` reports carry only warning/info diagnostics; `Err` is the
/// normalized error set (the project is semantically invalid).
pub fn validate(
    compilation: &Compilation,
    profile: &ValidationProfile,
) -> Result<ValidationReport, DiagnosticSet> {
    run(compilation, profile, None)
}

/// Validate one compiled project and report module-scoped results.
///
/// The whole project plus its dependency closure is always validated; the
/// report keeps diagnostics owned by `module_scope` at any severity plus
/// every error anywhere, so mandatory cross-module failures are never
/// hidden. An unknown scope fails closed before any rule runs.
pub fn validate_scoped(
    compilation: &Compilation,
    profile: &ValidationProfile,
    module_scope: &str,
) -> Result<ValidationReport, DiagnosticSet> {
    run(compilation, profile, Some(module_scope))
}

fn run(
    compilation: &Compilation,
    profile: &ValidationProfile,
    module_scope: Option<&str>,
) -> Result<ValidationReport, DiagnosticSet> {
    let mut context = context::Context::new(&compilation.project, compilation.source_map.entries());
    if let Some(scope) = module_scope {
        let known = compilation
            .project
            .modules
            .iter()
            .any(|module| module.id.as_str() == scope);
        if !known {
            return Err(module_unresolved(scope));
        }
    }
    rules::resolution_phase(&mut context, profile);
    rules::semantic_phase(&mut context, profile);
    rules::portability_phase(&mut context, profile);
    let mut diagnostics = context.finish();
    if let Some(scope) = module_scope {
        diagnostics.retain(|diagnostic| {
            diagnostic.severity() == crate::diagnostics::types::Severity::Error
                || diagnostic
                    .symbol
                    .as_deref()
                    .and_then(context::Context::owner)
                    == Some(scope)
        });
    }
    let invalid = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == crate::diagnostics::types::Severity::Error);
    let status = if invalid {
        Status::Invalid
    } else {
        Status::Valid
    };
    match DiagnosticSet::try_from_unsorted(diagnostics, status) {
        Ok(set) => {
            if invalid {
                Err(set)
            } else {
                Ok(ValidationReport::from_set(
                    profile,
                    module_scope.map(str::to_owned),
                    set,
                ))
            }
        }
        // A rejected set is a developer fault: collapse to the single
        // registry invariant diagnostic instead of emitting unvalidated wire.
        Err(_) => Err(crate::result::singleton_set("diagnostics.registry-invalid")),
    }
}

/// The fail-closed set for an unusable embedded contract (developer
/// fault): the single registry invariant diagnostic.
pub fn registry_invariant_failure() -> DiagnosticSet {
    crate::result::singleton_set("diagnostics.registry-invalid")
}

fn module_unresolved(scope: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("module".to_owned(), token_value(scope));
    let diagnostic = build("validate.module-unresolved", None, None, data);
    match diagnostic {
        Ok(diagnostic) => DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Invalid)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}
