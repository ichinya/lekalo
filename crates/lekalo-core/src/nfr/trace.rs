//! The neutral trace-manifest projection of the NFR resolution
//! (issue #85).
//!
//! The integration never defines an NFR-specific wire: it projects the
//! resolved constraints into the closed #22 trace contract
//! (`lekalo/trace-manifest/v0.2.16`). Every constraint becomes a
//! requirement node carrying the Lekalo external reference with its
//! exact revision; every scope symbol becomes a symbol node linked
//! `implements` into its constraints; every declared gate becomes a
//! gate node evidencing the scope symbol (the only legal gate→symbol
//! kind in the closed #22 endpoint matrix). Unverified constraints
//! surface as explicit missing-gate gaps, stale evidence as stale
//! revisions, and conflicts as conflicts — the manifest documents
//! absence instead of silently claiming coverage. The whole manifest
//! is re-validated by the typed #22 validator before any byte is
//! emitted.
//!
//! Completeness is `partial` by construction: this projection carries
//! the constraint→symbol→gate chain but no binding or artifact chain.

use crate::diagnostics::DiagnosticSet;
use crate::trace::completeness::{Completeness, ExportProfile, Gap, GapKind};
use crate::trace::node::{ExternalRef, Node, NodeKind};
use crate::trace::provenance::{Confidence, Origin, Provenance, Status};
use crate::trace::relation::{Relation, RelationKind};
use crate::trace::version as trace_version;
use crate::trace::{ContractRef, Manifest};

use super::canonical::sha256_hex;
use super::diagnostic;
use super::report::Report;

/// The stable manifest id of this projection.
const MANIFEST_ID: &str = "nfr-trace";

/// Project one resolved report into the validated neutral trace
/// manifest. Pure and read-only; the manifest is re-validated through
/// the typed #22 validator, so an internal projection bug fails
/// closed instead of emitting invalid bytes.
pub(crate) fn validated_manifest(
    report: &Report,
) -> Result<crate::trace::TraceManifest, DiagnosticSet> {
    if report.runtime.constraints.is_empty() && report.ai_budget.constraints.is_empty() {
        return Err(diagnostic::projection_empty());
    }
    let mut nodes: Vec<Node> = Vec::new();
    let mut relations: Vec<Relation> = Vec::new();
    let mut gaps: Vec<Gap> = Vec::new();
    // Scope symbols are shared: several constraints may cover one
    // symbol, so the symbol nodes deduplicate by semantic id.
    let mut symbols: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for row in report
        .runtime
        .constraints
        .iter()
        .chain(report.ai_budget.constraints.iter())
    {
        symbols.insert(row.scope.r#ref.as_str());
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

    let rows: Vec<&super::report::ConstraintRow> = report
        .runtime
        .constraints
        .iter()
        .chain(report.ai_budget.constraints.iter())
        .collect();
    for (row_index, row) in rows.iter().enumerate() {
        let requirement_node = constraint_node_id(&row.constraint_id);
        nodes.push(Node {
            node_id: requirement_node.clone(),
            node_kind: NodeKind::Requirement,
            requirement_id: Some(row.constraint_id.clone()),
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
            contract_version: Some(super::version::VERSION.to_owned()),
            evidence_digest: None,
            external_refs: vec![ExternalRef {
                system: crate::trace::node::ExternalSystem::Lekalo,
                original_id: row.constraint_id.clone(),
                contract_version: Some(super::version::VERSION.to_owned()),
                revision: None,
                digest: None,
            }],
        });
        // The scope symbol implements the constraint.
        let symbol_node = symbol_node_id(&row.scope.r#ref);
        let occurrence = format!("impl.{row_index}");
        relations.push(Relation {
            relation_id: Relation::canonical_id(
                RelationKind::Implements,
                &symbol_node,
                &requirement_node,
                &occurrence,
            ),
            relation_kind: RelationKind::Implements,
            from_node: symbol_node.clone(),
            to_node: requirement_node.clone(),
            occurrence,
            provenance: Provenance {
                origin: Origin::Declared,
                source_system: "lekalo".to_owned(),
                source_revision: bare_digest(&report.attachment_digest),
                source_digest: report.attachment_digest.clone(),
                recorded_by: "lekalo.core".to_owned(),
                source_path: None,
            },
            confidence: Confidence::Exact,
            status: Status::Confirmed,
            evidence_refs: vec![requirement_node.clone()],
        });
        // The declared gate evidences the scope symbol — the only
        // legal gate endpoint in the closed #22 matrix.
        if let Some(gate_ref) = &row.gate_ref {
            // The #22 gate identity is a semantic id; the namespaced
            // gateRef maps its namespace slash to a dot, and the gate
            // stays recoverable through the constraint's report row.
            let gate_identity = gate_ref.replace('/', ".");
            let gate_node = format!("gate:{gate_identity}");
            let gate_occurrence = format!("gate.{row_index}");
            nodes.push(Node {
                node_id: gate_node.clone(),
                node_kind: NodeKind::Gate,
                requirement_id: None,
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
                gate_id: Some(gate_identity),
                diagnostic_id: None,
                contract_version: None,
                evidence_digest: None,
                external_refs: Vec::new(),
            });
            relations.push(Relation {
                relation_id: Relation::canonical_id(
                    RelationKind::Evidences,
                    &gate_node,
                    &symbol_node,
                    &gate_occurrence,
                ),
                relation_kind: RelationKind::Evidences,
                from_node: gate_node.clone(),
                to_node: symbol_node.clone(),
                occurrence: gate_occurrence,
                provenance: Provenance {
                    origin: Origin::Declared,
                    source_system: "lekalo".to_owned(),
                    source_revision: bare_digest(&report.attachment_digest),
                    source_digest: report.attachment_digest.clone(),
                    recorded_by: "lekalo.core".to_owned(),
                    source_path: None,
                },
                confidence: if row.status == "satisfied" {
                    Confidence::Exact
                } else {
                    Confidence::Medium
                },
                status: if row.status == "satisfied" {
                    Status::Confirmed
                } else if row.status == "stale" {
                    Status::Stale
                } else {
                    Status::Candidate
                },
                evidence_refs: vec![requirement_node.clone()],
            });
        }
        // Absence stays explicit: the closed gap vocabulary documents
        // every constraint whose evidence does not satisfy it.
        match row.status {
            "unverified" | "open-question" | "unsupported" => gaps.push(Gap {
                gap_kind: GapKind::MissingGate,
                status: Status::Candidate,
                anchor_node: Some(requirement_node),
                expected: None,
                source_path: None,
            }),
            "stale" | "foreign-environment" => gaps.push(Gap {
                gap_kind: GapKind::StaleRevision,
                status: Status::Stale,
                anchor_node: Some(requirement_node),
                expected: None,
                source_path: None,
            }),
            "conflict" => gaps.push(Gap {
                gap_kind: GapKind::Conflict,
                status: Status::Conflicting,
                anchor_node: Some(requirement_node),
                expected: None,
                source_path: None,
            }),
            _ => {}
        }
    }

    nodes.sort_by(|left, right| {
        (left.node_kind, left.node_id.as_str()).cmp(&(right.node_kind, right.node_id.as_str()))
    });
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

    let manifest = Manifest {
        schema_version: trace_version::SCHEMA_VERSION.to_owned(),
        identity: trace_version::IDENTITY.to_owned(),
        manifest_id: MANIFEST_ID.to_owned(),
        project_ref: report.project_id.clone(),
        completeness: Completeness::Partial,
        source_revision: bare_digest(&report.attachment_digest),
        model_ref: ContractRef {
            schema_version: report.model_ref.model_version.clone(),
            digest: report.model_ref.digest.clone(),
        },
        ir_ref: Some(ContractRef {
            schema_version: report.ir_ref.ir_version.clone(),
            digest: report.ir_ref.digest.clone(),
        }),
        graph_ref: None,
        artifact_manifest_ref: None,
        export_profile: ExportProfile::RequirementToGate,
        nodes,
        relations,
        gaps,
    };
    let bytes = serde_json::to_string(&manifest)
        .map_err(|_| diagnostic::document_invalid("projection-serialize", None))?;
    crate::trace::TraceManifest::parse(bytes.as_bytes())
}

/// The manifest-local node id of one constraint.
fn constraint_node_id(constraint_id: &str) -> String {
    format!("requirement:{constraint_id}")
}

/// The manifest-local node id of one scope symbol: short local
/// identities stay readable; long ones use their full SHA-256 in a
/// disjoint namespace, never a truncated semantic id.
fn symbol_node_id(symbol: &str) -> String {
    let readable = format!("symbol:{symbol}");
    if crate::trace::id::is_node_id(&readable) {
        readable
    } else {
        format!("symbol-sha256:{}", sha256_hex(symbol.as_bytes()))
    }
}

/// The bare digest the #22 sourceRevision member requires.
fn bare_digest(attachment_digest: &str) -> String {
    attachment_digest
        .strip_prefix("sha256:")
        .unwrap_or(attachment_digest)
        .to_owned()
}
