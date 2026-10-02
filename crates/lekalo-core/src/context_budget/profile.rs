//! The closed context-budget profile document (issue #75).
//!
//! A budget profile is a caller-selected, digest-bound JSON document that
//! pins one estimator identity, the budget arithmetic (window minus
//! reservations), the fact-selection recipe, and the hard resource
//! bounds. Documents are non-inheriting in v1, carry unique canonically
//! sorted profile ids, and normalize so that different declared key
//! orderings produce the same effective profile. Nothing here touches the
//! unrelated validation, target, impact, diff, storage, or native-gate
//! profile families, and no universal threshold exists: a caller without
//! an explicit budget or profile has no report.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::diagnostic;
use super::estimate;
use crate::context::version::MAX_BUDGET_TOKENS as LEGACY_MAX_BUDGET;
use crate::diagnostics::DiagnosticSet;
use crate::digest::sha256_hex;

/// The one accepted fact-selection recipe version.
pub const SELECTION_VERSION_REQUIRED: &str = "required-semantic-facts/1";

/// Whether `text` is a bounded wire identifier (1..=64 bytes, safe
/// grammar): hostile caller strings can never violate the profile
/// schema bounds.
fn is_bounded_identifier(text: &str) -> bool {
    let bytes = text.as_bytes();
    !text.is_empty()
        && text.len() <= 64
        && bytes[0].is_ascii_alphanumeric()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'/' | b':' | b'-'))
}

/// The exact wire discriminator of the profile contract.
pub const SCHEMA_VERSION: &str = "lekalo/context-budget-profile/v0.6.3";
/// The exact profile contract identity.
pub const IDENTITY: &str = "dev.lekalo.context-budget-profile@0.6.3";

/// The closed source-context selection vocabulary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SourceContext {
    /// No source measurement at all (the default semantic-only recipe).
    #[default]
    None,
    /// Opt-in whole-file accounting over current mapped files only.
    MappedFiles,
}

impl SourceContext {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MappedFiles => "mapped-files",
        }
    }
}

/// The budget arithmetic of one profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetWire {
    #[serde(rename = "contextWindowTokens")]
    pub context_window_tokens: u64,
    #[serde(rename = "reservedOutputTokens")]
    pub reserved_output_tokens: u64,
    #[serde(rename = "reservedSystemToolTokens")]
    pub reserved_system_tool_tokens: u64,
    #[serde(default, rename = "framingTokens")]
    pub framing_tokens: u64,
    #[serde(default = "one", rename = "marginNumerator")]
    pub margin_numerator: u64,
    #[serde(default = "one", rename = "marginDenominator")]
    pub margin_denominator: u64,
}

fn one() -> u64 {
    1
}

/// The hard resource bounds of one profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsWire {
    #[serde(default = "default_max_nodes", rename = "maxNodes")]
    pub max_nodes: u64,
    #[serde(default = "default_max_edges", rename = "maxEdges")]
    pub max_edges: u64,
    #[serde(default = "default_max_facts", rename = "maxFacts")]
    pub max_facts: u64,
    #[serde(default = "default_max_subjects", rename = "maxSubjects")]
    pub max_subjects: u64,
}

fn default_max_nodes() -> u64 {
    super::version::MAX_CLOSURE_NODES as u64
}

fn default_max_edges() -> u64 {
    super::version::MAX_CLOSURE_EDGES as u64
}

fn default_max_facts() -> u64 {
    super::version::MAX_FACTS as u64
}

fn default_max_subjects() -> u64 {
    super::version::MAX_SUBJECTS as u64
}

/// One named profile inside the document.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileEntryWire {
    pub id: String,
    pub version: String,
    pub estimator: EstimatorWire,
    pub budget: BudgetWire,
    #[serde(default)]
    pub selection: SelectionWire,
    #[serde(default)]
    pub limits: Option<LimitsWire>,
}

/// The estimator pin of one profile.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstimatorWire {
    pub id: String,
    pub version: String,
    #[serde(rename = "specDigest")]
    pub spec_digest: String,
}

/// The fact-selection recipe of one profile.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SelectionWire {
    pub version: String,
    #[serde(rename = "sourceContext")]
    pub source_context: SourceContextIn,
}

impl Default for SelectionWire {
    fn default() -> Self {
        Self {
            version: SELECTION_VERSION_REQUIRED.to_owned(),
            source_context: SourceContextIn::None,
        }
    }
}

/// The closed source-context wire vocabulary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceContextIn {
    #[default]
    None,
    MappedFiles,
}

/// One effective, validated profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    pub id: String,
    pub version: String,
    /// The fact-selection recipe version; only the pinned vocabulary is
    /// accepted, so the digest binds the exact selection semantics.
    pub selection_version: String,
    pub estimator_identity: String,
    pub estimator_version: String,
    pub estimator_digest: String,
    pub available_content_tokens: u64,
    pub framing_tokens: u64,
    pub margin_numerator: u64,
    pub margin_denominator: u64,
    pub source_context: SourceContext,
    pub max_nodes: u64,
    pub max_edges: u64,
    pub max_facts: u64,
    pub max_subjects: u64,
    /// The sha256 over the exact canonical normalized profile bytes.
    pub digest: String,
}

/// The whole profile document: the closed envelope plus its profiles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileDocument {
    pub profiles: BTreeMap<(String, String), Profile>,
}

impl ProfileDocument {
    /// Parse and validate profile bytes into the effective profiles.
    pub fn parse(bytes: &[u8]) -> Result<Self, DiagnosticSet> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct DocumentWire {
            #[serde(rename = "schemaVersion")]
            schema_version: String,
            identity: String,
            profiles: Vec<ProfileEntryWire>,
        }
        let wire: DocumentWire = serde_json::from_slice(bytes)
            .map_err(|_| diagnostic::input_invalid("profile-document-malformed"))?;
        if wire.schema_version != SCHEMA_VERSION || wire.identity != IDENTITY {
            return Err(diagnostic::profile_unsupported(
                "profile-contract-version",
                None,
            ));
        }
        let mut profiles = BTreeMap::new();
        for entry in wire.profiles {
            if !is_bounded_identifier(&entry.id) {
                return Err(diagnostic::input_invalid("profile-id-bounds"));
            }
            if !is_bounded_identifier(&entry.version) {
                return Err(diagnostic::input_invalid("profile-version-bounds"));
            }
            if entry.selection.version != SELECTION_VERSION_REQUIRED {
                return Err(diagnostic::profile_unsupported(
                    "selection-version",
                    Some(&entry.id),
                ));
            }
            let key = (entry.id.clone(), entry.version.clone());
            if profiles.contains_key(&key) {
                return Err(diagnostic::input_invalid("profile-id-duplicate"));
            }
            profiles.insert(key, effective_profile(entry)?);
        }
        if profiles.is_empty() {
            return Err(diagnostic::input_invalid("profile-document-empty"));
        }
        Ok(Self { profiles })
    }

    /// Resolve one named profile (`id`, `version` pair).
    pub fn resolve(&self, id: &str, version: &str) -> Result<&Profile, DiagnosticSet> {
        self.profiles
            .get(&(id.to_owned(), version.to_owned()))
            .ok_or_else(|| diagnostic::input_invalid_detail("profile-unknown", Some(id)))
    }
}

/// The generic chars-4 profile with an explicit caller budget: the only
/// non-file profile, pinned by the same estimator identity and digest.
pub fn generic_profile(budget: u64) -> Result<Profile, DiagnosticSet> {
    if budget == 0 || budget > LEGACY_MAX_BUDGET {
        return Err(diagnostic::input_invalid("budget-out-of-range"));
    }
    let mut profile = Profile {
        id: "chars-4-generic".to_owned(),
        version: "1".to_owned(),
        selection_version: SELECTION_VERSION_REQUIRED.to_owned(),
        estimator_identity: estimate::CHARS4_IDENTITY.to_owned(),
        estimator_version: estimate::CHARS4_VERSION.to_owned(),
        estimator_digest: estimate::chars4_digest(),
        available_content_tokens: budget,
        framing_tokens: 0,
        margin_numerator: 1,
        margin_denominator: 1,
        source_context: SourceContext::None,
        max_nodes: super::version::MAX_CLOSURE_NODES as u64,
        max_edges: super::version::MAX_CLOSURE_EDGES as u64,
        max_facts: super::version::MAX_FACTS as u64,
        max_subjects: super::version::MAX_SUBJECTS as u64,
        digest: String::new(),
    };
    profile.digest = profile_digest(&profile);
    Ok(profile)
}

/// Validate one entry into its effective profile.
fn effective_profile(entry: ProfileEntryWire) -> Result<Profile, DiagnosticSet> {
    if !estimate::accepted(&entry.estimator.id) {
        return Err(diagnostic::profile_unsupported("estimator-identity", None));
    }
    if entry.estimator.version != estimate::CHARS4_VERSION
        || entry.estimator.spec_digest != estimate::chars4_digest()
    {
        return Err(diagnostic::profile_unsupported(
            "estimator-spec-digest",
            None,
        ));
    }
    let available = estimate::available_content_tokens(
        entry.budget.context_window_tokens,
        entry.budget.reserved_output_tokens,
        entry.budget.reserved_system_tool_tokens,
    )?;
    if entry.budget.margin_denominator == 0 {
        return Err(diagnostic::input_invalid("profile-margin-denominator"));
    }
    let source_context = match entry.selection.source_context {
        SourceContextIn::None => SourceContext::None,
        SourceContextIn::MappedFiles => SourceContext::MappedFiles,
    };
    let limits = entry.limits.unwrap_or(LimitsWire {
        max_nodes: default_max_nodes(),
        max_edges: default_max_edges(),
        max_facts: default_max_facts(),
        max_subjects: default_max_subjects(),
    });
    let mut profile = Profile {
        id: entry.id,
        version: entry.version,
        selection_version: entry.selection.version,
        estimator_identity: entry.estimator.id,
        estimator_version: entry.estimator.version,
        estimator_digest: entry.estimator.spec_digest,
        available_content_tokens: available,
        framing_tokens: entry.budget.framing_tokens,
        margin_numerator: entry.budget.margin_numerator,
        margin_denominator: entry.budget.margin_denominator,
        source_context,
        max_nodes: limits.max_nodes,
        max_edges: limits.max_edges,
        max_facts: limits.max_facts,
        max_subjects: limits.max_subjects,
        digest: String::new(),
    };
    profile.digest = profile_digest(&profile);
    Ok(profile)
}

/// The canonical normalized digest of one effective profile: the exact
/// bytes of its typed fields in fixed order (field order normalizes any
/// declared document ordering).
fn profile_digest(profile: &Profile) -> String {
    let canonical = format!(
        "id={}|version={}|selection={}|estimator={}|estimator-version={}|estimator-digest={}|tokens={}|framing={}|margin={}/{}|source={}|nodes={}|edges={}|facts={}|subjects={}",
        profile.id,
        profile.version,
        profile.selection_version,
        profile.estimator_identity,
        profile.estimator_version,
        profile.estimator_digest,
        profile.available_content_tokens,
        profile.framing_tokens,
        profile.margin_numerator,
        profile.margin_denominator,
        profile.source_context.key(),
        profile.max_nodes,
        profile.max_edges,
        profile.max_facts,
        profile.max_subjects,
    );
    format!("sha256:{}", sha256_hex(canonical.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(profiles: &[&str]) -> String {
        format!(
            "{{\"schemaVersion\":\"{SCHEMA_VERSION}\",\"identity\":\"{IDENTITY}\",\"profiles\":[{}]}}",
            profiles.join(",")
        )
    }

    fn local_profile() -> String {
        format!(
            concat!(
                "{{\"id\":\"local-12k\",\"version\":\"1\",",
                "\"estimator\":{{\"id\":\"{}\",\"version\":\"{}\",\"specDigest\":\"{}\"}},",
                "\"budget\":{{\"contextWindowTokens\":16384,\"reservedOutputTokens\":2048,",
                "\"reservedSystemToolTokens\":2336,\"framingTokens\":0,",
                "\"marginNumerator\":1,\"marginDenominator\":1}},",
                "\"selection\":{{\"version\":\"required-semantic-facts/1\",",
                "\"sourceContext\":\"none\"}},",
                "\"limits\":{{\"maxNodes\":50000,\"maxEdges\":250000,",
                "\"maxFacts\":50000,\"maxSubjects\":10000}}}}"
            ),
            estimate::CHARS4_IDENTITY,
            estimate::CHARS4_VERSION,
            estimate::chars4_digest(),
        )
    }

    #[test]
    fn local_12k_resolves_to_12000_available_tokens() {
        let document = ProfileDocument::parse(document(&[&local_profile()]).as_bytes()).unwrap();
        let profile = document.resolve("local-12k", "1").unwrap();
        assert_eq!(profile.available_content_tokens, 12_000);
        assert_eq!(profile.source_context, SourceContext::None);
        assert!(profile.digest.starts_with("sha256:"));
    }

    #[test]
    fn key_order_does_not_change_the_digest() {
        let reordered = local_profile().replace(
            "\"id\":\"local-12k\",\"version\":\"1\"",
            "\"version\":\"1\",\"id\":\"local-12k\"",
        );
        let first = ProfileDocument::parse(document(&[&local_profile()]).as_bytes()).unwrap();
        let second = ProfileDocument::parse(document(&[&reordered]).as_bytes()).unwrap();
        assert_eq!(
            first.resolve("local-12k", "1").unwrap().digest,
            second.resolve("local-12k", "1").unwrap().digest
        );
    }

    #[test]
    fn unknown_estimator_and_bad_pins_refuse() {
        let bad_estimator = local_profile().replace(
            &format!("\"id\":\"{}\"", estimate::CHARS4_IDENTITY),
            "\"id\":\"dev.lekalo.estimator.claude\"",
        );
        assert!(ProfileDocument::parse(document(&[&bad_estimator]).as_bytes()).is_err());
        let bad_digest = local_profile().replace(
            &estimate::chars4_digest(),
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert!(ProfileDocument::parse(document(&[&bad_digest]).as_bytes()).is_err());
    }

    #[test]
    fn duplicates_empty_and_malformed_refuse() {
        assert!(
            ProfileDocument::parse(document(&[&local_profile(), &local_profile()]).as_bytes())
                .is_err()
        );
        assert!(ProfileDocument::parse(document(&[]).as_bytes()).is_err());
        assert!(ProfileDocument::parse(b"{").is_err());
        let extra = local_profile().replace("\"limits\":{", "\"wat\":1,\"limits\":{");
        assert!(ProfileDocument::parse(document(&[&extra]).as_bytes()).is_err());
    }

    #[test]
    fn generic_profile_pins_budget_bounds() {
        let profile = generic_profile(12_000).unwrap();
        assert_eq!(profile.available_content_tokens, 12_000);
        assert!(generic_profile(0).is_err());
        assert!(generic_profile(LEGACY_MAX_BUDGET + 1).is_err());
    }

    #[test]
    fn unknown_profile_refuses() {
        let document = ProfileDocument::parse(document(&[&local_profile()]).as_bytes()).unwrap();
        assert!(document.resolve("other", "1").is_err());
        assert!(document.resolve("local-12k", "2").is_err());
    }
}
