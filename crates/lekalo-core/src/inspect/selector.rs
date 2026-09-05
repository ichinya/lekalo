//! Safe selector classification and resolution (issue #15).
//!
//! The invocation selector is validated before any project discovery
//! and is never passed to a filesystem API: the accepted grammar is the
//! pure semantic-id spelling (one to three lowercase segments), so
//! path-like, URI-like, control, and overlong inputs fail as CLI usage
//! errors with no echo of the rejected bytes. A valid full id resolves
//! case-sensitively through the canonical definition index only; a miss
//! consults the accepted #6 alias registry (`rename_history`) and never
//! falls back to fuzzy or case-insensitive matching. A valid one
//! segment token is a safe short name that matches final name segments
//! only; zero matches and more than one match are distinct stable
//! diagnostics, and ambiguity never selects a first match.

use crate::diagnostics::DiagnosticSet;
use crate::ir::CompiledProject;

use super::diagnostic;

/// The closed resolution mode of one selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectorMode {
    /// The selector was a full semantic id and matched directly.
    ExactId,
    /// The selector was a one-segment safe short name.
    ShortName,
    /// The selector was a renamed former id resolved through the #6
    /// alias registry.
    Alias,
}

impl SelectorMode {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactId => "exact-id",
            Self::ShortName => "short-name",
            Self::Alias => "alias",
        }
    }
}

/// The closed selector shape after grammar validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SelectorForm {
    /// Two or three segments: a full semantic id.
    ExactId(String),
    /// One segment: a safe short name.
    ShortName(String),
}

/// One resolved selection: the mode, the validated input, and the index
/// of the resolved definition in the canonical IR order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Selection {
    pub mode: SelectorMode,
    pub input: String,
    pub definition: usize,
}

/// Classify one raw selector without touching any project data.
///
/// The accepted grammar is `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*){0,2}$`
/// with a total length of at most 191 bytes (the Model 1.0.0 symbol-id
/// bound). Everything else — uppercase, Unicode, whitespace, control
/// bytes, path and URI punctuation, trailing dots, overlength — is a
/// malformed invocation, not an unknown symbol.
pub(crate) fn classify(raw: &str) -> Result<SelectorForm, DiagnosticSet> {
    /// The total byte cap of one selector (the Model 1.0.0 symbol-id bound).
    const MAX_SELECTOR_BYTES: usize = 191;
    if raw.is_empty() || raw.len() > MAX_SELECTOR_BYTES {
        return Err(diagnostic::selector_invalid_set());
    }
    let bytes = raw.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return Err(diagnostic::selector_invalid_set());
    }
    let mut segments = 1usize;
    let mut previous_was_dot = false;
    for &byte in bytes {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' | b'_' => previous_was_dot = false,
            b'.' => {
                if previous_was_dot {
                    return Err(diagnostic::selector_invalid_set());
                }
                previous_was_dot = true;
                segments += 1;
                if segments > 3 {
                    return Err(diagnostic::selector_invalid_set());
                }
            }
            _ => return Err(diagnostic::selector_invalid_set()),
        }
    }
    if previous_was_dot {
        // The last byte was a dot: a trailing-dot alias spelling.
        return Err(diagnostic::selector_invalid_set());
    }
    if segments == 1 {
        Ok(SelectorForm::ShortName(raw.to_owned()))
    } else {
        Ok(SelectorForm::ExactId(raw.to_owned()))
    }
}

/// Resolve one raw selector against the compiled project.
///
/// Full ids look up the canonical definition index; a miss consults the
/// project alias registry and fails closed. Short names match final
/// segments across the canonical definitions; zero and multiple
/// matches are distinct diagnostics.
pub(crate) fn resolve(project: &CompiledProject, raw: &str) -> Result<Selection, DiagnosticSet> {
    match classify(raw)? {
        SelectorForm::ExactId(id) => {
            if let Some(definition) = project
                .definitions
                .iter()
                .position(|definition| definition.id().as_str() == id)
            {
                return Ok(Selection {
                    mode: SelectorMode::ExactId,
                    input: id,
                    definition,
                });
            }
            // Alias eligibility: only through the accepted #6 registry.
            let Some(registry) = project
                .project
                .as_ref()
                .and_then(|project| project.id_registry.as_ref())
            else {
                return Err(diagnostic::symbol_unknown_set(&id, "exact-id-miss"));
            };
            let tombstoned = registry
                .tombstones
                .iter()
                .any(|tombstone| tombstoned_id(tombstone) == id);
            let live: Vec<&str> = registry
                .rename_history
                .iter()
                .filter(|entry| entry.from.as_str() == id)
                .map(|entry| entry.to.as_str())
                .filter(|to| {
                    project
                        .definitions
                        .iter()
                        .any(|definition| definition.id().as_str() == *to)
                })
                .collect();
            match live.as_slice() {
                [target] => Ok(Selection {
                    mode: SelectorMode::Alias,
                    input: id,
                    definition: project
                        .definitions
                        .iter()
                        .position(|definition| definition.id().as_str() == *target)
                        .expect("live alias target just verified"),
                }),
                _ if tombstoned => Err(diagnostic::symbol_unknown_set(&id, "tombstoned")),
                _ if live.len() > 1 => {
                    Err(diagnostic::symbol_unknown_set(&id, "alias-conflicting"))
                }
                _ => Err(diagnostic::symbol_unknown_set(&id, "exact-id-miss")),
            }
        }
        SelectorForm::ShortName(name) => {
            let matches: Vec<usize> = project
                .definitions
                .iter()
                .enumerate()
                .filter(|(_, definition)| final_segment(definition.id().as_str()) == name.as_str())
                .map(|(index, _)| index)
                .collect();
            match matches.len() {
                0 => Err(diagnostic::short_name_unknown_set(&name)),
                1 => Ok(Selection {
                    mode: SelectorMode::ShortName,
                    input: name,
                    definition: matches[0],
                }),
                matched => {
                    let candidates: Vec<String> = matches
                        .iter()
                        .take(super::limits::InspectLimits::v1().candidates)
                        .map(|&index| project.definitions[index].id().as_str().to_owned())
                        .collect();
                    Err(diagnostic::short_name_ambiguous_set(
                        &name,
                        &candidates,
                        matched,
                    ))
                }
            }
        }
    }
}

/// The final name segment of a semantic id.
pub(crate) fn final_segment(id: &str) -> &str {
    id.rsplit('.').next().unwrap_or(id)
}

/// The id of one tombstone entry.
fn tombstoned_id(tombstone: &crate::ir::Tombstone) -> &str {
    match tombstone {
        crate::ir::Tombstone::Replaced { id, .. } => id.as_str(),
        crate::ir::Tombstone::Deleted { id, .. } => id.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::version::MAX_SYMBOL_ID_BYTES;
    use super::*;
    #[test]
    fn grammar_accepts_only_lowercase_semantic_spellings() {
        assert!(matches!(classify("task"), Ok(SelectorForm::ShortName(_))));
        assert!(matches!(
            classify("planner.task"),
            Ok(SelectorForm::ExactId(_))
        ));
        assert!(matches!(classify("a.b.c"), Ok(SelectorForm::ExactId(_))));
    }

    #[test]
    fn grammar_rejects_path_like_and_uppercase_inputs() {
        for raw in [
            "",
            ".",
            "..",
            "planner.",
            ".planner",
            "Planner.task",
            "planner.task.x.y",
            "planner/task",
            "planner\\task",
            "operation:planner.focus_task",
            "file:///etc/passwd",
            "%2e%2e",
            "planner task",
            "planner\u{0041}",
        ] {
            assert!(classify(raw).is_err(), "accepted {raw:?}");
        }
        let overlong = format!("a.{}", "b".repeat(MAX_SYMBOL_ID_BYTES));
        assert!(classify(&overlong).is_err());
    }

    #[test]
    fn control_bytes_never_enter_the_wire() {
        for raw in ["ta\nsk", "ta\tsk", "ta\0sk", "ta\u{7f}sk"] {
            assert!(classify(raw).is_err(), "accepted {raw:?}");
        }
    }
}
