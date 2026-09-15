//! The adapter compatibility preflight (issue #9).
//!
//! Adapters declare compatibility ranges over the IR and protocol families
//! through a typed, closed manifest. The preflight decides in one fixed
//! order — manifest schema/version, current IR registry support, current
//! protocol registry support, IR inclusive range, protocol inclusive
//! range, required extensions — and never lets generation start on a
//! non-compatible verdict. Ranges are exact canonical versions with
//! inclusive bounds; wildcards, `VersionReq`-style requirements, and build
//! metadata do not exist here.
//!
//! Until issue #27 the protocol family was unpublished, so no external
//! adapter could be compatible. The family now publishes `1.0.0`
//! (`lekalo.target/v1`); an unpublished or unregistered protocol still
//! refuses through `versioning.protocol-unpublished` before any runner
//! could be created.

use serde::Serialize;

use super::family::{ContractFamily, IrContract, ProtocolContract, RegistryContract};
use super::reasons;
use super::registry::VersionRegistry;
use super::support::{ir_support, protocol_support};
use super::version::ContractVersion;

/// The current adapter-compatibility manifest schema version.
pub const MANIFEST_SCHEMA_VERSION: &str = "1.0.0";

/// The inclusive protocol version range of one adapter manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolBounds {
    /// Inclusive lower bound.
    pub min: ContractVersion<ProtocolContract>,
    /// Inclusive upper bound.
    pub max: ContractVersion<ProtocolContract>,
}

/// The typed adapter compatibility manifest.
///
/// Every version is a canonical [`ContractVersion`] of the right family;
/// nothing is a string parsed ad hoc. The manifest deliberately carries no
/// adapter package version or digest: issue #10 owns those in the lock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterCompatibilityManifest {
    /// The manifest schema version; must equal
    /// [`MANIFEST_SCHEMA_VERSION`].
    pub manifest_version: ContractVersion<RegistryContract>,
    /// The stable adapter identity.
    pub adapter_id: String,
    /// Inclusive lower IR bound.
    pub ir_min: ContractVersion<IrContract>,
    /// Inclusive upper IR bound.
    pub ir_max: ContractVersion<IrContract>,
    /// Inclusive lower protocol bound (required for a runnable adapter).
    pub protocol_min: Option<ContractVersion<ProtocolContract>>,
    /// Inclusive upper protocol bound (required for a runnable adapter).
    pub protocol_max: Option<ContractVersion<ProtocolContract>>,
    /// IR extensions the adapter requires; sorted, deduplicated.
    pub required_extensions: Vec<String>,
    /// IR extensions the adapter tolerates; sorted, deduplicated.
    pub optional_extensions: Vec<String>,
}

/// Why a manifest was rejected before preflight could run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestInvalidity {
    /// The manifest schema version is not the current one.
    SchemaVersion,
    /// The IR bounds are inverted.
    IrRange,
    /// The protocol bounds are inverted.
    ProtocolRange,
    /// The adapter identity is empty or oversized.
    AdapterId,
}

impl ManifestInvalidity {
    /// The stable detail token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SchemaVersion => "manifest-version",
            Self::IrRange => "ir-range",
            Self::ProtocolRange => "protocol-range",
            Self::AdapterId => "adapter-id",
        }
    }
}

impl AdapterCompatibilityManifest {
    /// Construct and validate a manifest from typed parts. The protocol
    /// bounds travel together: an adapter declares a range, not two
    /// unrelated endpoints.
    pub fn new(
        manifest_version: ContractVersion<RegistryContract>,
        adapter_id: impl Into<String>,
        ir_min: ContractVersion<IrContract>,
        ir_max: ContractVersion<IrContract>,
        protocol: Option<ProtocolBounds>,
        required_extensions: Vec<String>,
        optional_extensions: Vec<String>,
    ) -> Result<Self, ManifestInvalidity> {
        let (protocol_min, protocol_max) = match protocol {
            Some(bounds) => (Some(bounds.min), Some(bounds.max)),
            None => (None, None),
        };
        let manifest = Self {
            manifest_version,
            adapter_id: adapter_id.into(),
            ir_min,
            ir_max,
            protocol_min,
            protocol_max,
            required_extensions,
            optional_extensions,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// The closed manifest invariants.
    pub fn validate(&self) -> Result<(), ManifestInvalidity> {
        if self.manifest_version.as_str() != MANIFEST_SCHEMA_VERSION {
            return Err(ManifestInvalidity::SchemaVersion);
        }
        let id_ok = !self.adapter_id.is_empty()
            && self.adapter_id.len() <= 128
            && self
                .adapter_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        if !id_ok {
            return Err(ManifestInvalidity::AdapterId);
        }
        if self.ir_min > self.ir_max {
            return Err(ManifestInvalidity::IrRange);
        }
        if let (Some(min), Some(max)) = (&self.protocol_min, &self.protocol_max) {
            if min > max {
                return Err(ManifestInvalidity::ProtocolRange);
            }
        }
        Ok(())
    }
}

/// The preflight verdict: compatible, or incompatible with sorted stable
/// reasons.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompatibilityVerdict {
    /// Generation may proceed (unreachable while the protocol family
    /// stays unpublished in the consulted registry).
    Compatible,
    /// Generation must not start; the reasons are sorted stable codes.
    Incompatible {
        /// Sorted stable reason codes.
        reasons: Vec<&'static str>,
    },
}

impl CompatibilityVerdict {
    /// Whether generation may proceed.
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible)
    }

    /// The sorted reasons of an incompatible verdict (empty when
    /// compatible).
    pub fn reasons(&self) -> &[&'static str] {
        match self {
            Self::Compatible => &[],
            Self::Incompatible { reasons } => reasons,
        }
    }
}

/// The compatibility decision entry point.
pub struct CompatibilityPreflight;

impl CompatibilityPreflight {
    /// Decide whether a generation run may start for one adapter manifest
    /// against the current IR and protocol versions.
    ///
    /// `ir` is the exact IR version the compiled project carries;
    /// `protocol` is the resolved protocol version, which stays `None`
    /// until the protocol family publishes its first version.
    pub fn check(
        registry: &VersionRegistry,
        ir: &ContractVersion<IrContract>,
        protocol: Option<&ContractVersion<ProtocolContract>>,
        manifest: &AdapterCompatibilityManifest,
    ) -> CompatibilityVerdict {
        let mut reasons: Vec<&'static str> = Vec::new();

        // 1. Manifest schema/version and structural invariants.
        if manifest.validate().is_err() {
            reasons.push(reasons::ADAPTER_MANIFEST_INVALID);
            return finish(reasons);
        }

        // 2. Current IR registry support.
        let ir_verdict = ir_support(registry, ir);
        if !ir_verdict.is_usable() {
            reasons.push(reasons::UNSUPPORTED_VERSION);
        }

        // 3. Current protocol registry support. An unpublished family
        //    carries no version, so no external adapter is runnable.
        let Some(protocol_version) = protocol else {
            reasons.push(reasons::PROTOCOL_UNPUBLISHED);
            return finish(reasons);
        };
        let protocol_verdict = protocol_support(registry, protocol_version);
        if protocol_verdict.is_unregistered() || protocol_verdict.state == "retired" {
            reasons.push(reasons::UNSUPPORTED_VERSION);
        }

        // 4. Inclusive IR range.
        if ir < &manifest.ir_min || ir > &manifest.ir_max {
            reasons.push(reasons::ADAPTER_INCOMPATIBLE);
        }

        // 5. Inclusive protocol range.
        if let (Some(min), Some(max)) = (&manifest.protocol_min, &manifest.protocol_max) {
            if protocol_version < min || protocol_version > max {
                reasons.push(reasons::ADAPTER_INCOMPATIBLE);
            }
        } else {
            // A runnable adapter must declare both protocol bounds.
            reasons.push(reasons::ADAPTER_MANIFEST_INVALID);
        }

        // 6. Required extensions. The accepted #8 IR has no extension
        //    surface; requiring one can never be satisfied.
        if !manifest.required_extensions.is_empty() {
            reasons.push(reasons::EXTENSION_INCOMPATIBLE);
        }

        finish(reasons)
    }
}

fn finish(mut reasons: Vec<&'static str>) -> CompatibilityVerdict {
    if reasons.is_empty() {
        CompatibilityVerdict::Compatible
    } else {
        reasons.sort_unstable();
        reasons.dedup();
        CompatibilityVerdict::Incompatible { reasons }
    }
}

/// One family's projection in the `lekalo compatibility` report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FamilySummary {
    /// The stable family wire name.
    pub family: &'static str,
    /// The current version, when the family is published.
    pub current: Option<String>,
    /// The smallest registered version, when any.
    pub min: Option<String>,
    /// The largest registered version, when any.
    pub max: Option<String>,
    /// The declared aliases, in registry order.
    pub aliases: Vec<AliasSummary>,
    /// The registered versions with their lifecycle, in registry order.
    pub versions: Vec<VersionSummary>,
    /// The declared migration edges, in registry order.
    pub migrations: Vec<EdgeSummary>,
}

/// One alias projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AliasSummary {
    /// The alias token (`v1`).
    pub alias: String,
    /// The bound version.
    pub version: String,
}

/// One version projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VersionSummary {
    /// The canonical version.
    pub version: String,
    /// The lifecycle state.
    pub state: String,
    /// The change classification of the introducing change.
    pub classification: String,
}

/// One migration edge projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EdgeSummary {
    /// The stable edge identity.
    pub id: String,
    /// The source version.
    pub from: String,
    /// The target version.
    pub to: String,
    /// The change classification.
    pub classification: String,
}

/// The full `lekalo compatibility` projection: closed, ordered, and
/// byte-identical between the CLI JSON renderer and the golden fixture.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CompatibilityReport {
    /// Always `valid`.
    pub status: &'static str,
    /// The registry artifact version.
    #[serde(rename = "registryVersion")]
    pub registry_version: String,
    /// The family summaries in fixed order: model, ir, protocol.
    pub families: Vec<FamilySummary>,
}

impl CompatibilityReport {
    /// Project the embedded registry.
    pub fn from_registry(registry: &VersionRegistry) -> Self {
        fn family_of<F: ContractFamily>(
            name: &'static str,
            family: &super::registry::FamilyRegistry<F>,
        ) -> FamilySummary {
            FamilySummary {
                family: name,
                current: family.current().map(ContractVersion::to_string),
                min: family.min().map(ContractVersion::to_string),
                max: family.max().map(ContractVersion::to_string),
                aliases: family
                    .aliases()
                    .iter()
                    .map(|(alias, version)| AliasSummary {
                        alias: alias.as_str().to_owned(),
                        version: version.to_string(),
                    })
                    .collect(),
                versions: family
                    .versions()
                    .iter()
                    .map(|record| VersionSummary {
                        version: record.version.to_string(),
                        state: record.state.as_str().to_owned(),
                        classification: record.classification.as_str().to_owned(),
                    })
                    .collect(),
                migrations: family
                    .edges()
                    .iter()
                    .map(|edge| EdgeSummary {
                        id: edge.id.clone(),
                        from: edge.from.to_string(),
                        to: edge.to.to_string(),
                        classification: edge.classification.as_str().to_owned(),
                    })
                    .collect(),
            }
        }
        Self {
            status: "valid",
            registry_version: registry.registry_version().to_string(),
            families: vec![
                family_of("model", registry.model()),
                family_of("ir", registry.ir()),
                family_of("protocol", registry.protocol()),
            ],
        }
    }
}
