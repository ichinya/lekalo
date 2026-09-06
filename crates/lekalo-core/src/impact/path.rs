//! Bounded explanation paths for impact results (issue #16).
//!
//! A path is the semantic discovery chain from one root to one affected
//! subject: the ordered canonical edge keys of the #13 graph with their
//! relations, the confidence meet of the contributing edges, and the
//! provenance kinds met along the way. No physical path, source span, or
//! filesystem reference ever enters. Path identities are opaque bounded
//! digests derived only from canonical content.

use std::collections::HashMap;

use crate::graph::model::{Confidence, EdgeProvenance, GraphEdge, NodeId, RelationKindId};

use super::version::MAX_PROVENANCE_RECORDS;

/// Reconstruct the discovery chain from the parent map, ordered root to
/// subject. The parent map records the first (canonical order) discovery
/// edge of every node, so the chain is deterministic. The result is the
/// ordered edge keys, the distinct relation kinds in walk order, and the
/// confidence meet of the contributing edges.
pub fn reconstruct(
    parents: &HashMap<NodeId, GraphEdge>,
    subject: &NodeId,
    root: &NodeId,
) -> (Vec<String>, Vec<RelationKindId>, Confidence) {
    let mut keys = Vec::new();
    let mut edges = Vec::new();
    let mut cursor = subject.clone();
    let mut confidence = Confidence::Canonical;
    while cursor != *root && keys.len() < super::version::MAX_PATH_EDGES {
        let Some(parent_edge) = parents.get(&cursor) else {
            break;
        };
        confidence = confidence.meet(parent_edge.confidence());
        keys.push(parent_edge.key().to_canonical_string());
        edges.push(parent_edge.clone());
        // The parent edge discovered `cursor` from its `to` side: the
        // chain advances toward the discovery root through `to`.
        cursor = parent_edge.key().to().clone();
    }
    keys.reverse();
    edges.reverse();
    let mut relation_kinds: Vec<RelationKindId> = Vec::new();
    for edge in &edges {
        let relation = edge.key().relation();
        if !relation_kinds.contains(&relation) {
            relation_kinds.push(relation);
        }
    }
    (keys, relation_kinds, confidence)
}

/// The bounded opaque path identity: the first 16 hex characters of the
/// SHA-256 over the canonical chain.
pub fn path_id(root: &NodeId, subject: &NodeId, ordered_edges: &[String]) -> String {
    let mut canonical = String::new();
    canonical.push_str(root.as_str());
    canonical.push('>');
    canonical.push_str(subject.as_str());
    for edge in ordered_edges {
        canonical.push('|');
        canonical.push_str(edge);
    }
    let hex = crate::versioning::plan::sha256_hex(canonical.as_bytes());
    format!("impact.path.{}", &hex[..16])
}

/// The provenance kinds met along one edge chain, canonical order,
/// bounded.
pub fn provenance_refs(edges: &[GraphEdge]) -> Vec<super::ProvenanceKind> {
    let mut kinds: Vec<super::ProvenanceKind> = Vec::new();
    for edge in edges {
        let kind = match edge.provenance() {
            EdgeProvenance::CanonicalIr { .. } => super::ProvenanceKind::CanonicalIr,
            EdgeProvenance::AdapterEvidence { .. } => super::ProvenanceKind::Evidence,
            EdgeProvenance::Derived { .. } => super::ProvenanceKind::Derived,
        };
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        if kinds.len() == MAX_PROVENANCE_RECORDS {
            break;
        }
    }
    if kinds.is_empty() {
        kinds.push(super::ProvenanceKind::CanonicalIr);
    }
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_ids_are_stable_and_bounded() {
        let root = NodeId::from_qualified("entity:mod.a").expect("valid");
        let subject = NodeId::from_qualified("operation:mod.b").expect("valid");
        let edges = vec!["operation:mod.b|references|entity:mod.a|0".to_owned()];
        assert_eq!(
            path_id(&root, &subject, &edges),
            path_id(&root, &subject, &edges)
        );
        let id = path_id(&root, &subject, &edges);
        assert!(id.starts_with("impact.path."));
        assert_eq!(id.len(), "impact.path.".len() + 16);
        assert_ne!(path_id(&root, &subject, &[]), id);
    }
    #[test]
    fn reconstruction_walks_to_the_root() {
        let root = NodeId::from_qualified("entity:mod.a").expect("valid");
        let middle = NodeId::from_qualified("effect:mod.mid").expect("valid");
        let subject = NodeId::from_qualified("operation:mod.b").expect("valid");
        let mut parents = HashMap::new();
        // parents[X] is the incoming edge of the discoverer that found X:
        // `from` is the dependent, `to` the already-visited side. The
        // chain from the subject advances through `to` toward the root.
        let edge_to_root = GraphEdge::new(
            crate::graph::model::EdgeKey::new(
                middle.clone(),
                root.clone(),
                RelationKindId::REFERENCES,
                crate::graph::model::OccurrenceOrdinal::new(0).expect("valid"),
            ),
            crate::graph::model::EdgeProvenance::CanonicalIr {
                reference_role: crate::graph::model::ReferenceRole::CommandEffect,
                occurrence: crate::graph::model::OccurrenceOrdinal::new(0).expect("valid"),
                source_symbol: "mod.b".to_owned(),
            },
        );
        let edge_to_middle = GraphEdge::new(
            crate::graph::model::EdgeKey::new(
                subject.clone(),
                middle.clone(),
                RelationKindId::REFERENCES,
                crate::graph::model::OccurrenceOrdinal::new(1).expect("valid"),
            ),
            crate::graph::model::EdgeProvenance::AdapterEvidence {
                adapter_id: "test.adapter/edge".to_owned(),
                target_id: "test.target/edge".to_owned(),
                protocol_version: "1.0.0".to_owned(),
                evidence_digest: format!("sha256:{}", "0".repeat(64)),
                evidence_status: "fresh".to_owned(),
            },
        );
        parents.insert(middle.clone(), edge_to_root);
        parents.insert(subject.clone(), edge_to_middle);
        let (keys, relations, confidence) = reconstruct(&parents, &subject, &root);
        assert_eq!(keys[0], "effect:mod.mid|references|entity:mod.a|0");
        assert_eq!(keys[1], "operation:mod.b|references|effect:mod.mid|1");
        assert_eq!(relations, vec![RelationKindId::REFERENCES]);
        assert_eq!(confidence, Confidence::Verified);
    }
}
