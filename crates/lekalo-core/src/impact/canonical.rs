//! Canonical serialization and digesting of impact results (issue #16).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, object keys in the
//! frozen schema order (not byte order — the impact contract freezes its
//! field order through the schema). Set-like arrays sort by unsigned
//! UTF-8 of their typed keys; semantically ordered arrays (explanation
//! edge chains) preserve their accepted order. The bytes are path
//! independent: no physical root, raw source, patch text, timestamp,
//! host, locale, repository identity, or adapter transcript ever enters.
//! The digest is the SHA-256 over the canonical bytes with the `digest`
//! field set to the empty string; the CLI adds exactly one trailing LF.

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::version::MAX_EXPORT_BYTES;
use super::{ImpactResult, SectionSummary};

/// The canonical bytes of one finished result (the real digest included).
pub fn impact_bytes(result: &ImpactResult) -> Result<String, DiagnosticSet> {
    render(result, &result.digest)
}

/// The canonical digest of one result: SHA-256 over the canonical bytes
/// with an empty digest field.
pub fn digest_of(result: &ImpactResult) -> String {
    let bytes = render(result, "").unwrap_or_default();
    format!(
        "sha256:{}",
        crate::versioning::plan::sha256_hex(bytes.as_bytes())
    )
}

fn render(result: &ImpactResult, digest: &str) -> Result<String, DiagnosticSet> {
    let input = object(vec![
        field_raw("mode", quote(result.input_mode.key())),
        field_raw(
            "baseRevisionRef",
            optional(result.base_revision_ref.as_deref().map(quote)),
        ),
        field_raw(
            "candidateRevisionRef",
            optional(result.candidate_revision_ref.as_deref().map(quote)),
        ),
        field_raw("changedInputDigest", quote(&result.changed_input_digest)),
    ]);
    let request = object(vec![
        field_raw("depth", result.request.depth().to_string()),
        field_raw("relations", string_array(result.request.relations())),
        field_raw("kinds", string_array(result.request.kinds())),
        field_raw("module", optional(result.request.module().map(quote))),
        field_raw("target", optional(result.request.target().map(quote))),
        field_raw("profile", quote(result.request.profile().key())),
    ]);
    let mut out = object(vec![
        field_raw("schemaVersion", quote(super::version::SCHEMA_VERSION)),
        field_raw("identity", quote(super::version::IDENTITY)),
        field_raw("algorithm", quote(super::version::ALGORITHM)),
        field_raw("modelVersion", quote(result.model_version.as_str())),
        field_raw("project", optional(result.project.as_deref().map(quote))),
        field_raw("input", input),
        field_raw("request", request),
        field_raw(
            "roots",
            array(result.roots.iter().map(|root| quote(root.as_str()))),
        ),
        field_raw("direct", impact_section(&result.direct)),
        field_raw("transitive", impact_section(&result.transitive)),
        field_raw("mandatoryPublic", impact_section(&result.mandatory_public)),
        field_raw("risks", risk_section(&result.risks)),
        field_raw("targets", target_section(&result.targets)),
        field_raw("artifacts", named_section(&result.artifacts)),
        field_raw("scenarios", named_section(&result.scenarios)),
        field_raw("tests", named_section(&result.tests)),
        field_raw("gates", gate_section(&result.gates)),
        field_raw(
            "explanations",
            array(result.explanations.iter().map(explanation_bytes)),
        ),
        field_raw("evidence", evidence_section(&result.evidence)),
        field_raw("completeness", summary_object(&result.completeness)),
        field_raw("diagnosticRefs", string_array(&result.diagnostic_refs)),
        field_raw("digest", quote(digest)),
    ]);
    out.push('\u{0}');
    let bytes = out.trim_end_matches('\u{0}');
    if bytes.len() > MAX_EXPORT_BYTES {
        return Err(diagnostic::output_limit_set(bytes.len()));
    }
    Ok(bytes.to_owned())
}

/// One affected-item object in schema order.
fn item_bytes(item: &super::ImpactItem) -> String {
    object(vec![
        field_raw("subject", quote(item.subject.as_str())),
        field_raw("scope", quote(item.scope.key())),
        field_raw("distance", item.distance.to_string()),
        field_raw("reasonRefs", string_array(&item.reason_refs)),
        field_raw("pathRefs", string_array(&item.path_refs)),
        field_raw("evidenceState", quote(item.evidence_state.key())),
        field_raw("confidence", quote(item.confidence.as_str())),
        field_raw(
            "riskRefs",
            string_array(
                &item
                    .risk_refs
                    .iter()
                    .map(|dimension| dimension.key().to_owned())
                    .collect::<Vec<_>>(),
            ),
        ),
        field_raw("requiredGateRefs", string_array(&item.required_gate_refs)),
    ])
}

fn impact_section(section: &super::ImpactSection) -> String {
    let mut fields = Vec::with_capacity(9);
    fields.push(field_raw("state", quote(section.summary.state.key())));
    fields.push(field_raw("complete", bool_bytes(section.summary.complete)));
    fields.push(field_raw("returned", section.summary.returned.to_string()));
    fields.push(field_raw("omitted", section.summary.omitted.to_string()));
    fields.push(field_raw("frontier", section.summary.frontier.to_string()));
    fields.push(field_raw(
        "reasonRefs",
        string_array(&section.summary.reason_refs),
    ));
    fields.push(field_raw(
        "provenance",
        array(
            section
                .summary
                .provenance
                .iter()
                .map(|provenance| quote(provenance.key())),
        ),
    ));
    fields.push(field_raw(
        "confidence",
        quote(section.summary.confidence.as_str()),
    ));
    if !section.items.is_empty() {
        fields.push(field_raw(
            "items",
            array(section.items.iter().map(item_bytes)),
        ));
    }
    object(fields)
}

fn risk_bytes(risk: &super::RiskItem) -> String {
    object(vec![
        field_raw("dimension", quote(risk.dimension.key())),
        field_raw("state", quote(risk.state.key())),
        field_raw(
            "subjectRefs",
            array(
                risk.subject_refs
                    .iter()
                    .map(|subject| quote(subject.as_str())),
            ),
        ),
        field_raw("reasonRefs", string_array(&risk.reason_refs)),
        field_raw("evidenceRefs", string_array(&risk.evidence_refs)),
        field_raw("required", bool_bytes(risk.required)),
        field_raw("confidence", quote(risk.confidence.as_str())),
    ])
}

fn risk_section(section: &super::RiskSection) -> String {
    section_section(&section.summary, &section.items, risk_bytes)
}

fn target_bytes(target: &super::TargetItem) -> String {
    object(vec![
        field_raw("binding", quote(target.binding.as_str())),
        field_raw("target", quote(&target.target)),
        field_raw("reasonRefs", string_array(&target.reason_refs)),
        field_raw("confidence", quote(target.confidence.as_str())),
    ])
}

fn target_section(section: &super::TargetSection) -> String {
    section_section(&section.summary, &section.items, target_bytes)
}

fn named_bytes(item: &super::NamedItem) -> String {
    object(vec![
        field_raw("id", quote(item.id.as_str())),
        field_raw("state", quote(item.state.key())),
        field_raw("reasonRefs", string_array(&item.reason_refs)),
        field_raw("confidence", quote(item.confidence.as_str())),
    ])
}

fn named_section(section: &super::NamedSection) -> String {
    section_section(&section.summary, &section.items, named_bytes)
}

fn gate_bytes(gate: &super::GateItem) -> String {
    object(vec![
        field_raw("gateId", quote(&gate.gate_id)),
        field_raw("owner", quote(gate.owner)),
        field_raw("reasonRefs", string_array(&gate.reason_refs)),
        field_raw("required", bool_bytes(gate.required)),
        field_raw("state", quote(gate.state.key())),
        field_raw("evidenceState", quote(gate.evidence_state.key())),
        field_raw("confidence", quote(gate.confidence.as_str())),
    ])
}

fn gate_section(section: &super::GateSection) -> String {
    section_section(&section.summary, &section.items, gate_bytes)
}

fn explanation_bytes(path: &super::ExplanationPath) -> String {
    object(vec![
        field_raw("pathId", quote(&path.path_id)),
        field_raw("root", quote(path.root.as_str())),
        field_raw("subject", quote(path.subject.as_str())),
        field_raw(
            "orderedEdges",
            array(path.ordered_edges.iter().map(|edge| quote(edge))),
        ),
        field_raw(
            "relationKinds",
            array(
                path.relation_kinds
                    .iter()
                    .map(|relation| quote(relation.key())),
            ),
        ),
        field_raw("confidence", quote(path.confidence.as_str())),
        field_raw(
            "provenanceRefs",
            array(
                path.provenance_refs
                    .iter()
                    .map(|provenance| quote(provenance.key())),
            ),
        ),
    ])
}

fn evidence_bytes(surface: &super::EvidenceSurface) -> String {
    object(vec![
        field_raw("surface", quote(surface.surface.key())),
        field_raw("state", quote(surface.state.key())),
        field_raw("reasonRefs", string_array(&surface.reason_refs)),
        field_raw("confidence", quote(surface.confidence.as_str())),
    ])
}

fn evidence_section(section: &super::EvidenceSection) -> String {
    section_section(&section.summary, &section.items, evidence_bytes)
}

/// A generic section: summary fields in schema order, then items.
fn section_section<T>(
    summary: &SectionSummary,
    items: &[T],
    render_item: fn(&T) -> String,
) -> String {
    let mut fields = section_summary_fields(summary);
    if !items.is_empty() {
        fields.push(field_raw("items", array(items.iter().map(render_item))));
    }
    object(fields)
}

fn section_summary_fields(summary: &SectionSummary) -> Vec<String> {
    vec![
        field_raw("state", quote(summary.state.key())),
        field_raw("complete", bool_bytes(summary.complete)),
        field_raw("returned", summary.returned.to_string()),
        field_raw("omitted", summary.omitted.to_string()),
        field_raw("frontier", summary.frontier.to_string()),
        field_raw("reasonRefs", string_array(&summary.reason_refs)),
        field_raw(
            "provenance",
            array(
                summary
                    .provenance
                    .iter()
                    .map(|provenance| quote(provenance.key())),
            ),
        ),
        field_raw("confidence", quote(summary.confidence.as_str())),
    ]
}

fn summary_object(summary: &SectionSummary) -> String {
    object(section_summary_fields(summary))
}

fn bool_bytes(value: bool) -> String {
    if value {
        "true".to_owned()
    } else {
        "false".to_owned()
    }
}

fn field_raw(name: &str, value: String) -> String {
    format!("{}:{}", quote(name), value)
}

fn optional(value: Option<String>) -> String {
    value.unwrap_or_else(|| "null".to_owned())
}

fn string_array(values: &[String]) -> String {
    array(values.iter().map(|value| quote(value)))
}

fn array(items: impl IntoIterator<Item = String>) -> String {
    let mut parts: Vec<String> = items.into_iter().collect();
    if parts.is_empty() {
        return "[]".to_owned();
    }
    let joined = parts.join(",");
    parts.clear();
    format!("[{joined}]")
}

fn object(fields: Vec<String>) -> String {
    if fields.is_empty() {
        return "{}".to_owned();
    }
    format!("{{{}}}", fields.join(","))
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    write_json_string(text, &mut out);
    out
}

/// The JSON string writer shared by the canonical families (compact, no
/// escapes beyond the mandatory set).
fn write_json_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}
