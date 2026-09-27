//! Client-SDK artifact impact and the consumer index (issue #72).
//!
//! [`ClientArtifactIndex`] relates one client artifact to its safe
//! logical path, language, contract fingerprint, endpoint coverage,
//! and declared consumer ids. Consumers are explicit maintained
//! declarations, never inferred from network traffic or imports.
//!
//! [`affected_clients`] joins a base/candidate transport-attachment
//! pair (through the #70 comparison) with before/after indexes so an
//! endpoint removal still finds the consumers of the artifact that
//! covered it. Unregistered consumers and stale inventories stay
//! visible: the result carries an explicit incomplete state, never a
//! guessed pass.

use crate::diagnostics::DiagnosticSet;
use crate::scenario::id::SemanticId;
use crate::transport_http::{self, TransportDocument};

use super::diagnostic;
use super::id::{ConsumerId, Language};
use super::version;

/// One registered client artifact in the index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientArtifactEntry {
    /// The stable artifact id (`planner.clients.typescript`).
    pub artifact_id: String,
    /// The safe project-relative logical path of the artifact root.
    pub path: String,
    /// The target language.
    pub language: Language,
    /// The digest of the contract the artifact was generated from.
    pub contract_digest: String,
    /// The endpoint ids the artifact covers.
    pub endpoints: Vec<SemanticId>,
    /// The declared consumer ids of this artifact.
    pub consumers: Vec<ConsumerId>,
}

/// The consumer/artifact index of one project.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ClientArtifactIndex {
    entries: Vec<ClientArtifactEntry>,
}

impl ClientArtifactIndex {
    /// Assemble one index from validated entries; entries sort by
    /// artifact id and duplicates refuse.
    pub fn new(mut entries: Vec<ClientArtifactEntry>) -> Result<Self, DiagnosticSet> {
        if entries.len() > version::MAX_CONSUMERS {
            return Err(diagnostic::rule_invalid(
                diagnostic::LIMIT_EXCEEDED,
                "index-bound",
                None,
            ));
        }
        entries.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
        if entries
            .windows(2)
            .any(|window| window[0].artifact_id == window[1].artifact_id)
        {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONSUMER_INVALID,
                "duplicate-artifact",
                None,
            ));
        }
        Ok(Self { entries })
    }

    /// The empty index.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Every registered artifact, artifact-id order.
    pub fn entries(&self) -> &[ClientArtifactEntry] {
        &self.entries
    }

    /// Whether the index declares no artifact at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// One artifact by id.
    pub fn artifact(&self, id: &str) -> Option<&ClientArtifactEntry> {
        self.entries.iter().find(|entry| entry.artifact_id == id)
    }

    /// Every artifact covering one endpoint.
    pub fn artifacts_of_endpoint(&self, endpoint: &str) -> Vec<&ClientArtifactEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.endpoints.iter().any(|id| id.as_str() == endpoint))
            .collect()
    }

    /// The union of declared consumer ids over one endpoint's
    /// artifacts, id-sorted, deduplicated.
    pub fn consumers_of_endpoint(&self, endpoint: &str) -> Vec<&ConsumerId> {
        let mut consumers: Vec<&ConsumerId> = self
            .artifacts_of_endpoint(endpoint)
            .into_iter()
            .flat_map(|entry| entry.consumers.iter())
            .collect();
        consumers.sort();
        consumers.dedup();
        consumers
    }
}

/// The closed kind of one affected-client finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum AffectedKind {
    /// The endpoint wire surface changed under the artifact.
    EndpointChanged,
    /// The endpoint (and any wire guarantee it carried) is gone.
    EndpointRemoved,
    /// The declared error union changed for one endpoint.
    ErrorChanged,
}

impl AffectedKind {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EndpointChanged => "endpoint-changed",
            Self::EndpointRemoved => "endpoint-removed",
            Self::ErrorChanged => "error-changed",
        }
    }
}

/// One affected client artifact/consumer finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AffectedClient {
    /// The affected artifact id.
    pub artifact_id: String,
    /// The closed kind of the impact.
    pub kind: AffectedKind,
    /// The changed endpoint id, when the impact is endpoint-scoped.
    pub endpoint: Option<SemanticId>,
    /// The affected consumer ids, id-sorted.
    pub consumers: Vec<String>,
}

/// Compute the affected clients of one transport-attachment change.
/// The before index carries the base generation's artifacts, so a
/// removed endpoint still resolves its old consumers. The result is
/// byte-deterministic: artifact-id order, then kind, then endpoint.
pub fn affected_clients(
    base: &TransportDocument,
    candidate: &TransportDocument,
    before: &ClientArtifactIndex,
) -> Result<(Vec<AffectedClient>, bool), DiagnosticSet> {
    let diff = transport_http::compare(base, candidate)?;
    let mut affected: Vec<AffectedClient> = Vec::new();
    for path in diff.paths() {
        // The diff path spells `endpoints/<id>/...`; the endpoint id
        // is the second segment. A removal is the bare
        // `endpoints/<id>` (no member segment) classified breaking —
        // the candidate no longer declares the endpoint. Non-endpoint
        // paths (document defaults) affect every artifact of the
        // project.
        let endpoint_segments = path
            .path()
            .strip_prefix("endpoints/")
            .map(|rest| rest.split('/').collect::<Vec<_>>());
        let endpoint_id = endpoint_segments
            .as_ref()
            .map(|segments| segments[0].to_owned());
        let is_endpoint_removal = matches!(&endpoint_segments, Some(segments) if segments.len() == 1)
            && path.class() == transport_http::DiffClass::Breaking
            && candidate.endpoint(endpoint_id.as_deref().unwrap_or_default()).is_none();
        let kind = match path.class() {
            transport_http::DiffClass::Breaking => {
                if is_endpoint_removal {
                    AffectedKind::EndpointRemoved
                } else {
                    AffectedKind::EndpointChanged
                }
            }
            transport_http::DiffClass::PolicyChange => {
                // Policy-class members reshape the projection without
                // removing a wire guarantee; only the error map
                // members change the typed union the clients decode,
                // so the kind names the actual member family.
                let member = endpoint_segments
                    .as_ref()
                    .and_then(|segments| segments.get(1).copied())
                    .unwrap_or_default();
                if member == "errors" {
                    AffectedKind::ErrorChanged
                } else {
                    AffectedKind::EndpointChanged
                }
            }
            transport_http::DiffClass::NonBreaking => continue,
        };
        let covering = match &endpoint_id {
            Some(id) => before.artifacts_of_endpoint(id),
            None => before.entries().iter().collect(),
        };
        for entry in covering {
            let consumers = entry
                .consumers
                .iter()
                .map(|consumer| consumer.as_str().to_owned())
                .collect();
            affected.push(AffectedClient {
                artifact_id: entry.artifact_id.clone(),
                kind,
                endpoint: endpoint_id
                    .as_ref()
                    .and_then(|id| SemanticId::parse(id).ok()),
                consumers,
            });
        }
    }
    affected.sort_by(|left, right| {
        (&left.artifact_id, left.kind.as_str(), left.endpoint.as_ref().map(|e| e.as_str()))
            .cmp(&(
                &right.artifact_id,
                right.kind.as_str(),
                right.endpoint.as_ref().map(|e| e.as_str()),
            ))
    });
    affected.dedup_by(|left, right| {
        left.artifact_id == right.artifact_id
            && left.kind == right.kind
            && left.endpoint == right.endpoint
    });
    if affected.len() > version::MAX_AFFECTED {
        return Err(diagnostic::rule_invalid(
            diagnostic::LIMIT_EXCEEDED,
            "affected-bound",
            None,
        ));
    }
    // Completeness: an empty before-index can only name nothing —
    // the inventory is incomplete, not clean.
    let complete = !before.is_empty();
    Ok((affected, complete))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, endpoints: &[&str], consumers: &[&str]) -> ClientArtifactEntry {
        ClientArtifactEntry {
            artifact_id: id.to_owned(),
            path: format!(".lekalo/generated/clients/{id}"),
            language: Language::Typescript,
            contract_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .to_owned(),
            endpoints: endpoints
                .iter()
                .map(|endpoint| SemanticId::parse(endpoint).expect("endpoint id"))
                .collect(),
            consumers: consumers
                .iter()
                .map(|consumer| ConsumerId::parse(consumer).expect("consumer"))
                .collect(),
        }
    }

    #[test]
    fn index_sorts_and_refuses_duplicates() {
        let index = ClientArtifactIndex::new(vec![
            entry("planner.clients.go", &["planner.endpoint_focus_task"], &["billing"]),
            entry("planner.clients.php", &[], &[]),
        ])
        .expect("index");
        assert_eq!(index.entries().len(), 2);
        assert_eq!(
            index.entries()[0].artifact_id, "planner.clients.go",
            "id-sorted"
        );
        assert!(ClientArtifactIndex::new(vec![
            entry("planner.clients.ts", &[], &[]),
            entry("planner.clients.ts", &[], &[]),
        ])
        .is_err());
    }

    #[test]
    fn consumers_resolve_over_shared_endpoints() {
        let index = ClientArtifactIndex::new(vec![
            entry(
                "planner.clients.ts",
                &["planner.endpoint_focus_task", "planner.endpoint_list_tasks"],
                &["web_console", "billing"],
            ),
            entry(
                "planner.clients.go",
                &["planner.endpoint_focus_task"],
                &["billing", "audit"],
            ),
        ])
        .expect("index");
        let consumers = index.consumers_of_endpoint("planner.endpoint_focus_task");
        let spelled: Vec<&str> = consumers.iter().map(|consumer| consumer.as_str()).collect();
        assert_eq!(spelled, vec!["audit", "billing", "web_console"], "sorted, deduped");
    }
}
