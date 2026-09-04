//! Lock verification, contract digest recomputation, and tamper detection
//! (issue #10).
//!
//! [`LockVerifier::verify`] re-derives every declared digest from its
//! documented domain: the embedded registry artifact bytes, the committed
//! Model schema bytes for the pinned version, the IR/protocol identity
//! lines, and — through the caller-supplied [`RuntimeInventory`] — the
//! exact local component bytes by typed identity, never install paths.
//! The verdict separates integrity denials (`lock.digest-mismatch`, exit 3)
//! from availability (exit 4) and from request staleness (`lock.stale`,
//! exit 1). [`LockRequirement::Required`] is the exact preflight issue #91
//! reuses for `generate --locked`/`verify --locked`.

use super::resolution::ResolutionRequest;
use super::types::{
    ComponentId, ContractPin, LockDigest, Lockfile, Platform, SemVer, Sha256Digest,
    RESOLVER_VERSION,
};
use super::LockFailure;
use crate::versioning::family::ModelContract;
use crate::versioning::plan::sha256_hex;
use crate::versioning::registry::REGISTRY_BYTES;
use crate::versioning::ContractVersion;
use crate::versioning::VersionRegistry;

/// The exact committed Model 0.1.0 schema bytes.
pub const MODEL_SCHEMA_V0_1_0_BYTES: &[u8] =
    include_bytes!("../../../../contracts/model.schema.v0.1.0.json");

/// The exact committed Model 1.0.0 schema bytes.
pub const MODEL_SCHEMA_V1_0_0_BYTES: &[u8] =
    include_bytes!("../../../../contracts/model.schema.v1.0.0.json");

/// The exact committed Model schema bytes for one accepted version
/// spelling; `None` for versions without a committed schema artifact.
pub fn model_schema_bytes_for(spelling: &str) -> Option<&'static [u8]> {
    match spelling {
        "0.1.0" => Some(MODEL_SCHEMA_V0_1_0_BYTES),
        "1.0.0" => Some(MODEL_SCHEMA_V1_0_0_BYTES),
        _ => None,
    }
}

/// The digest over the exact contract identity line (`<identity>\n`); the
/// declared byte domain for families whose contract artifact is the
/// published identity itself (IR today, protocol after #27).
pub fn identity_line_digest(identity: &str) -> Sha256Digest {
    let mut bytes = identity.as_bytes().to_vec();
    bytes.push(b'\n');
    Sha256Digest::from_hex(&sha256_hex(&bytes))
}

/// Recompute the registry-family pin from the exact embedded artifact.
pub fn registry_pin(registry: &VersionRegistry) -> Result<ContractPin, LockFailure> {
    let version = SemVer::parse(&registry.registry_version().to_string())?;
    Ok(ContractPin::new(
        version,
        Sha256Digest::from_hex(&sha256_hex(REGISTRY_BYTES)),
    ))
}

/// Recompute the Model-family pin for one accepted version.
pub fn model_pin(version: &ContractVersion<ModelContract>) -> Result<ContractPin, LockFailure> {
    let spelling = version.to_string();
    let bytes =
        model_schema_bytes_for(&spelling).ok_or(LockFailure::ComponentVersionUnsupported {
            family: "model",
            version: spelling.clone(),
        })?;
    Ok(ContractPin::new(
        SemVer::parse(&spelling)?,
        Sha256Digest::from_hex(&sha256_hex(bytes)),
    ))
}

/// Recompute the IR-family pin from the published identity.
pub fn ir_pin() -> Result<ContractPin, LockFailure> {
    let identity = crate::ir::IDENTITY;
    let version = identity
        .rsplit('@')
        .next()
        .ok_or(LockFailure::SchemaInvalid)?;
    Ok(ContractPin::new(
        SemVer::parse(version)?,
        identity_line_digest(identity),
    ))
}

/// Recompute the protocol-family pin; `None` while unpublished.
pub fn protocol_pin(registry: &VersionRegistry) -> Result<Option<ContractPin>, LockFailure> {
    match registry.protocol().current() {
        None => Ok(None),
        Some(version) => {
            let identity = format!("dev.lekalo.protocol@{version}");
            Ok(Some(ContractPin::new(
                SemVer::parse(&version.to_string())?,
                identity_line_digest(&identity),
            )))
        }
    }
}

/// Recompute every contract pin for one request's Model version in one
/// call; the resolver and the verifier share this exact recomputation.
pub fn embedded_contract_pins(
    registry: &VersionRegistry,
    model_version: &ContractVersion<ModelContract>,
) -> Result<(ContractPin, ContractPin, ContractPin, Option<ContractPin>), LockFailure> {
    Ok((
        registry_pin(registry)?,
        model_pin(model_version)?,
        ir_pin()?,
        protocol_pin(registry)?,
    ))
}

/// One locally available component: exact bytes digest by typed identity,
/// never an install path.
#[derive(Clone, Debug)]
pub struct InventoryComponent {
    id: ComponentId,
    version: SemVer,
    digest: Sha256Digest,
}

impl InventoryComponent {
    /// Construct one inventory entry from typed parts.
    pub fn new(id: ComponentId, version: SemVer, digest: Sha256Digest) -> Self {
        Self {
            id,
            version,
            digest,
        }
    }
}

/// One locally available profile snapshot.
#[derive(Clone, Debug)]
pub struct InventoryProfile {
    id: ComponentId,
    version: SemVer,
    source_digest: Sha256Digest,
    digest: Sha256Digest,
}

impl InventoryProfile {
    /// Construct one inventory entry from typed parts.
    pub fn new(
        id: ComponentId,
        version: SemVer,
        source_digest: Sha256Digest,
        digest: Sha256Digest,
    ) -> Self {
        Self {
            id,
            version,
            source_digest,
            digest,
        }
    }
}

/// One locally available platform artifact of an adapter.
#[derive(Clone, Debug)]
pub struct InventoryArtifact {
    adapter: ComponentId,
    platform: Platform,
    digest: Sha256Digest,
}

impl InventoryArtifact {
    /// Construct one inventory entry from typed parts.
    pub fn new(adapter: ComponentId, platform: Platform, digest: Sha256Digest) -> Self {
        Self {
            adapter,
            platform,
            digest,
        }
    }
}

/// The exact local component inventory by typed identity.
#[derive(Clone, Debug)]
pub struct RuntimeInventory {
    adapters: Vec<InventoryComponent>,
    generators: Vec<InventoryComponent>,
    profiles: Vec<InventoryProfile>,
    artifacts: Vec<InventoryArtifact>,
}

impl RuntimeInventory {
    /// The empty inventory (contract-only verification, CI use).
    pub fn empty() -> Self {
        Self {
            adapters: Vec::new(),
            generators: Vec::new(),
            profiles: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    /// Declare one local adapter package.
    pub fn with_adapter(mut self, component: InventoryComponent) -> Self {
        self.adapters.push(component);
        self
    }

    /// Declare one local generator implementation.
    pub fn with_generator(mut self, component: InventoryComponent) -> Self {
        self.generators.push(component);
        self
    }

    /// Declare one local profile snapshot.
    pub fn with_profile(mut self, profile: InventoryProfile) -> Self {
        self.profiles.push(profile);
        self
    }

    /// Declare one local platform artifact.
    pub fn with_artifact(mut self, artifact: InventoryArtifact) -> Self {
        self.artifacts.push(artifact);
        self
    }

    fn adapter(&self, id: &str, version: &str) -> Option<&Sha256Digest> {
        self.adapters
            .iter()
            .find(|component| component.id.as_str() == id && component.version.as_str() == version)
            .map(|component| &component.digest)
    }

    fn generator(&self, id: &str, version: &str) -> Option<&Sha256Digest> {
        self.generators
            .iter()
            .find(|component| component.id.as_str() == id && component.version.as_str() == version)
            .map(|component| &component.digest)
    }

    fn profile(&self, id: &str, version: &str) -> Option<(&Sha256Digest, &Sha256Digest)> {
        self.profiles
            .iter()
            .find(|profile| profile.id.as_str() == id && profile.version.as_str() == version)
            .map(|profile| (&profile.source_digest, &profile.digest))
    }

    fn artifact(&self, adapter: &str, platform: &str) -> Option<&Sha256Digest> {
        self.artifacts
            .iter()
            .find(|artifact| {
                artifact.adapter.as_str() == adapter
                    && (artifact.platform.as_str() == platform
                        || artifact.platform.as_str() == "any")
            })
            .map(|artifact| &artifact.digest)
    }
}

/// The verification strength. `Required` is the exact downstream preflight
/// issue #91 owns the flags for; `Optional` verifies everything declared
/// available and ignores absent components.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockRequirement {
    /// Verify structure, request currency, contracts, and every component
    /// the inventory declares.
    Optional,
    /// Additionally require every locked component and platform artifact to
    /// be locally available before any runner or writer may start.
    Required,
}

use LockRequirement as Requirement;

/// The verdict of one verification run.
#[derive(Clone, Debug, PartialEq)]
pub enum LockVerdict {
    /// The lock matches the request, contracts, and inventory.
    Satisfied {
        /// The digest of the verified lock payload.
        digest: LockDigest,
    },
    /// The verification refused with the typed, classified reason.
    Refused(LockFailure),
}

/// The verification entry point.
pub struct LockVerifier;

impl LockVerifier {
    /// Verify one parsed lock against the request, the inventory, and the
    /// accepted registry. Pure: no filesystem, network, or process access.
    pub fn verify(
        lock: &Lockfile,
        request: &ResolutionRequest,
        inventory: &RuntimeInventory,
        registry: &VersionRegistry,
        requirement: Requirement,
    ) -> LockVerdict {
        match Self::verify_inner(lock, request, inventory, registry, requirement) {
            Ok(digest) => LockVerdict::Satisfied { digest },
            Err(failure) => LockVerdict::Refused(failure),
        }
    }

    fn verify_inner(
        lock: &Lockfile,
        request: &ResolutionRequest,
        inventory: &RuntimeInventory,
        registry: &VersionRegistry,
        requirement: Requirement,
    ) -> Result<LockDigest, LockFailure> {
        // 1. Protocol compatibility first: a lock made under a published
        // protocol can never be honored by an unpublished registry, and
        // that is a stronger diagnosis than request staleness.
        if lock.target_protocol().is_some() && registry.protocol().current().is_none() {
            return Err(LockFailure::ProtocolUnpublished);
        }
        // 2. Request currency: only changed requests or contracts make a
        // lock stale; newer available candidates never do.
        if lock.request_digest().as_str() != request.request_digest().as_str() {
            return Err(LockFailure::Stale);
        }

        // 3. The resolver algorithm must be the accepted one.
        if lock.resolver_version().as_str() != RESOLVER_VERSION {
            return Err(LockFailure::ComponentVersionUnsupported {
                family: "resolver",
                version: lock.resolver_version().as_str().to_owned(),
            });
        }

        // 3. Contract digests over their exact declared domains.
        check_pin(lock.registry_pin(), &registry_pin(registry)?, "registry")?;
        check_pin(
            lock.model_pin(),
            &model_pin_of_spelling(lock.model_pin().version().as_str())?,
            "model",
        )?;
        check_pin(lock.ir_pin(), &ir_pin()?, "ir")?;
        let protocol = protocol_pin(registry)?;
        match (lock.target_protocol(), protocol) {
            (None, None) => {}
            (Some(locked), Some(actual)) => check_pin(locked, &actual, "protocol")?,
            (Some(_), None) => return Err(LockFailure::ProtocolUnpublished),
            (None, Some(_)) => return Err(LockFailure::ReferenceInvalid),
        }

        // 4. Components: every locked identity is checked against the
        // inventory when declared; `Required` demands availability.
        for adapter in lock.adapters() {
            let identity = adapter.id().as_str();
            let version = adapter.version().as_str();
            match inventory.adapter(identity, version) {
                Some(digest) => {
                    if digest != adapter.digest() {
                        return Err(LockFailure::DigestMismatch {
                            domain: "adapter",
                            identity: identity.to_owned(),
                        });
                    }
                }
                None if requirement == Requirement::Required => {
                    return Err(LockFailure::ComponentUnavailable {
                        kind: "adapter",
                        id: identity.to_owned(),
                    });
                }
                None => {}
            }
            for artifact in adapter.artifacts() {
                match inventory.artifact(identity, artifact.platform().as_str()) {
                    Some(digest) => {
                        if digest != artifact.digest() {
                            return Err(LockFailure::DigestMismatch {
                                domain: "artifact",
                                identity: format!("{identity}@{}", artifact.platform().as_str()),
                            });
                        }
                    }
                    None if requirement == Requirement::Required => {
                        return Err(LockFailure::PlatformUnavailable {
                            adapter: identity.to_owned(),
                            platform: artifact.platform().as_str().to_owned(),
                        });
                    }
                    None => {}
                }
            }
        }
        for generator in lock.generators() {
            let identity = generator.id().as_str();
            let version = generator.version().as_str();
            match inventory.generator(identity, version) {
                Some(digest) => {
                    if digest != generator.digest() {
                        return Err(LockFailure::DigestMismatch {
                            domain: "generator",
                            identity: identity.to_owned(),
                        });
                    }
                }
                None if requirement == Requirement::Required => {
                    return Err(LockFailure::ComponentUnavailable {
                        kind: "generator",
                        id: identity.to_owned(),
                    });
                }
                None => {}
            }
        }
        for profile in lock.profiles() {
            let identity = profile.id().as_str();
            let version = profile.version().as_str();
            match inventory.profile(identity, version) {
                Some((source, resolved)) => {
                    if source != profile.source_digest() {
                        return Err(LockFailure::DigestMismatch {
                            domain: "profile-source",
                            identity: identity.to_owned(),
                        });
                    }
                    if resolved != profile.digest() {
                        return Err(LockFailure::DigestMismatch {
                            domain: "profile",
                            identity: identity.to_owned(),
                        });
                    }
                }
                None if requirement == Requirement::Required => {
                    return Err(LockFailure::ComponentUnavailable {
                        kind: "profile",
                        id: identity.to_owned(),
                    });
                }
                None => {}
            }
        }
        Ok(lock.digest())
    }
}

fn model_pin_of_spelling(spelling: &str) -> Result<ContractPin, LockFailure> {
    let bytes =
        model_schema_bytes_for(spelling).ok_or(LockFailure::ComponentVersionUnsupported {
            family: "model",
            version: spelling.to_owned(),
        })?;
    Ok(ContractPin::new(
        SemVer::parse(spelling)?,
        Sha256Digest::from_hex(&sha256_hex(bytes)),
    ))
}

fn check_pin(
    locked: &ContractPin,
    recomputed: &ContractPin,
    domain: &'static str,
) -> Result<(), LockFailure> {
    if locked.digest() != recomputed.digest() {
        return Err(LockFailure::DigestMismatch {
            domain,
            identity: locked.version().as_str().to_owned(),
        });
    }
    Ok(())
}
