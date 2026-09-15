//! The neutral trace-manifest projection (issue #36).
//!
//! The integration never defines an OpenSpec-specific wire: it projects
//! the resolved requirements and their symbol links into the closed #22
//! trace contract (`lekalo/trace-manifest/v1.0.0`). Requirement nodes
//! carry the OpenSpec original ids verbatim as external references with
//! their exact body digests; every reference becomes an `implements`
//! edge (symbol→requirement — the only legal symbol→requirement kind in
//! the closed #22 endpoint matrix, covering both `derived_from` and
//! `implements` declarations); missing and conflicted links become
//! explicit gaps; the whole manifest is validated by the typed #22
//! validator before any byte is emitted.
//!
//! Completeness is `partial` by construction: this projection carries
//! the requirement→symbol chain but no binding/test/gate chain, and the
//! one unanchored `missing-gate` gap makes that absence explicit instead
//! of silently claiming coverage.

use crate::diagnostics::DiagnosticSet;
use crate::trace::completeness::{Completeness, ExportProfile, Gap, GapKind};
use crate::trace::node::{ExternalRef, Node, NodeKind};
use crate::trace::provenance::{Confidence, Origin, Provenance, Status};
use crate::trace::relation::{Relation, RelationKind};
use crate::trace::version as trace_version;
use crate::trace::{ContractRef, Manifest};

use super::diagnostic;
use super::report::Report;

/// The stable manifest id of this projection.
const MANIFEST_ID: &str = "requirements-trace";

/// Project one resolved report into the validated neutral trace
/// manifest. Pure and read-only; the manifest is re-validated through
/// the typed #22 validator, so an internal projection bug fails closed
/// instead of emitting invalid bytes.
pub(crate) fn project(report: &Report) -> Result<Manifest, DiagnosticSet> {
    if report.requirements.is_empty() && report.references.is_empty() && report.conflicts.is_empty()
    {
        return Err(diagnostic::projection_empty());
    }

    let mut nodes: Vec<Node> = Vec::new();
    let mut relations: Vec<Relation> = Vec::new();
    let mut gaps: Vec<Gap> = Vec::new();

    // Requirement nodes: one per catalog entry, with the OpenSpec
    // original id preserved verbatim and its exact body digest.
    for row in &report.requirements {
        nodes.push(Node {
            node_id: requirement_node_id(&row.source, &row.id),
            node_kind: NodeKind::Requirement,
            requirement_id: Some(format!("{}:{}", row.source, row.id)),
            semantic_id: None,
            artifact_id: None,
            ownership: None,
            path: None,
            content_digest: None,
            revision: None,
            generator_ref: None,
            manifest_digest: None,
            scenario_id: None,
            test_id: None,
            gate_id: None,
            diagnostic_id: None,
            contract_version: None,
            evidence_digest: None,
            external_refs: vec![ExternalRef {
                system: crate::trace::node::ExternalSystem::Openspec,
                original_id: row.id.clone(),
                contract_version: None,
                revision: None,
                digest: Some(row.digest.clone()),
            }],
        });
    }

    // Symbol nodes: one per referenced symbol.
    let mut symbols: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for row in &report.references {
        symbols.insert(row.symbol.as_str());
    }
    for symbol in &symbols {
        nodes.push(Node {
            node_id: symbol_node_id(symbol),
            node_kind: NodeKind::Symbol,
            requirement_id: None,
            semantic_id: Some((*symbol).to_owned()),
            artifact_id: None,
            ownership: None,
            path: None,
            content_digest: None,
            revision: None,
            generator_ref: None,
            manifest_digest: None,
            scenario_id: None,
            test_id: None,
            gate_id: None,
            diagnostic_id: None,
            contract_version: None,
            evidence_digest: None,
            external_refs: Vec::new(),
        });
    }

    nodes.sort_by(|left, right| {
        (left.node_kind, left.node_id.as_str()).cmp(&(right.node_kind, right.node_id.as_str()))
    });

    // Relations: one `implements` edge per non-missing, non-conflicted
    // reference. Fresh links confirm against the manifest revision;
    // stale links carry the stale status explicitly.
    for (index, row) in report.references.iter().enumerate() {
        if row.status == "missing" || row.status == "conflict" {
            continue;
        }
        let from = symbol_node_id(&row.symbol);
        let to = requirement_node_id(&row.source, &row.requirement);
        let relation_kind = RelationKind::Implements;
        let occurrence = format!("{}.{}", row.relation, index);
        let confirmed = row.status == "fresh";
        relations.push(Relation {
            relation_id: Relation::canonical_id(relation_kind, &from, &to, &occurrence),
            relation_kind,
            from_node: from,
            to_node: to,
            occurrence,
            provenance: Provenance {
                origin: Origin::Declared,
                source_system: "openspec".to_owned(),
                source_revision: report.source_revision.clone(),
                source_digest: row.current_revision.clone().unwrap_or_default(),
                recorded_by: "lekalo.core".to_owned(),
                source_path: None,
            },
            confidence: if confirmed {
                Confidence::Exact
            } else {
                Confidence::Medium
            },
            status: if confirmed {
                Status::Confirmed
            } else {
                Status::Stale
            },
            evidence_refs: vec![requirement_node_id(&row.source, &row.requirement)],
        });
    }
    relations.sort_by(|left, right| {
        (
            left.relation_kind,
            left.from_node.as_str(),
            left.to_node.as_str(),
            left.occurrence.as_str(),
        )
            .cmp(&(
                right.relation_kind,
                right.from_node.as_str(),
                right.to_node.as_str(),
                right.occurrence.as_str(),
            ))
    });

    // Gaps: every missing reference, every conflict, and the one
    // explicit absence of the binding/test/gate chain.
    for row in &report.references {
        if row.status == "missing" {
            gaps.push(Gap {
                gap_kind: GapKind::MissingRequirement,
                status: Status::Candidate,
                anchor_node: Some(symbol_node_id(&row.symbol)),
                expected: Some(requirement_node_id(&row.source, &row.requirement)),
                source_path: None,
            });
        } else if row.status == "conflict" {
            gaps.push(Gap {
                gap_kind: GapKind::Conflict,
                status: Status::Conflicting,
                anchor_node: Some(symbol_node_id(&row.symbol)),
                expected: Some(requirement_node_id(&row.source, &row.requirement)),
                source_path: None,
            });
        }
    }
    for row in &report.conflicts {
        gaps.push(Gap {
            gap_kind: GapKind::Conflict,
            status: Status::Conflicting,
            anchor_node: None,
            expected: Some(format!("conflict:{}:{}", row.source, row.subject_id)),
            source_path: None,
        });
    }
    gaps.push(Gap {
        gap_kind: GapKind::MissingGate,
        status: Status::Candidate,
        anchor_node: None,
        expected: None,
        source_path: None,
    });
    gaps.sort_by(|left, right| {
        (
            left.gap_kind,
            left.anchor_node.as_deref().unwrap_or_default(),
            left.expected.as_deref().unwrap_or_default(),
        )
            .cmp(&(
                right.gap_kind,
                right.anchor_node.as_deref().unwrap_or_default(),
                right.expected.as_deref().unwrap_or_default(),
            ))
    });

    gaps.dedup_by(|a, b| {
        a.gap_kind == b.gap_kind && a.anchor_node == b.anchor_node && a.expected == b.expected
    });

    // Provider rows are absent-tree-annotating: an absent provider with
    // resolvable requirements cannot occur here, so an unsupported
    // provider gap never appears. The expected gap is a semantic id
    // grammar string, and the source revision is the bare catalog
    // digest, so the #22 validator accepts the projection unchanged.
    Ok(Manifest {
        schema_version: trace_version::SCHEMA_VERSION.to_owned(),
        identity: trace_version::IDENTITY.to_owned(),
        manifest_id: MANIFEST_ID.to_owned(),
        project_ref: report.project_id.clone(),
        completeness: Completeness::Partial,
        source_revision: report.source_revision.clone(),
        model_ref: ContractRef {
            schema_version: report.model_ref.model_version.clone(),
            digest: report.model_ref.digest.clone(),
        },
        ir_ref: None,
        graph_ref: None,
        artifact_manifest_ref: None,
        export_profile: ExportProfile::RequirementToGate,
        nodes,
        relations,
        gaps,
    })
}

/// The manifest-local node id of one namespaced requirement.
fn requirement_node_id(source: &str, requirement: &str) -> String {
    format!("requirement:{source}:{requirement}")
}

/// Preserve existing short local identities. Long semantic IDs use their full
/// SHA-256 in a disjoint namespace, never a truncated semantic ID or array index.
/// The original semantic ID remains verbatim on the symbol node.
fn symbol_node_id(symbol: &str) -> String {
    let readable = format!("symbol:{symbol}");
    if crate::trace::id::is_node_id(&readable) {
        readable
    } else {
        format!("symbol-sha256:{}", super::sha256_hex(symbol.as_bytes()))
    }
}

/// Project one resolved report into the typed #22 trace manifest,
/// re-validated by the accepted trace validator. Pure and read-only.
pub(crate) fn validated_manifest(
    report: &Report,
) -> Result<crate::trace::TraceManifest, DiagnosticSet> {
    let manifest = project(report)?;
    let bytes = serde_json::to_string(&manifest)
        .map_err(|_| diagnostic::document_invalid("projection-serialize", None))?;
    crate::trace::TraceManifest::parse(bytes.as_bytes())
}
