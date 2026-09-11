//! The typed graph facts of the query-model attachment (issue #64).
//!
//! Deterministic node and edge data a graph or context consumer may
//! project: every declared query becomes one node, every read source
//! one `reads` edge, every semantic-link include one `includes` edge
//! carrying its path, every policy reference one `constrained-by`
//! edge, and every scenario reference one `pinned-by` edge. A read
//! effect edge is data only — this module never mutates the #13
//! dependency graph, never infers effects, and never executes.

use std::collections::BTreeSet;

use crate::scenario::id::SemanticId;

use super::QueryModelAttachment;

/// The closed fact-edge kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum FactEdgeKind {
    /// The query reads the entity.
    Reads,
    /// The query joins the entity through a semantic-link path.
    Includes,
    /// The query is constrained by the policy.
    ConstrainedBy,
    /// The query mapping is pinned by the scenario.
    PinnedBy,
}

impl FactEdgeKind {
    /// The exact wire token of the edge kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reads => "reads",
            Self::Includes => "includes",
            Self::ConstrainedBy => "constrained-by",
            Self::PinnedBy => "pinned-by",
        }
    }
}

/// One fact node: a declared query with its result posture.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FactNode {
    /// The query id.
    pub query: SemanticId,
    /// The declared result cardinality.
    pub cardinality: &'static str,
    /// The declared consistency profile.
    pub consistency: &'static str,
    /// Whether managed generation is blocked (foreign mapping).
    pub foreign: bool,
}

/// One fact edge.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FactEdge {
    /// The query id.
    pub from: SemanticId,
    /// The edge kind.
    pub kind: FactEdgeKind,
    /// The target symbol (entity, policy, or scenario).
    pub to: SemanticId,
    /// The dotted relation path (`includes` edges only).
    pub path: Option<String>,
}

/// The finished fact set: deterministic, sorted, and shareable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryGraphFacts {
    nodes: Vec<FactNode>,
    edges: Vec<FactEdge>,
}

impl QueryGraphFacts {
    /// Build the fact set of one attachment. Pure and read-only.
    pub fn build(attachment: &QueryModelAttachment) -> Self {
        let mut nodes: Vec<FactNode> = Vec::new();
        let mut edges: BTreeSet<FactEdge> = BTreeSet::new();
        for decl in attachment.queries() {
            nodes.push(FactNode {
                query: decl.query.clone(),
                cardinality: decl.cardinality.as_str(),
                consistency: decl.consistency.as_str(),
                foreign: decl.foreign.is_some(),
            });
            let mut source_edge = FactEdge {
                from: decl.query.clone(),
                kind: FactEdgeKind::Reads,
                to: decl.source.clone(),
                path: None,
            };
            edges.insert(source_edge.clone());
            for include in &decl.includes {
                source_edge = FactEdge {
                    from: decl.query.clone(),
                    kind: FactEdgeKind::Includes,
                    to: decl.source.clone(),
                    path: Some(super::wire::include_path_key(&include.path)),
                };
                edges.insert(source_edge.clone());
            }
            for policy in &decl.policies {
                edges.insert(FactEdge {
                    from: decl.query.clone(),
                    kind: FactEdgeKind::ConstrainedBy,
                    to: policy.clone(),
                    path: None,
                });
            }
            for scenario in &decl.scenarios {
                edges.insert(FactEdge {
                    from: decl.query.clone(),
                    kind: FactEdgeKind::PinnedBy,
                    to: scenario.clone(),
                    path: None,
                });
            }
        }
        nodes.sort();
        Self {
            nodes,
            edges: edges.into_iter().collect(),
        }
    }

    /// Every fact node, byte-sorted by query id.
    pub fn nodes(&self) -> &[FactNode] {
        &self.nodes
    }

    /// Every fact edge, byte-sorted by (from, kind, to, path).
    pub fn edges(&self) -> &[FactEdge] {
        &self.edges
    }
}

#[cfg(test)]
mod tests {
    use super::super::version;
    use super::*;

    fn attachment(queries: serde_json::Value) -> QueryModelAttachment {
        QueryModelAttachment::from_value(&serde_json::json!({
            "schemaVersion": version::SCHEMA_VERSION,
            "identity": version::IDENTITY,
            "attachmentRevision": "1.0.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "tenancy": [],
            "queries": queries
        }))
        .expect("attachment")
    }

    #[test]
    fn facts_are_sorted_and_complete() {
        let attachment = attachment(serde_json::json!([
            {
                "query": "planner.b_query",
                "source": "planner.task",
                "cardinality": "list",
                "consistency": "strong",
                "policies": ["planner.task_read"],
                "scenarios": ["planner.b_scenario"]
            },
            {
                "query": "planner.a_query",
                "source": "planner.task",
                "cardinality": "list",
                "consistency": "strong",
                "includes": [{"path": ["project"]}]
            }
        ]));
        let facts = QueryGraphFacts::build(&attachment);
        let ids: Vec<&str> = facts
            .nodes()
            .iter()
            .map(|node| node.query.as_str())
            .collect();
        assert_eq!(ids, vec!["planner.a_query", "planner.b_query"]);
        assert_eq!(facts.edges().len(), 5);
        assert_eq!(facts.edges()[0].kind.as_str(), "reads");
        let include = facts
            .edges()
            .iter()
            .find(|edge| edge.kind == FactEdgeKind::Includes)
            .expect("include edge");
        assert_eq!(include.path.as_deref(), Some("project"));
    }
}
