//! The closed `search-inputs` manifest (issue #93): the project-scoped,
//! caller-validated list of typed producer inputs an index build or an
//! explicit cold query admits.
//!
//! Only explicitly supported typed producers are legal: requirements
//! attachments/resolved reports, neutral trace manifests, observed
//! binding indexes, detected-effect evidence envelopes, extended-effect
//! error contracts, and waivers. A generic arbitrary JSON row is never
//! searchable evidence. The manifest carries relative project paths plus
//! exact contract identity and digest expectations; it never selects a
//! cache home and never widens an authority boundary.

use serde::{Deserialize, Serialize};

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{bound_token, token_value, DataObject};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

use super::version::{
    is_sha256, INPUTS_IDENTITY, INPUTS_SCHEMA_VERSION, MAX_PATH_BYTES, MAX_POSTINGS,
};

/// The registered rule reused for a malformed manifest or producer.
pub(crate) const INPUT_INVALID: &str = "search.input-invalid";
/// The registered rule reused for an unsupported contract version.
pub(crate) const VERSION_UNSUPPORTED: &str = "search.version-unsupported";

/// Finalize one registered diagnostic; a registry failure collapses to
/// the registry-invariant set (double developer fault).
pub(crate) fn one(
    id: &str,
    symbol: Option<String>,
    data: DataObject,
) -> Result<crate::diagnostics::Diagnostic, crate::diagnostics::normalize::BuildError> {
    build(id, symbol, None, data)
}

pub(crate) fn invalid_set(diagnostics: Vec<crate::diagnostics::Diagnostic>) -> DiagnosticSet {
    DiagnosticSet::try_from_unsorted(diagnostics, Status::Invalid)
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// The `search.input-invalid` rejection: malformed, duplicate-key, or
/// mixed project/revision/digest input (reason only; where a specific
/// upstream parse diagnostic exists, the producer keeps its own).
pub fn input_invalid(detail: &str, producer: Option<&str>) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    if let Some(producer) = producer {
        data.insert("producer".to_owned(), token_value(&bound_token(producer)));
    }
    match one(INPUT_INVALID, None, data) {
        Ok(diagnostic) => invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The closed typed producer kinds this issue admits.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProducerKind {
    /// The neutral trace manifest (`lekalo/trace-manifest/v0.2.16`).
    Trace,
    /// The observed binding index (`lekalo/observed-index/v0.4.0`).
    ObservedIndex,
    /// The detected-effect evidence envelope (`lekalo/extended-effects/v0.4.0`).
    Effects,
    /// The AI-lint waiver set (`lekalo/ai-lint-waivers/v0.6.5`).
    Waivers,
    /// The requirements attachment (`lekalo/requirements/v0.2.16`),
    /// decoded by its own published typed constructor.
    Requirements,
    /// The error-contract registry slice (`lekalo/error-registry/v0.2.16`),
    /// decoded and project-validated by the #62 owners' constructors.
    ErrorContract,
    /// The closed search diagnostic-set snapshot
    /// (`lekalo/search-diagnostics/v0.8.0`).
    Diagnostics,
    /// The closed artifact-ownership/profile snapshot
    /// (`lekalo/search-ownership-profile/v0.8.0`).
    OwnershipProfile,
}

impl ProducerKind {
    /// The closed kinds in canonical byte order.
    pub const KEYS: [Self; 8] = [
        Self::Diagnostics,
        Self::ErrorContract,
        Self::Effects,
        Self::ObservedIndex,
        Self::OwnershipProfile,
        Self::Requirements,
        Self::Trace,
        Self::Waivers,
    ];

    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::ObservedIndex => "observed-index",
            Self::Effects => "effects",
            Self::Waivers => "waivers",
            Self::Requirements => "requirements",
            Self::ErrorContract => "error-contract",
            Self::Diagnostics => "diagnostics",
            Self::OwnershipProfile => "ownership-profile",
        }
    }

    /// Registry lookup by exact key.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|kind| kind.key() == text)
    }

    /// The exact contract identity the producer must carry.
    pub const fn identity(self) -> &'static str {
        match self {
            Self::Trace => "dev.lekalo.trace-manifest@0.2.16",
            Self::ObservedIndex => "dev.lekalo.observed-index@0.2.16",
            Self::Effects => "dev.lekalo.effects@0.2.16",
            Self::Waivers => "dev.lekalo.ai-lint-waivers@0.6.5",
            Self::Requirements => "dev.lekalo.requirements@0.2.16",
            Self::ErrorContract => "dev.lekalo.error-registry@0.2.16",
            Self::Diagnostics => "dev.lekalo.search-diagnostics@0.8.0",
            Self::OwnershipProfile => "dev.lekalo.search-ownership-profile@0.8.0",
        }
    }
}

/// One admitted producer input: kind, relative path, and the exact
/// identity/digest expectations (when pinned).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputEntry {
    /// The closed producer kind.
    pub kind: ProducerKind,
    /// The project-relative path of the saved producer document.
    pub path: String,
    /// The exact contract identity the document must carry (when pinned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// The exact `sha256:<hex>` of the document bytes (when pinned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

impl InputEntry {
    fn validate(&self) -> Result<(), ManifestError> {
        if self.path.is_empty() || self.path.len() > MAX_PATH_BYTES {
            return Err(ManifestError::Invalid(input_invalid("path-bounds", None)));
        }
        // The path is a logical project-relative POSIX spelling under the
        // frozen privacy path policy: absolute forms (leading separator,
        // Windows drive, UNC, URI scheme, home), traversal segments, and
        // separator escapes are all refused before any read site is even
        // constructible. `Path::is_absolute` alone is platform-dependent,
        // so the closed textual grammar is the authority; the typed
        // component check below is the second, platform-native barrier.
        if crate::project_fs::path_violation(&self.path).is_some() {
            return Err(ManifestError::Invalid(input_invalid(
                "path-traversal",
                None,
            )));
        }
        let logical = std::path::Path::new(&self.path);
        if logical.is_absolute()
            || logical.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::Prefix(_)
                        | std::path::Component::RootDir
                        | std::path::Component::ParentDir
                )
            })
        {
            return Err(ManifestError::Invalid(input_invalid(
                "path-traversal",
                None,
            )));
        }
        if let Some(identity) = &self.identity {
            if identity != self.kind.identity() {
                return Err(ManifestError::Version(version_failure_with(
                    "identity-pin",
                    identity,
                )));
            }
        }
        if let Some(digest) = &self.digest {
            if !is_sha256(digest) {
                return Err(ManifestError::Invalid(input_invalid(
                    "digest-grammar",
                    None,
                )));
            }
        }
        Ok(())
    }
}

/// The parsed, validated input manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchInputs {
    identity: String,
    project_id: String,
    entries: Vec<InputEntry>,
    max_postings: usize,
}

impl SearchInputs {
    /// Parse manifest bytes (exact UTF-8 JSON) into the validated typed
    /// manifest, or return the terminal typed rejection. An unsupported
    /// schema/identity version is `ManifestError::Version` so callers
    /// keep the typed `unsupported-version` status and exit code.
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(rename = "schemaVersion")]
            schema_version: String,
            identity: String,
            #[serde(rename = "projectId")]
            project_id: String,
            inputs: Vec<WireEntry>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            limits: Option<WireLimits>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireEntry {
            kind: String,
            path: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            identity: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            digest: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireLimits {
            #[serde(
                rename = "maxPostings",
                default,
                skip_serializing_if = "Option::is_none"
            )]
            max_postings: Option<u64>,
        }
        // Duplicate JSON keys are rejected before decoding.
        let text = std::str::from_utf8(bytes)
            .map_err(|_| ManifestError::Invalid(input_invalid("invalid-encoding", None)))?;
        crate::search::wire::reject_duplicate_keys(text)
            .map_err(|_| ManifestError::Invalid(input_invalid("duplicate-key", None)))?;
        let wire: Wire = serde_json::from_str(text)
            .map_err(|_| ManifestError::Invalid(input_invalid("malformed", None)))?;
        if wire.schema_version != INPUTS_SCHEMA_VERSION {
            return Err(ManifestError::Version(version_failure_with(
                "inputs-schema",
                &wire.schema_version,
            )));
        }
        if wire.identity != INPUTS_IDENTITY {
            return Err(ManifestError::Version(version_failure_with(
                "inputs-identity",
                &wire.identity,
            )));
        }
        if wire.project_id.is_empty() || wire.project_id.len() > 192 {
            return Err(ManifestError::Invalid(input_invalid(
                "project-id-bounds",
                None,
            )));
        }
        if wire.inputs.len() > 64 {
            return Err(ManifestError::Invalid(input_invalid("input-count", None)));
        }
        let mut entries: Vec<InputEntry> = Vec::with_capacity(wire.inputs.len());
        let mut seen: Vec<(ProducerKind, String)> = Vec::new();
        for input in wire.inputs {
            let kind = ProducerKind::parse(&input.kind)
                .ok_or_else(|| ManifestError::Invalid(input_invalid("producer-unknown", None)))?;
            let entry = InputEntry {
                kind,
                path: input.path,
                identity: input.identity,
                digest: input.digest,
            };
            entry.validate()?;
            let key = (kind, entry.path.clone());
            if seen.contains(&key) {
                return Err(ManifestError::Invalid(input_invalid(
                    "duplicate-input",
                    None,
                )));
            }
            seen.push(key);
            entries.push(entry);
        }
        let max_postings = wire
            .limits
            .and_then(|limits| limits.max_postings)
            .unwrap_or(MAX_POSTINGS as u64);
        if max_postings == 0 || max_postings > MAX_POSTINGS as u64 {
            return Err(ManifestError::Invalid(input_invalid("limit-bounds", None)));
        }
        entries.sort_by(|left, right| {
            (left.kind.key(), left.path.as_bytes()).cmp(&(right.kind.key(), right.path.as_bytes()))
        });
        Ok(Self {
            identity: wire.identity,
            project_id: wire.project_id,
            entries,
            max_postings: max_postings as usize,
        })
    }

    /// The manifest contract identity.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The project the manifest is scoped to.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// The validated, canonically sorted entries.
    pub fn entries(&self) -> &[InputEntry] {
        &self.entries
    }

    /// The posting cap the manifest selected (bounded).
    pub fn max_postings(&self) -> usize {
        self.max_postings
    }

    /// The canonical manifest bytes of this parsed manifest: compact,
    /// byte-sorted keys, sorted entries, no trailing LF.
    pub fn canonical_bytes(&self) -> String {
        let mut json = String::from("{\"identity\":");
        json.push_str(&serde_json::to_string(&self.identity).expect("identity serializes"));
        json.push_str(",\"inputs\":[");
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str("{\"digest\":");
            json.push_str(&match &entry.digest {
                Some(digest) => serde_json::to_string(digest).expect("digest serializes"),
                None => "null".to_owned(),
            });
            json.push_str(",\"identity\":");
            json.push_str(&match &entry.identity {
                Some(identity) => serde_json::to_string(identity).expect("identity serializes"),
                None => "null".to_owned(),
            });
            json.push_str(",\"kind\":");
            json.push_str(&serde_json::to_string(entry.kind.key()).expect("kind serializes"));
            json.push_str(",\"path\":");
            json.push_str(&serde_json::to_string(&entry.path).expect("path serializes"));
            json.push('}');
        }
        json.push_str("],\"limits\":{\"maxPostings\":");
        json.push_str(&self.max_postings.to_string());
        json.push_str("},\"projectId\":");
        json.push_str(&serde_json::to_string(&self.project_id).expect("project serializes"));
        json.push_str(",\"schemaVersion\":");
        json.push_str(&serde_json::to_string(INPUTS_SCHEMA_VERSION).expect("const serializes"));
        json.push('}');
        json
    }
}

/// The `search.version-unsupported` failure carrying the declared
/// contract spelling (bounded, never a full echo of hostile bytes).
pub(crate) fn version_failure_with(detail: &'static str, contract: &str) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(detail));
    data.insert("contract".to_owned(), token_value(&bound_token(contract)));
    match one(VERSION_UNSUPPORTED, None, data) {
        Ok(diagnostic) => {
            DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::UnsupportedVersion)
                .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
        }
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The typed parse failure of one manifest: a closed-grammar refusal
/// (`invalid`, exit 1) or an unsupported contract version
/// (`unsupported-version`, exit 5) — never collapsed together.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    Invalid(DiagnosticSet),
    Version(DiagnosticSet),
}

impl ManifestError {
    /// The diagnostic set of this refusal.
    pub fn set(&self) -> &DiagnosticSet {
        match self {
            Self::Invalid(set) | Self::Version(set) => set,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_canonicalizes_the_closed_manifest() {
        let text = r#"{
            "schemaVersion": "lekalo/search-inputs/v0.8.0",
            "identity": "dev.lekalo.search-inputs@0.8.0",
            "projectId": "planner",
            "inputs": [
                {"kind": "trace", "path": "ai-factory/traces/plan.json"},
                {"kind": "effects", "path": "evidence/effects.json",
                 "identity": "dev.lekalo.effects@0.2.16",
                 "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"}
            ]
        }"#;
        let manifest = SearchInputs::parse(text.as_bytes()).expect("parses");
        assert_eq!(manifest.entries().len(), 2);
        assert_eq!(manifest.entries()[0].kind.key(), "effects");
        let canonical = manifest.canonical_bytes();
        assert_eq!(
            SearchInputs::parse(canonical.as_bytes()).expect("reparses"),
            manifest
        );
    }

    #[test]
    fn rejects_traversal_duplicates_and_unknown_kinds() {
        let ok = |body: &str| {
            format!(
                r#"{{"schemaVersion":"lekalo/search-inputs/v0.8.0","identity":"dev.lekalo.search-inputs@0.8.0","projectId":"planner","inputs":[{body}]}}"#
            )
        };
        assert!(
            SearchInputs::parse(ok(r#"{"kind":"trace","path":"../escape"}"#).as_bytes()).is_err()
        );
        assert!(SearchInputs::parse(ok(r#"{"kind":"trace","path":"/abs"}"#).as_bytes()).is_err());
        // Windows drive-absolute and UNC spellings never admit: the read
        // site would discard the project root on `Path::join`.
        assert!(SearchInputs::parse(
            ok(r#"{"kind":"trace","path":"C:/tmp/evil.json"}"#).as_bytes()
        )
        .is_err());
        assert!(
            SearchInputs::parse(ok(r#"{"kind":"trace","path":"D:\\evil.json"}"#).as_bytes())
                .is_err()
        );
        assert!(SearchInputs::parse(
            ok(r#"{"kind":"trace","path":"\\\\host/share/x"}"#).as_bytes()
        )
        .is_err());
        // URI-scheme and home-relative spellings are absolute forms too.
        assert!(SearchInputs::parse(
            ok(r#"{"kind":"trace","path":"file:///etc/passwd"}"#).as_bytes()
        )
        .is_err());
        assert!(
            SearchInputs::parse(ok(r#"{"kind":"trace","path":"~/escape"}"#).as_bytes()).is_err()
        );
        assert!(SearchInputs::parse(ok(r#"{"kind":"who","path":"x"}"#).as_bytes()).is_err());
        let duplicated_inputs = r#"{"schemaVersion":"lekalo/search-inputs/v0.8.0","identity":"dev.lekalo.search-inputs@0.8.0","projectId":"planner","inputs":[{"kind":"trace","path":"a"},{"kind":"trace","path":"a"}]}"#;
        assert!(SearchInputs::parse(duplicated_inputs.as_bytes()).is_err());
        // Duplicate JSON keys are rejected before decoding.
        let duplicated = r#"{"schemaVersion":"lekalo/search-inputs/v0.8.0","schemaVersion":"lekalo/search-inputs/v0.8.0","identity":"dev.lekalo.search-inputs@0.8.0","projectId":"planner","inputs":[]}"#;
        assert!(SearchInputs::parse(duplicated.as_bytes()).is_err());
    }
}
