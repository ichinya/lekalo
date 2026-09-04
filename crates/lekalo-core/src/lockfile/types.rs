//! Private-field typed values for the committed `lekalo.lock` (issue #10).
//!
//! Every wire string is parsed into a closed typed value before it can exist
//! in a [`Lockfile`]: there is no `Default`, no public field, no unchecked
//! constructor, and no raw `String`-to-version conversion a caller could
//! forge. Versions are exact canonical SemVer spellings without build
//! metadata; digests are exactly `sha256:<64 lowercase hex>` bound to the
//! domain that computed them; identifiers use the closed lowercase grammars
//! documented in `docs/lockfile.md`. Anything URL-shaped, credential-shaped,
//! or path-escaping is refused as private data before it can serialize.

use std::cmp::Ordering;
use std::fmt;

use super::LockFailure;

/// The closed wire discriminator of the only supported lock schema.
pub const SCHEMA_VERSION: &str = "lekalo/lock/v1.0.0";

/// The contract identity of the published lock schema artifact.
pub const LOCK_IDENTITY: &str = "dev.lekalo.lock@1.0.0";

/// The independent resolver algorithm version (not a product or contract
/// version). Bumped only by a reviewed resolver change.
pub const RESOLVER_VERSION: &str = "1.0.0";

/// Canonical SemVer without build metadata: one spelling per version.
///
/// `semver::Version` is wrapped with private fields; the round-trip check
/// closes every residual spelling gap, and build metadata is rejected
/// outright because SemVer precedence ignores it.
#[derive(Clone, Debug)]
pub struct SemVer {
    parsed: semver::Version,
    text: String,
}

impl SemVer {
    /// Parse exactly one canonical spelling; anything else is refused.
    pub fn parse(text: &str) -> Result<Self, LockFailure> {
        if text.starts_with('v') || text.starts_with('V') {
            return Err(LockFailure::SchemaInvalid);
        }
        let parsed = semver::Version::parse(text).map_err(|_| LockFailure::SchemaInvalid)?;
        if parsed.build != semver::BuildMetadata::EMPTY || parsed.to_string() != text {
            return Err(LockFailure::SchemaInvalid);
        }
        Ok(Self {
            parsed,
            text: text.to_owned(),
        })
    }

    /// The canonical wire spelling.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether this version is a stable release (no prerelease).
    pub fn is_stable(&self) -> bool {
        self.parsed.pre == semver::Prerelease::EMPTY
    }
}

impl PartialEq for SemVer {
    fn eq(&self, other: &Self) -> bool {
        self.parsed == other.parsed
    }
}

impl Eq for SemVer {}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parsed.cmp(&other.parsed)
    }
}

impl fmt::Display for SemVer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

/// One exact SHA-256 digest spelling: `sha256:<64 lowercase hex>`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    /// Parse and validate the closed spelling.
    pub fn parse(text: &str) -> Result<Self, LockFailure> {
        let hex = text
            .strip_prefix("sha256:")
            .ok_or(LockFailure::SchemaInvalid)?;
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(LockFailure::SchemaInvalid);
        }
        if hex.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(LockFailure::SchemaInvalid);
        }
        Ok(Self(text.to_owned()))
    }

    /// Wrap an already-lowercase 64-hex digest (tests and internal digest
    /// computation; the spelling is debug-asserted).
    pub fn from_hex(hex: &str) -> Self {
        debug_assert!(hex.len() == 64);
        debug_assert!(hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
        Self(format!("sha256:{hex}"))
    }

    /// The exact wire spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The digest of a whole canonical lock payload (without the final LF).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockDigest(Sha256Digest);

impl LockDigest {
    pub(crate) fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// The exact wire spelling.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Whether a candidate text is refused as private data (issue #10 AC: no
/// absolute paths, URLs, or credentials ever serialize).
pub(crate) fn is_private_data(text: &str) -> bool {
    if text.starts_with('/') || text.starts_with('~') || text.starts_with('.') {
        return true;
    }
    text.chars().any(|character| {
        matches!(character, ':' | '\\' | '%') || (character as u32) < 0x20 || character == '\x7f'
    }) || text.contains("://")
        || text.contains("..")
        || text.contains("//")
        || text.contains("::")
        || credential_shaped(text)
}

/// The closed credential-shape screen: stable provider token prefixes and
/// PEM markers never belong in any lock field.
fn credential_shaped(text: &str) -> bool {
    const PREFIXES: [&str; 9] = [
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "sk-",
        "AKIA",
        "xox",
    ];
    PREFIXES.iter().any(|prefix| text.starts_with(prefix))
        || text.contains("PRIVATE KEY")
        || text.contains("BEGIN CERTIFICATE")
}

/// A stable logical component identifier: lowercase ASCII, starts with a
/// letter or digit, then letters/digits/hyphens; never a path.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ComponentId(String);

impl ComponentId {
    /// Parse the closed grammar; path-like or private shapes are refused as
    /// private data, other violations as invalid schema.
    pub fn parse(text: &str) -> Result<Self, LockFailure> {
        if is_private_data(text) {
            return Err(LockFailure::PrivateData { field: "id" });
        }
        grammar_checked(text).map(Self)
    }

    /// The exact wire spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The shared closed body grammar of lowercase logical identifiers.
fn grammar_checked(text: &str) -> Result<String, LockFailure> {
    let valid = !text.is_empty()
        && text.len() <= 128
        && text.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        return Err(LockFailure::SchemaInvalid);
    }
    Ok(text.to_owned())
}

/// A capability identifier: lowercase dotted segments (`planner.render`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CapabilityId(String);

impl CapabilityId {
    /// Parse the closed dotted grammar; traversal and private shapes fail.
    pub fn parse(text: &str) -> Result<Self, LockFailure> {
        if is_private_data(text) {
            return Err(LockFailure::PrivateData { field: "id" });
        }
        let segments: Vec<&str> = text.split('.').collect();
        let valid = !text.is_empty()
            && text.len() <= 128
            && segments.iter().all(|segment| {
                !segment.is_empty()
                    && segment.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || byte == b'-'
                            || byte == b'_'
                    })
            });
        if !valid {
            return Err(LockFailure::SchemaInvalid);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact wire spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A provider target platform: `any` or one stable lowercase target triple.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Platform(String);

impl Platform {
    /// Parse the closed platform grammar.
    pub fn parse(text: &str) -> Result<Self, LockFailure> {
        if is_private_data(text) {
            return Err(LockFailure::PrivateData { field: "platform" });
        }
        if text == "any" {
            return Ok(Self(text.to_owned()));
        }
        let valid = !text.is_empty()
            && text.len() <= 64
            && text.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
            })
            && text.split('-').all(|segment| !segment.is_empty())
            && text.split('-').count() >= 2;
        if !valid {
            return Err(LockFailure::SchemaInvalid);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact wire spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The declared origin kind of an adapter package.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SourceKind {
    /// Compiled into the Lekalo implementation.
    Builtin,
    /// Frozen in an immutable catalog snapshot.
    Catalog,
    /// Located inside the validated project tree.
    Project,
}

impl SourceKind {
    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Result<Self, LockFailure> {
        match text {
            "builtin" => Ok(Self::Builtin),
            "catalog" => Ok(Self::Catalog),
            "project" => Ok(Self::Project),
            _ => Err(LockFailure::SchemaInvalid),
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Catalog => "catalog",
            Self::Project => "project",
        }
    }
}

/// The source coordinate of an adapter package.
///
/// `Project` ids must be `#4`-safe project-relative POSIX paths (validated
/// with the accepted structure grammar); `Builtin`/`Catalog` ids are stable
/// logical coordinates. Neither ever carries an install location.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SourceRef {
    kind: SourceKind,
    id: String,
    digest: Sha256Digest,
}

impl SourceRef {
    /// Construct and validate one source reference.
    pub fn new(kind: SourceKind, id: &str, digest: Sha256Digest) -> Result<Self, LockFailure> {
        if is_private_data(id) {
            return Err(LockFailure::PrivateData { field: "source.id" });
        }
        match kind {
            SourceKind::Project => {
                if crate::project_fs::path_violation(id).is_some() {
                    return Err(LockFailure::PrivateData { field: "source.id" });
                }
            }
            SourceKind::Builtin | SourceKind::Catalog => {
                grammar_checked(id).map_err(|_| LockFailure::SchemaInvalid)?;
            }
        }
        Ok(Self {
            kind,
            id: id.to_owned(),
            digest,
        })
    }

    /// The declared kind.
    pub const fn kind(&self) -> SourceKind {
        self.kind
    }

    /// The stable logical coordinate.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The digest of the exact source snapshot bytes.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// An exact component version reference (`{id, version}`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ComponentRef {
    id: ComponentId,
    version: SemVer,
}

impl ComponentRef {
    /// Construct one reference from typed parts.
    pub fn new(id: ComponentId, version: SemVer) -> Self {
        Self { id, version }
    }

    /// The component identifier.
    pub fn id(&self) -> &ComponentId {
        &self.id
    }

    /// The exact version.
    pub fn version(&self) -> &SemVer {
        &self.version
    }
}

/// One pinned contract: exact version plus the digest over that contract's
/// declared byte domain (documented in `docs/lockfile.md`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractPin {
    version: SemVer,
    digest: Sha256Digest,
}

impl ContractPin {
    /// Construct one pin from typed parts.
    pub fn new(version: SemVer, digest: Sha256Digest) -> Self {
        Self { version, digest }
    }

    /// The exact contract version.
    pub fn version(&self) -> &SemVer {
        &self.version
    }

    /// The contract-domain digest.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// One immutable candidate-catalog snapshot identity.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CatalogRef {
    id: ComponentId,
    digest: Sha256Digest,
}

impl CatalogRef {
    /// Construct one catalog reference from typed parts.
    pub fn new(id: ComponentId, digest: Sha256Digest) -> Self {
        Self { id, digest }
    }

    /// The catalog identifier.
    pub fn id(&self) -> &ComponentId {
        &self.id
    }

    /// The catalog snapshot digest.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// One platform artifact of a selected adapter.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ArtifactPin {
    platform: Platform,
    digest: Sha256Digest,
}

impl ArtifactPin {
    /// Construct one artifact pin from typed parts.
    pub fn new(platform: Platform, digest: Sha256Digest) -> Self {
        Self { platform, digest }
    }

    /// The target platform.
    pub fn platform(&self) -> &Platform {
        &self.platform
    }

    /// The artifact byte digest.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// One selected adapter with its immutable package identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAdapter {
    id: ComponentId,
    version: SemVer,
    digest: Sha256Digest,
    source: SourceRef,
    compatibility_digest: Sha256Digest,
    artifacts: Vec<ArtifactPin>,
}

impl ResolvedAdapter {
    pub(crate) fn from_parts(
        id: ComponentId,
        version: SemVer,
        digest: Sha256Digest,
        source: SourceRef,
        compatibility_digest: Sha256Digest,
        mut artifacts: Vec<ArtifactPin>,
    ) -> Result<Self, LockFailure> {
        if artifacts.is_empty() {
            return Err(LockFailure::ReferenceInvalid);
        }
        artifacts.sort();
        if artifacts.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(LockFailure::ReferenceInvalid);
        }
        Ok(Self {
            id,
            version,
            digest,
            source,
            compatibility_digest,
            artifacts,
        })
    }

    /// The adapter identifier.
    pub fn id(&self) -> &ComponentId {
        &self.id
    }

    /// The exact adapter version.
    pub fn version(&self) -> &SemVer {
        &self.version
    }

    /// The package-root digest.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }

    /// The declared source.
    pub fn source(&self) -> &SourceRef {
        &self.source
    }

    /// The compatibility-manifest digest.
    pub fn compatibility_digest(&self) -> &Sha256Digest {
        &self.compatibility_digest
    }

    /// The sorted platform artifacts.
    pub fn artifacts(&self) -> &[ArtifactPin] {
        &self.artifacts
    }
}

/// One selected generator: an independently versioned logical
/// implementation inside an adapter package (owner decision #4).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedGenerator {
    id: ComponentId,
    version: SemVer,
    digest: Sha256Digest,
    adapter: ComponentRef,
}

impl ResolvedGenerator {
    pub(crate) fn from_parts(
        id: ComponentId,
        version: SemVer,
        digest: Sha256Digest,
        adapter: ComponentRef,
    ) -> Self {
        Self {
            id,
            version,
            digest,
            adapter,
        }
    }

    /// The generator identifier.
    pub fn id(&self) -> &ComponentId {
        &self.id
    }

    /// The exact generator version.
    pub fn version(&self) -> &SemVer {
        &self.version
    }

    /// The implementation-descriptor digest.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }

    /// The owning adapter reference.
    pub fn adapter(&self) -> &ComponentRef {
        &self.adapter
    }
}

/// One selected profile: exact version plus declared and resolved digests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedProfile {
    id: ComponentId,
    version: SemVer,
    source_digest: Sha256Digest,
    digest: Sha256Digest,
    adapters: Vec<ComponentRef>,
    generators: Vec<ComponentRef>,
}

impl ResolvedProfile {
    pub(crate) fn from_parts(
        id: ComponentId,
        version: SemVer,
        source_digest: Sha256Digest,
        digest: Sha256Digest,
        mut adapters: Vec<ComponentRef>,
        mut generators: Vec<ComponentRef>,
    ) -> Result<Self, LockFailure> {
        // Owner decision #5: profiles without an exact version fail closed;
        // a fabricated 0.0.0 never exists on the wire.
        if version.as_str() == "0.0.0" {
            return Err(LockFailure::ProfileUnversioned {
                id: id.as_str().to_owned(),
            });
        }
        let order = |left: &ComponentRef, right: &ComponentRef| {
            left.id()
                .as_str()
                .as_bytes()
                .cmp(right.id().as_str().as_bytes())
                .then_with(|| {
                    left.version()
                        .as_str()
                        .as_bytes()
                        .cmp(right.version().as_str().as_bytes())
                })
        };
        adapters.sort_by(order);
        generators.sort_by(order);
        if adapters
            .windows(2)
            .any(|pair| order(&pair[0], &pair[1]) != Ordering::Less)
            || generators
                .windows(2)
                .any(|pair| order(&pair[0], &pair[1]) != Ordering::Less)
        {
            return Err(LockFailure::ReferenceInvalid);
        }
        Ok(Self {
            id,
            version,
            source_digest,
            digest,
            adapters,
            generators,
        })
    }

    /// The profile identifier.
    pub fn id(&self) -> &ComponentId {
        &self.id
    }

    /// The exact profile version.
    pub fn version(&self) -> &SemVer {
        &self.version
    }

    /// The declared-input digest.
    pub fn source_digest(&self) -> &Sha256Digest {
        &self.source_digest
    }

    /// The resolved-snapshot digest.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }

    /// Sorted selected adapter references.
    pub fn adapters(&self) -> &[ComponentRef] {
        &self.adapters
    }

    /// Sorted selected generator references.
    pub fn generators(&self) -> &[ComponentRef] {
        &self.generators
    }
}

/// The declared support level of one capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Support {
    /// Satisfies every request.
    Full,
    /// Satisfies only requests carrying the explicit accepted policy.
    Partial,
    /// Never satisfies a required capability.
    Unsupported,
    /// Never satisfies a required capability.
    Unknown,
}

impl Support {
    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Result<Self, LockFailure> {
        match text {
            "full" => Ok(Self::Full),
            "partial" => Ok(Self::Partial),
            "unsupported" => Ok(Self::Unsupported),
            "unknown" => Ok(Self::Unknown),
            _ => Err(LockFailure::SchemaInvalid),
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}

/// Which component kind provides a capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProviderKind {
    /// An adapter provides the capability.
    Adapter,
    /// A generator provides the capability.
    Generator,
}

impl ProviderKind {
    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Result<Self, LockFailure> {
        match text {
            "adapter" => Ok(Self::Adapter),
            "generator" => Ok(Self::Generator),
            _ => Err(LockFailure::SchemaInvalid),
        }
    }

    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Adapter => "adapter",
            Self::Generator => "generator",
        }
    }
}

/// The provider that supplies one capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRef {
    kind: ProviderKind,
    id: ComponentId,
    version: SemVer,
}

impl ProviderRef {
    /// Construct one provider reference from typed parts.
    pub fn new(kind: ProviderKind, id: ComponentId, version: SemVer) -> Self {
        Self { kind, id, version }
    }

    /// The provider kind.
    pub const fn kind(&self) -> ProviderKind {
        self.kind
    }

    /// The provider identifier.
    pub fn id(&self) -> &ComponentId {
        &self.id
    }

    /// The provider version.
    pub fn version(&self) -> &SemVer {
        &self.version
    }
}

/// One resolved capability snapshot entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCapability {
    target: Platform,
    profile: ComponentId,
    id: CapabilityId,
    version: SemVer,
    support: Support,
    provider: ProviderRef,
}

impl ResolvedCapability {
    pub(crate) fn from_parts(
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

    /// The target platform.
    pub fn target(&self) -> &Platform {
        &self.target
    }

    /// The profile identifier.
    pub fn profile(&self) -> &ComponentId {
        &self.profile
    }

    /// The capability identifier.
    pub fn id(&self) -> &CapabilityId {
        &self.id
    }

    /// The capability version.
    pub fn version(&self) -> &SemVer {
        &self.version
    }

    /// The declared support level.
    pub const fn support(&self) -> Support {
        self.support
    }

    /// The providing component.
    pub fn provider(&self) -> &ProviderRef {
        &self.provider
    }
}

/// The whole parsed, validated `lekalo.lock` value.
///
/// Construction goes through [`super::Lockfile::parse_canonical`] or the
/// resolver; both inputs are fully validated and canonical before a value
/// exists, so every lock is byte-stable under
/// [`super::Lockfile::canonical_bytes`].
#[derive(Clone, Debug)]
pub struct Lockfile {
    resolver_version: SemVer,
    request_digest: Sha256Digest,
    catalogs: Vec<CatalogRef>,
    core_version: SemVer,
    registry: ContractPin,
    model: ContractPin,
    ir: ContractPin,
    target_protocol: Option<ContractPin>,
    adapters: Vec<ResolvedAdapter>,
    generators: Vec<ResolvedGenerator>,
    profiles: Vec<ResolvedProfile>,
    capabilities: Vec<ResolvedCapability>,
}

impl Lockfile {
    /// Construct one lock from fully validated typed parts. Sorted arrays
    /// are normalized here; reference and cross-field validation belongs to
    /// the parser and resolver, which are the only callers.
    #[allow(clippy::too_many_arguments)] // the closed v1 wire has eight top-level sections
    pub(crate) fn from_parts(
        resolver_version: SemVer,
        request_digest: Sha256Digest,
        mut catalogs: Vec<CatalogRef>,
        core_version: SemVer,
        registry: ContractPin,
        model: ContractPin,
        ir: ContractPin,
        target_protocol: Option<ContractPin>,
        mut adapters: Vec<ResolvedAdapter>,
        mut generators: Vec<ResolvedGenerator>,
        mut profiles: Vec<ResolvedProfile>,
        mut capabilities: Vec<ResolvedCapability>,
    ) -> Self {
        let by_id = |left: &dyn IdOf, right: &dyn IdOf| {
            left.id_bytes()
                .cmp(right.id_bytes())
                .then_with(|| left.version_bytes().cmp(right.version_bytes()))
        };
        catalogs.sort();
        adapters.sort_by(|left, right| by_id(left, right));
        generators.sort_by(|left, right| by_id(left, right));
        profiles.sort_by(|left, right| by_id(left, right));
        capabilities.sort_by(|left, right| {
            (
                left.target().as_str(),
                left.profile().as_str(),
                left.id().as_str(),
                left.version().as_str(),
            )
                .cmp(&(
                    right.target().as_str(),
                    right.profile().as_str(),
                    right.id().as_str(),
                    right.version().as_str(),
                ))
        });
        Self {
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
        }
    }

    /// The resolver algorithm version.
    pub fn resolver_version(&self) -> &SemVer {
        &self.resolver_version
    }

    /// The digest of the typed resolution request this lock froze.
    pub fn request_digest(&self) -> &Sha256Digest {
        &self.request_digest
    }

    /// Sorted catalog snapshot identities.
    pub fn catalogs(&self) -> &[CatalogRef] {
        &self.catalogs
    }

    /// The pinned product/CLI version.
    pub fn core_version(&self) -> &SemVer {
        &self.core_version
    }

    /// The pinned version-registry contract.
    pub fn registry_pin(&self) -> &ContractPin {
        &self.registry
    }

    /// The pinned Model contract.
    pub fn model_pin(&self) -> &ContractPin {
        &self.model
    }

    /// The pinned IR contract.
    pub fn ir_pin(&self) -> &ContractPin {
        &self.ir
    }

    /// The pinned target protocol, `None` while unpublished.
    pub fn target_protocol(&self) -> Option<&ContractPin> {
        self.target_protocol.as_ref()
    }

    /// Sorted selected adapters.
    pub fn adapters(&self) -> &[ResolvedAdapter] {
        &self.adapters
    }

    /// Sorted selected generators.
    pub fn generators(&self) -> &[ResolvedGenerator] {
        &self.generators
    }

    /// Sorted selected profiles.
    pub fn profiles(&self) -> &[ResolvedProfile] {
        &self.profiles
    }

    /// Sorted resolved capability snapshot.
    pub fn capabilities(&self) -> &[ResolvedCapability] {
        &self.capabilities
    }
}

trait IdOf {
    fn id_bytes(&self) -> &[u8];
    fn version_bytes(&self) -> &[u8];
}

macro_rules! impl_id_of {
    ($($kind:ty),*) => {$(
        impl IdOf for $kind {
            fn id_bytes(&self) -> &[u8] {
                self.id().as_str().as_bytes()
            }
            fn version_bytes(&self) -> &[u8] {
                self.version().as_str().as_bytes()
            }
        }
    )*};
}

impl_id_of!(
    ResolvedAdapter,
    ResolvedGenerator,
    ResolvedProfile,
    ResolvedCapability
);
