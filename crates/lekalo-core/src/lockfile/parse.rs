//! Strict canonical parsing of `lekalo.lock` (issue #10).
//!
//! Classification order is fixed and documented in `docs/lockfile.md`:
//! a size bound and encoding check, the JSON parse, the schema-discriminator
//! dispatch (`lock.unsupported-schema-version` for a future `lekalo/lock/vX`
//! spelling), the closed v1 shape, full typed and cross-field validation,
//! and only then the byte-equality check that classifies a semantically
//! valid but noncanonical document as `lock.noncanonical` instead of ever
//! rewriting it. A value exists only after every invariant passed, so
//! `canonical_bytes` is total and byte-stable.

use serde::Deserialize;

use super::types::{
    ArtifactPin, CapabilityId, CatalogRef, ComponentId, ComponentRef, ContractPin, Platform,
    ProviderKind, ResolvedAdapter, ResolvedCapability, ResolvedGenerator, ResolvedProfile, SemVer,
    Sha256Digest, SourceKind, SourceRef, Support, SCHEMA_VERSION,
};
use super::{canonical, LockDigest, LockFailure, Lockfile};

/// Maximum accepted lock file size. The bound keeps hostile inputs bounded;
/// it is documented in `docs/lockfile.md`.
pub const MAX_LOCKFILE_BYTES: usize = 1 << 20;

impl Lockfile {
    /// Parse and fully validate exact lock bytes (file or payload form).
    ///
    /// The bytes must equal the canonical payload (with or without exactly
    /// one trailing LF); any other semantically equal spelling is refused
    /// with `lock.noncanonical` and never rewritten.
    pub fn parse_canonical(bytes: &[u8]) -> Result<Self, LockFailure> {
        let lock = Self::parse_validated(bytes)?;
        let canonical = canonical::payload_bytes(&lock);
        let input = strip_final_lf(bytes);
        if input != canonical.as_slice() {
            return Err(LockFailure::Noncanonical);
        }
        Ok(lock)
    }

    /// The exact canonical file bytes: payload plus exactly one LF.
    pub fn canonical_bytes(&self) -> Box<[u8]> {
        canonical::file_bytes(self).into_boxed_slice()
    }

    /// SHA-256 of the canonical payload bytes (without the final LF).
    pub fn digest(&self) -> LockDigest {
        canonical::lock_digest(self)
    }

    /// Parse with every typed and cross-field invariant, without the final
    /// byte-equality gate (internal reuse for the canonicality check).
    pub(crate) fn parse_validated(bytes: &[u8]) -> Result<Self, LockFailure> {
        if bytes.len() > MAX_LOCKFILE_BYTES {
            return Err(LockFailure::SchemaInvalid);
        }
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| LockFailure::SchemaInvalid)?;
        let raw = match value.get("schema_version") {
            Some(serde_json::Value::String(text)) => text.clone(),
            _ => return Err(LockFailure::SchemaInvalid),
        };
        if raw != SCHEMA_VERSION {
            let future = raw.starts_with("lekalo/lock/v");
            return Err(if future {
                LockFailure::UnsupportedSchemaVersion { found: raw }
            } else {
                LockFailure::SchemaInvalid
            });
        }
        let parsed: RawLock =
            serde_json::from_value(value).map_err(|_| LockFailure::SchemaInvalid)?;
        build(parsed)
    }
}

fn strip_final_lf(bytes: &[u8]) -> &[u8] {
    match bytes.strip_suffix(b"\n") {
        Some(stripped) => stripped,
        None => bytes,
    }
}

fn digest_field(text: &str) -> Result<Sha256Digest, LockFailure> {
    Sha256Digest::parse(text)
}

fn version_field(text: &str) -> Result<SemVer, LockFailure> {
    SemVer::parse(text)
}

fn component_ref(text_id: &str, text_version: &str) -> Result<ComponentRef, LockFailure> {
    Ok(ComponentRef::new(
        ComponentId::parse(text_id)?,
        version_field(text_version)?,
    ))
}

fn contract_pin(raw: &RawPin) -> Result<ContractPin, LockFailure> {
    Ok(ContractPin::new(
        version_field(&raw.version)?,
        digest_field(&raw.digest)?,
    ))
}

fn build(raw: RawLock) -> Result<Lockfile, LockFailure> {
    if raw.schema_version != SCHEMA_VERSION {
        return Err(LockFailure::SchemaInvalid);
    }
    // Wire sort order is part of the closed v1 format: the canonical
    // serializer emits these arrays in exactly this order.
    require_sorted(&raw.resolver.catalogs, |catalog| &catalog.id)?;
    require_sorted(&raw.adapters, |adapter| &adapter.id)?;
    require_sorted(&raw.generators, |generator| &generator.id)?;
    require_sorted(&raw.profiles, |profile| &profile.id)?;
    for adapter in &raw.adapters {
        for pair in adapter.artifacts.windows(2) {
            if pair[0].platform.as_bytes() >= pair[1].platform.as_bytes() {
                return Err(LockFailure::ReferenceInvalid);
            }
        }
    }
    for pair in raw.capabilities.windows(2) {
        let ascending = pair[0].target.as_bytes() < pair[1].target.as_bytes()
            || (pair[0].target == pair[1].target
                && (pair[0].profile.as_bytes() < pair[1].profile.as_bytes()
                    || (pair[0].profile == pair[1].profile
                        && (pair[0].id.as_bytes() < pair[1].id.as_bytes()
                            || (pair[0].id == pair[1].id
                                && pair[0].version.as_bytes() < pair[1].version.as_bytes())))));
        if !ascending {
            return Err(LockFailure::ReferenceInvalid);
        }
    }
    let resolver_version = version_field(&raw.resolver.version)?;
    let request_digest = digest_field(&raw.resolver.request_digest)?;
    let mut catalogs = Vec::with_capacity(raw.resolver.catalogs.len());
    for catalog in &raw.resolver.catalogs {
        catalogs.push(CatalogRef::new(
            ComponentId::parse(&catalog.id)?,
            digest_field(&catalog.digest)?,
        ));
    }
    catalogs.sort();
    let core_version = version_field(&raw.core.version)?;

    let registry = contract_pin(&raw.contracts.registry)?;
    let model = contract_pin(&raw.contracts.model)?;
    let ir = contract_pin(&raw.contracts.ir)?;
    let target_protocol = match &raw.contracts.target_protocol {
        Some(pin) => Some(contract_pin(pin)?),
        None => None,
    };

    let mut adapters = Vec::with_capacity(raw.adapters.len());
    for adapter in &raw.adapters {
        adapters.push(ResolvedAdapter::from_parts(
            ComponentId::parse(&adapter.id)?,
            version_field(&adapter.version)?,
            digest_field(&adapter.digest)?,
            SourceRef::new(
                SourceKind::parse(&adapter.source.kind)?,
                &adapter.source.id,
                digest_field(&adapter.source.digest)?,
            )?,
            digest_field(&adapter.compatibility_digest)?,
            adapter
                .artifacts
                .iter()
                .map(|artifact| {
                    Ok(ArtifactPin::new(
                        Platform::parse(&artifact.platform)?,
                        digest_field(&artifact.digest)?,
                    ))
                })
                .collect::<Result<Vec<_>, LockFailure>>()?,
        )?);
    }

    let mut generators = Vec::with_capacity(raw.generators.len());
    for generator in &raw.generators {
        generators.push(ResolvedGenerator::from_parts(
            ComponentId::parse(&generator.id)?,
            version_field(&generator.version)?,
            digest_field(&generator.digest)?,
            component_ref(&generator.adapter.id, &generator.adapter.version)?,
        ));
    }

    let mut profiles = Vec::with_capacity(raw.profiles.len());
    for profile in &raw.profiles {
        profiles.push(ResolvedProfile::from_parts(
            ComponentId::parse(&profile.id)?,
            version_field(&profile.version)?,
            digest_field(&profile.source_digest)?,
            digest_field(&profile.digest)?,
            profile
                .adapters
                .iter()
                .map(|reference| component_ref(&reference.id, &reference.version))
                .collect::<Result<Vec<_>, LockFailure>>()?,
            profile
                .generators
                .iter()
                .map(|reference| component_ref(&reference.id, &reference.version))
                .collect::<Result<Vec<_>, LockFailure>>()?,
        )?);
    }

    let mut capabilities = Vec::with_capacity(raw.capabilities.len());
    for capability in &raw.capabilities {
        capabilities.push(ResolvedCapability::from_parts(
            Platform::parse(&capability.target)?,
            ComponentId::parse(&capability.profile)?,
            CapabilityId::parse(&capability.id)?,
            version_field(&capability.version)?,
            Support::parse(&capability.support)?,
            super::types::ProviderRef::new(
                ProviderKind::parse(&capability.provider.kind)?,
                ComponentId::parse(&capability.provider.id)?,
                version_field(&capability.provider.version)?,
            ),
        ));
    }

    let lock = Lockfile::from_parts(
        resolver_version,
        request_digest,
        catalogs,
        core_version,
        registry,
        model,
        ir,
        target_protocol,
        adapters,
        generators,
        profiles,
        capabilities,
    );
    validate_references(&lock)?;
    Ok(lock)
}

/// The closed cross-field invariants over an otherwise well-typed value.
fn validate_references(lock: &Lockfile) -> Result<(), LockFailure> {
    sorted_unique(lock.catalogs().iter().map(|catalog| catalog.id().as_str()))?;

    let adapter_ids: Vec<&str> = lock.adapters().iter().map(|a| a.id().as_str()).collect();
    sorted_unique(adapter_ids.iter().copied())?;
    let generator_ids: Vec<&str> = lock
        .generators()
        .iter()
        .map(|generator| generator.id().as_str())
        .collect();
    sorted_unique(generator_ids.iter().copied())?;
    let profile_ids: Vec<&str> = lock.profiles().iter().map(|p| p.id().as_str()).collect();
    sorted_unique(profile_ids.iter().copied())?;

    if lock.target_protocol().is_none() {
        // Unpublished protocol: no executable component may be locked.
        if !lock.adapters().is_empty()
            || !lock.generators().is_empty()
            || !lock.capabilities().is_empty()
        {
            return Err(LockFailure::ProtocolUnpublished);
        }
    }

    let adapter_matches = |reference: &ComponentRef| {
        lock.adapters().iter().any(|adapter| {
            adapter.id().as_str() == reference.id().as_str()
                && adapter.version() == reference.version()
        })
    };
    let generator_matches = |reference: &ComponentRef| {
        lock.generators().iter().any(|generator| {
            generator.id().as_str() == reference.id().as_str()
                && generator.version() == reference.version()
        })
    };
    for generator in lock.generators() {
        if !adapter_matches(generator.adapter()) {
            return Err(LockFailure::ReferenceInvalid);
        }
    }
    for profile in lock.profiles() {
        for reference in profile.adapters() {
            if !adapter_matches(reference) {
                return Err(LockFailure::ReferenceInvalid);
            }
        }
        for reference in profile.generators() {
            if !generator_matches(reference) {
                return Err(LockFailure::ReferenceInvalid);
            }
        }
    }
    for capability in lock.capabilities() {
        if lock
            .profiles()
            .iter()
            .all(|profile| profile.id().as_str() != capability.profile().as_str())
        {
            return Err(LockFailure::ReferenceInvalid);
        }
        let provided = match capability.provider().kind() {
            ProviderKind::Adapter => adapter_matches(&ComponentRef::new(
                ComponentId::parse(capability.provider().id().as_str())?,
                capability.provider().version().clone(),
            )),
            ProviderKind::Generator => generator_matches(&ComponentRef::new(
                ComponentId::parse(capability.provider().id().as_str())?,
                capability.provider().version().clone(),
            )),
        };
        if !provided {
            return Err(LockFailure::ReferenceInvalid);
        }
    }
    Ok(())
}

/// Sorted, strictly unique id spellings (byte order).
fn sorted_unique<'a>(ids: impl Iterator<Item = &'a str>) -> Result<(), LockFailure> {
    let mut previous: Option<&str> = None;
    for id in ids {
        if let Some(before) = previous {
            if before.as_bytes() >= id.as_bytes() {
                return Err(LockFailure::ReferenceInvalid);
            }
        }
        previous = Some(id);
    }
    Ok(())
}
// Raw serde shapes. Field names are the closed v1 wire names; unknown
// fields are rejected and duplicate keys collapse only to be caught by the
// canonical byte-equality gate as `lock.noncanonical`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLock {
    schema_version: String,
    resolver: RawResolver,
    core: RawCore,
    contracts: RawContracts,
    adapters: Vec<RawAdapter>,
    generators: Vec<RawGenerator>,
    profiles: Vec<RawProfile>,
    capabilities: Vec<RawCapability>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResolver {
    version: String,
    #[serde(rename = "request_digest")]
    request_digest: String,
    catalogs: Vec<RawCatalog>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalog {
    id: String,
    digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCore {
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawContracts {
    registry: RawPin,
    model: RawPin,
    ir: RawPin,
    target_protocol: Option<RawPin>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPin {
    version: String,
    digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAdapter {
    id: String,
    version: String,
    digest: String,
    source: RawSource,
    #[serde(rename = "compatibility_digest")]
    compatibility_digest: String,
    artifacts: Vec<RawArtifact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSource {
    kind: String,
    id: String,
    digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArtifact {
    platform: String,
    digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGenerator {
    id: String,
    version: String,
    digest: String,
    adapter: RawRef,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRef {
    id: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    id: String,
    version: String,
    #[serde(rename = "source_digest")]
    source_digest: String,
    digest: String,
    adapters: Vec<RawRef>,
    generators: Vec<RawRef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCapability {
    target: String,
    profile: String,
    id: String,
    version: String,
    support: String,
    provider: RawProvider,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProvider {
    kind: String,
    id: String,
    version: String,
}

/// Pairwise strictly-ascending check over the wire sort key.
fn require_sorted<T>(items: &[T], key: impl Fn(&T) -> &String) -> Result<(), LockFailure> {
    for pair in items.windows(2) {
        if key(&pair[0]).as_bytes() >= key(&pair[1]).as_bytes() {
            return Err(LockFailure::ReferenceInvalid);
        }
    }
    Ok(())
}
