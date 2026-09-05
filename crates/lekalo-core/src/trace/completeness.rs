//! Completeness, gaps, and export profiles (issue #22).
//!
//! `full` requires zero gaps, every relation confirmed, and complete sink
//! coverage for the export profile (every sink node reachable from at
//! least one source node following relation directions). `partial`
//! requires at least one explicit gap. An unresolved endpoint is never
//! silently dropped: an ordinary relation naming an absent node is fatal,
//! and only an explicit gap may represent a dangling/unresolved external
//! reference. A gap can never be confirmed and never satisfies a gate.

use serde::{Deserialize, Serialize};

use super::node::NodeKind;
use super::provenance::Status;

/// The closed completeness claim vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Completeness {
    /// Zero gaps, all confirmed, complete sink coverage.
    Full,
    /// At least one explicit gap; no missing link disappears.
    Partial,
}

impl Completeness {
    /// The closed v1 values in canonical order.
    pub const KEYS: [Self; 2] = [Self::Full, Self::Partial];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// The closed export-profile vocabulary: the chain segment a manifest
/// claims to cover, evaluated as directed reachability from the profile's
/// source kinds to its sink kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportProfile {
    /// Every requirement reachable from a gate.
    RequirementToGate,
    /// Every requirement reachable from a native test.
    RequirementToTest,
    /// Every requirement reachable from a scenario.
    RequirementToScenario,
    /// Every artifact reachable from a gate.
    ArtifactToGate,
    /// Every artifact reachable from a native test.
    ArtifactToTest,
}

impl ExportProfile {
    /// The closed v1 profiles in canonical order.
    pub const KEYS: [Self; 5] = [
        Self::RequirementToGate,
        Self::RequirementToTest,
        Self::RequirementToScenario,
        Self::ArtifactToGate,
        Self::ArtifactToTest,
    ];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::RequirementToGate => "requirement-to-gate",
            Self::RequirementToTest => "requirement-to-test",
            Self::RequirementToScenario => "requirement-to-scenario",
            Self::ArtifactToGate => "artifact-to-gate",
            Self::ArtifactToTest => "artifact-to-test",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }

    /// The node kind every profile instance must cover.
    pub const fn sink(&self) -> NodeKind {
        match self {
            Self::RequirementToGate | Self::RequirementToTest | Self::RequirementToScenario => {
                NodeKind::Requirement
            }
            Self::ArtifactToGate | Self::ArtifactToTest => NodeKind::Artifact,
        }
    }

    /// The node kind coverage starts from.
    pub const fn source(&self) -> NodeKind {
        match self {
            Self::RequirementToGate | Self::ArtifactToGate => NodeKind::Gate,
            Self::RequirementToTest | Self::ArtifactToTest => NodeKind::NativeTest,
            Self::RequirementToScenario => NodeKind::Scenario,
        }
    }
}

/// The closed gap kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GapKind {
    MissingRequirement,
    MissingSymbol,
    MissingBinding,
    MissingArtifact,
    MissingScenario,
    MissingNativeTest,
    MissingGate,
    StaleRevision,
    DigestMismatch,
    Conflict,
    UnsupportedProvider,
    PrivacyRedacted,
}

impl GapKind {
    /// The closed v1 gap kinds in canonical order.
    pub const KEYS: [Self; 12] = [
        Self::MissingRequirement,
        Self::MissingSymbol,
        Self::MissingBinding,
        Self::MissingArtifact,
        Self::MissingScenario,
        Self::MissingNativeTest,
        Self::MissingGate,
        Self::StaleRevision,
        Self::DigestMismatch,
        Self::Conflict,
        Self::UnsupportedProvider,
        Self::PrivacyRedacted,
    ];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::MissingRequirement => "missing-requirement",
            Self::MissingSymbol => "missing-symbol",
            Self::MissingBinding => "missing-binding",
            Self::MissingArtifact => "missing-artifact",
            Self::MissingScenario => "missing-scenario",
            Self::MissingNativeTest => "missing-native-test",
            Self::MissingGate => "missing-gate",
            Self::StaleRevision => "stale-revision",
            Self::DigestMismatch => "digest-mismatch",
            Self::Conflict => "conflict",
            Self::UnsupportedProvider => "unsupported-provider",
            Self::PrivacyRedacted => "privacy-redacted",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// The wire gap: one explicit typed absence. A gap never carries
/// `confirmed` — the closed gap status set excludes it by construction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Gap {
    pub gap_kind: GapKind,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// Gap validation errors (fixed detail tags).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GapError {
    Status,
    Anchor,
    Expected,
    Path,
}

impl Gap {
    /// Validate every field: anchors and expectations must be
    /// grammar-legal, the source path privacy-safe, and the status never
    /// `confirmed` (excluded by the gap status vocabulary, re-checked on
    /// the parsed value so a future wire regression fails closed).
    pub fn validate(&self) -> Result<(), GapError> {
        if self.status == Status::Confirmed {
            return Err(GapError::Status);
        }
        if let Some(anchor) = &self.anchor_node {
            if !super::id::is_node_id(anchor) {
                return Err(GapError::Anchor);
            }
        }
        if let Some(expected) = &self.expected {
            if !super::id::is_semantic_id(expected) {
                return Err(GapError::Expected);
            }
        }
        if let Some(path) = &self.source_path {
            if super::id::LogicalPath::parse(path).is_err() {
                return Err(GapError::Path);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_pair_sources_and_sinks() {
        for profile in ExportProfile::KEYS {
            let source = profile.source();
            let sink = profile.sink();
            assert_ne!(source, sink);
            assert!(matches!(
                (profile, source),
                (ExportProfile::RequirementToGate, NodeKind::Gate)
                    | (ExportProfile::RequirementToTest, NodeKind::NativeTest)
                    | (ExportProfile::RequirementToScenario, NodeKind::Scenario)
                    | (ExportProfile::ArtifactToGate, NodeKind::Gate)
                    | (ExportProfile::ArtifactToTest, NodeKind::NativeTest)
            ));
            assert!(matches!(
                (profile, sink),
                (
                    ExportProfile::RequirementToGate
                        | ExportProfile::RequirementToTest
                        | ExportProfile::RequirementToScenario,
                    NodeKind::Requirement
                ) | (
                    ExportProfile::ArtifactToGate | ExportProfile::ArtifactToTest,
                    NodeKind::Artifact
                )
            ));
        }
    }

    #[test]
    fn gap_status_never_confirms() {
        let gap = Gap {
            gap_kind: GapKind::MissingGate,
            status: Status::Confirmed,
            anchor_node: None,
            expected: None,
            source_path: None,
        };
        assert_eq!(gap.validate(), Err(GapError::Status));

        let gap = Gap {
            gap_kind: GapKind::MissingGate,
            status: Status::Candidate,
            anchor_node: Some("symbol:planner.focus_task".to_owned()),
            expected: Some("hlv.gate.focus".to_owned()),
            source_path: Some("hlv/gates.md".to_owned()),
        };
        assert_eq!(gap.validate(), Ok(()));
    }

    #[test]
    fn wire_spellings_round_trip() {
        for key in GapKind::KEYS {
            assert_eq!(GapKind::parse(key.as_str()), Some(key));
        }
        assert_eq!(GapKind::parse("missing"), None);
        for key in ExportProfile::KEYS {
            assert_eq!(ExportProfile::parse(key.as_str()), Some(key));
        }
        assert_eq!(ExportProfile::parse("everything"), None);
    }
}
