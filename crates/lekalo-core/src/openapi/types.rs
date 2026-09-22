//! The closed configuration and finding surface of the OpenAPI
//! projection (issue #46).

use serde::{Deserialize, Serialize};

/// The closed declared OpenAPI version. `3.1` is the v1 default; a
/// `3.0` document is emitted only when explicitly declared and
/// refuses constructs it cannot express (`openapi.version-unsupported`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DocumentVersion {
    /// OpenAPI 3.1 (JSON Schema 2020-12 type arrays, `const`).
    V3_1,
    /// OpenAPI 3.0 (`nullable` siblings, single-value `enum`).
    V3_0,
}

impl DocumentVersion {
    /// The declared policy token (`3.1`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V3_1 => "3.1",
            Self::V3_0 => "3.0",
        }
    }

    /// The emitted `openapi` root member.
    pub const fn wire_str(self) -> &'static str {
        match self {
            Self::V3_1 => "3.1.0",
            Self::V3_0 => "3.0.0",
        }
    }

    /// The version for one declared policy token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "3.1" => Some(Self::V3_1),
            "3.0" => Some(Self::V3_0),
            _ => None,
        }
    }
}

/// The closed document mode of the target-document policy block.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DocumentMode {
    /// One generator-owned document at the declared path.
    Full,
    /// Per-endpoint fragments merged through the ownership manifest.
    Fragments,
}

impl DocumentMode {
    /// The declared policy token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Fragments => "fragments",
        }
    }

    /// The mode for one declared policy token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "full" => Some(Self::Full),
            "fragments" => Some(Self::Fragments),
            _ => None,
        }
    }
}

/// The declared render configuration: the OpenAPI version and the
/// document mode. Defaults follow the policy-block precedent
/// (`version: "3.1"`, `mode: full`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderConfig {
    /// The declared OpenAPI version.
    pub version: DocumentVersion,
    /// The declared document mode.
    pub mode: DocumentMode,
}

impl RenderConfig {
    /// The declared defaults: 3.1, full.
    pub fn new() -> Self {
        Self {
            version: DocumentVersion::V3_1,
            mode: DocumentMode::Full,
        }
    }

    /// Select the declared version.
    pub fn with_version(mut self, version: DocumentVersion) -> Self {
        self.version = version;
        self
    }

    /// Select the declared mode.
    pub fn with_mode(mut self, mode: DocumentMode) -> Self {
        self.mode = mode;
        self
    }
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// One member the declared sources cannot express in the projection.
/// Rendered as an `openapi.projection-partial` warning — the member is
/// reported, never silently dropped and never invented. The wire
/// carries `symbol:<id>` plus the fixed detail tag.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Finding {
    /// The fixed detail tag (`command-output-undeclared`,
    /// `scheme-not-expressible`, …).
    pub detail: String,
    /// The subject semantic id (or `symbol:<id>` rendering key).
    pub symbol: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_tokens_round_trip() {
        assert_eq!(DocumentVersion::parse("3.1"), Some(DocumentVersion::V3_1));
        assert_eq!(DocumentVersion::parse("3.0"), Some(DocumentVersion::V3_0));
        assert_eq!(DocumentVersion::parse("2.0"), None);
        assert_eq!(DocumentVersion::V3_1.wire_str(), "3.1.0");
        assert_eq!(DocumentVersion::V3_0.wire_str(), "3.0.0");
    }

    #[test]
    fn mode_tokens_round_trip() {
        assert_eq!(DocumentMode::parse("full"), Some(DocumentMode::Full));
        assert_eq!(
            DocumentMode::parse("fragments"),
            Some(DocumentMode::Fragments)
        );
        assert_eq!(DocumentMode::parse("whole"), None);
    }

    #[test]
    fn the_defaults_are_the_policy_block_defaults() {
        let config = RenderConfig::new();
        assert_eq!(config.version, DocumentVersion::V3_1);
        assert_eq!(config.mode, DocumentMode::Full);
    }
}
