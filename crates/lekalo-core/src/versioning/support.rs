//! Shared support-policy gate and Model target resolution (issue #9).
//!
//! One support-policy check serves every command: `load`, `ir`, `migrate`,
//! and the adapter preflight all consult the same embedded registry through
//! this module, so a retired version can never pass one surface while
//! failing another. Unsupported contract versions fail closed with the
//! stable `versioning.unsupported-version` reason and exit 5 before any
//! canonicalization or write.

use super::family::{ContractFamily, ModelContract};
use super::registry::VersionRegistry;
use super::version::{ContractVersion, VersionParseError};
use crate::loader::ModelVersion;

/// The verdict of one exact-set support lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportVerdict {
    /// The canonical version string the verdict describes.
    pub version: String,
    /// The lifecycle state's wire spelling (`supported`, `deprecated`,
    /// `retired`), or `unregistered` when the family does not carry it.
    pub state: String,
    /// The registered replacement for a retired version, when declared.
    pub replacement: Option<String>,
}

impl SupportVerdict {
    /// Whether every operation accepts this version.
    pub fn is_usable(&self) -> bool {
        self.state == "supported" || self.state == "deprecated"
    }

    /// Whether this version is absent from the registry's exact set.
    pub fn is_unregistered(&self) -> bool {
        self.state == "unregistered"
    }
}

fn verdict_for<F: ContractFamily>(
    registry: &VersionRegistry,
    lookup: impl Fn(&VersionRegistry) -> Option<&super::registry::FamilyRegistry<F>>,
    version: &ContractVersion<F>,
) -> SupportVerdict {
    let family = lookup(registry);
    let state = family.and_then(|family| family.record(version));
    let replacement = state.and_then(|record| match &record.state {
        super::registry::VersionState::Retired { replacement, .. } => {
            replacement.as_ref().map(ContractVersion::to_string)
        }
        _ => None,
    });
    SupportVerdict {
        version: version.to_string(),
        state: state
            .map_or("unregistered", |record| record.state.as_str())
            .to_owned(),
        replacement,
    }
}

/// The support verdict for one loaded Model version.
pub fn model_support(registry: &VersionRegistry, version: ModelVersion) -> SupportVerdict {
    let converted = ContractVersion::<ModelContract>::from(version);
    verdict_for(registry, |registry| Some(registry.model()), &converted)
}

/// The support verdict for one exact IR contract version.
pub fn ir_support(
    registry: &VersionRegistry,
    version: &ContractVersion<super::family::IrContract>,
) -> SupportVerdict {
    verdict_for(registry, |registry| Some(registry.ir()), version)
}

/// The support verdict for one exact protocol contract version.
pub fn protocol_support(
    registry: &VersionRegistry,
    version: &ContractVersion<super::family::ProtocolContract>,
) -> SupportVerdict {
    verdict_for(registry, |registry| Some(registry.protocol()), version)
}

/// The shared gate: fail when `verdict` is not usable.
///
/// `load` and `ir` call this once per resolved Model version; `migrate`
/// calls it for the source version. The caller owns the stable diagnostic.
pub fn ensure_usable(verdict: &SupportVerdict) -> Result<(), UnsupportedVersion> {
    if verdict.is_usable() {
        Ok(())
    } else {
        Err(UnsupportedVersion {
            version: verdict.version.clone(),
            state: verdict.state.clone(),
            replacement: verdict.replacement.clone(),
        })
    }
}

/// The typed rejection behind `versioning.unsupported-version`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsupportedVersion {
    /// The canonical or raw version string that was refused.
    pub version: String,
    /// `unregistered` or `retired`.
    pub state: String,
    /// The declared replacement for a retired version.
    pub replacement: Option<String>,
}

/// One parsed `lekalo migrate --to` selector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelTarget {
    /// `model/<canonical-semver>`: an exact canonical Model version.
    Canonical(ContractVersion<ModelContract>),
    /// `model/<alias>`: a registry-declared alias such as `model/v1`.
    Alias(String),
}

impl ModelTarget {
    /// Parse the selector text after the `model/` prefix.
    ///
    /// Canonical spellings must satisfy [`ContractVersion::parse_canonical`];
    /// aliases must satisfy [`super::registry::AliasToken::parse`]. Both
    /// failures collapse to `versioning.invalid-version` (exit 1); a
    /// well-formed but unregistered target is `unsupported` (exit 5).
    pub fn parse(selector: &str) -> Result<Self, TargetError> {
        if selector.starts_with('v') {
            return super::registry::AliasToken::parse(selector)
                .map(|token| Self::Alias(token.as_str().to_owned()))
                .ok_or(TargetError::Malformed);
        }
        let version = ContractVersion::parse_canonical(selector).map_err(|error| {
            TargetError::MalformedDetail(match error {
                VersionParseError::BuildMetadata => TargetMalformation::BuildMetadata,
                VersionParseError::NotAscii => TargetMalformation::NotAscii,
                _ => TargetMalformation::Malformed,
            })
        })?;
        Ok(Self::Canonical(version))
    }

    /// Resolve against the registry to one exact registered version.
    ///
    /// Canonical targets must be registered; aliases must be declared.
    /// Nothing is inferred: `model/1`, `model/v1.0.0`, and repeated `v`
    /// prefixes are malformed, and an absent `model/v2` is unsupported.
    pub fn resolve(
        self,
        registry: &VersionRegistry,
    ) -> Result<ContractVersion<ModelContract>, TargetError> {
        match self {
            Self::Canonical(version) => {
                if registry.model().record(&version).is_some() {
                    Ok(version)
                } else {
                    Err(TargetError::Unsupported(version.to_string()))
                }
            }
            Self::Alias(token) => registry
                .model()
                .resolve_alias(&token)
                .cloned()
                .ok_or_else(|| TargetError::Unsupported(format!("model/{token}"))),
        }
    }
}

/// The closed malformation details behind `versioning.invalid-version`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetMalformation {
    /// Non-canonical spelling, partial components, or a bad alias token.
    Malformed,
    /// Build metadata is not a Lekalo contract version.
    BuildMetadata,
    /// Non-ASCII selector text.
    NotAscii,
}

/// Why a Model target selector was refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetError {
    /// The selector spelling is invalid (exit 1).
    Malformed,
    MalformedDetail(TargetMalformation),
    /// The selector is well-formed but not registered (exit 5).
    Unsupported(String),
}

impl TargetError {
    /// The stable reason code for this refusal.
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::Malformed | Self::MalformedDetail(_) => "versioning.invalid-version",
            Self::Unsupported(_) => "versioning.unsupported-version",
        }
    }
}
