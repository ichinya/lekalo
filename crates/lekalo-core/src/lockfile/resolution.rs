//! The pure hermetic lock resolver (issue #10).
//!
//! [`LockResolver::resolve`] consumes one typed, path-free
//! [`ResolutionRequest`], one sealed [`CandidateSet`], and the accepted `#9`
//! [`VersionRegistry`], and produces the fully validated [`Lockfile`]. The
//! resolver performs no filesystem, `PATH`, registry, network, install,
//! process spawn, or profile parsing: candidates and inventories are
//! supplied by upstream seams (#28/#29/#32) and are only validated, frozen,
//! and compared here.
//!
//! Filter order is fixed and documented: registry support for the request's
//! contract versions, the protocol-publication gate, catalog availability,
//! per-adapter compatibility preflights (IR range, protocol range, required
//! extensions), the requested target platform, required capability support,
//! and platform artifact availability. Selection takes the highest stable
//! exact SemVer per identity; prereleases require an explicit exact opt-in
//! in the request, and two surviving candidates at one identity/version are
//! an ambiguity, never a source-order choice.

use super::types::{
    ArtifactPin, CapabilityId, CatalogRef, ComponentId, Lockfile, Platform, ProviderKind,
    ProviderRef, SemVer, Sha256Digest, SourceRef, Support, RESOLVER_VERSION,
};
use super::{canonical, LockFailure};
use crate::versioning::compatibility::{AdapterCompatibilityManifest, CompatibilityPreflight};
use crate::versioning::family::{IrContract, ModelContract, ProtocolContract, RegistryContract};
use crate::versioning::ContractVersion;
use crate::versioning::VersionRegistry;

/// The typed, path-free resolution request.
///
/// Construction is builder-style: every setter validates or normalizes its
/// input, the canonical byte form is derived from the typed fields only,
/// and the request digest is SHA-256 over those bytes. Available-candidate
/// order is excluded by construction (candidates are not part of the
/// request).
#[derive(Clone, Debug)]
pub struct ResolutionRequest {
    registry_version: ContractVersion<RegistryContract>,
    model_version: ContractVersion<ModelContract>,
    ir_version: ContractVersion<IrContract>,
    protocol_version: Option<ContractVersion<ProtocolContract>>,
    core_version: SemVer,
    target: Option<Platform>,
    adapters: Vec<ComponentId>,
    generators: Vec<ComponentId>,
    profiles: Vec<ComponentId>,
    required_capabilities: Vec<CapabilityId>,
    allow_partial: bool,
    exact_versions: Vec<String>,
    catalogs: Vec<CatalogRef>,
}

impl ResolutionRequest {
    /// Build the base request from the accepted registry, the loaded
    /// project's Model version, and the product version this workspace
    /// carries.
    pub fn new(
        registry: &VersionRegistry,
        model_version: &ContractVersion<ModelContract>,
        core_version: SemVer,
    ) -> Self {
        Self {
            registry_version: registry.registry_version().clone(),
            model_version: model_version.clone(),
            ir_version: ContractVersion::<IrContract>::current(),
            protocol_version: registry.protocol().current().cloned(),
            core_version,
            target: None,
            adapters: Vec::new(),
            generators: Vec::new(),
            profiles: Vec::new(),
            required_capabilities: Vec::new(),
            allow_partial: false,
            exact_versions: Vec::new(),
            catalogs: Vec::new(),
        }
    }

    /// Pin the requested target platform.
    pub fn with_target(mut self, target: Platform) -> Self {
        self.target = Some(target);
        self
    }

    /// Request one adapter id (duplicates collapse; output stays sorted).
    pub fn with_adapter(mut self, id: ComponentId) -> Self {
        if !self.adapters.iter().any(|existing| existing == &id) {
            self.adapters.push(id);
            self.adapters
                .sort_by(|left, right| left.as_str().as_bytes().cmp(right.as_str().as_bytes()));
        }
        self
    }

    /// Request one generator id.
    pub fn with_generator(mut self, id: ComponentId) -> Self {
        if !self.generators.iter().any(|existing| existing == &id) {
            self.generators.push(id);
            self.generators
                .sort_by(|left, right| left.as_str().as_bytes().cmp(right.as_str().as_bytes()));
        }
        self
    }

    /// Request one profile id.
    pub fn with_profile(mut self, id: ComponentId) -> Self {
        if !self.profiles.iter().any(|existing| existing == &id) {
            self.profiles.push(id);
            self.profiles
                .sort_by(|left, right| left.as_str().as_bytes().cmp(right.as_str().as_bytes()));
        }
        self
    }

    /// Require one capability (partial support satisfies only with the
    /// explicit accepted policy).
    pub fn require_capability(mut self, id: CapabilityId) -> Self {
        if !self
            .required_capabilities
            .iter()
            .any(|existing| existing == &id)
        {
            self.required_capabilities.push(id);
            self.required_capabilities.sort();
        }
        self
    }

    /// Record the explicit accepted partial-support policy.
    pub fn with_partial_policy(mut self) -> Self {
        self.allow_partial = true;
        self
    }

    /// Opt in to one exact (normally prerelease) version spelling.
    pub fn with_exact_version(mut self, spelling: &str) -> Result<Self, LockFailure> {
        SemVer::parse(spelling)?;
        if !self
            .exact_versions
            .iter()
            .any(|existing| existing == spelling)
        {
            self.exact_versions.push(spelling.to_owned());
            self.exact_versions.sort();
        }
        Ok(self)
    }

    /// Consume one immutable catalog snapshot identity.
    pub fn with_catalog(mut self, catalog: CatalogRef) -> Self {
        if !self.catalogs.iter().any(|existing| existing == &catalog) {
            self.catalogs.push(catalog);
            self.catalogs.sort();
        }
        self
    }

    /// The product version the request pins.
    pub fn core_version(&self) -> &SemVer {
        &self.core_version
    }

    /// The consumed catalog snapshot identities, sorted.
    pub fn catalogs(&self) -> &[CatalogRef] {
        &self.catalogs
    }

    /// SHA-256 over the canonical typed request bytes.
    pub fn request_digest(&self) -> Sha256Digest {
        Sha256Digest::from_hex(&crate::versioning::plan::sha256_hex(
            &self.canonical_bytes(),
        ))
    }

    /// The canonical typed request bytes (sorted keys, path-free).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let text = canonical::Canon::object(vec![
            (
                "allow_partial",
                canonical::Canon::str(self.allow_partial.to_string()),
            ),
            (
                "catalogs",
                canonical::Canon::array(
                    self.catalogs
                        .iter()
                        .map(|catalog| {
                            canonical::Canon::object(vec![
                                ("digest", canonical::Canon::str(catalog.digest().as_str())),
                                ("id", canonical::Canon::str(catalog.id().as_str())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "core_version",
                canonical::Canon::str(self.core_version.as_str()),
            ),
            (
                "exact_versions",
                canonical::Canon::array(
                    self.exact_versions
                        .iter()
                        .map(|spelling| canonical::Canon::str(spelling.clone()))
                        .collect(),
                ),
            ),
            (
                "ir_version",
                canonical::Canon::str(self.ir_version.to_string()),
            ),
            (
                "model_version",
                canonical::Canon::str(self.model_version.to_string()),
            ),
            (
                "profiles",
                canonical::Canon::array(
                    self.profiles
                        .iter()
                        .map(|id| canonical::Canon::str(id.as_str()))
                        .collect(),
                ),
            ),
            (
                "protocol_version",
                match &self.protocol_version {
                    Some(protocol) => canonical::Canon::str(protocol.to_string()),
                    None => canonical::Canon::Null,
                },
            ),
            (
                "registry",
                canonical::Canon::object(vec![
                    (
                        "identity",
                        canonical::Canon::str(crate::versioning::REGISTRY_IDENTITY),
                    ),
                    (
                        "version",
                        canonical::Canon::str(self.registry_version.to_string()),
                    ),
                ]),
            ),
            (
                "requested_adapters",
                canonical::Canon::array(
                    self.adapters
                        .iter()
                        .map(|id| canonical::Canon::str(id.as_str()))
                        .collect(),
                ),
            ),
            (
                "requested_generators",
                canonical::Canon::array(
                    self.generators
                        .iter()
                        .map(|id| canonical::Canon::str(id.as_str()))
                        .collect(),
                ),
            ),
            (
                "required_capabilities",
                canonical::Canon::array(
                    self.required_capabilities
                        .iter()
                        .map(|id| canonical::Canon::str(id.as_str()))
                        .collect(),
                ),
            ),
            (
                "target",
                match &self.target {
                    Some(target) => canonical::Canon::str(target.as_str()),
                    None => canonical::Canon::Null,
                },
            ),
        ])
        .to_json();
        text.into_bytes()
    }
}

/// One candidate adapter package with its typed `#9` compatibility
/// manifest; the locked compatibility digest is derived from the manifest.
#[derive(Clone, Debug)]
pub struct CandidateAdapter {
    id: ComponentId,
    version: SemVer,
    digest: Sha256Digest,
    source: SourceRef,
    artifacts: Vec<ArtifactPin>,
    manifest: AdapterCompatibilityManifest,
}

impl CandidateAdapter {
    /// Construct one candidate from typed parts.
    pub fn new(
        id: ComponentId,
        version: SemVer,
        digest: Sha256Digest,
        source: SourceRef,
        artifacts: Vec<ArtifactPin>,
        manifest: AdapterCompatibilityManifest,
    ) -> Self {
        Self {
            id,
            version,
            digest,
            source,
            artifacts,
            manifest,
        }
    }

    /// The digest over the canonical manifest bytes (the locked
    /// `compatibility_digest` domain).
    pub fn compatibility_digest(&self) -> Sha256Digest {
        Sha256Digest::from_hex(&crate::versioning::plan::sha256_hex(
            canonical::manifest_bytes(&self.manifest).as_slice(),
        ))
    }
}

/// One candidate generator.
#[derive(Clone, Debug)]
pub struct CandidateGenerator {
    id: ComponentId,
    version: SemVer,
    digest: Sha256Digest,
    adapter: super::types::ComponentRef,
}

impl CandidateGenerator {
    /// Construct one candidate from typed parts.
    pub fn new(
        id: ComponentId,
        version: SemVer,
        digest: Sha256Digest,
        adapter: super::types::ComponentRef,
    ) -> Self {
        Self {
            id,
            version,
            digest,
            adapter,
        }
    }
}

/// One candidate profile: declared and resolved digests over canonical
/// profile bytes.
#[derive(Clone, Debug)]
pub struct CandidateProfile {
    id: ComponentId,
    version: SemVer,
    source_digest: Sha256Digest,
    digest: Sha256Digest,
    adapters: Vec<super::types::ComponentRef>,
    generators: Vec<super::types::ComponentRef>,
}

impl CandidateProfile {
    /// Construct one candidate from typed parts.
    pub fn new(
        id: ComponentId,
        version: SemVer,
        source_digest: Sha256Digest,
        digest: Sha256Digest,
        adapters: Vec<super::types::ComponentRef>,
        generators: Vec<super::types::ComponentRef>,
    ) -> Self {
        Self {
            id,
            version,
            source_digest,
            digest,
            adapters,
            generators,
        }
    }
}

/// One candidate capability declaration.
#[derive(Clone, Debug)]
pub struct CandidateCapability {
    target: Platform,
    profile: ComponentId,
    id: CapabilityId,
    version: SemVer,
    support: Support,
    provider: ProviderRef,
}

impl CandidateCapability {
    /// Construct one candidate from typed parts.
    pub fn new(
        target: Platform,
        profile: ComponentId,
        id: CapabilityId,
        version: SemVer,
        support: Support,
        provider: ProviderRef,
    ) -> Self {
        Self {
            target,
            profile,
            id,
            version,
            support,
            provider,
        }
    }
}

/// One immutable catalog snapshot available to resolution.
#[derive(Clone, Debug)]
pub struct CatalogSnapshot {
    id: ComponentId,
    digest: Sha256Digest,
}

impl CatalogSnapshot {
    /// Construct one snapshot identity from typed parts.
    pub fn new(id: ComponentId, digest: Sha256Digest) -> Self {
        Self { id, digest }
    }
}

/// The sealed candidate supply. Validation rejects duplicate identities up
/// front so the resolver never chooses between indistinguishable
/// candidates by order.
#[derive(Clone, Debug)]
pub struct CandidateSet {
    adapters: Vec<CandidateAdapter>,
    generators: Vec<CandidateGenerator>,
    profiles: Vec<CandidateProfile>,
    capabilities: Vec<CandidateCapability>,
    catalogs: Vec<CatalogSnapshot>,
}

impl CandidateSet {
    /// The empty contract-only supply.
    pub fn empty() -> Self {
        Self {
            adapters: Vec::new(),
            generators: Vec::new(),
            profiles: Vec::new(),
            capabilities: Vec::new(),
            catalogs: Vec::new(),
        }
    }

    /// Add one adapter candidate.
    pub fn with_adapter(mut self, adapter: CandidateAdapter) -> Self {
        self.adapters.push(adapter);
        self
    }

    /// Add one generator candidate.
    pub fn with_generator(mut self, generator: CandidateGenerator) -> Self {
        self.generators.push(generator);
        self
    }

    /// Add one profile candidate.
    pub fn with_profile(mut self, profile: CandidateProfile) -> Self {
        self.profiles.push(profile);
        self
    }

    /// Add one capability candidate.
    pub fn with_capability(mut self, capability: CandidateCapability) -> Self {
        self.capabilities.push(capability);
        self
    }

    /// Add one available catalog snapshot.
    pub fn with_catalog(mut self, catalog: CatalogSnapshot) -> Self {
        self.catalogs.push(catalog);
        self
    }

    fn validate(&self) -> Result<(), LockFailure> {
        fn unique<'a, K: Ord + Eq + Clone>(
            identities: impl Iterator<Item = (K, &'a str)>,
        ) -> Result<(), LockFailure> {
            let mut sorted: Vec<(K, &str)> = identities.collect();
            sorted.sort();
            for pair in sorted.windows(2) {
                if pair[0] == pair[1] {
                    return Err(LockFailure::ResolutionAmbiguous {
                        identity: pair[0].1.to_owned(),
                    });
                }
            }
            Ok(())
        }
        unique(self.adapters.iter().map(|adapter| {
            (
                (adapter.id.clone(), adapter.version.clone()),
                adapter.id.as_str(),
            )
        }))?;
        unique(self.generators.iter().map(|generator| {
            (
                (generator.id.clone(), generator.version.clone()),
                generator.id.as_str(),
            )
        }))?;
        unique(self.profiles.iter().map(|profile| {
            (
                (profile.id.clone(), profile.version.clone()),
                profile.id.as_str(),
            )
        }))?;
        Ok(())
    }
}
/// The resolution entry point.
pub struct LockResolver;

impl LockResolver {
    /// Resolve the request against the sealed candidates and the accepted
    /// registry into one fully validated lock.
    pub fn resolve(
        request: &ResolutionRequest,
        candidates: &CandidateSet,
        registry: &VersionRegistry,
    ) -> Result<Lockfile, LockFailure> {
        candidates.validate()?;

        // Fixed filter order, step 1: exact-set registry support of the
        // requested contract versions.
        if !registry
            .model()
            .record(&request.model_version)
            .is_some_and(|record| record.state.is_usable())
        {
            return Err(LockFailure::ComponentVersionUnsupported {
                family: "model",
                version: request.model_version.to_string(),
            });
        }
        if !registry
            .ir()
            .record(&request.ir_version)
            .is_some_and(|record| record.state.is_usable())
        {
            return Err(LockFailure::ComponentVersionUnsupported {
                family: "ir",
                version: request.ir_version.to_string(),
            });
        }

        // Step 2: the protocol-publication gate. With the protocol family
        // unpublished, nothing executable can be locked and no executable
        // candidate may even be offered.
        if request.protocol_version.is_none()
            && (request.target.is_some()
                || !request.adapters.is_empty()
                || !request.generators.is_empty()
                || !request.profiles.is_empty()
                || !request.required_capabilities.is_empty()
                || !candidates.adapters.is_empty()
                || !candidates.generators.is_empty()
                || !candidates.profiles.is_empty()
                || !candidates.capabilities.is_empty())
        {
            return Err(LockFailure::ProtocolUnpublished);
        }

        // Catalog availability: every consumed snapshot must be declared
        // available by the sealed supply.
        for catalog in &request.catalogs {
            let available = candidates.catalogs.iter().any(|snapshot| {
                snapshot.id == *catalog.id() && snapshot.digest == *catalog.digest()
            });
            if !available {
                return Err(LockFailure::CatalogUnavailable);
            }
        }

        // Steps 3-5: compatibility, target availability, version choice.
        let selected_adapters = Self::select_adapters(request, candidates, registry)?;
        let selected_generators = Self::select_generators(request, candidates, &selected_adapters)?;
        let selected_profiles = Self::select_profiles(
            request,
            candidates,
            &selected_adapters,
            &selected_generators,
        )?;
        let selected_capabilities = Self::select_capabilities(
            request,
            candidates,
            &selected_profiles,
            &selected_generators,
        )?;

        let (registry_pin, model_pin, ir_pin, protocol_pin) =
            super::verify::embedded_contract_pins(registry, &request.model_version)?;

        let mut adapters = Vec::with_capacity(selected_adapters.len());
        for candidate in &selected_adapters {
            adapters.push(super::types::ResolvedAdapter::from_parts(
                candidate.id.clone(),
                candidate.version.clone(),
                candidate.digest.clone(),
                candidate.source.clone(),
                candidate.compatibility_digest(),
                candidate.artifacts.clone(),
            )?);
        }
        let generators = selected_generators
            .iter()
            .map(|candidate| {
                super::types::ResolvedGenerator::from_parts(
                    candidate.id.clone(),
                    candidate.version.clone(),
                    candidate.digest.clone(),
                    candidate.adapter.clone(),
                )
            })
            .collect();
        let mut profiles = Vec::with_capacity(selected_profiles.len());
        for candidate in &selected_profiles {
            profiles.push(super::types::ResolvedProfile::from_parts(
                candidate.id.clone(),
                candidate.version.clone(),
                candidate.source_digest.clone(),
                candidate.digest.clone(),
                candidate.adapters.clone(),
                candidate.generators.clone(),
            )?);
        }
        let capabilities = selected_capabilities
            .iter()
            .map(|candidate| {
                super::types::ResolvedCapability::from_parts(
                    candidate.target.clone(),
                    candidate.profile.clone(),
                    candidate.id.clone(),
                    candidate.version.clone(),
                    candidate.support,
                    candidate.provider.clone(),
                )
            })
            .collect();

        Ok(Lockfile::from_parts(
            SemVer::parse(RESOLVER_VERSION)?,
            request.request_digest(),
            request.catalogs.clone(),
            request.core_version.clone(),
            registry_pin,
            model_pin,
            ir_pin,
            protocol_pin,
            adapters,
            generators,
            profiles,
            capabilities,
        ))
    }

    fn select_adapters(
        request: &ResolutionRequest,
        candidates: &CandidateSet,
        registry: &VersionRegistry,
    ) -> Result<Vec<CandidateAdapter>, LockFailure> {
        let mut selected = Vec::new();
        for id in &request.adapters {
            let group: Vec<&CandidateAdapter> = candidates
                .adapters
                .iter()
                .filter(|candidate| &candidate.id == id)
                .collect();
            if group.is_empty() {
                return Err(LockFailure::ComponentUnavailable {
                    kind: "adapter",
                    id: id.as_str().to_owned(),
                });
            }
            let mut incompatible = false;
            let mut survivors: Vec<&CandidateAdapter> = Vec::new();
            for candidate in group {
                let verdict = CompatibilityPreflight::check(
                    registry,
                    &request.ir_version,
                    request.protocol_version.as_ref(),
                    &candidate.manifest,
                );
                if !verdict.is_compatible() {
                    if verdict
                        .reasons()
                        .contains(&crate::versioning::reasons::EXTENSION_INCOMPATIBLE)
                    {
                        return Err(LockFailure::ExtensionIncompatible {
                            adapter: id.as_str().to_owned(),
                        });
                    }
                    incompatible = true;
                    continue;
                }
                if let Some(target) = &request.target {
                    let available = candidate.artifacts.iter().any(|artifact| {
                        artifact.platform().as_str() == target.as_str()
                            || artifact.platform().as_str() == "any"
                    });
                    if !available {
                        continue;
                    }
                }
                survivors.push(candidate);
            }
            if survivors.is_empty() {
                if incompatible {
                    return Err(LockFailure::AdapterIncompatible {
                        adapter: id.as_str().to_owned(),
                    });
                }
                let platform = request
                    .target
                    .as_ref()
                    .map(|target| target.as_str().to_owned())
                    .unwrap_or_else(|| "any".to_owned());
                return Err(LockFailure::PlatformUnavailable {
                    adapter: id.as_str().to_owned(),
                    platform,
                });
            }
            match pick_version(&survivors, &request.exact_versions) {
                Some(chosen) => selected.push((*chosen).clone()),
                None => {
                    return Err(LockFailure::ComponentUnavailable {
                        kind: "version",
                        id: id.as_str().to_owned(),
                    })
                }
            }
        }
        Ok(selected)
    }

    fn select_generators(
        request: &ResolutionRequest,
        candidates: &CandidateSet,
        selected_adapters: &[CandidateAdapter],
    ) -> Result<Vec<CandidateGenerator>, LockFailure> {
        let mut selected = Vec::new();
        for id in &request.generators {
            let group: Vec<&CandidateGenerator> = candidates
                .generators
                .iter()
                .filter(|candidate| &candidate.id == id)
                .filter(|candidate| {
                    selected_adapters.iter().any(|adapter| {
                        adapter.id.as_str() == candidate.adapter.id().as_str()
                            && adapter.version == *candidate.adapter.version()
                    })
                })
                .collect();
            if group.is_empty() {
                return Err(LockFailure::ComponentUnavailable {
                    kind: "generator",
                    id: id.as_str().to_owned(),
                });
            }
            match pick_version(&group, &request.exact_versions) {
                Some(chosen) => selected.push((*chosen).clone()),
                None => {
                    return Err(LockFailure::ComponentUnavailable {
                        kind: "version",
                        id: id.as_str().to_owned(),
                    })
                }
            }
        }
        Ok(selected)
    }

    fn select_profiles(
        request: &ResolutionRequest,
        candidates: &CandidateSet,
        selected_adapters: &[CandidateAdapter],
        selected_generators: &[CandidateGenerator],
    ) -> Result<Vec<CandidateProfile>, LockFailure> {
        let mut selected = Vec::new();
        for id in &request.profiles {
            let group: Vec<&CandidateProfile> = candidates
                .profiles
                .iter()
                .filter(|candidate| &candidate.id == id)
                .filter(|candidate| {
                    let adapters_resolve = candidate.adapters.iter().all(|reference| {
                        selected_adapters.iter().any(|adapter| {
                            adapter.id.as_str() == reference.id().as_str()
                                && adapter.version == *reference.version()
                        })
                    });
                    let generators_resolve = candidate.generators.iter().all(|reference| {
                        selected_generators.iter().any(|generator| {
                            generator.id.as_str() == reference.id().as_str()
                                && generator.version == *reference.version()
                        })
                    });
                    adapters_resolve && generators_resolve
                })
                .collect();
            if group.is_empty() {
                return Err(LockFailure::ComponentUnavailable {
                    kind: "profile",
                    id: id.as_str().to_owned(),
                });
            }
            match pick_version(&group, &request.exact_versions) {
                Some(chosen) => selected.push((*chosen).clone()),
                None => {
                    return Err(LockFailure::ComponentUnavailable {
                        kind: "version",
                        id: id.as_str().to_owned(),
                    })
                }
            }
        }
        Ok(selected)
    }

    fn select_capabilities(
        request: &ResolutionRequest,
        candidates: &CandidateSet,
        selected_profiles: &[CandidateProfile],
        selected_generators: &[CandidateGenerator],
    ) -> Result<Vec<CandidateCapability>, LockFailure> {
        let mut selected: Vec<CandidateCapability> = Vec::new();
        for capability in &candidates.capabilities {
            if let Some(target) = &request.target {
                if capability.target.as_str() != target.as_str()
                    && capability.target.as_str() != "any"
                {
                    continue;
                }
            }
            if !selected_profiles
                .iter()
                .any(|profile| profile.id == capability.profile)
            {
                continue;
            }
            let provider_selected = match capability.provider.kind() {
                ProviderKind::Adapter => request
                    .adapters
                    .iter()
                    .any(|id| id.as_str() == capability.provider.id().as_str()),
                ProviderKind::Generator => selected_generators.iter().any(|generator| {
                    generator.id.as_str() == capability.provider.id().as_str()
                        && generator.version == *capability.provider.version()
                }),
            };
            if !provider_selected {
                continue;
            }
            selected.push(capability.clone());
        }
        // Every required capability must be satisfied by a full (or
        // explicitly accepted partial) entry from a selected provider.
        for required in &request.required_capabilities {
            let satisfied = selected.iter().any(|capability| {
                &capability.id == required
                    && (capability.support == Support::Full
                        || (capability.support == Support::Partial && request.allow_partial))
            });
            if !satisfied {
                return Err(LockFailure::ComponentUnavailable {
                    kind: "capability",
                    id: required.as_str().to_owned(),
                });
            }
        }
        Ok(selected)
    }
}

/// Highest stable version unless the exact spelling is explicitly opted in.
fn pick_version<'a, T>(group: &[&'a T], exact: &[String]) -> Option<&'a T>
where
    T: VersionedCandidate,
{
    let mut best: Option<(SemVer, &'a T)> = None;
    for candidate in group {
        let spelling = candidate.version_spelling();
        let parsed = SemVer::parse(spelling).ok()?;
        if !parsed.is_stable() && !exact.iter().any(|opted| opted == spelling) {
            continue;
        }
        match &best {
            Some((current, _)) if *current >= parsed => {}
            _ => best = Some((parsed, candidate)),
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// One exact version spelling carried by every candidate kind.
trait VersionedCandidate {
    fn version_spelling(&self) -> &str;
}

macro_rules! versioned_candidate {
    ($($kind:ty),*) => {$(
        impl VersionedCandidate for $kind {
            fn version_spelling(&self) -> &str {
                self.version.as_str()
            }
        }
    )*};
}

versioned_candidate!(CandidateAdapter, CandidateGenerator, CandidateProfile);
