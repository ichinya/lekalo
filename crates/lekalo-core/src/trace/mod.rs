//! Issue #22: the neutral trace manifest.
//!
//! A read-only, typed validation of the requirement -> symbol ->
//! binding/artifact -> scenario/native-test -> gate chain, expressed in
//! the closed `lekalo/trace-manifest/v1.0.0` wire. Lekalo owns the
//! contract, the typed validation semantics, the canonical writer, and
//! the derived reverse queries; persisted `trace.manifest` instances
//! remain AI Factory-owned direct evidence under the accepted authority
//! boundary (`canonicalOwner ai-factory`, `.ai-factory/traces/**`), and
//! HLV/OpenSpec/native formats enter only as verbatim external ids
//! through adapters — never embedded, never parsed here.
//!
//! Boundaries: this module never scans a project, never parses Git, never
//! spawns a process or provider, never executes a test or gate, never
//! writes anything, and never resolves a logical path physically. Every
//! reference resolves against the supplied document only.
//!
//! Determinism: nodes sort by (kind, id), relations by (kind, from, to,
//! occurrence) with the canonical tuple digest, gaps by (kind, anchor,
//! expected); external and evidence references are set-like and sorted.
//! Canonical export is compact UTF-8 JSON with object keys in fixed
//! contract order, byte-identical for the same manifest regardless of
//! input insertion order. The manifest digest is `sha256:` over exactly
//! those canonical bytes (no trailing LF).

pub mod canonical;
pub mod completeness;
pub mod diagnostic;
pub mod id;
pub mod node;
pub mod parse;
pub mod provenance;
pub mod query;
pub mod relation;
pub mod version;

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::diagnostics::DiagnosticSet;

pub use completeness::{Completeness, ExportProfile, Gap, GapKind};
pub use diagnostic::{io_failure, PathViolation};
pub use node::{ExternalRef, ExternalSystem, Node, NodeKind, Ownership};
pub use provenance::{Confidence, Origin, Provenance, Status};
pub use query::{QueryRow, QuerySelection};
pub use relation::{Relation, RelationKind};

/// One contract reference: the exact referenced contract version plus the
/// canonical digest of its payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContractRef {
    pub schema_version: String,
    pub digest: String,
}

/// The wire manifest: exactly the closed top-level shape.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: String,
    pub identity: String,
    pub manifest_id: String,
    pub project_ref: String,
    pub completeness: Completeness,
    pub source_revision: String,
    pub model_ref: ContractRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir_ref: Option<ContractRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_ref: Option<ContractRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_manifest_ref: Option<ContractRef>,
    pub export_profile: ExportProfile,
    pub nodes: Vec<Node>,
    pub relations: Vec<Relation>,
    pub gaps: Vec<Gap>,
}

/// The accepted manifest summary projected by `validate` and `export`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceReport {
    pub manifest_id: String,
    pub project_ref: String,
    pub completeness: Completeness,
    pub export_profile: ExportProfile,
    pub source_revision: String,
    pub node_count: usize,
    pub relation_count: usize,
    pub gap_count: usize,
    /// Sink nodes of the export profile no chain reaches (partial
    /// manifests report them explicitly — never silently).
    pub uncovered_sinks: Vec<String>,
}

/// One finished trace manifest: validated, canonically ordered, and
/// safe to share across threads. Every query is side-effect free.
pub struct TraceManifest {
    manifest: Manifest,
    node_positions: HashMap<String, usize>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
}

impl TraceManifest {
    /// The validated wire manifest in canonical order.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Resolve one node position by node id.
    pub(crate) fn node_position(&self, node_id: &str) -> Option<usize> {
        self.node_positions.get(node_id).copied()
    }

    /// The node at one position.
    pub(crate) fn node(&self, position: usize) -> &Node {
        &self.manifest.nodes[position]
    }

    /// The identity id (requirementId/semanticId/...) of one node.
    pub(crate) fn node_identity(&self, position: usize) -> &str {
        self.manifest.nodes[position].identity().unwrap_or_default()
    }

    /// The nodes of exactly one kind, in canonical order.
    pub(crate) fn nodes_of_kind(&self, kind: NodeKind) -> impl Iterator<Item = usize> + '_ {
        self.manifest
            .nodes
            .iter()
            .enumerate()
            .filter(move |(_, node)| node.node_kind == kind)
            .map(|(position, _)| position)
    }

    /// The relations whose `from` endpoint is `position`, in canonical
    /// relation order.
    pub(crate) fn relations_from(&self, position: usize) -> impl Iterator<Item = &Relation> + '_ {
        let positions = &self.outgoing[position];
        positions
            .iter()
            .map(move |&relation_position| &self.manifest.relations[relation_position])
    }

    /// The relations naming `position` as their `to` endpoint, in
    /// canonical relation order.
    pub(crate) fn relations_to(&self, position: usize) -> impl Iterator<Item = &Relation> + '_ {
        let positions = &self.incoming[position];
        positions
            .iter()
            .map(move |&relation_position| &self.manifest.relations[relation_position])
    }

    /// All relations in canonical order.
    pub fn relations(&self) -> impl Iterator<Item = &Relation> {
        self.manifest.relations.iter()
    }

    /// The sinks of the export profile no chain reaches, in canonical
    /// node order.
    pub fn uncovered_sinks(&self) -> Vec<String> {
        let profile = self.manifest.export_profile;
        let mut reached = vec![false; self.manifest.nodes.len()];
        let mut queue: VecDeque<usize> = VecDeque::new();
        for source in self.nodes_of_kind(profile.source()) {
            if !reached[source] {
                reached[source] = true;
                queue.push_back(source);
            }
        }
        while let Some(position) = queue.pop_front() {
            for relation_position in &self.outgoing[position] {
                let to_node = &self.manifest.relations[*relation_position].to_node;
                let to = self.node_positions[to_node];
                if !reached[to] {
                    reached[to] = true;
                    queue.push_back(to);
                }
            }
        }
        self.manifest
            .nodes
            .iter()
            .enumerate()
            .filter(|(position, node)| node.node_kind == profile.sink() && !reached[*position])
            .map(|(position, _)| self.manifest.nodes[position].node_id.clone())
            .collect()
    }

    /// The accepted summary of one validated manifest.
    pub fn report(&self) -> TraceReport {
        TraceReport {
            manifest_id: self.manifest.manifest_id.clone(),
            project_ref: self.manifest.project_ref.clone(),
            completeness: self.manifest.completeness,
            export_profile: self.manifest.export_profile,
            source_revision: self.manifest.source_revision.clone(),
            node_count: self.manifest.nodes.len(),
            relation_count: self.manifest.relations.len(),
            gap_count: self.manifest.gaps.len(),
            uncovered_sinks: self.uncovered_sinks(),
        }
    }

    /// Run one reverse/forward query over the normalized relations.
    pub fn query(&self, selection: &QuerySelection) -> Result<Vec<QueryRow>, DiagnosticSet> {
        query::run(self, selection)
    }

    /// Parse and validate one manifest document, or return the terminal
    /// diagnostic set (invalid envelope).
    pub fn parse(bytes: &[u8]) -> Result<Self, DiagnosticSet> {
        parse::parse_and_validate(bytes)
    }

    /// The canonical bytes of the whole manifest.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        canonical::manifest_bytes(&self.manifest)
    }

    /// The canonical manifest digest: `sha256:` over the canonical bytes.
    pub fn digest(&self) -> Result<String, DiagnosticSet> {
        Ok(canonical::sha256_hex(self.canonical_bytes()?.as_bytes()))
    }
}
