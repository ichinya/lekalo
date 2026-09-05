//! Closed trace provenance, confidence, and status vocabulary (issue #22).
//!
//! `Provenance` is a closed object: where a relation came from, which
//! system recorded it, and the exact source revision/digest it was
//! confirmed against. `Confidence` is a closed deterministic enum; `Status`
//! is closed and never inferred from missing fields. A confirmed relation
//! requires matching revisions, non-inferred origin, exact-or-high
//! confidence, and cited evidence — inferred/candidate evidence never
//! counts as a passing gate.

use serde::{Deserialize, Serialize};

use super::id::{self, LogicalPath};

/// The closed provenance origin vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Stated directly by an accepted owner contract.
    Declared,
    /// Observed on the recorded evidence by a registered recorder.
    Observed,
    /// Derived by a recorded algorithm or adapter; never gate-passing.
    Inferred,
    /// Imported from an external system through a typed adapter.
    Imported,
}

impl Origin {
    /// The closed v1 origins in canonical order.
    pub const KEYS: [Self; 4] = [
        Self::Declared,
        Self::Observed,
        Self::Inferred,
        Self::Imported,
    ];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Observed => "observed",
            Self::Inferred => "inferred",
            Self::Imported => "imported",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// The closed confidence vocabulary (ADR-0014 owner decision: words, not
/// numbers — numeric confidence would need a decimal canonicalization rule
/// this contract does not carry).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Identity-exact: machine-checked against the cited evidence.
    Exact,
    /// High: recorded by the accepted owner with a matching digest.
    High,
    /// Medium: consistent evidence, not re-verified.
    Medium,
    /// Low: weak or indirect evidence.
    Low,
    /// Unknown: no confidence statement.
    Unknown,
}

impl Confidence {
    /// The closed v1 confidences in canonical order.
    pub const KEYS: [Self; 5] = [
        Self::Exact,
        Self::High,
        Self::Medium,
        Self::Low,
        Self::Unknown,
    ];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// The closed relation/gap status vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Resolved against matching revisions and acceptable provenance.
    Confirmed,
    /// Plausible but awaiting confirmation; never gate-passing.
    Candidate,
    /// Bound to a revision/digest that no longer matches.
    Stale,
    /// Contradicting evidence retained side by side; blocks per policy.
    Conflicting,
    /// Refused by validation.
    Invalid,
    /// The owning provider/adapter cannot express this link.
    Unsupported,
    /// Infrastructure failure (timeouts, cancellation); retryable upstream.
    Infrastructure,
}

impl Status {
    /// The closed v1 statuses in canonical order.
    pub const KEYS: [Self; 7] = [
        Self::Confirmed,
        Self::Candidate,
        Self::Stale,
        Self::Conflicting,
        Self::Invalid,
        Self::Unsupported,
        Self::Infrastructure,
    ];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Candidate => "candidate",
            Self::Stale => "stale",
            Self::Conflicting => "conflicting",
            Self::Invalid => "invalid",
            Self::Unsupported => "unsupported",
            Self::Infrastructure => "infrastructure",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// The closed provenance record every relation carries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Provenance {
    pub origin: Origin,
    pub source_system: String,
    pub source_revision: String,
    pub source_digest: String,
    pub recorded_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// Provenance validation errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProvenanceError {
    SourceSystem,
    Revision,
    Digest,
    RecordedBy,
    Path,
}

impl Provenance {
    /// Validate every field against its closed grammar.
    pub fn validate(&self) -> Result<(), ProvenanceError> {
        if super::node::ExternalSystem::parse(&self.source_system).is_none() {
            return Err(ProvenanceError::SourceSystem);
        }
        if !id::is_revision(&self.source_revision) {
            return Err(ProvenanceError::Revision);
        }
        if !id::is_digest(&self.source_digest) {
            return Err(ProvenanceError::Digest);
        }
        if !id::is_adapter_id(&self.recorded_by) {
            return Err(ProvenanceError::RecordedBy);
        }
        if let Some(path) = &self.source_path {
            if LogicalPath::parse(path).is_err() {
                return Err(ProvenanceError::Path);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_spellings_round_trip() {
        for key in Origin::KEYS {
            assert_eq!(Origin::parse(key.as_str()), Some(key));
        }
        for key in Confidence::KEYS {
            assert_eq!(Confidence::parse(key.as_str()), Some(key));
        }
        for key in Status::KEYS {
            assert_eq!(Status::parse(key.as_str()), Some(key));
        }
        assert_eq!(Origin::parse("other"), None);
        assert_eq!(Confidence::parse("canonical"), None);
        assert_eq!(Status::parse("open"), None);
    }

    #[test]
    fn provenance_validates_every_grammar() {
        let ok = Provenance {
            origin: Origin::Declared,
            source_system: "openspec".to_owned(),
            source_revision: "a".repeat(40),
            source_digest: format!("sha256:{}", "b".repeat(64)),
            recorded_by: "lekalo.core".to_owned(),
            source_path: Some("docs/requirements.md".to_owned()),
        };
        assert_eq!(ok.validate(), Ok(()));

        let mut bad = ok.clone();
        bad.source_system = "wiki".to_owned();
        assert_eq!(bad.validate(), Err(ProvenanceError::SourceSystem));

        let mut bad = ok.clone();
        bad.source_revision = "HEAD".to_owned();
        assert_eq!(bad.validate(), Err(ProvenanceError::Revision));

        let mut bad = ok.clone();
        bad.source_digest = "sha256:short".to_owned();
        assert_eq!(bad.validate(), Err(ProvenanceError::Digest));

        let mut bad = ok.clone();
        bad.recorded_by = "Lekalo Core".to_owned();
        assert_eq!(bad.validate(), Err(ProvenanceError::RecordedBy));

        let mut bad = ok;
        bad.source_path = Some("/etc/passwd".to_owned());
        assert_eq!(bad.validate(), Err(ProvenanceError::Path));
    }
}
