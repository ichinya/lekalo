//! Deterministic diagnostic construction, normalization, sorting, and
//! deduplication (issue #11).
//!
//! Producers build diagnostics through [`build`], which resolves the embedded
//! registry entry and validates every field against it. [`DiagnosticSet`]
//! validates a whole result against its envelope status, normalizes nested
//! sets, removes exact machine duplicates, and applies the total order so
//! output bytes never depend on producer, thread, or filesystem order.

use serde::Serialize;

use super::id::{DiagnosticId, MessageId};
use super::registry::DiagnosticRegistry;
use super::types::Severity;
use super::version::DiagnosticSchemaVersion;
use super::version::REGISTRY_VERSION;
use super::{Diagnostic, DiagnosticCause, RelatedLocation, SourceLocation, SuggestedFix};
use crate::result::Status;

/// Why one diagnostic could not be built from its registry entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BuildError {
    /// The embedded registry itself failed validation.
    Registry,
    /// The rule id is not registered.
    UnknownRule(String),
    /// The rule exists but is not currently emitted.
    Inactive(String),
}

/// Why a diagnostic set could not be normalized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetError {
    /// The embedded registry itself failed validation.
    Registry,
    /// A rule id is not registered or not active.
    UnknownRule(String),
    /// A rule may not appear under this envelope status.
    StatusNotAllowed(String),
    /// The set exceeds the fixed diagnostic bound.
    OverLimit,
}

/// Resolve the registry entry and construct one fully validated diagnostic.
///
/// `message`, `code`, `severity`, and `category` always come from the
/// registry; callers never supply rendered text.
pub(crate) fn build(
    id: &str,
    symbol: Option<String>,
    source: Option<SourceLocation>,
    data: super::DataObject,
) -> Result<Diagnostic, BuildError> {
    let registry = DiagnosticRegistry::embedded().map_err(|_| BuildError::Registry)?;
    let entry = registry
        .entry(id)
        .ok_or_else(|| BuildError::UnknownRule(id.to_owned()))?;
    if entry.lifecycle() != super::registry::Lifecycle::Active {
        return Err(BuildError::Inactive(id.to_owned()));
    }
    Ok(Diagnostic {
        schema_version: DiagnosticSchemaVersion::V1_0_0,
        registry_version: REGISTRY_VERSION.to_owned(),
        id: DiagnosticId::new(id).ok_or(BuildError::Registry)?,
        code: super::id::DiagnosticCode::new(entry.code().to_owned())
            .ok_or(BuildError::Registry)?,
        severity: entry.default_severity(),
        category: entry.category(),
        message_id: MessageId::new(entry.id().to_owned()).ok_or(BuildError::Registry)?,
        message: entry.default_message().to_owned(),
        symbol,
        source,
        data,
        related_locations: Vec::new(),
        causes: Vec::new(),
        fixes: Vec::new(),
        metadata: super::ProviderMetadata::new(),
    })
}

/// Attach validated related locations (sorted set).
///
/// v1 producers emit none; this composition surface exists for the in-crate
/// provider seam and its unit tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn with_related(
    mut diagnostic: Diagnostic,
    related: Vec<RelatedLocation>,
) -> Diagnostic {
    let mut related = related;
    related.sort_by(|left, right| {
        (
            left.relation.as_str(),
            location_path(&left.location),
            position_key(&left.location),
        )
            .cmp(&(
                right.relation.as_str(),
                location_path(&right.location),
                position_key(&right.location),
            ))
    });
    related.dedup();
    diagnostic.related_locations = related;
    diagnostic
}

/// Attach validated causes; the immediate-to-root order is semantic and is
/// never reordered.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn with_causes(mut diagnostic: Diagnostic, causes: Vec<DiagnosticCause>) -> Diagnostic {
    diagnostic.causes = causes;
    diagnostic
}

/// Attach validated fixes (sorted by id, applicability rank, then target).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn with_fixes(mut diagnostic: Diagnostic, fixes: Vec<SuggestedFix>) -> Diagnostic {
    let mut fixes = fixes;
    fixes.sort_by(|left, right| {
        (
            left.fix_id.as_str(),
            left.applicability.rank(),
            left.target
                .as_ref()
                .map(|target| target.path.clone().unwrap_or_default())
                .unwrap_or_default(),
        )
            .cmp(&(
                right.fix_id.as_str(),
                right.applicability.rank(),
                right
                    .target
                    .as_ref()
                    .map(|target| target.path.clone().unwrap_or_default())
                    .unwrap_or_default(),
            ))
    });
    fixes.dedup();
    diagnostic.fixes = fixes;
    diagnostic
}

#[cfg_attr(not(test), allow(dead_code))]
fn location_path(location: &SourceLocation) -> String {
    location.path.clone().unwrap_or_default()
}

#[cfg_attr(not(test), allow(dead_code))]
fn position_key(location: &SourceLocation) -> (usize, usize) {
    location
        .range
        .map(|range| (range.start.byte, range.end.byte))
        .unwrap_or((usize::MAX, usize::MAX))
}

/// A normalized, deduplicated, totally sorted diagnostic set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DiagnosticSet {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticSet {
    /// Validate, normalize, deduplicate, and sort one result set.
    ///
    /// Every diagnostic must be registered, active, and allowed under
    /// `status`; the set must fit the fixed bound. Producers never sort,
    /// classify, or deduplicate diagnostics themselves.
    pub fn try_from_unsorted(
        diagnostics: Vec<Diagnostic>,
        status: Status,
    ) -> Result<Self, SetError> {
        if diagnostics.len() > super::types::limits::DIAGNOSTICS_PER_RESULT {
            return Err(SetError::OverLimit);
        }
        let registry = DiagnosticRegistry::embedded().map_err(|_| SetError::Registry)?;
        for diagnostic in &diagnostics {
            let Some(entry) = registry.entry(diagnostic.id()) else {
                return Err(SetError::UnknownRule(diagnostic.id().to_owned()));
            };
            if entry.lifecycle() != super::registry::Lifecycle::Active {
                return Err(SetError::UnknownRule(diagnostic.id().to_owned()));
            }
            if !entry.allows_status(status) {
                return Err(SetError::StatusNotAllowed(diagnostic.id().to_owned()));
            }
        }
        let mut diagnostics = diagnostics;
        for diagnostic in &mut diagnostics {
            normalize_nested(diagnostic);
        }
        diagnostics.sort_by(|left, right| {
            sort_key(left)
                .cmp(&sort_key(right))
                .then_with(|| {
                    let left = serde_json::to_string(&left.data).unwrap_or_default();
                    let right = serde_json::to_string(&right.data).unwrap_or_default();
                    left.cmp(&right)
                })
                .then_with(|| {
                    let left = serde_json::to_string(&(
                        &left.related_locations,
                        &left.causes,
                        &left.fixes,
                        &left.metadata,
                    ))
                    .unwrap_or_default();
                    let right = serde_json::to_string(&(
                        &right.related_locations,
                        &right.causes,
                        &right.fixes,
                        &right.metadata,
                    ))
                    .unwrap_or_default();
                    left.cmp(&right)
                })
        });
        diagnostics.dedup_by(|left, right| super::machine_eq(left, right));
        Ok(Self { diagnostics })
    }

    /// The empty set; only reachable through the double developer fault
    /// where even the registry invariant diagnostic cannot be built.
    pub fn empty() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    /// The normalized diagnostics in total order.
    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether no diagnostic is present.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// The unique rule ids in normalized order (the derived reason codes).
    pub fn reason_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = Vec::with_capacity(self.diagnostics.len());
        for diagnostic in &self.diagnostics {
            let id = diagnostic.id();
            if ids.last() != Some(&id) {
                ids.push(id);
            }
        }
        ids
    }
}

/// Normalize nested sets inside one diagnostic (data and metadata keys are
/// BTree-sorted by construction; list members sort by UTF-8 bytes).
fn normalize_nested(diagnostic: &mut Diagnostic) {
    for value in diagnostic.data.values_mut() {
        if let super::types::DataValue::List(members) = value {
            super::types::normalize_scalars(members);
        }
    }
}

/// The total diagnostic order: global before located, then logical path,
/// span bytes, line/column tie-breakers, code, id, symbol, and severity
/// rank. Canonical data and nested-payload bytes break remaining ties.
fn sort_key(
    diagnostic: &Diagnostic,
) -> (
    bool,
    String,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    String,
    String,
    String,
    u8,
) {
    let (path, start_byte, end_byte, start_line, start_column, end_line, end_column) = diagnostic
        .source
        .as_ref()
        .map(|source| {
            (
                source.path.clone().unwrap_or_default(),
                source.range.map_or(0, |range| range.start.byte),
                source.range.map_or(0, |range| range.end.byte),
                source.range.map_or(0, |range| range.start.line),
                source.range.map_or(0, |range| range.start.column),
                source.range.map_or(0, |range| range.end.line),
                source.range.map_or(0, |range| range.end.column),
            )
        })
        .unwrap_or_else(|| (String::new(), 0, 0, 0, 0, 0, 0));
    (
        diagnostic.source.is_some(),
        path,
        start_byte,
        end_byte,
        start_line,
        start_column,
        end_line,
        end_column,
        diagnostic.code.as_str().to_owned(),
        diagnostic.id.as_str().to_owned(),
        diagnostic.symbol.clone().unwrap_or_default(),
        severity_rank(diagnostic.severity),
    )
}

fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Info => 0,
        Severity::Warning => 1,
        Severity::Error => 2,
    }
}

/// Construct one source location from optional path and range parts.
pub(crate) fn source_location(
    path: Option<String>,
    range: Option<super::Range>,
) -> Option<SourceLocation> {
    if path.is_none() && range.is_none() {
        return None;
    }
    Some(SourceLocation { path, range })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::types::{Category, DataObject, DataValue, Scalar};
    use crate::diagnostics::FixId;

    fn build_at(
        id: &str,
        path: &str,
        start: (usize, usize, usize),
        end: (usize, usize, usize),
    ) -> Diagnostic {
        let source = source_location(
            Some(path.to_owned()),
            Some(crate::diagnostics::Range {
                start: crate::diagnostics::Position {
                    byte: start.0,
                    line: start.1,
                    column: start.2,
                },
                end: crate::diagnostics::Position {
                    byte: end.0,
                    line: end.1,
                    column: end.2,
                },
            }),
        );
        build(id, None, source, DataObject::new()).expect("registered rule")
    }

    #[test]
    fn registry_backed_build_fills_identity_from_the_entry() {
        let diagnostic = build("cli.usage", None, None, DataObject::new()).expect("builds");
        assert_eq!(diagnostic.id(), "cli.usage");
        assert_eq!(diagnostic.code(), "LEK-CLI-001");
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.category(), Category::Infrastructure);
        assert_eq!(diagnostic.message(), "Malformed command-line syntax.");
    }

    #[test]
    fn unknown_rules_are_refused() {
        assert_eq!(
            build("loader.not-a-rule", None, None, DataObject::new()),
            Err(BuildError::UnknownRule("loader.not-a-rule".to_owned()))
        );
    }

    #[test]
    fn sets_reject_rules_not_allowed_under_the_envelope_status() {
        let diagnostic = build("cli.usage", None, None, DataObject::new()).expect("builds");
        assert!(matches!(
            DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Unsupported),
            Err(SetError::StatusNotAllowed(_))
        ));
    }

    #[test]
    fn sorting_is_global_first_then_path_span_code_and_id() {
        let global = build("cli.usage", None, None, DataObject::new()).expect("global");
        let located_a = build_at("loader.json-parse", "b.yaml", (5, 1, 6), (9, 1, 10));
        let located_b = build_at("loader.encoding", "a.yaml", (0, 1, 1), (1, 1, 2));
        let set = DiagnosticSet::try_from_unsorted(
            vec![located_a, global.clone(), located_b],
            Status::Invalid,
        )
        .expect("set builds");
        let ids: Vec<&str> = set.as_slice().iter().map(|d| d.id()).collect();
        assert_eq!(ids, ["cli.usage", "loader.encoding", "loader.json-parse"]);
    }

    #[test]
    fn exact_duplicates_collapse_but_different_data_stays() {
        let first = build_at("loader.json-parse", "a.yaml", (5, 1, 6), (9, 1, 10));
        let second = build_at("loader.json-parse", "a.yaml", (5, 1, 6), (9, 1, 10));
        let mut different_data = build_at("loader.json-parse", "a.yaml", (5, 1, 6), (9, 1, 10));
        different_data.data.insert(
            "detail".to_owned(),
            DataValue::Token("unexpected-token".to_owned()),
        );
        let set =
            DiagnosticSet::try_from_unsorted(vec![first, second, different_data], Status::Invalid)
                .expect("set builds");
        assert_eq!(set.as_slice().len(), 2, "exact duplicate collapsed");
        let ids = set.reason_ids();
        assert_eq!(ids, ["loader.json-parse"], "reason ids are unique");
    }

    #[test]
    fn message_text_never_affects_identity() {
        let mut first = build_at("loader.json-parse", "a.yaml", (5, 1, 6), (9, 1, 10));
        let second = build_at("loader.json-parse", "a.yaml", (5, 1, 6), (9, 1, 10));
        first.message = "different locale text".to_owned();
        let set = DiagnosticSet::try_from_unsorted(vec![first, second], Status::Invalid)
            .expect("set builds");
        assert_eq!(set.as_slice().len(), 1);
    }

    #[test]
    fn causes_keep_immediate_to_root_order_while_related_and_fixes_sort() {
        let diagnostic = build_at("loader.json-parse", "a.yaml", (5, 1, 6), (9, 1, 10));
        let related = vec![
            RelatedLocation {
                relation: DiagnosticId::new("ir.missing-field").expect("relation"),
                message_id: MessageId::new("ir.missing-field").expect("message"),
                location: SourceLocation {
                    path: Some("z.yaml".to_owned()),
                    range: None,
                },
            },
            RelatedLocation {
                relation: DiagnosticId::new("ir.unknown-field").expect("relation"),
                message_id: MessageId::new("ir.unknown-field").expect("message"),
                location: SourceLocation {
                    path: Some("a.yaml".to_owned()),
                    range: None,
                },
            },
        ];
        let causes = vec![
            DiagnosticCause {
                id: DiagnosticId::new("loader.import-missing").expect("cause id"),
                code: crate::diagnostics::DiagnosticCode::new("LEK-LOAD-010".to_owned())
                    .expect("cause code"),
                message_id: MessageId::new("loader.import-missing").expect("cause message"),
                message: "A declared import names no known module.".to_owned(),
                data: DataObject::new(),
            },
            DiagnosticCause {
                id: DiagnosticId::new("loader.import-cycle").expect("root id"),
                code: crate::diagnostics::DiagnosticCode::new("LEK-LOAD-008".to_owned())
                    .expect("root code"),
                message_id: MessageId::new("loader.import-cycle").expect("root message"),
                message: "Module imports form a cycle.".to_owned(),
                data: DataObject::new(),
            },
        ];
        let fixes = vec![
            SuggestedFix {
                fix_id: FixId::new("loader.fix-b").expect("fix b"),
                applicability: crate::diagnostics::FixApplicability::Breaking,
                message: "restructure".to_owned(),
                target: None,
            },
            SuggestedFix {
                fix_id: FixId::new("loader.fix-a").expect("fix a"),
                applicability: crate::diagnostics::FixApplicability::Safe,
                message: "add the import".to_owned(),
                target: None,
            },
        ];
        let diagnostic = with_fixes(
            with_causes(with_related(diagnostic, related), causes),
            fixes,
        );
        let set = DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Invalid)
            .expect("set builds");
        let rendered = set.as_slice()[0].clone();
        let related_ids: Vec<&str> = rendered
            .related_locations
            .iter()
            .map(|item| item.relation.as_str())
            .collect();
        // Related sort by relation bytes first, then location.
        assert_eq!(related_ids, ["ir.missing-field", "ir.unknown-field"]);
        let cause_ids: Vec<&str> = rendered
            .causes
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(cause_ids, ["loader.import-missing", "loader.import-cycle"]);
        let fix_ids: Vec<&str> = rendered
            .fixes
            .iter()
            .map(|item| item.fix_id.as_str())
            .collect();
        assert_eq!(fix_ids, ["loader.fix-a", "loader.fix-b"]);
    }

    #[test]
    fn nested_lists_normalize_by_unsigned_bytes() {
        let mut diagnostic =
            build("loader.import-cycle", None, None, DataObject::new()).expect("registered");
        diagnostic.data.insert(
            "cycle".to_owned(),
            DataValue::List(vec![
                Scalar::Token("m2".to_owned()),
                Scalar::Token("m1".to_owned()),
            ]),
        );
        let set = DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Invalid)
            .expect("set builds");
        match &set.as_slice()[0].data["cycle"] {
            DataValue::List(members) => {
                let tokens: Vec<&str> = members
                    .iter()
                    .map(|member| match member {
                        Scalar::Token(text) => text.as_str(),
                        _ => panic!("token"),
                    })
                    .collect();
                assert_eq!(tokens, ["m1", "m2"]);
            }
            _ => panic!("list"),
        }
    }
}
