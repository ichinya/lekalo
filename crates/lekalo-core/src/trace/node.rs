//! Trace nodes: the closed discriminated union (issue #22).
//!
//! Seven node kinds cover the target chain — requirement, symbol,
//! artifact, scenario, native_test, gate, diagnostic — and each node may
//! carry external references that preserve foreign original ids verbatim
//! (OpenSpec requirements, HLV native tests/diagnostics, provider ids).
//! No OpenSpec, HLV, Scenario, Graph, or Artifact native wire object ever
//! enters a node: only opaque exact ids, contract versions, revisions, and
//! digests.

use super::id::{self, LogicalPath};
use serde::{Deserialize, Serialize};

/// The closed external source-system vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalSystem {
    /// OpenSpec requirements and change documents.
    Openspec,
    /// HLV gates, native tests, and diagnostics.
    Hlv,
    /// The target's own native test/diagnostic system.
    SourceNative,
    /// AIFHub and its registered providers.
    Aifhub,
    /// Lekalo itself (declared model/IR provenance).
    Lekalo,
}

impl ExternalSystem {
    /// The closed v1 systems in canonical order.
    pub const KEYS: [Self; 5] = [
        Self::Openspec,
        Self::Hlv,
        Self::SourceNative,
        Self::Aifhub,
        Self::Lekalo,
    ];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Openspec => "openspec",
            Self::Hlv => "hlv",
            Self::SourceNative => "source-native",
            Self::Aifhub => "aifhub",
            Self::Lekalo => "lekalo",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// The closed node kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Requirement,
    Symbol,
    Artifact,
    Scenario,
    NativeTest,
    Gate,
    Diagnostic,
}

impl NodeKind {
    /// The closed v1 kinds in canonical order.
    pub const KEYS: [Self; 7] = [
        Self::Requirement,
        Self::Symbol,
        Self::Artifact,
        Self::Scenario,
        Self::NativeTest,
        Self::Gate,
        Self::Diagnostic,
    ];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Requirement => "requirement",
            Self::Symbol => "symbol",
            Self::Artifact => "artifact",
            Self::Scenario => "scenario",
            Self::NativeTest => "native_test",
            Self::Gate => "gate",
            Self::Diagnostic => "diagnostic",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// The closed artifact ownership vocabulary (the artifact-manifest seam's
/// accepted ownership words; the seam itself is a typed reference here).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ownership {
    Generated,
    Scaffolded,
    Checked,
    External,
    Custom,
}

impl Ownership {
    /// The closed v1 ownership values in canonical order.
    pub const KEYS: [Self; 5] = [
        Self::Generated,
        Self::Scaffolded,
        Self::Checked,
        Self::External,
        Self::Custom,
    ];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Scaffolded => "scaffolded",
            Self::Checked => "checked",
            Self::External => "external",
            Self::Custom => "custom",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// One verbatim external reference: the foreign original id plus the
/// source contract/version and optional revision/digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExternalRef {
    pub system: ExternalSystem,
    pub original_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// The wire node: one closed discriminated union member.
///
/// Kind-specific identity fields are exclusive by contract: a node of one
/// kind must carry exactly its own identity field and none of the other
/// kinds' identity fields (the JSON Schema cannot express exclusivity of
/// declared keys; the validator enforces it, like the graph subkind
/// coherence rule).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Node {
    pub node_id: String,
    pub node_kind: NodeKind,
    // requirement
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_id: Option<String>,
    // symbol
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_id: Option<String>,
    // artifact
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership: Option<Ownership>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_digest: Option<String>,
    // scenario / native_test / gate / diagnostic
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_refs: Vec<ExternalRef>,
}

/// Node validation errors (fixed detail tags, never attacker echo).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeError {
    NodeId,
    Identity,
    ForeignIdentity,
    ArtifactId,
    Ownership,
    Path,
    Digest,
    Revision,
    GeneratorRef,
    ContractVersion,
    EvidenceDigest,
    ExternalId,
    ExternalContractVersion,
    ExternalRevision,
    ExternalDigest,
    ExternalOverLimit,
    ManifestDigest,
}

impl Node {
    /// The kind-specific identity id of this node, if present.
    pub fn identity(&self) -> Option<&str> {
        match self.node_kind {
            NodeKind::Requirement => self.requirement_id.as_deref(),
            NodeKind::Symbol => self.semantic_id.as_deref(),
            NodeKind::Artifact => self.artifact_id.as_deref(),
            NodeKind::Scenario => self.scenario_id.as_deref(),
            NodeKind::NativeTest => self.test_id.as_deref(),
            NodeKind::Gate => self.gate_id.as_deref(),
            NodeKind::Diagnostic => self.diagnostic_id.as_deref(),
        }
    }

    /// Whether any identity field this node's kind does not own is
    /// present.
    fn carries_foreign_identity(&self) -> bool {
        let present = [
            self.requirement_id.is_some(),
            self.semantic_id.is_some(),
            self.artifact_id.is_some(),
            self.scenario_id.is_some(),
            self.test_id.is_some(),
            self.gate_id.is_some(),
            self.diagnostic_id.is_some(),
        ];
        let owned_index = match self.node_kind {
            NodeKind::Requirement => 0,
            NodeKind::Symbol => 1,
            NodeKind::Artifact => 2,
            NodeKind::Scenario => 3,
            NodeKind::NativeTest => 4,
            NodeKind::Gate => 5,
            NodeKind::Diagnostic => 6,
        };
        present
            .into_iter()
            .enumerate()
            .any(|(index, is_present)| is_present && index != owned_index)
    }

    /// Validate every field against its closed grammar and the kind union
    /// rules.
    pub fn validate(&self) -> Result<(), NodeError> {
        if !id::is_node_id(&self.node_id) {
            return Err(NodeError::NodeId);
        }
        match self.identity() {
            Some(identity) if id::is_semantic_id(identity) => {}
            _ => return Err(NodeError::Identity),
        }
        if self.carries_foreign_identity() {
            return Err(NodeError::ForeignIdentity);
        }
        if let Some(ownership) = &self.ownership {
            if Ownership::parse(ownership.as_str()).is_none() {
                return Err(NodeError::Ownership);
            }
        }
        if let Some(version) = &self.contract_version {
            if !id::is_contract_version(version) {
                return Err(NodeError::ContractVersion);
            }
        }
        if let Some(digest) = &self.evidence_digest {
            if !id::is_digest(digest) {
                return Err(NodeError::EvidenceDigest);
            }
        }
        match self.node_kind {
            NodeKind::Artifact => {
                match self.artifact_id.as_deref() {
                    Some(artifact_id) if id::is_artifact_id(artifact_id) => {}
                    _ => return Err(NodeError::ArtifactId),
                }
                if self.ownership.is_none() {
                    return Err(NodeError::Ownership);
                }
                let Some(path) = &self.path else {
                    return Err(NodeError::Path);
                };
                LogicalPath::parse(path).map_err(|_| NodeError::Path)?;
                match self.content_digest.as_deref() {
                    Some(digest) if id::is_digest(digest) => {}
                    _ => return Err(NodeError::Digest),
                }
            }
            NodeKind::Requirement | NodeKind::Symbol => {}
            NodeKind::Scenario | NodeKind::NativeTest | NodeKind::Gate | NodeKind::Diagnostic => {}
        }
        if let Some(revision) = &self.revision {
            if !id::is_revision(revision) {
                return Err(NodeError::Revision);
            }
        }
        if let Some(generator) = &self.generator_ref {
            if !id::is_adapter_id(generator) {
                return Err(NodeError::GeneratorRef);
            }
        }
        if let Some(digest) = &self.manifest_digest {
            if !id::is_digest(digest) {
                return Err(NodeError::ManifestDigest);
            }
        }
        if self.external_refs.len() > super::version::MAX_EXTERNAL_REFS {
            return Err(NodeError::ExternalOverLimit);
        }
        for external in &self.external_refs {
            if !id::is_semantic_id(&external.original_id) {
                return Err(NodeError::ExternalId);
            }
            if let Some(version) = &external.contract_version {
                if !id::is_contract_version(version) {
                    return Err(NodeError::ExternalContractVersion);
                }
            }
            if let Some(revision) = &external.revision {
                if !id::is_revision(revision) {
                    return Err(NodeError::ExternalRevision);
                }
            }
            if let Some(digest) = &external.digest {
                if !id::is_digest(digest) {
                    return Err(NodeError::ExternalDigest);
                }
            }
        }
        Ok(())
    }
}

/// Sort external references into canonical order (unsigned UTF-8 by
/// system, then original id).
pub(crate) fn canonicalize_external_refs(refs: &mut [ExternalRef]) {
    refs.sort_by(|left, right| {
        (left.system.as_str(), left.original_id.as_str())
            .cmp(&(right.system.as_str(), right.original_id.as_str()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: NodeKind) -> Node {
        Node {
            node_id: format!("{}:x", kind.as_str()),
            node_kind: kind,
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
            gate_id: None,
            diagnostic_id: None,
            contract_version: None,
            evidence_digest: None,
            external_refs: Vec::new(),
        }
    }

    #[test]
    fn every_kind_identity_round_trips() {
        let mut requirement = node(NodeKind::Requirement);
        requirement.requirement_id = Some("PLANNER-REQ-001".to_owned());
        assert!(requirement.validate().is_ok());
        assert_eq!(requirement.identity(), Some("PLANNER-REQ-001"));

        let mut symbol = node(NodeKind::Symbol);
        symbol.semantic_id = Some("planner.focus_task".to_owned());
        assert!(symbol.validate().is_ok());
    }

    #[test]
    fn foreign_identity_fields_are_rejected() {
        let mut symbol = node(NodeKind::Symbol);
        symbol.semantic_id = Some("planner.focus_task".to_owned());
        symbol.gate_id = Some("gate.evict".to_owned());
        assert_eq!(symbol.validate(), Err(NodeError::ForeignIdentity));
    }

    #[test]
    fn artifact_nodes_require_path_and_digest() {
        let mut artifact = node(NodeKind::Artifact);
        artifact.artifact_id = Some("apps-api-focus-task".to_owned());
        assert_eq!(artifact.validate(), Err(NodeError::Ownership));
        artifact.ownership = Some(Ownership::Generated);
        assert_eq!(artifact.validate(), Err(NodeError::Path));
        artifact.path = Some("apps/api/src/planner/focus-task.ts".to_owned());
        assert_eq!(artifact.validate(), Err(NodeError::Digest));
        artifact.content_digest = Some(format!("sha256:{}", "a".repeat(64)));
        assert!(artifact.validate().is_ok());
    }

    #[test]
    fn hostile_node_ids_are_rejected() {
        let mut symbol = node(NodeKind::Symbol);
        symbol.node_id = " leading space".to_owned();
        symbol.semantic_id = Some("planner.focus_task".to_owned());
        assert_eq!(symbol.validate(), Err(NodeError::NodeId));
    }

    #[test]
    fn external_refs_preserve_foreign_ids_verbatim() {
        let mut symbol = node(NodeKind::Symbol);
        symbol.semantic_id = Some("planner.focus_task".to_owned());
        symbol.external_refs = vec![ExternalRef {
            system: ExternalSystem::Openspec,
            original_id: "PLANNER-REQ-001".to_owned(),
            contract_version: None,
            revision: None,
            digest: None,
        }];
        assert!(symbol.validate().is_ok());
    }
}
