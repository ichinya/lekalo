//! The embedded diagnostic-explanation catalog (issue #96).
//!
//! [`Catalog::embedded`] parses and validates the exact `include_bytes!`
//! catalog data once per process. The catalog is data, versioned together
//! with the diagnostic-registry union it explains: every registered code
//! has exactly one stable entry, and the catalog pins the exact registry
//! identity and digest it was generated from. Lookups are closed — an
//! unknown code is a typed [`ExplainError::UnknownCode`], never a guess.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// The exact embedded catalog bytes.
pub const CATALOG_BYTES: &[u8] =
    include_bytes!("../../../../contracts/explain-catalog.v0.8.0.json");

/// Why the embedded explanation catalog could not be trusted or queried.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    /// The bytes are not valid JSON or do not match the closed shape.
    Malformed,
    /// The discriminator or identity is unknown (unsupported, fail closed).
    Unsupported,
    /// A structural invariant (sort, uniqueness, registry pin) is violated.
    Invariant,
}

/// The closed fix risk-class vocabulary. It mirrors the versioned
/// fix-recipe classes plus `manual-only`: no severity or adapter
/// assertion can re-class a fix.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FixClass {
    Safe,
    Unsafe,
    Breaking,
    ManualOnly,
}

impl FixClass {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Unsafe => "unsafe",
            Self::Breaking => "breaking",
            Self::ManualOnly => "manual-only",
        }
    }
}

/// One safe/unsafe/breaking/manual-only remediation option. A
/// `recipeId`, when present, names the executable versioned fix recipe
/// this option describes; its presence is a pointer, never a claim that
/// the recipe applies to any current source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FixOption {
    pub class: FixClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe_id: Option<String>,
    pub description: String,
}

/// One before/after example pair.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Example {
    pub before: String,
    pub after: String,
}

/// One target-specific note. `scope` is `general` for target-neutral
/// entries and otherwise a target identifier.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TargetNote {
    pub scope: String,
    pub note: String,
}

/// One versioned documentation link. Links are per docs version so a
/// frozen catalog keeps pointing at its own frozen prose.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DocsLink {
    pub version: String,
    pub url: String,
}

/// Profile and severity behavior: the registry default severity, the
/// severities this rule may carry, the envelope statuses it may appear
/// under, whether it blocks (an envelope may be `invalid`), and the
/// closed waiver policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SeverityProfile {
    pub default_severity: String,
    pub severities: Vec<String>,
    pub allowed_statuses: Vec<String>,
    pub blocking: bool,
    pub waiver_policy: String,
}

/// One stable explanation: code/title/category, what the rule checks,
/// why it matters, root causes, fix options, examples, related codes,
/// profile behavior, target notes and versioned docs links.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Entry {
    pub code: String,
    pub rule_id: String,
    pub title: String,
    pub category: String,
    pub checks: String,
    pub rationale: String,
    pub root_causes: Vec<String>,
    pub fix_options: Vec<FixOption>,
    pub executable_recipe_id: Option<String>,
    pub examples: Vec<Example>,
    pub related_codes: Vec<String>,
    pub severity_profile: SeverityProfile,
    pub target_notes: Vec<TargetNote>,
    pub docs_links: Vec<DocsLink>,
}

impl Entry {
    /// The executable fix recipe this entry links to, if any. The link
    /// is advisory: `lekalo fix` still owns admission, preview and the
    /// exact plan confirmation for the current source.
    pub fn executable_recipe_id(&self) -> Option<&str> {
        self.executable_recipe_id.as_deref()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CatalogWire {
    schema_version: String,
    identity: String,
    catalog_version: String,
    registry_identity: String,
    registry_digest: String,
    closed: bool,
    entries: Vec<Entry>,
}

/// The validated explanation catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Catalog {
    catalog_version: String,
    registry_identity: String,
    registry_digest: String,
    entries: BTreeMap<String, Entry>,
    digest: String,
}

/// Why one explanation lookup failed. Unknown codes are distinct,
/// registered outcomes, never empty successes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExplainError {
    /// The embedded catalog failed validation (developer fault).
    Catalog(CatalogError),
    /// The selector is not explained by the embedded catalog. Carries
    /// the exact unknown input.
    UnknownCode(String),
    /// The selector is a registered diagnostic code or rule id, but the
    /// catalog entry's shape violates its invariants (developer fault).
    Invariant(String),
}

impl Catalog {
    /// The embedded, once-validated catalog.
    ///
    /// A validation failure is a developer fault: the error is cached
    /// and every caller fails closed instead of trusting the table.
    pub fn embedded() -> Result<&'static Self, CatalogError> {
        static CATALOG: LazyLock<Result<Catalog, CatalogError>> =
            LazyLock::new(|| Catalog::from_bytes(CATALOG_BYTES));
        CATALOG.as_ref().map_err(Clone::clone)
    }

    /// The catalog contract version.
    pub fn catalog_version(&self) -> &str {
        &self.catalog_version
    }

    /// The diagnostic-registry identity this catalog explains.
    pub fn registry_identity(&self) -> &str {
        &self.registry_identity
    }

    /// The exact `sha256:` digest of the explained registry bytes.
    pub fn registry_digest(&self) -> &str {
        &self.registry_digest
    }

    /// The exact `sha256:` digest of the embedded catalog bytes.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Number of explained codes.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the catalog is empty (never true for the embedded one).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every entry in canonical (diagnostic-code) order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// Closed lookup by diagnostic code (`LEK-HIDDEN-009`).
    pub fn entry(&self, code: &str) -> Result<&Entry, ExplainError> {
        self.entries
            .get(code)
            .ok_or_else(|| ExplainError::UnknownCode(code.to_owned()))
    }

    /// Closed lookup by rule id (`hidden.observer-write`), resolved to
    /// the entry keyed by that rule's stable code.
    pub fn entry_by_rule_id(&self, rule_id: &str) -> Result<&Entry, ExplainError> {
        self.entries
            .values()
            .find(|entry| entry.rule_id == rule_id)
            .ok_or_else(|| ExplainError::UnknownCode(rule_id.to_owned()))
    }

    /// Parse and validate catalog bytes: identity, closure, registry
    /// pin, canonical order, per-entry shape and recipe-reference
    /// sanity. Structural failures fail closed.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CatalogError> {
        let wire: CatalogWire =
            serde_json::from_slice(bytes).map_err(|_| CatalogError::Malformed)?;
        if wire.schema_version != super::version::CATALOG_SCHEMA_VERSION
            || wire.identity != super::version::CATALOG_IDENTITY
            || !wire.closed
            || wire.entries.is_empty()
        {
            return Err(CatalogError::Unsupported);
        }
        if !wire
            .registry_identity
            .starts_with("dev.lekalo.diagnostic-registry@")
            || !wire.registry_digest.starts_with("sha256:")
        {
            return Err(CatalogError::Invariant);
        }
        let mut entries = BTreeMap::new();
        let mut previous: Option<String> = None;
        for entry in wire.entries {
            if let Some(previous) = &previous {
                if entry.code.as_str() <= previous.as_str() {
                    // Entries must be sorted by unsigned UTF-8 code bytes
                    // and unique; a stale or reordered catalog fails closed.
                    return Err(CatalogError::Invariant);
                }
            }
            previous = Some(entry.code.clone());
            if entry.title.is_empty()
                || entry.title.len() > 200
                || entry.checks.is_empty()
                || entry.rationale.is_empty()
                || entry.root_causes.is_empty()
                || entry.fix_options.is_empty()
                || entry.examples.is_empty()
                || entry.target_notes.is_empty()
                || entry.docs_links.is_empty()
                || entry.severity_profile.severities.is_empty()
                || entry.severity_profile.allowed_statuses.is_empty()
            {
                return Err(CatalogError::Invariant);
            }
            entries.insert(entry.code.clone(), entry);
        }
        Ok(Self {
            catalog_version: wire.catalog_version,
            registry_identity: wire.registry_identity,
            registry_digest: wire.registry_digest,
            entries,
            digest: format!("sha256:{}", crate::digest::sha256_hex(bytes)),
        })
    }
}
