//! Derived forward/reverse trace queries (issue #22).
//!
//! Every query answers from the normalized relation array and its
//! precomputed adjacency — never a re-parse or re-validation. The reverse
//! index is derived, rebuildable data, never a second source of truth.
//! Rows carry the relation status, confidence, and occurrence, sort
//! canonically, and preserve every occurrence, so one symbol implementing
//! several requirements (and vice versa) stays many-to-many.

use serde::Serialize;

use super::diagnostic;
use super::node::NodeKind;
use super::relation::RelationKind;
use super::version;
use super::TraceManifest;

/// The closed query selection vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuerySelection {
    /// Requirements implemented by one symbol (semantic id).
    RequirementsFor(String),
    /// Symbols implementing one requirement (requirement id).
    SymbolsFor(String),
    /// Artifacts bound to one symbol (semantic id).
    ArtifactsFor(String),
    /// Native tests verifying one symbol directly or through a covered
    /// scenario (semantic id).
    TestsFor(String),
    /// Gates evidencing one native test (test id).
    GatesFor(String),
    /// Diagnostics referenced by one gate (gate id).
    DiagnosticsFor(String),
    /// Every explicit gap, in canonical order (projected by the caller
    /// from the manifest; the relation engine has no rows for it).
    Gaps,
}

impl QuerySelection {
    /// Parse one `selector:id` wire spelling; `gaps` takes no id.
    pub fn parse(text: &str) -> Result<Self, super::PathViolation> {
        let (name, rest) = text.split_once(':').unwrap_or((text, ""));
        let id = |value: &str| {
            if super::id::is_semantic_id(value) {
                Ok(value.to_owned())
            } else {
                Err(super::PathViolation("structure.selection-segment"))
            }
        };
        match name {
            "gaps" if rest.is_empty() => Ok(Self::Gaps),
            "requirements-for" => id(rest).map(Self::RequirementsFor),
            "symbols-for" => id(rest).map(Self::SymbolsFor),
            "artifacts-for" => id(rest).map(Self::ArtifactsFor),
            "tests-for" => id(rest).map(Self::TestsFor),
            "gates-for" => id(rest).map(Self::GatesFor),
            "diagnostics-for" => id(rest).map(Self::DiagnosticsFor),
            _ => Err(super::PathViolation("structure.selection-segment")),
        }
    }

    /// The wire spelling of the selector name.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::RequirementsFor(_) => "requirements-for",
            Self::SymbolsFor(_) => "symbols-for",
            Self::ArtifactsFor(_) => "artifacts-for",
            Self::TestsFor(_) => "tests-for",
            Self::GatesFor(_) => "gates-for",
            Self::DiagnosticsFor(_) => "diagnostics-for",
            Self::Gaps => "gaps",
        }
    }

    /// The selector target id, if any.
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::RequirementsFor(target)
            | Self::SymbolsFor(target)
            | Self::ArtifactsFor(target)
            | Self::TestsFor(target)
            | Self::GatesFor(target)
            | Self::DiagnosticsFor(target) => Some(target),
            Self::Gaps => None,
        }
    }

    /// The kind of node the selection starts from.
    fn origin_kind(&self) -> NodeKind {
        match self {
            Self::RequirementsFor(_) | Self::ArtifactsFor(_) | Self::TestsFor(_) => {
                NodeKind::Symbol
            }
            Self::SymbolsFor(_) => NodeKind::Requirement,
            Self::GatesFor(_) => NodeKind::NativeTest,
            Self::DiagnosticsFor(_) => NodeKind::Gate,
            Self::Gaps => NodeKind::Requirement,
        }
    }
}

/// One query answer row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryRow {
    /// The matched node's identity id (requirementId, semanticId, ...).
    pub id: String,
    /// The node id of the matched node.
    pub node_id: String,
    /// The relation kind that produced the match.
    pub relation: RelationKind,
    /// The relation occurrence.
    pub occurrence: String,
    /// The relation status.
    pub status: super::Status,
    /// The relation confidence.
    pub confidence: super::Confidence,
}

/// Execute one selection against the normalized manifest.
pub(super) fn run(
    manifest: &TraceManifest,
    selection: &QuerySelection,
) -> Result<Vec<QueryRow>, crate::diagnostics::DiagnosticSet> {
    let Some(target) = selection.target() else {
        return Ok(Vec::new());
    };
    let origin_kind = selection.origin_kind();
    let known = manifest
        .nodes_of_kind(origin_kind)
        .any(|position| manifest.node_identity(position) == target);
    if !known {
        return Err(diagnostic::unknown_subject_set(target));
    }

    let mut rows: Vec<QueryRow> = Vec::new();
    for position in manifest
        .nodes_of_kind(origin_kind)
        .filter(|&position| manifest.node_identity(position) == target)
    {
        match selection {
            // symbol -implements-> requirement
            QuerySelection::RequirementsFor(_) => {
                for relation in manifest.relations_from(position) {
                    if relation.relation_kind == RelationKind::Implements {
                        push_matched(manifest, relation, relation.to_node.as_str(), &mut rows);
                    }
                }
            }
            // symbol -implements-> requirement, answered in reverse.
            QuerySelection::SymbolsFor(_) => {
                for relation in manifest.relations_to(position) {
                    if relation.relation_kind == RelationKind::Implements {
                        push_matched(manifest, relation, relation.from_node.as_str(), &mut rows);
                    }
                }
            }
            // symbol -binds-> artifact
            QuerySelection::ArtifactsFor(_) => {
                for relation in manifest.relations_from(position) {
                    if relation.relation_kind == RelationKind::Binds {
                        push_matched(manifest, relation, relation.to_node.as_str(), &mut rows);
                    }
                }
            }
            // native_test -verifies-> symbol, plus native_test
            // -verifies-> scenario -covers-> symbol answered in reverse.
            QuerySelection::TestsFor(_) => {
                for relation in manifest.relations_to(position) {
                    match relation.relation_kind {
                        RelationKind::Verifies => {
                            push_matched(
                                manifest,
                                relation,
                                relation.from_node.as_str(),
                                &mut rows,
                            );
                        }
                        RelationKind::Covers => {
                            let Some(scenario_position) =
                                manifest.node_position(&relation.from_node)
                            else {
                                continue;
                            };
                            for verified in manifest.relations_to(scenario_position) {
                                if verified.relation_kind != RelationKind::Verifies {
                                    continue;
                                }
                                let Some(test_position) =
                                    manifest.node_position(&verified.from_node)
                                else {
                                    continue;
                                };
                                if manifest.node(test_position).node_kind != NodeKind::NativeTest {
                                    continue;
                                }
                                rows.push(QueryRow {
                                    id: manifest.node_identity(test_position).to_owned(),
                                    node_id: manifest.node(test_position).node_id.clone(),
                                    relation: RelationKind::Verifies,
                                    occurrence: verified.occurrence.clone(),
                                    status: verified.status,
                                    confidence: verified.confidence,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            // gate -evidences-> native_test, answered in reverse.
            QuerySelection::GatesFor(_) => {
                for relation in manifest.relations_to(position) {
                    if relation.relation_kind == RelationKind::Evidences {
                        push_matched(manifest, relation, relation.from_node.as_str(), &mut rows);
                    }
                }
            }
            // gate -references-> diagnostic
            QuerySelection::DiagnosticsFor(_) => {
                for relation in manifest.relations_from(position) {
                    if relation.relation_kind == RelationKind::References {
                        push_matched(manifest, relation, relation.to_node.as_str(), &mut rows);
                    }
                }
            }
            QuerySelection::Gaps => {}
        }
    }
    if rows.len() > version::MAX_RESULT_ROWS {
        return Err(diagnostic::traversal_limit_set("result-rows"));
    }
    rows.sort_by(|left, right| {
        (
            left.id.as_bytes(),
            left.occurrence.as_bytes(),
            left.node_id.as_bytes(),
        )
            .cmp(&(
                right.id.as_bytes(),
                right.occurrence.as_bytes(),
                right.node_id.as_bytes(),
            ))
    });
    Ok(rows)
}

/// Record one matched endpoint row from `relation`.
fn push_matched(
    manifest: &TraceManifest,
    relation: &super::Relation,
    matched_node: &str,
    rows: &mut Vec<QueryRow>,
) {
    let Some(position) = manifest.node_position(matched_node) else {
        return;
    };
    rows.push(QueryRow {
        id: manifest.node_identity(position).to_owned(),
        node_id: manifest.node(position).node_id.clone(),
        relation: relation.relation_kind,
        occurrence: relation.occurrence.clone(),
        status: relation.status,
        confidence: relation.confidence,
    });
}
