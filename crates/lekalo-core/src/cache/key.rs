//! Typed cache keys and their digests (issue #20).
//!
//! A key is a closed, typed identity — never a path string or a database
//! row. Every downstream key embeds its upstream digests, so a changed
//! source byte invalidates exactly the downstream closure and unrelated
//! modules keep their entries. The key digest is SHA-256 over the
//! deterministic canonical key bytes.

use serde::{Deserialize, Serialize};

use super::canonical::{canonical_bytes, is_digest, sha256_digest};

/// The closed record-kind vocabulary of v1.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RecordKind {
    Source,
    ParsedFragment,
    IrFragment,
    GraphFragment,
    EffectFragment,
    AdapterCapability,
    AdapterResult,
    ArtifactManifestKey,
    ContextKey,
}

impl RecordKind {
    /// The kebab-case wire spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::ParsedFragment => "parsed-fragment",
            Self::IrFragment => "ir-fragment",
            Self::GraphFragment => "graph-fragment",
            Self::EffectFragment => "effect-fragment",
            Self::AdapterCapability => "adapter-capability",
            Self::AdapterResult => "adapter-result",
            Self::ArtifactManifestKey => "artifact-manifest-key",
            Self::ContextKey => "context-key",
        }
    }

    /// Parse the wire spelling back to the closed kind.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let kind = match text {
            "source" => Self::Source,
            "parsed-fragment" => Self::ParsedFragment,
            "ir-fragment" => Self::IrFragment,
            "graph-fragment" => Self::GraphFragment,
            "effect-fragment" => Self::EffectFragment,
            "adapter-capability" => Self::AdapterCapability,
            "adapter-result" => Self::AdapterResult,
            "artifact-manifest-key" => Self::ArtifactManifestKey,
            "context-key" => Self::ContextKey,
            _ => return None,
        };
        Some(kind)
    }
}

/// The closed typed key of one cache entry.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Key {
    Source {
        path: String,
        raw_digest: String,
        normalized_digest: String,
    },
    ParsedFragment {
        path: String,
        raw_digest: String,
    },
    IrFragment {
        model_version: String,
        parsed_digests: Vec<String>,
    },
    GraphFragment {
        ir_digest: String,
    },
    EffectFragment {
        graph_digest: String,
    },
    AdapterCapability {
        adapter_id: String,
        protocol_version: String,
        executable_digest: String,
    },
    AdapterResult {
        capability_digest: String,
        input_digest: String,
        profile_digest: String,
    },
    #[serde(rename = "artifact-manifest-key")]
    ArtifactManifest {
        manifest_schema_digest: String,
        lock_revision: String,
        artifact_digest: String,
    },
    #[serde(rename = "context-key")]
    Context {
        context_schema_digest: String,
        scope_digest: String,
        profile_digest: String,
        budget: u64,
    },
}

/// Maximum source-path bytes accepted inside one key.
pub(crate) const MAX_KEY_PATH_BYTES: usize = 512;

impl Key {
    /// The record kind this key belongs to.
    pub(crate) fn kind(&self) -> RecordKind {
        match self {
            Self::Source { .. } => RecordKind::Source,
            Self::ParsedFragment { .. } => RecordKind::ParsedFragment,
            Self::IrFragment { .. } => RecordKind::IrFragment,
            Self::GraphFragment { .. } => RecordKind::GraphFragment,
            Self::EffectFragment { .. } => RecordKind::EffectFragment,
            Self::AdapterCapability { .. } => RecordKind::AdapterCapability,
            Self::AdapterResult { .. } => RecordKind::AdapterResult,
            Self::ArtifactManifest { .. } => RecordKind::ArtifactManifestKey,
            Self::Context { .. } => RecordKind::ContextKey,
        }
    }

    /// The `sha256:<hex>` digest over the canonical key bytes.
    pub(crate) fn digest(&self) -> String {
        sha256_digest(&canonical_bytes(&self))
    }

    /// Validate the bounded, well-formed subset every key must satisfy
    /// before the store touches it: digest spellings and path bounds.
    /// Unknown malformed content is a miss, never a trusted hit.
    pub(crate) fn is_well_formed(&self) -> bool {
        let digest_ok = |digest: &str| is_digest(digest);
        let path_ok = |path: &str| {
            !path.is_empty()
                && path.len() <= MAX_KEY_PATH_BYTES
                && path
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() && byte != b'\\')
        };
        match self {
            Self::Source {
                path,
                raw_digest,
                normalized_digest,
            } => path_ok(path) && digest_ok(raw_digest) && digest_ok(normalized_digest),
            Self::ParsedFragment { path, raw_digest } => path_ok(path) && digest_ok(raw_digest),
            Self::IrFragment {
                model_version,
                parsed_digests,
            } => {
                let version_ok = matches!(model_version.as_str(), "0.1.0" | "1.0.0");
                version_ok
                    && !parsed_digests.is_empty()
                    && parsed_digests.len() <= super::limits::MAX_ENTRIES
                    && parsed_digests.iter().all(|digest| digest_ok(digest))
                    && is_sorted_unique(parsed_digests)
            }
            Self::GraphFragment { ir_digest } => digest_ok(ir_digest),
            Self::EffectFragment { graph_digest } => digest_ok(graph_digest),
            Self::AdapterCapability {
                adapter_id,
                protocol_version,
                executable_digest,
            } => {
                !adapter_id.is_empty()
                    && adapter_id.len() <= 256
                    && !protocol_version.is_empty()
                    && protocol_version.len() <= 64
                    && digest_ok(executable_digest)
            }
            Self::AdapterResult {
                capability_digest,
                input_digest,
                profile_digest,
            } => {
                digest_ok(capability_digest) && digest_ok(input_digest) && digest_ok(profile_digest)
            }
            Self::ArtifactManifest {
                manifest_schema_digest,
                lock_revision,
                artifact_digest,
            } => {
                digest_ok(manifest_schema_digest)
                    && digest_ok(lock_revision)
                    && digest_ok(artifact_digest)
            }
            Self::Context {
                context_schema_digest,
                scope_digest,
                profile_digest,
                budget,
            } => {
                digest_ok(context_schema_digest)
                    && digest_ok(scope_digest)
                    && digest_ok(profile_digest)
                    && (1..=10_000_000).contains(budget)
            }
        }
    }
}

/// Whether the slice is strictly sorted with no duplicates.
pub(crate) fn is_sorted_unique(values: &[String]) -> bool {
    values
        .windows(2)
        .all(|pair| pair[0].as_bytes() < pair[1].as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    fn hex(suffix: u8) -> String {
        format!("sha256:{suffix:064x}")
    }

    #[test]
    fn source_keys_embed_path_and_content_digest() {
        let key = Key::Source {
            path: "lekalo/modules/planner/entities.yaml".to_owned(),
            raw_digest: HEX.to_owned(),
            normalized_digest: HEX.to_owned(),
        };
        assert_eq!(key.kind(), RecordKind::Source);
        assert!(key.is_well_formed());
        let again = key.digest();
        assert_eq!(key.digest(), again);
        assert!(again.starts_with("sha256:"));
    }

    #[test]
    fn different_content_changes_the_key_digest() {
        let first = Key::Source {
            path: "a.yaml".to_owned(),
            raw_digest: hex(1),
            normalized_digest: HEX.to_owned(),
        };
        let second = Key::Source {
            path: "a.yaml".to_owned(),
            raw_digest: hex(2),
            normalized_digest: HEX.to_owned(),
        };
        assert_ne!(first.digest(), second.digest());
    }

    #[test]
    fn ir_keys_require_sorted_unique_digest_sets() {
        let ok = Key::IrFragment {
            model_version: "1.0.0".to_owned(),
            parsed_digests: vec![hex(1), hex(2)],
        };
        assert!(ok.is_well_formed());
        let unsorted = Key::IrFragment {
            model_version: "1.0.0".to_owned(),
            parsed_digests: vec![hex(2), hex(1)],
        };
        assert!(!unsorted.is_well_formed());
        let duplicated = Key::IrFragment {
            model_version: "1.0.0".to_owned(),
            parsed_digests: vec![hex(1), hex(1)],
        };
        assert!(!duplicated.is_well_formed());
        let wrong_version = Key::IrFragment {
            model_version: "2.0.0".to_owned(),
            parsed_digests: vec![hex(1)],
        };
        assert!(!wrong_version.is_well_formed());
    }

    #[test]
    fn hostile_paths_and_digests_are_not_well_formed() {
        let backslash = Key::Source {
            path: "lekalo\\planner.yaml".to_owned(),
            raw_digest: HEX.to_owned(),
            normalized_digest: HEX.to_owned(),
        };
        assert!(!backslash.is_well_formed());
        let too_long = Key::Source {
            path: "a/".repeat(600),
            raw_digest: HEX.to_owned(),
            normalized_digest: HEX.to_owned(),
        };
        assert!(!too_long.is_well_formed());
        let bad_digest = Key::Source {
            path: "a.yaml".to_owned(),
            raw_digest: "sha512:xyz".to_owned(),
            normalized_digest: HEX.to_owned(),
        };
        assert!(!bad_digest.is_well_formed());
    }

    #[test]
    fn kind_wire_round_trip_covers_the_closed_vocabulary() {
        for kind in [
            RecordKind::Source,
            RecordKind::ParsedFragment,
            RecordKind::IrFragment,
            RecordKind::GraphFragment,
            RecordKind::EffectFragment,
            RecordKind::AdapterCapability,
            RecordKind::AdapterResult,
            RecordKind::ArtifactManifestKey,
            RecordKind::ContextKey,
        ] {
            assert_eq!(RecordKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(RecordKind::parse("wallet"), None);
    }
}
