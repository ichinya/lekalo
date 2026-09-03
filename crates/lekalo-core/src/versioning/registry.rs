//! The typed, embedded version registry (issue #9).
//!
//! The registry is the single source of truth for contract support policy:
//! which exact versions each family accepts, which are deprecated, which
//! are retired, the declared selector aliases, and the migration edges.
//! Support is exact-set membership — never a numeric interval — so a
//! well-formed but unregistered version such as `0.2.0` is unsupported even
//! though it lies between registered entries.
//!
//! The canonical bytes live in
//! `versioning/contracts/version-registry.v1.0.0.json`, are embedded with
//! [`include_bytes`], parsed once, and fully validated before use. A
//! registry that violates its own invariants is a developer fault:
//! [`VersionRegistry::embedded`] fails closed and the CLI renders
//! `versioning.registry-invalid` instead of guessing policy.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::LazyLock;

use serde::Deserialize;

use super::family::{
    ContractFamily, IrContract, ModelContract, ProtocolContract, RegistryContract,
};
use super::version::{ContractVersion, VersionParseError};

/// The embedded registry artifact identity (`dev.lekalo.version-registry`).
pub const REGISTRY_IDENTITY: &str = "dev.lekalo.version-registry";

/// The exact embedded registry bytes.
pub const REGISTRY_BYTES: &[u8] = include_bytes!("contracts/version-registry.v1.0.0.json");

/// Why the embedded registry data was rejected.
///
/// Details are developer-facing; CLI envelopes carry only the stable
/// `versioning.registry-invalid` reason code, never these strings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryError {
    /// Stable, sorted details of every violated invariant.
    pub details: Vec<String>,
}

impl RegistryError {
    pub(crate) fn single(detail: impl Into<String>) -> Self {
        Self {
            details: vec![detail.into()],
        }
    }
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("version registry invariants violated")
    }
}

/// The lifecycle state of one registered contract version.
///
/// Deprecated versions remain fully usable: loadable, validatable, and
/// migratable. Retirement is never automatic — it requires an explicit,
/// reviewed registry change, cannot take effect before
/// `retirement_not_before`, and makes the version fail closed with exit 5.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VersionState<F: ContractFamily> {
    /// Accepted for every operation.
    Supported,
    /// Accepted, with the deprecation window recorded in contract versions.
    Deprecated {
        /// The family version that deprecated this entry.
        deprecated_since: ContractVersion<F>,
        /// Retirement must not happen before this family version exists.
        retirement_not_before: ContractVersion<F>,
    },
    /// Rejected everywhere; recorded with its replacement, when one exists.
    Retired {
        /// The family version that retired this entry.
        retired_since: ContractVersion<F>,
        /// The replacement version when the entry was superseded.
        replacement: Option<ContractVersion<F>>,
    },
}

impl<F: ContractFamily> VersionState<F> {
    /// Whether a version in this state is still accepted for load, IR, and
    /// migration.
    pub const fn is_usable(&self) -> bool {
        matches!(self, Self::Supported | Self::Deprecated { .. })
    }

    /// The stable wire spelling used by the registry projection.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Deprecated { .. } => "deprecated",
            Self::Retired { .. } => "retired",
        }
    }
}

/// Why a version change exists: the closed change taxonomy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ChangeClassification {
    /// Removed or reordered core meaning; consumers must act.
    Breaking,
    /// Purely additive; unrelated artifacts are unaffected.
    Additive,
    /// Same schema, different behavior; never hidden in a patch.
    Behavioral,
}

impl ChangeClassification {
    /// Parse the registry's closed classification tokens.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "breaking" => Some(Self::Breaking),
            "additive" => Some(Self::Additive),
            "behavioral" => Some(Self::Behavioral),
            _ => None,
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::Additive => "additive",
            Self::Behavioral => "behavioral",
        }
    }
}

/// One declared selector alias (`v1`) bound to an exact registered version.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct AliasToken(String);

impl AliasToken {
    /// Aliases are exactly `v` plus a decimal major with no leading zeroes:
    /// `v1` is accepted, `vv1`, `v01`, and `v1.0` are not.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('v')?;
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        if digits.len() > 1 && digits.starts_with('0') {
            return None;
        }
        Some(Self(text.to_owned()))
    }

    /// The alias exactly as declared.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One registered version record with its written change classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionRecord<F: ContractFamily> {
    /// The exact registered version.
    pub version: ContractVersion<F>,
    /// The lifecycle state.
    pub state: VersionState<F>,
    /// The classification of the change that introduced this version.
    pub classification: ChangeClassification,
    /// The written policy reason (bounded, reviewable prose).
    pub reason: String,
}

/// One declared migration edge of a family's migration graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationEdge<F: ContractFamily> {
    /// The stable edge identity (also the implementation identity).
    pub id: String,
    /// The exact source version.
    pub from: ContractVersion<F>,
    /// The exact target version; strictly greater than `from`.
    pub to: ContractVersion<F>,
    /// The classification of the transformation.
    pub classification: ChangeClassification,
    /// The declared formatting/comment/manual-edit losses; empty is honest.
    pub loss: Vec<String>,
    /// Which unrelated artifacts regeneration must touch.
    pub regeneration_impact: RegenerationImpact,
    /// The written policy reason.
    pub reason: String,
}

/// The declared regeneration impact of a migration edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RegenerationImpact {
    /// No unrelated module needs regeneration.
    None,
    /// Only modules whose documents carried changed content.
    ChangedModules,
    /// Every module must be regenerated.
    All,
}

impl RegenerationImpact {
    /// Parse the registry's closed impact tokens.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "changed-modules" => Some(Self::ChangedModules),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ChangedModules => "changed-modules",
            Self::All => "all",
        }
    }
}

/// One family's registry: exact versions, lifecycle, aliases, and edges.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FamilyRegistry<F: ContractFamily> {
    current: Option<ContractVersion<F>>,
    versions: Vec<VersionRecord<F>>,
    aliases: Vec<(AliasToken, ContractVersion<F>)>,
    edges: Vec<MigrationEdge<F>>,
}

impl<F: ContractFamily> FamilyRegistry<F> {
    /// The current version; `None` only for an unpublished family.
    pub fn current(&self) -> Option<&ContractVersion<F>> {
        self.current.as_ref()
    }

    /// The registered records, sorted ascending by version.
    pub fn versions(&self) -> &[VersionRecord<F>] {
        &self.versions
    }

    /// The declared aliases, sorted by token bytes.
    pub fn aliases(&self) -> &[(AliasToken, ContractVersion<F>)] {
        &self.aliases
    }

    /// The declared migration edges, sorted by identity bytes.
    pub fn edges(&self) -> &[MigrationEdge<F>] {
        &self.edges
    }

    /// The exact-set support lookup; `None` when unregistered.
    pub fn record(&self, version: &ContractVersion<F>) -> Option<&VersionRecord<F>> {
        self.versions
            .iter()
            .find(|record| record.version == *version)
    }

    /// Resolve a declared alias; `None` proves no inferred alias exists.
    pub fn resolve_alias(&self, token: &str) -> Option<&ContractVersion<F>> {
        self.aliases
            .iter()
            .find(|(alias, _)| alias.as_str() == token)
            .map(|(_, version)| version)
    }

    /// The smallest registered version (a computed display value, never a
    /// support interval).
    pub fn min(&self) -> Option<&ContractVersion<F>> {
        self.versions.first().map(|record| &record.version)
    }

    /// The largest registered version (a computed display value).
    pub fn max(&self) -> Option<&ContractVersion<F>> {
        self.versions.last().map(|record| &record.version)
    }

    /// Construct one family registry from typed records. Records are
    /// sorted; validation happens through [`Self::validate`], which the
    /// embedded registry runs before use.
    pub fn new(
        current: Option<ContractVersion<F>>,
        mut versions: Vec<VersionRecord<F>>,
        mut aliases: Vec<(AliasToken, ContractVersion<F>)>,
        mut edges: Vec<MigrationEdge<F>>,
    ) -> Self {
        versions.sort_by(|left, right| left.version.cmp(&right.version));
        aliases.sort_by(|left, right| left.0.cmp(&right.0));
        edges.sort_by(|left, right| left.id.as_bytes().cmp(right.id.as_bytes()));
        Self {
            current,
            versions,
            aliases,
            edges,
        }
    }

    /// Validate this family's invariants, appending stable details for
    /// every violation. Used by [`VersionRegistry::embedded`]; public so
    /// typed constructors can validate before embedding.
    pub fn validate(&self, details: &mut Vec<String>) {
        let published = !self.versions.is_empty();
        match (&self.current, published) {
            (None, false) => {}
            (Some(current), true) => {
                if self.record(current).is_none() {
                    details.push(format!(
                        "family {} current {} is not registered",
                        F::NAME,
                        current
                    ));
                } else if let Some(record) = self.record(current) {
                    if matches!(record.state, VersionState::Retired { .. }) {
                        details.push(format!(
                            "family {} current {} must not be retired",
                            F::NAME,
                            current
                        ));
                    }
                }
            }
            (None, true) => details.push(format!("family {} has no current version", F::NAME)),
            (Some(_), false) => details.push(format!(
                "family {} declares a current version without entries",
                F::NAME
            )),
        }

        for pair in self.versions.windows(2) {
            if pair[0].version >= pair[1].version {
                details.push(format!(
                    "family {} versions are not strictly ascending: {} then {}",
                    F::NAME,
                    pair[0].version,
                    pair[1].version
                ));
            }
        }
        for record in &self.versions {
            match &record.state {
                VersionState::Supported => {}
                VersionState::Deprecated {
                    deprecated_since,
                    retirement_not_before,
                } => {
                    if deprecated_since <= &record.version {
                        details.push(format!(
                            "family {} version {} deprecated since {} must be a later version",
                            F::NAME,
                            record.version,
                            deprecated_since
                        ));
                    }
                    if self.record(deprecated_since).is_none() {
                        details.push(format!(
                            "family {} version {} deprecation threshold {} is not registered",
                            F::NAME,
                            record.version,
                            deprecated_since
                        ));
                    }
                    if retirement_not_before <= &record.version {
                        details.push(format!(
                            "family {} version {} retirement threshold {} must exceed the version",
                            F::NAME,
                            record.version,
                            retirement_not_before
                        ));
                    }
                }
                VersionState::Retired {
                    retired_since,
                    replacement,
                } => {
                    if retired_since <= &record.version {
                        details.push(format!(
                            "family {} version {} retired since {} must be a later version",
                            F::NAME,
                            record.version,
                            retired_since
                        ));
                    }
                    if let Some(replacement) = replacement {
                        if self.record(replacement).is_none() {
                            details.push(format!(
                                "family {} version {} replacement {} is not registered",
                                F::NAME,
                                record.version,
                                replacement
                            ));
                        }
                    }
                }
            }
            if record.reason.is_empty() || record.reason.len() > 512 {
                details.push(format!(
                    "family {} version {} needs a bounded written policy reason",
                    F::NAME,
                    record.version
                ));
            }
        }

        for pair in self.aliases.windows(2) {
            if pair[0].0 == pair[1].0 {
                details.push(format!(
                    "family {} duplicate alias {}",
                    F::NAME,
                    pair[1].0.as_str()
                ));
            }
        }
        for (alias, version) in &self.aliases {
            if self.record(version).is_none() {
                details.push(format!(
                    "family {} alias {} targets unregistered {}",
                    F::NAME,
                    alias.as_str(),
                    version
                ));
            }
        }

        for pair in self.edges.windows(2) {
            if pair[0].id == pair[1].id {
                details.push(format!(
                    "family {} duplicate edge id {}",
                    F::NAME,
                    pair[1].id
                ));
            }
        }
        for edge in &self.edges {
            if self.record(&edge.from).is_none() {
                details.push(format!(
                    "family {} edge {} source {} is not registered",
                    F::NAME,
                    edge.id,
                    edge.from
                ));
            }
            if self.record(&edge.to).is_none() {
                details.push(format!(
                    "family {} edge {} target {} is not registered",
                    F::NAME,
                    edge.id,
                    edge.to
                ));
            }
            if edge.from >= edge.to {
                details.push(format!(
                    "family {} edge {} must strictly ascend ({} to {})",
                    F::NAME,
                    edge.id,
                    edge.from,
                    edge.to
                ));
            }
            if edge.reason.is_empty() || edge.reason.len() > 512 {
                details.push(format!(
                    "family {} edge {} needs a bounded written policy reason",
                    F::NAME,
                    edge.id
                ));
            }
        }

        // At most one path between any two registered versions: more than
        // one would make migration target selection ambiguous, which is a
        // registry fault, never a runtime choice.
        for source in &self.versions {
            let paths = self.all_paths(&source.version);
            for target in &self.versions {
                if source.version == target.version {
                    continue;
                }
                let count = paths.get(&target.version).map_or(0, Vec::len);
                if count > 1 {
                    details.push(format!(
                        "family {} has {} migration paths from {} to {}",
                        F::NAME,
                        count,
                        source.version,
                        target.version
                    ));
                }
            }
        }
    }

    /// Every simple migration path leaving `from`, keyed by target. Paths
    /// are edge-id chains; the graph cannot cycle because edges ascend.
    /// The unique-path invariant is a registry property, not a runtime
    /// choice.
    pub fn all_paths(
        &self,
        from: &ContractVersion<F>,
    ) -> BTreeMap<ContractVersion<F>, Vec<Vec<String>>> {
        let mut paths: BTreeMap<ContractVersion<F>, Vec<Vec<String>>> = BTreeMap::new();
        let mut frontier: Vec<(ContractVersion<F>, Vec<String>)> = vec![(from.clone(), Vec::new())];
        while let Some((version, chain)) = frontier.pop() {
            for edge in &self.edges {
                if edge.from != version {
                    continue;
                }
                let mut chain = chain.clone();
                chain.push(edge.id.clone());
                paths
                    .entry(edge.to.clone())
                    .or_default()
                    .push(chain.clone());
                frontier.push((edge.to.clone(), chain));
            }
        }
        paths
    }
}

/// The validated, embedded registry over all contract families.
#[derive(Clone, Debug)]
pub struct VersionRegistry {
    registry_version: ContractVersion<RegistryContract>,
    model: FamilyRegistry<ModelContract>,
    ir: FamilyRegistry<IrContract>,
    protocol: FamilyRegistry<ProtocolContract>,
}

impl VersionRegistry {
    /// The validated embedded registry, parsed once per process.
    pub fn embedded() -> Result<&'static Self, RegistryError> {
        static EMBEDDED: LazyLock<Result<VersionRegistry, RegistryError>> = LazyLock::new(|| {
            let registry = VersionRegistry::from_bytes(REGISTRY_BYTES)?;
            super::graph::validate_registry_binding(&registry)?;
            Ok(registry)
        });
        EMBEDDED.as_ref().map_err(Clone::clone)
    }

    /// Build and validate from arbitrary candidate registry bytes. Public
    /// so embedders can validate a candidate artifact before embedding it
    /// and so the invalid-registry fixtures can exercise every invariant.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RegistryError> {
        let raw: RawRegistry = serde_json::from_slice(bytes)
            .map_err(|error| RegistryError::single(format!("registry JSON invalid: {error}")))?;
        let mut details = Vec::new();
        let registry_version =
            ContractVersion::parse_canonical(&raw.registry_version).map_err(|error| {
                RegistryError::single(format!("registryVersion: {}", error.as_str()))
            })?;
        if raw.registry != REGISTRY_IDENTITY {
            details.push(format!(
                "registry identity must be {REGISTRY_IDENTITY}, found {}",
                raw.registry
            ));
        }
        let model = raw_family::<ModelContract>(&raw.families.model, &mut details);
        let ir = raw_family::<IrContract>(&raw.families.ir, &mut details);
        let protocol = raw_family::<ProtocolContract>(&raw.families.protocol, &mut details);
        let registry = Self {
            registry_version,
            model,
            ir,
            protocol,
        };
        registry.model.validate(&mut details);
        registry.ir.validate(&mut details);
        registry.protocol.validate(&mut details);
        if details.is_empty() {
            Ok(registry)
        } else {
            details.sort();
            Err(RegistryError { details })
        }
    }

    /// The registry artifact version.
    pub fn registry_version(&self) -> &ContractVersion<RegistryContract> {
        &self.registry_version
    }

    /// The Model family registry.
    pub fn model(&self) -> &FamilyRegistry<ModelContract> {
        &self.model
    }

    /// The IR family registry.
    pub fn ir(&self) -> &FamilyRegistry<IrContract> {
        &self.ir
    }

    /// The protocol family registry (unpublished).
    pub fn protocol(&self) -> &FamilyRegistry<ProtocolContract> {
        &self.protocol
    }
}

fn raw_family<F: ContractFamily>(raw: &RawFamily, details: &mut Vec<String>) -> FamilyRegistry<F> {
    let mut versions: Vec<VersionRecord<F>> = Vec::new();
    for record in &raw.versions {
        let version = match ContractVersion::parse_canonical(&record.version) {
            Ok(version) => version,
            Err(error) => {
                details.push(format!(
                    "family {} version {} is not canonical: {}",
                    F::NAME,
                    record.version,
                    error.as_str()
                ));
                continue;
            }
        };
        let classification = match ChangeClassification::parse(&record.classification) {
            Some(classification) => classification,
            None => {
                details.push(format!(
                    "family {} version {} has unknown classification {}",
                    F::NAME,
                    record.version,
                    record.classification
                ));
                continue;
            }
        };
        let state = match record.state.as_str() {
            "supported" => VersionState::Supported,
            "deprecated" => {
                let deprecated_since = parse_or_detail(
                    Some(&record.deprecated_since),
                    F::NAME,
                    &record.version,
                    "deprecatedSince",
                    details,
                );
                let retirement_not_before = parse_or_detail(
                    Some(&record.retirement_not_before),
                    F::NAME,
                    &record.version,
                    "retirementNotBefore",
                    details,
                );
                match (deprecated_since, retirement_not_before) {
                    (Some(deprecated_since), Some(retirement_not_before)) => {
                        VersionState::Deprecated {
                            deprecated_since,
                            retirement_not_before,
                        }
                    }
                    _ => continue,
                }
            }
            "retired" => {
                let retired_since = parse_or_detail(
                    record.retired_since.as_deref(),
                    F::NAME,
                    &record.version,
                    "retiredSince",
                    details,
                );
                let replacement = match &record.replacement {
                    Some(text) => match parse_or_detail(
                        Some(text),
                        F::NAME,
                        &record.version,
                        "replacement",
                        details,
                    ) {
                        Some(replacement) => Some(replacement),
                        None => continue,
                    },
                    None => None,
                };
                match retired_since {
                    Some(retired_since) => VersionState::Retired {
                        retired_since,
                        replacement,
                    },
                    None => continue,
                }
            }
            other => {
                details.push(format!(
                    "family {} version {} has unknown state {}",
                    F::NAME,
                    record.version,
                    other
                ));
                continue;
            }
        };
        versions.push(VersionRecord {
            version,
            state,
            classification,
            reason: record.reason.clone(),
        });
    }
    let current = match &raw.current {
        Some(text) => match ContractVersion::parse_canonical(text) {
            Ok(version) => Some(version),
            Err(error) => {
                details.push(format!(
                    "family {} current {} is not canonical: {}",
                    F::NAME,
                    text,
                    error.as_str()
                ));
                None
            }
        },
        None => None,
    };
    let mut aliases: Vec<(AliasToken, ContractVersion<F>)> = Vec::new();
    for alias in &raw.aliases {
        let token = match AliasToken::parse(&alias.alias) {
            Some(token) => token,
            None => {
                details.push(format!(
                    "family {} alias {} is not a v-major token",
                    F::NAME,
                    alias.alias
                ));
                continue;
            }
        };
        match ContractVersion::parse_canonical(&alias.version) {
            Ok(version) => aliases.push((token, version)),
            Err(error) => details.push(format!(
                "family {} alias {} target {} is not canonical: {}",
                F::NAME,
                alias.alias,
                alias.version,
                error.as_str()
            )),
        }
    }
    let mut edges: Vec<MigrationEdge<F>> = Vec::new();
    for edge in &raw.migrations {
        let from = match ContractVersion::parse_canonical(&edge.from) {
            Ok(version) => version,
            Err(error) => {
                details.push(format!(
                    "family {} edge {} source {} is not canonical: {}",
                    F::NAME,
                    edge.id,
                    edge.from,
                    error.as_str()
                ));
                continue;
            }
        };
        let to = match ContractVersion::parse_canonical(&edge.to) {
            Ok(version) => version,
            Err(error) => {
                details.push(format!(
                    "family {} edge {} target {} is not canonical: {}",
                    F::NAME,
                    edge.id,
                    edge.to,
                    error.as_str()
                ));
                continue;
            }
        };
        let (classification, regeneration_impact) = match (
            ChangeClassification::parse(&edge.classification),
            RegenerationImpact::parse(&edge.regeneration_impact),
        ) {
            (Some(classification), Some(impact)) => (classification, impact),
            _ => {
                details.push(format!(
                    "family {} edge {} has an unknown classification or regeneration impact",
                    F::NAME,
                    edge.id
                ));
                continue;
            }
        };
        edges.push(MigrationEdge {
            id: edge.id.clone(),
            from,
            to,
            classification,
            loss: edge.loss.clone(),
            regeneration_impact,
            reason: edge.reason.clone(),
        });
    }
    FamilyRegistry::new(current, versions, aliases, edges)
}

fn parse_or_detail<F: ContractFamily>(
    text: Option<&str>,
    family: &str,
    version: &str,
    field: &str,
    details: &mut Vec<String>,
) -> Option<ContractVersion<F>> {
    let text = text?;
    match ContractVersion::parse_canonical(text) {
        Ok(parsed) => Some(parsed),
        Err(VersionParseError::Empty) => {
            details.push(format!(
                "family {family} version {version} is missing required field {field}"
            ));
            None
        }
        Err(error) => {
            details.push(format!(
                "family {family} version {version} field {field} value {text} is not canonical: {}",
                error.as_str()
            ));
            None
        }
    }
}

// Raw serde shapes. Field names are the closed artifact wire names; unknown
// fields are rejected, and the canonical bytes are golden-tested.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRegistry {
    registry: String,
    #[serde(rename = "registryVersion")]
    registry_version: String,
    families: RawFamilies,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFamilies {
    model: RawFamily,
    ir: RawFamily,
    protocol: RawFamily,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFamily {
    current: Option<String>,
    #[serde(default)]
    aliases: Vec<RawAlias>,
    #[serde(default)]
    versions: Vec<RawVersionRecord>,
    #[serde(default)]
    migrations: Vec<RawEdge>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAlias {
    alias: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVersionRecord {
    version: String,
    state: String,
    #[serde(rename = "deprecatedSince", default)]
    deprecated_since: String,
    #[serde(rename = "retirementNotBefore", default)]
    retirement_not_before: String,
    #[serde(rename = "retiredSince", default)]
    retired_since: Option<String>,
    #[serde(default)]
    replacement: Option<String>,
    classification: String,
    reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEdge {
    id: String,
    from: String,
    to: String,
    classification: String,
    #[serde(default)]
    loss: Vec<String>,
    #[serde(rename = "regenerationImpact")]
    regeneration_impact: String,
    reason: String,
}
