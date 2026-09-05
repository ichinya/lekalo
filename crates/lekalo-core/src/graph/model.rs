//! The typed graph model: closed registries, node and edge contracts.
//!
//! Everything here is a closed, checked surface: node and relation kinds are
//! kind-qualified and path-independent, edges carry an occurrence ordinal
//! plus closed provenance and confidence, and exact duplicates collapse only
//! when every machine field matches. No raw `String`, `Path`, or
//! `serde_json::Value` enters through a public constructor.

use std::fmt;

pub use super::version::{
    MAX_DEPTH, MAX_EDGES, MAX_EXPORT_BYTES, MAX_FILTER_TERMS, MAX_NODES, MAX_PATH_NODES,
    MAX_PROVENANCE_RECORDS, MAX_RESULT_EDGES, MAX_RESULT_NODES,
};

/// The closed node-kind registry entry.
///
/// Core kinds have fixed ranks; the wire key is the exact registry key.
/// Future kinds (#14 effects, #23 scenario traces, provider kinds) enter as
/// versioned registry successors, so no core enum ever blocks them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct NodeKindId(u16);

/// The closed relation-kind registry entry.
///
/// `requires` and `derived_from` carry the graph-owned acyclic policy;
/// every other core relation may cycle and stays traversal-safe by visited
/// keys. `writes`, `implements`, and `verifies` are registered but not
/// emitted until their accepted typed owners (#14, #22/#23, #27/#29)
/// contribute evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct RelationKindId(u16);

/// The closed v1 node-kind keys in registry rank order.
pub(crate) const NODE_KIND_KEYS: [&str; 12] = [
    "project",
    "module",
    "type",
    "entity",
    "operation",
    "policy",
    "event",
    "effect",
    "endpoint",
    "scenario",
    "target-binding",
    "requirement",
];

/// The closed v1 relation-kind keys in registry rank order.
pub(crate) const RELATION_KIND_KEYS: [&str; 12] = [
    "requires",
    "references",
    "accepts",
    "returns",
    "reads",
    "writes",
    "emits",
    "authorizes",
    "exposes",
    "implements",
    "verifies",
    "derived_from",
];

impl NodeKindId {
    /// The registry rank; the primary node sort key.
    pub const fn rank(self) -> u16 {
        self.0
    }

    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        NODE_KIND_KEYS[self.0 as usize]
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        NODE_KIND_KEYS
            .iter()
            .position(|candidate| *candidate == key)
            .map(|index| Self(index as u16))
    }

    /// The `project` kind.
    pub const PROJECT: Self = Self(0);
    /// The `module` kind.
    pub const MODULE: Self = Self(1);
    /// The `type` kind (scalar, enum, value-object subkinds).
    pub const TYPE: Self = Self(2);
    /// The `entity` kind.
    pub const ENTITY: Self = Self(3);
    /// The `operation` kind (command, query subkinds).
    pub const OPERATION: Self = Self(4);
    /// The `policy` kind.
    pub const POLICY: Self = Self(5);
    /// The `event` kind.
    pub const EVENT: Self = Self(6);
    /// The declared `effect` kind (the #14 effect model is a successor).
    pub const EFFECT: Self = Self(7);
    /// The `endpoint` kind.
    pub const ENDPOINT: Self = Self(8);
    /// The `scenario` kind.
    pub const SCENARIO: Self = Self(9);
    /// The `target-binding` kind.
    pub const TARGET_BINDING: Self = Self(10);
    /// The `requirement` kind (stable ids only; no requirement text).
    pub const REQUIREMENT: Self = Self(11);
}

impl RelationKindId {
    /// The registry rank; the relation sort key inside one node pair.
    pub const fn rank(self) -> u16 {
        self.0
    }

    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        RELATION_KIND_KEYS[self.0 as usize]
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        RELATION_KIND_KEYS
            .iter()
            .position(|candidate| *candidate == key)
            .map(|index| Self(index as u16))
    }

    /// Whether this relation is graph-owned acyclic: any cycle (self-loop
    /// or larger strongly connected component) is a fatal diagnostic.
    pub const fn is_acyclic(self) -> bool {
        matches!(self, Self::REQUIRES | Self::DERIVED_FROM)
    }

    /// The `requires` relation (module imports).
    pub const REQUIRES: Self = Self(0);
    /// The `references` relation (type leaves, effect subjects, covers).
    pub const REFERENCES: Self = Self(1);
    /// The `accepts` relation (command input types).
    pub const ACCEPTS: Self = Self(2);
    /// The `returns` relation (operation return types).
    pub const RETURNS: Self = Self(3);
    /// The `reads` relation (query reads).
    pub const READS: Self = Self(4);
    /// The `writes` relation: registered, not emitted until #14.
    pub const WRITES: Self = Self(5);
    /// The `emits` relation (declared effect emits).
    pub const EMITS: Self = Self(6);
    /// The `authorizes` relation (policy applies-to).
    pub const AUTHORIZES: Self = Self(7);
    /// The `exposes` relation (endpoint invokes).
    pub const EXPOSES: Self = Self(8);
    /// The `implements` relation: registered, not emitted until a
    /// target-binding semantic contract exists.
    pub const IMPLEMENTS: Self = Self(9);
    /// The `verifies` relation: registered, not emitted until #23.
    pub const VERIFIES: Self = Self(10);
    /// The `derived_from` relation (requirement provenance).
    pub const DERIVED_FROM: Self = Self(11);
}

impl fmt::Display for NodeKindId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

impl fmt::Display for RelationKindId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

/// A kind-qualified, path-independent node identity.
///
/// The wire form is `{kind}:{semantic-id}`; requirement ids carry their
/// own `UPPER-CASE` grammar, so the qualified form stays unambiguous.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct NodeId(String);

impl NodeId {
    /// Assemble the qualified id of a core kind.
    ///
    /// The semantic id must be non-empty and free of control characters;
    /// every accepted IR id and requirement id already satisfies this, so
    /// a rejection here is a programming error surfaced as `None`.
    pub fn new(kind: NodeKindId, semantic_id: &str) -> Option<Self> {
        if semantic_id.is_empty()
            || semantic_id.len() > 192
            || !semantic_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':')
            })
        {
            return None;
        }
        Some(Self(format!("{}:{semantic_id}", kind.key())))
    }

    /// Parse an exact qualified wire form; the kind prefix must be a
    /// registered core kind and the semantic id must be node-safe.
    pub fn from_qualified(qualified: &str) -> Option<Self> {
        let (kind_key, semantic_id) = qualified.split_once(':')?;
        let kind = NodeKindId::from_key(kind_key)?;
        Self::new(kind, semantic_id)
    }

    /// The exact wire form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The bare semantic id below the kind prefix.
    pub fn semantic_id(&self) -> &str {
        &self.0[self.0.find(':').map_or(0, |position| position + 1)..]
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The 0-based source occurrence of one reference site inside its role.
///
/// Repeated identical references at different sites stay separate edges
/// because the ordinal differs; the ordinal is assigned in canonical IR
/// site order and doubles as the final edge tie breaker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct OccurrenceOrdinal(u32);

impl OccurrenceOrdinal {
    /// Wrap an ordinal that came from a counted IR site walk.
    pub fn new(ordinal: usize) -> Option<Self> {
        u32::try_from(ordinal).ok().map(Self)
    }

    /// The numeric ordinal.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// The closed confidence vocabulary: never a floating score.
///
/// Ordering is trust rank: `Canonical` is the most trusted value, `Unknown`
/// the least. The meet of several confidences is the least trusted one.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Confidence {
    /// Directly decoded from the accepted canonical IR.
    Canonical,
    /// Confirmed by an accepted typed evidence owner.
    Verified,
    /// Contributed by an adapter or provider envelope.
    Extracted,
    /// Computed by a recorded algorithm over other edges.
    Inferred,
    /// Unresolved or stale evidence: visibly degraded, never optimistic.
    Unknown,
}

impl Confidence {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Verified => "verified",
            Self::Extracted => "extracted",
            Self::Inferred => "inferred",
            Self::Unknown => "unknown",
        }
    }

    /// Registry lookup by exact wire spelling.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "canonical" => Some(Self::Canonical),
            "verified" => Some(Self::Verified),
            "extracted" => Some(Self::Extracted),
            "inferred" => Some(Self::Inferred),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// The least-trustworthy of two confidences (the meet).
    pub fn meet(self, other: Self) -> Self {
        if self >= other {
            self
        } else {
            other
        }
    }
}

/// The closed provenance sum of one edge.
///
/// Canonical provenance names the IR reference role and occurrence;
/// adapter evidence keeps only validated namespaced identities and an
/// opaque digest; derived provenance points at parent edge keys. Source
/// paths and spans never enter provenance: they resolve through the #8
/// source map on explicit request only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EdgeProvenance {
    /// Decoded straight from the accepted canonical IR.
    CanonicalIr {
        /// The closed reference role of the source site.
        reference_role: ReferenceRole,
        /// The 0-based site ordinal inside its role.
        occurrence: OccurrenceOrdinal,
        /// The fully qualified semantic id of the referencing definition.
        source_symbol: String,
    },
    /// Typed adapter evidence (#83 contributors; produced, never created,
    /// by the graph).
    AdapterEvidence {
        /// Validated namespaced adapter id.
        adapter_id: String,
        /// The target identity the evidence is bound to.
        target_id: String,
        /// The exact target protocol version.
        protocol_version: String,
        /// Opaque evidence digest (`sha256:<64 lowercase hex>`).
        evidence_digest: String,
        /// The closed evidence status (for example `fresh`/`stale`).
        evidence_status: String,
    },
    /// Computed by a recorded algorithm over recorded parent edges.
    Derived {
        /// Namespaced algorithm id.
        algorithm_id: String,
        /// Exact algorithm version.
        algorithm_version: String,
        /// Parent edge keys; at most [`MAX_PROVENANCE_RECORDS`].
        parent_edge_keys: Vec<String>,
    },
}

impl EdgeProvenance {
    /// The provenance sort rank (canonical first, then by wire spelling).
    pub(crate) fn sort_rank(&self) -> u8 {
        match self {
            Self::CanonicalIr { .. } => 0,
            Self::AdapterEvidence { .. } => 1,
            Self::Derived { .. } => 2,
        }
    }
    /// The confidence this provenance kind carries on its own.
    pub(crate) fn confidence(&self) -> Confidence {
        match self {
            Self::CanonicalIr { .. } => Confidence::Canonical,
            Self::AdapterEvidence {
                evidence_status, ..
            } => match evidence_status.as_str() {
                "fresh" => Confidence::Verified,
                "stale" | "unknown" => Confidence::Unknown,
                _ => Confidence::Extracted,
            },
            Self::Derived { .. } => Confidence::Inferred,
        }
    }
}

/// The closed reference-role vocabulary of canonical IR provenance.
///
/// The role names the exact IR surface an edge was decoded from, so a
/// diagnostic or spans lookup can address the source site deterministically.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ReferenceRole {
    /// A module import (the `requires` edge).
    ModuleImport,
    /// An entity field type leaf.
    EntityField,
    /// A value-object field type leaf.
    ValueObjectField,
    /// An event payload type leaf.
    EventPayload,
    /// A command effect symbol.
    CommandEffect,
    /// The entity subject of a declared effect.
    EffectEntity,
    /// A scenario `covers` symbol (generic reference until #23).
    ScenarioCovers,
    /// A command input type leaf.
    CommandInput,
    /// A query return type leaf.
    QueryReturns,
    /// A query `reads` symbol.
    QueryReads,
    /// A policy `applies_to` symbol.
    PolicyAppliesTo,
    /// An endpoint `invokes` symbol.
    EndpointInvokes,
    /// A declared effect `emits` symbol.
    EffectEmits,
    /// A requirement provenance reference.
    DerivedFrom,
}

impl ReferenceRole {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModuleImport => "module-import",
            Self::EntityField => "entity-field",
            Self::ValueObjectField => "value-object-field",
            Self::EventPayload => "event-payload",
            Self::CommandEffect => "command-effect",
            Self::EffectEntity => "effect-entity",
            Self::ScenarioCovers => "scenario-covers",
            Self::CommandInput => "command-input",
            Self::QueryReturns => "query-returns",
            Self::QueryReads => "query-reads",
            Self::PolicyAppliesTo => "policy-applies-to",
            Self::EndpointInvokes => "endpoint-invokes",
            Self::EffectEmits => "effect-emits",
            Self::DerivedFrom => "derived-from",
        }
    }
}

impl GraphNode {
    /// Assemble one node from an exact qualified id (internal test seam).
    #[cfg(test)]
    pub(crate) fn from_qualified_for_tests(qualified: &str) -> Option<Self> {
        let id = NodeId::from_qualified(qualified)?;
        Some(Self {
            id,
            kind: NodeKindId::from_key(qualified.split_once(':')?.0)?,
            module: None,
            subkind: None,
        })
    }
}

/// One graph node: kind, stable semantic identity, module ownership, and a
/// bounded typed subkind. Node identity is path-independent: the same IR
/// always produces the same nodes whatever the filesystem layout was.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphNode {
    id: NodeId,
    kind: NodeKindId,
    module: Option<String>,
    subkind: Option<&'static str>,
}

impl GraphNode {
    /// Assemble one node; the subkind must belong to the kind.
    pub(crate) fn new(
        kind: NodeKindId,
        semantic_id: &str,
        module: Option<String>,
        subkind: Option<&'static str>,
    ) -> Option<Self> {
        if subkind.is_some() && !matches!(kind, NodeKindId::TYPE | NodeKindId::OPERATION) {
            return None;
        }
        Some(Self {
            id: NodeId::new(kind, semantic_id)?,
            kind,
            module,
            subkind,
        })
    }

    /// The kind-qualified identity.
    pub fn id(&self) -> &NodeId {
        &self.id
    }

    /// The registry kind.
    pub const fn kind(&self) -> NodeKindId {
        self.kind
    }

    /// The typed subkind (`scalar`/`enum`/`value-object`, `command`/`query`).
    pub const fn subkind(&self) -> Option<&'static str> {
        self.subkind
    }

    /// The owning module id, when the symbol is module-owned.
    pub fn module(&self) -> Option<&str> {
        self.module.as_deref()
    }

    /// The canonical node sort key: kind rank, module, semantic id, subkind.
    pub(crate) fn sort_key(&self) -> (u16, Option<&str>, &str, Option<&'static str>) {
        (
            self.kind.rank(),
            self.module.as_deref(),
            self.id.semantic_id(),
            self.subkind,
        )
    }
}

/// The canonical identity of one direct edge: endpoints, relation, and the
/// source occurrence. Two edges are exact duplicates only when every
/// machine field matches.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct EdgeKey {
    from: NodeId,
    to: NodeId,
    relation: RelationKindId,
    occurrence: OccurrenceOrdinal,
}

impl EdgeKey {
    /// Assemble one checked edge key.
    pub fn new(
        from: NodeId,
        to: NodeId,
        relation: RelationKindId,
        occurrence: OccurrenceOrdinal,
    ) -> Self {
        Self {
            from,
            to,
            relation,
            occurrence,
        }
    }

    /// The tail node id.
    pub fn from(&self) -> &NodeId {
        &self.from
    }

    /// The head node id.
    pub fn to(&self) -> &NodeId {
        &self.to
    }

    /// The relation kind.
    pub const fn relation(&self) -> RelationKindId {
        self.relation
    }

    /// The source occurrence ordinal.
    pub const fn occurrence(&self) -> OccurrenceOrdinal {
        self.occurrence
    }

    /// The bounded canonical string form used by derived provenance.
    pub fn to_canonical_string(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.from.as_str(),
            self.relation.key(),
            self.to.as_str(),
            self.occurrence.get()
        )
    }
}

/// One direct dependency edge with provenance and confidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphEdge {
    key: EdgeKey,
    provenance: EdgeProvenance,
    confidence: Confidence,
}

impl GraphEdge {
    /// Assemble one edge; the confidence follows from the provenance
    /// kind, so it can never claim more trust than its evidence.
    pub(crate) fn new(key: EdgeKey, provenance: EdgeProvenance) -> Self {
        let confidence = provenance.confidence();
        Self {
            key,
            provenance,
            confidence,
        }
    }

    /// The edge identity.
    pub const fn key(&self) -> &EdgeKey {
        &self.key
    }

    /// The closed provenance record.
    pub const fn provenance(&self) -> &EdgeProvenance {
        &self.provenance
    }

    /// The closed confidence level.
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// The canonical edge sort key: endpoints, relation, occurrence,
    /// provenance kind, then provenance bytes.
    pub(crate) fn sort_key(&self) -> (NodeId, NodeId, u16, u32, u8) {
        (
            self.key.from.clone(),
            self.key.to.clone(),
            self.key.relation.rank(),
            self.key.occurrence.get(),
            self.provenance.sort_rank(),
        )
    }
}
