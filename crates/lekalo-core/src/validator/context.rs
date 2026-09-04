//! The per-run rule context: source-span lookup, owner resolution, and the
//! bounded diagnostic sink (issue #12).
//!
//! Spans come only from the accepted #8 source map, which the compiler
//! filled after grammar validation; lookup is by `(logical path, JSON
//! pointer)` derived from the definition's own source-map base, so the
//! occurrence-safe ordinal of every reference site is preserved. Every
//! echoed token is bounded exactly like the #11 wire requires, and the sink
//! enforces the fixed per-result diagnostic bound.

use std::collections::{BTreeSet, HashMap};

use crate::diagnostics::normalize::{build, BuildError};
use crate::diagnostics::types::{limits, token_value, DataObject, DataValue, Scalar};
use crate::diagnostics::{Diagnostic, Range, SourceLocation};
use crate::ir::CompiledProject;
use crate::loader::SourceMapEntry;

/// One half-open source range with 1-based line/column positions.
struct Entry {
    path: String,
    start: (usize, usize, usize),
    end: (usize, usize, usize),
}

/// The mutable state every rule walks with.
pub(crate) struct Context<'a> {
    project: &'a CompiledProject,
    /// `(logical path, pointer)` to span positions, from the source map.
    spans: HashMap<(String, String), Entry>,
    /// Semantic id to `(logical path, base pointer)` of the definition.
    bases: HashMap<String, (String, String)>,
    /// Collected diagnostics in rule execution order (normalized later).
    diagnostics: Vec<Diagnostic>,
    /// Definitions whose span-unavailable invariant was already reported.
    missing_spans: BTreeSet<String>,
}

impl<'a> Context<'a> {
    /// Index the compilation's source map once per run.
    pub(crate) fn new(project: &'a CompiledProject, source_map: &[SourceMapEntry]) -> Self {
        let mut spans = HashMap::with_capacity(source_map.len());
        let mut bases = HashMap::with_capacity(source_map.len());
        for entry in source_map {
            if let Some(id) = &entry.semantic_id {
                bases.insert(id.clone(), (entry.path.clone(), entry.pointer.clone()));
            }
            spans.insert(
                (entry.path.clone(), entry.pointer.clone()),
                Entry {
                    path: entry.path.clone(),
                    start: (entry.start.byte, entry.start.line, entry.start.column),
                    end: (entry.end.byte, entry.end.line, entry.end.column),
                },
            );
        }
        Self {
            project,
            spans,
            bases,
            diagnostics: Vec::new(),
            missing_spans: BTreeSet::new(),
        }
    }

    /// The compiled project under validation.
    pub(crate) fn project(&self) -> &'a CompiledProject {
        self.project
    }

    /// The owning module of one symbol id: its first dot segment.
    ///
    /// Module ids are single-segment in every accepted Model version, so
    /// the first segment is exactly the declaring module qualifier the
    /// loader expanded every short reference to.
    pub(crate) fn owner(symbol: &str) -> Option<&str> {
        let dot = symbol.find('.')?;
        Some(&symbol[..dot])
    }

    /// Resolve the source location of one reference site.
    ///
    /// `base` is the subject definition's semantic id and `suffix` the
    /// pointer suffix below it, for example `/effects/0` or
    /// `/fields/1/type/list`. A miss is an infrastructure invariant, never
    /// a silent partial result.
    pub(crate) fn span_of(&mut self, base: &str, suffix: &str) -> Option<SourceLocation> {
        let Some((path, pointer)) = self.bases.get(base) else {
            self.report_missing_span(base);
            return None;
        };
        let full = format!("{pointer}{suffix}");
        let Some(entry) = self.spans.get(&(path.clone(), full)) else {
            self.report_missing_span(base);
            return None;
        };
        let (start_byte, start_line, start_column) = entry.start;
        let (end_byte, end_line, end_column) = entry.end;
        Some(SourceLocation {
            path: Some(entry.path.clone()),
            range: Some(Range {
                start: crate::diagnostics::Position {
                    byte: start_byte,
                    line: start_line,
                    column: start_column,
                },
                end: crate::diagnostics::Position {
                    byte: end_byte,
                    line: end_line,
                    column: end_column,
                },
            }),
        })
    }

    /// Emit one registered diagnostic, honoring profile severity overrides.
    pub(crate) fn emit(
        &mut self,
        profile: &super::profile::ValidationProfile,
        rule_id: &str,
        symbol: Option<&str>,
        source: Option<SourceLocation>,
        data: DataObject,
    ) {
        if self.diagnostics.len() >= limits::DIAGNOSTICS_PER_RESULT {
            return;
        }
        let mut diagnostic = match build(rule_id, symbol.map(token_value_string), source, data) {
            Ok(diagnostic) => diagnostic,
            Err(BuildError::UnknownRule(_)) | Err(BuildError::Inactive(_)) => {
                // The registry, the profile, and the rule set ship in
                // lockstep; an unknown rule is a developer fault that
                // fails closed in `DiagnosticSet` normalization.
                return;
            }
            Err(BuildError::Registry) => return,
        };
        if let Some(selection) = profile.selection(rule_id) {
            if let Some(severity) = selection.severity_override() {
                diagnostic.severity = severity;
            }
        }
        self.diagnostics.push(diagnostic);
    }

    /// Report the infrastructure invariant once per subject definition.
    fn report_missing_span(&mut self, base: &str) {
        if self.missing_spans.insert(base.to_owned()) {
            let mut data = DataObject::new();
            data.insert("detail".to_owned(), token_value("source-map-miss"));
            if let Ok(diagnostic) = build(
                "validate.span-unavailable",
                Some(token_value_string(base)),
                None,
                data,
            ) {
                if self.diagnostics.len() < limits::DIAGNOSTICS_PER_RESULT {
                    self.diagnostics.push(diagnostic);
                }
            }
        }
    }

    /// Take the collected diagnostics (execution order; normalized later).
    pub(crate) fn finish(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

/// Bound one echoed symbol through the closed token invariant.
fn token_value_string(text: &str) -> String {
    match token_value(text) {
        DataValue::Token(bounded) => bounded,
        _ => unreachable!("token_value always yields a token"),
    }
}

/// Build the closed `expected` data value from a sorted kind list.
pub(crate) fn expected_kinds(kinds: &[&str]) -> DataValue {
    let mut members: Vec<Scalar> = kinds
        .iter()
        .map(|kind| Scalar::Token((*kind).to_owned()))
        .collect();
    members.sort_by_key(crate::diagnostics::types::scalar_sort_key);
    DataValue::List(members)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_is_the_first_dot_segment() {
        assert_eq!(Context::owner("planner.task"), Some("planner"));
        assert_eq!(Context::owner("planner.kind_token.task"), Some("planner"));
        assert_eq!(Context::owner("project"), None);
    }

    #[test]
    fn expected_lists_are_byte_sorted() {
        let value = expected_kinds(&["entity", "scalar", "enum", "value-object"]);
        let DataValue::List(members) = value else {
            unreachable!("expected_kinds yields a list");
        };
        let spellings: Vec<&str> = members
            .iter()
            .map(|scalar| match scalar {
                Scalar::Token(text) => text.as_str(),
                _ => unreachable!("kind members are tokens"),
            })
            .collect();
        assert_eq!(spellings, ["entity", "enum", "scalar", "value-object"]);
    }
}
