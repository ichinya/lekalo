//! The changed-constraint impact synthesis (issue #85).
//!
//! A constraint change reaches the accepted #16 impact engine through
//! the typed `ChangedInputSet` handoff — the same seam a Git adapter
//! uses — so the analysis, the radius, the scenario coverage, and the
//! gate selection stay exactly the impact contract's word. This module
//! diffs two same-family attachments and synthesizes one changed
//! input per affected scope symbol: the attachment's project-relative
//! logical path as the changed path, `modified` as the file change,
//! and canonical evidence, because the resolution is fully resolved
//! against the current source map. Zero impact contract changes; no
//! ninth risk dimension.

use crate::diagnostics::DiagnosticSet;
use crate::impact::input::{
    validate_path, ChangedInput, ChangedInputSet, ChangedMode, EntryEvidence, FileChange,
};

use super::diff;
use super::wire::NfrAttachment;

/// The synthesized changed-input handoff for one constraint change,
/// or `None` when the two attachments are semantically equal (the
/// caller owns the "nothing changed" decision).
pub fn changed_input_set(
    base: &NfrAttachment,
    candidate: &NfrAttachment,
    logical_path: &str,
) -> Result<Option<ChangedInputSet>, DiagnosticSet> {
    let logical_path = validate_path(logical_path)?;
    let diff = diff::compare(base, candidate)?;
    if diff.equal() {
        return Ok(None);
    }
    // Every changed path under `constraints/` names one constraint;
    // collect the affected scope symbols from whichever side still
    // declares the constraint.
    let mut symbols: Vec<String> = Vec::new();
    for path in diff.paths() {
        let Some(rest) = path.path().strip_prefix("constraints/") else {
            continue;
        };
        let constraint_id = rest.split('/').next().unwrap_or_default();
        if constraint_id.is_empty() {
            continue;
        }
        let scope_ref = candidate
            .constraints()
            .iter()
            .find(|constraint| constraint.constraint_id().as_str() == constraint_id)
            .map(|constraint| constraint.scope().reference().to_owned())
            .or_else(|| {
                base.constraints()
                    .iter()
                    .find(|constraint| constraint.constraint_id().as_str() == constraint_id)
                    .map(|constraint| constraint.scope().reference().to_owned())
            });
        if let Some(scope_ref) = scope_ref {
            if !symbols.contains(&scope_ref) {
                symbols.push(scope_ref);
            }
        }
    }
    if symbols.is_empty() {
        return Ok(None);
    }
    symbols.sort();
    let entries: Vec<ChangedInput> = symbols
        .iter()
        .map(|symbol| {
            ChangedInput::new(
                vec![symbol.clone()],
                Vec::new(),
                Some((logical_path.as_str(), None)),
                FileChange::Modified,
                EntryEvidence::Canonical,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let set = ChangedInputSet::from_entries(ChangedMode::Committed, None, None, entries)?;
    Ok(Some(set))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_never_escape_the_project() {
        assert!(validate_path("../escape.json").is_err());
        assert!(validate_path("/absolute.json").is_err());
        assert!(validate_path("lekalo/nfr.attachment.json").is_ok());
    }
}
