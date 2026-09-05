//! Trace relations: the closed directed edge contract (issue #22).
//!
//! Eight relation kinds with an explicit endpoint matrix; direction and
//! endpoint kinds are validated before any indexing. Relation identity is
//! the tuple (kind, from, to, occurrence) — the canonical `relationId`
//! digest is derived from exactly that tuple, so two uses of the same
//! requirement/symbol/artifact pair with different occurrences stay
//! distinct and repeated endpoint pairs are never collapsed.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::node::NodeKind;
use super::provenance::{Confidence, Provenance, Status};

/// The fixed domain tag inside every canonical relation id.
const RELATION_ID_DOMAIN: &str = "lekalo/trace-manifest/v1.0.0/relation";

/// The closed relation kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// symbol -> requirement.
    Implements,
    /// symbol -> artifact.
    Binds,
    /// scenario -> symbol | scenario -> requirement.
    Covers,
    /// native_test -> scenario | native_test -> symbol.
    Verifies,
    /// gate -> native_test | gate -> scenario | gate -> symbol.
    Evidences,
    /// A derived node -> its source node (same-kind lineage).
    DerivedFrom,
    /// Explicitly typed cross-references (see [`Self::references_legal`]).
    References,
    /// The current revision -> a retired/stale revision (same kind).
    Supersedes,
}

impl RelationKind {
    /// The closed v1 kinds in canonical order.
    pub const KEYS: [Self; 8] = [
        Self::Implements,
        Self::Binds,
        Self::Covers,
        Self::Verifies,
        Self::Evidences,
        Self::DerivedFrom,
        Self::References,
        Self::Supersedes,
    ];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Implements => "implements",
            Self::Binds => "binds",
            Self::Covers => "covers",
            Self::Verifies => "verifies",
            Self::Evidences => "evidences",
            Self::DerivedFrom => "derived_from",
            Self::References => "references",
            Self::Supersedes => "supersedes",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }

    /// Whether the (from, to) endpoint kind pair is legal for this kind.
    pub fn endpoints_legal(&self, from: NodeKind, to: NodeKind) -> bool {
        match self {
            Self::Implements => from == NodeKind::Symbol && to == NodeKind::Requirement,
            Self::Binds => from == NodeKind::Symbol && to == NodeKind::Artifact,
            Self::Covers => {
                from == NodeKind::Scenario && matches!(to, NodeKind::Symbol | NodeKind::Requirement)
            }
            Self::Verifies => {
                from == NodeKind::NativeTest && matches!(to, NodeKind::Scenario | NodeKind::Symbol)
            }
            Self::Evidences => {
                from == NodeKind::Gate
                    && matches!(
                        to,
                        NodeKind::NativeTest | NodeKind::Scenario | NodeKind::Symbol
                    )
            }
            Self::DerivedFrom => from == to,
            Self::Supersedes => from == to,
            Self::References => matches!(
                (from, to),
                (NodeKind::Gate, NodeKind::Diagnostic)
                    | (NodeKind::Diagnostic, NodeKind::Gate)
                    | (NodeKind::Requirement, NodeKind::Requirement)
                    | (NodeKind::Symbol, NodeKind::Symbol)
                    | (NodeKind::Artifact, NodeKind::Artifact)
                    | (NodeKind::Scenario, NodeKind::Scenario)
            ),
        }
    }
}

/// The wire relation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Relation {
    pub relation_id: String,
    pub relation_kind: RelationKind,
    pub from_node: String,
    pub to_node: String,
    pub occurrence: String,
    pub provenance: Provenance,
    pub confidence: Confidence,
    pub status: Status,
    pub evidence_refs: Vec<String>,
}

/// Relation validation errors (fixed detail tags).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationError {
    RelationId,
    RelationKind,
    Endpoints,
    Occurrence,
    Provenance,
    EvidenceRef,
    EvidenceOverLimit,
    ConfirmedPolicy,
}

impl Relation {
    /// The canonical relation identity: `sha256:` over the compact JSON
    /// array of the domain tag, kind, endpoints, and occurrence. The
    /// tuple fully determines the digest, and the digest is recomputed by
    /// every consumer.
    pub fn canonical_id(kind: RelationKind, from: &str, to: &str, occurrence: &str) -> String {
        let mut hasher = Sha256::new();
        // Compact fixed-order JSON array encoding; no ambiguity because
        // every component is grammar-validated before hashing.
        let payload = format!(
            "[\"{RELATION_ID_DOMAIN}\",\"{}\",\"{}\",\"{}\",\"{}\"]",
            kind.as_str(),
            from,
            to,
            occurrence
        );
        hasher.update(payload.as_bytes());
        format!("sha256:{:x}", hasher.finalize())
    }

    /// Validate every field against its closed grammar and record the
    /// confirmed-status policy: a confirmed relation requires a matching
    /// source revision, non-inferred origin, exact-or-high confidence,
    /// and at least one cited evidence node.
    pub fn validate(&self, manifest_revision: &str) -> Result<(), RelationError> {
        match RelationKind::parse(self.relation_kind.as_str()) {
            Some(_) => {}
            None => return Err(RelationError::RelationKind),
        }
        if !super::id::is_node_id(&self.from_node) || !super::id::is_node_id(&self.to_node) {
            return Err(RelationError::Endpoints);
        }
        if !super::id::is_occurrence(&self.occurrence) {
            return Err(RelationError::Occurrence);
        }
        if self.relation_id
            != Self::canonical_id(
                self.relation_kind,
                &self.from_node,
                &self.to_node,
                &self.occurrence,
            )
        {
            return Err(RelationError::RelationId);
        }
        self.provenance
            .validate()
            .map_err(|_| RelationError::Provenance)?;
        if self.evidence_refs.len() > super::version::MAX_EVIDENCE_REFS {
            return Err(RelationError::EvidenceOverLimit);
        }
        for reference in &self.evidence_refs {
            if !super::id::is_node_id(reference) {
                return Err(RelationError::EvidenceRef);
            }
        }
        if self.status == Status::Confirmed {
            let revision_matches = self.provenance.source_revision == manifest_revision;
            let origin_acceptable =
                !matches!(self.provenance.origin, super::provenance::Origin::Inferred);
            let confidence_acceptable =
                matches!(self.confidence, Confidence::Exact | Confidence::High);
            if !revision_matches
                || !origin_acceptable
                || !confidence_acceptable
                || self.evidence_refs.is_empty()
            {
                return Err(RelationError::ConfirmedPolicy);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::provenance::Origin;

    fn provenance(revision: &str) -> Provenance {
        Provenance {
            origin: Origin::Declared,
            source_system: "openspec".to_owned(),
            source_revision: revision.to_owned(),
            source_digest: format!("sha256:{}", "a".repeat(64)),
            recorded_by: "lekalo.core".to_owned(),
            source_path: None,
        }
    }

    #[test]
    fn endpoint_matrix_matches_the_recorded_owner_decision() {
        let revision = "a".repeat(40);
        let mut relation = Relation {
            relation_id: String::new(),
            relation_kind: RelationKind::Implements,
            from_node: "symbol:planner.focus_task".to_owned(),
            to_node: "requirement:PLANNER-REQ-001".to_owned(),
            occurrence: "requirements.focus_task".to_owned(),
            provenance: provenance(&revision),
            confidence: Confidence::Exact,
            status: Status::Confirmed,
            evidence_refs: vec!["gate:hlv.focus".to_owned()],
        };
        relation.relation_id = Relation::canonical_id(
            relation.relation_kind,
            &relation.from_node,
            &relation.to_node,
            &relation.occurrence,
        );
        assert_eq!(relation.validate(&revision), Ok(()));

        // implements reversed is illegal.
        assert!(!RelationKind::Implements.endpoints_legal(NodeKind::Requirement, NodeKind::Symbol));
        // derived_from must stay same-kind.
        assert!(RelationKind::DerivedFrom.endpoints_legal(NodeKind::Symbol, NodeKind::Symbol));
        assert!(!RelationKind::DerivedFrom.endpoints_legal(NodeKind::Symbol, NodeKind::Artifact));
        // references is a closed pair list.
        assert!(RelationKind::References.endpoints_legal(NodeKind::Gate, NodeKind::Diagnostic));
        assert!(!RelationKind::References.endpoints_legal(NodeKind::Artifact, NodeKind::Scenario));
    }

    #[test]
    fn canonical_ids_are_tuple_determined_and_occurrence_sensitive() {
        let first =
            Relation::canonical_id(RelationKind::Binds, "symbol:s", "artifact:a", "bindings.0");
        let second =
            Relation::canonical_id(RelationKind::Binds, "symbol:s", "artifact:a", "bindings.1");
        assert_ne!(first, second);
        assert_eq!(
            first,
            Relation::canonical_id(RelationKind::Binds, "symbol:s", "artifact:a", "bindings.0")
        );
        assert!(first.starts_with("sha256:"));
    }

    #[test]
    fn confirmed_policy_requires_revision_origin_confidence_and_evidence() {
        let revision = "a".repeat(40);
        let mut relation = Relation {
            relation_id: String::new(),
            relation_kind: RelationKind::Implements,
            from_node: "symbol:s".to_owned(),
            to_node: "requirement:R".to_owned(),
            occurrence: "requirements.r".to_owned(),
            provenance: provenance(&revision),
            confidence: Confidence::Exact,
            status: Status::Confirmed,
            evidence_refs: vec!["gate:g".to_owned()],
        };
        relation.relation_id = Relation::canonical_id(
            relation.relation_kind,
            &relation.from_node,
            &relation.to_node,
            &relation.occurrence,
        );

        // Mismatched manifest revision.
        assert_eq!(
            relation.validate(&"b".repeat(40)),
            Err(RelationError::ConfirmedPolicy)
        );
        // Inferred origin never confirms.
        let mut inferred = relation.clone();
        inferred.provenance.origin = Origin::Inferred;
        assert_eq!(
            inferred.validate(&revision),
            Err(RelationError::ConfirmedPolicy)
        );
        // Low confidence never confirms.
        let mut low = relation.clone();
        low.confidence = Confidence::Low;
        assert_eq!(low.validate(&revision), Err(RelationError::ConfirmedPolicy));
        // No cited evidence never confirms.
        let mut bare = relation.clone();
        bare.evidence_refs.clear();
        assert_eq!(
            bare.validate(&revision),
            Err(RelationError::ConfirmedPolicy)
        );
        // Candidate is legal without evidence.
        let mut candidate = relation;
        candidate.status = Status::Candidate;
        candidate.evidence_refs.clear();
        assert_eq!(candidate.validate(&revision), Ok(()));
    }
}
