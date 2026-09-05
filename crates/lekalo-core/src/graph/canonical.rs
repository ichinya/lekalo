//! Canonical serialization of the graph and its query payloads (issue #13).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, object keys in
//! unsigned UTF-8 byte order, nodes and edges in their canonical order.
//! The bytes are path-independent: no physical root, raw source, timestamp,
//! host, locale, target implementation body, or adapter transcript ever
//! enters them. Equivalent frontends and module-directory orders produce
//! byte-identical exports. The CLI adds exactly one trailing LF; these
//! functions never do.

use std::collections::HashMap;

use super::diagnostic;
use super::model::{EdgeProvenance, GraphEdge, GraphNode};
use super::{DependencyGraph, MAX_EXPORT_BYTES};
use crate::loader::canonical::write_json_string;
use crate::loader::SourceMapEntry;

/// Serialize the whole graph to canonical bytes, or refuse beyond the
/// export bound.
pub fn graph_bytes(graph: &DependencyGraph) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let mut fields: Vec<(&'static str, String)> = Vec::with_capacity(7);
    fields.push(("schemaVersion", string(graph.schema_version())));
    fields.push(("identity", string(graph.identity())));
    fields.push(("modelVersion", string(graph.model_version().as_str())));
    if let Some(project) = graph.project_id() {
        fields.push(("project", string(project)));
    }
    fields.push((
        "metadata",
        object(vec![
            ("edgeCount", number(graph.edges().len() as u64)),
            ("nodeCount", number(graph.nodes().len() as u64)),
        ]),
    ));
    fields.push(("nodes", array(graph.nodes().iter().map(node_bytes))));
    fields.push(("edges", array(graph.edges().iter().map(edge_bytes))));
    let out = object(fields);
    if out.len() > MAX_EXPORT_BYTES {
        return Err(diagnostic::export_limit_set(out.len()));
    }
    Ok(out)
}

/// The canonical bytes of one node.
pub fn node_bytes(node: &GraphNode) -> String {
    let mut fields = vec![
        ("id", string(node.id().as_str())),
        ("kind", string(node.kind().key())),
    ];
    if let Some(module) = node.module() {
        fields.push(("module", string(module)));
    }
    if let Some(subkind) = node.subkind() {
        fields.push(("subkind", string(subkind)));
    }
    object(fields)
}

/// The canonical bytes of one edge.
pub fn edge_bytes(edge: &GraphEdge) -> String {
    let key = edge.key();
    object(vec![
        ("confidence", string(edge.confidence().as_str())),
        ("from", string(key.from().as_str())),
        ("occurrence", number(key.occurrence().get() as u64)),
        ("provenance", provenance_bytes(edge.provenance())),
        ("relation", string(key.relation().key())),
        ("to", string(key.to().as_str())),
    ])
}

/// The canonical bytes of one provenance record.
pub fn provenance_bytes(provenance: &EdgeProvenance) -> String {
    match provenance {
        EdgeProvenance::CanonicalIr {
            reference_role,
            occurrence,
            source_symbol,
        } => object(vec![
            ("occurrence", number(occurrence.get() as u64)),
            ("referenceRole", string(reference_role.as_str())),
            ("sourceSymbol", string(source_symbol)),
        ]),
        EdgeProvenance::AdapterEvidence {
            adapter_id,
            target_id,
            protocol_version,
            evidence_digest,
            evidence_status,
        } => object(vec![
            ("adapterId", string(adapter_id)),
            ("evidenceDigest", string(evidence_digest)),
            ("evidenceStatus", string(evidence_status)),
            ("protocolVersion", string(protocol_version)),
            ("targetId", string(target_id)),
        ]),
        EdgeProvenance::Derived {
            algorithm_id,
            algorithm_version,
            parent_edge_keys,
        } => object(vec![
            ("algorithmId", string(algorithm_id)),
            ("algorithmVersion", string(algorithm_version)),
            (
                "parentEdgeKeys",
                array(parent_edge_keys.iter().map(|key| string(key))),
            ),
        ]),
    }
}

/// Render one node card with its direct dependencies and dependents as the
/// `graph show` payload bytes (the envelope wrapper stays with the CLI).
pub fn show_payload_bytes(
    node: &GraphNode,
    dependencies: &[&GraphEdge],
    dependents: &[&GraphEdge],
) -> String {
    object(vec![
        (
            "dependencies",
            array(dependencies.iter().map(|edge| edge_bytes(edge))),
        ),
        (
            "dependents",
            array(dependents.iter().map(|edge| edge_bytes(edge))),
        ),
        ("node", node_bytes(node)),
    ])
}

/// Render the reverse-dependency payload of `graph callers`.
pub fn callers_payload_bytes(node: &GraphNode, callers: &[GraphEdge], complete: bool) -> String {
    object(vec![
        ("callers", array(callers.iter().map(edge_bytes))),
        ("complete", boolean(complete)),
        ("node", node_bytes(node)),
    ])
}

/// Render the shortest-path payload of `graph path`.
pub fn path_payload_bytes(path: &super::Path) -> String {
    object(vec![
        ("confidence", string(path.confidence().as_str())),
        ("complete", boolean(path.complete())),
        ("edges", array(path.edges().iter().map(edge_bytes))),
        ("from", string(path.from().as_str())),
        ("length", number(path.length() as u64)),
        (
            "nodes",
            array(path.nodes().iter().map(|id| string(id.as_str()))),
        ),
        ("to", string(path.to().as_str())),
    ])
}

/// Render one bounded traversal payload.
pub fn traversal_payload_bytes(traversal: &super::Traversal) -> String {
    object(vec![
        ("complete", boolean(traversal.complete())),
        ("direction", string(traversal.direction().as_str())),
        ("edges", array(traversal.edges().iter().map(edge_bytes))),
        ("maxDepthSeen", number(traversal.max_depth_seen() as u64)),
        (
            "nodes",
            array(traversal.nodes().iter().map(|id| string(id.as_str()))),
        ),
        ("root", string(traversal.root().as_str())),
    ])
}

/// An unsigned integer value.
fn number(value: u64) -> String {
    value.to_string()
}

/// A boolean value.
fn boolean(value: bool) -> String {
    if value {
        "true".to_owned()
    } else {
        "false".to_owned()
    }
}

/// A quoted JSON string value (the writer adds the surrounding quotes).
fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    write_json_string(text, &mut out);
    out
}

/// The declaration-span sidecar: for every node whose semantic id has a
/// declaration entry in the accepted #8 source map, the first (canonical
/// order) logical path and half-open range. The lookup is kind-aware so a
/// project and a module sharing one id never cross: project entries carry
/// the `/project` pointer, module entries a `/modules/` pointer.
/// Requirements and unanchored nodes are absent; no physical path enters.
pub fn spans_sidecar_bytes(graph: &DependencyGraph, entries: &[SourceMapEntry]) -> String {
    let mut by_id: HashMap<&str, Vec<&SourceMapEntry>> = HashMap::new();
    for entry in entries {
        if let Some(id) = &entry.semantic_id {
            by_id.entry(id.as_str()).or_default().push(entry);
        }
    }
    let nodes: Vec<String> = graph
        .nodes()
        .iter()
        .filter_map(|node| {
            let candidates = by_id.get(node.id().semantic_id())?;
            let entry = candidates
                .iter()
                .copied()
                .find(|entry| entry_pointer_matches(node.kind(), &entry.pointer))?;
            Some(object(vec![
                ("id", string(node.id().as_str())),
                (
                    "span",
                    object(vec![
                        ("end", position_bytes(entry.end)),
                        ("path", string(&entry.path)),
                        ("start", position_bytes(entry.start)),
                    ]),
                ),
            ]))
        })
        .collect();
    object(vec![("nodes", array(nodes))])
}

/// Whether one source-map entry can declare a node of this kind.
fn entry_pointer_matches(kind: super::model::NodeKindId, pointer: &str) -> bool {
    match kind {
        super::model::NodeKindId::PROJECT => pointer == "/project",
        super::model::NodeKindId::MODULE => pointer.starts_with("/modules/"),
        _ => !pointer.starts_with("/modules/"),
    }
}

/// One span endpoint: `{byte, column, line}` in byte-sorted key order.
/// A JSON array from an ordered iterator.
fn array(items: impl IntoIterator<Item = String>) -> String {
    let mut out = String::from("[");
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&item);
    }
    out.push(']');
    out
}

/// Emit one object with byte-sorted keys; a duplicated key is a
/// programming error and panics in debug builds.
fn object(mut fields: Vec<(&'static str, String)>) -> String {
    fields.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut out = String::from("{");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(key);
        out.push_str("\":");
        out.push_str(value);
    }
    out.push('}');
    out
}

fn position_bytes(position: crate::loader::Position) -> String {
    object(vec![
        ("byte", number(position.byte as u64)),
        ("column", number(position.column as u64)),
        ("line", number(position.line as u64)),
    ])
}
