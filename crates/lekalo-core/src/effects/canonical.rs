//! Canonical serialization of the effect graph and its query payloads
//! (issue #14).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, object keys in
//! unsigned UTF-8 byte order, effect edges in their canonical order. The
//! bytes are path-independent: no physical root, raw source, timestamp,
//! host, runtime value, or adapter transcript ever enters them. The CLI
//! adds exactly one trailing LF; these functions never do.

use super::diagnostic;
use super::edge::EffectEdge;
use super::provenance::quote;
use super::{
    Comparison, ComparisonItem, ConflictItem, ConflictReport, EffectGraph, SubjectSelector,
    MAX_EXPORT_BYTES,
};

/// Serialize the whole effect graph to canonical bytes, or refuse beyond
/// the export bound.
pub fn effects_bytes(graph: &EffectGraph) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let mut json = String::from("{\"effects\":[");
    let mut first = true;
    for edge in graph.declared().iter().chain(graph.detected().iter()) {
        if !first {
            json.push(',');
        }
        first = false;
        json.push_str(&edge_bytes(edge));
    }
    let project = match graph.project_id() {
        Some(project) => quote(project),
        None => "null".to_owned(),
    };
    json.push_str(&format!(
        "],\"identity\":{},\"metadata\":{{\"declaredCount\":{},\"detectedCount\":{},\"effectCount\":{},\"envelopeCount\":{},\"operationCount\":{}}},\"modelVersion\":{},\"project\":{},\"schemaVersion\":{}}}",
        quote(super::version::IDENTITY),
        graph.declared().len(),
        graph.detected().len(),
        graph.declared().len() + graph.detected().len(),
        graph.envelope_count(),
        graph.operation_count(),
        quote(graph.model_version().as_str()),
        project,
        quote(super::version::SCHEMA_VERSION),
    ));
    if json.len() > MAX_EXPORT_BYTES {
        return Err(diagnostic::export_limit_set(json.len()));
    }
    Ok(json)
}

/// The canonical bytes of one effect edge.
pub fn edge_bytes(edge: &EffectEdge) -> String {
    let key = edge.key();
    let mut json = String::from("{\"action\":");
    json.push_str(&match key.kind().action() {
        Some(action) => quote(action.key()),
        None => "null".to_owned(),
    });
    json.push_str(",\"confidence\":");
    json.push_str(&quote(edge.confidence().as_str()));
    json.push_str(",\"effect\":");
    json.push_str(&quote(&key.origin().to_canonical_string()));
    json.push_str(",\"field\":");
    json.push_str(&match key.subject().field() {
        Some(field) => quote(field.as_str()),
        None => "null".to_owned(),
    });
    json.push_str(",\"kind\":");
    json.push_str(&quote(key.kind().key()));
    json.push_str(",\"occurrence\":");
    json.push_str(&key.occurrence().to_string());
    json.push_str(",\"operation\":");
    json.push_str(&quote(key.operation().as_str()));
    json.push_str(",\"provenance\":");
    json.push_str(&edge.provenance().to_canonical_bytes());
    json.push_str(",\"resource\":");
    json.push_str(&resource_bytes(key.subject().resource()));
    json.push_str(",\"sensitivity\":");
    json.push_str(&match edge.sensitivity() {
        Some(marker) => format!(
            "{{\"contract\":{},\"state\":{}}}",
            quote(marker.contract()),
            quote(marker.state())
        ),
        None => "null".to_owned(),
    });
    json.push_str(",\"transactionGroup\":");
    json.push_str(&match edge.transaction_group() {
        Some(group) => quote(group.as_str()),
        None => "null".to_owned(),
    });
    json.push('}');
    json
}

/// The canonical bytes of one resource identity.
fn resource_bytes(resource: &super::identity::ResourceId) -> String {
    format!(
        "{{\"id\":{},\"kind\":{}}}",
        quote(resource.as_str()),
        quote(resource.kind().key())
    )
}

/// The `effects show` payload bytes (the envelope wrapper stays with the
/// CLI): the operation card plus its edges and explicit completeness.
pub fn show_payload_bytes(
    graph: &EffectGraph,
    operation: &super::identity::OperationId,
) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let edges = graph.operation_edges(operation);
    let complete = edges.len() <= super::version::MAX_RESULT_EDGES;
    let mut json = String::from("{\"complete\":");
    json.push_str(if complete { "true" } else { "false" });
    json.push_str(",\"edges\":[");
    let mut first = true;
    for edge in edges.iter().take(super::version::MAX_RESULT_EDGES) {
        if !first {
            json.push(',');
        }
        first = false;
        json.push_str(&edge_bytes(edge));
    }
    json.push_str(&format!(
        "],\"identity\":{},\"operation\":{}}}",
        quote(super::version::IDENTITY),
        quote(operation.as_str())
    ));
    Ok(json)
}

/// The `effects writers` payload bytes: the reverse-writer edges for one
/// subject selector, answered from the precomputed index.
pub fn writers_payload_bytes(
    graph: &EffectGraph,
    selector: &SubjectSelector,
    readers: bool,
) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let edges = if readers {
        graph.readers(selector)?
    } else {
        graph.writers(selector)?
    };
    let mut json = String::from("{\"complete\":true");
    json.push_str(",\"identity\":");
    json.push_str(&quote(super::version::IDENTITY));
    json.push_str(",\"scope\":");
    json.push_str(&quote(&selector.scope_key()));
    json.push_str(",\"subject\":");
    json.push_str(&subject_bytes(selector));
    json.push_str(",\"view\":");
    json.push_str(&quote(if readers { "readers" } else { "writers" }));
    json.push_str(",\"edges\":[");
    let mut first = true;
    for edge in &edges {
        if !first {
            json.push(',');
        }
        first = false;
        json.push_str(&edge_bytes(edge));
    }
    json.push_str("]}");
    Ok(json)
}

/// The canonical bytes of one subject selector.
fn subject_bytes(selector: &SubjectSelector) -> String {
    let resource = selector.subject().resource();
    match selector.subject().field() {
        Some(field) => format!(
            "{{\"field\":{},\"resource\":{}}}",
            quote(field.as_str()),
            resource_bytes(resource)
        ),
        None => format!(
            "{{\"field\":null,\"resource\":{}}}",
            resource_bytes(resource)
        ),
    }
}

/// The `effects conflicts` payload bytes: the classified pairs for one
/// explicit change set.
pub fn conflicts_payload_bytes(
    report: &ConflictReport,
) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let mut json = String::from("{\"changed\":[");
    let mut changed: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut first = true;
    for item in report.items() {
        for operation in [item.left(), item.right()] {
            if changed.insert(operation.as_str()) {
                if !first {
                    json.push(',');
                }
                first = false;
                json.push_str(&quote(operation.as_str()));
            }
        }
    }
    json.push_str("],\"complete\":");
    json.push_str(if report.complete() { "true" } else { "false" });
    json.push_str(",\"conflicts\":[");
    let mut first = true;
    for item in report.items() {
        if !first {
            json.push(',');
        }
        first = false;
        json.push_str(&conflict_item_bytes(item));
    }
    json.push(']');
    if let Some(reason) = report.bounded_reason() {
        json.push_str(",\"boundedReason\":");
        json.push_str(&quote(reason));
    }
    json.push('}');
    Ok(json)
}

/// The canonical bytes of one conflict item.
fn conflict_item_bytes(item: &ConflictItem) -> String {
    let mut json = String::from("{\"classification\":");
    json.push_str(&quote(item.classification().key()));
    json.push_str(",\"explanation\":");
    json.push_str(&quote(item.explanation()));
    json.push_str(",\"left\":");
    json.push_str(&quote(item.left().as_str()));
    json.push_str(",\"leftKey\":");
    json.push_str(&quote(item.left_key()));
    json.push_str(",\"right\":");
    json.push_str(&quote(item.right().as_str()));
    json.push_str(",\"rightKey\":");
    json.push_str(&quote(item.right_key()));
    json.push_str(",\"subject\":");
    json.push_str(&quote(item.subject()));
    json.push('}');
    json
}

/// The comparison payload bytes: every classified item with its
/// explanation and the explicit completeness flags.
pub fn comparison_payload_bytes(comparison: &Comparison) -> String {
    let mut json = String::from("{\"complete\":");
    json.push_str(if comparison.complete() {
        "true"
    } else {
        "false"
    });
    json.push_str(",\"items\":[");
    let mut first = true;
    for item in comparison.items() {
        if !first {
            json.push(',');
        }
        first = false;
        json.push_str(&comparison_item_bytes(item));
    }
    json.push(']');
    if let Some(reason) = comparison.bounded_reason() {
        json.push_str(",\"boundedReason\":");
        json.push_str(&quote(reason));
    }
    json.push('}');
    json
}

/// The canonical bytes of one comparison item.
fn comparison_item_bytes(item: &ComparisonItem) -> String {
    let mut json = String::from("{\"declaredConfidence\":");
    json.push_str(&match item.declared_confidence() {
        Some(confidence) => quote(confidence.as_str()),
        None => "null".to_owned(),
    });
    json.push_str(",\"detectedConfidence\":");
    json.push_str(&match item.detected_confidence() {
        Some(confidence) => quote(confidence.as_str()),
        None => "null".to_owned(),
    });
    json.push_str(",\"explanation\":");
    json.push_str(&quote(item.explanation()));
    json.push_str(",\"key\":");
    json.push_str(&quote(item.key()));
    json.push_str(",\"operation\":");
    json.push_str(&quote(item.operation().as_str()));
    json.push_str(",\"state\":");
    json.push_str(&quote(item.state().key()));
    json.push('}');
    json
}
